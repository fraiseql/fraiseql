#![allow(clippy::unwrap_used)] // Reason: test/bench code, panics are acceptable
//! Mock implementations for testing.

/// In-memory mock implementations of all webhook traits for use in unit and integration tests.
pub mod mocks {
    use std::{
        collections::HashMap,
        sync::{
            Mutex,
            atomic::{AtomicU64, Ordering},
        },
    };

    use crate::{
        Clock, Result, SecretProvider, SignatureVerifier, WebhookError, signature::SignatureError,
    };

    /// Mock signature verifier that always succeeds or fails based on configuration.
    ///
    /// Constructed with [`MockSignatureVerifier::succeeding`] or
    /// [`MockSignatureVerifier::failing`]. All calls to `verify` are recorded and can be
    /// retrieved with [`MockSignatureVerifier::get_calls`].
    pub struct MockSignatureVerifier {
        /// Whether `verify` should return `Ok(true)` or `Ok(false)`.
        pub should_succeed: bool,
        /// Ordered record of every `verify` invocation made against this mock.
        pub calls:          Mutex<Vec<MockVerifyCall>>,
    }

    /// A single recorded invocation of [`MockSignatureVerifier::verify`].
    #[derive(Debug, Clone)]
    pub struct MockVerifyCall {
        /// The raw request body passed to `verify`.
        pub payload:   Vec<u8>,
        /// The signature string passed to `verify`.
        pub signature: String,
    }

    impl MockSignatureVerifier {
        /// Create a verifier that returns `Ok(true)` for every call to `verify`.
        #[must_use]
        pub fn succeeding() -> Self {
            Self {
                should_succeed: true,
                calls:          Mutex::new(Vec::new()),
            }
        }

        /// Create a verifier that returns `Ok(false)` for every call to `verify`.
        #[must_use]
        pub fn failing() -> Self {
            Self {
                should_succeed: false,
                calls:          Mutex::new(Vec::new()),
            }
        }

        /// Return a snapshot of all `verify` calls recorded so far.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned (a prior panic occurred
        /// while the lock was held).
        #[must_use]
        pub fn get_calls(&self) -> Vec<MockVerifyCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl SignatureVerifier for MockSignatureVerifier {
        fn name(&self) -> &'static str {
            "mock"
        }

        fn signature_header(&self) -> &'static str {
            "X-Mock-Signature"
        }

        fn verify(
            &self,
            payload: &[u8],
            signature: &str,
            _secret: &str,
            _timestamp: Option<&str>,
            _url: Option<&str>,
        ) -> std::result::Result<bool, SignatureError> {
            self.calls.lock().unwrap().push(MockVerifyCall {
                payload:   payload.to_vec(),
                signature: signature.to_string(),
            });
            Ok(self.should_succeed)
        }
    }

    /// Mock secret provider with configurable secrets
    pub struct MockSecretProvider {
        secrets: HashMap<String, String>,
    }

    impl MockSecretProvider {
        /// Create a secret provider with no pre-configured secrets.
        #[must_use]
        pub fn new() -> Self {
            Self {
                secrets: HashMap::new(),
            }
        }

        /// Register a named secret value, returning `self` to enable builder-style chaining.
        #[must_use]
        pub fn with_secret(mut self, name: &str, value: &str) -> Self {
            self.secrets.insert(name.to_string(), value.to_string());
            self
        }
    }

    impl Default for MockSecretProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    impl SecretProvider for MockSecretProvider {
        async fn get_secret(&self, name: &str) -> Result<String> {
            self.secrets
                .get(name)
                .cloned()
                .ok_or_else(|| WebhookError::MissingSecret(name.to_string()))
        }
    }

    /// Mock clock for testing timestamp validation
    pub struct MockClock {
        current_time: AtomicU64,
    }

    impl MockClock {
        /// Create a clock frozen at the given Unix timestamp (seconds since epoch).
        #[must_use]
        pub fn new(timestamp: u64) -> Self {
            Self {
                current_time: AtomicU64::new(timestamp),
            }
        }

        /// Advance the clock forward by `seconds`, simulating elapsed time.
        pub fn advance(&self, seconds: u64) {
            self.current_time.fetch_add(seconds, Ordering::SeqCst);
        }

        /// Overwrite the current timestamp with the given Unix timestamp value.
        pub fn set(&self, timestamp: u64) {
            self.current_time.store(timestamp, Ordering::SeqCst);
        }
    }

    impl Clock for MockClock {
        fn now(&self) -> i64 {
            self.current_time.load(Ordering::SeqCst) as i64
        }
    }
}
