//! File-based audit backend (Phase 11.3 Cycle 2 - GREEN)
//!
//! Stores audit events as JSON lines in a file.

use super::*;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// File-based audit backend that stores events as JSON lines.
///
/// Each audit event is written as a single JSON line, enabling efficient
/// parsing and querying of log files.
pub struct FileAuditBackend {
    /// Path to the audit log file
    file_path: String,

    /// Mutex for serializing writes to ensure consistency
    write_lock: Arc<Mutex<()>>,
}

impl FileAuditBackend {
    /// Create a new file-based audit backend.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the audit log file
    ///
    /// # Errors
    ///
    /// Returns error if the parent directory doesn't exist or file cannot be created
    pub async fn new<P: AsRef<Path>>(path: P) -> AuditResult<Self> {
        let file_path = path
            .as_ref()
            .to_str()
            .ok_or_else(|| AuditError::FileError("Invalid path".to_string()))?
            .to_string();

        // Verify we can create/open the file
        let _file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to open file: {}", e)))?;

        Ok(Self {
            file_path,
            write_lock: Arc::new(Mutex::new(())),
        })
    }
}

#[async_trait::async_trait]
impl AuditBackend for FileAuditBackend {
    /// Log an audit event to the file.
    async fn log_event(&self, event: AuditEvent) -> AuditResult<()> {
        // Validate event before logging
        event.validate()?;

        // Acquire write lock to ensure serialization
        let _lock = self.write_lock.lock().await;

        // Convert event to JSON line
        let json_str = serde_json::to_string(&event)
            .map_err(|e| AuditError::SerializationError(e.to_string()))?;

        // Open file in append mode
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to open file: {}", e)))?;

        // Write JSON line with newline
        file.write_all(json_str.as_bytes())
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to write event: {}", e)))?;

        file.write_all(b"\n")
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to write newline: {}", e)))?;

        file.sync_all()
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to sync file: {}", e)))?;

        Ok(())
    }

    /// Query audit events from the file.
    ///
    /// Reads all events from the file and applies filters in memory.
    async fn query_events(
        &self,
        filters: AuditQueryFilters,
    ) -> AuditResult<Vec<AuditEvent>> {
        // Read file content
        let content = tokio::fs::read_to_string(&self.file_path)
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to read file: {}", e)))?;

        // Parse JSON lines
        let mut events: Vec<AuditEvent> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<AuditEvent>(line).ok())
            .collect();

        // Apply filters
        if let Some(event_type) = &filters.event_type {
            events.retain(|e| e.event_type == *event_type);
        }

        if let Some(user_id) = &filters.user_id {
            events.retain(|e| e.user_id == *user_id);
        }

        if let Some(resource_type) = &filters.resource_type {
            events.retain(|e| e.resource_type == *resource_type);
        }

        if let Some(status) = &filters.status {
            events.retain(|e| e.status == *status);
        }

        if let Some(tenant_id) = &filters.tenant_id {
            events.retain(|e| e.tenant_id.as_ref() == Some(tenant_id));
        }

        // Apply pagination
        let offset = filters.offset.unwrap_or(0);
        let limit = filters.limit.unwrap_or(100);

        let paginated = events
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();

        Ok(paginated)
    }
}

// Re-export for convenience
pub use super::AuditBackend;
