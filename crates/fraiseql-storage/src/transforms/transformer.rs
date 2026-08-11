//! Image transformation engine: crop, resize, effects, watermark, encode.

use std::io::Cursor;

use fraiseql_error::{FraiseQLError, Result};
use image::{DynamicImage, ImageEncoder, ImageReader, Rgba};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::ops::{
    CropSpec, Gravity, ResizeMode, Watermark, apply_blur, apply_crop, apply_sharpen,
    apply_watermark, contain_size, resize_into,
};

/// Output format for transformed images
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputFormat {
    /// `WebP` format (modern, efficient)
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
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Webp => "image/webp",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Avif => "image/avif",
            Self::Bmp => "image/bmp",
        }
    }

    /// Get the image format for encoding
    const fn as_image_format(self) -> Option<image::ImageFormat> {
        match self {
            Self::Webp => Some(image::ImageFormat::WebP),
            Self::Jpeg => Some(image::ImageFormat::Jpeg),
            Self::Png => Some(image::ImageFormat::Png),
            Self::Avif => Some(image::ImageFormat::Avif),
            Self::Bmp => None, // Unsupported
        }
    }

    /// Whether this format's encoder has a quality dial at all.
    ///
    /// PNG is lossless by definition, and the `image` crate's `WebP` encoder
    /// writes lossless `WebP` only. A `quality` accepted for either would be a
    /// parameter that does nothing — the shape #973 was filed to remove, not to
    /// add more of — so the render route refuses the combination by name.
    #[must_use]
    pub const fn honours_quality(self) -> bool {
        matches!(self, Self::Jpeg | Self::Avif)
    }

    /// The format's canonical name, as it appears in a URL and in the render
    /// audit record.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Webp => "webp",
            Self::Jpeg => "jpeg",
            Self::Png => "png",
            Self::Avif => "avif",
            Self::Bmp => "bmp",
        }
    }
}

/// Parameters for image transformation
#[derive(Debug, Clone, Default)]
pub struct TransformParams {
    /// Target width in pixels (optional)
    pub width:       Option<u32>,
    /// Target height in pixels (optional)
    pub height:      Option<u32>,
    /// Output format (optional, defaults to input format)
    pub format:      Option<OutputFormat>,
    /// Quality for lossy formats (1-100, default 80)
    pub quality:     Option<u8>,
    /// How the resize fills the requested box (defaults to
    /// [`ResizeMode::Contain`], or the bucket's `default_resize_mode`)
    pub resize_mode: Option<ResizeMode>,
    /// Where a crop or a `fill` keeps its content
    pub gravity:     Option<Gravity>,
    /// Letterbox colour for [`ResizeMode::Fit`]
    pub background:  Option<Rgba<u8>>,
    /// An explicit crop, applied before the resize
    pub crop:        Option<CropSpec>,
    /// Gaussian blur sigma
    pub blur:        Option<f32>,
    /// Unsharp-mask sigma
    pub sharpen:     Option<f32>,
    /// A watermark composited over the result
    pub watermark:   Option<Watermark>,
}

impl TransformParams {
    /// Whether any operation at all was requested.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width.is_none()
            && self.height.is_none()
            && self.format.is_none()
            && self.quality.is_none()
            && self.crop.is_none()
            && self.blur.is_none()
            && self.sharpen.is_none()
            && self.watermark.is_none()
    }

    /// A canonical, stable description of the resolved transform.
    ///
    /// It is the render audit record's `transform` field and the material the
    /// cache key is derived from, so the two can never disagree about what was
    /// rendered: anything that changes the output changes this string.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(w) = self.width {
            parts.push(format!("w={w}"));
        }
        if let Some(h) = self.height {
            parts.push(format!("h={h}"));
        }
        parts.push(format!("mode={}", self.resize_mode.unwrap_or_default().as_str()));
        if let Some(g) = self.gravity {
            parts.push(format!("gravity={}", g.as_str()));
        }
        if let Some(bg) = self.background {
            parts.push(format!("bg={:02x}{:02x}{:02x}{:02x}", bg.0[0], bg.0[1], bg.0[2], bg.0[3]));
        }
        match self.crop {
            Some(CropSpec::BBox { x, y, w, h }) => parts.push(format!("crop={x},{y},{w},{h}")),
            Some(CropSpec::Aspect { w, h }) => parts.push(format!("crop={w}:{h}")),
            None => {},
        }
        if let Some(b) = self.blur {
            parts.push(format!("blur={b}"));
        }
        if let Some(s) = self.sharpen {
            parts.push(format!("sharpen={s}"));
        }
        if let Some(ref mark) = self.watermark {
            parts.push(mark.describe());
        }
        if let Some(f) = self.format {
            parts.push(format!("format={}", f.as_str()));
        }
        if let Some(q) = self.quality {
            parts.push(format!("quality={q}"));
        }
        parts.join(";")
    }
}

