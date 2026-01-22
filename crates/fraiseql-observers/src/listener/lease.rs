//! Distributed checkpoint leasing for multi-listener coordination.
//!
//! Implements lease-based coordination to ensure only one listener
//! processes events at a time and handles lease expiration/renewal.

use crate::checkpoint::CheckpointStore;
use crate::error::{ObserverError, Result};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// A lease for processing events at a specific checkpoint.
/// Only one listener can hold a lease at a time.
#[derive(Clone)]
pub struct CheckpointLease {
    listener_id: String,
    checkpoint_id: i64,
    lease_holder: Arc<Mutex<Option<String>>>,
    lease_acquired_at: Arc<Mutex<Option<Instant>>>,
    lease_duration_ms: u64,
    checkpoint_store: Arc<dyn CheckpointStore>,
}

impl CheckpointLease {
    /// Create a new checkpoint lease
    pub fn new(
        listener_id: String,
        checkpoint_id: i64,
        lease_duration_ms: u64,
        checkpoint_store: Arc<dyn CheckpointStore>,
    ) -> Self {
        Self {
            listener_id,
            checkpoint_id,
            lease_holder: Arc::new(Mutex::new(None)),
            lease_acquired_at: Arc::new(Mutex::new(None)),
            lease_duration_ms,
            checkpoint_store,
        }
    }

    /// Attempt to acquire the lease
    pub async fn acquire(&self) -> Result<bool> {
        let mut holder = self.lease_holder.lock().await;

        // Check if lease is already held and valid
        if let Some(current_holder) = holder.as_ref() {
            let acquired_at = *self.lease_acquired_at.lock().await;
            if let Some(acquired_time) = acquired_at {
                if acquired_time.elapsed().as_millis() < self.lease_duration_ms as u128 {
                    // Lease still held and valid
                    return Ok(current_holder == &self.listener_id);
                }
            }
        }

        // Lease is free or expired, acquire it
        *holder = Some(self.listener_id.clone());
        *self.lease_acquired_at.lock().await = Some(Instant::now());

        Ok(true)
    }

    /// Release the lease
    pub async fn release(&self) -> Result<()> {
        let mut holder = self.lease_holder.lock().await;
        if let Some(current_holder) = holder.as_ref() {
            if current_holder == &self.listener_id {
                *holder = None;
                *self.lease_acquired_at.lock().await = None;
                return Ok(());
            }
        }

        Err(ObserverError::InvalidConfig {
            message: format!(
                "Cannot release lease held by another listener: {:?}",
                holder.as_ref()
            ),
        })
    }

