//! Who may sign in by `OTP`: `[auth.local] otp_allowed_query` (#1852).
//!
//! A list decides an address's fate, so everything observable about a request must not depend
//! on the list's answer. The routes keep the per-address budgets identical by storing a code
//! either way, and [`OtpAllowGate`] takes delivery off the request, so a slow or failing relay
//! cannot answer who is approved.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::mpsc;

use super::EmailDelivery;
use crate::error::{AuthError, Result};

/// Mails waiting to leave. Sends are limited per client and per address before they reach this
/// queue, so it fills only when the relay is stalled.
pub const OTP_DELIVERY_QUEUE_CAPACITY: usize = 256;

/// How long the list may take to answer: every sign-in request waits on it.
const STATEMENT_TIMEOUT_MS: u32 = 2_000;

/// Decides whether an address may sign in by `OTP`.
// Reason: used as dyn Trait (Arc<dyn OtpAllowList>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait OtpAllowList: Send + Sync {
    /// Whether `email`, already normalized, may sign in.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] when the list cannot answer. Callers refuse the
    /// address on any error.
    async fn allows(&self, email: &str) -> Result<bool>;
}

/// An [`OtpAllowList`] answered by the operator's `SELECT`: a row admits the address.
pub struct PgOtpAllowList {
    db:        PgPool,
    /// The checked statement, with the address as `$1`.
    statement: String,
}

impl PgOtpAllowList {
    /// A list answered by `statement`, whose `$1` is the address. Build `statement` with
    /// `fraiseql_core::schema::otp_allowed_statement`.
    #[must_use]
    pub const fn new(db: PgPool, statement: String) -> Self {
        Self { db, statement }
    }
}

// Reason: OtpAllowList is defined with #[async_trait]; all implementations must match
#[async_trait]
impl OtpAllowList for PgOtpAllowList {
    async fn allows(&self, email: &str) -> Result<bool> {
        let db_err = |e: sqlx::Error| AuthError::DatabaseError {
            message: e.to_string(),
        };
        let mut tx = self.db.begin().await.map_err(db_err)?;
        // The list answers a question and changes nothing, whatever the statement calls.
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query(&format!("SET LOCAL statement_timeout = {STATEMENT_TIMEOUT_MS}"))
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        let row = sqlx::query(&self.statement)
            .bind(email)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;
        Ok(row.is_some())
    }
}

/// One mail waiting to leave. No `Debug`: it holds a live code.
struct QueuedOtp {
    email: String,
    code:  String,
}

/// An [`OtpAllowList`] and the bounded queue its admitted mail leaves through.
#[derive(Clone)]
pub struct OtpAllowGate {
    list:  Arc<dyn OtpAllowList>,
    queue: mpsc::Sender<QueuedOtp>,
}

impl OtpAllowGate {
    /// Start the gate's delivery worker, which sends one mail at a time for as long as a clone
    /// of the gate exists. Must be called inside a Tokio runtime.
    #[must_use]
    pub fn new(
        list: Arc<dyn OtpAllowList>,
        delivery: Arc<dyn EmailDelivery>,
        capacity: usize,
    ) -> Self {
        let (queue, mut queued) = mpsc::channel::<QueuedOtp>(capacity);
        tokio::spawn(async move {
            while let Some(QueuedOtp { email, code }) = queued.recv().await {
                if let Err(e) = delivery.send_otp(&email, &code).await {
                    let detail = without_address(&e.to_string(), &email);
                    tracing::error!(error = %detail, "OTP mail delivery failed");
                }
            }
        });
        Self { list, queue }
    }

    /// Whether the list admits `email`. A list that cannot answer refuses.
    pub(super) async fn admits(&self, email: &str) -> bool {
        self.list.allows(email).await.unwrap_or_else(|e| {
            let detail = without_address(&e.to_string(), email);
            tracing::error!(error = %detail, "otp_allowed_query failed; refusing the address");
            false
        })
    }

    /// Queue `code` for `email` if the list admits it. Never waits on the relay: with the queue
    /// full the mail is dropped and logged, because waiting would answer who is approved.
    pub(super) async fn deliver_if_admitted(&self, email: &str, code: String) {
        if !self.admits(email).await {
            return;
        }
        let queued = QueuedOtp {
            email: email.to_string(),
            code,
        };
        if self.queue.try_send(queued).is_err() {
            tracing::error!("OTP mail dropped: the delivery queue is full or stopped");
        }
    }
}

/// `detail` with `email` removed: a relay's or a database's error may quote the address, and
/// the log must not say which address a failure belongs to.
fn without_address(detail: &str, email: &str) -> String {
    detail.replace(email, "[address]")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::Semaphore;

    use super::*;

    struct AdmitAll;

    #[async_trait]
    impl OtpAllowList for AdmitAll {
        async fn allows(&self, _email: &str) -> Result<bool> {
            Ok(true)
        }
    }

    /// Delivers one mail per permit, so a test can hold the relay stalled.
    struct StalledRelay {
        permits:   Arc<Semaphore>,
        delivered: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl EmailDelivery for StalledRelay {
        async fn send_otp(&self, _email: &str, _code: &str) -> Result<String> {
            self.permits.acquire().await.expect("semaphore open").forget();
            self.delivered.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }
    }

    #[tokio::test]
    async fn a_full_queue_drops_the_mail_and_never_blocks_the_request() {
        let permits = Arc::new(Semaphore::new(0));
        let delivered = Arc::new(AtomicUsize::new(0));
        let gate = OtpAllowGate::new(
            Arc::new(AdmitAll),
            Arc::new(StalledRelay {
                permits:   Arc::clone(&permits),
                delivered: Arc::clone(&delivered),
            }),
            1,
        );

        // The worker holds at most one, the queue one more; the other eight are dropped. Any
        // of them waiting on the stalled relay would hang here and trip the timeout.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            for n in 0..10 {
                gate.deliver_if_admitted(&format!("{n}@example.com"), "000000".to_string())
                    .await;
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("a stalled relay must not block a request");

        permits.add_permits(10);
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while delivered.load(Ordering::SeqCst) < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the queued mail leaves once the relay recovers");
        tokio::task::yield_now().await;
        assert_eq!(delivered.load(Ordering::SeqCst), 2, "the rest were dropped");
    }

    #[test]
    fn a_logged_failure_does_not_name_the_address() {
        assert_eq!(
            without_address("recipient <a@example.com> rejected", "a@example.com"),
            "recipient <[address]> rejected"
        );
    }
}