/// Output from image transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformOutput {
    /// Transformed image bytes
    pub body:          Vec<u8>,
    /// MIME type of output
    pub content_type:  String,
    /// Actual output width in pixels
    pub width:         u32,
    /// Actual output height in pixels
    pub height:        u32,
    /// `ETag` for cache validation (SHA256 hash of transformed bytes)
    #[serde(default)]
    pub etag:          Option<String>,
    /// Cache control header value for HTTP response
    #[serde(default)]
    pub cache_control: Option<String>,
}

/// Hard ceiling on image dimensions, per side, for both the SOURCE (a
/// decompression bomb declares huge dimensions in a tiny file) and the
/// REQUESTED output (a hostile query string allocates on resize regardless of
/// the source). 12k × 12k ≈ 144 MP ≈ 576 MB decoded RGBA — the practical top
/// end of real photographic assets; anything past it is refused with a named
/// validation error before any allocation (#370).
const MAX_DIMENSION: u32 = 12_000;

/// Encoder quality used when a lossy format is chosen and none was requested.
const DEFAULT_QUALITY: u8 = 80;

/// AVIF encoder speed (0 slowest/best, 10 fastest). 6 keeps a render inside a
/// request's latency budget; the quality dial is the one operators asked for.
const AVIF_SPEED: u8 = 6;

fn invalid(message: impl Into<String>, path: &str) -> FraiseQLError {
    FraiseQLError::Validation {
        message: message.into(),
        path:    Some(path.to_string()),
    }
}

/// Image transformation engine
pub struct ImageTransformer;

impl ImageTransformer {
    /// Transform an image according to the provided parameters.
    ///
    /// The pipeline is crop → resize → blur/sharpen → watermark → encode. Every
    /// stage is bounded before it allocates (#370, extended by #973): the
    /// source and requested dimensions are capped, a crop must lie inside the
    /// source, blur and sharpen radii are capped, and a watermark cannot be
    /// scaled past the canvas it is drawn on.
    ///
    /// # Errors
    /// - `FraiseQLError::Validation` if dimensions are invalid or format is unsupported
    /// - `FraiseQLError::Validation` if input is not a valid image
    /// - `FraiseQLError::Validation` if a parameter exceeds its bound
    pub fn transform(input: &[u8], params: &TransformParams) -> Result<TransformOutput> {
        Self::validate(params)?;
        let img = Self::decode(input)?;

        let cropped = match params.crop {
            Some(spec) => apply_crop(&img, spec, params.gravity.unwrap_or_default())?,
            None => img,
        };

        let mode = params.resize_mode.unwrap_or_default();
        let resized = match (params.width, params.height) {
            (None, None) => cropped,
            (Some(w), Some(h)) => resize_into(
                &cropped,
                w,
                h,
                mode,
                params.gravity.unwrap_or_default(),
                params.background.unwrap_or(Rgba([0, 0, 0, 255])),
            )?,
            (w, h) => {
                // One axis given. Only `contain` has a meaning here: every
                // other mode is defined by the box it fills, and inventing the
                // missing side would silently render something the caller did
                // not ask for.
                if mode.fills_the_box() {
                    return Err(invalid(
                        format!(
                            "resize mode '{}' needs both w and h; only 'contain' can derive the \
                             missing side",
                            mode.as_str()
                        ),
                        "mode",
                    ));
                }
                let (cw, ch) = Self::single_axis_size(&cropped, w, h)?;
                cropped.resize_exact(cw, ch, super::ops::FILTER)
            },
        };

        let blurred = match params.blur {
            Some(sigma) => apply_blur(&resized, sigma)?,
            None => resized,
        };
        let sharpened = match params.sharpen {
            Some(sigma) => apply_sharpen(&blurred, sigma)?,
            None => blurred,
        };
        let marked = match params.watermark {
            Some(ref mark) => apply_watermark(&sharpened, mark)?,
            None => sharpened,
        };

        let output_format = params
            .format
            .unwrap_or_else(|| Self::infer_format(input).unwrap_or(OutputFormat::Jpeg));
        if params.quality.is_some() && !output_format.honours_quality() {
            return Err(invalid(
                format!(
                    "quality does not apply to {}: it is encoded losslessly. Ask for jpeg or \
                     avif, or drop the quality parameter",
                    output_format.as_str()
                ),
                "quality",
            ));
        }

        let (width, height) = (marked.width(), marked.height());
        let body = Self::encode(&marked, output_format, params.quality)?;
        let etag = {
            let mut hasher = Sha256::new();
            hasher.update(&body);
            format!("\"{}\"", hex::encode(hasher.finalize()))
        };

        Ok(TransformOutput {
            body,
            content_type: output_format.mime_type().to_string(),
            width,
            height,
            etag: Some(etag),
            // Cache transformed images for 30 days (they're deterministic based on source +
            // params)
            cache_control: Some("public, max-age=2592000, immutable".to_string()),
        })
    }

