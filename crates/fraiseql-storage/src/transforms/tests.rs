#![cfg(all(test, feature = "transforms"))]
#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
#![allow(clippy::indexing_slicing)] // Reason: test fixtures index into known-shape collections; OOB indices correctly fail the test
#![allow(clippy::cast_precision_loss)] // Reason: u32→f64 math on image dimensions in test assertions
#![allow(clippy::cast_possible_truncation)] // Reason: rounding image dimensions back to integer in test assertions
#![allow(clippy::cast_sign_loss)] // Reason: rounding image dimensions back to unsigned in test assertions
#![allow(missing_docs)] // Reason: test functions are self-describing

use std::io::Cursor;

use image::{ImageBuffer, ImageFormat, RgbImage, Rgba, RgbaImage};

use super::*;

/// Helper to create a simple test image (JPEG-like RGB)
fn create_test_image_1000x800() -> Vec<u8> {
    let img: RgbImage = ImageBuffer::from_fn(1000, 800, |x, y| {
        image::Rgb([
            ((x % 256) as u8),
            ((y % 256) as u8),
            (((x + y) % 256) as u8),
        ])
    });

    let dyn_img = image::DynamicImage::ImageRgb8(img);
    let mut bytes = Vec::new();
    dyn_img
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Jpeg)
        .expect("Failed to encode test JPEG");
    bytes
}

/// Helper to create a simple test PNG with alpha channel
fn create_test_png_with_alpha() -> Vec<u8> {
    let img: RgbaImage = ImageBuffer::from_fn(800, 600, |x, y| {
        let alpha = if (x + y) % 2 == 0 { 255 } else { 128 };
        Rgba([
            ((x % 256) as u8),
            ((y % 256) as u8),
            (((x + y) % 256) as u8),
            alpha,
        ])
    });

    let dyn_img = image::DynamicImage::ImageRgba8(img);
    let mut bytes = Vec::new();
    dyn_img
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("Failed to encode test PNG");
    bytes
}

/// Helper to create a simple test PDF (non-image)
fn create_test_pdf() -> Vec<u8> {
    // Minimal PDF structure (just enough to be a valid PDF)
    b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec()
}

#[test]
fn test_resize_jpeg_to_width() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: Some(500),
        height: None,
        format: None,
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.width <= 500);
    // Height should be auto-scaled to preserve aspect ratio
    let expected_height = (500 * 800) / 1000;
    assert!(output.height <= expected_height + 1); // Allow 1px rounding error
}

#[test]
fn test_resize_with_height() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: None,
        height: Some(200),
        format: None,
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert!(output.height <= 200);
    // Width should be auto-scaled to preserve aspect ratio
    let expected_width = (200 * 1000) / 800;
    assert!(output.width <= expected_width + 1);
}

#[test]
fn test_resize_with_both_dimensions() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: Some(300),
        height: Some(300),
        format: None,
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Should fit within bounds without stretching
    assert!(output.width <= 300);
    assert!(output.height <= 300);
}

#[test]
fn test_convert_jpeg_to_webp() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: None,
        height: None,
        format: Some(OutputFormat::Webp),
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert_eq!(output.content_type, "image/webp");
    // Verify it's actually WebP by checking for RIFF header
    assert!(output.body.starts_with(b"RIFF"));
}

#[test]
fn test_convert_png_to_jpeg() {
    let input = create_test_png_with_alpha();
    let params = TransformParams {
        width: None,
        height: None,
        format: Some(OutputFormat::Jpeg),
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert_eq!(output.content_type, "image/jpeg");
    // JPEG magic number: FFD8
    assert_eq!(&output.body[0..2], &[0xFF, 0xD8]);
}

#[test]
fn test_unsupported_format_returns_error() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: None,
        height: None,
        format: Some(OutputFormat::Bmp), // BMP is intentionally unsupported
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_err());
}

#[test]
fn test_non_image_file_returns_error() {
    let input = create_test_pdf();
    let params = TransformParams {
        width: None,
        height: None,
        format: None,
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_err());
}

#[test]
fn test_transform_with_quality_parameter() {
    let input = create_test_image_1000x800();
    let params_low_quality = TransformParams {
        width: Some(500),
        height: None,
        format: Some(OutputFormat::Jpeg),
        quality: Some(50),
        ..TransformParams::default()
    };

    let params_high_quality = TransformParams {
        width: Some(500),
        height: None,
        format: Some(OutputFormat::Jpeg),
        quality: Some(95),
        ..TransformParams::default()
    };

    let result_low = ImageTransformer::transform(&input, &params_low_quality);
    let result_high = ImageTransformer::transform(&input, &params_high_quality);

    assert!(result_low.is_ok());
    assert!(result_high.is_ok());

    // Both should produce valid JPEG output
    let low_output = result_low.unwrap();
    let high_output = result_high.unwrap();
    assert_eq!(low_output.content_type, "image/jpeg");
    assert_eq!(high_output.content_type, "image/jpeg");
    // JPEG magic number verification
    assert_eq!(&low_output.body[0..2], &[0xFF, 0xD8]);
    assert_eq!(&high_output.body[0..2], &[0xFF, 0xD8]);
}

#[test]
fn test_transform_default_quality() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: Some(500),
        height: None,
        format: Some(OutputFormat::Jpeg),
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_ok());
    // Default quality (80) should work without error
}

