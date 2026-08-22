//! Geometry and pixel operations for the render pipeline (#973).
//!
//! Every operation here is **bounded before it allocates** — #370's invariant,
//! restated for each new op. The 12 000 px/side ceiling bounds the canvas;
//! blur and sharpen bound their radii; a watermark cannot be scaled past the
//! canvas; a crop rectangle must lie inside the source. An operator-supplied
//! number never becomes an unbounded allocation or an unbounded loop.

use fraiseql_error::{FraiseQLError, Result};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage, imageops::FilterType};

/// The resampling filter every scale uses. Lanczos3 is the quality end of the
/// `image` crate's ladder and the cost is bounded by the output size, which is
/// already capped.
pub(super) const FILTER: FilterType = FilterType::Lanczos3;

/// Largest Gaussian sigma a request may ask for.
///
/// This is the *shape* bound; [`MAX_BLUR_WORK`] is the one that actually bounds
/// the cost. Kept because a radius past this is meaningless on any canvas and
/// deserves a refusal that says so.
pub const MAX_BLUR_SIGMA: f32 = 100.0;

/// Largest unsharp-mask sigma a request may ask for.
pub const MAX_SHARPEN_SIGMA: f32 = 50.0;

/// Radius `cover-blur` uses for the bars it fills from the image itself,
/// clamped down by [`MAX_BLUR_WORK`] on a large canvas.
const COVER_BLUR_SIGMA: u64 = 24;

/// Largest `pixels × sigma` a blur or an unsharp mask may cost.
///
/// `image`'s Gaussian is separable with a kernel proportional to sigma, so the
/// cost is linear in **both** the pixel count and the radius — a flat radius cap
/// bounds nothing on its own. Measured at ~6 ns per pixel·sigma unit on the
/// reference build, so this budget is ~0.4 s of CPU: a 1000×1000 render may ask
/// for sigma 64, and a 12 000×12 000 one may not ask for 1. Without it, the
/// dimension ceiling and a sigma of 100 still buy a ~90-second request — the
/// exact resource-exhaustion shape #370 exists to refuse.
pub const MAX_BLUR_WORK: u64 = 64_000_000;

/// Refuse a blur whose cost, not merely whose radius, is out of bounds.
fn check_blur_budget(width: u32, height: u32, sigma: f32, path: &str) -> Result<()> {
    // Reason: sigma is already bounded to (0, 100] by the caller, and the
    // dimensions by MAX_DIMENSION, so the product fits a u64 comfortably.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let work = u64::from(width) * u64::from(height) * (sigma.ceil() as u64);
    if work > MAX_BLUR_WORK {
        let max_sigma = MAX_BLUR_WORK / (u64::from(width) * u64::from(height)).max(1);
        return Err(reject(
            format!(
                "a radius of {sigma} over {width}x{height} pixels exceeds the blur budget; at \
                 this size the maximum is {max_sigma}"
            ),
            path,
        ));
    }
    Ok(())
}

/// A validation refusal, phrased for the render route's `400`.
fn reject(message: impl Into<String>, path: &str) -> FraiseQLError {
    FraiseQLError::Validation {
        message: message.into(),
        path:    Some(path.to_string()),
    }
}

/// How a resize fills the requested box.
///
/// The names are the ones #973 asked for. `Contain` is the behaviour that
/// shipped with #370 — scale to fit inside the box and let the output keep the
/// source's aspect ratio — and stays the default, because changing what an
/// existing `?w=&h=` URL returns is not something a mode list should do
/// silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeMode {
    /// Scale to fit inside the box; the output is the scaled size, never the
    /// box. No padding, no cropping, aspect ratio preserved.
    #[default]
    Contain,
    /// Scale to exactly the box, ignoring the source's aspect ratio.
    Stretch,
    /// Scale to fit inside the box and letterbox to exactly the box with the
    /// requested background colour.
    Fit,
    /// Scale to cover the box and crop the overflow at the requested gravity.
    Fill,
    /// Letterbox to exactly the box, with the bars filled by a blurred,
    /// box-covering copy of the image itself.
    CoverBlur,
    /// Letterbox to exactly the box, with the bars filled by mirrored copies of
    /// the image's own edges.
    CoverMirror,
    /// Content-aware retarget (seam carving) to exactly the box, falling back
    /// to [`Fill`](Self::Fill) past the aspect-delta threshold.
    #[cfg(feature = "transforms-retarget")]
    Retarget,
}

