//! `[auth.local]` — the first-party auth methods the server operates itself (#367).
//!
//! `fraiseql-auth` shipped email+password (with a complete reset flow), email `OTP`,
//! `TOTP` `MFA` and anonymous sessions as **library code with no way to reach it**:
//! the route-state setters had zero callers, three route groups were registered
//! against fields hard-coded to `None`, and the reset flow had no concrete
//! [`ResetEmailSender`] outside its own test double. This module is the wiring —
//! compiled config in, mounted routes and durable stores out — plus the mail bridge
//! that makes "we emailed you a code" true.
//!
//! Every method that cannot work refuses to boot rather than mounting a flow that
//! dead-ends: a missing mailbox, a mailbox with no `SMTP` half, a build without the
//! `inbound-email` feature, or a missing database pool are all startup errors naming
//! the offending key.

use std::sync::Arc;

use fraiseql_core::schema::LocalAuthConfig;
use tracing::info;

use crate::{ServerError, server_config::ServerConfig};

/// Local alias mirroring the server's result type.
type Result<T> = std::result::Result<T, ServerError>;

/// Placeholder the reset-link template substitutes the opaque token for.
/// Kept as a constant so the CLI validation, the docs and this substitution
/// cannot drift on the spelling.
#[cfg(feature = "inbound-email")]
const RESET_TOKEN_PLACEHOLDER: &str = "{token}";

/// Placeholder the magic-link template substitutes the OTP code for.
#[cfg(feature = "inbound-email")]
const MAGIC_CODE_PLACEHOLDER: &str = "{code}";

/// Default `MFA` issuer shown in authenticator apps.
const DEFAULT_MFA_ISSUER: &str = "FraiseQL";

/// The states `[auth.local]` produces, each `Some` only when its method is enabled.
pub struct LocalAuthStates {
    pub otp:      Option<Arc<fraiseql_auth::OtpRouteState>>,
    pub mfa:      Option<Arc<fraiseql_auth::MfaRouteState>>,
    pub password: Option<Arc<fraiseql_auth::LocalPasswordRouteState>>,
    pub anon:     Option<Arc<fraiseql_auth::AnonSignupState>>,
}

/// Delivers `OTP` codes and password-reset links through a configured
/// `[mailbox.<name>.smtp]` account (#367).
///
/// This is the concrete sender the reset flow documented but never had. It relays
/// through the same transport the `send_email` host op uses, so a deployment
/// configures its outbound mail **once**.
#[cfg(feature = "inbound-email")]
pub struct MailboxEmailSender {
    transport:           Arc<dyn fraiseql_functions::outbound::EmailTransport>,
    /// The verified sending address; selects the account inside the transport.
    from:                fraiseql_functions::outbound::SenderIdentity,
    /// `{token}`-templated reset link. `None` disables reset delivery.
    reset_url_template:  Option<String>,
    /// `{code}`-templated magic link. `None` sends the bare code.
    magic_link_template: Option<String>,
}

#[cfg(feature = "inbound-email")]
impl MailboxEmailSender {
    async fn send(&self, to: &str, subject: &str, body: String) -> Result<()> {
        let request = fraiseql_functions::outbound::SendEmailRequest {
            to:       to.to_string(),
            subject:  subject.to_string(),
            text:     Some(body),
            html:     None,
            reply_to: None,
        };
        // No send-id: these are transactional auth mails, not tracked campaign
        // sends, so they carry no VERP Return-Path and no exactly-once key.
        let context = fraiseql_functions::outbound::SendContext {
            send_id: None,
            tenant:  None,
        };
        self.transport
            .send(&self.from, &request, context)
            .await
            .map(|_| ())
            .map_err(|e| ServerError::ConfigError(format!("auth email delivery failed: {e}")))
    }
}

#[cfg(feature = "inbound-email")]
#[async_trait::async_trait]
impl fraiseql_auth::ResetEmailSender for MailboxEmailSender {
    async fn send_reset_link(&self, to: &str, token: &str) -> fraiseql_auth::Result<()> {
        // `reset_url_template` is required by both the CLI validation and the
        // boot check when password auth is on, so `None` here is unreachable
        // through configuration; refuse rather than mail a bare token.
        let template = self.reset_url_template.as_ref().ok_or_else(|| {
            fraiseql_auth::AuthError::ConfigError {
                message: "[auth.local] reset_url_template is not configured".to_string(),
            }
        })?;
        let link = template.replace(RESET_TOKEN_PLACEHOLDER, &urlencoding::encode(token));
        self.send(
            to,
            "Reset your password",
            format!(
                "Use the link below to choose a new password. It expires in one hour and can \
                 only be used once.\n\n{link}\n\nIf you did not request this, you can ignore \
                 this message."
            ),
        )
        .await
        .map_err(|e| fraiseql_auth::AuthError::Internal {
            message: e.to_string(),
        })
    }
}

