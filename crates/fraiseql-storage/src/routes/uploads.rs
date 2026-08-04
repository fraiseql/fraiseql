//! Resumable (Tus 1.0.0) upload endpoints (#369).
//!
//! - `POST   /storage/v1/uploads/{bucket}/{*key}` — create an upload session
//! - `PATCH  /storage/v1/uploads/{id}` — append a chunk at the declared offset
//! - `HEAD   /storage/v1/uploads/{id}` — read the current offset (resume point)
//! - `DELETE /storage/v1/uploads/{id}` — cancel the upload
//!
//! Every path funnels into the SAME ownership and metadata machinery as the
//! single-shot routes: creation passes `can_write_object` (the H9/B4 overwrite
//! gate) and *reserves* the metadata row exactly like a presigned upload
//! (#866), so the key carries an owner for the whole upload; completion is one
//! routine that finalises the backend staging and `confirm`s that row. A
//! session is owner-scoped: another identity probing, appending to, or
//! cancelling it gets the same `404` as a session that does not exist (#876 —
//! no existence oracle), and an unauthenticated caller gets `401`.

use axum::{
    Extension,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::Utc;

use super::{
    StorageState, StorageUser, backend_object_key, error_response, reject_unsafe_key,
    storage_error_response,
};
use crate::{metadata::NewStorageObject, uploads::NewUploadSession};

/// The Tus protocol version advertised on every response.
const TUS_RESUMABLE: &str = "1.0.0";

/// Session lifetime when the bucket does not configure `upload_ttl_secs`.
const DEFAULT_UPLOAD_TTL_SECS: u64 = 24 * 60 * 60;

/// The only body content type Tus permits on `PATCH`.
const OFFSET_OCTET_STREAM: &str = "application/offset+octet-stream";

/// Attach the `Tus-Resumable` header to a response.
fn with_tus(mut response: Response) -> Response {
    if let Ok(v) = TUS_RESUMABLE.parse() {
        response.headers_mut().insert("Tus-Resumable", v);
    }
    response
}

/// Parse the Tus `Upload-Metadata` header (`key base64value` pairs, comma
/// separated) and return the declared `filetype`/`contentType`, if any.
fn content_type_from_upload_metadata(headers: &HeaderMap) -> Option<String> {
    use base64::Engine as _;
    let raw = headers.get("Upload-Metadata")?.to_str().ok()?;
    for pair in raw.split(',') {
        let mut parts = pair.trim().splitn(2, ' ');
        let name = parts.next()?.trim();
        if name != "filetype" && name != "contentType" {
            continue;
        }
        let value = parts.next()?.trim();
        let decoded = base64::engine::general_purpose::STANDARD.decode(value).ok()?;
        return String::from_utf8(decoded).ok();
    }
    None
}

/// Read a non-negative integer header, or `None` when absent/malformed.
fn header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers.get(name)?.to_str().ok()?.parse::<i64>().ok().filter(|v| *v >= 0)
}

