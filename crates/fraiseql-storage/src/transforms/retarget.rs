//! Content-aware retargeting by seam carving (#973), behind `transforms-retarget`.
//!
//! Seam carving is the one transform whose cost is not bounded by its output:
//! it is `O(pixels × seams)`, and both factors grow with the *difference*
//! between the source and the target shape. A size cap alone would not bound
//! it, so this module bounds it three ways, in order:
//!
//! 1. **An aspect-delta threshold.** Past it, carving stops being content-preserving and starts
//!    being a slow way to produce a distorted image, so the request falls back to `fill` — the mode
//!    it would have been a better answer to ask for.
//! 2. **A working-resolution cap.** Carving runs on a downscaled copy and the result is scaled to
//!    the requested size, so the seam count is bounded by a constant rather than by the source.
//! 3. **A wall-clock budget.** Whatever is left falls back to `fill` mid-flight rather than holding
//!    a request open.
//!
//! A fall-back is a *different but valid* rendering, never an error and never a
//! partially-carved image.

use std::time::{Duration, Instant};

use fraiseql_error::Result;
use image::{DynamicImage, GenericImageView, RgbaImage};

use super::ops::{FILTER, Gravity, ResizeMode, cover_size, resize_into};

/// Largest side the carving itself runs at. A 1 000 px working copy keeps the
/// worst case (carving away half of each axis) at a few hundred million pixel
/// visits rather than tens of billions.
const MAX_WORKING_SIDE: u32 = 1_000;

/// How far the aspect ratio may change before carving is the wrong tool.
/// A 2× change in the width-to-height ratio is already well past the point
/// where seams start cutting through subject matter.
const MAX_ASPECT_DELTA: f32 = 2.0;

/// Wall-clock budget for one retarget. Past it the request completes as `fill`.
const BUDGET: Duration = Duration::from_millis(750);

/// Retarget `img` to exactly `w x h`, falling back to `fill` when carving is
/// the wrong tool or runs out of budget.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` only from the geometry it shares with
/// the other resize modes; carving itself never fails, it falls back.
// Reason: dimensions are bounded by MAX_DIMENSION, so the f32 conversions and
// the narrowing casts below stay in range.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn retarget(img: &DynamicImage, w: u32, h: u32, gravity: Gravity) -> Result<DynamicImage> {
    let (sw, sh) = img.dimensions();
    let source_aspect = sw as f32 / sh as f32;
    let target_aspect = w as f32 / h as f32;
    let delta = (source_aspect / target_aspect).max(target_aspect / source_aspect);
    if delta > MAX_ASPECT_DELTA {
        tracing::debug!(
            delta,
            threshold = MAX_ASPECT_DELTA,
            "retarget: aspect delta past the threshold, rendering as fill"
        );
        return fill(img, w, h, gravity);
    }

    // Scale so one axis already matches and the other carries the excess the
    // seams will remove, capped to the working resolution.
    let (cw, ch) = cover_size(sw, sh, w, h)?;
    let cap = MAX_WORKING_SIDE.max(w).max(h);
    let (work_w, work_h) = if cw.max(ch) > cap {
        let shrink = f32::from(u16::try_from(cap).unwrap_or(u16::MAX)) / cw.max(ch) as f32;
        (
            ((cw as f32 * shrink).round() as u32).max(w.min(cw)),
            ((ch as f32 * shrink).round() as u32).max(h.min(ch)),
        )
    } else {
        (cw, ch)
    };
    let target_w = w.min(work_w);
    let target_h = h.min(work_h);

    let deadline = Instant::now() + BUDGET;
    let mut canvas = img.resize_exact(work_w, work_h, FILTER).to_rgba8();

    if !carve_columns(&mut canvas, work_w - target_w, deadline) {
        return fill(img, w, h, gravity);
    }
    let mut transposed = transpose(&canvas);
    if !carve_columns(&mut transposed, work_h - target_h, deadline) {
        return fill(img, w, h, gravity);
    }
    canvas = transpose(&transposed);

    Ok(DynamicImage::ImageRgba8(canvas).resize_exact(w, h, FILTER))
}

