#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::panic)] // Reason: NeverHandler panics loudly if the pipeline wrongly reaches it

use serde_json::Value;
use sqlx::{Postgres, Transaction, postgres::PgPoolOptions};

use super::*;
use crate::{
    StaticSecretProvider, idempotency::PostgresIdempotencyStore,
    testing::mocks::MockSignatureVerifier,
};

/// A handler that must never run in these (pre-transaction) short-circuit tests.
/// If the pipeline reached the transaction stage, this would panic and fail loudly.
struct NeverHandler;

impl EventHandler for NeverHandler {
    async fn handle(
        &self,
        _function_name: &str,
        _params: Value,
        _tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Handled> {
        panic!("handler must not run when the pipeline short-circuits before the transaction");
    }
}

/// Borrowed by `InboundRequest`, so it outlives every delivery built below.
static NO_HEADERS: std::sync::LazyLock<std::collections::BTreeMap<String, String>> =
    std::sync::LazyLock::new(std::collections::BTreeMap::new);

fn delivery() -> Delivery<'static> {
    Delivery {
        route:         "stripe",
        function_name: "process_payment",
        request:       InboundRequest::new(&NO_HEADERS, b"{}", None),
    }
}

/// The `event_of` for tests that must not reach it: every case below
/// short-circuits before verification succeeds, so reaching this is the failure
/// it is written to catch.
fn never_read(_authenticated: Authenticated<'_>) -> Result<VerifiedEvent> {
    panic!("the event must not be read when the pipeline short-circuits before verifying")
}

#[tokio::test]
async fn verify_signature_accepts_a_valid_signature() {
    let verifier = MockSignatureVerifier::succeeding();
    assert!(verify_signature(&verifier, Some("secret"), &delivery()).await.is_ok());
}

#[tokio::test]
async fn verify_signature_rejects_a_mismatch_as_signature_invalid() {
    let verifier = MockSignatureVerifier::failing();
    let err = verify_signature(&verifier, Some("secret"), &delivery()).await.unwrap_err();
    assert!(
        matches!(err, WebhookError::SignatureInvalid(_)),
        "a mismatched signature must be SignatureInvalid, got: {err:?}",
    );
}

/// A route whose scheme has a shared secret, reached with none, must not verify —
/// and must report it as the **operator's** error, not the sender's.
///
/// Unreachable through a mounted route, because `check_key_material` refuses that
/// combination at boot. It is asserted here because the trait is this crate's
/// advertised extension point: an embedder driving it directly gets a 5xx naming
/// its own mistake rather than a verification against something unintended.
#[tokio::test]
async fn verify_signature_with_no_secret_is_the_operators_error() {
    let verifier = MockSignatureVerifier::succeeding();
    let err = verify_signature(&verifier, None, &delivery()).await.unwrap_err();
    assert!(
        matches!(err, WebhookError::KeyMaterial(_)),
        "a secret-needing scheme reached without one is KeyMaterial (a 5xx), never \
         SignatureInvalid (a 401 blaming the sender): {err:?}",
    );
}

/// A bogus, never-connectable pool. `connect_lazy` does not dial until first use,
/// so any test that returns before the transaction stage never touches it. If the
/// pipeline *did* reach `pool.begin()`, the call would surface a connection error
/// (mapped to `WebhookError::Database`), not the short-circuit error we assert.
fn unreachable_pool() -> sqlx::PgPool {
    PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@127.0.0.1:1/unused")
        .unwrap()
}

#[tokio::test]
async fn forged_signature_is_rejected_before_any_database_work() {
    let pool = unreachable_pool();
    let pipeline = WebhookPipeline::new(
        pool.clone(),
        StaticSecretProvider::new().with_secret("stripe", "whsec"),
        PostgresIdempotencyStore::new(pool),
        NeverHandler,
    );

    let verifier = MockSignatureVerifier::failing();
    let err = pipeline
        .process(&verifier, Some("stripe"), &delivery(), never_read)
        .await
        .unwrap_err();

    assert!(
        matches!(err, WebhookError::SignatureInvalid(_)),
        "a forged signature must short-circuit with SignatureInvalid (no DB), got: {err:?}",
    );
}

#[tokio::test]
async fn missing_secret_is_rejected_before_any_database_work() {
    let pool = unreachable_pool();
    let pipeline = WebhookPipeline::new(
        pool.clone(),
        StaticSecretProvider::new(), // no secrets registered
        PostgresIdempotencyStore::new(pool),
        NeverHandler,
    );

    // A succeeding verifier proves the rejection is the missing secret, resolved
    // before verification — not a signature failure.
    let verifier = MockSignatureVerifier::succeeding();
    let err = pipeline
        .process(&verifier, Some("stripe"), &delivery(), never_read)
        .await
        .unwrap_err();

    assert!(
        matches!(&err, WebhookError::MissingSecret(name) if name == "stripe"),
        "an unknown secret must short-circuit with MissingSecret (no DB), got: {err:?}",
    );
}