/// Create an upload session (Tus creation).
#[tracing::instrument(skip(state, user, headers), fields(bucket = %bucket_name, key = %key))]
pub(super) async fn create_upload_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path((bucket_name, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if let Some(rejection) = reject_unsafe_key(&key) {
        return with_tus(rejection);
    }
    let Some(bucket) = state.buckets.get(&bucket_name) else {
        return with_tus(error_response(
            StatusCode::NOT_FOUND,
            "bucket_not_found",
            "Bucket not found",
        ));
    };
    let user = user.map(|Extension(u)| u).unwrap_or_default();

    let Some(declared) = header_i64(&headers, "Upload-Length").filter(|v| *v > 0) else {
        return with_tus(error_response(
            StatusCode::BAD_REQUEST,
            "missing_upload_length",
            "Upload-Length header (a positive byte count) is required",
        ));
    };
    if let Some(max) = bucket.max_object_bytes {
        // Reason: `declared` is positive; the u64 cast cannot wrap.
        #[allow(clippy::cast_sign_loss)]
        if declared as u64 > max {
            return with_tus(error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Declared upload length exceeds the bucket's maximum object size",
            ));
        }
    }

    let content_type = content_type_from_upload_metadata(&headers)
        .unwrap_or_else(|| "application/octet-stream".to_string());
    if !bucket.allows_mime(&content_type) {
        return with_tus(error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "mime_type_rejected",
            "Content type not allowed for this bucket",
        ));
    }

    // H9/B4 overwrite gate, identical to put_handler / presign_handler:
    // creating needs bucket write permission, overwriting needs owner-or-admin.
    let existing = match state.metadata.get(&bucket_name, &key).await {
        Ok(existing) => existing,
        Err(e) => return with_tus(storage_error_response(&e)),
    };
    if !state
        .rls
        .can_write_object(user.user_id.as_deref(), &user.roles, bucket, existing.as_ref())
    {
        tracing::warn!(
            bucket = %bucket_name,
            key = %key,
            user_id = ?user.user_id,
            overwrite = existing.is_some(),
            "Resumable upload creation denied"
        );
        return with_tus(if user.user_id.is_none() {
            error_response(StatusCode::UNAUTHORIZED, "unauthorized", "Authentication required")
        } else {
            error_response(StatusCode::FORBIDDEN, "forbidden", "Access denied")
        });
    }
    let authorised_against = existing.as_ref().map(|row| row.pk_storage_object);

    // #866: claim the metadata row BEFORE any bytes exist, so the key carries
    // an owner for the whole (possibly days-long) upload.
    let claim = NewStorageObject {
        bucket:       bucket_name.clone(),
        key:          key.clone(),
        content_type: content_type.clone(),
        size_bytes:   0,
        etag:         None,
        owner_id:     user.user_id.clone(),
    };
    let pk = match state.metadata.reserve(&claim, authorised_against).await {
        Ok(Some(pk)) => pk,
        Ok(None) => {
            return with_tus(error_response(
                StatusCode::CONFLICT,
                "conflict",
                "The object changed while the request was being authorized; retry",
            ));
        },
        Err(e) => return with_tus(storage_error_response(&e)),
    };
    let created_reservation = authorised_against.is_none();

    // Begin backend staging. On failure, release a claim this request created.
    let object_key = backend_object_key(&bucket_name, &key);
    let backend_state = match state.backend.multipart_begin(&object_key, &content_type).await {
        Ok(s) => s,
        Err(e) => {
            release_if_created(&state, pk, created_reservation).await;
            return with_tus(storage_error_response(&e));
        },
    };

    let ttl = bucket.upload_ttl_secs.unwrap_or(DEFAULT_UPLOAD_TTL_SECS);
    let expires_at = Utc::now() + chrono::Duration::seconds(i64::try_from(ttl).unwrap_or(86_400));
    let session = NewUploadSession {
        bucket: bucket_name.clone(),
        key: key.clone(),
        content_type,
        declared_bytes: declared,
        owner_id: user.user_id.clone(),
        pk_storage_object: pk,
        created_reservation,
        backend_state: backend_state.clone(),
        expires_at,
    };
    let upload_id = match state.uploads.create(&session).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Another in-flight resumable upload holds this key. Do not
            // clobber it: unwind what this request set up and refuse.
            if let Err(e) = state.backend.multipart_abort(&object_key, &backend_state).await {
                tracing::error!(bucket = %bucket_name, key = %key, error = %e,
                    "Failed to abort backend staging after an upload-session conflict");
            }
            release_if_created(&state, pk, created_reservation).await;
            return with_tus(error_response(
                StatusCode::CONFLICT,
                "upload_in_flight",
                "A resumable upload for this key is already in progress",
            ));
        },
        Err(e) => {
            if let Err(abort) = state.backend.multipart_abort(&object_key, &backend_state).await {
                tracing::error!(bucket = %bucket_name, key = %key, error = %abort,
                    "Failed to abort backend staging after a session-create failure");
            }
            release_if_created(&state, pk, created_reservation).await;
            return with_tus(storage_error_response(&e));
        },
    };

    let mut headers = HeaderMap::new();
    if let Ok(v) = format!("/storage/v1/uploads/{upload_id}").parse() {
        headers.insert(header::LOCATION, v);
    }
    if let Ok(v) = expires_at.to_rfc2822().parse() {
        headers.insert("Upload-Expires", v);
    }
    with_tus((StatusCode::CREATED, headers).into_response())
}

