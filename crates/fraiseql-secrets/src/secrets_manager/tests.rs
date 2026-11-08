#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use super::*;

// ---------------------------------------------------------------------------
// Mock SecretsBackend for LeaseRenewalTask tests
// ---------------------------------------------------------------------------

/// A mock backend that returns a fixed secret with configurable expiry.
/// Rotation calls are counted so tests can assert how many renewals occurred.
struct MockBackend {
    secret:       String,
    expiry:       Timestamp,
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

    async fn get_secret(&self, _name: &str) -> Result<Secret, SecretsError> {
        Ok(Secret::new(self.secret.clone()))
    }

    async fn get_secret_with_expiry(
        &self,
        _name: &str,
    ) -> Result<(Secret, Timestamp), SecretsError> {
        Ok((Secret::new(self.secret.clone()), self.expiry))
    }

    async fn rotate_secret(&self, _name: &str) -> Result<Secret, SecretsError> {
        self.rotate_count.fetch_add(1, Ordering::SeqCst);
        Ok(Secret::new("rotated".to_string()))
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
        expiry:       Timestamp::now() + SignedDuration::from_hours(1),
        rotate_count: Arc::clone(&rotate_count),
    };
    let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

    let (task, cancel_tx) =
        LeaseRenewalTask::new(manager, vec!["db/creds".to_string()], Duration::from_mins(1));

