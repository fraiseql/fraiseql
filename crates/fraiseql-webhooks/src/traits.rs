//! Testing seams for webhook dependencies.
//!
//! All external dependencies are abstracted behind traits for easy testing.

use serde_json::Value;
use sqlx::{Postgres, Transaction};

use super::{Result, signature::SignatureError};

/// Signature verification abstraction for testing
pub trait SignatureVerifier: Send + Sync {
    /// Provider name (e.g., "stripe", "github")
    fn name(&self) -> &'static str;

    /// Header name containing the signature
    fn signature_header(&self) -> &'static str;

    /// Verify the signature
    ///
    /// # Arguments
    ///
    /// * `payload` - Raw request body bytes
    /// * `signature` - Signature from header
    /// * `secret` - Webhook signing secret
    /// * `timestamp` - Optional timestamp from headers (for replay protection)
    /// * `url` - Full request URL (required by Twilio; ignored by most providers)
    ///
    /// # Returns
    ///
    /// `Ok(true)` if signature is valid, `Ok(false)` if invalid, `Err` for format errors
    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        timestamp: Option<&str>,
        url: Option<&str>,
    ) -> std::result::Result<bool, SignatureError>;

    /// Optional: Extract timestamp from signature or headers
    fn extract_timestamp(&self, _signature: &str) -> Option<i64> {
        None
    }
}

/// Idempotency store abstraction for testing
#[allow(async_fn_in_trait)] // Reason: trait is used with concrete types only, not dyn Trait
pub trait IdempotencyStore: Send + Sync {
    /// Check if event has already been processed
    async fn check(&self, provider: &str, event_id: &str) -> Result<bool>;

    /// Record processed event
    async fn record(
        &self,
        provider: &str,
        event_id: &str,
        event_type: &str,
        status: &str,
    ) -> Result<uuid::Uuid>;

    /// Update event status
    async fn update_status(
        &self,
        provider: &str,
        event_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()>;
}

/// Secret provider abstraction for testing
#[allow(async_fn_in_trait)] // Reason: trait is used with concrete types only, not dyn Trait
pub trait SecretProvider: Send + Sync {
    /// Get webhook secret by name
    async fn get_secret(&self, name: &str) -> Result<String>;
}

/// Event handler abstraction for testing
#[allow(async_fn_in_trait)] // Reason: trait is used with concrete types only, not dyn Trait
pub trait EventHandler: Send + Sync {
    /// Handle webhook event by calling database function
    async fn handle(
        &self,
        function_name: &str,
        params: Value,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Value>;
}

/// Clock abstraction for testing timestamp validation
pub trait Clock: Send + Sync {
    /// Get current Unix timestamp
    fn now(&self) -> i64;
}

/// Production `Clock` implementation that delegates to `std::time::SystemTime`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs() as i64
    }
}