/// Append one chunk (Tus `PATCH`).
#[tracing::instrument(skip(state, user, headers, body), fields(upload = %id))]
pub(super) async fn patch_upload_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some((session, bucket_name)) = load_owned_session(&state, user.as_ref(), &id).await else {
        return with_tus(session_not_visible(user.as_ref()));
    };
    if let Some(response) = reap_if_expired(&state, &session).await {
        return with_tus(response);
    }

    let content_type_ok = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(';').next().map(str::trim) == Some(OFFSET_OCTET_STREAM));
    if !content_type_ok {
        return with_tus(error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "invalid_content_type",
            "PATCH requires Content-Type: application/offset+octet-stream",
        ));
    }

    let Some(offset) = header_i64(&headers, "Upload-Offset") else {
        return with_tus(error_response(
            StatusCode::BAD_REQUEST,
            "missing_upload_offset",
            "Upload-Offset header is required",
        ));
    };
    if offset != session.received_bytes {
        return with_tus(error_response(
            StatusCode::CONFLICT,
            "offset_mismatch",
            "Upload-Offset does not match the server's current offset",
        ));
    }

    let object_key = backend_object_key(&bucket_name, &session.key);

    // A zero-length PATCH is only meaningful as a completion retry: all bytes
    // are staged (a previous final PATCH advanced the offset) but completion
    // itself failed. Anything else is a malformed request.
    if body.is_empty() {
        if session.received_bytes == session.declared_bytes {
            return with_tus(
                complete_upload(&state, &session, &object_key, session.received_bytes).await,
            );
        }
        return with_tus(error_response(
            StatusCode::BAD_REQUEST,
            "empty_chunk",
            "A chunk must contain at least one byte",
        ));
    }

    let chunk_len = i64::try_from(body.len()).unwrap_or(i64::MAX);
    let new_offset = session.received_bytes.saturating_add(chunk_len);
    if new_offset > session.declared_bytes {
        return with_tus(error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "length_exceeded",
            "Chunk would exceed the declared Upload-Length",
        ));
    }
    // Backends with a minimum part size (S3: 5 MiB) refuse undersized
    // non-final parts at completion time; reject up front so the constraint
    // is a clean 400 with a reason instead of a late backend error.
    let min_chunk = state.backend.multipart_min_chunk_bytes();
    // Reason: `body.len()` fits u64 on every supported target.
    #[allow(clippy::cast_sign_loss)]
    if (chunk_len as u64) < min_chunk && new_offset != session.declared_bytes {
        return with_tus(error_response(
            StatusCode::BAD_REQUEST,
            "chunk_too_small",
            "Each non-final chunk must be at least the backend's minimum part size",
        ));
    }

    let new_state = match state
        .backend
        .multipart_append(&object_key, session.backend_state.clone(), &body)
        .await
    {
        Ok(s) => s,
        Err(e) => return with_tus(storage_error_response(&e)),
    };
    match state.uploads.advance(session.upload_id, offset, new_offset, &new_state).await {
        Ok(true) => {},
        Ok(false) => {
            // A concurrent PATCH won the append race after our backend write.
            // The staged bytes may now hold both chunks in an undefined order;
            // the loser reports the conflict and the client resynchronises via
            // HEAD. (S3 parts are keyed by part number, so the winner's state
            // is internally consistent; the local staging file is repaired by
            // the client re-appending from the authoritative offset.)
            return with_tus(error_response(
                StatusCode::CONFLICT,
                "offset_mismatch",
                "A concurrent append advanced this upload; re-read the offset",
            ));
        },
        Err(e) => return with_tus(storage_error_response(&e)),
    }

    if new_offset == session.declared_bytes {
        return with_tus(complete_upload(&state, &session, &object_key, new_offset).await);
    }

    let mut response_headers = HeaderMap::new();
    if let Ok(v) = new_offset.to_string().parse() {
        response_headers.insert("Upload-Offset", v);
    }
    with_tus((StatusCode::NO_CONTENT, response_headers).into_response())
}

