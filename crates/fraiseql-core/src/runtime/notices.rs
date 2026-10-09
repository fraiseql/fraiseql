//! Response notices: what a successful response says about itself (#1314).
//!
//! A notice is not an error. The response is served; the notice tells the caller something
//! the data alone cannot, such as "this similarity search may have stopped short". The
//! engine records notices while it executes a GraphQL document and renders them in the
//! response's `extensions.notices`. (`nearest`, today's only source, is a GraphQL argument:
//! REST, gRPC and MCP tools have no way to ask for one.)
//!
//! Notices are collected per request through a task-local scope ([`collect_notices`]); a
//! notice pushed outside one ([`push_notice`]) is dropped, so an engine path that no
//! transport wraps costs nothing.

use std::cell::RefCell;

use serde::Serialize;

/// One notice about a served response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResponseNotice {
    /// The response path the notice is about (the root field's response key).
    pub path:   Vec<String>,
    /// What kind of notice it is, a stable machine-readable name.
    pub kind:   &'static str,
    /// The notice's details, specific to its kind.
    pub detail: serde_json::Value,
}

/// The kind of a notice that a filtered similarity search returned fewer rows than `k`.
pub const POSSIBLY_TRUNCATED: &str = "nearest_possibly_truncated";

/// What a `nearest` search that came back with fewer than `k` rows does (#1314).
///
/// pgvector's graph search keeps `hnsw.ef_search` candidates (40 by default) and, without an
/// iterative scan, returns at most those: a selective filter can leave fewer than `k` among
/// them, and a `k` above `ef_search` is cut to it even with no filter. The search returns
/// what it found, successfully, and the rows alone cannot tell that from "only these
/// matched".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortResultPolicy {
    /// Serve the rows with a notice that they may be truncated (unverified). Costs nothing.
    #[default]
    Signal,
    /// Count, in the same statement, how many rows match (up to `k`), and say so only when
    /// more matched than were returned.
    Verify,
    /// As `Verify`, and refuse a verified truncation instead of serving it.
    Refuse,
}

/// Decide a `nearest` search of `k` that returned `returned` rows (#1314).
///
/// A notice, an error, or nothing. `matched` is the counted matches (up to `k`) under `Verify` and
/// `Refuse`, `None` when no count could be taken (the notice is then unverified, and never a
/// refusal).
///
/// # Errors
///
/// `FraiseQLError::Unsupported` under [`ShortResultPolicy::Refuse`] for a verified truncation.
pub fn settle_short_nearest(
    policy: ShortResultPolicy,
    response_key: &str,
    k: u32,
    returned: usize,
    matched: Option<u64>,
) -> crate::error::Result<()> {
    let returned_u64 = u64::try_from(returned).unwrap_or(u64::MAX);
    if returned_u64 >= u64::from(k) {
        return Ok(());
    }
    let verified = match (policy, matched) {
        // Unverified: asked for, or no count could be taken on this read (a composed one).
        (ShortResultPolicy::Signal, _) | (_, None) => false,
        // A genuine short result (fewer than `k` matched) is no truncation at all.
        (ShortResultPolicy::Verify | ShortResultPolicy::Refuse, Some(matched)) => {
            if matched <= returned_u64 {
                return Ok(());
            }
            true
        },
    };
    // Only a verified truncation is refused; an unverified one is served with its notice.
    if verified && policy == ShortResultPolicy::Refuse {
        return Err(crate::error::FraiseQLError::Unsupported {
            message: format!(
                "`{response_key}`: the similarity search found {returned} of the {k} nearest \
                 rows its filter matches and stopped short (`vector_on_short_result = \
                 \"refuse\"`). Raise `vector_hnsw_ef_search` so the search keeps more \
                 candidates, or turn on `vector_hnsw_iterative_scan`."
            ),
        });
    }
    push_notice(ResponseNotice {
        path:   vec![response_key.to_string()],
        kind:   POSSIBLY_TRUNCATED,
        detail: serde_json::json!({
            "requested": k,
            "returned": returned,
            "verified": verified,
        }),
    });
    Ok(())
}

tokio::task_local! {
    static NOTICES: RefCell<Vec<ResponseNotice>>;
}

/// Run `future`, collecting the notices the engine records while it runs.
pub async fn collect_notices<F: std::future::Future>(
    future: F,
) -> (F::Output, Vec<ResponseNotice>) {
    NOTICES
        .scope(RefCell::new(Vec::new()), async move {
            let output = future.await;
            let notices = NOTICES.with(|cell| cell.take());
            (output, notices)
        })
        .await
}

/// Record a notice for the current request. A no-op outside [`collect_notices`].
pub fn push_notice(notice: ResponseNotice) {
    let _ = NOTICES.try_with(|cell| cell.borrow_mut().push(notice));
}

/// `response` with `notices` under `extensions.notices`; unchanged when there are none.
#[must_use]
pub fn with_notices(
    mut response: serde_json::Value,
    notices: &[ResponseNotice],
) -> serde_json::Value {
    if notices.is_empty() {
        return response;
    }
    if let Some(object) = response.as_object_mut() {
        let extensions = object
            .entry("extensions")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let Some(extensions) = extensions.as_object_mut() {
            extensions.insert(
                "notices".to_string(),
                serde_json::to_value(notices).unwrap_or(serde_json::Value::Null),
            );
        }
    }
    response
}

#[cfg(test)]
#[path = "notices_tests.rs"]
mod tests;