/// The `fill` rendering this mode falls back to.
fn fill(img: &DynamicImage, w: u32, h: u32, gravity: Gravity) -> Result<DynamicImage> {
    resize_into(img, w, h, ResizeMode::Fill, gravity, image::Rgba([0, 0, 0, 255]))
}

/// Remove `count` lowest-energy vertical seams in place.
///
/// Returns `false` when the wall-clock budget ran out, leaving the caller to
/// discard the partial result — a half-carved image is not an answer.
fn carve_columns(canvas: &mut RgbaImage, count: u32, deadline: Instant) -> bool {
    for _ in 0..count {
        if Instant::now() >= deadline {
            return false;
        }
        let seam = lowest_energy_seam(canvas);
        remove_seam(canvas, &seam);
    }
    true
}

/// Per-pixel energy: the absolute luma gradient against the right and lower
/// neighbours.
fn energy_at(canvas: &RgbaImage, x: u32, y: u32) -> u32 {
    let luma = |px: u32, py: u32| -> i32 {
        let p = canvas.get_pixel(px.min(canvas.width() - 1), py.min(canvas.height() - 1)).0;
        (i32::from(p[0]) * 299 + i32::from(p[1]) * 587 + i32::from(p[2]) * 114) / 1000
    };
    let here = luma(x, y);
    (luma(x + 1, y) - here).unsigned_abs() + (luma(x, y + 1) - here).unsigned_abs()
}

/// The x-coordinate of the lowest-energy seam, one entry per row.
// Reason (indexing_slicing): `cost` and `from` are allocated at exactly `w * h`,
// and every index is `y * w + x` with `x < w` and `y < h`; `seam` is allocated at
// `h` and indexed by `y < h`. All bounds come from the canvas itself.
#[allow(clippy::indexing_slicing)]
fn lowest_energy_seam(canvas: &RgbaImage) -> Vec<u32> {
    let (w, h) = canvas.dimensions();
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    let mut cost = vec![0_u32; (w * h) as usize];
    let mut from = vec![0_i8; (w * h) as usize];

    for x in 0..w {
        cost[idx(x, 0)] = energy_at(canvas, x, 0);
    }
    for y in 1..h {
        for x in 0..w {
            // The three predecessors a seam may come from, cheapest wins.
            let mut best = (cost[idx(x, y - 1)], 0_i8);
            if x > 0 {
                let left = (cost[idx(x - 1, y - 1)], -1_i8);
                if left.0 < best.0 {
                    best = left;
                }
            }
            if x + 1 < w {
                let right = (cost[idx(x + 1, y - 1)], 1_i8);
                if right.0 < best.0 {
                    best = right;
                }
            }
            let (best, step) = best;

            cost[idx(x, y)] = best.saturating_add(energy_at(canvas, x, y));
            from[idx(x, y)] = step;
        }
    }

    let mut x = (0..w).min_by_key(|&x| cost[idx(x, h - 1)]).unwrap_or(0);
    let mut seam = vec![0_u32; h as usize];
    for y in (0..h).rev() {
        seam[y as usize] = x;
        let step = from[idx(x, y)];
        x = match step {
            -1 => x.saturating_sub(1),
            1 => (x + 1).min(w - 1),
            _ => x,
        };
    }
    seam
}

/// Delete one pixel per row, shifting the remainder left.
// Reason (indexing_slicing): `seam` carries one entry per row, indexed by
// `y < h` where `h` is the canvas height the seam was computed from.
#[allow(clippy::indexing_slicing)]
fn remove_seam(canvas: &mut RgbaImage, seam: &[u32]) {
    let (w, h) = canvas.dimensions();
    let mut next = RgbaImage::new(w - 1, h);
    for y in 0..h {
        let cut = seam[y as usize];
        let mut nx = 0;
        for x in 0..w {
            if x == cut {
                continue;
            }
            next.put_pixel(nx, y, *canvas.get_pixel(x, y));
            nx += 1;
        }
    }
    *canvas = next;
}

/// Swap the axes, so the vertical-seam machinery can carve horizontal ones.
fn transpose(canvas: &RgbaImage) -> RgbaImage {
    let (w, h) = canvas.dimensions();
    let mut out = RgbaImage::new(h, w);
    for y in 0..h {
        for x in 0..w {
            out.put_pixel(y, x, *canvas.get_pixel(x, y));
        }
    }
    out
}
