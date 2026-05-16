#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use chrono::Utc;

use super::*;

// ---------------------------------------------------------------------------
// Mock SecretsBackend for LeaseRenewalTask tests
// ---------------------------------------------------------------------------

/// A mock backend that returns a fixed secret with configurable expiry.
/// Rotation calls are counted so tests can assert how many renewals occurred.
struct MockBackend {
    secret:       String,
    expiry:       DateTime<Utc>,
    rotate_count: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl SecretsBackend for MockBackend {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn health_check(&self) -> Result<(), SecretsError> {
        Ok(())
    }

    async fn get_secret(&self, _name: &str) -> Result<String, SecretsError> {
        Ok(self.secret.clone())
    }

    async fn get_secret_with_expiry(
        &self,
        _name: &str,
    ) -> Result<(String, DateTime<Utc>), SecretsError> {
        Ok((self.secret.clone(), self.expiry))
    }

    async fn rotate_secret(&self, _name: &str) -> Result<String, SecretsError> {
        self.rotate_count.fetch_add(1, Ordering::SeqCst);
        Ok("rotated".to_string())
    }
}

// ---------------------------------------------------------------------------
// LeaseRenewalTask tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_lease_renewal_task_cancels_cleanly() {
    let rotate_count = Arc::new(AtomicUsize::new(0));
    let backend = MockBackend {
        secret:       "s3cret".to_string(),
        // Expiry far in the future — no renewal needed.
        expiry:       Utc::now() + chrono::Duration::hours(1),
        rotate_count: Arc::clone(&rotate_count),
    };
    let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

    let (task, cancel_tx) =
        LeaseRenewalTask::new(manager, vec!["db/creds".to_string()], Duration::from_secs(60));

    // Cancel immediately before the first sleep interval fires.
    cancel_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), task.run())
        .await
        .expect("task should exit quickly after cancellation");

    // No renewals should have occurred since we cancelled before the first tick.
    assert_eq!(rotate_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_lease_renewal_triggers_rotate_when_expiry_near() {
    let rotate_count = Arc::new(AtomicUsize::new(0));
    let backend = MockBackend {
        secret:       "s3cret".to_string(),
        // Already-expired credential: remaining is negative, which is always
        // less than the check_interval threshold, so renewal fires on every tick.
        // This works with any sub-second check_interval (where as_secs() == 0)
        // because negative < zero is true for chrono::Duration.
        expiry:       Utc::now() - chrono::Duration::seconds(1),
        rotate_count: Arc::clone(&rotate_count),
    };
    let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

    let (task, cancel_tx) = LeaseRenewalTask::new(
        manager,
        vec!["db/creds".to_string()],
        Duration::from_millis(50), // very short interval so the test is fast
    );

    let handle = tokio::spawn(task.run());

    // Wait long enough for at least one tick to fire.
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("task should exit quickly after cancellation")
        .unwrap();

    assert!(
        rotate_count.load(Ordering::SeqCst) >= 1,
        "expected at least one renewal for an expired credential"
    );
}

#[tokio::test]
async fn test_lease_renewal_skips_non_expiring_keys() {
    let rotate_count = Arc::new(AtomicUsize::new(0));
    let backend = MockBackend {
        secret:       "s3cret".to_string(),
        // Expiry 1 hour away — much longer than the check interval (50 ms).
        expiry:       Utc::now() + chrono::Duration::hours(1),
        rotate_count: Arc::clone(&rotate_count),
    };
    let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

    let (task, cancel_tx) =
        LeaseRenewalTask::new(manager, vec!["db/creds".to_string()], Duration::from_millis(50));

    let handle = tokio::spawn(task.run());
    tokio::time::sleep(Duration::from_millis(200)).await;
    cancel_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("task should exit quickly")
        .unwrap();

    assert_eq!(
        rotate_count.load(Ordering::SeqCst),
        0,
        "credentials with distant expiry should not be rotated"
    );
}

#[tokio::test]
async fn test_create_secrets_manager_file_backend() {
    let dir = tempfile::tempdir().unwrap();
    let secret_path = dir.path().join("db_password");
    tokio::fs::write(&secret_path, "s3cret").await.unwrap();

    let manager = create_secrets_manager(SecretsBackendConfig::File {
        path: dir.path().to_path_buf(),
    })
    .await
    .unwrap();

    let value = manager.get_secret("db_password").await.unwrap();
    assert_eq!(value, "s3cret");
}

#[tokio::test]
async fn test_create_secrets_manager_env_backend() {
    // Use a unique env var to avoid test interference
    let key = "FRAISEQL_TEST_SM_SECRET_FACTORY";
    temp_env::async_with_vars([(key, Some("env_value"))], async {
        let manager = create_secrets_manager(SecretsBackendConfig::Env).await.unwrap();
        let value = manager.get_secret(key).await.unwrap();
        assert_eq!(value, "env_value");
    })
    .await;
}
