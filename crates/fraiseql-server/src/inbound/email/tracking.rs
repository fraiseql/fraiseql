//! The delivery-feedback stores: send-status lifecycle + suppression list.
//!
//! Where the [`warming::SendCounter`](super::warming::SendCounter) seam gates a
//! send by volume, the [`SendTracker`] seam gates it by *outcome history* and
//! records the send so an inbound bounce/challenge/reply can later be correlated
//! back to it. The `send_email` transport consults it before every relay:
//!
//! 1. **suppression** — is the recipient on the do-not-contact list? (a permanent refusal, the
//!    biggest deliverability + GDPR lever);
//! 2. **exactly-once** — has this exact dispatch (`send_id`) already been sent? (a durable retry
//!    must not double-send);
//!
//! and writes a `Sent` row after a successful relay so the correlation step
//! (cycle 3) has something to transition.
//!
//! The Postgres implementation ([`PgSendTracker`]) is the server-side owner of the
//! `_fraiseql_send_status` / `_fraiseql_suppression` tables (DDL in
//! [`fraiseql_functions::migrations::send_tracking_migration_sql`]); the seam keeps
//! the transport testable without a database. Addresses reach the tracker already
//! **keyed-hashed** (never raw) so the store holds no recipient PII on the
//! suppression list — see
//! [`hash_address`](fraiseql_observers::hash_address).

use std::{future::Future, pin::Pin};

use fraiseql_error::Result;
use sqlx::PgPool;

/// Why a recipient is on the suppression list.
///
/// Serialised to the `reason` column as a stable snake-case token; a granular
/// reason (rather than a bare boolean) lets the correlation step lift only the
/// right kind of suppression — a reply lifts a `challenge_unanswered`, never a
/// `hard_bounce`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SuppressionReason {
    /// A hard bounce (5xx / DSN): the address does not accept mail. ~Permanent.
    HardBounce,
    /// N unanswered challenge-response prompts (Mailinblack-style). Lifts on a
    /// genuine reply; a ~30-day TTL otherwise.
    ChallengeUnanswered,
    /// A manual unsubscribe / support removal. Permanent unless re-consented.
    Unsubscribe,
}

impl SuppressionReason {
    /// The stable token stored in the `reason` column.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SuppressionReason::HardBounce => "hard_bounce",
            SuppressionReason::ChallengeUnanswered => "challenge_unanswered",
            SuppressionReason::Unsubscribe => "unsubscribe",
        }
    }

    /// Parse a stored `reason` token back into a variant, or `None` if unknown
    /// (a forward-compatible reason written by a newer version).
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "hard_bounce" => Some(SuppressionReason::HardBounce),
            "challenge_unanswered" => Some(SuppressionReason::ChallengeUnanswered),
            "unsubscribe" => Some(SuppressionReason::Unsubscribe),
            _ => None,
        }
    }
}

impl std::fmt::Display for SuppressionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A prior send recorded on the send-status store — the exactly-once skip returns
/// this instead of relaying again.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordedSend {
    /// The relay/provider message id recorded on the original send, if any.
    pub message_id: Option<String>,
}

/// The write payload for a successful relay: what the transport records as `Sent`.
///
/// The `recipient` is stored **raw** here (operational history the app surfaces —
/// "sent to bob@…, status Bounced"); it is erasable PII. The suppression list, by
/// contrast, keeps only a keyed hash (the do-not-contact fact that must survive
/// erasure). The `tenant` scopes the row for RLS.
#[derive(Debug, Clone, Copy)]
pub struct SentRecord<'a> {
    /// The per-dispatch VERP send-id (the exactly-once key).
    pub send_id:         &'a str,
    /// The tenant the send is scoped to (RLS stamp). `None` → single-tenant.
    pub tenant:          Option<&'a str>,
    /// The raw recipient address.
    pub recipient:       &'a str,
    /// The verified sending address the message went out from.
    pub sending_address: &'a str,
    /// The relay/provider message id, if one was returned.
    pub message_id:      Option<&'a str>,
}

