//use image::{DynamicImage, GenericImageView};
use fast_image_resize as fr;
use webp::Encoder;
use std::num::NonZeroU32;

use crate::domain::{errors::media_error::MediaError, models::message_event_model::{MediaSizeModel, RecipeMediaModel}};
pub struct ImageManagerService {}

impl ImageManagerService {
    pub fn new() -> Self {
        Self {}
    }

    pub fn process(&self, bytes: &[u8], recipe: &RecipeMediaModel) -> Result<Vec<MediaSizeModel>, MediaError> {
        // 1. Cargar imagen original
        let img =
            image::load_from_memory(bytes).map_err(|e| MediaError::ImageProcessingError(e.to_string()))?;

        // Convertir a RGB8 para consistencia (fast_image_resize prefiere buffers planos)
        let width = NonZeroU32::new(img.width()).ok_or(MediaError::InvalidDimensions("Ancho inválido".into()))?;
        let height = NonZeroU32::new(img.height()).ok_or(MediaError::InvalidDimensions("Alto inválido".into()))?;

        // Creamos la imagen de origen
        let src_image =
            fr::Image::from_vec_u8(width, height, img.to_rgb8().into_raw(), fr::PixelType::U8x3)
                .map_err(|_| MediaError::OtherMediaProcessingError("Failed to create src image".into()))?;

        let mut results = Vec::new();

        // 2. Procesar cada variante definida en la receta
        for version in &recipe.target_size {
            let dst_width = NonZeroU32::new(version.width as u32).ok_or(MediaError::InvalidDimensions("Ancho inválido".into()))?;
            let dst_height =
                NonZeroU32::new(version.height as u32).ok_or(MediaError::InvalidDimensions("Alto inválido".into()))?;

            // Contenedor para la imagen escalada
            let mut dst_image = fr::Image::new(dst_width, dst_height, src_image.pixel_type());

            // Configurar el resizer con Lanczos3 (Calidad Premium)
            let mut resizer =
                fr::Resizer::new(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3));

            resizer
                .resize(&src_image.view(), &mut dst_image.view_mut())
                .map_err(|e| MediaError::OtherMediaProcessingError(e.to_string()))?;

            // 3. Codificar a WebP
            // La librería 'webp' espera el buffer, ancho y alto
            let encoder = Encoder::from_rgb(dst_image.buffer(), version.width as u32, version.height as u32);

            // Calidad 80.0 es el estándar de oro para web
            let webp_data = encoder.encode(80.0);

            results.push(MediaSizeModel {
                format: "webp".into(),
                bytes: webp_data.to_vec(),
                priority: version.priority,
                size: version.size.clone(),
                width: version.width,
                height: version.height,
            });
        }

        Ok(results)
    }
}
