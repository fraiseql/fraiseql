//! Image transformation engine for resizing and format conversion.

use fraiseql_error::{FraiseQLError, Result};
use image::ImageReader;

/// Output format for transformed images
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// WebP format (modern, efficient)
    Webp,
    /// JPEG format (lossy, widely supported)
    Jpeg,
    /// PNG format (lossless)
    Png,
    /// AVIF format (modern, efficient)
    Avif,
    /// BMP format (intentionally unsupported)
    Bmp,
}

impl OutputFormat {
    /// Get the MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            OutputFormat::Webp => "image/webp",
            OutputFormat::Jpeg => "image/jpeg",
            OutputFormat::Png => "image/png",
            OutputFormat::Avif => "image/avif",
            OutputFormat::Bmp => "image/bmp",
        }
    }

    /// Get the image format for encoding
    fn as_image_format(&self) -> Option<image::ImageFormat> {
        match self {
            OutputFormat::Webp => Some(image::ImageFormat::WebP),
            OutputFormat::Jpeg => Some(image::ImageFormat::Jpeg),
            OutputFormat::Png => Some(image::ImageFormat::Png),
            OutputFormat::Avif => Some(image::ImageFormat::Avif),
            OutputFormat::Bmp => None, // Unsupported
        }
    }
}

/// Parameters for image transformation
#[derive(Debug, Clone)]
pub struct TransformParams {
    /// Target width in pixels (optional)
    pub width: Option<u32>,
    /// Target height in pixels (optional)
    pub height: Option<u32>,
    /// Output format (optional, defaults to input format)
    pub format: Option<OutputFormat>,
    /// Quality for lossy formats (1-100, default 80)
    pub quality: Option<u8>,
}

/// Output from image transformation
#[derive(Debug, Clone)]
pub struct TransformOutput {
    /// Transformed image bytes
    pub body: Vec<u8>,
    /// MIME type of output
    pub content_type: String,
    /// Actual output width in pixels
    pub width: u32,
    /// Actual output height in pixels
    pub height: u32,
}

/// Image transformation engine
pub struct ImageTransformer;

