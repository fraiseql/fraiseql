//! The `200` body a relay (cursor) list route publishes: the page's rows, its page info and
//! cursors, and the links that resume it.

use serde_json::{Value, json};

/// The JSON envelope of a relay route's page, its rows typed by `type_ref`.
///
/// `meta` and `links` name what `handler::relay_page::body` writes: the page info, the window
/// the request asked for, `total` under `Prefer: count=exact`, and a `next`/`prev` link that is
/// `null` when there is no such page.
pub(super) fn envelope(type_ref: &str) -> Value {
    let cursor = json!({ "type": "string", "nullable": true });
    let link = json!({
        "type": "string",
        "nullable": true,
        "description": "The route with the request's own parameters and the cursor that \
                        resumes the walk; null at the end of the walk."
    });
    json!({
        "type": "object",
        "properties": {
            "data": {
                "type": "array",
                "items": { "$ref": type_ref }
            },
            "meta": {
                "type": "object",
                "properties": {
                    "hasNextPage": { "type": "boolean" },
                    "hasPreviousPage": { "type": "boolean" },
                    "startCursor": cursor,
                    "endCursor": cursor,
                    "first": { "type": "integer" },
                    "after": { "type": "string" },
                    "last": { "type": "integer" },
                    "before": { "type": "string" },
                    "total": { "type": "integer" }
                },
                "required": ["hasNextPage", "hasPreviousPage", "startCursor", "endCursor"]
            },
            "links": {
                "type": "object",
                "properties": {
                    "next": link,
                    "prev": link
                },
                "required": ["next", "prev"]
            }
        }
    })
}