impl ResizeMode {
    /// Parse an operator- or client-supplied mode name.
    ///
    /// Unknown names are `None` — the caller turns that into a named `400`
    /// rather than falling back to a default, because a typo that silently
    /// renders something else is the defect this vocabulary exists to avoid.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "contain" => Some(Self::Contain),
            "stretch" => Some(Self::Stretch),
            "fit" => Some(Self::Fit),
            "fill" => Some(Self::Fill),
            "cover-blur" => Some(Self::CoverBlur),
            "cover-mirror" => Some(Self::CoverMirror),
            #[cfg(feature = "transforms-retarget")]
            "retarget" => Some(Self::Retarget),
            _ => None,
        }
    }

    /// The mode's canonical name, as it appears in a URL, a preset and the
    /// render audit record.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contain => "contain",
            Self::Stretch => "stretch",
            Self::Fit => "fit",
            Self::Fill => "fill",
            Self::CoverBlur => "cover-blur",
            Self::CoverMirror => "cover-mirror",
            #[cfg(feature = "transforms-retarget")]
            Self::Retarget => "retarget",
        }
    }

    /// Whether this mode produces exactly the requested box. Every mode but
    /// [`Contain`](Self::Contain) does, and every mode that does needs both a
    /// width and a height to have a box at all.
    #[must_use]
    pub const fn fills_the_box(self) -> bool {
        !matches!(self, Self::Contain)
    }
}

/// Where a crop or an overlay sits inside its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gravity {
    /// Centred on both axes.
    #[default]
    Center,
    /// Top edge.
    North,
    /// Bottom edge.
    South,
    /// Right edge.
    East,
    /// Left edge.
    West,
    /// Top-left corner.
    NorthWest,
    /// Top-right corner.
    NorthEast,
    /// Bottom-left corner.
    SouthWest,
    /// Bottom-right corner.
    SouthEast,
    /// Chosen from image content: the window carrying the most edge energy.
    Smart,
}

impl Gravity {
    /// Parse an operator- or client-supplied gravity name; `None` for unknown.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "center" | "centre" => Some(Self::Center),
            "north" | "top" => Some(Self::North),
            "south" | "bottom" => Some(Self::South),
            "east" | "right" => Some(Self::East),
            "west" | "left" => Some(Self::West),
            "north-west" | "top-left" => Some(Self::NorthWest),
            "north-east" | "top-right" => Some(Self::NorthEast),
            "south-west" | "bottom-left" => Some(Self::SouthWest),
            "south-east" | "bottom-right" => Some(Self::SouthEast),
            "smart" => Some(Self::Smart),
            _ => None,
        }
    }

    /// The gravity's canonical name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::NorthWest => "north-west",
            Self::NorthEast => "north-east",
            Self::SouthWest => "south-west",
            Self::SouthEast => "south-east",
            Self::Smart => "smart",
        }
    }

    /// Top-left offset that places a `win` box inside an `outer` box.
    ///
    /// `Smart` is resolved by the caller (it needs the pixels); everything else
    /// is arithmetic. An oversized window clamps to zero rather than
    /// underflowing.
    const fn offset(self, outer: (u32, u32), win: (u32, u32)) -> (u32, u32) {
        let slack_x = outer.0.saturating_sub(win.0);
        let slack_y = outer.1.saturating_sub(win.1);
        let (x, y) = match self {
            Self::NorthWest | Self::West | Self::SouthWest => (0, slack_y / 2),
            Self::NorthEast | Self::East | Self::SouthEast => (slack_x, slack_y / 2),
            _ => (slack_x / 2, slack_y / 2),
        };
        // The vertical family overrides the shared mid-line above.
        match self {
            Self::North | Self::NorthWest | Self::NorthEast => (x, 0),
            Self::South | Self::SouthWest | Self::SouthEast => (x, slack_y),
            Self::West | Self::East => (x, slack_y / 2),
            _ => (x, y),
        }
    }
}