#[test]
fn test_invalid_dimensions_returns_error() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: Some(0), // Invalid: zero width
        height: None,
        format: None,
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_err());
}

#[test]
fn test_resize_maintains_aspect_ratio() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: Some(250),
        height: None,
        format: None,
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_ok());

    let output = result.unwrap();
    // Original aspect ratio: 1000:800 = 1.25
    let output_ratio = output.width as f32 / output.height as f32;
    let original_ratio = 1000.0 / 800.0;

    // Allow small rounding error
    assert!((output_ratio - original_ratio).abs() < 0.05);
}

#[test]
fn test_transform_output_has_correct_dimensions() {
    let input = create_test_image_1000x800();
    let params = TransformParams {
        width: Some(500),
        height: None,
        format: None,
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_ok());

    let output = result.unwrap();
    assert_eq!(output.width, 500);
    assert_eq!(output.height, 400); // 500 * 800 / 1000
}

#[test]
fn test_transform_empty_input_returns_error() {
    let input = vec![];
    let params = TransformParams {
        width: Some(500),
        height: None,
        format: None,
        quality: None,
        ..TransformParams::default()
    };

    let result = ImageTransformer::transform(&input, &params);
    assert!(result.is_err());
}

#[test]
fn test_apply_preset_thumbnail() {
    use crate::config::TransformPreset;

    let presets = vec![
        TransformPreset {
            name: "thumbnail".to_string(),
            width: Some(150),
            height: Some(150),
            format: Some("webp".to_string()),
            quality: Some(80),
            ..TransformPreset::default()
        },
        TransformPreset {
            name: "medium".to_string(),
            width: Some(800),
            height: Some(600),
            format: Some("jpeg".to_string()),
            quality: Some(85),
            ..TransformPreset::default()
        },
    ];

    let params = ImageTransformer::apply_preset("thumbnail", Some(&presets));
    assert!(params.is_some());

    let p = params.unwrap();
    assert_eq!(p.width, Some(150));
    assert_eq!(p.height, Some(150));
    assert_eq!(p.format, Some(OutputFormat::Webp));
    assert_eq!(p.quality, Some(80));
}

#[test]
fn test_apply_preset_not_found() {
    use crate::config::TransformPreset;

    let presets = vec![TransformPreset {
        name: "thumbnail".to_string(),
        width: Some(150),
        height: Some(150),
        format: Some("webp".to_string()),
        quality: Some(80),
        ..TransformPreset::default()
    }];

    let params = ImageTransformer::apply_preset("nonexistent", Some(&presets));
    assert!(params.is_none());
}

#[test]
fn test_apply_preset_none_presets() {
    let params = ImageTransformer::apply_preset("any", None);
    assert!(params.is_none());
}

#[test]
fn test_apply_preset_format_conversion() {
    use crate::config::TransformPreset;

    let presets = vec![
        TransformPreset {
            name: "png".to_string(),
            width: None,
            height: None,
            format: Some("png".to_string()),
            quality: None,
            ..TransformPreset::default()
        },
        TransformPreset {
            name: "jpg".to_string(),
            width: None,
            height: None,
            format: Some("jpg".to_string()),
            quality: None,
            ..TransformPreset::default()
        },
        TransformPreset {
            name: "avif".to_string(),
            width: None,
            height: None,
            format: Some("avif".to_string()),
            quality: None,
            ..TransformPreset::default()
        },
    ];

    let png_params = ImageTransformer::apply_preset("png", Some(&presets)).unwrap();
    assert_eq!(png_params.format, Some(OutputFormat::Png));

    let jpg_params = ImageTransformer::apply_preset("jpg", Some(&presets)).unwrap();
    assert_eq!(jpg_params.format, Some(OutputFormat::Jpeg));

    let avif_params = ImageTransformer::apply_preset("avif", Some(&presets)).unwrap();
    assert_eq!(avif_params.format, Some(OutputFormat::Avif));
}

// ============================================================================
// Transform Caching & HTTP Route Tests
// ============================================================================

/// #973: the cache round-trips a rendering, and the entry it reads is the one
/// it wrote.
#[tokio::test]
async fn render_cache_round_trips_a_rendering() {
    use std::sync::Arc;

    use tempfile::TempDir;

    use crate::{
        backend::{LocalBackend, StorageBackend},
        transforms::cache::TransformCache,
    };

    let temp_dir = TempDir::new().unwrap();
    let backend =
        Arc::new(StorageBackend::Local(LocalBackend::new(temp_dir.path().to_str().unwrap())));
    let cache = TransformCache::new(Arc::clone(&backend));

    let source = create_test_image_1000x800();
    let params = TransformParams {
        width: Some(500),
        ..TransformParams::default()
    };
    let cache_key = TransformCache::build_cache_key("docs", "test.jpg", &source, &params);

    assert!(cache.get(&cache_key).await.is_none(), "an unwritten key must miss");

    let output = ImageTransformer::transform(&source, &params).unwrap();
    cache.put(&cache_key, &output).await.unwrap();

    let hit = cache.get(&cache_key).await.expect("the entry just written must be readable");
    assert_eq!(hit.body, output.body, "a hit must serve exactly what was stored");
    assert_eq!(hit.width, 500);
}

