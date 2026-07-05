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
use fraiseql_functions::{EmailTransport, SendEmailRequest, SendEmailResponse, SenderIdentity};
use lettre::{
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
};
use tracing::warn;

use super::config::{MailboxSmtpConfig, SmtpTlsMode};

/// Build the `send_email` transport from the SMTP halves of the configured mailboxes.
///
/// Returns an `Arc<dyn EmailTransport>` ready to attach via
/// [`BeforeMutationHooks::with_email`](crate::subsystems::BeforeMutationHooks::with_email).
/// Returns `None` when no `[mailbox.<name>.smtp]` account was successfully built —
/// the caller then leaves `send_email` unconfigured (fail-loud). `get_env` resolves
/// each account's password env (in production, [`std::env::var`]).
#[must_use]
pub fn build_email_transport<S: std::hash::BuildHasher>(
    mailboxes: &HashMap<String, super::MailboxConfig, S>,
    get_env: impl Fn(&str) -> Option<String>,
) -> Option<std::sync::Arc<dyn EmailTransport>> {
    let accounts = mailboxes
        .iter()
        .filter_map(|(name, mailbox)| mailbox.smtp.as_ref().map(|smtp| (name.as_str(), smtp)));
    SmtpMailboxTransport::build(accounts, get_env)
        .map(|transport| std::sync::Arc::new(transport) as std::sync::Arc<dyn EmailTransport>)
}

/// One connected SMTP account: a pooled `lettre` transport, keyed in the parent
/// map by its verified sending address.
struct SmtpAccount {
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

/// The `send_email` transport: routes each send to the connected account whose
/// verified sending address matches the resolved sender identity.
pub struct SmtpMailboxTransport {
    accounts: HashMap<String, SmtpAccount>,
    /// Optional per-mailbox send-warming counter. `None` → no daily cap is
    /// enforced (see [`with_send_counter`](Self::with_send_counter)).
    counter:  Option<std::sync::Arc<dyn super::warming::SendCounter>>,
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
            match build_account_transport(cfg, password) {
                Ok(transport) => {
                    accounts.insert(cfg.address.clone(), SmtpAccount { transport });
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

            // Warming cap: refuse when the mailbox is at its daily limit. A daily
            // cap will not clear on a seconds-scale retry, so this is a permanent
            // failure for this dispatch (429 → dead-letter → replay next day),
            // not a transient one.
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

            let message = build_message(sender, request)?;

            match account.transport.send(message).await {
                Ok(response) => {
                    // Best-effort accounting: the message is already sent, so a
                    // counter error is logged, not surfaced (it must not un-send).
                    if let Some(counter) = self.counter.as_ref() {
                        if let Err(error) = counter.record_send(&sender.address).await {
                            warn!(address = %sender.address, %error, "failed to record send for warming");
                        }
                    }
                    Ok(SendEmailResponse {
                        message_id: response.first_line().map(ToString::to_string),
                        accepted:   true,
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
fn build_message(sender: &SenderIdentity, request: &SendEmailRequest) -> Result<Message> {
    let from = mailbox(&sender.address, sender.display_name.as_deref())?;
    let to = mailbox(&request.to, None)?;

    let mut builder = Message::builder().from(from).to(to).subject(request.subject.clone());
    if let Some(reply_to) = request.reply_to.as_deref() {
        builder = builder.reply_to(mailbox(reply_to, None)?);
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