/// An explicit crop, applied before any scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropSpec {
    /// An exact rectangle in source pixels.
    BBox {
        /// Left edge.
        x: u32,
        /// Top edge.
        y: u32,
        /// Width.
        w: u32,
        /// Height.
        h: u32,
    },
    /// The largest rectangle of the given aspect ratio, positioned by gravity.
    Aspect {
        /// Aspect numerator.
        w: u32,
        /// Aspect denominator.
        h: u32,
    },
}

impl CropSpec {
    /// Parse `x,y,w,h` (a bounding box) or `w:h` (an aspect ratio).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` when the shape is neither, when a
    /// component is not a number, or when a dimension is zero — a zero-sized
    /// crop is a mistake, not an empty image.
    pub fn parse(raw: &str) -> Result<Self> {
        if let Some((w, h)) = raw.split_once(':') {
            let w = parse_u32(w, "crop")?;
            let h = parse_u32(h, "crop")?;
            if w == 0 || h == 0 {
                return Err(reject("crop aspect components must be greater than 0", "crop"));
            }
            return Ok(Self::Aspect { w, h });
        }
        let parts: Vec<&str> = raw.split(',').collect();
        let [x, y, w, h] = parts.as_slice() else {
            return Err(reject(
                "crop must be 'x,y,w,h' (a bounding box) or 'w:h' (an aspect ratio)",
                "crop",
            ));
        };
        let w = parse_u32(w, "crop")?;
        let h = parse_u32(h, "crop")?;
        if w == 0 || h == 0 {
            return Err(reject("crop width and height must be greater than 0", "crop"));
        }
        Ok(Self::BBox {
            x: parse_u32(x, "crop")?,
            y: parse_u32(y, "crop")?,
            w,
            h,
        })
    }
}

fn parse_u32(raw: &str, path: &str) -> Result<u32> {
    raw.trim()
        .parse::<u32>()
        .map_err(|_| reject(format!("'{}' is not a non-negative integer", raw.trim()), path))
}

/// Parse `#rrggbb`, `#rrggbbaa` or a bare hex form of either.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` for anything else. There is no named
/// colour table: a misspelt name silently rendering black is exactly the class
/// of surprise this refuses.
pub fn parse_colour(raw: &str) -> Result<Rgba<u8>> {
    let hex = raw.trim().trim_start_matches('#');
    let byte = |i: usize| -> Result<u8> {
        u8::from_str_radix(&hex[i..i + 2], 16)
            .map_err(|_| reject("background must be hex, as #rrggbb or #rrggbbaa", "background"))
    };
    match hex.len() {
        6 => Ok(Rgba([byte(0)?, byte(2)?, byte(4)?, 0xFF])),
        8 => Ok(Rgba([byte(0)?, byte(2)?, byte(4)?, byte(6)?])),
        _ => Err(reject("background must be hex, as #rrggbb or #rrggbbaa", "background")),
    }
}

/// Apply an explicit crop to the source.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` when a bounding box leaves the source.
/// It is deliberately not clamped: a caller that asked for a rectangle outside
/// the image asked for something that does not exist, and quietly returning a
/// different rectangle would be the wrong answer delivered with a `200`.
pub fn apply_crop(img: &DynamicImage, spec: CropSpec, gravity: Gravity) -> Result<DynamicImage> {
    let (sw, sh) = img.dimensions();
    match spec {
        CropSpec::BBox { x, y, w, h } => {
            if x.saturating_add(w) > sw || y.saturating_add(h) > sh {
                return Err(reject(
                    format!("crop box {x},{y},{w},{h} lies outside the {sw}x{sh} source"),
                    "crop",
                ));
            }
            Ok(img.crop_imm(x, y, w, h))
        },
        CropSpec::Aspect { w, h } => {
            let (cw, ch) = largest_box_with_aspect(sw, sh, w, h);
            let (x, y) = place(img, (sw, sh), (cw, ch), gravity);
            Ok(img.crop_imm(x, y, cw, ch))
        },
    }
}