/// Read the current offset (Tus `HEAD`).
#[tracing::instrument(skip(state, user), fields(upload = %id))]
pub(super) async fn head_upload_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path(id): Path<String>,
) -> Response {
    let Some((session, _)) = load_owned_session(&state, user.as_ref(), &id).await else {
        return with_tus(session_not_visible(user.as_ref()));
    };
    if let Some(response) = reap_if_expired(&state, &session).await {
        return with_tus(response);
    }
    let mut headers = HeaderMap::new();
    if let Ok(v) = session.received_bytes.to_string().parse() {
        headers.insert("Upload-Offset", v);
    }
    if let Ok(v) = session.declared_bytes.to_string().parse() {
        headers.insert("Upload-Length", v);
    }
    headers.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
    with_tus((StatusCode::OK, headers).into_response())
}

/// Cancel an upload (Tus termination).
#[tracing::instrument(skip(state, user), fields(upload = %id))]
pub(super) async fn delete_upload_handler(
    State(state): State<StorageState>,
    user: Option<Extension<StorageUser>>,
    Path(id): Path<String>,
) -> Response {
    let Some((session, bucket_name)) = load_owned_session(&state, user.as_ref(), &id).await else {
        return with_tus(session_not_visible(user.as_ref()));
    };
    let object_key = backend_object_key(&bucket_name, &session.key);
    with_tus(teardown_session(&state, &session, &object_key, StatusCode::NO_CONTENT).await)
}

// ---------------------------------------------------------------------------
// Shared session machinery
// ---------------------------------------------------------------------------

/// Load a session and prove the caller owns it. `None` covers every
/// indistinguishable refusal: malformed id, no such session, someone else's
/// session (#876 — no existence oracle). The bucket name is returned alongside
/// because teardown needs it after the session moves.
async fn load_owned_session(
    state: &StorageState,
    user: Option<&Extension<StorageUser>>,
    id: &str,
) -> Option<(crate::uploads::UploadSession, String)> {
    let upload_id = uuid::Uuid::parse_str(id).ok()?;
    let session = match state.uploads.get(upload_id).await {
        Ok(session) => session?,
        Err(e) => {
            tracing::error!(upload = %id, error = %e, "Failed to load upload session");
            return None;
        },
    };
    let caller = user.and_then(|Extension(u)| u.user_id.as_deref())?;
    if session.owner_id.as_deref() != Some(caller) {
        tracing::warn!(
            upload = %id,
            user_id = %caller,
            "Upload session access denied: not the owner"
        );
        return None;
    }
    let bucket = session.bucket.clone();
    Some((session, bucket))
}

/// The uniform refusal for a session the caller cannot see: `401` for an
/// unauthenticated caller (who can never own a session), `404` otherwise.
fn session_not_visible(user: Option<&Extension<StorageUser>>) -> Response {
    if user.map_or(true, |Extension(u)| u.user_id.is_none()) {
        error_response(StatusCode::UNAUTHORIZED, "unauthorized", "Authentication required")
    } else {
        error_response(StatusCode::NOT_FOUND, "not_found", "Upload not found")
    }
}