/// #973: invalidation is structural. A re-uploaded source hashes differently
/// and therefore reads a *different* key, so a stale entry is unreachable
/// rather than merely marked — which is what the previous cache's `invalidate`
/// pretended to do while writing a marker no reader consulted.
#[tokio::test]
async fn render_cache_keys_change_with_the_source() {
    use crate::transforms::cache::TransformCache;

    let params = TransformParams {
        width: Some(500),
        ..TransformParams::default()
    };
    let first = create_test_image_1000x800();
    let second = create_test_png_with_alpha();

    let key_1 = TransformCache::build_cache_key("docs", "test.jpg", &first, &params);
    let key_2 = TransformCache::build_cache_key("docs", "test.jpg", &second, &params);
    assert_ne!(key_1, key_2, "a changed source must not read the previous rendering's key");
}

/// #973: every parameter that changes the output changes the key. A cache that
/// collapsed two different renderings onto one key would serve the wrong image
/// with a `200`.
#[test]
fn render_cache_keys_change_with_every_parameter() {
    use crate::transforms::{Gravity, ResizeMode, cache::TransformCache, ops::CropSpec};

    let source = create_test_image_1000x800();
    let base = TransformParams {
        width: Some(500),
        height: Some(400),
        ..TransformParams::default()
    };
    let key_of = |p: &TransformParams| TransformCache::build_cache_key("docs", "k.jpg", &source, p);
    let baseline = key_of(&base);

    let variants: Vec<(&str, TransformParams)> = vec![
        (
            "mode",
            TransformParams {
                resize_mode: Some(ResizeMode::Fill),
                ..base.clone()
            },
        ),
        (
            "gravity",
            TransformParams {
                gravity: Some(Gravity::North),
                ..base.clone()
            },
        ),
        (
            "format",
            TransformParams {
                format: Some(OutputFormat::Png),
                ..base.clone()
            },
        ),
        (
            "quality",
            TransformParams {
                quality: Some(42),
                ..base.clone()
            },
        ),
        (
            "blur",
            TransformParams {
                blur: Some(3.0),
                ..base.clone()
            },
        ),
        (
            "sharpen",
            TransformParams {
                sharpen: Some(3.0),
                ..base.clone()
            },
        ),
        (
            "crop",
            TransformParams {
                crop: Some(CropSpec::Aspect { w: 16, h: 9 }),
                ..base.clone()
            },
        ),
        ("bucket-scope", base.clone()),
    ];
    for (name, params) in &variants[..variants.len() - 1] {
        assert_ne!(baseline, key_of(params), "{name} must change the cache key");
    }
    assert_ne!(
        baseline,
        TransformCache::build_cache_key("other", "k.jpg", &source, &base),
        "the bucket must be part of the key: two buckets can hold the same key"
    );
}

/// #973: no caller-supplied key can name a cache entry. A caller's object
/// always lands under its logical bucket, and every key is validated to be a
/// relative path with no `.`/`..` segment — so the only route into the cache
/// namespace is a bucket *configured* with the reserved name, which the server
/// refuses at boot.
#[test]
fn render_cache_namespace_is_unreachable_from_a_client_key() {
    use crate::{
        backend::validate_key, config::RESERVED_BUCKET_NAMES, transforms::cache::CACHE_PREFIX,
    };

    let source = create_test_image_1000x800();
    let params = TransformParams {
        width: Some(10),
        ..TransformParams::default()
    };
    let key = TransformCache::build_cache_key("docs", "k.jpg", &source, &params);
    assert!(key.starts_with(CACHE_PREFIX), "entries must live under the reserved prefix");
    assert!(
        RESERVED_BUCKET_NAMES.contains(&CACHE_PREFIX),
        "the cache prefix must be a name no bucket may take"
    );

    // Every escape a key could attempt out of its bucket is already refused.
    for escape in [
        "../.fraiseql-transforms/x",
        "..",
        "a/../../.fraiseql-transforms/x",
        "/.fraiseql-transforms/x",
        ".fraiseql-transforms//x",
    ] {
        assert!(validate_key(escape).is_err(), "{escape:?} must be refused");
    }
}

/// #973: a preset resolves into the same parameters the cache keys on, so a
/// preset render is cached like any other.
#[tokio::test]
async fn render_cache_with_preset_lookup() {
    use std::sync::Arc;

    use tempfile::TempDir;

    use crate::{
        backend::{LocalBackend, StorageBackend},
        config::TransformPreset,
        transforms::cache::TransformCache,
    };

    let temp_dir = TempDir::new().unwrap();
    let backend =
        Arc::new(StorageBackend::Local(LocalBackend::new(temp_dir.path().to_str().unwrap())));
    let cache = TransformCache::new(Arc::clone(&backend));
    let source = create_test_image_1000x800();

    // `quality` is deliberately absent: WebP is encoded losslessly here, so a
    // quality on this preset would be refused rather than silently ignored.
    let presets = vec![TransformPreset {
        name: "thumbnail".to_string(),
        width: Some(150),
        height: Some(150),
        format: Some("webp".to_string()),
        ..TransformPreset::default()
    }];

    let params = ImageTransformer::apply_preset("thumbnail", Some(&presets)).unwrap();
    let output = ImageTransformer::transform(&source, &params).unwrap();

    // Original is 1000x800, so a 150x150 `contain` preset yields 150x120.
    assert_eq!(output.width, 150);
    assert_eq!(output.height, 120);
    assert_eq!(output.content_type, "image/webp");

    let cache_key = TransformCache::build_cache_key("docs", "test.jpg", &source, &params);
    cache.put(&cache_key, &output).await.unwrap();
    assert_eq!(cache.get(&cache_key).await.unwrap().body, output.body);
}