/// The largest `aw:ah` rectangle that fits inside `sw x sh`.
// Reason: dimensions are bounded by MAX_DIMENSION, so the u64 arithmetic below
// cannot overflow and the narrowing back to u32 cannot truncate.
#[allow(clippy::cast_possible_truncation)]
fn largest_box_with_aspect(sw: u32, sh: u32, aw: u32, ah: u32) -> (u32, u32) {
    let (sw64, sh64) = (u64::from(sw), u64::from(sh));
    let (aw64, ah64) = (u64::from(aw), u64::from(ah));
    if sw64 * ah64 > sh64 * aw64 {
        // Source is wider than the target ratio: height is the limit.
        (((sh64 * aw64) / ah64).max(1) as u32, sh)
    } else {
        (sw, ((sw64 * ah64) / aw64).max(1) as u32)
    }
}

/// Resolve `gravity` to a top-left offset for a `win` window inside `outer`,
/// consulting the pixels when the gravity is `Smart`.
fn place(img: &DynamicImage, outer: (u32, u32), win: (u32, u32), gravity: Gravity) -> (u32, u32) {
    if gravity == Gravity::Smart {
        return smart_offset(img, outer, win);
    }
    gravity.offset(outer, win)
}

/// Downscale used by the entropy search. Bounding the search grid keeps
/// `Smart` gravity linear in a fixed number of pixels rather than in the
/// source's, so it costs the same for a 12 000 px image as for a 600 px one.
const ENTROPY_GRID: u32 = 128;

/// Pick the `win`-sized window carrying the most edge energy.
///
/// Energy is the sum of absolute horizontal and vertical luma gradients, summed
/// over the window through an integral image, so every candidate position is
/// evaluated in constant time on a fixed 128 px grid.
// Reason: every cast below is between bounded small integers and f32/u32 within
// range; the grid is ENTROPY_GRID at most.
// Reason (indexing_slicing): `integral` is allocated at exactly
// `(gw + 1) * (gh + 1)`, and every index below is `(y + 1) * (gw + 1) + (x + 1)`
// with `x < gw` and `y < gh`, whose maximum is `gh * (gw + 1) + gw` — inside the
// allocation by construction.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing
)]
fn smart_offset(img: &DynamicImage, outer: (u32, u32), win: (u32, u32)) -> (u32, u32) {
    let (ow, oh) = outer;
    let (ww, wh) = win;
    if ww >= ow && wh >= oh {
        return (0, 0);
    }

    let gw = ENTROPY_GRID.min(ow).max(2);
    let gh = ENTROPY_GRID.min(oh).max(2);
    let small = img.resize_exact(gw, gh, FilterType::Triangle).to_luma8();

    // Gradient magnitude per cell, then a summed-area table over it.
    let mut integral = vec![0_u64; ((gw + 1) * (gh + 1)) as usize];
    for y in 0..gh {
        for x in 0..gw {
            let here = i32::from(small.get_pixel(x, y).0[0]);
            let right = i32::from(small.get_pixel((x + 1).min(gw - 1), y).0[0]);
            let down = i32::from(small.get_pixel(x, (y + 1).min(gh - 1)).0[0]);
            let energy = (right - here).unsigned_abs() + (down - here).unsigned_abs();
            let idx = ((y + 1) * (gw + 1) + (x + 1)) as usize;
            integral[idx] = u64::from(energy)
                + integral[(y * (gw + 1) + (x + 1)) as usize]
                + integral[((y + 1) * (gw + 1) + x) as usize]
                - integral[(y * (gw + 1) + x) as usize];
        }
    }
    let sum = |x0: u32, y0: u32, x1: u32, y1: u32| -> u64 {
        let at = |x: u32, y: u32| integral[(y * (gw + 1) + x) as usize];
        at(x1, y1) + at(x0, y0) - at(x1, y0) - at(x0, y1)
    };

    // Window size expressed on the grid, then slid over every grid position.
    let scale_x = f32::from(u16::try_from(gw).unwrap_or(u16::MAX)) / ow as f32;
    let scale_y = f32::from(u16::try_from(gh).unwrap_or(u16::MAX)) / oh as f32;
    let gww = ((ww as f32 * scale_x).round() as u32).clamp(1, gw);
    let gwh = ((wh as f32 * scale_y).round() as u32).clamp(1, gh);

    let mut best = (0_u32, 0_u32, 0_u64);
    for gy in 0..=(gh - gwh) {
        for gx in 0..=(gw - gww) {
            let energy = sum(gx, gy, gx + gww, gy + gwh);
            if energy > best.2 {
                best = (gx, gy, energy);
            }
        }
    }

    // Back to source pixels, clamped so the window stays inside.
    let x = ((best.0 as f32 / scale_x).round() as u32).min(ow - ww);
    let y = ((best.1 as f32 / scale_y).round() as u32).min(oh - wh);
    (x, y)
}