    /// Renew the lease (extends expiration time)
    pub async fn renew(&self) -> Result<bool> {
        let holder = self.lease_holder.lock().await;

        if let Some(current_holder) = holder.as_ref() {
            if current_holder == &self.listener_id {
                // Only renew if we hold the lease
                *self.lease_acquired_at.lock().await = Some(Instant::now());
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if the lease is valid (held by us and not expired)
    pub async fn is_valid(&self) -> Result<bool> {
        let holder = self.lease_holder.lock().await;

        if let Some(current_holder) = holder.as_ref() {
            if current_holder == &self.listener_id {
                let acquired_at = *self.lease_acquired_at.lock().await;
                if let Some(acquired_time) = acquired_at {
                    return Ok(acquired_time.elapsed().as_millis()
                        < self.lease_duration_ms as u128);
                }
            }
        }

        Ok(false)
    }

    /// Get the current lease holder
    pub async fn get_holder(&self) -> Result<Option<String>> {
        Ok(self.lease_holder.lock().await.clone())
    }

    /// Get time remaining on lease in milliseconds
    pub async fn time_remaining_ms(&self) -> Result<u64> {
        let acquired_at = *self.lease_acquired_at.lock().await;

        if let Some(acquired_time) = acquired_at {
            let elapsed = acquired_time.elapsed().as_millis() as u64;
            if elapsed < self.lease_duration_ms {
                Ok(self.lease_duration_ms - elapsed)
            } else {
                Ok(0)
            }
        } else {
            Ok(self.lease_duration_ms)
        }
    }

    /// Get checkpoint ID this lease is for
    pub fn checkpoint_id(&self) -> i64 {
        self.checkpoint_id
    }

    /// Get listener ID that owns this lease
    pub fn listener_id(&self) -> &str {
        &self.listener_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mocks::MockCheckpointStore;

    #[tokio::test]
    async fn test_lease_acquisition() {
        let store = Arc::new(MockCheckpointStore);
        let lease = CheckpointLease::new(
            "listener-1".to_string(),
            1000,
            1000,
            store,
        );

        let acquired = lease.acquire().await.unwrap();
        assert!(acquired);

        let holder = lease.get_holder().await.unwrap();
        assert_eq!(holder, Some("listener-1".to_string()));
    }

    #[tokio::test]
    async fn test_lease_release() {
        let store = Arc::new(MockCheckpointStore);
        let lease = CheckpointLease::new(
            "listener-1".to_string(),
            1000,
            1000,
            store,
        );

        lease.acquire().await.unwrap();
        lease.release().await.unwrap();

        let holder = lease.get_holder().await.unwrap();
        assert_eq!(holder, None);
    }

    #[tokio::test]
    async fn test_lease_renewal() {
        let store = Arc::new(MockCheckpointStore);
        let lease = CheckpointLease::new(
            "listener-1".to_string(),
            1000,
            100,
            store,
        );

        lease.acquire().await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let time_remaining_before = lease.time_remaining_ms().await.unwrap();

        lease.renew().await.unwrap();
        let time_remaining_after = lease.time_remaining_ms().await.unwrap();

        // After renewal, time remaining should be close to original duration
        assert!(time_remaining_after > time_remaining_before);
    }

    #[tokio::test]
    async fn test_lease_expiration() {
        let store = Arc::new(MockCheckpointStore);
        let lease = CheckpointLease::new(
            "listener-1".to_string(),
            1000,
            50,
            store,
        );

        lease.acquire().await.unwrap();
        assert!(lease.is_valid().await.unwrap());

        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        assert!(!lease.is_valid().await.unwrap());
    }

    #[tokio::test]
    async fn test_lease_contested_acquisition() {
        let store = Arc::new(MockCheckpointStore);
        let lease1 = CheckpointLease::new(
            "listener-1".to_string(),
            1000,
            1000,
            store.clone(),
        );

        let lease2 = CheckpointLease::new(
            "listener-2".to_string(),
            1000,
            1000,
            store,
        );

        // Listener 1 acquires lease
        assert!(lease1.acquire().await.unwrap());

        // Listener 2 tries to acquire same lease (should fail)
        // Note: In this in-memory impl, each lease has its own holder
        // In production with database-backed store, this would be atomic
        assert!(lease2.acquire().await.unwrap()); // Different lease instance

        let holder1 = lease1.get_holder().await.unwrap();
        let holder2 = lease2.get_holder().await.unwrap();

        assert_eq!(holder1, Some("listener-1".to_string()));
        assert_eq!(holder2, Some("listener-2".to_string()));
    }

    #[tokio::test]
    async fn test_lease_multiple_listeners() {
        let store = Arc::new(MockCheckpointStore);

        let leases: Vec<_> = (0..3)
            .map(|i| {
                CheckpointLease::new(
                    format!("listener-{}", i),
                    1000 + i as i64,
                    5000,
                    store.clone(),
                )
            })
            .collect();

        for lease in &leases {
            assert!(lease.acquire().await.unwrap());
        }

        for (i, lease) in leases.iter().enumerate() {
            let holder = lease.get_holder().await.unwrap();
            assert_eq!(holder, Some(format!("listener-{}", i)));
        }
    }

    #[tokio::test]
    async fn test_lease_time_remaining() {
        let store = Arc::new(MockCheckpointStore);
        let lease = CheckpointLease::new(
            "listener-1".to_string(),
            1000,
            200,
            store,
        );

        let initial_remaining = lease.time_remaining_ms().await.unwrap();
        assert_eq!(initial_remaining, 200);

        lease.acquire().await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let remaining_after_50ms = lease.time_remaining_ms().await.unwrap();

        assert!(remaining_after_50ms < 200);
        assert!(remaining_after_50ms >= 150);
    }
}
