//! The image-render endpoint (#370, closing #901's "transforms have no HTTP
//! surface"): `GET /storage/v1/render/{bucket}/{*key}?w=&h=&format=&quality=&preset=`.
//!
//! Renders run through exactly the gates the download route uses — metadata
//! lookup, `can_read`, the missing/not-yours collapse (#876) — and then
//! through [`ImageTransformer`], whose hard dimension/allocation ceilings make
//! a hostile image (decompression bomb, absurd request size, malformed bytes)
//! a named `400`, never a resource-exhaustion vector (#370).
//!
//! Output is freshly transformed per request and served with an `ETag` and a
//! bucket-appropriate `Cache-Control`, so browser/CDN caches do the caching;
//! re-encoding through the `image` crate drops source metadata (EXIF, GPS) by
//! construction.

use std::sync::Arc;

use axum::{
    Extension,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use super::{
    StorageState, StorageUser, backend_object_key, error_response, object_not_visible,
    reject_unsafe_key, storage_error_response,
};
use crate::transforms::{
    CropSpec, Gravity, ImageTransformer, OutputFormat, ResizeMode, TransformCache, TransformParams,
    Watermark, parse_colour,
};

/// Query parameters for the render endpoint.
#[derive(Debug, Deserialize)]
pub(super) struct RenderQuery {
    /// Target width in pixels.
    pub w:               Option<u32>,
    /// Target height in pixels.
    pub h:               Option<u32>,
    /// Output format: `webp` | `jpeg` | `png` | `avif`.
    pub format:          Option<String>,
    /// Encoder quality (1–100) for lossy formats.
    pub quality:         Option<u8>,
    /// A named preset from the bucket's `transform_presets`.
    pub preset:          Option<String>,
    /// How the resize fills the `w`×`h` box (#973).
    pub mode:            Option<String>,
    /// Where a `fill` keeps its content, or a crop is taken from.
    pub gravity:         Option<String>,
    /// Letterbox colour for `mode=fit`, as `#rrggbb` or `#rrggbbaa`.
    pub background:      Option<String>,
    /// An explicit crop: `x,y,w,h` or an aspect ratio `w:h`.
    pub crop:            Option<String>,
    /// Gaussian blur sigma.
    pub blur:            Option<f32>,
    /// Unsharp-mask sigma.
    pub sharpen:         Option<f32>,
    /// Key of a stored object in the same bucket to composite as a watermark.
    pub watermark:       Option<String>,
    /// Text to rasterise as a watermark, using the bucket's `watermark_font`.
    pub watermark_text:  Option<String>,
    /// Type size for `watermark_text`, in pixels.
    pub watermark_size:  Option<f32>,
    /// Watermark colour, as `#rrggbb` or `#rrggbbaa`.
    pub watermark_color: Option<String>,
    /// Watermark width as a fraction of the canvas, `0` < s ≤ `1`.
    pub watermark_scale: Option<f32>,
    /// Watermark inset from the canvas edge, in pixels.
    pub watermark_inset: Option<u32>,
}

/// Parse a client-supplied format name. Unknown names are a 400, not a
/// fallthrough.
fn parse_format(name: &str) -> Option<OutputFormat> {
    match name.to_ascii_lowercase().as_str() {
        "webp" => Some(OutputFormat::Webp),
        "jpeg" | "jpg" => Some(OutputFormat::Jpeg),
        "png" => Some(OutputFormat::Png),
        "avif" => Some(OutputFormat::Avif),
        _ => None,
    }
}

/// Parse an RFC 9110 q-value (`0`–`1` with at most three decimals) into
/// integer thousandths. Malformed values are treated as the default `1`, which
/// is what a lenient parser must do rather than dropping the entry.
fn parse_qvalue(raw: &str) -> u32 {
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    let whole: u32 = whole.parse().unwrap_or(1);
    if whole >= 1 {
        return 1000;
    }
    let mut thousandths = 0_u32;
    for i in 0..3 {
        let digit = fraction.as_bytes().get(i).map_or(0, |b| u32::from(b.wrapping_sub(b'0')));
        thousandths = thousandths * 10 + if digit < 10 { digit } else { 0 };
    }
    thousandths
}

/// Pick an output format from an `Accept` header: the supported `image/*`
/// entry with the highest q-value (first listed wins a tie). `None` when the
/// header expresses no usable image preference — the source format is kept.
pub(super) fn negotiate_format(accept: Option<&str>) -> Option<OutputFormat> {
    let accept = accept?;
    // q-values are compared as integer thousandths: the header grammar allows
    // at most three decimals, so this is exact and avoids float equality.
    let mut best: Option<(u32, usize, OutputFormat)> = None;
    for (position, entry) in accept.split(',').enumerate() {
        let mut parts = entry.trim().split(';');
        let media = parts.next().unwrap_or("").trim();
        let format = match media.to_ascii_lowercase().as_str() {
            "image/webp" => OutputFormat::Webp,
            "image/avif" => OutputFormat::Avif,
            "image/jpeg" => OutputFormat::Jpeg,
            "image/png" => OutputFormat::Png,
            _ => continue,
        };
        let q = parts
            .find_map(|p| p.trim().strip_prefix("q="))
            .map_or(1000, |raw| parse_qvalue(raw.trim()));
        if q == 0 {
            continue;
        }
        let better = match &best {
            None => true,
            Some((best_q, best_pos, _)) => q > *best_q || (q == *best_q && position < *best_pos),
        };
        if better {
            best = Some((q, position, format));
        }
    }
    best.map(|(_, _, format)| format)
}

/// Turn a transform-validation error into the render route's `400`.
fn validation_response(error: &fraiseql_error::FraiseQLError) -> Response {
    match error {
        fraiseql_error::FraiseQLError::Validation { message, .. } => {
            error_response(StatusCode::BAD_REQUEST, "transform_rejected", message)
        },
        other => storage_error_response(other),
    }
}

/// Resolve the requested watermark into pixels.
///
/// A watermark asset is a stored object, so it goes through **the same read
/// gate as any other object in this bucket** — metadata lookup, `can_read`, and
/// the missing/not-yours collapse. #336's property is that a bucket boundary is
/// a boundary for every path that reads bytes, and a watermark parameter would
/// otherwise be a way to read one object through another object's permissions.
///
/// Text watermarks need the bucket's `watermark_font`; a bucket without one
/// refuses by name rather than rendering in some substitute typeface.
async fn resolve_watermark(
    state: &StorageState,
    bucket: &crate::config::BucketConfig,
    bucket_name: &str,
    user: &StorageUser,
    query: &RenderQuery,
) -> Result<Watermark, Response> {
    let gravity = query.gravity.as_deref().and_then(Gravity::parse).unwrap_or(Gravity::SouthEast);
    let opacity = query
        .watermark_color
        .as_deref()
        .map(parse_colour)
        .transpose()
        .map_err(|e| validation_response(&e))?
        .map_or(u8::MAX, |c| c.0[3]);

    let (image, source) = if let Some(ref mark_key) = query.watermark {
        if let Some(rejection) = reject_unsafe_key(mark_key) {
            return Err(rejection);
        }
        let row = match state.metadata.get(bucket_name, mark_key).await {
            Ok(Some(row)) => row,
            Ok(None) => return Err(object_not_visible(bucket, user)),
            Err(e) => return Err(storage_error_response(&e)),
        };
        if !state.rls.can_read(&user.caller(chrono::Utc::now()), bucket, &row) {
            tracing::warn!(
                bucket = %bucket_name,
                key = %mark_key,
                user_id = ?user.user_id,
                "Watermark asset denied: access forbidden"
            );
            return Err(object_not_visible(bucket, user));
        }
        let bytes = match state.backend.download(&backend_object_key(bucket_name, mark_key)).await {
            Ok(bytes) => bytes,
            Err(e) => return Err(storage_error_response(&e)),
        };
        let decoded = image::load_from_memory(&bytes).map_err(|_| {
            error_response(
                StatusCode::BAD_REQUEST,
                "transform_rejected",
                "The watermark object is not a readable image",
            )
        })?;
        (decoded, format!("object:{mark_key}"))
    } else {
        let text = query.watermark_text.as_deref().unwrap_or_default();
        let Some(ref font_bytes) = bucket.watermark_font else {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "watermark_font_unset",
                "This bucket has no watermark_font configured, so text watermarks are not \
                 available on it",
            ));
        };
        let font = crate::transforms::text::parse_font(font_bytes.as_ref().clone())
            .map_err(|e| validation_response(&e))?;
        let colour = query
            .watermark_color
            .as_deref()
            .map_or(Ok(image::Rgba([255, 255, 255, 255])), parse_colour)
            .map_err(|e| validation_response(&e))?;
        let size = query.watermark_size.unwrap_or(48.0);
        let rendered = crate::transforms::text::render_text(&font, text, size, colour)
            .map_err(|e| validation_response(&e))?;
        (rendered, format!("text:{text}"))
    };

    Ok(Watermark {
        image,
        gravity,
        opacity,
        scale: query.watermark_scale.unwrap_or(0.25),
        margin: query.watermark_inset.unwrap_or(0),
        source,
    })
}