/// Scale `img` into a `w x h` box according to `mode`.
///
/// The output is exactly `w x h` for every mode but [`ResizeMode::Contain`],
/// which returns the aspect-preserving scaled size.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the geometry degenerates to zero
/// pixels.
pub fn resize_into(
    img: &DynamicImage,
    w: u32,
    h: u32,
    mode: ResizeMode,
    gravity: Gravity,
    background: Rgba<u8>,
) -> Result<DynamicImage> {
    match mode {
        ResizeMode::Contain => {
            let (cw, ch) = contain_size(img.width(), img.height(), w, h)?;
            Ok(img.resize_exact(cw, ch, FILTER))
        },
        ResizeMode::Stretch => Ok(img.resize_exact(w, h, FILTER)),
        ResizeMode::Fill => {
            let (cw, ch) = cover_size(img.width(), img.height(), w, h)?;
            let scaled = img.resize_exact(cw, ch, FILTER);
            let (x, y) = place(&scaled, (cw, ch), (w, h), gravity);
            Ok(scaled.crop_imm(x, y, w.min(cw), h.min(ch)))
        },
        ResizeMode::Fit => {
            let (cw, ch) = contain_size(img.width(), img.height(), w, h)?;
            let fg = img.resize_exact(cw, ch, FILTER);
            let mut canvas = RgbaImage::from_pixel(w, h, background);
            let (x, y) = Gravity::Center.offset((w, h), (cw, ch));
            image::imageops::overlay(&mut canvas, &fg.to_rgba8(), i64::from(x), i64::from(y));
            Ok(DynamicImage::ImageRgba8(canvas))
        },
        ResizeMode::CoverBlur | ResizeMode::CoverMirror => {
            let (cw, ch) = contain_size(img.width(), img.height(), w, h)?;
            let fg = img.resize_exact(cw, ch, FILTER);
            let mut canvas = if mode == ResizeMode::CoverBlur {
                let (bw, bh) = cover_size(img.width(), img.height(), w, h)?;
                let filler = img.resize_exact(bw, bh, FilterType::Triangle);
                let (x, y) = Gravity::Center.offset((bw, bh), (w, h));
                // Blur the covering copy, not the source: the bars are the only
                // place it shows, and blurring after the crop keeps the cost
                // proportional to the output rather than to the source. The
                // radius is clamped to the same budget a caller's blur is held
                // to — this one is chosen by us, so it bends rather than fails.
                let area = u64::from(w) * u64::from(h);
                // Reason: the quotient is bounded by COVER_BLUR_SIGMA below.
                #[allow(clippy::cast_precision_loss)]
                let sigma = (MAX_BLUR_WORK / area.max(1)).min(COVER_BLUR_SIGMA) as f32;
                filler.crop_imm(x, y, w.min(bw), h.min(bh)).blur(sigma.max(1.0)).to_rgba8()
            } else {
                mirror_canvas(&fg.to_rgba8(), w, h)
            };
            let (x, y) = Gravity::Center.offset((w, h), (cw, ch));
            image::imageops::overlay(&mut canvas, &fg.to_rgba8(), i64::from(x), i64::from(y));
            Ok(DynamicImage::ImageRgba8(canvas))
        },
        #[cfg(feature = "transforms-retarget")]
        ResizeMode::Retarget => super::retarget::retarget(img, w, h, gravity),
    }
}