// ── #370: hostile inputs are rejected with bounded resource use ─────────────

/// A syntactically valid PNG header declaring 20000×20000 pixels (a ~1.6 GB
/// decode if believed — past FraiseQL's 12k/side ceiling, but small enough
/// that the png header parser itself does not refuse it first, so this pins
/// OUR guard). Only the signature + IHDR chunk: header parsing sees the
/// claimed size from the IHDR; a token IDAT/IEND lets header parsing
/// complete without any real pixel data.
pub const BOMB_PNG_HEADER: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x4E, 0x20, 0x00, 0x00, 0x4E, 0x20, 0x08, 0x02, 0x00, 0x00, 0x00, 0x6C, 0x12, 0xD1,
    0x6E, 0x00, 0x00, 0x00, 0x09, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x5E, 0xFF, 0x7D, 0xF9, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42,
    0x60, 0x82,
];

/// A decompression bomb must be refused BY THE DIMENSION GUARD — a named,
/// clean validation error — not by whatever the decoder happens to do while
/// attempting a 40 GB allocation.
#[test]
fn decompression_bomb_is_rejected_by_the_dimension_guard() {
    let result = ImageTransformer::transform(
        BOMB_PNG_HEADER,
        &TransformParams {
            width: Some(100),
            height: None,
            format: None,
            quality: None,
            ..TransformParams::default()
        },
    );
    let err = result.expect_err("a 20000x20000 declaration must be refused");
    let message = err.to_string();
    assert!(
        message.contains("dimensions") && message.contains("exceed"),
        "the refusal must name the dimension limit, proving the guard (not a \
         downstream decode failure) rejected it: {message}"
    );
}

/// A hostile *request* must not allocate either: an absurd target size is
/// refused up front, before any resize work.
#[test]
fn oversized_target_dimensions_are_rejected() {
    let input = create_test_image_1000x800();
    let result = ImageTransformer::transform(
        &input,
        &TransformParams {
            width: Some(60_000),
            height: Some(60_000),
            format: None,
            quality: None,
            ..TransformParams::default()
        },
    );
    let err = result.expect_err("a 60000x60000 target must be refused");
    let message = err.to_string();
    assert!(
        message.contains("dimensions") && message.contains("exceed"),
        "the refusal must name the dimension limit: {message}"
    );
}

/// Garbage bytes fail as a clean validation error, never a panic or a 500.
#[test]
fn malformed_bytes_are_a_clean_validation_error() {
    let garbage = vec![0xFF_u8; 4096];
    let result = ImageTransformer::transform(
        &garbage,
        &TransformParams {
            width: Some(10),
            height: None,
            format: None,
            quality: None,
            ..TransformParams::default()
        },
    );
    assert!(matches!(result, Err(fraiseql_error::FraiseQLError::Validation { .. })));
}

// ── #973: resize modes, crop, effects, watermark, quality ──────────────────

