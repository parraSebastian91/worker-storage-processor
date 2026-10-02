use std::{collections::HashMap, sync::Arc, time::Instant};

use crate::{
    aplication::service::{
        document_manager_service::DocumentManagerService,
        image_manager_service::ImageManagerService,
    },
    domain::{
        errors::handler_error::HandlerError,
        models::{
            CATEGORY_PROCESS_DOCUMENT_DTO, constantes_model::{CATEGORY_PROCESS_DOCUMENT_DTO_RESPALDO, CATEGORY_PROCESS_USER_AVATAR, CATEGORY_PROCESS_USER_BANNER}, factura_data_model::InvoiceData, message_event_model::{
                PublishPayload, Recipe, RecipeMediaModel, VariantMetadataModel, VariantModel,
            }
        },
        ports::outbound::{
            object_db_repository::IObjectDBRepository,
            object_storage_repository::IObjectStorageRepository,
        },
    },
};
use crate::domain::models::extraccion_factura_model::ExtraccionFactura;
#[cfg(all(feature = "pdf", feature = "timbre"))]
use crate::aplication::service::{
    extraccion_factura_service::ExtraccionFacturaService,
    timbre_manager_service::TimbreManagerService,
};
use regex::Regex;
use tracing::{debug, error, info, warn};
pub struct EventManagerService {
    object_storage: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
    object_repository: Arc<dyn IObjectDBRepository>,
    image_process_service: Arc<ImageManagerService>,
    document_manager_service: Arc<DocumentManagerService>,
    // object_cache_repository: Arc<dyn IObjectCacheRepository>,
}

fn replace_extension(storage_key: &str, new_ext: &str) -> String {
    let normalized_ext = new_ext.trim().trim_start_matches('.');
    if normalized_ext.is_empty() {
        return storage_key.to_string();
    }

    if let Some((base, _)) = storage_key.rsplit_once('.') {
        format!("{}.{}", base, normalized_ext)
    } else {
        format!("{}.{}", storage_key, normalized_ext)
    }
}

impl EventManagerService {
    /// Capas 1 y 2 de la extracción: timbre firmado y capa de texto del PDF.
    ///
    /// Devuelve `None` cuando el worker se compiló sin `pdf`+`timbre`, o cuando
    /// el PDF no dio nada por ninguna de las dos vías. Ese `None` no es un
    /// error: un respaldo puede ser legítimamente una guía de despacho escaneada
    /// sin timbre ni capa de texto, y para eso está el OCR.
    #[cfg(all(feature = "pdf", feature = "timbre"))]
    fn extraer_en_capas(
        &self,
        pdf_bytes: &[u8],
        correlation_id: &str,
    ) -> Option<ExtraccionFactura> {
        let t0 = Instant::now();
        let e = ExtraccionFacturaService::new().extraer_de_pdf(
            pdf_bytes,
            &self.document_manager_service,
            &TimbreManagerService::new(),
        );
        let ms = t0.elapsed().as_millis();

        if e == ExtraccionFactura::default() {
            info!(
                correlation_id = %correlation_id,
                ms,
                "Ni timbre ni capa de texto: el documento queda a cargo del OCR"
            );
            return None;
        }

        // La discrepancia se loguea como warn a propósito. No es ruido de
        // lectura: el timbre está firmado y la capa de texto no, así que
        // superponerle texto a la imagen de una factura real es trivial y se
        // midió que engaña a un extractor que sólo lea el texto.
        for d in &e.discrepancias {
            warn!(
                correlation_id = %correlation_id,
                campo = %d.campo,
                segun_timbre = %d.segun_timbre,
                segun_texto = %d.segun_texto,
                "El texto del PDF contradice al timbre firmado"
            );
        }

        info!(
            correlation_id = %correlation_id,
            ms,
            origen = ?e.origen_dominante(),
            confiable = e.es_confiable(),
            folio = ?e.folio.as_ref().map(|c| &c.valor),
            "Extracción en capas lista"
        );
        Some(e)
    }

    #[cfg(not(all(feature = "pdf", feature = "timbre")))]
    fn extraer_en_capas(&self, _pdf: &[u8], _correlation_id: &str) -> Option<ExtraccionFactura> {
        None
    }