    // Cancel immediately before the first sleep interval fires.
    cancel_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(2), task.run())
        .await
        .expect("task should exit quickly after cancellation");

    // No renewals should have occurred since we cancelled before the first tick.
    assert_eq!(rotate_count.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn test_lease_renewal_triggers_rotate_when_expiry_near() {
    let rotate_count = Arc::new(AtomicUsize::new(0));
    let backend = MockBackend {
        secret:       "s3cret".to_string(),
        // Already-expired credential: remaining is negative, which is always
        // less than the check_interval threshold, so renewal fires on every tick.
        // This works with any sub-second check_interval (where as_secs() == 0)
        // because negative < zero is true for a SignedDuration.
        expiry:       Timestamp::now() - SignedDuration::from_secs(1),
        rotate_count: Arc::clone(&rotate_count),
    };
    let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

    let (task, cancel_tx) = LeaseRenewalTask::new(
        manager,
        vec!["db/creds".to_string()],
        Duration::from_millis(50), // very short interval so the test is fast
    );

    let handle = tokio::spawn(task.run());

    // Let the task reach `tokio::select!` and arm its first `tokio::time::sleep` timer
    // (with start_paused, the timer is registered against the frozen clock).
    tokio::task::yield_now().await;

    // Advance past the check interval so the sleep timer becomes ready.
    tokio::time::advance(Duration::from_millis(200)).await;

    // Drain the task through its renewal cycle BEFORE signalling cancellation.
    // The `select!` in `LeaseRenewalTask::run` is not `biased`, so if both the
    // timer and the cancel branch are ready when the task is next polled, tokio
    // picks pseudo-randomly — and the cancel branch would win ~50% of the time,
    // skipping the renewal. Multiple yields force the renewal future to complete
    // and return to `select!` before we send cancel.
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }

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

#[tokio::test(start_paused = true)]
async fn test_lease_renewal_skips_non_expiring_keys() {
    let rotate_count = Arc::new(AtomicUsize::new(0));
    let backend = MockBackend {
        secret:       "s3cret".to_string(),
        // Expiry 1 hour away — much longer than the check interval (50 ms).
        expiry:       Timestamp::now() + SignedDuration::from_hours(1),
        rotate_count: Arc::clone(&rotate_count),
    };
    let manager = Arc::new(SecretsManager::new(Arc::new(backend)));

    let (task, cancel_tx) =
        LeaseRenewalTask::new(manager, vec!["db/creds".to_string()], Duration::from_millis(50));

    let handle = tokio::spawn(task.run());
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(200)).await;
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

// ---------------------------------------------------------------------------
// #727: tls_verify = false is refused in production
// ---------------------------------------------------------------------------

#[test]
fn vault_tls_verify_off_is_refused_in_production() {
    let err = vault_tls_verify_check(false, Deployment::Production)
        .expect_err("tls_verify = false must refuse to boot in production");
    assert!(
        err.to_string().contains("tls_verify"),
        "the refusal must name the knob; got: {err}"
    );
}

/// The entry point reads the deployment itself: with nothing declaring it, a Vault backend
/// with `tls_verify = false` is refused before any connection, and a declared development
/// environment goes on to build it.
#[tokio::test]
async fn create_secrets_manager_refuses_vault_tls_verify_off_when_nothing_declares_the_deployment()
{
    let vault = || SecretsBackendConfig::Vault {
        addr:       "https://127.0.0.1:1".to_string(),
        auth:       VaultAuth::Token(Zeroizing::new("token".to_string())),
        namespace:  None,
        tls_verify: false,
    };
    let undeclared: [(&str, Option<&str>); 3] = [
        ("FRAISEQL_ENV", None),
        ("FRAISEQL_PROFILE", None),
        ("KUBERNETES_SERVICE_HOST", None),
    ];
    let refused = temp_env::async_with_vars(undeclared, create_secrets_manager(vault())).await;
    let err = refused.err().map(|e| e.to_string());
    assert!(
        err.as_deref().is_some_and(|e| e.contains("tls_verify")),
        "production must refuse tls_verify = false; got: {err:?}"
    );

    let development: [(&str, Option<&str>); 3] = [
        ("FRAISEQL_ENV", Some("development")),
        ("FRAISEQL_PROFILE", None),
        ("KUBERNETES_SERVICE_HOST", None),
    ];
    let built = temp_env::async_with_vars(development, create_secrets_manager(vault())).await;
    let err = built.err().map(|e| e.to_string());
    assert!(
        !err.as_deref().is_some_and(|e| e.contains("tls_verify")),
        "development passes the TLS check; got: {err:?}"
    );
}

#[test]
fn vault_tls_verify_off_is_a_warning_in_development() {
    assert!(vault_tls_verify_check(false, Deployment::Development).is_ok());
}

#[test]
fn vault_tls_verify_on_is_always_ok() {
    assert!(vault_tls_verify_check(true, Deployment::Production).is_ok());
    assert!(vault_tls_verify_check(true, Deployment::Development).is_ok());
}

// ── The Vault address is logged without its credentials ──────────────────────

#[tokio::test]
async fn the_vault_setup_log_hides_the_password() {
    use std::sync::{Arc as StdArc, Mutex};

    #[derive(Clone, Default)]
    struct Captured(StdArc<Mutex<Vec<u8>>>);
    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    const SECRET: &str = "pw-vault-5d21";
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // The address guard runs before the setup line; hold the temp_env lock with the
    // opt-in and posture markers cleared so a sibling cannot change what it reads.
    temp_env::async_with_vars(
        [
            ("FRAISEQL_VAULT_ALLOW_INSECURE", None::<&str>),
            ("FRAISEQL_ENV", None),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        async {
            let guard = fraiseql_test_support::tracing_capture::set_default(subscriber);
            let _ = create_secrets_manager(SecretsBackendConfig::Vault {
                addr:       format!("https://ops:{SECRET}@vault.example.com:8200"),
                auth:       VaultAuth::Token(zeroize::Zeroizing::new("token".to_string())),
                namespace:  None,
                tls_verify: true,
            })
            .await;
            drop(guard);
        },
    )
    .await;
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("Initializing Vault secrets backend"),
        "the setup log was not reached:\n{log}"
    );
    assert!(
        log.contains("vault.example.com"),
        "the setup log no longer names the host:\n{log}"
    );
    assert!(!log.contains(SECRET), "the setup log wrote the password:\n{log}");
}