/// A 1000×800 source whose top-left quadrant is bright and whose right half is
/// flat, so a gravity choice is visible in the output.
fn lopsided_source() -> Vec<u8> {
    let img = image::RgbImage::from_fn(1000, 800, |x, y| {
        if x < 200 && y < 160 {
            // A busy chequer in the top-left: high edge energy.
            let on = ((x / 4) + (y / 4)) % 2 == 0;
            image::Rgb([if on { 255 } else { 0 }, if on { 255 } else { 0 }, 0])
        } else {
            image::Rgb([10, 10, 10])
        }
    });
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn decode(output: &TransformOutput) -> image::DynamicImage {
    image::load_from_memory(&output.body).expect("rendered output decodes")
}

fn render_at(mode: ResizeMode, w: u32, h: u32) -> image::DynamicImage {
    let params = TransformParams {
        width: Some(w),
        height: Some(h),
        format: Some(OutputFormat::Png),
        resize_mode: Some(mode),
        background: Some(image::Rgba([0, 0, 255, 255])),
        ..TransformParams::default()
    };
    decode(&ImageTransformer::transform(&lopsided_source(), &params).unwrap())
}

/// #973's core ask: the caller can choose how the box is filled, and each mode
/// produces the geometry its name promises. `contain` — the behaviour that
/// shipped — keeps the source's aspect ratio and is therefore the only mode
/// that does not return the requested box.
#[test]
fn each_resize_mode_produces_the_geometry_it_names() {
    use image::GenericImageView;

    // 1000×800 into a 400×400 box.
    assert_eq!(render_at(ResizeMode::Contain, 400, 400).dimensions(), (400, 320));
    for mode in [
        ResizeMode::Stretch,
        ResizeMode::Fit,
        ResizeMode::Fill,
        ResizeMode::CoverBlur,
        ResizeMode::CoverMirror,
    ] {
        assert_eq!(
            render_at(mode, 400, 400).dimensions(),
            (400, 400),
            "{} must fill the requested box exactly",
            mode.as_str()
        );
    }
}

/// The modes are not aliases of each other: `stretch` distorts where `contain`
/// preserves, and `fill` crops where `fit` pads. Before #973 the caller had one
/// behaviour and no way to ask for another.
#[test]
fn resize_modes_are_visibly_different() {
    use image::GenericImageView;

    let fit = render_at(ResizeMode::Fit, 400, 400);
    let fill = render_at(ResizeMode::Fill, 400, 400);
    let stretch = render_at(ResizeMode::Stretch, 400, 400);

    // `fit` letterboxes with the requested background: the top-right corner of
    // a 1000×800 source in a square box is a bar.
    assert_eq!(
        fit.get_pixel(399, 2).0[..3],
        [0, 0, 255],
        "fit must letterbox with the requested background colour"
    );
    // `fill` crops instead, so the same pixel is image content.
    assert_ne!(fill.get_pixel(399, 2).0[..3], [0, 0, 255], "fill must not letterbox");
    // `stretch` fills the box with image content too, but by distorting rather
    // than cropping — its content differs from `fill`'s.
    assert_ne!(stretch.get_pixel(399, 2).0[..3], [0, 0, 255]);
    // At (60, 40): `stretch` maps the whole 1000×800 frame into the box, so the
    // busy top-left quadrant still covers it; `fill` scales to cover and crops
    // 50 px off each side, so the same output pixel is past the quadrant.
    assert_ne!(
        stretch.get_pixel(60, 40).0,
        fill.get_pixel(60, 40).0,
        "stretch and fill must not resolve to the same rendering"
    );
}

/// `cover-blur` and `cover-mirror` fill the bars from the image itself, so the
/// bar is neither the background colour nor a copy of the letterbox.
#[test]
fn cover_modes_fill_the_bars_from_the_image() {
    use image::GenericImageView;

    for mode in [ResizeMode::CoverBlur, ResizeMode::CoverMirror] {
        let rendered = render_at(mode, 400, 400);
        let corner = rendered.get_pixel(399, 2).0;
        assert_ne!(
            corner[..3],
            [0, 0, 255],
            "{} must not fall back to the background colour",
            mode.as_str()
        );
    }
}

/// Gravity decides which part of a `fill` survives the crop. The source's busy
/// quadrant is top-left, so `north-west` keeps it and `south-east` does not.
#[test]
fn fill_honours_gravity() {
    use image::GenericImageView;

    let with = |gravity: Gravity| {
        let params = TransformParams {
            width: Some(300),
            height: Some(300),
            format: Some(OutputFormat::Png),
            resize_mode: Some(ResizeMode::Fill),
            gravity: Some(gravity),
            ..TransformParams::default()
        };
        decode(&ImageTransformer::transform(&lopsided_source(), &params).unwrap())
    };

    let north_west = with(Gravity::NorthWest);
    let south_east = with(Gravity::SouthEast);
    assert_ne!(
        north_west.get_pixel(10, 10).0,
        south_east.get_pixel(10, 10).0,
        "opposite gravities must not produce the same crop"
    );
    // The chequer is bright; the rest of the frame is near-black.
    let brightness = |img: &image::DynamicImage| -> u32 {
        (0..40).map(|i| u32::from(img.get_pixel(i, i).0[0])).sum()
    };
    assert!(
        brightness(&north_west) > brightness(&south_east),
        "north-west must keep the busy top-left quadrant"
    );
}

/// `smart` gravity is resolved from the pixels, and lands on the busy quadrant
/// rather than on the flat expanse that occupies most of the frame.
#[test]
fn smart_gravity_finds_the_busy_region() {
    use image::GenericImageView;

    let params = TransformParams {
        crop: Some(CropSpec::Aspect { w: 1, h: 1 }),
        format: Some(OutputFormat::Png),
        gravity: Some(Gravity::Smart),
        ..TransformParams::default()
    };
    let smart = decode(&ImageTransformer::transform(&lopsided_source(), &params).unwrap());
    let centred = {
        let params = TransformParams {
            gravity: Some(Gravity::Center),
            ..params
        };
        decode(&ImageTransformer::transform(&lopsided_source(), &params).unwrap())
    };
    let brightness = |img: &image::DynamicImage| -> u32 {
        (0..40).map(|i| u32::from(img.get_pixel(i, i).0[0])).sum()
    };
    assert!(
        brightness(&smart) > brightness(&centred),
        "smart gravity must prefer the high-energy quadrant over the flat centre"
    );
}

/// A crop rectangle outside the source is refused, not clamped: quietly
/// returning a different rectangle would be the wrong answer with a `200`.
#[test]
fn a_crop_outside_the_source_is_refused() {
    let params = TransformParams {
        crop: Some(CropSpec::BBox {
            x: 900,
            y: 700,
            w: 400,
            h: 400,
        }),
        format: Some(OutputFormat::Png),
        ..TransformParams::default()
    };
    let err = ImageTransformer::transform(&lopsided_source(), &params).unwrap_err();
    assert!(format!("{err}").contains("outside"), "{err}");
}

/// An aspect crop takes the largest rectangle of that ratio that fits.
#[test]
fn an_aspect_crop_takes_the_largest_box() {
    let params = TransformParams {
        crop: Some(CropSpec::Aspect { w: 1, h: 1 }),
        format: Some(OutputFormat::Png),
        ..TransformParams::default()
    };
    let out = ImageTransformer::transform(&lopsided_source(), &params).unwrap();
    // 1000×800 source, square crop → 800×800.
    assert_eq!((out.width, out.height), (800, 800));
}

/// Crop specifications parse in both shapes and refuse everything else.
#[test]
fn crop_specifications_parse_or_refuse() {
    assert_eq!(
        CropSpec::parse("10,20,30,40").unwrap(),
        CropSpec::BBox {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        }
    );
    assert_eq!(CropSpec::parse("16:9").unwrap(), CropSpec::Aspect { w: 16, h: 9 });
    for bad in [
        "",
        "1,2,3",
        "1,2,3,4,5",
        "a,b,c,d",
        "0:1",
        "1,2,0,4",
        "16:0",
    ] {
        assert!(CropSpec::parse(bad).is_err(), "{bad:?} must be refused");
    }
}

/// #370's invariant, restated for the effects #973 adds: each carries its own
/// cap, and a request past it is a named refusal rather than an allocation.
#[test]
fn blur_and_sharpen_are_bounded_before_they_allocate() {
    use crate::transforms::ops::{MAX_BLUR_SIGMA, MAX_SHARPEN_SIGMA};

    let source = lopsided_source();
    let with = |blur: Option<f32>, sharpen: Option<f32>| {
        ImageTransformer::transform(
            &source,
            &TransformParams {
                width: Some(100),
                format: Some(OutputFormat::Png),
                blur,
                sharpen,
                ..TransformParams::default()
            },
        )
    };

    assert!(with(Some(2.0), None).is_ok());
    assert!(with(None, Some(2.0)).is_ok());
    assert!(
        with(Some(MAX_BLUR_SIGMA + 1.0), None).is_err(),
        "blur past its cap must be refused"
    );
    assert!(
        with(None, Some(MAX_SHARPEN_SIGMA + 1.0)).is_err(),
        "sharpen past its cap must be refused"
    );
    assert!(with(Some(0.0), None).is_err(), "a zero radius is a mistake, not a no-op");
    assert!(with(Some(-1.0), None).is_err());
}

/// The radius cap alone bounds nothing: `image`'s Gaussian is separable with a
/// kernel proportional to sigma, so the cost is linear in pixels AND in radius.
/// A sigma of 100 over the 12 000 px dimension ceiling is ~90 seconds of CPU —
/// the resource-exhaustion shape #370 exists to refuse. The budget is on the
/// product.
#[test]
fn the_blur_budget_bounds_pixels_times_radius() {
    use crate::transforms::ops::MAX_BLUR_WORK;

    let source = lopsided_source();
    let at = |side: u32, sigma: f32| {
        ImageTransformer::transform(
            &source,
            &TransformParams {
                width: Some(side),
                height: Some(side),
                resize_mode: Some(ResizeMode::Stretch),
                format: Some(OutputFormat::Png),
                blur: Some(sigma),
                ..TransformParams::default()
            },
        )
    };

    // A small canvas can afford a large radius.
    let small = 500_u32;
    let affordable = MAX_BLUR_WORK / u64::from(small) / u64::from(small);
    assert!(affordable >= 8, "the budget must leave a usable radius on a small canvas");
    assert!(at(small, 8.0).is_ok());

    // A large canvas cannot afford the same radius, even though it is far
    // inside the flat sigma cap.
    let large = 4000_u32;
    let over = (MAX_BLUR_WORK / u64::from(large) / u64::from(large)) + 4;
    // Reason: `over` is bounded by the budget arithmetic above.
    #[allow(clippy::cast_precision_loss)]
    let over = over as f32;
    assert!(
        over < crate::transforms::ops::MAX_BLUR_SIGMA,
        "the case must be inside the flat cap"
    );
    let refused = at(large, over).unwrap_err();
    assert!(
        format!("{refused}").contains("budget"),
        "a blur past the work budget must be refused by name: {refused}"
    );

    // `cover-blur` fills its bars from the image and chooses its own radius, so
    // it clamps to the budget rather than failing a request nobody mis-typed.
    assert!(
        ImageTransformer::transform(
            &source,
            &TransformParams {
                width: Some(large),
                height: Some(large),
                resize_mode: Some(ResizeMode::CoverBlur),
                format: Some(OutputFormat::Png),
                ..TransformParams::default()
            }
        )
        .is_ok(),
        "cover-blur must clamp its own radius, not refuse"
    );
}

/// A blur actually blurs: the rendering differs from the unblurred one.
#[test]
fn blur_changes_the_rendering() {
    let source = lopsided_source();
    let base = TransformParams {
        width: Some(200),
        height: Some(160),
        format: Some(OutputFormat::Png),
        ..TransformParams::default()
    };
    let plain = ImageTransformer::transform(&source, &base).unwrap();
    let blurred = ImageTransformer::transform(
        &source,
        &TransformParams {
            blur: Some(6.0),
            ..base
        },
    )
    .unwrap();
    assert_ne!(plain.body, blurred.body, "blur must change the output");
}

/// A watermark is composited, cannot be scaled past the canvas, and its scale
/// is validated before anything is drawn.
#[test]
fn a_watermark_is_bounded_by_the_canvas() {
    let mark = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        50,
        50,
        image::Rgba([255, 0, 0, 255]),
    ));
    let with_scale = |scale: f32| {
        ImageTransformer::transform(
            &lopsided_source(),
            &TransformParams {
                width: Some(200),
                height: Some(160),
                format: Some(OutputFormat::Png),
                watermark: Some(Watermark {
                    image: mark.clone(),
                    gravity: Gravity::SouthEast,
                    opacity: 255,
                    scale,
                    margin: 0,
                    source: "test".to_string(),
                }),
                ..TransformParams::default()
            },
        )
    };

    assert!(with_scale(0.25).is_ok());
    assert!(with_scale(1.0).is_ok(), "a full-canvas-width mark is the largest allowed");
    assert!(
        with_scale(1.5).is_err(),
        "a mark cannot be larger than the canvas it is drawn on"
    );
    assert!(with_scale(0.0).is_err());
    assert!(with_scale(-1.0).is_err());

    // And it is actually drawn: the marked rendering differs.
    let plain = ImageTransformer::transform(
        &lopsided_source(),
        &TransformParams {
            width: Some(200),
            height: Some(160),
            format: Some(OutputFormat::Png),
            ..TransformParams::default()
        },
    )
    .unwrap();
    assert_ne!(
        plain.body,
        with_scale(0.25).unwrap().body,
        "the watermark must reach the output"
    );
}