    pub fn new(
        object_storage: HashMap<String, Arc<dyn IObjectStorageRepository + Send + Sync>>,
        object_repository: Arc<dyn IObjectDBRepository>,
        // object_cache_repository: Arc<dyn IObjectCacheRepository>,
        image_process_service: Arc<ImageManagerService>,
        document_manager_service: Arc<DocumentManagerService>,
    ) -> Self {
        Self {
            object_storage,
            object_repository,
            // object_cache_repository,
            image_process_service,
            document_manager_service,
        }
    }

    pub async fn handle_image_process(
        &self,
        _payload: PublishPayload,
        _private: bool,
    ) -> Result<(), HandlerError> {
        let started_at = Instant::now();
        let correlation_id = _payload.correlation_id.as_deref().unwrap_or("n/a");

        // Extraer variante Image del enum
        let recipe = match &_payload.recipe {
            Some(Recipe::Image(r)) => r,
            _ => {
                error!(
                    correlation_id = %correlation_id,
                    asset_id = %_payload.event.asset_id,
                    "Recipe inválida o ausente para procesamiento de imagen"
                );
                return Err(HandlerError::ProcessingError(
                    "Se esperaba una receta de imagen".to_string(),
                ));
            }
        };

        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            media_type = %_payload.event.media_type,
            storage_key = %_payload.event.storage_key,
            recipe_name = %recipe.name,
            "Inicio de procesamiento de imagen"
        );

        if _payload.event.category_process != CATEGORY_PROCESS_USER_AVATAR
            || _payload.event.category_process != CATEGORY_PROCESS_USER_BANNER
        {
            info!(
                correlation_id = %correlation_id,
                asset_id = %_payload.event.asset_id,
                media_type = %_payload.event.media_type,
                category_process = %_payload.event.category_process,
                "Se deprecan assets antiguos del usuario para esta categoria de proceso"
            );
            self.object_repository
                .deprecate_old_assets(&_payload.event.owner_uuid, &_payload.event.category_process)
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
        }

