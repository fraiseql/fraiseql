//! Per-connected-account SMTP send transport for the `send_email` host op.
//!
//! Consumes the `[mailbox.<name>.smtp]` halves: one pooled `lettre` transport per
//! connected account, keyed by the account's verified sending address. The op
//! resolves the host-owned `from` (the #539 sender seam), then this transport
//! routes the send to the account whose address matches — never falling back to a
//! different mailbox. Secrets (the SMTP password) are read server-side from the
//! account's `password_env`, never from the DB row or guest input.
//!
//! Failures are classified onto the error status durable dispatch reads: a
//! permanent SMTP error (5xx, unknown account, malformed recipient) is a 4xx
//! `FraiseQLError` (dead-lettered), a transient one (connection refused, timeout,
//! greylisting) is a 5xx (retried).

use std::{collections::HashMap, future::Future, pin::Pin, time::Duration};

use fraiseql_error::{FraiseQLError, Result};
use fraiseql_functions::{
    EmailTransport, SendContext, SendEmailRequest, SendEmailResponse, SenderIdentity,
};
use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    address::Envelope,
    message::{Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
};
use tracing::warn;

use super::{
    config::{MailboxSmtpConfig, SmtpTlsMode},
    tracking::{SendTracker, SentRecord},
};

/// Build the `send_email` transport from the SMTP halves of the configured mailboxes.
///
/// Returns an `Arc<dyn EmailTransport>` ready to attach via
/// [`BeforeMutationHooks::with_email`](crate::subsystems::BeforeMutationHooks::with_email).
/// Returns `None` when no `[mailbox.<name>.smtp]` account was successfully built —
/// the caller then leaves `send_email` unconfigured (fail-loud). `get_env` resolves
/// each account's password env (in production, [`std::env::var`]).
///
/// When both `tracker` and `address_hash_key` are `Some`, the transport enforces
/// the delivery-feedback loop (suppression check before send, exactly-once skip,
/// `Sent` record); otherwise it sends without correlation.
#[must_use]
pub fn build_email_transport<S: std::hash::BuildHasher>(
    mailboxes: &HashMap<String, super::MailboxConfig, S>,
    get_env: impl Fn(&str) -> Option<String>,
    tracker: Option<std::sync::Arc<dyn SendTracker>>,
    address_hash_key: Option<std::sync::Arc<[u8]>>,
) -> Option<std::sync::Arc<dyn EmailTransport>> {
    let accounts = mailboxes
        .iter()
        .filter_map(|(name, mailbox)| mailbox.smtp.as_ref().map(|smtp| (name.as_str(), smtp)));
    let mut transport = SmtpMailboxTransport::build(accounts, get_env)?;
    // Attach the delivery-feedback store when present; the address-hash key (which
    // needs the server HMAC secret) additionally enables the suppression check.
    if let Some(tracker) = tracker {
        transport = transport.with_tracker(tracker, address_hash_key);
    }
    Some(std::sync::Arc::new(transport) as std::sync::Arc<dyn EmailTransport>)
}

/// One connected SMTP account: a pooled `lettre` transport, keyed in the parent
/// map by its verified sending address.
struct SmtpAccount {
    transport:       AsyncSmtpTransport<Tokio1Executor>,
    /// The VERP Return-Path local part (`bounces` by default) — the envelope
    /// `MAIL FROM` becomes `<local_part>+<send-id>@<domain>`.
    verp_local_part: String,
    /// The VERP Return-Path domain (the sending address's own domain by default,
    /// for SPF/DMARC alignment).
    verp_domain:     String,
}

impl SmtpAccount {
    /// Build the VERP envelope sender `<local_part>+<send-id>@<domain>` for a send.
    ///
    /// Returns a permanent [`FraiseQLError::Validation`] if the resolved
    /// Return-Path is not a valid address (a misconfigured `return_path`).
    fn verp_from(&self, send_id: &str) -> Result<Address> {
        Address::new(format!("{}+{send_id}", self.verp_local_part), &self.verp_domain).map_err(
            |error| FraiseQLError::Validation {
                message: format!(
                    "invalid VERP Return-Path {}+{send_id}@{}: {error}",
                    self.verp_local_part, self.verp_domain
                ),
                path:    None,
            },
        )
    }
}