/// #973 found `quality` accepted by the route, threaded through, and never
/// handed to an encoder. It now reaches the encoder: a low quality produces
/// materially fewer bytes than a high one.
#[test]
fn quality_reaches_the_encoder() {
    let source = lopsided_source();
    let at = |q: u8| {
        ImageTransformer::transform(
            &source,
            &TransformParams {
                width: Some(600),
                height: Some(480),
                format: Some(OutputFormat::Jpeg),
                quality: Some(q),
                ..TransformParams::default()
            },
        )
        .unwrap()
        .body
        .len()
    };
    let (low, high) = (at(10), at(95));
    assert!(
        low * 2 < high,
        "quality=10 must be far smaller than quality=95 ({low} vs {high})"
    );
}

/// Quality on a format this server encodes losslessly is refused by name,
/// rather than accepted and silently dropped — the shape #973 exists to remove.
#[test]
fn quality_on_a_lossless_format_is_refused() {
    for format in [OutputFormat::Png, OutputFormat::Webp] {
        let err = ImageTransformer::transform(
            &lopsided_source(),
            &TransformParams {
                width: Some(100),
                format: Some(format),
                quality: Some(50),
                ..TransformParams::default()
            },
        )
        .unwrap_err();
        assert!(format!("{err}").contains("losslessly"), "{format:?}: {err}");
    }
    assert!(!OutputFormat::Png.honours_quality());
    assert!(!OutputFormat::Webp.honours_quality());
    assert!(OutputFormat::Jpeg.honours_quality());
    assert!(OutputFormat::Avif.honours_quality());
}

