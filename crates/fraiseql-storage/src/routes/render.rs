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
use crate::transforms::{ImageTransformer, OutputFormat, TransformParams};

/// Query parameters for the render endpoint.
#[derive(Debug, Deserialize)]
pub(super) struct RenderQuery {
    /// Target width in pixels.
    pub w:       Option<u32>,
    /// Target height in pixels.
    pub h:       Option<u32>,
    /// Output format: `webp` | `jpeg` | `png` | `avif`.
    pub format:  Option<String>,
    /// Encoder quality (1–100) for lossy formats.
    pub quality: Option<u8>,
    /// A named preset from the bucket's `transform_presets`.
    pub preset:  Option<String>,
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
    let Some(bucket) = state.buckets.get(&bucket_name) else {
        return error_response(StatusCode::NOT_FOUND, "bucket_not_found", "Bucket not found");
    };
    let user = user.map(|Extension(u)| u).unwrap_or_default();

    // Resolve the transform: preset first (when named), explicit params on top.
    let mut params = TransformParams {
        width:   None,
        height:  None,
        format:  None,
        quality: None,
    };
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
    // No explicit format: honour the client's Accept preference, if any.
    if params.format.is_none() {
        params.format = negotiate_format(headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()));
    }
    if params.width.is_none()
        && params.height.is_none()
        && params.format.is_none()
        && params.quality.is_none()
    {
        return error_response(
            StatusCode::BAD_REQUEST,
            "no_transform",
            "Specify at least one of w, h, format, quality, or preset (or an image Accept \
             preference)",
        );
    }

    // The same read gate as the download route (#876: missing and not-yours
    // answer identically).
    let row = match state.metadata.get(&bucket_name, &key).await {
        Ok(Some(row)) => row,
        Ok(None) => return object_not_visible(bucket, &user),
        Err(e) => return storage_error_response(&e),
    };
    if !state.rls.can_read(user.user_id.as_deref(), &user.roles, bucket, &row) {
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

    // A hostile or non-image object is a clean, named 400 (#370): the
    // transformer's dimension/allocation ceilings did the bounding.
    let output = match ImageTransformer::transform(&source, &params) {
        Ok(output) => output,
        Err(fraiseql_error::FraiseQLError::Validation { message, .. }) => {
            return error_response(StatusCode::BAD_REQUEST, "transform_rejected", &message);
        },
        Err(e) => return storage_error_response(&e),
    };

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