/// The `send_email` transport: routes each send to the connected account whose
/// verified sending address matches the resolved sender identity.
pub struct SmtpMailboxTransport {
    accounts:         HashMap<String, SmtpAccount>,
    /// Optional per-mailbox send-warming counter. `None` → no daily cap is
    /// enforced (see [`with_send_counter`](Self::with_send_counter)).
    counter:          Option<std::sync::Arc<dyn super::warming::SendCounter>>,
    /// Optional delivery-feedback store: suppression check before send +
    /// exactly-once skip + `Sent` record. `None` → no suppression/exactly-once (a
    /// plain send). Paired with `address_hash_key` (see [`with_tracker`](Self::with_tracker)).
    tracker:          Option<std::sync::Arc<dyn SendTracker>>,
    /// The keyed hash of a recipient address for suppression lookups. `None` → no
    /// suppression check even with a tracker (no secret configured).
    address_hash_key: Option<std::sync::Arc<[u8]>>,
}

impl SmtpMailboxTransport {
    /// Build a transport from the SMTP halves of the configured mailboxes.
    ///
    /// Each `[mailbox.<name>.smtp]` account becomes one pooled `lettre` transport,
    /// keyed by its `address`. `get_env` resolves the password env (in production,
    /// [`std::env::var`]). An account whose password env is unset, or whose relay
    /// cannot be built, is skipped with a warning (never relays unauthenticated).
    /// Returns `None` when no account was successfully built — the caller then
    /// leaves `send_email` unconfigured (fail-loud), never a phantom transport.
    #[must_use]
    pub fn build<'a>(
        mailboxes: impl Iterator<Item = (&'a str, &'a MailboxSmtpConfig)>,
        get_env: impl Fn(&str) -> Option<String>,
    ) -> Option<Self> {
        let mut accounts = HashMap::new();
        for (name, cfg) in mailboxes {
            let Some(password) = get_env(&cfg.password_env) else {
                warn!(
                    mailbox = %name,
                    password_env = %cfg.password_env,
                    "SMTP send not enabled for mailbox: password env is unset"
                );
                continue;
            };
            // Return-Path domain should align with the sending domain (SPF/DMARC);
            // a mismatch still sends but silently degrades deliverability, so warn.
            let verp_domain = cfg.return_path_domain().to_string();
            if verp_domain != cfg.sending_domain() {
                warn!(
                    mailbox = %name,
                    sending_domain = %cfg.sending_domain(),
                    return_path_domain = %verp_domain,
                    "VERP Return-Path domain differs from the sending domain — SPF/DMARC \
                     alignment is broken and deliverability of tracked sends may degrade"
                );
            }
            match build_account_transport(cfg, password) {
                Ok(transport) => {
                    accounts.insert(
                        cfg.address.clone(),
                        SmtpAccount {
                            transport,
                            verp_local_part: cfg.return_path_local_part().to_string(),
                            verp_domain,
                        },
                    );
                },
                Err(error) => warn!(
                    mailbox = %name,
                    %error,
                    "SMTP send not enabled for mailbox: relay build failed"
                ),
            }
        }
        if accounts.is_empty() {
            None
        } else {
            Some(Self {
                accounts,
                counter: None,
                tracker: None,
                address_hash_key: None,
            })
        }
    }

    /// Attach a per-mailbox send-warming counter that caps daily volume during a
    /// mailbox's warming period. Without one, no daily cap is enforced.
    #[must_use]
    pub fn with_send_counter(
        mut self,
        counter: std::sync::Arc<dyn super::warming::SendCounter>,
    ) -> Self {
        self.counter = Some(counter);
        self
    }

    /// Attach the delivery-feedback store, and optionally the recipient
    /// address-hash key.
    ///
    /// The store enables the exactly-once skip (whenever a send carries a send-id)
    /// and the `Sent` record after a relay. The `address_hash_key` (a subkey of the
    /// server HMAC secret, see
    /// [`derive_address_hash_key`](fraiseql_observers::derive_address_hash_key))
    /// additionally enables the suppression check — it keys the recipient hash so
    /// the store holds no raw address. Without the key, the suppression check is
    /// skipped (there is no secret to derive the same hash the admin surface writes
    /// with), but exactly-once still applies.
    #[must_use]
    pub fn with_tracker(
        mut self,
        tracker: std::sync::Arc<dyn SendTracker>,
        address_hash_key: Option<std::sync::Arc<[u8]>>,
    ) -> Self {
        self.tracker = Some(tracker);
        self.address_hash_key = address_hash_key;
        self
    }

    /// Number of connected send accounts (for diagnostics/tests).
    #[must_use]
    pub fn account_count(&self) -> usize {
        self.accounts.len()
    }
}