/// Render an object through the image transform pipeline.
#[tracing::instrument(skip(state, user, headers, query), fields(bucket = %bucket_name, key = %key))]
pub(super) async fn render_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
    Query(query): Query<RenderQuery>,
    headers: HeaderMap,
) -> Response {
    if let Some(rejection) = reject_unsafe_key(&key) {
        return rejection;
    }
    // Snapshot the bucket map for the whole request: a policy pushed over the
    // admin API mid-request must not decide half of it (#974).
    let buckets = state.buckets.load();
    let Some(bucket) = buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };
    let user = user.map(|Extension(u)| u).unwrap_or_default();

    // Resolve the transform: bucket default, preset when named, explicit
    // params on top.
    let mut params = TransformParams::default();
    if let Some(ref name) = bucket.default_resize_mode {
        // Parsed at boot, so an unknown name here is impossible; treating it as
        // absent rather than panicking keeps a config path from crashing a
        // render.
        params.resize_mode = ResizeMode::parse(name);
    }
    if let Some(ref preset_name) = query.preset {
        let Some(preset) = bucket
            .transform_presets
            .as_ref()
            .and_then(|presets| presets.iter().find(|p| &p.name == preset_name))
        else {
            return error_response(
                StatusCode::BAD_REQUEST,
                "unknown_preset",
                "No such transform preset on this bucket",
            );
        };
        params.width = preset.width;
        params.height = preset.height;
        params.quality = preset.quality;
        if let Some(ref mode) = preset.resize_mode {
            params.resize_mode = ResizeMode::parse(mode);
        }
        if let Some(ref gravity) = preset.gravity {
            params.gravity = Gravity::parse(gravity);
        }
        if let Some(ref format) = preset.format {
            let Some(parsed) = parse_format(format) else {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_preset",
                    "The preset declares an unsupported output format",
                );
            };
            params.format = Some(parsed);
        }
    }
    if query.w.is_some() {
        params.width = query.w;
    }
    if query.h.is_some() {
        params.height = query.h;
    }
    if query.quality.is_some() {
        params.quality = query.quality;
    }
    if let Some(ref format) = query.format {
        let Some(parsed) = parse_format(format) else {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_format",
                "format must be one of webp, jpeg, png, avif",
            );
        };
        params.format = Some(parsed);
    }
    // #973's geometry and effect parameters, each refused by name rather than
    // ignored: a misspelt mode that silently renders something else is exactly
    // the surprise this vocabulary exists to prevent.
    if let Some(ref mode) = query.mode {
        let Some(parsed) = ResizeMode::parse(mode) else {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_mode",
                "mode must be one of contain, stretch, fit, fill, cover-blur, cover-mirror",
            );
        };
        params.resize_mode = Some(parsed);
    }
    if let Some(ref gravity) = query.gravity {
        let Some(parsed) = Gravity::parse(gravity) else {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_gravity",
                "gravity must be a compass point, center, or smart",
            );
        };
        params.gravity = Some(parsed);
    }
    if let Some(ref background) = query.background {
        match parse_colour(background) {
            Ok(colour) => params.background = Some(colour),
            Err(e) => return validation_response(&e),
        }
    }
    if let Some(ref crop) = query.crop {
        match CropSpec::parse(crop) {
            Ok(spec) => params.crop = Some(spec),
            Err(e) => return validation_response(&e),
        }
    }
    params.blur = query.blur;
    params.sharpen = query.sharpen;

    // No explicit format: honour the client's Accept preference, if any.
    if params.format.is_none() {
        params.format = negotiate_format(headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()));
    }
    let wants_watermark = query.watermark.is_some() || query.watermark_text.is_some();
    if params.is_empty() && !wants_watermark {
        return error_response(
            StatusCode::BAD_REQUEST,
            "no_transform",
            "Specify at least one of w, h, format, quality, crop, blur, sharpen, watermark, or \
             preset (or an image Accept preference)",
        );
    }
    if query.watermark.is_some() && query.watermark_text.is_some() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "ambiguous_watermark",
            "Specify watermark (a stored object) or watermark_text, not both",
        );
    }

    // The same read gate as the download route (#876: missing and not-yours
    // answer identically).
    let row = match state.metadata.get(&bucket_name, &key).await {
        Ok(Some(row)) => row,
        Ok(None) => return object_not_visible(bucket, &user),
        Err(e) => return storage_error_response(&e),
    };
    if !state.rls.can_read(&user.caller(chrono::Utc::now()), bucket, &row) {
        tracing::warn!(
            bucket = %bucket_name,
            key = %key,
            user_id = ?user.user_id,
            "Storage render denied: access forbidden"
        );
        return object_not_visible(bucket, &user);
    }

    let source = match state.backend.download(&backend_object_key(&bucket_name, &key)).await {
        Ok(source) => source,
        Err(e) => return storage_error_response(&e),
    };

    if wants_watermark {
        match resolve_watermark(&state, bucket, &bucket_name, &user, &query).await {
            Ok(mark) => params.watermark = Some(mark),
            Err(response) => return response,
        }
    }

    // A render is a pure function of the source bytes and the resolved
    // transform, so the cache is keyed on a digest of both (#973): a
    // re-uploaded source hashes differently and reads a different key, which is
    // the whole invalidation story.
    let cache_key = TransformCache::build_cache_key(&bucket_name, &key, &source, &params);
    let cache = TransformCache::new(Arc::clone(&state.backend));
    let cached = cache.get(&cache_key).await;
    let hit = cached.is_some();

    let output = if let Some(output) = cached {
        output
    } else {
        // A hostile or non-image object is a clean, named 400 (#370): the
        // transformer's dimension/allocation ceilings did the bounding.
        let output = match ImageTransformer::transform(&source, &params) {
            Ok(output) => output,
            Err(fraiseql_error::FraiseQLError::Validation { message, .. }) => {
                return error_response(StatusCode::BAD_REQUEST, "transform_rejected", &message);
            },
            Err(e) => return storage_error_response(&e),
        };
        if let Err(e) = cache.put(&cache_key, &output).await {
            // The render succeeded; a cache that cannot be written is a
            // slower service, not a failed request.
            tracing::warn!(bucket = %bucket_name, key = %key, error = %e,
                    "Could not store a rendered image in the transform cache");
        }
        output
    };

    // #973: the resolved transform is auditable per request. `params.describe()`
    // is the same string the cache key is derived from, so the log and the
    // cache can never disagree about what was rendered.
    tracing::info!(
        bucket = %bucket_name,
        key = %key,
        user_id = ?user.user_id,
        transform = %params.describe(),
        cache = if hit { "hit" } else { "miss" },
        width = output.width,
        height = output.height,
        bytes = output.body.len(),
        "Storage render served"
    );

    let mut response_headers = HeaderMap::new();
    if let Ok(ct) = output.content_type.parse() {
        response_headers.insert(header::CONTENT_TYPE, ct);
    }
    if let Some(ref etag) = output.etag {
        if let Ok(v) = etag.parse() {
            response_headers.insert(header::ETAG, v);
        }
    }
    // #608's caching rule applies to renders identically: a Private bucket's
    // per-request RLS decision cannot be represented by a URL-keyed shared
    // cache.
    let cache_control = match bucket.access {
        crate::config::BucketAccess::Private => "private, no-store",
        crate::config::BucketAccess::PublicRead => "public, max-age=3600",
    };
    response_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_control));
    response_headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    // Rendered output is always a raster image (the transformer only encodes
    // webp/jpeg/png/avif — never active content), so inline display is safe
    // regardless of the bucket's `serve_inline` stance for raw objects.
    response_headers.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("inline"));
    (StatusCode::OK, response_headers, axum::body::Body::from(output.body)).into_response()
}