    /// Refuse a request whose numbers are out of bounds, before any decode.
    fn validate(params: &TransformParams) -> Result<()> {
        for (value, name) in [(params.width, "width"), (params.height, "height")] {
            if let Some(v) = value {
                if v == 0 {
                    return Err(invalid(format!("{name} must be greater than 0"), name));
                }
                // #370: a hostile *request* is as dangerous as a hostile image
                // — an absurd target size allocates on resize regardless of the
                // source.
                if v > MAX_DIMENSION {
                    return Err(invalid(
                        format!(
                            "Requested dimensions exceed the maximum of {MAX_DIMENSION} pixels \
                             per side"
                        ),
                        "width",
                    ));
                }
            }
        }
        if let Some(q) = params.quality {
            if q == 0 || q > 100 {
                return Err(invalid("quality must be between 1 and 100", "quality"));
            }
        }
        if let Some(fmt) = params.format {
            if fmt == OutputFormat::Bmp || fmt.as_image_format().is_none() {
                return Err(invalid(
                    "BMP format is not supported for transforms".to_string(),
                    "format",
                ));
            }
        }
        Ok(())
    }

    /// Decode under hard limits, refusing decompression bombs from the header.
    fn decode(input: &[u8]) -> Result<DynamicImage> {
        // #370: refuse decompression bombs BEFORE decoding. The header's
        // declared dimensions are read without touching pixel data, so a
        // 100000×100000 declaration (a ~40 GB decode if believed) costs a
        // header parse and a named refusal, never an allocation.
        let (src_w, src_h) = Self::reader(input)?
            .into_dimensions()
            .map_err(|_| invalid("Failed to decode image".to_string(), "input"))?;
        if src_w > MAX_DIMENSION || src_h > MAX_DIMENSION {
            return Err(invalid(
                format!(
                    "Image dimensions ({src_w}x{src_h}) exceed the maximum of {MAX_DIMENSION} \
                     pixels per side"
                ),
                "input",
            ));
        }

        // Belt and braces for headers that lie about their dimensions: the
        // decoder itself runs under hard limits, so even a format whose header
        // parse and pixel stream disagree cannot exhaust the process.
        let mut reader = Self::reader(input)?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        reader.limits(limits);

        reader.decode().map_err(|e| match e {
            image::ImageError::Limits(_) => invalid(
                format!("Image dimensions exceed the maximum of {MAX_DIMENSION} pixels per side"),
                "input",
            ),
            _ => invalid("Failed to decode image".to_string(), "input"),
        })
    }