/// If the session's deadline has passed, tear it down (abort staging, release
/// a created reservation, drop the row) and answer `410 Gone`.
async fn reap_if_expired(
    state: &StorageState,
    session: &crate::uploads::UploadSession,
) -> Option<Response> {
    if !session.is_expired(Utc::now()) {
        return None;
    }
    let object_key = backend_object_key(&session.bucket, &session.key);
    let _teardown = teardown_session(state, session, &object_key, StatusCode::GONE).await;
    Some(error_response(
        StatusCode::GONE,
        "upload_expired",
        "The upload session has expired",
    ))
}

/// Tear a session down: abort the backend staging, release a metadata
/// reservation this session created, and delete the session row. Teardown
/// failures are logged loud but do not mask the caller's outcome — the
/// session row is only removed when the backend abort succeeded, so a
/// transiently-failing abort is retryable.
async fn teardown_session(
    state: &StorageState,
    session: &crate::uploads::UploadSession,
    object_key: &str,
    success: StatusCode,
) -> Response {
    if let Err(e) = state.backend.multipart_abort(object_key, &session.backend_state).await {
        tracing::error!(upload = %session.upload_id, error = %e,
            "Failed to abort backend staging for an upload session");
        return storage_error_response(&e);
    }
    if session.created_reservation {
        if let Err(e) = state.metadata.release_reservation(session.pk_storage_object).await {
            tracing::error!(upload = %session.upload_id, error = %e,
                "Failed to release the metadata reservation for a cancelled upload");
            return storage_error_response(&e);
        }
    }
    if let Err(e) = state.uploads.delete(session.upload_id).await {
        tracing::error!(upload = %session.upload_id, error = %e,
            "Failed to delete an upload session row");
        return storage_error_response(&e);
    }
    success.into_response()
}

/// THE completion routine: finalise the backend staging into the object and
/// confirm the reserved metadata row. Every upload path ends here — there is
/// deliberately no second way for staged bytes to become an object, so the
/// metadata row can never be skipped (#866's lesson applied to #369).
async fn complete_upload(
    state: &StorageState,
    session: &crate::uploads::UploadSession,
    object_key: &str,
    total_bytes: i64,
) -> Response {
    // Re-read the authoritative backend state: `session` may carry the
    // pre-advance snapshot when completion happens on the final PATCH.
    let current = match state.uploads.get(session.upload_id).await {
        Ok(Some(current)) => current,
        Ok(None) => {
            // A concurrent completion won; the object exists. Report success
            // idempotently.
            return (StatusCode::NO_CONTENT, offset_header(total_bytes)).into_response();
        },
        Err(e) => return storage_error_response(&e),
    };
    let etag = match state.backend.multipart_complete(object_key, &current.backend_state).await {
        Ok(etag) => etag,
        Err(e) => return storage_error_response(&e),
    };
    if let Err(e) = state
        .metadata
        .confirm(session.pk_storage_object, total_bytes, Some(&etag))
        .await
    {
        return storage_error_response(&e);
    }
    if let Err(e) = state.uploads.delete(session.upload_id).await {
        tracing::error!(upload = %session.upload_id, error = %e,
            "Completed upload's session row could not be deleted");
        return storage_error_response(&e);
    }
    (StatusCode::NO_CONTENT, offset_header(total_bytes)).into_response()
}

/// Release a metadata reservation this request created; a claim over a
/// pre-existing object stays put (its bytes still exist).
async fn release_if_created(state: &StorageState, pk: i64, created: bool) {
    if created {
        if let Err(e) = state.metadata.release_reservation(pk).await {
            tracing::error!(pk, error = %e,
                "Failed to release a storage reservation after an upload-create failure");
        }
    }
}

fn offset_header(offset: i64) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(v) = offset.to_string().parse() {
        headers.insert("Upload-Offset", v);
    }
    headers
}