/// A box-filling mode with only one axis has no box. Deriving the missing side
/// would silently render something the caller did not ask for.
#[test]
fn a_box_filling_mode_needs_both_axes() {
    for mode in [ResizeMode::Fill, ResizeMode::Fit, ResizeMode::Stretch] {
        let err = ImageTransformer::transform(
            &lopsided_source(),
            &TransformParams {
                width: Some(300),
                format: Some(OutputFormat::Png),
                resize_mode: Some(mode),
                ..TransformParams::default()
            },
        )
        .unwrap_err();
        assert!(format!("{err}").contains("needs both w and h"), "{}: {err}", mode.as_str());
    }
    // `contain` derives it, because its output is the scaled source either way.
    assert!(
        ImageTransformer::transform(
            &lopsided_source(),
            &TransformParams {
                width: Some(300),
                format: Some(OutputFormat::Png),
                resize_mode: Some(ResizeMode::Contain),
                ..TransformParams::default()
            }
        )
        .is_ok()
    );
}

/// Mode and gravity names round-trip, and an unknown one is `None` — the caller
/// turns that into a `400` rather than falling back to a default.
#[test]
fn mode_and_gravity_names_round_trip() {
    for mode in [
        ResizeMode::Contain,
        ResizeMode::Stretch,
        ResizeMode::Fit,
        ResizeMode::Fill,
        ResizeMode::CoverBlur,
        ResizeMode::CoverMirror,
    ] {
        assert_eq!(ResizeMode::parse(mode.as_str()), Some(mode));
    }
    assert_eq!(ResizeMode::parse("fil"), None);
    assert_eq!(ResizeMode::parse(""), None);

    for gravity in [
        Gravity::Center,
        Gravity::North,
        Gravity::South,
        Gravity::East,
        Gravity::West,
        Gravity::NorthWest,
        Gravity::NorthEast,
        Gravity::SouthWest,
        Gravity::SouthEast,
        Gravity::Smart,
    ] {
        assert_eq!(Gravity::parse(gravity.as_str()), Some(gravity));
    }
    assert_eq!(Gravity::parse("northwest"), None);
}

