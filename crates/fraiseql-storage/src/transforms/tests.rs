#![cfg(all(test, feature = "transforms"))]

use super::*;
use image::{ImageBuffer, RgbImage, RgbaImage, Rgba, ImageFormat};

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
    use std::io::Cursor;
    dyn_img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Jpeg)
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
    use std::io::Cursor;
    dyn_img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
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
    };

    let params_high_quality = TransformParams {
        width: Some(500),
        height: None,
        format: Some(OutputFormat::Jpeg),
        quality: Some(95),
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
        },
        TransformPreset {
            name: "medium".to_string(),
            width: Some(800),
            height: Some(600),
            format: Some("jpeg".to_string()),
            quality: Some(85),
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
        },
        TransformPreset {
            name: "jpg".to_string(),
            width: None,
            height: None,
            format: Some("jpg".to_string()),
            quality: None,
        },
        TransformPreset {
            name: "avif".to_string(),
            width: None,
            height: None,
            format: Some("avif".to_string()),
            quality: None,
        },
    ];

    let png_params = ImageTransformer::apply_preset("png", Some(&presets)).unwrap();
    assert_eq!(png_params.format, Some(OutputFormat::Png));

    let jpg_params = ImageTransformer::apply_preset("jpg", Some(&presets)).unwrap();
    assert_eq!(jpg_params.format, Some(OutputFormat::Jpeg));

    let avif_params = ImageTransformer::apply_preset("avif", Some(&presets)).unwrap();
    assert_eq!(avif_params.format, Some(OutputFormat::Avif));
}