impl EmailTransport for SmtpMailboxTransport {
    fn send<'a>(
        &'a self,
        sender: &'a SenderIdentity,
        request: &'a SendEmailRequest,
        context: SendContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<SendEmailResponse>> + Send + 'a>> {
        Box::pin(async move {
            // Select the account by the verified sending address — never fall back
            // to a different mailbox (uniform with the read path's fail-closed).
            let Some(account) = self.accounts.get(&sender.address) else {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "no connected SMTP account for sending address {:?}",
                        sender.address
                    ),
                    path:    None,
                });
            };

            if let Some(tracker) = self.tracker.as_ref() {
                // 1. Suppression: refuse a recipient on the do-not-contact list before anything
                //    else — the biggest deliverability + GDPR lever. A suppressed recipient is
                //    permanent (a retry won't un-suppress), so a 4xx durable dispatch dead-letters
                //    rather than retries. Only with the address-hash key (the same secret the admin
                //    surface hashes with); without it the suppression check is skipped.
                if let Some(key) = self.address_hash_key.as_ref() {
                    let recipient_hash = fraiseql_observers::hash_address(key, &request.to);
                    if let Some(reason) =
                        tracker.suppression_reason(context.tenant, &recipient_hash).await?
                    {
                        return Err(FraiseQLError::Validation {
                            message: format!(
                                "recipient is suppressed ({reason}) — refusing to send"
                            ),
                            path:    None,
                        });
                    }
                }

                // 2. Exactly-once: a durable retry of an already-sent dispatch must not
                //    double-send. If this send-id already completed, skip the relay and return the
                //    recorded response.
                if let Some(send_id) = context.send_id {
                    if let Some(recorded) = tracker.recorded_send(context.tenant, send_id).await? {
                        return Ok(SendEmailResponse {
                            message_id: recorded.message_id,
                            accepted:   true,
                        });
                    }
                }
            }

            // 3. Warming cap: refuse when the mailbox is at its daily limit. A daily cap will not
            //    clear on a seconds-scale retry, so this is a permanent failure for this dispatch
            //    (429 → dead-letter → replay next day), not a transient one.
            if let Some(counter) = self.counter.as_ref() {
                if let Some(state) = counter.state(&sender.address).await? {
                    if !state.within_cap() {
                        return Err(FraiseQLError::RateLimited {
                            message:          format!(
                                "sending address {:?} is at its warming daily cap",
                                sender.address
                            ),
                            retry_after_secs: 86_400,
                        });
                    }
                }
            }

            // 4. Set the VERP Return-Path envelope so an inbound bounce/challenge/ reply correlates
            //    back to this send-id. Only when a send-id is present (a signed token); otherwise
            //    the plain header-derived envelope is used.
            let verp_from =
                context.send_id.map(|send_id| account.verp_from(send_id)).transpose()?;
            let message = build_message(sender, request, verp_from)?;

            match account.transport.send(message).await {
                Ok(response) => {
                    let message_id = response.first_line().map(ToString::to_string);
                    // Record the send as `Sent` so the correlation step has a row to
                    // transition and the exactly-once skip fires on a retry. The
                    // message is already sent, so a bookkeeping error is logged, not
                    // surfaced (it must not un-send / trigger a retry).
                    if let (Some(tracker), Some(send_id)) = (self.tracker.as_ref(), context.send_id)
                    {
                        let record = SentRecord {
                            send_id,
                            tenant: context.tenant,
                            recipient: &request.to,
                            sending_address: &sender.address,
                            message_id: message_id.as_deref(),
                        };
                        if let Err(error) = tracker.record_sent(record).await {
                            warn!(%send_id, %error, "failed to record Sent status after relay");
                        }
                    }
                    // Best-effort warming accounting (same must-not-un-send rule).
                    if let Some(counter) = self.counter.as_ref() {
                        if let Err(error) = counter.record_send(&sender.address).await {
                            warn!(address = %sender.address, %error, "failed to record send for warming");
                        }
                    }
                    Ok(SendEmailResponse {
                        message_id,
                        accepted: true,
                    })
                },
                // A permanent SMTP failure (5xx: auth rejected, bad recipient) must
                // NOT retry — map to a 4xx so durable dispatch dead-letters it.
                Err(error) if error.is_permanent() => Err(FraiseQLError::Validation {
                    message: format!("SMTP permanent error: {error}"),
                    path:    None,
                }),
                // Everything else (connection refused, timeout, 4xx greylisting) is
                // transient — a 5xx so durable dispatch retries.
                Err(error) => Err(FraiseQLError::ServiceUnavailable {
                    message:     format!("SMTP transient error: {error}"),
                    retry_after: None,
                }),
            }
        })
    }
}

