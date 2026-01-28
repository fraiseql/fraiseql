//! Dead Letter Queue (DLQ) operations for job queue.
//!
//! The DLQ stores jobs that have failed permanently (exhausted all retry attempts).
//! This allows manual inspection and recovery of failed jobs.

use redis::aio::ConnectionManager;
use uuid::Uuid;

use super::Job;
use crate::error::Result;

/// Dead Letter Queue operations
#[derive(Clone)]
pub struct DeadLetterQueueManager {
    conn: ConnectionManager,
}

impl DeadLetterQueueManager {
    /// Create a new DLQ manager
    #[must_use]
    pub const fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    /// DLQ key in Redis
    const fn dlq_key() -> &'static str {
        "queue:dlq"
    }

    /// Job storage key
    fn job_key(job_id: Uuid) -> String {
        format!("job:{job_id}")
    }

    /// DLQ member key
    fn dlq_member(job_id: Uuid) -> String {
        format!("dlq:{job_id}")
    }

    /// Get all jobs in the DLQ
    ///
    /// # Returns
    ///
    /// Vector of jobs currently in the DLQ
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn list_all(&self) -> Result<Vec<Job>> {
        let members: Vec<String> = redis::cmd("LRANGE")
            .arg(Self::dlq_key())
            .arg(0)
            .arg(-1)
            .query_async(&mut self.conn.clone())
            .await?;

        let mut jobs = Vec::new();

        for member in members {
            // Extract job ID from member (format: "dlq:{job_id}")
            if let Some(job_id_str) = member.strip_prefix("dlq:") {
                if let Ok(job_id) = Uuid::parse_str(job_id_str) {
                    if let Ok(Some(job_json)) = self.get_job(job_id).await {
                        if let Ok(job) = serde_json::from_str::<Job>(&job_json) {
                            jobs.push(job);
                        }
                    }
                }
            }
        }

        Ok(jobs)
    }

    /// Get a specific job from the DLQ by ID
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the job to retrieve
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn get(&self, job_id: Uuid) -> Result<Option<Job>> {
        let job_json = self.get_job(job_id).await?;

        match job_json {
            Some(json) => {
                let job = serde_json::from_str::<Job>(&json)
                    .map_err(|e| crate::error::ObserverError::SerializationError(e.to_string()))?;
                Ok(Some(job))
            },
            None => Ok(None),
        }
    }

    /// Get count of jobs in the DLQ
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn count(&self) -> Result<usize> {
        let count: usize = redis::cmd("LLEN")
            .arg(Self::dlq_key())
            .query_async(&mut self.conn.clone())
            .await?;

        Ok(count)
    }

    /// Get the size of the DLQ (same as count, for compatibility)
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn size(&self) -> Result<usize> {
        self.count().await
    }

    /// Remove a job from the DLQ (for manual retry or cleanup)
    ///
    /// # Arguments
    ///
    /// * `job_id` - ID of the job to remove
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn remove(&self, job_id: Uuid) -> Result<()> {
        // Remove from DLQ list
        redis::cmd("LREM")
            .arg(Self::dlq_key())
            .arg(0) // Remove all occurrences
            .arg(Self::dlq_member(job_id))
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        // Remove job data
        redis::cmd("DEL")
            .arg(Self::job_key(job_id))
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    /// Clear all jobs from the DLQ (dangerous operation)
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn clear(&self) -> Result<()> {
        // Get all members to also delete job data
        let members: Vec<String> = redis::cmd("LRANGE")
            .arg(Self::dlq_key())
            .arg(0)
            .arg(-1)
            .query_async(&mut self.conn.clone())
            .await?;

        // Delete job data for each member
        for member in members {
            if let Some(job_id_str) = member.strip_prefix("dlq:") {
                if let Ok(job_id) = Uuid::parse_str(job_id_str) {
                    redis::cmd("DEL")
                        .arg(Self::job_key(job_id))
                        .query_async::<_, ()>(&mut self.conn.clone())
                        .await?;
                }
            }
        }

        // Clear the DLQ list
        redis::cmd("DEL")
            .arg(Self::dlq_key())
            .query_async::<_, ()>(&mut self.conn.clone())
            .await?;

        Ok(())
    }

    /// Get DLQ statistics
    ///
    /// # Returns
    ///
    /// Statistics about the current DLQ state
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn stats(&self) -> Result<DlqStats> {
        let count = self.count().await?;
        let all_jobs = self.list_all().await?;

        let mut by_action_type: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for job in &all_jobs {
            *by_action_type.entry(job.action_type().to_string()).or_insert(0) += 1;
        }

        Ok(DlqStats {
            total_jobs: count,
            by_action_type,
        })
    }

    /// Internal helper to get job JSON
    async fn get_job(&self, job_id: Uuid) -> Result<Option<String>> {
        let json: Option<String> = redis::cmd("GET")
            .arg(Self::job_key(job_id))
            .query_async(&mut self.conn.clone())
            .await?;

        Ok(json)
    }
}

/// Statistics about the Dead Letter Queue
#[derive(Debug, Clone)]
pub struct DlqStats {
    /// Total number of jobs in the DLQ
    pub total_jobs:     usize,
    /// Count of jobs by action type
    pub by_action_type: std::collections::HashMap<String, usize>,
}

impl std::fmt::Display for DlqStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DLQ Stats: {} total jobs", self.total_jobs)?;

        if !self.by_action_type.is_empty() {
            write!(f, " (")?;
            let parts: Vec<String> = self
                .by_action_type
                .iter()
                .map(|(action_type, count)| format!("{action_type}: {count}"))
                .collect();
            write!(f, "{})", parts.join(", "))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dlq_key_generation() {
        assert_eq!(DeadLetterQueueManager::dlq_key(), "queue:dlq");

        let job_id = Uuid::nil();
        assert!(DeadLetterQueueManager::job_key(job_id).starts_with("job:"));
        assert!(DeadLetterQueueManager::dlq_member(job_id).starts_with("dlq:"));
    }

    #[test]
    fn test_dlq_manager_clone() {
        // Ensure DeadLetterQueueManager is Clone-able
        fn assert_clone<T: Clone>() {}
        assert_clone::<DeadLetterQueueManager>();
    }

    #[test]
    fn test_dlq_stats_display() {
        let mut by_action_type = std::collections::HashMap::new();
        by_action_type.insert("webhook".to_string(), 5);
        by_action_type.insert("email".to_string(), 3);

        let stats = DlqStats {
            total_jobs: 8,
            by_action_type,
        };

        let display_str = format!("{stats}");
        assert!(display_str.contains("8 total jobs"));
        assert!(display_str.contains("webhook"));
        assert!(display_str.contains("email"));
    }

    #[test]
    fn test_dlq_stats_empty() {
        let stats = DlqStats {
            total_jobs:     0,
            by_action_type: std::collections::HashMap::new(),
        };

        let display_str = format!("{stats}");
        assert_eq!(display_str, "DLQ Stats: 0 total jobs");
    }
}
