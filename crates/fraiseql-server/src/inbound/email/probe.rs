//! Return-Path probe: verify the provider preserves plus-addressing before trusting
//! VERP correlation.
//!
//! The whole delivery-feedback loop rides on the polled mailbox receiving
//! `bounces+<send-id>@…` with the `+<send-id>` sub-address intact. Most providers
//! (Gmail, Fastmail, M365) preserve it, but some — a few French / legacy hosters —
//! silently strip the plus-tag, so every bounce lands at bare `bounces@` with no
//! send-id and correlation quietly fails: every send then looks delivered.
//!
//! The probe turns that silent, deployment-dependent failure into a diagnosable one
//! (the same doctrine as the `security_invoker` doctor check): at startup it sends a
//! self-addressed `bounces+probe-<nonce>@<domain>` and watches the poll cursor for
//! it. If it lands with the tag intact, VERP correlation is confirmed; if it does
//! not within the window, the operator is warned loudly that plus-addressing may be
//! stripped and correlation will not work — sends still go out, just untracked.
//!
//! It is opt-in (`[send] verp_probe_on_start`, default off) because it emits a real
//! message per eligible mailbox on each boot; an operator enables it once to verify
//! a new deployment.

use std::time::Duration;

use fraiseql_functions::{
    EmailTransport, IngestSource, SendContext, SendEmailRequest, SenderIdentity, normalize_email,
    parse_recipient,
};

use super::imap::MailboxFetcher;

/// The plus-tag prefix that marks a probe (`bounces+probe-<nonce>@…`).
const PROBE_TAG_PREFIX: &str = "probe-";

/// How many messages to scan per poll while waiting for the probe to land.
const PROBE_FETCH_BATCH: u32 = 50;

/// The probe recipient address `<local_part>+probe-<nonce>@<domain>`.
#[must_use]
pub fn probe_recipient(local_part: &str, domain: &str, nonce: &str) -> String {
    format!("{local_part}+{PROBE_TAG_PREFIX}{nonce}@{domain}")
}

/// Whether a normalized inbound message is the probe with `nonce` — i.e. a
/// recipient carries the `probe-<nonce>` plus-tag (proving the provider kept it).
#[must_use]
pub fn message_carries_probe(message: &fraiseql_functions::InboundMessage, nonce: &str) -> bool {
    let want = format!("{PROBE_TAG_PREFIX}{nonce}");
    message
        .to
        .iter()
        .map(String::as_str)
        .filter_map(parse_recipient)
        .filter_map(|recipient| recipient.tag)
        .any(|tag| tag == want)
}

/// The result of a Return-Path probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The probe landed with its plus-tag intact — VERP correlation works.
    Confirmed,
    /// The probe did not land (tagged) within the window — plus-addressing may be
    /// stripped, so correlation cannot be trusted.
    NotObserved,
}

/// Send a self-addressed probe and poll the mailbox for it within `timeout`.
///
/// Sends `bounces+probe-<nonce>@<domain>` (a plain send — no send-id, so no VERP
/// envelope or suppression bookkeeping) from `sender`, then polls `fetcher` every
/// `poll_interval` until the probe lands (tagged) or the window elapses. The poll
/// reads from the mailbox start with no cursor, so it never disturbs the worker's
/// cursor.
///
/// # Errors
///
/// Returns the send error, or a database/transport error from a poll.
pub async fn run_return_path_probe(
    transport: &dyn EmailTransport,
    fetcher: &dyn MailboxFetcher,
    sender: &SenderIdentity,
    probe_to: &str,
    nonce: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> fraiseql_error::Result<ProbeOutcome> {
    let request = SendEmailRequest {
        to:       probe_to.to_string(),
        subject:  "fraiseql Return-Path probe".to_string(),
        text:     Some(
            "Automated probe verifying VERP plus-addressing. Safe to ignore.".to_string(),
        ),
        html:     None,
        reply_to: None,
    };
    // A plain send (no send-id): the probe address itself carries the tag we watch
    // for, so no VERP envelope is needed.
    transport.send(sender, &request, SendContext::default()).await?;

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let batch = fetcher
            .fetch(None, PROBE_FETCH_BATCH)
            .await
            .map_err(|error| fraiseql_error::FraiseQLError::database(error.to_string()))?;
        for message in &batch.messages {
            if let Ok(parsed) =
                normalize_email(&message.raw, IngestSource::Email, chrono::Utc::now())
            {
                if message_carries_probe(&parsed.message, nonce) {
                    return Ok(ProbeOutcome::Confirmed);
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(ProbeOutcome::NotObserved);
        }
        tokio::time::sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests;