/// Build one account's pooled SMTP transport from its config + resolved password.
fn build_account_transport(
    cfg: &MailboxSmtpConfig,
    password: String,
) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
    let mut builder = match cfg.tls {
        SmtpTlsMode::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)
            .map_err(|error| FraiseQLError::Configuration {
                message: format!("cannot build STARTTLS relay to {}: {error}", cfg.host),
            })?,
        SmtpTlsMode::Tls => {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host).map_err(|error| {
                FraiseQLError::Configuration {
                    message: format!("cannot build TLS relay to {}: {error}", cfg.host),
                }
            })?
        },
        SmtpTlsMode::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host),
    };
    builder = builder
        .port(cfg.port)
        .timeout(Some(Duration::from_secs(cfg.timeout_secs)))
        .credentials(Credentials::new(cfg.username.clone(), password));
    Ok(builder.build())
}

/// Build the `lettre` message: host-owned `from`, guest-supplied recipient/body.
///
/// When `verp_from` is `Some`, the SMTP envelope sender (`MAIL FROM`) is overridden
/// to the VERP Return-Path while the header `From` stays the verified sending
/// address — so a bounce/challenge addressed to the Return-Path carries the send-id
/// back for correlation. When `None`, `lettre` derives the envelope from the
/// headers (the plain, uncorrelated path).
fn build_message(
    sender: &SenderIdentity,
    request: &SendEmailRequest,
    verp_from: Option<Address>,
) -> Result<Message> {
    let to_address = request.to.parse::<Address>().map_err(|error| FraiseQLError::Validation {
        message: format!("invalid email address {:?}: {error}", request.to),
        path:    None,
    })?;
    let from = mailbox(&sender.address, sender.display_name.as_deref())?;
    let to = Mailbox::new(None, to_address.clone());

    let mut builder = Message::builder().from(from).to(to).subject(request.subject.clone());
    if let Some(reply_to) = request.reply_to.as_deref() {
        builder = builder.reply_to(mailbox(reply_to, None)?);
    }
    if let Some(verp) = verp_from {
        // Override MAIL FROM only; RCPT TO stays the recipient. `Envelope::new`
        // only errors on an empty recipient list, which cannot happen here.
        let envelope = Envelope::new(Some(verp), vec![to_address]).map_err(|error| {
            FraiseQLError::Validation {
                message: format!("failed to build VERP envelope: {error}"),
                path:    None,
            }
        })?;
        builder = builder.envelope(envelope);
    }

    let built = match (request.text.as_deref(), request.html.as_deref()) {
        (Some(text), Some(html)) => {
            builder.multipart(MultiPart::alternative_plain_html(text.to_owned(), html.to_owned()))
        },
        (Some(text), None) => builder.singlepart(SinglePart::plain(text.to_owned())),
        (None, Some(html)) => builder.singlepart(SinglePart::html(html.to_owned())),
        // No body: a valid, empty plain-text part rather than an error.
        (None, None) => builder.singlepart(SinglePart::plain(String::new())),
    };

    built.map_err(|error| FraiseQLError::Validation {
        message: format!("failed to build email message: {error}"),
        path:    None,
    })
}

/// Parse an address into a `lettre` [`Mailbox`] with an optional display name.
fn mailbox(address: &str, display_name: Option<&str>) -> Result<Mailbox> {
    let parsed = address.parse::<Address>().map_err(|error| FraiseQLError::Validation {
        message: format!("invalid email address {address:?}: {error}"),
        path:    None,
    })?;
    Ok(Mailbox::new(display_name.map(ToOwned::to_owned), parsed))
}

#[cfg(test)]
mod tests;
