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
    /// # Errors
    ///
    /// Returns `SignatureError` if the signature format is invalid or cannot be parsed.
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

/// Atomic, transaction-scoped deduplication of inbound webhook deliveries.
///
/// A delivery is claimed *inside* the same transaction that runs its handler, so
/// the claim and the handler's effects commit or roll back together. This is the
/// only race-free shape: two concurrent duplicate deliveries serialise on the
/// unique-key row lock, exactly one wins, and a handler failure rolls the claim
/// back so the sender's retry reprocesses cleanly (no lost / double-processed
/// events). A check-then-record split (read outside the transaction, write after)
/// has a TOCTOU window where concurrent duplicates both pass the read and both
/// process — which is why this trait exposes a single atomic [`claim`] rather
/// than separate check / record calls.
///
/// [`claim`]: IdempotencyStore::claim
#[allow(async_fn_in_trait)] // Reason: trait is used with concrete types only, not dyn Trait
pub trait IdempotencyStore: Send + Sync {
    /// Atomically claim a `(provider, event_id)` delivery within the caller's
    /// transaction.
    ///
    /// Returns `Ok(Some(id))` when the delivery is newly claimed (the caller
    /// should process it) and `Ok(None)` when it was already claimed by an
    /// earlier committed delivery (a duplicate the caller must silently discard).
    ///
    /// The claim must be performed with the supplied transaction so that it is
    /// rolled back if the handler later fails — otherwise an event marked
    /// processed but not handled would be lost.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookError::Database`](crate::WebhookError::Database) if the
    /// claim query fails.
    async fn claim(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        provider: &str,
        event_id: &str,
        event_type: &str,
    ) -> Result<Option<uuid::Uuid>>;
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