/// The delivery-feedback seam the `send_email` transport consults before a relay
/// and writes to after one.
///
/// Object-safe (`BoxFuture` returns, no `#[async_trait]`) so the transport holds an
/// `Arc<dyn SendTracker>` and tests can substitute an in-memory fake.
pub trait SendTracker: Send + Sync {
    /// The active suppression reason for a recipient (by keyed hash) in a tenant,
    /// or `None` if the recipient may be contacted. TTL-expired rows are ignored.
    fn suppression_reason<'a>(
        &'a self,
        tenant: Option<&'a str>,
        address_hash: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SuppressionReason>>> + Send + 'a>>;

    /// The recorded response if a send with `send_id` in this tenant has already
    /// completed (the exactly-once skip), else `None`.
    fn recorded_send<'a>(
        &'a self,
        tenant: Option<&'a str>,
        send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<RecordedSend>>> + Send + 'a>>;

    /// Record a successful relay as `Sent`. Idempotent on the exactly-once key: a
    /// concurrent/duplicate write for the same `(tenant, send_id)` is discarded.
    fn record_sent<'a>(
        &'a self,
        record: SentRecord<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

/// Map a `sqlx` error onto the canonical database error.
fn db_err(context: &str, error: &sqlx::Error) -> fraiseql_error::FraiseQLError {
    fraiseql_error::FraiseQLError::database(format!("send tracking: {context}: {error}"))
}

/// PostgreSQL-backed [`SendTracker`] over a connection pool.
///
/// Owns the `_fraiseql_send_status` / `_fraiseql_suppression` tables; the
/// connecting role must own them (or `BYPASSRLS`), which [`init`](Self::init)
/// arranges by creating them.
pub struct PgSendTracker {
    pool: PgPool,
}

impl PgSendTracker {
    /// Create a tracker over an existing pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create the delivery-feedback tables (idempotent). Call once on startup.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database)
    /// if the DDL fails.
    pub async fn init(&self) -> fraiseql_error::Result<()> {
        sqlx::raw_sql(fraiseql_functions::migrations::send_tracking_migration_sql())
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("init", &error))?;
        Ok(())
    }
}

impl SendTracker for PgSendTracker {
    fn suppression_reason<'a>(
        &'a self,
        tenant: Option<&'a str>,
        address_hash: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<SuppressionReason>>> + Send + 'a>> {
        Box::pin(async move {
            // `IS NOT DISTINCT FROM` matches a NULL (single-tenant) row against a
            // NULL bind; the `ttl` guard ignores expired suppressions.
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT reason FROM _fraiseql_suppression \
                 WHERE address_hash = $1 AND tenant_id IS NOT DISTINCT FROM $2 \
                   AND (ttl IS NULL OR ttl > now()) \
                 LIMIT 1",
            )
            .bind(address_hash)
            .bind(tenant)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| db_err("suppression lookup", &error))?;

            // An unknown reason token (forward-compat) still suppresses — a row on
            // the do-not-contact list means do not contact, whatever the label.
            Ok(row.map(|(reason,)| {
                SuppressionReason::parse(&reason).unwrap_or(SuppressionReason::Unsubscribe)
            }))
        })
    }

    fn recorded_send<'a>(
        &'a self,
        tenant: Option<&'a str>,
        send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<RecordedSend>>> + Send + 'a>> {
        Box::pin(async move {
            let row: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT message_id FROM _fraiseql_send_status \
                 WHERE send_id = $1 AND tenant_id IS NOT DISTINCT FROM $2 \
                 LIMIT 1",
            )
            .bind(send_id)
            .bind(tenant)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| db_err("recorded-send lookup", &error))?;

            Ok(row.map(|(message_id,)| RecordedSend { message_id }))
        })
    }

    fn record_sent<'a>(
        &'a self,
        record: SentRecord<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // ON CONFLICT on the exactly-once expression index keeps a
            // crash-retry (or a race) from writing two `Sent` rows for one send.
            sqlx::query(
                "INSERT INTO _fraiseql_send_status \
                     (send_id, tenant_id, recipient, sending_address, status, message_id, \
                      sent_at, updated_at) \
                 VALUES ($1, $2, $3, $4, 'Sent', $5, now(), now()) \
                 ON CONFLICT (COALESCE(tenant_id, ''), send_id) DO NOTHING",
            )
            .bind(record.send_id)
            .bind(record.tenant)
            .bind(record.recipient)
            .bind(record.sending_address)
            .bind(record.message_id)
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("record sent", &error))?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