    /// A format-resolved reader over `input`.
    fn reader(input: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>> {
        let mut reader = ImageReader::new(Cursor::new(input));
        if reader.format().is_none() {
            reader = reader
                .with_guessed_format()
                .map_err(|_| invalid("Could not determine image format".to_string(), "input"))?;
        }
        Ok(reader)
    }

    /// Decode just enough of `input` to name its format.
    fn infer_format(input: &[u8]) -> Option<OutputFormat> {
        match Self::reader(input).ok()?.format()? {
            image::ImageFormat::WebP => Some(OutputFormat::Webp),
            image::ImageFormat::Jpeg => Some(OutputFormat::Jpeg),
            image::ImageFormat::Png => Some(OutputFormat::Png),
            image::ImageFormat::Avif => Some(OutputFormat::Avif),
            _ => None,
        }
    }

    /// Derive the missing side of a `contain` resize from the one that was given.
    fn single_axis_size(
        img: &DynamicImage,
        width: Option<u32>,
        height: Option<u32>,
    ) -> Result<(u32, u32)> {
        let (sw, sh) = (img.width(), img.height());
        match (width, height) {
            (Some(w), None) => contain_size(sw, sh, w, MAX_DIMENSION),
            (None, Some(h)) => contain_size(sw, sh, MAX_DIMENSION, h),
            _ => Ok((sw, sh)),
        }
    }

    /// Encode to `format`, honouring `quality` where the encoder has the dial.
    fn encode(img: &DynamicImage, format: OutputFormat, quality: Option<u8>) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let fail = |what: &str| invalid(format!("Failed to encode {what}"), "format");
        match format {
            OutputFormat::Jpeg => {
                // JPEG has no alpha channel; a padded or watermarked canvas is
                // RGBA, so flatten before handing it to the encoder rather than
                // letting it refuse the colour type.
                let rgb = img.to_rgb8();
                image::codecs::jpeg::JpegEncoder::new_with_quality(
                    &mut Cursor::new(&mut out),
                    quality.unwrap_or(DEFAULT_QUALITY),
                )
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|_| fail("JPEG"))?;
            },
            OutputFormat::Avif => {
                let rgba = img.to_rgba8();
                image::codecs::avif::AvifEncoder::new_with_speed_quality(
                    &mut out,
                    AVIF_SPEED,
                    quality.unwrap_or(DEFAULT_QUALITY),
                )
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(|_| fail("AVIF"))?;
            },
            OutputFormat::Webp => {
                img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::WebP)
                    .map_err(|_| fail("WebP"))?;
            },
            OutputFormat::Png => {
                img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                    .map_err(|_| fail("PNG"))?;
            },
            OutputFormat::Bmp => {
                // Defense in depth: BMP is rejected by `validate`. If we somehow
                // reach here, return an error rather than panic so production
                // cannot be crashed by a missed validation path.
                return Err(invalid(
                    "BMP format is not supported for transforms".to_string(),
                    "format",
                ));
            },
        }
        Ok(out)
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.mime_type())
    }
}

impl ImageTransformer {
    /// Apply a transform preset to get a `TransformParams`
    ///
    /// Presets are named sets of transform parameters that can be defined in bucket configuration.
    /// This helper converts a preset into `TransformParams` for use with the transform method.
    ///
    /// # Arguments
    /// - `preset_name` - Name of the preset to look up
    /// - `presets` - Available presets (typically from `BucketConfig.transform_presets`)
    ///
    /// # Returns
    /// - `Some(TransformParams)` if preset is found
    /// - `None` if preset is not found
    #[must_use]
    pub fn apply_preset(
        preset_name: &str,
        presets: Option<&[crate::config::TransformPreset]>,
    ) -> Option<TransformParams> {
        let presets = presets?;
        let preset = presets.iter().find(|p| p.name == preset_name)?;

        let format = preset.format.as_ref().and_then(|f| match f.to_lowercase().as_str() {
            "webp" => Some(OutputFormat::Webp),
            "jpeg" | "jpg" => Some(OutputFormat::Jpeg),
            "png" => Some(OutputFormat::Png),
            "avif" => Some(OutputFormat::Avif),
            _ => None,
        });

        Some(TransformParams {
            width: preset.width,
            height: preset.height,
            format,
            quality: preset.quality,
            resize_mode: preset.resize_mode.as_deref().and_then(ResizeMode::parse),
            gravity: preset.gravity.as_deref().and_then(Gravity::parse),
            ..TransformParams::default()
        })
    }
}
