use async_trait::async_trait;

use crate::domain::models::extraccion_factura_model::ExtraccionFactura;

#[async_trait]
pub trait IExternalService {
    /// Avisa que el documento quedó procesado, con lo que se pudo extraer.
    ///
    /// `extraccion` va como `Option` a propósito: un respaldo puede ser una foto
    /// sin timbre ni capa de texto, y en ese caso no hay nada que declarar. Que
    /// el campo pueda estar vacío es parte del contrato, no un caso de borde —
    /// quien lo reciba tiene que poder mostrar "no se pudo leer" en vez de
    /// inventar ceros.
    ///
    /// El `correlation_id` es el que permite que el formulario que está abierto
    /// en el navegador sepa que ESTA respuesta es la suya.
    async fn notify_object_processed(
        &self,
        extraccion: Option<ExtraccionFactura>,
        category: &str,
        status: &str,
        correlation_id: &str,
        owner_uuid: &str,
        gestor: &str,
        asset_id: &str,
        resource_id: &str,
        resource_type: &str,
    ) -> String;
}
