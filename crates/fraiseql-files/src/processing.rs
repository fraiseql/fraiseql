//! Image processing pipeline

use async_trait::async_trait;
use bytes::Bytes;
use image::{imageops::FilterType, DynamicImage, ImageFormat};
use std::collections::HashMap;
use std::io::Cursor;

use crate::config::{ProcessingConfig, VariantConfig};
use crate::error::ProcessingError;
use crate::traits::ImageProcessor;

pub struct ProcessedImages {
    pub variants: HashMap<String, Bytes>,
}

pub struct ImageProcessorImpl {
    config: ProcessingConfig,
}

impl ImageProcessorImpl {
    pub fn new(config: ProcessingConfig) -> Self {
        Self { config }
    }

    /// Process an image and generate variants
    pub fn process_sync(&self, data: &Bytes) -> Result<ProcessedImages, ProcessingError> {
        // Load image
        let img = image::load_from_memory(data).map_err(|e| ProcessingError::LoadFailed {
            message: e.to_string(),
        })?;

        // Strip EXIF if configured
        // Note: image crate doesn't preserve EXIF, so it's stripped by default
        // For explicit control, we'd need a different approach

        let mut variants = HashMap::new();

        // Generate original (possibly in different format)
        let original_key = "original".to_string();
        let original_data = self.encode_image(&img, None)?;
        variants.insert(original_key, original_data);

        // Generate configured variants
        for variant_config in &self.config.variants {
            let resized = self.resize_image(&img, variant_config)?;
            let encoded = self.encode_image(&resized, None)?;

            variants.insert(variant_config.name.clone(), encoded);
        }

        Ok(ProcessedImages { variants })
    }

    fn resize_image(
        &self,
        img: &DynamicImage,
        config: &VariantConfig,
    ) -> Result<DynamicImage, ProcessingError> {
        let resized = match config.mode.as_str() {
            "fit" => img.resize(config.width, config.height, FilterType::Lanczos3),
            "fill" | "crop" => img.resize_to_fill(config.width, config.height, FilterType::Lanczos3),
            "exact" => img.resize_exact(config.width, config.height, FilterType::Lanczos3),
            _ => {
                return Err(ProcessingError::InvalidConfig {
                    message: format!("Invalid resize mode: {}", config.mode),
                })
            }
        };

        Ok(resized)
    }

    fn encode_image(
        &self,
        img: &DynamicImage,
        quality: Option<u8>,
    ) -> Result<Bytes, ProcessingError> {
        let format = match self.config.output_format.as_deref() {
            Some("webp") => ImageFormat::WebP,
            Some("jpeg") | Some("jpg") => ImageFormat::Jpeg,
            Some("png") => ImageFormat::Png,
            _ => ImageFormat::Jpeg, // Default
        };

        let quality = quality.or(self.config.quality).unwrap_or(85);

        let mut buffer = Cursor::new(Vec::new());

        match format {
            ImageFormat::Jpeg => {
                let encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality);
                img.write_with_encoder(encoder)
                    .map_err(|e| ProcessingError::EncodeFailed {
                        message: e.to_string(),
                    })?;
            }
            ImageFormat::WebP => {
                // image crate's WebP encoder
                img.write_to(&mut buffer, ImageFormat::WebP)
                    .map_err(|e| ProcessingError::EncodeFailed {
                        message: e.to_string(),
                    })?;
            }
            ImageFormat::Png => {
                img.write_to(&mut buffer, ImageFormat::Png)
                    .map_err(|e| ProcessingError::EncodeFailed {
                        message: e.to_string(),
                    })?;
            }
            _ => {
                return Err(ProcessingError::InvalidConfig {
                    message: format!("Unsupported format: {:?}", format),
                });
            }
        }

        Ok(Bytes::from(buffer.into_inner()))
    }
}

#[async_trait]
impl ImageProcessor for ImageProcessorImpl {
    async fn process(
        &self,
        data: &Bytes,
        _config: &ProcessingConfig,
    ) -> Result<ProcessedImages, ProcessingError> {
        // Run synchronous processing in blocking task
        let data = data.clone();
        let processor = self.clone();
        tokio::task::spawn_blocking(move || processor.process_sync(&data))
            .await
            .map_err(|e| ProcessingError::LoadFailed {
                message: format!("Task join error: {}", e),
            })?
    }
}

impl Clone for ImageProcessorImpl {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}