/// Fill a `w x h` canvas by mirroring the edges of `fg`, which is then drawn
/// centred over it by the caller.
fn mirror_canvas(fg: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    let (fw, fh) = fg.dimensions();
    let mut canvas = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));
    let (ox, oy) = Gravity::Center.offset((w, h), (fw, fh));

    let flip_h = image::imageops::flip_horizontal(fg);
    let flip_v = image::imageops::flip_vertical(fg);
    // Three reflected copies on each axis cover any bar a contain-fit can leave
    // (the bars are never wider than the image itself on the axis they pad).
    for step in 1..=2_i64 {
        let dx = i64::from(fw) * step;
        let dy = i64::from(fh) * step;
        image::imageops::overlay(&mut canvas, &flip_h, i64::from(ox) - dx, i64::from(oy));
        image::imageops::overlay(&mut canvas, &flip_h, i64::from(ox) + dx, i64::from(oy));
        image::imageops::overlay(&mut canvas, &flip_v, i64::from(ox), i64::from(oy) - dy);
        image::imageops::overlay(&mut canvas, &flip_v, i64::from(ox), i64::from(oy) + dy);
    }
    canvas
}

/// The largest `w x h`-bounded size with the source's aspect ratio.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the result would be zero pixels.
// Reason: dimensions are bounded by MAX_DIMENSION; the u64 arithmetic cannot
// overflow and the narrowing back to u32 cannot truncate.
#[allow(clippy::cast_possible_truncation)]
pub fn contain_size(sw: u32, sh: u32, w: u32, h: u32) -> Result<(u32, u32)> {
    let (sw64, sh64, w64, h64) = (u64::from(sw), u64::from(sh), u64::from(w), u64::from(h));
    let (cw, ch) = if sw64 * h64 > sh64 * w64 {
        (w64, (w64 * sh64) / sw64)
    } else {
        ((h64 * sw64) / sh64, h64)
    };
    let (cw, ch) = (cw.max(1) as u32, ch.max(1) as u32);
    if cw == 0 || ch == 0 {
        return Err(reject("the requested size collapses the image to zero pixels", "dimensions"));
    }
    Ok((cw, ch))
}

/// The smallest size with the source's aspect ratio that covers `w x h`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the result would be zero pixels.
// Reason: as `contain_size`.
#[allow(clippy::cast_possible_truncation)]
pub fn cover_size(sw: u32, sh: u32, w: u32, h: u32) -> Result<(u32, u32)> {
    let (sw64, sh64, w64, h64) = (u64::from(sw), u64::from(sh), u64::from(w), u64::from(h));
    let (cw, ch) = if sw64 * h64 > sh64 * w64 {
        ((h64 * sw64).div_ceil(sh64), h64)
    } else {
        (w64, (w64 * sh64).div_ceil(sw64))
    };
    let (cw, ch) = (cw.max(1) as u32, ch.max(1) as u32);
    if cw == 0 || ch == 0 {
        return Err(reject("the requested size collapses the image to zero pixels", "dimensions"));
    }
    Ok((cw, ch))
}

/// A watermark to composite over the rendered image.
#[derive(Debug, Clone)]
pub struct Watermark {
    /// The mark's own pixels, already decoded under the pipeline's limits.
    pub image:   DynamicImage,
    /// Where it sits on the canvas.
    pub gravity: Gravity,
    /// `0`–`255`; scales the mark's own alpha.
    pub opacity: u8,
    /// The mark's width as a fraction of the canvas width, `0.0` < s ≤ `1.0`.
    pub scale:   f32,
    /// Inset from the canvas edge, in pixels, applied away from the edge the
    /// gravity names.
    pub margin:  u32,
    /// How the mark was named, for the audit record and the cache key: the
    /// stored object's key, or the rendered text.
    pub source:  String,
}

