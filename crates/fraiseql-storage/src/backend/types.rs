//! Shared types for storage operations.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Result of a successful PUT operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutResult {
    /// Entity tag (`ETag`) for integrity verification.
    pub etag: String,
    /// Size of the uploaded object in bytes.
    pub size: u64,
}

/// Result of a LIST operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResult {
    /// Objects matching the query.
    pub objects:     Vec<ObjectInfo>,
    /// Cursor for the next page (if any).
    pub next_cursor: Option<String>,
}

/// Summary information about a stored object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    /// Key (path) of the object.
    pub key:           String,
    /// Size in bytes.
    pub size:          u64,
    /// MIME type.
    pub content_type:  String,
    /// Entity tag for integrity.
    pub etag:          String,
    /// When the object was last modified, as the backend reports it; `None` when it
    /// reports no time. Never a time the listing made up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// Metadata associated with a stored object.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObjectMetadata {
    /// Owner/uploader of the object.
    pub owner:          Option<String>,
    /// Custom headers to store with the object.
    pub custom_headers: HashMap<String, String>,
}
