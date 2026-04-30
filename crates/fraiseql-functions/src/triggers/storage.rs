//! Storage event triggers.
//!
//! Handles `after:storage:<bucket>:<operation>` triggers that fire when objects
//! are uploaded or deleted from object storage.
//!
//! ## Operations
//!
//! - `upload`: Fires after successful file upload
//! - `delete`: Fires after successful file deletion
//! - `all`: Fires for both upload and delete operations
//!
//! ## Event Payload
//!
//! The function receives metadata about the storage operation:
//! - Bucket name
//! - Object key (path)
//! - File size
//! - Content type (MIME type)
//! - Owner (user ID or service account)
//! - Operation type
//!
//! ## Async Dispatch
//!
//! Storage triggers fire asynchronously after the storage operation completes.
//! Failures in the trigger function do not affect the storage operation result.
use crate::types::EventPayload;
use serde::{Deserialize, Serialize};

/// Storage operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StorageOperation {
    /// Upload/put operation.
    Upload,
    /// Delete operation.
    Delete,
    /// Any storage operation.
    Any,
}

impl StorageOperation {
    /// Convert to string for trigger type.
    pub const fn as_str(&self) -> &str {
        match self {
            StorageOperation::Upload => "upload",
            StorageOperation::Delete => "delete",
            StorageOperation::Any => "any",
        }
    }
}

/// Event data for storage operations.
#[derive(Debug, Clone)]
pub struct StorageEventPayload {
    /// Bucket name.
    pub bucket: String,
    /// Object key/path.
    pub key: String,
    /// Object size in bytes.
    pub size_bytes: i64,
    /// MIME type of the object.
    pub content_type: String,
    /// User ID of the owner (if applicable).
    pub owner_id: Option<String>,
    /// Operation that triggered the event.
    pub operation: StorageOperation,
}

/// A trigger that fires after storage operations.
#[derive(Debug, Clone)]
pub struct StorageTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Bucket name to listen on.
    pub bucket: String,
    /// Operation filter (Upload, Delete, or Any).
    pub operation: StorageOperation,
}

impl StorageTrigger {
    /// Check if this trigger matches the given storage event.
    ///
    /// Matches if:
    /// - Bucket name matches exactly
    /// - Operation matches (Upload/Delete/Any)
    /// - Key doesn't have `_transforms/` prefix (internal cache operations)
    pub fn matches(&self, event: &StorageEventPayload) -> bool {
        // Bucket must match
        if self.bucket != event.bucket {
            return false;
        }

        // Operation must match
        let op_matches = match self.operation {
            StorageOperation::Any => true,
            _ => self.operation == event.operation,
        };

        if !op_matches {
            return false;
        }

        // Exclude internal transform cache operations
        if event.key.starts_with("_transforms/") {
            return false;
        }

        true
    }

    /// Check if this trigger should fire for the given event.
    ///
    /// This is an explicit check (same as `matches` but with a different name
    /// for clarity in tests).
    pub fn should_fire(&self, event: &StorageEventPayload) -> bool {
        self.matches(event)
    }

    /// Build an `EventPayload` from a storage event.
    pub fn build_payload(&self, event: &StorageEventPayload) -> EventPayload {
        let trigger_type = format!(
            "after:storage:{}:{}",
            event.bucket,
            event.operation.as_str()
        );

        let mut data = serde_json::json!({
            "bucket": event.bucket,
            "key": event.key,
            "size_bytes": event.size_bytes,
            "content_type": event.content_type,
            "operation": event.operation.as_str(),
        });

        if let Some(owner_id) = &event.owner_id {
            data["owner_id"] = serde_json::Value::String(owner_id.clone());
        }

        EventPayload {
            trigger_type,
            entity: event.bucket.clone(),
            event_kind: event.operation.as_str().to_string(),
            data,
            timestamp: chrono::Utc::now(),
        }
    }
}
