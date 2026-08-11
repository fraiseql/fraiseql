//! Text rasterisation for text watermarks (#973).
//!
//! The `image` crate draws no text and ships no font, so a text watermark needs
//! both a rasteriser and a typeface. The rasteriser is `ab_glyph`; the typeface
//! is **the operator's**, named by `watermark_font` on the bucket and read at
//! boot, so a missing or unparseable font is a refusal to start rather than a
//! render-time surprise — and so a published crate carries no vendored font and
//! no font licence.

use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use fraiseql_error::{FraiseQLError, Result};
use image::{DynamicImage, Rgba, RgbaImage};

/// Largest type size a request may ask for.
///
/// The rendered mark is bounded again by the canvas when it is composited, but
/// the raster itself is allocated from this number, so it is capped before
/// anything is drawn.
pub const MAX_TEXT_PX: f32 = 512.0;

/// Longest watermark string.
///
/// A watermark is a mark, not a document; the cap bounds the raster's width
/// independently of the type size.
pub const MAX_TEXT_CHARS: usize = 256;

fn reject(message: impl Into<String>) -> FraiseQLError {
    FraiseQLError::Validation {
        message: message.into(),
        path:    Some("watermark_text".to_string()),
    }
}

/// Parse an operator-supplied font file.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` when the bytes are not a font
/// `ab_glyph` can read.
pub fn parse_font(bytes: Vec<u8>) -> Result<FontVec> {
    FontVec::try_from_vec(bytes).map_err(|e| FraiseQLError::Validation {
        message: format!("watermark_font is not a readable font file: {e}"),
        path:    Some("watermark_font".to_string()),
    })
}

/// Rasterise `text` at `px` into a tight RGBA image.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the size or the string is past its
/// bound, or if the text rasterises to nothing (an all-whitespace mark).
pub fn render_text(font: &FontVec, text: &str, px: f32, colour: Rgba<u8>) -> Result<DynamicImage> {
    if !px.is_finite() || px <= 0.0 || px > MAX_TEXT_PX {
        return Err(reject(format!("watermark text size must be between 0 and {MAX_TEXT_PX}")));
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(reject(format!("watermark text must be at most {MAX_TEXT_CHARS} characters")));
    }

    let scaled = font.as_scaled(PxScale::from(px));
    // Lay the glyphs out on one baseline, accumulating advances and kerning.
    let mut caret = 0.0_f32;
    let mut glyphs = Vec::new();
    let mut previous = None;
    for ch in text.chars() {
        let id = font.glyph_id(ch);
        if let Some(prev) = previous {
            caret += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(px, ab_glyph::point(caret, scaled.ascent()));
        caret += scaled.h_advance(id);
        previous = Some(id);
        glyphs.push(glyph);
    }

    // Reason: the width is bounded by MAX_TEXT_CHARS × MAX_TEXT_PX and the
    // height by MAX_TEXT_PX, so both fit a u32 with room to spare.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let width = (caret.ceil() as u32).max(1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let height = ((scaled.ascent() - scaled.descent()).ceil() as u32).max(1);
    let mut canvas = RgbaImage::new(width, height);

    for glyph in glyphs {
        let Some(outline) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outline.px_bounds();
        outline.draw(|gx, gy, coverage| {
            // Reason: bounds come from the layout above, which is bounded as
            // documented; the sum stays inside the canvas and is checked below.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let x = gx.saturating_add(bounds.min.x.max(0.0) as u32);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let y = gy.saturating_add(bounds.min.y.max(0.0) as u32);
            if x >= width || y >= height {
                return;
            }
            // Reason: `coverage` is `ab_glyph`'s 0.0..=1.0 alpha.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let alpha = (coverage.clamp(0.0, 1.0) * f32::from(colour.0[3])) as u8;
            if alpha > 0 {
                canvas.put_pixel(x, y, Rgba([colour.0[0], colour.0[1], colour.0[2], alpha]));
            }
        });
    }

    if canvas.pixels().all(|p| p.0[3] == 0) {
        return Err(reject("watermark text rasterised to nothing"));
    }
    Ok(DynamicImage::ImageRgba8(canvas))
}
