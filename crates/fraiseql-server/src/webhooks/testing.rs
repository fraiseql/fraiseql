//! Mock implementations for testing.

pub mod mocks {
    use std::{
        collections::HashMap,
        sync::{
            Mutex,
            atomic::{AtomicU64, Ordering},
        },
    };

    use async_trait::async_trait;

    use crate::webhooks::{
        Clock, IdempotencyStore, Result, SecretProvider, SignatureVerifier, WebhookError,
        signature::SignatureError,
    };

    /// Mock signature verifier that always succeeds or fails based on configuration
    pub struct MockSignatureVerifier {
        pub should_succeed: bool,
        pub calls:          Mutex<Vec<MockVerifyCall>>,
    }

    #[derive(Debug, Clone)]
    pub struct MockVerifyCall {
        pub payload:   Vec<u8>,
        pub signature: String,
    }

    impl MockSignatureVerifier {
        #[must_use]
        pub fn succeeding() -> Self {
            Self {
                should_succeed: true,
                calls:          Mutex::new(Vec::new()),
            }
        }

        #[must_use]
        pub fn failing() -> Self {
            Self {
                should_succeed: false,
                calls:          Mutex::new(Vec::new()),
            }
        }

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
        ) -> std::result::Result<bool, SignatureError> {
            self.calls.lock().unwrap().push(MockVerifyCall {
                payload:   payload.to_vec(),
                signature: signature.to_string(),
            });
            Ok(self.should_succeed)
        }
    }

    /// Mock idempotency store with in-memory storage
    pub struct MockIdempotencyStore {
        events: Mutex<HashMap<(String, String), IdempotencyRecord>>,
    }

    #[derive(Debug, Clone)]
    pub struct IdempotencyRecord {
        pub id:         uuid::Uuid,
        pub event_type: String,
        pub status:     String,
        pub error:      Option<String>,
    }

    impl MockIdempotencyStore {
        #[must_use]
        pub fn new() -> Self {
            Self {
                events: Mutex::new(HashMap::new()),
            }
        }

        /// Pre-populate with existing events for testing duplicates
        #[must_use]
        pub fn with_existing_events(events: Vec<(&str, &str)>) -> Self {
            let store = Self::new();
            let mut map = store.events.lock().unwrap();
            for (provider, event_id) in events {
                map.insert(
                    (provider.to_string(), event_id.to_string()),
                    IdempotencyRecord {
                        id:         uuid::Uuid::new_v4(),
                        event_type: "test".to_string(),
                        status:     "success".to_string(),
                        error:      None,
                    },
                );
            }
            drop(map);
            store
        }

        #[must_use]
        pub fn get_record(&self, provider: &str, event_id: &str) -> Option<IdempotencyRecord> {
            self.events
                .lock()
                .unwrap()
                .get(&(provider.to_string(), event_id.to_string()))
                .cloned()
        }
    }

    impl Default for MockIdempotencyStore {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl IdempotencyStore for MockIdempotencyStore {
        async fn check(&self, provider: &str, event_id: &str) -> Result<bool> {
            Ok(self
                .events
                .lock()
                .unwrap()
                .contains_key(&(provider.to_string(), event_id.to_string())))
        }

        async fn record(
            &self,
            provider: &str,
            event_id: &str,
            event_type: &str,
            status: &str,
        ) -> Result<uuid::Uuid> {
            let id = uuid::Uuid::new_v4();
            self.events.lock().unwrap().insert(
                (provider.to_string(), event_id.to_string()),
                IdempotencyRecord {
                    id,
                    event_type: event_type.to_string(),
                    status: status.to_string(),
                    error: None,
                },
            );
            Ok(id)
        }

        async fn update_status(
            &self,
            provider: &str,
            event_id: &str,
            status: &str,
            error: Option<&str>,
        ) -> Result<()> {
            if let Some(record) = self
                .events
                .lock()
                .unwrap()
                .get_mut(&(provider.to_string(), event_id.to_string()))
            {
                record.status = status.to_string();
                record.error = error.map(std::string::ToString::to_string);
            }
            Ok(())
        }
    }

    /// Mock secret provider with configurable secrets
    pub struct MockSecretProvider {
        secrets: HashMap<String, String>,
    }

    impl MockSecretProvider {
        #[must_use]
        pub fn new() -> Self {
            Self {
                secrets: HashMap::new(),
            }
        }

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

    #[async_trait]
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
        #[must_use]
        pub fn new(timestamp: u64) -> Self {
            Self {
                current_time: AtomicU64::new(timestamp),
            }
        }

        pub fn advance(&self, seconds: u64) {
            self.current_time.fetch_add(seconds, Ordering::SeqCst);
        }

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
