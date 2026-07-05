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

    /// The default suppression expiry for this reason, relative to `now`.
    ///
    /// A hard bounce and a manual unsubscribe are ~permanent (`None`): the address
    /// does not accept mail / the recipient opted out. An unanswered-challenge
    /// suppression expires after ~30 days — long enough to stop re-quarantining the
    /// domain, short enough that a since-fixed mailbox is not muted forever (and it
    /// lifts immediately on a genuine reply, well before the TTL).
    #[must_use]
    pub fn default_ttl(
        self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        match self {
            SuppressionReason::HardBounce | SuppressionReason::Unsubscribe => None,
            SuppressionReason::ChallengeUnanswered => Some(now + chrono::Duration::days(30)),
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
/// Object-safe (`BoxFuture` returns, no `async_trait` macro) so the transport holds an
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

/// A send matched by the correlation step.
///
/// Carries the fields the transition needs: the tenant (RLS scope for suppression
/// writes) and the raw recipient (which the correlator keyed-hashes to write/lift a
/// suppression row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrelatedSend {
    /// The send-id (echoed for send-id lookups; resolved from the row for
    /// message-id fallback lookups).
    pub send_id:   String,
    /// The tenant the original send was scoped to.
    pub tenant:    Option<String>,
    /// The raw recipient address of the original send.
    pub recipient: String,
}

/// The inbound-correlation seam: look a tracked send up from an inbound signal and
/// transition its status + the recipient's suppression state.
///
/// Separate from [`SendTracker`] (the send path) so the poll worker depends only on
/// the inbound half and a test fake implements only these methods. `PgSendTracker`
/// implements both. All lookups run through the table-owning pool (which bypasses
/// RLS) because an inbound bounce carries no session tenant — the tenant is read
/// from the matched row and stamped on any suppression write.
pub trait SendCorrelator: Send + Sync {
    /// Find a tracked send by its VERP send-id (the Return-Path plus-tag).
    fn find_by_send_id<'a>(
        &'a self,
        send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<CorrelatedSend>>> + Send + 'a>>;

    /// Find a tracked send by our sent message-id (the References fallback).
    fn find_by_message_id<'a>(
        &'a self,
        message_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<CorrelatedSend>>> + Send + 'a>>;

    /// Transition a send to `Bounced`.
    fn mark_bounced<'a>(
        &'a self,
        send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Transition a send to `ChallengePending` and return the recipient's total
    /// unanswered-challenge count (across campaigns, tenant-scoped) — the value the
    /// challenge policy compares against `challenge_suppress_after`.
    fn bump_challenge<'a>(
        &'a self,
        send_id: &'a str,
        tenant: Option<&'a str>,
        recipient: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<i64>> + Send + 'a>>;

    /// Transition a send to `Replied` and reset the recipient's unanswered-challenge
    /// state (a reply is the positive signal that resets the per-recipient counter).
    fn mark_replied<'a>(
        &'a self,
        send_id: &'a str,
        tenant: Option<&'a str>,
        recipient: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Record an informational signal (out-of-office / auto-generated) without
    /// changing the send status.
    fn record_signal<'a>(
        &'a self,
        send_id: &'a str,
        signal: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Add or refresh a suppression for a recipient (by keyed hash). A permanent
    /// existing suppression (no TTL, e.g. a hard bounce) is never downgraded.
    fn suppress<'a>(
        &'a self,
        tenant: Option<&'a str>,
        address_hash: &'a str,
        reason: SuppressionReason,
        ttl: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Lift a suppression of a specific reason for a recipient (by keyed hash) — a
    /// reply lifts a `challenge_unanswered`, never a `hard_bounce`.
    fn lift_suppression<'a>(
        &'a self,
        tenant: Option<&'a str>,
        address_hash: &'a str,
        reason: SuppressionReason,
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

/// The columns a correlation lookup selects.
type SendRow = (String, Option<String>, String);

fn to_correlated((send_id, tenant, recipient): SendRow) -> CorrelatedSend {
    CorrelatedSend {
        send_id,
        tenant,
        recipient,
    }
}

impl SendCorrelator for PgSendTracker {
    fn find_by_send_id<'a>(
        &'a self,
        send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<CorrelatedSend>>> + Send + 'a>> {
        Box::pin(async move {
            let row: Option<SendRow> = sqlx::query_as(
                "SELECT send_id, tenant_id, recipient FROM _fraiseql_send_status \
                 WHERE send_id = $1 LIMIT 1",
            )
            .bind(send_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| db_err("find by send-id", &error))?;
            Ok(row.map(to_correlated))
        })
    }

    fn find_by_message_id<'a>(
        &'a self,
        message_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<CorrelatedSend>>> + Send + 'a>> {
        Box::pin(async move {
            let row: Option<SendRow> = sqlx::query_as(
                "SELECT send_id, tenant_id, recipient FROM _fraiseql_send_status \
                 WHERE message_id = $1 LIMIT 1",
            )
            .bind(message_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| db_err("find by message-id", &error))?;
            Ok(row.map(to_correlated))
        })
    }

    fn mark_bounced<'a>(
        &'a self,
        send_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE _fraiseql_send_status \
                 SET status = 'Bounced', last_signal = 'bounce', updated_at = now() \
                 WHERE send_id = $1",
            )
            .bind(send_id)
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("mark bounced", &error))?;
            Ok(())
        })
    }

    fn bump_challenge<'a>(
        &'a self,
        send_id: &'a str,
        tenant: Option<&'a str>,
        recipient: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<i64>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE _fraiseql_send_status \
                 SET status = 'ChallengePending', challenge_count = challenge_count + 1, \
                     last_signal = 'challenge', updated_at = now() \
                 WHERE send_id = $1",
            )
            .bind(send_id)
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("bump challenge", &error))?;

            // Per-recipient across campaigns: how many of this recipient's sends are
            // still awaiting a challenge answer.
            let (count,): (i64,) = sqlx::query_as(
                "SELECT count(*) FROM _fraiseql_send_status \
                 WHERE recipient = $1 AND tenant_id IS NOT DISTINCT FROM $2 \
                   AND status = 'ChallengePending'",
            )
            .bind(recipient)
            .bind(tenant)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| db_err("count pending challenges", &error))?;
            Ok(count)
        })
    }

    fn mark_replied<'a>(
        &'a self,
        send_id: &'a str,
        tenant: Option<&'a str>,
        recipient: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // The replied send → Replied, and reset the recipient's other pending
            // challenges (a reply is the per-recipient positive signal).
            sqlx::query(
                "UPDATE _fraiseql_send_status \
                 SET status = 'Replied', challenge_count = 0, last_signal = 'reply', \
                     updated_at = now() \
                 WHERE tenant_id IS NOT DISTINCT FROM $2 \
                   AND (send_id = $1 OR (recipient = $3 AND status = 'ChallengePending'))",
            )
            .bind(send_id)
            .bind(tenant)
            .bind(recipient)
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("mark replied", &error))?;
            Ok(())
        })
    }

    fn record_signal<'a>(
        &'a self,
        send_id: &'a str,
        signal: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE _fraiseql_send_status SET last_signal = $2, updated_at = now() \
                 WHERE send_id = $1",
            )
            .bind(send_id)
            .bind(signal)
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("record signal", &error))?;
            Ok(())
        })
    }

    fn suppress<'a>(
        &'a self,
        tenant: Option<&'a str>,
        address_hash: &'a str,
        reason: SuppressionReason,
        ttl: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            // Never downgrade a permanent suppression (ttl IS NULL, e.g. a hard
            // bounce) to a temporary one — only refresh/upgrade a temporary row or
            // insert a new one.
            sqlx::query(
                "INSERT INTO _fraiseql_suppression (tenant_id, address_hash, reason, ttl) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (COALESCE(tenant_id, ''), address_hash) DO UPDATE \
                     SET reason = EXCLUDED.reason, ttl = EXCLUDED.ttl, \
                         since = now(), updated_at = now() \
                     WHERE _fraiseql_suppression.ttl IS NOT NULL",
            )
            .bind(tenant)
            .bind(address_hash)
            .bind(reason.as_str())
            .bind(ttl)
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("suppress", &error))?;
            Ok(())
        })
    }

    fn lift_suppression<'a>(
        &'a self,
        tenant: Option<&'a str>,
        address_hash: &'a str,
        reason: SuppressionReason,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query(
                "DELETE FROM _fraiseql_suppression \
                 WHERE address_hash = $1 AND tenant_id IS NOT DISTINCT FROM $2 AND reason = $3",
            )
            .bind(address_hash)
            .bind(tenant)
            .bind(reason.as_str())
            .execute(&self.pool)
            .await
            .map_err(|error| db_err("lift suppression", &error))?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
