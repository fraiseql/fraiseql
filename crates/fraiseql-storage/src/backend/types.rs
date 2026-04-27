//! Shared types for storage operations.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of a successful PUT operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutResult {
    /// Entity tag (ETag) for integrity verification.
    pub etag: String,
    /// Size of the uploaded object in bytes.
    pub size: u64,
}

/// An object retrieved from storage.
#[derive(Debug, Clone)]
pub struct StorageObject {
    /// Raw file contents.
    pub body: Bytes,
    /// MIME type of the object.
    pub content_type: String,
    /// Size in bytes.
    pub size: u64,
    /// Entity tag for integrity verification.
    pub etag: String,
    /// Last modification time as ISO 8601 string.
    pub last_modified: String,
}

/// Result of a LIST operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResult {
    /// Objects matching the query.
    pub objects: Vec<ObjectInfo>,
    /// Cursor for the next page (if any).
    pub next_cursor: Option<String>,
}

/// Summary information about a stored object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    /// Key (path) of the object.
    pub key: String,
    /// Size in bytes.
    pub size: u64,
    /// MIME type.
    pub content_type: String,
    /// Entity tag for integrity.
    pub etag: String,
    /// Last modification time as ISO 8601.
    pub last_modified: String,
}

/// Metadata associated with a stored object.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObjectMetadata {
    /// Owner/uploader of the object.
    pub owner: Option<String>,
    /// Custom headers to store with the object.
    pub custom_headers: HashMap<String, String>,
}