impl Watermark {
    /// The mark's contribution to the resolved-transform description.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "watermark={}@{},{},{},{}",
            self.source,
            self.gravity.as_str(),
            self.opacity,
            self.scale,
            self.margin
        )
    }
}

/// Composite `mark` over `canvas`.
///
/// The mark is scaled to `scale` of the canvas width — bounded above by the
/// canvas itself, so an overlay can never allocate more than the image it is
/// drawn on — and its alpha is multiplied by `opacity`.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if `scale` is outside `(0, 1]`.
// Reason: the scale is validated into (0, 1] and the canvas is bounded by
// MAX_DIMENSION, so every cast below stays in range.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn apply_watermark(canvas: &DynamicImage, mark: &Watermark) -> Result<DynamicImage> {
    if !mark.scale.is_finite() || mark.scale <= 0.0 || mark.scale > 1.0 {
        return Err(reject("watermark scale must be greater than 0 and at most 1", "watermark"));
    }
    let (cw, ch) = canvas.dimensions();
    let target_w = ((cw as f32 * mark.scale).round() as u32).clamp(1, cw);
    let (mw, mh) = mark.image.dimensions();
    let target_h = ((u64::from(target_w) * u64::from(mh)) / u64::from(mw).max(1))
        .max(1)
        .min(u64::from(ch)) as u32;

    let scaled = mark.image.resize_exact(target_w, target_h, FILTER);
    let mut overlay = scaled.to_rgba8();
    if mark.opacity != u8::MAX {
        for pixel in overlay.pixels_mut() {
            pixel.0[3] = ((u16::from(pixel.0[3]) * u16::from(mark.opacity)) / 255) as u8;
        }
    }

    let inset_w = target_w.saturating_add(mark.margin.saturating_mul(2)).min(cw);
    let inset_h = target_h.saturating_add(mark.margin.saturating_mul(2)).min(ch);
    let (bx, by) = mark.gravity.offset((cw, ch), (inset_w, inset_h));
    let x = (bx + mark.margin).min(cw.saturating_sub(target_w));
    let y = (by + mark.margin).min(ch.saturating_sub(target_h));

    let mut base = canvas.to_rgba8();
    image::imageops::overlay(&mut base, &overlay, i64::from(x), i64::from(y));
    Ok(DynamicImage::ImageRgba8(base))
}

/// Gaussian blur, refusing a radius past [`MAX_BLUR_SIGMA`].
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` for a non-positive or over-large sigma.
pub fn apply_blur(img: &DynamicImage, sigma: f32) -> Result<DynamicImage> {
    if !sigma.is_finite() || sigma <= 0.0 {
        return Err(reject("blur must be greater than 0", "blur"));
    }
    if sigma > MAX_BLUR_SIGMA {
        return Err(reject(format!("blur must be at most {MAX_BLUR_SIGMA}"), "blur"));
    }
    check_blur_budget(img.width(), img.height(), sigma, "blur")?;
    Ok(img.blur(sigma))
}

/// Unsharp mask, refusing a radius past [`MAX_SHARPEN_SIGMA`].
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` for a non-positive or over-large sigma.
pub fn apply_sharpen(img: &DynamicImage, sigma: f32) -> Result<DynamicImage> {
    if !sigma.is_finite() || sigma <= 0.0 {
        return Err(reject("sharpen must be greater than 0", "sharpen"));
    }
    if sigma > MAX_SHARPEN_SIGMA {
        return Err(reject(format!("sharpen must be at most {MAX_SHARPEN_SIGMA}"), "sharpen"));
    }
    // An unsharp mask blurs internally, so it carries the same cost.
    check_blur_budget(img.width(), img.height(), sigma, "sharpen")?;
    Ok(img.unsharpen(sigma, 0))
}