#[cfg(feature = "inbound-email")]
#[async_trait::async_trait]
impl fraiseql_auth::EmailDelivery for MailboxEmailSender {
    async fn send_otp(&self, email: &str, code: &str) -> fraiseql_auth::Result<String> {
        let body = self.magic_link_template.as_ref().map_or_else(
            || {
                format!(
                    "Your sign-in code is {code}. It expires in 10 minutes and can only be \
                     used once."
                )
            },
            |template| {
                let link = template.replace(MAGIC_CODE_PLACEHOLDER, &urlencoding::encode(code));
                format!(
                    "Use the link below to sign in. It expires in 10 minutes and can only be \
                     used once.\n\n{link}\n\nOr enter this code: {code}"
                )
            },
        );
        self.send(email, "Your sign-in code", body).await.map_err(|e| {
            fraiseql_auth::AuthError::Internal {
                message: e.to_string(),
            }
        })?;
        // The transport does not surface a provider message id here; the caller
        // only uses it for correlation/debugging.
        Ok(String::new())
    }
}

/// Build the mail sender for `[auth.local]`, or refuse to boot naming what is missing.
///
/// Requires the `inbound-email` feature (the `SMTP` transport lives there) and a
/// `[mailbox.<name>]` with an `smtp` half whose password env is set.
#[cfg(feature = "inbound-email")]
fn build_email_sender(
    local: &LocalAuthConfig,
    config: &ServerConfig,
    mailbox_name: &str,
) -> Result<Arc<MailboxEmailSender>> {
    let mailbox = config.mailbox.get(mailbox_name).ok_or_else(|| {
        ServerError::ConfigError(format!(
            "[auth.local] email_from = \"{mailbox_name}\" names no configured mailbox. Add a \
             [mailbox.{mailbox_name}] section, or correct the name."
        ))
    })?;
    let smtp = mailbox.smtp.as_ref().ok_or_else(|| {
        ServerError::ConfigError(format!(
            "[auth.local] email_from = \"{mailbox_name}\" names a receive-only mailbox: it has \
             no [mailbox.{mailbox_name}.smtp] section, so it cannot send OTP codes or reset \
             links."
        ))
    })?;
    let from_address = smtp.address.clone();
    let transport = crate::inbound::email::build_email_transport(
        &config.mailbox,
        |name| std::env::var(name).ok(),
        None,
        None,
    )
    .ok_or_else(|| {
        ServerError::ConfigError(format!(
            "[auth.local] email delivery could not be built from [mailbox.{mailbox_name}.smtp] \
             — its password env ({}) is unset or the relay could not be constructed. Refusing \
             to mount a login flow whose mail cannot be sent.",
            smtp.password_env
        ))
    })?;
    Ok(Arc::new(MailboxEmailSender {
        transport,
        from: fraiseql_functions::outbound::SenderIdentity {
            address:      from_address,
            display_name: None,
        },
        reset_url_template: local.reset_url_template.clone(),
        magic_link_template: local.magic_link_template.clone(),
    }))
}

/// On a build without `inbound-email` there is no `SMTP` transport at all, so a
/// method that must send mail is unservable. Fail loud with the rebuild hint
/// (the P24 feature-gate doctrine).
#[cfg(not(feature = "inbound-email"))]
fn missing_email_feature(method: &str) -> ServerError {
    ServerError::ConfigError(format!(
        "[auth.local] {method} = true needs email delivery, but this binary was built without \
         the `inbound-email` feature, which provides the SMTP transport. Rebuild with \
         `--features inbound-email`, or disable {method}."
    ))
}