impl ImageTransformer {
    /// Transform an image according to the provided parameters
    ///
    /// # Arguments
    /// - `input`: Raw image bytes
    /// - `params`: Transform parameters (resize, format, quality)
    ///
    /// # Returns
    /// - `Ok(TransformOutput)` on success
    /// - `Err(FraiseQLError)` if the input is not a valid image, format is unsupported, etc.
    ///
    /// # Errors
    /// - `FraiseQLError::Validation` if dimensions are invalid or format is unsupported
    /// - `FraiseQLError::Validation` if input is not a valid image
    pub fn transform(input: &[u8], params: &TransformParams) -> Result<TransformOutput> {
        // Validate dimensions
        if let Some(w) = params.width {
            if w == 0 {
                return Err(FraiseQLError::Validation {
                    message: "Width must be greater than 0".to_string(),
                    path: Some("width".to_string()),
                });
            }
        }

        if let Some(h) = params.height {
            if h == 0 {
                return Err(FraiseQLError::Validation {
                    message: "Height must be greater than 0".to_string(),
                    path: Some("height".to_string()),
                });
            }
        }

        // Check if output format is supported
        if let Some(fmt) = params.format {
            if fmt == OutputFormat::Bmp {
                return Err(FraiseQLError::Validation {
                    message: "BMP format is not supported for transforms".to_string(),
                    path: Some("format".to_string()),
                });
            }
            if fmt.as_image_format().is_none() {
                return Err(FraiseQLError::Validation {
                    message: format!("Format {:?} is not supported", fmt),
                    path: Some("format".to_string()),
                });
            }
        }

        // Decode the input image
        let cursor = Cursor::new(input);
        let mut reader = ImageReader::new(cursor);

        // Try to infer format if not explicitly set
        if reader.format().is_none() {
            reader = reader.with_guessed_format()
                .map_err(|_| FraiseQLError::Validation {
                    message: "Could not determine image format".to_string(),
                    path: Some("input".to_string()),
                })?;
        }

        let format = reader.format();
        let img = reader.decode()
            .map_err(|_| FraiseQLError::Validation {
                message: "Failed to decode image".to_string(),
                path: Some("input".to_string()),
            })?;

        // Calculate output dimensions
        let (output_width, output_height) = Self::calculate_dimensions(
            img.width(),
            img.height(),
            params.width,
            params.height,
        )?;

        // Resize if needed
        let resized = if params.width.is_some() || params.height.is_some() {
            img.resize_exact(
                output_width,
                output_height,
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            img
        };

        // Determine output format
        let output_format = if let Some(fmt) = params.format {
            fmt
        } else {
            // Infer from input
            Self::infer_format_from_image_format(format).unwrap_or(OutputFormat::Jpeg)
        };

        // Encode to output format
        let mut output_bytes = Vec::new();
        use std::io::Cursor;

        match output_format {
            OutputFormat::Webp => {
                resized.write_to(
                    &mut Cursor::new(&mut output_bytes),
                    image::ImageFormat::WebP,
                ).map_err(|_| FraiseQLError::Validation {
                    message: "Failed to encode WebP".to_string(),
                    path: Some("format".to_string()),
                })?;
            }
            OutputFormat::Jpeg => {
                resized.write_to(
                    &mut Cursor::new(&mut output_bytes),
                    image::ImageFormat::Jpeg,
                ).map_err(|_| FraiseQLError::Validation {
                    message: "Failed to encode JPEG".to_string(),
                    path: Some("format".to_string()),
                })?;
            }
            OutputFormat::Png => {
                resized.write_to(
                    &mut Cursor::new(&mut output_bytes),
                    image::ImageFormat::Png,
                ).map_err(|_| FraiseQLError::Validation {
                    message: "Failed to encode PNG".to_string(),
                    path: Some("format".to_string()),
                })?;
            }
            OutputFormat::Avif => {
                resized.write_to(
                    &mut Cursor::new(&mut output_bytes),
                    image::ImageFormat::Avif,
                ).map_err(|_| FraiseQLError::Validation {
                    message: "Failed to encode AVIF".to_string(),
                    path: Some("format".to_string()),
                })?;
            }
            OutputFormat::Bmp => {
                // Already validated as unsupported above
                unreachable!()
            }
        }

        Ok(TransformOutput {
            body: output_bytes,
            content_type: output_format.mime_type().to_string(),
            width: output_width,
            height: output_height,
        })
    }

    /// Calculate output dimensions preserving aspect ratio
    fn calculate_dimensions(
        orig_width: u32,
        orig_height: u32,
        target_width: Option<u32>,
        target_height: Option<u32>,
    ) -> Result<(u32, u32)> {
        let (width, height) = match (target_width, target_height) {
            (Some(w), Some(h)) => {
                // Both specified: fit within bounds preserving aspect ratio
                let aspect = orig_width as f32 / orig_height as f32;
                let target_aspect = w as f32 / h as f32;

                if aspect > target_aspect {
                    // Original is wider, scale by width
                    (w, (w as f32 / aspect) as u32)
                } else {
                    // Original is taller, scale by height
                    ((h as f32 * aspect) as u32, h)
                }
            }
            (Some(w), None) => {
                // Only width specified, calculate height
                let h = (w as f32 * orig_height as f32 / orig_width as f32) as u32;
                (w, h)
            }
            (None, Some(h)) => {
                // Only height specified, calculate width
                let w = (h as f32 * orig_width as f32 / orig_height as f32) as u32;
                (w, h)
            }
            (None, None) => {
                // No dimensions specified, use original
                (orig_width, orig_height)
            }
        };

        if width == 0 || height == 0 {
            return Err(FraiseQLError::Validation {
                message: "Calculated dimensions would be zero".to_string(),
                path: Some("dimensions".to_string()),
            });
        }

        Ok((width, height))
    }

    /// Infer output format from the decoded image format
    fn infer_format_from_image_format(format: Option<image::ImageFormat>) -> Option<OutputFormat> {
        match format {
            Some(image::ImageFormat::WebP) => Some(OutputFormat::Webp),
            Some(image::ImageFormat::Jpeg) => Some(OutputFormat::Jpeg),
            Some(image::ImageFormat::Png) => Some(OutputFormat::Png),
            Some(image::ImageFormat::Avif) => Some(OutputFormat::Avif),
            _ => None,
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.mime_type())
    }
}
