//! File-based audit backend
//!
//! Stores audit events as JSON lines in a file.

use std::{path::Path, sync::Arc};

use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::Mutex};

use super::*;

/// File-based audit backend that stores events as JSON lines.
///
/// Each audit event is written as a single JSON line, enabling efficient
/// parsing and querying of log files.
///
/// The file handle is kept open for the lifetime of the backend; writes are
/// serialized via a `Mutex<File>` and the JSON + newline are combined into a
/// single `write_all` call to avoid partial writes.
pub struct FileAuditBackend {
    /// Path to the audit log file (retained for query_events reads)
    file_path: String,

    /// Open file handle — held for the lifetime of the backend.
    /// Mutex ensures serialized, non-interleaved writes.
    file: Arc<Mutex<tokio::fs::File>>,
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

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to open file: {}", e)))?;

        Ok(Self {
            file_path,
            file: Arc::new(Mutex::new(file)),
        })
    }
}

#[async_trait::async_trait]
impl AuditBackend for FileAuditBackend {
    /// Log an audit event to the file.
    ///
    /// Serializes the event to JSON, appends a newline, and writes both in a
    /// single `write_all` call while holding the file lock — ensuring each
    /// log entry is atomic at the OS level.
    async fn log_event(&self, event: AuditEvent) -> AuditResult<()> {
        // Validate event before logging
        event.validate()?;

        // Serialize to JSON and append newline in one allocation.
        let mut line = serde_json::to_string(&event)
            .map_err(|e| AuditError::SerializationError(e.to_string()))?;
        line.push('\n');

        // Acquire file lock and write JSON+newline as a single syscall.
        let mut file = self.file.lock().await;
        file.write_all(line.as_bytes())
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to write event: {}", e)))?;

        file.sync_all()
            .await
            .map_err(|e| AuditError::FileError(format!("Failed to sync file: {}", e)))?;

        Ok(())
    }

    /// Query audit events from the file.
    ///
    /// Reads all events from the file and applies filters in memory.
    async fn query_events(&self, filters: AuditQueryFilters) -> AuditResult<Vec<AuditEvent>> {
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

        let paginated = events.into_iter().skip(offset).take(limit).collect();

        Ok(paginated)
    }
}

// Re-export for convenience
pub use super::AuditBackend;