/// Build every state `[auth.local]` enables.
///
/// # Errors
///
/// Returns [`ServerError::ConfigError`] when a method is enabled without what it
/// needs: no database pool, no `[auth_hs256]`, a missing or send-less mailbox, or
/// a build lacking `inbound-email`.
pub async fn build_local_auth_states(
    local: &LocalAuthConfig,
    config: &ServerConfig,
    db_pool: Option<sqlx::PgPool>,
) -> Result<LocalAuthStates> {
    // Every enabled method persists something (credentials, enrollments, OTP
    // budgets, sessions), so all of them need the signing config and the pool.
    // Config first, infrastructure second: a typo in the config file should
    // surface before "your database is missing".
    let hs = config.auth_hs256.as_ref().ok_or_else(|| {
        ServerError::ConfigError(
            "[auth.local] requires [auth_hs256]: every local sign-in mints HS256 sessions this \
             server itself validates. Add [auth_hs256] to the server config."
                .to_string(),
        )
    })?;
    let pool = db_pool.ok_or_else(|| {
        ServerError::ConfigError(
            "[auth.local] requires a database pool: credentials, MFA enrollments, OTP budgets \
             and sessions are all durable state. The binary provides one when database_url is \
             set; library embedders must pass a PgPool to Server::new."
                .to_string(),
        )
    })?;
    let secret = hs.load_secret().map_err(ServerError::ConfigError)?;
    // Mint the claims the configured validator demands, or every login would
    // "succeed" and then 401 on the first validated request.
    let session_store: Arc<dyn fraiseql_auth::SessionStore> = Arc::new(
        fraiseql_auth::PostgresSessionStore::with_hs256_secret(pool.clone(), secret.into_bytes())
            .with_token_claims(
                hs.issuer.clone().unwrap_or_else(|| {
                    fraiseql_auth::session_postgres::DEFAULT_TOKEN_ISSUER.to_string()
                }),
                hs.audience.clone().unwrap_or_else(|| {
                    fraiseql_auth::session_postgres::DEFAULT_TOKEN_AUDIENCE.to_string()
                }),
            ),
    );
    let account_store = Arc::new(fraiseql_auth::PostgresAccountStore::new(pool.clone()));

    // The mail sender is built once and shared by OTP and password reset.
    let needs_email = local.otp || local.password;
    #[cfg(feature = "inbound-email")]
    let email_sender = if needs_email {
        let mailbox_name = local.email_from.as_deref().ok_or_else(|| {
            ServerError::ConfigError(
                "[auth.local] enables a mail-sending method but sets no email_from. Name the \
                 [mailbox.<name>] account whose SMTP half should deliver OTP codes and reset \
                 links."
                    .to_string(),
            )
        })?;
        Some(build_email_sender(local, config, mailbox_name)?)
    } else {
        None
    };
    #[cfg(not(feature = "inbound-email"))]
    if needs_email {
        return Err(missing_email_feature(if local.otp { "otp" } else { "password" }));
    }

    // `needs_email` already returned above on a build without `inbound-email`,
    // so from here on an enabled mail method has a sender.
    #[cfg(feature = "inbound-email")]
    let otp = if local.otp {
        let delivery: Arc<dyn fraiseql_auth::EmailDelivery> = email_sender
            .clone()
            .ok_or_else(|| ServerError::ConfigError("[auth.local] otp needs email".into()))?;
        info!("Local OTP sign-in enabled (POST /auth/v1/otp, POST /auth/v1/verify)");
        Some(Arc::new(fraiseql_auth::OtpRouteState {
            otp_store:      Arc::new(fraiseql_auth::PgOtpStore::new(pool.clone())),
            email_delivery: delivery,
            session_store:  Arc::clone(&session_store),
            // #367: OTP identities resolve through the account store, so the
            // same person's OTP, social and password sign-ins converge on one
            // account instead of a parallel `otp:<email>` pseudo-identity.
            account_store:  Some(Arc::clone(&account_store) as Arc<dyn fraiseql_auth::AccountStore>),
        }))
    } else {
        None
    };
    #[cfg(not(feature = "inbound-email"))]
    let otp = None;

    let mfa = if local.mfa {
        info!("TOTP MFA enabled (POST /auth/v1/mfa/{{enroll,challenge,verify,unenroll}})");
        Some(Arc::new(fraiseql_auth::MfaRouteState {
            mfa_store:     Arc::new(fraiseql_auth::PgMfaStore::new(pool.clone())),
            session_store: Arc::clone(&session_store),
            issuer:        local
                .mfa_issuer
                .clone()
                .unwrap_or_else(|| DEFAULT_MFA_ISSUER.to_string()),
        }))
    } else {
        None
    };

    let password = if local.password {
        // Reason: `mut` is only used by the inbound-email-gated sender attachment below
        #[cfg_attr(not(feature = "inbound-email"), allow(unused_mut))]
        let mut authenticator = fraiseql_auth::LocalPasswordAuthenticator::new(
            pool,
            Arc::clone(&account_store) as Arc<dyn fraiseql_auth::AccountStore>,
        )
        .with_session_store(Arc::clone(&session_store));
        #[cfg(feature = "inbound-email")]
        {
            let sender = email_sender.ok_or_else(|| {
                ServerError::ConfigError("[auth.local] password needs email".into())
            })?;
            authenticator =
                authenticator.with_email_sender(sender as Arc<dyn fraiseql_auth::ResetEmailSender>);
        }
        info!("Local password sign-in enabled (POST /auth/v1/password/{{signup,login,reset}})");
        Some(Arc::new(fraiseql_auth::LocalPasswordRouteState {
            authenticator: Arc::new(authenticator),
            session_store: Arc::clone(&session_store),
            rate_limiters: Arc::new(fraiseql_auth::RateLimiters::default()),
        }))
    } else {
        None
    };

    let anon = if local.anonymous {
        tracing::warn!(
            "[auth.local] anonymous = true: POST /auth/v1/signup issues a session to any \
             caller with no credentials. Ensure downstream authorization treats anon_* \
             subjects as untrusted."
        );
        Some(Arc::new(fraiseql_auth::AnonSignupState::new(Arc::clone(&session_store))))
    } else {
        None
    };

    Ok(LocalAuthStates {
        otp,
        mfa,
        password,
        anon,
    })
}