        let download_started_at = Instant::now();
        let object = self
            .download_object_temp("", &_payload.event.storage_key, _private)
            .await;
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            object_bytes = object.len(),
            elapsed_ms = download_started_at.elapsed().as_millis(),
            "Descarga de objeto temporal finalizada"
        );

        let process_started_at = Instant::now();
        let process_result = self
            .image_process_service
            .process(&object, recipe)
            .map_err(|e| HandlerError::ProcessingError(e.to_string()))?;
        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            variants_generated = process_result.len(),
            elapsed_ms = process_started_at.elapsed().as_millis(),
            "Transformacion de imagen finalizada"
        );

        for media in process_result {
            info!(
                correlation_id = %correlation_id,
                asset_id = %_payload.event.asset_id,
                format = %media.format,
                size = %media.size,
                width = media.width,
                height = media.height,
                bytes = media.bytes.len(),
                "Variante procesada"
            );
            let safe_name = _payload.event.name_file.replace(' ', "_");
            let key_object = format!(
                "profile-pictures/{}/{}/{}-{}.{}",
                _payload.event.owner_uuid,
                _payload.event.category_process,
                safe_name,
                media.size,
                media.format
            );

            self.upload_object_final("", &key_object as &str, media.bytes.clone(), false)
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
            info!(
                correlation_id = %correlation_id,
                asset_id = %_payload.event.asset_id,
                object_key = %key_object,
                "Variante subida a storage"
            );

            let metadata = VariantMetadataModel {
                format: media.format.clone(),
                size: media.size,
                width: media.width,
                height: media.height,
                headers: "Cache-Control: public, max-age=31536000".to_string(),
                data_obtenida: chrono::Utc::now().to_rfc3339(),
                extraccion: None,
            };

            let media_variant = VariantModel {
                asset_id: _payload.event.asset_id.clone(),
                name: _payload.event.name_file.clone(),
                metadata: metadata,
                url_path: key_object.clone(),
            };

            self.object_repository
                .create_variant(media_variant.into())
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
            info!(
                correlation_id = %correlation_id,
                asset_id = %_payload.event.asset_id,
                object_key = %key_object,
                "Metadata de variante persistida"
            );
        }

        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            elapsed_ms = started_at.elapsed().as_millis(),
            "Procesamiento de imagen completado"
        );

        Ok(())
    }

    pub async fn handle_video_process(
        &self,
        _payload: PublishPayload,
        _private: bool,
    ) -> Result<(), HandlerError> {
        info!("Manejando mensaje de video con EventManagerService...");
        info!("Payload recibido: {:?}", _payload);
        Ok(())
    }

    pub async fn handle_document_dte_process(
        &self,
        _payload: PublishPayload,
        private: bool,
    ) -> Result<InvoiceData, HandlerError> {
        let started_at = Instant::now();
        let correlation_id = _payload.correlation_id.as_deref().unwrap_or("n/a");

        // Extraer variante Document del enum
        let recipe = match &_payload.recipe {
            Some(Recipe::Document(r)) => r,
            _ => {
                error!(
                    correlation_id = %correlation_id,
                    asset_id = %_payload.event.asset_id,
                    "Recipe inválida o ausente para procesamiento de documento"
                );
                return Err(HandlerError::ProcessingError(
                    "Se esperaba una receta de documento".to_string(),
                ));
            }
        };

        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            storage_key = %_payload.event.storage_key,
            "Inicio de procesamiento de documento"
        );

        let document_bytes = self
            .download_object_temp("", &_payload.event.storage_key, private)
            .await;
        if document_bytes.is_empty() {
            return Err(HandlerError::RepositoryError(
                "No fue posible descargar el documento temporal para OCR".to_string(),
            ));
        }

        // Renderizar primera página del PDF para compresión/variants y OCR.
        let rendered_image_bytes = self
            .document_manager_service
            .render_first_page_png_from_pdf(&document_bytes)
            .map_err(|e| HandlerError::ProcessingError(e.to_string()))?;

        let media_recipe = RecipeMediaModel {
            name: format!("{}-document-render", recipe.name),
            target_size: recipe.target_size.clone(),
            format: "webp".to_string(),
            radio: 1.0,
            priority: 0,
        };

        let mut processed_variants = if media_recipe.target_size.is_empty() {
            vec![]
        } else {
            self.image_process_service
                .process(&rendered_image_bytes, &media_recipe)
                .map_err(|e| HandlerError::ProcessingError(e.to_string()))?
        };

        let main_image_key = replace_extension(&_payload.event.storage_key, "webp");
        let mut saved_storage_key = main_image_key.clone();
        if let Some(first_variant) = processed_variants.first() {
            self.upload_object_final("", &main_image_key, first_variant.bytes.clone(), private)
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
        } else {
            let fallback_image_key = replace_extension(&_payload.event.storage_key, "png");
            self.upload_object_final(
                "",
                &fallback_image_key,
                rendered_image_bytes.clone(),
                private,
            )
            .await
            .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
            saved_storage_key = fallback_image_key;
        }

        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            source_storage_key = %_payload.event.storage_key,
            target_storage_key = %saved_storage_key,
            "Documento convertido y guardado en formato de imagen"
        );

        let language = if recipe.ocr_language.is_empty() {
            std::env::var("OCR_TESSERACT_LANG").unwrap_or_else(|_| "spa+eng".to_string())
        } else {
            recipe.ocr_language.clone()
        };

        // Extraer datos estructurados de la factura
        // let invoice_data = self
        //     .document_manager_service
        //     .extract_invoice_data_from_image_bytes(&rendered_image_bytes, &language)
        //     .map_err(|e| HandlerError::ProcessingError(e.to_string()))?;

        let es_dte = matches!(
            _payload.event.category_process.as_str(),
            CATEGORY_PROCESS_DOCUMENT_DTO | CATEGORY_PROCESS_DOCUMENT_DTO_RESPALDO
        );

        // Capas 1 y 2: el timbre firmado y la capa de texto del PDF, con la
        // procedencia de cada campo. Las dos leen el PDF original —no la imagen
        // renderizada— y juntas cuestan decenas de milisegundos.
        //
        // Importa que corran ANTES del OCR y no después: el OCR tarda segundos,
        // y para una factura electrónica generada por software no aporta nada
        // que no esté ya en el timbre o en la capa de texto. Ver
        // `docs/general/findings/cotizadores-externos/`.
        let extraccion = if es_dte { self.extraer_en_capas(&document_bytes, correlation_id) } else { None };

        // Capa 3: el OCR. Se sigue corriendo —es lo que alimenta el gate de
        // publicación hoy— pero ya no es la única fuente, y cuando el timbre
        // verificó, sus candidatos pasan a ser lo que son: candidatos.
        let invoice_data = if es_dte {
            self.document_manager_service
                .extract_invoice_data_from_image_bytes(&rendered_image_bytes, &language)
                .map_err(|e| HandlerError::ProcessingError(e.to_string()))?
        } else {
            InvoiceData {
                numero_factura: vec![],
                rut_deudor: vec![],
                nombre_deudor: vec![],
                monto_total: vec![],
                full_text: vec![],
            }
        };

        for media in processed_variants.drain(..) {
            let safe_name = _payload.event.name_file.replace(' ', "_");
            let key_object = format!(
                "public/documents/{}/{}/{}-{}.{}",
                _payload.event.owner_uuid,
                _payload.event.category_process,
                safe_name,
                media.size,
                media.format
            );

            self.upload_object_final("", &key_object, media.bytes.clone(), private)
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

            let metadata = VariantMetadataModel {
                format: media.format.clone(),
                size: media.size.clone(),
                width: media.width,
                height: media.height,
                headers: "Cache-Control: public, max-age=31536000".to_string(),
                data_obtenida: serde_json::to_string(&invoice_data).map_err(|e| {
                    HandlerError::ProcessingError(format!(
                        "No se pudo serializar invoice_data: {}",
                        e
                    ))
                })?,
                extraccion: extraccion.clone(),
            };

            let media_variant = VariantModel {
                asset_id: _payload.event.asset_id.clone(),
                name: format!("{}-{}", _payload.event.name_file, media.size),
                metadata,
                url_path: key_object.clone(),
            };

            self.object_repository
                .create_variant(media_variant.into())
                .await
                .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;
        }

        debug!(
            correlation_id = %correlation_id,
            numero_factura = ?invoice_data.numero_factura,
            rut_deudor = ?invoice_data.rut_deudor,
            nombre_deudor = ?invoice_data.nombre_deudor,
            monto_total = ?invoice_data.monto_total,
            "Datos de factura extraídos exitosamente"
        );

        // Guardar texto completo de OCR
        // let text_key = format!(
        //     "public/documents/{}/{}/{}-ocr.txt",
        //     _payload.event.owner_uuid, _payload.event.category_process, _payload.event.name_file
        // );

        // let full_text_bytes = invoice_data.full_text.join("\n").into_bytes();
        // self.upload_object_final("", &text_key, full_text_bytes, private)
        //     .await
        //     .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

        // let metadata = VariantMetadataModel {
        //     format: "txt".to_string(),
        //     size: "ocr".to_string(),
        //     width: 0,
        //     height: 0,
        //     headers: "Content-Type: text/plain; charset=utf-8".to_string(),
        // };

        // let media_variant = VariantModel {
        //     asset_id: _payload.event.asset_id.clone(),
        //     name: format!("{}-ocr", _payload.event.name_file),
        //     metadata,
        //     url_path: text_key.clone(),
        // };

        // self.object_repository
        //     .create_variant(media_variant.into())
        //     .await
        //     .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

        // Guardar datos extraídos en JSON
        // let invoice_json = serde_json::json!({
        //     "numero_factura": &invoice_data.numero_factura,
        //     "rut_deudor": &invoice_data.rut_deudor,
        //     "nombre_deudor": &invoice_data.nombre_deudor,
        //     "monto_total": &invoice_data.monto_total,
        //     "extraction_timestamp": chrono::Utc::now().to_rfc3339(),
        //     "total_lines_processed": invoice_data.full_text.len(),
        // });

        // let json_key = format!(
        //     "public/documents/{}/{}/{}-ocr.json",
        //     _payload.event.owner_uuid, _payload.event.category_process, _payload.event.name_file
        // );

        // self.upload_object_final("", &json_key, invoice_json.to_string().into_bytes(), private)
        //     .await
        //     .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

        // let json_metadata = VariantMetadataModel {
        //     format: "json".to_string(),
        //     size: "ocr-data".to_string(),
        //     width: 0,
        //     height: 0,
        //     headers: "Content-Type: application/json; charset=utf-8".to_string(),
        // };

        // let json_variant = VariantModel {
        //     asset_id: _payload.event.asset_id.clone(),
        //     name: format!("{}-ocr-data", _payload.event.name_file),
        //     metadata: json_metadata,
        //     url_path: json_key.clone(),
        // };

        // self.object_repository
        //     .create_variant(json_variant.into())
        //     .await
        //     .map_err(|e| HandlerError::RepositoryError(e.to_string()))?;

        info!(
            correlation_id = %correlation_id,
            asset_id = %_payload.event.asset_id,
            // text_key = %text_key,
            elapsed_ms = started_at.elapsed().as_millis(),
            "Procesamiento OCR de documento completado"
        );

        Ok(invoice_data)
    }

    pub async fn handle_other_process(&self, _payload: PublishPayload) -> Result<(), HandlerError> {
        info!("Manejando mensaje de otro tipo con EventManagerService...");
        info!("Payload recibido: {:?}", _payload);
        Ok(())
    }

    async fn download_object_temp(&self, _bucket: &str, _key: &str, private: bool) -> Vec<u8> {
        let total_providers = self.object_storage.len();
        for (index, service_storage) in self.object_storage.values().enumerate() {
            info!(
                provider_attempt = index + 1,
                total_providers,
                bucket = _bucket,
                object_key = _key,
                "Intentando descargar objeto temporal"
            );
            match service_storage.download_file(_bucket, _key, private).await {
                Ok(data) => {
                    info!(
                        bucket = _bucket,
                        object_key = _key,
                        bytes = data.len(),
                        provider_attempt = index + 1,
                        "Archivo descargado exitosamente"
                    );
                    // Aquí podrías procesar los datos descargados según tus necesidades
                    return data;
                }
                Err(e) => {
                    warn!(
                        bucket = _bucket,
                        object_key = _key,
                        provider_attempt = index + 1,
                        error = %e,
                        "Fallo la descarga en proveedor actual"
                    );
                    continue;
                }
            }
        }
        error!(
            bucket = _bucket,
            object_key = _key,
            "No se pudo descargar el objeto temporal en ningun proveedor"
        );
        vec![]
    }

    async fn upload_object_final(
        &self,
        _bucket: &str,
        _key: &str,
        _data: Vec<u8>,
        private: bool,
    ) -> Result<(), HandlerError> {
        let spaces_regex = Regex::new(r"\s+")
            .map_err(|e| HandlerError::RepositoryError(format!("Regex inválida: {}", e)))?;
        let key_no_spaces = spaces_regex.replace_all(_key.trim(), "_");
        let total_providers = self.object_storage.len();
        for (index, service_storage) in self.object_storage.values().enumerate() {
            info!(
                provider_attempt = index + 1,
                total_providers,
                bucket = _bucket,
                object_key = %key_no_spaces,
                bytes = _data.len(),
                "Intentando subir variante final"
            );
            match service_storage
                .upload_file(_bucket, &key_no_spaces, _data.clone(), private)
                .await
            {
                Ok(_) => {
                    info!(
                        bucket = _bucket,
                        object_key = %key_no_spaces,
                        provider_attempt = index + 1,
                        "Archivo subido exitosamente"
                    );
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        bucket = _bucket,
                        object_key = %key_no_spaces,
                        provider_attempt = index + 1,
                        error = %e,
                        "Fallo la subida en proveedor actual"
                    );
                    continue;
                }
            }
        }
        error!(
            bucket = _bucket,
            object_key = %key_no_spaces,
            "No se pudo subir el archivo a ningun proveedor"
        );
        Err(HandlerError::RepositoryError(
            "No se pudo subir el archivo a ningún servicio de almacenamiento".to_string(),
        ))
    }

    pub async fn delete_object_temp(&self, _bucket: &str, _key: &str) -> Result<(), HandlerError> {
        let total_providers = self.object_storage.len();
        for (index, service_storage) in self.object_storage.values().enumerate() {
            info!(
                provider_attempt = index + 1,
                total_providers,
                bucket = _bucket,
                object_key = _key,
                "Intentando eliminar objeto temporal"
            );
            match service_storage.delete_file(_bucket, _key, true).await {
                Ok(_) => {
                    info!(
                        bucket = _bucket,
                        object_key = _key,
                        provider_attempt = index + 1,
                        "Archivo eliminado exitosamente"
                    );
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        bucket = _bucket,
                        object_key = _key,
                        provider_attempt = index + 1,
                        error = %e,
                        "Fallo la eliminacion en proveedor actual"
                    );
                    continue;
                }
            }
        }
        error!(
            bucket = _bucket,
            object_key = _key,
            "No se pudo eliminar el archivo temporal en ningun proveedor"
        );
        Err(HandlerError::RepositoryError(
            "No se pudo eliminar el archivo de ningún servicio de almacenamiento".to_string(),
        ))
    }
}