/// Background colours parse in both hex shapes and refuse everything else —
/// there is no named-colour table, because a misspelt name rendering black is
/// exactly the surprise this refuses.
#[test]
fn background_colours_parse_or_refuse() {
    use crate::transforms::parse_colour;

    assert_eq!(parse_colour("#ff0000").unwrap().0, [255, 0, 0, 255]);
    assert_eq!(parse_colour("00ff0080").unwrap().0, [0, 255, 0, 128]);
    for bad in ["", "#f00", "red", "#gggggg", "#ff00000"] {
        assert!(parse_colour(bad).is_err(), "{bad:?} must be refused");
    }
}

/// A `TrueType` font to rasterise with.
///
/// `FRAISEQL_TEST_FONT` names one explicitly (the CI storage leg sets it after
/// installing `fonts-dejavu-core`); otherwise the first font under
/// `/usr/share/fonts` is used. Absent both, this **fails** rather than skips:
/// a text watermark that is never rasterised in any run is a feature that only
/// looks shipped.
fn test_font_bytes() -> Vec<u8> {
    if let Ok(path) = std::env::var("FRAISEQL_TEST_FONT") {
        return std::fs::read(&path).expect("FRAISEQL_TEST_FONT names an unreadable file");
    }
    let found = walkdir::WalkDir::new("/usr/share/fonts")
        .into_iter()
        .filter_map(Result::ok)
        .find(|e| {
            e.file_type().is_file()
                && e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("ttf"))
        });
    let entry = found.expect(
        "no TrueType font found under /usr/share/fonts; set FRAISEQL_TEST_FONT to one. The text \
         watermark cannot be proven to rasterise without a real font.",
    );
    std::fs::read(entry.path()).expect("read the discovered font")
}

/// #973: a text watermark rasterises through the operator's font and reaches
/// the output.
#[test]
fn a_text_watermark_rasterises_and_composites() {
    use crate::transforms::text::{parse_font, render_text};

    let font = parse_font(test_font_bytes()).expect("the discovered font must parse");
    let mark = render_text(&font, "DRAFT", 64.0, image::Rgba([255, 0, 0, 255]))
        .expect("text must rasterise");
    assert!(mark.width() > 0 && mark.height() > 0);

    let base = TransformParams {
        width: Some(400),
        height: Some(320),
        format: Some(OutputFormat::Png),
        ..TransformParams::default()
    };
    let plain = ImageTransformer::transform(&lopsided_source(), &base).unwrap();
    let marked = ImageTransformer::transform(
        &lopsided_source(),
        &TransformParams {
            watermark: Some(Watermark {
                image:   mark,
                gravity: Gravity::SouthEast,
                opacity: 255,
                scale:   0.5,
                margin:  8,
                source:  "text:DRAFT".to_string(),
            }),
            ..base
        },
    )
    .unwrap();
    assert_ne!(plain.body, marked.body, "the rasterised text must reach the output");
}

/// #973: the text rasteriser is bounded before it allocates, like every other
/// operation — the raster's dimensions come from these numbers.
#[test]
fn text_watermarks_are_bounded_before_they_allocate() {
    use crate::transforms::text::{MAX_TEXT_CHARS, MAX_TEXT_PX, parse_font, render_text};

    let font = parse_font(test_font_bytes()).unwrap();
    let white = image::Rgba([255, 255, 255, 255]);

    assert!(render_text(&font, "ok", 32.0, white).is_ok());
    assert!(
        render_text(&font, "ok", MAX_TEXT_PX + 1.0, white).is_err(),
        "type size is capped"
    );
    assert!(render_text(&font, "ok", 0.0, white).is_err());
    assert!(render_text(&font, "ok", -4.0, white).is_err());
    let long = "x".repeat(MAX_TEXT_CHARS + 1);
    assert!(render_text(&font, &long, 16.0, white).is_err(), "text length is capped");
    assert!(render_text(&font, "   ", 32.0, white).is_err(), "an empty raster is a refusal");

    // A non-font is refused rather than producing an empty typeface.
    assert!(parse_font(b"not a font".to_vec()).is_err());
}

/// #973's opt-in tier: seam carving retargets to the requested box, and past
/// the aspect-delta threshold it renders as `fill` instead — a different but
/// valid rendering, never an error and never a partially-carved image.
#[cfg(feature = "transforms-retarget")]
#[test]
fn retarget_carves_within_the_threshold_and_falls_back_past_it() {
    let render = |mode: ResizeMode, w: u32, h: u32| {
        ImageTransformer::transform(
            &lopsided_source(),
            &TransformParams {
                width: Some(w),
                height: Some(h),
                format: Some(OutputFormat::Png),
                resize_mode: Some(mode),
                ..TransformParams::default()
            },
        )
        .unwrap()
    };

    // 1000×800 into 400×400: an aspect delta of 1.25, inside the threshold.
    let carved = render(ResizeMode::Retarget, 400, 400);
    assert_eq!((carved.width, carved.height), (400, 400));
    assert_ne!(
        carved.body,
        render(ResizeMode::Fill, 400, 400).body,
        "inside the threshold, retarget must actually carve rather than crop"
    );

    // 1000×800 into 1000×100: an aspect delta of 8, far past the threshold.
    let fell_back = render(ResizeMode::Retarget, 1000, 100);
    assert_eq!((fell_back.width, fell_back.height), (1000, 100));
    assert_eq!(
        fell_back.body,
        render(ResizeMode::Fill, 1000, 100).body,
        "past the threshold, retarget must render exactly as fill"
    );
}
