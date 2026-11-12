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

/// What a reset or verification link template substitutes the opaque token for. Kept as a
/// constant so the CLI validation, the docs and this substitution cannot drift on the spelling.
#[cfg(feature = "inbound-email")]
const LINK_TOKEN: &str = "token";

/// What the magic-link template substitutes the OTP code for. A link carrying only the code
/// needs the address too, so it signs in only where the address is known.
#[cfg(feature = "inbound-email")]
const MAGIC_CODE: &str = "code";

/// The built-in subjects, sent when `[auth.local.email]` sets none.
#[cfg(feature = "inbound-email")]
const OTP_SUBJECT: &str = "Your sign-in code";
#[cfg(feature = "inbound-email")]
const RESET_SUBJECT: &str = "Reset your password";
#[cfg(feature = "inbound-email")]
const VERIFICATION_SUBJECT: &str = "Verify your email address";

/// Default `MFA` issuer shown in authenticator apps.
const DEFAULT_MFA_ISSUER: &str = "FraiseQL";

/// The states `[auth.local]` produces, each `Some` only when its method is enabled.
pub struct LocalAuthStates {
    pub otp:                Option<Arc<fraiseql_auth::OtpRouteState>>,
    pub mfa:                Option<Arc<fraiseql_auth::MfaRouteState>>,
    pub password:           Option<Arc<fraiseql_auth::LocalPasswordRouteState>>,
    pub anon:               Option<Arc<fraiseql_auth::AnonSignupState>>,
    pub email_verification: Option<Arc<fraiseql_auth::EmailVerificationRouteState>>,
}

/// Delivers `OTP` codes and password-reset links through a configured
/// `[mailbox.<name>.smtp]` account (#367).
///
/// This is the concrete sender the reset flow documented but never had. It relays
/// through the same transport the `send_email` host op uses, so a deployment
/// configures its outbound mail **once**.
#[cfg(feature = "inbound-email")]
pub struct MailboxEmailSender {
    transport: Arc<dyn fraiseql_functions::outbound::EmailTransport>,
    /// The verified sending address; selects the account inside the transport.
    from: fraiseql_functions::outbound::SenderIdentity,
    /// `{token}`-templated reset link. `None` disables reset delivery.
    reset_url_template: Option<String>,
    /// `{code}`-templated magic link. `None` sends the bare code.
    magic_link_template: Option<String>,
    /// `{token}`-templated email-verification link. `None` disables verification
    /// delivery.
    verification_url_template: Option<String>,
    /// The operator's subjects and bodies, checked at boot. A kind left unset sends the
    /// built-in text.
    email: fraiseql_core::schema::LocalEmailConfig,
}

#[cfg(feature = "inbound-email")]
impl MailboxEmailSender {
    /// Send the `template` mail, or the built-in one where it sets no subject or body.
    /// `values` fill the template's placeholders, escaped in its `HTML` body.
    async fn send(
        &self,
        to: &str,
        template: Option<&fraiseql_core::schema::EmailTemplate>,
        built_in: (&str, String),
        values: &[(&str, &str)],
    ) -> fraiseql_auth::Result<()> {
        use fraiseql_core::schema::render_email_template;

        let (built_in_subject, built_in_text) = built_in;
        let subject = template.and_then(|t| t.subject.as_deref()).unwrap_or(built_in_subject);
        let text = template
            .and_then(|t| t.text.as_deref())
            .map_or(built_in_text, |text| render_email_template(text, values, false));
        let html = template
            .and_then(|t| t.html.as_deref())
            .map(|html| render_email_template(html, values, true));
        let request = fraiseql_functions::outbound::SendEmailRequest {
            to: to.to_string(),
            subject: subject.to_string(),
            text: Some(text),
            html,
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
            .map_err(|e| fraiseql_auth::AuthError::Internal {
                message: format!("auth email delivery failed: {e}"),
            })
    }
}

/// Whole minutes of a lifetime in seconds, for `{expires_minutes}`.
#[cfg(feature = "inbound-email")]
fn minutes(secs: u64) -> String {
    (secs / 60).to_string()
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
        let link = fraiseql_core::schema::render_link_template(template, &[(LINK_TOKEN, token)]);
        let expires = minutes(fraiseql_auth::RESET_TOKEN_TTL_SECS.unsigned_abs());
        self.send(
            to,
            self.email.reset.as_ref(),
            (
                RESET_SUBJECT,
                format!(
                    "Use the link below to choose a new password. It expires in one hour and \
                     can only be used once.\n\n{link}\n\nIf you did not request this, you can \
                     ignore this message."
                ),
            ),
            &[("link", &link), ("expires_minutes", &expires)],
        )
        .await
    }
}

#[cfg(feature = "inbound-email")]
#[async_trait::async_trait]
impl fraiseql_auth::VerificationEmailSender for MailboxEmailSender {
    async fn send_verification_link(&self, to: &str, token: &str) -> fraiseql_auth::Result<()> {
        // Required by both the CLI validation and the boot check when
        // email_verification is on, so `None` here is unreachable through
        // configuration; refuse rather than mail a bare token.
        let template = self.verification_url_template.as_ref().ok_or_else(|| {
            fraiseql_auth::AuthError::ConfigError {
                message: "[auth.local] verification_url_template is not configured".to_string(),
            }
        })?;
        let link = fraiseql_core::schema::render_link_template(template, &[(LINK_TOKEN, token)]);
        let expires = minutes(fraiseql_auth::EMAIL_VERIFICATION_TOKEN_TTL_SECS.unsigned_abs());
        self.send(
            to,
            self.email.verification.as_ref(),
            (
                VERIFICATION_SUBJECT,
                format!(
                    "Use the link below to confirm this address. It expires in one hour and \
                     can only be used once, and it only works while you are signed in to the \
                     account that asked for it.\n\n{link}\n\nIf you did not request this, you \
                     can ignore this message — nothing changes until the link is used."
                ),
            ),
            &[("link", &link), ("expires_minutes", &expires)],
        )
        .await
    }
}

#[cfg(feature = "inbound-email")]
#[async_trait::async_trait]
impl fraiseql_auth::EmailDelivery for MailboxEmailSender {
    async fn send_otp(
        &self,
        email: &str,
        issue: &fraiseql_auth::OtpIssue,
    ) -> fraiseql_auth::Result<String> {
        let code = issue.code.as_str();
        let link = self.magic_link_template.as_deref().map(|template| {
            fraiseql_core::schema::render_link_template(
                template,
                &[(LINK_TOKEN, &issue.link_token), (MAGIC_CODE, code)],
            )
        });
        let built_in = link.as_ref().map_or_else(
            || {
                format!(
                    "Your sign-in code is {code}. It expires in 10 minutes and can only be \
                     used once."
                )
            },
            |link| {
                format!(
                    "Use the link below to sign in. It expires in 10 minutes and can only be \
                     used once.\n\n{link}\n\nOr enter this code: {code}"
                )
            },
        );
        let expires = minutes(fraiseql_auth::OTP_TTL_SECS);
        // Boot refuses a template with `{link}` and no magic-link template, so an empty link
        // is never substituted.
        self.send(
            email,
            self.email.otp.as_ref(),
            (OTP_SUBJECT, built_in),
            &[
                ("code", code),
                ("link", link.as_deref().unwrap_or_default()),
                ("expires_minutes", &expires),
            ],
        )
        .await?;
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
            display_name: local.email_from_name.clone(),
        },
        reset_url_template: local.reset_url_template.clone(),
        magic_link_template: local.magic_link_template.clone(),
        verification_url_template: local.verification_url_template.clone(),
        email: local.email.as_deref().cloned().unwrap_or_default(),
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

/// The Postgres session store every FraiseQL sign-in mints through (#1450).
///
/// Signed with the first-party issuer's active key (`[auth_issuer]`), naming its issuer and
/// audience, and the account's tenant under the schema's `tenant_claim`, so a minted
/// token's tenant reaches `SecurityContext::tenant_id` exactly as an external identity provider's
/// does. One constructor for local, social and SAML sign-in: a new path cannot mint tokens the
/// validator rejects, or drop the tenant.
///
/// `lifetime` is the `[auth.session]` lifetime ([`session_lifetime`]); anonymous sign-up
/// passes the default, and keeps its fixed seven days.
pub fn minting_session_store(
    pool: sqlx::PgPool,
    issuer: &crate::first_party::FirstPartyIssuer,
    tenant_claim: &str,
    lifetime: fraiseql_auth::SessionLifetime,
) -> fraiseql_auth::PostgresSessionStore {
    fraiseql_auth::PostgresSessionStore::with_signer(pool, issuer.signer())
        .with_token_claims(issuer.config().url.clone(), issuer.config().audience.clone())
        .with_tenant_claim(tenant_claim)
        .with_lifetime(lifetime)
}

/// The lifetime `[auth.session]` asks of every session a sign-in mints (#1853). Unset, each
/// sign-in path keeps its own expiry and no session is refreshable.
#[must_use]
pub fn session_lifetime(
    session: Option<&fraiseql_core::schema::SessionConfig>,
) -> fraiseql_auth::SessionLifetime {
    session.map_or_else(fraiseql_auth::SessionLifetime::default, |s| {
        fraiseql_auth::SessionLifetime {
            access_ttl_secs:   s.ttl_secs,
            absolute_ttl_secs: s.absolute_ttl_secs,
        }
    })
}

/// The store behind `POST /auth/v1/session/{refresh,logout}`: `None` unless
/// `[auth.session] absolute_ttl_secs` opts in to refresh (#1853).
///
/// It signs with the same secret, claims and lifetime as the stores that minted the
/// sessions, so a refreshed pair is indistinguishable from a sign-in's.
///
/// # Errors
///
/// [`ServerError::ConfigError`] when refresh is asked for without `[auth_issuer]` or a
/// database pool, or with no sign-in method that mints a refreshable session.
pub fn session_routes_store(
    issuer: Option<&crate::first_party::FirstPartyIssuer>,
    db_pool: Option<&sqlx::PgPool>,
    tenant_claim: &str,
    lifetime: fraiseql_auth::SessionLifetime,
    mints_refreshable_sessions: bool,
) -> Result<Option<Arc<fraiseql_auth::PostgresSessionStore>>> {
    let Some(absolute) = lifetime.absolute_ttl_secs else {
        return Ok(None);
    };
    if !mints_refreshable_sessions {
        return Err(ServerError::ConfigError(format!(
            "[auth.session] absolute_ttl_secs = {absolute} asks for refreshable sessions, but \
             no sign-in method that mints one is enabled: enable password, otp, \
             [auth.social] or [saml], or remove absolute_ttl_secs. Anonymous sessions are \
             never refreshable."
        )));
    }
    let issuer = issuer.ok_or_else(|| {
        ServerError::ConfigError(
            "[auth.session] absolute_ttl_secs requires [auth_issuer]: a refresh mints a new \
             access token this server signs and validates."
                .to_string(),
        )
    })?;
    let pool = db_pool.ok_or_else(|| {
        ServerError::ConfigError(
            "[auth.session] absolute_ttl_secs requires a database pool: refresh tokens are \
             rows in auth.sessions."
                .to_string(),
        )
    })?;
    Ok(Some(Arc::new(minting_session_store(
        pool.clone(),
        issuer,
        tenant_claim,
        lifetime,
    ))))
}

/// Refuse to boot with `MFA` off while an account has a confirmed enrollment.
///
/// Such an account's every sign-in owes the second factor wherever its session is minted,
/// whatever this server enables, and only `[auth.local] mfa = true` mounts the
/// `/auth/v1/mfa/verify` that takes it. Booting would lock those accounts out without a
/// word. The refusal names both ways forward: turn `MFA` back on, or remove the
/// enrollments deliberately.
///
/// Only a count read from the store refuses. A store this cannot read is logged and let
/// through: the sign-in itself refuses on the same failure, so nothing is skipped.
///
/// # Errors
///
/// [`ServerError::ConfigError`] when confirmed enrollments exist.
pub async fn refuse_stranded_mfa_enrollments(pool: &sqlx::PgPool) -> Result<()> {
    let confirmed =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM auth.tb_mfa_enrollment WHERE confirmed")
            .fetch_one(pool)
            .await;
    match confirmed {
        Ok(0) => Ok(()),
        Ok(accounts) => Err(ServerError::ConfigError(format!(
            "{accounts} account(s) have a confirmed TOTP MFA enrollment, but [auth.local] mfa \
             is not enabled. Their sign-ins owe a second factor that only \
             POST /auth/v1/mfa/verify takes, and it is mounted only with mfa = true, so they \
             could not sign in at all. Either set [auth.local] mfa = true, or remove the \
             enrollments deliberately: DELETE FROM auth.tb_mfa_challenge; \
             DELETE FROM auth.tb_mfa_enrollment;"
        ))),
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42P01") => Ok(()),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "could not count MFA enrollments at boot; a sign-in that cannot read them \
                 refuses on its own"
            );
            Ok(())
        },
    }
}

/// Cookie transport's settings (`[auth.local] session_transport = "cookie"`, #1856), or
/// `None` in the default `json` transport.
///
/// # Errors
///
/// Refuses cookie transport that could not be made safe: without `[auth.session]
/// absolute_ttl_secs` nothing mounts the logout route that clears the cookies, and with
/// `require_json_content_type = false` `/graphql` would run a form another site posts,
/// carrying the cookie.
pub fn session_cookies(
    auth: Option<&fraiseql_core::schema::AuthClientConfig>,
    config: &ServerConfig,
) -> Result<Option<fraiseql_auth::SessionCookies>> {
    let Some(local) = auth.and_then(|a| a.local.as_ref()) else {
        return Ok(None);
    };
    let session = auth.and_then(|a| a.session.as_ref());
    local.check_session_transport(session).map_err(ServerError::ConfigError)?;
    if local.session_transport.is_json() {
        return Ok(None);
    }
    if !config.require_json_content_type {
        return Err(ServerError::ConfigError(
            "[auth.local] session_transport = \"cookie\" requires require_json_content_type = \
             true: the session cookie is a credential on /graphql, and that check is what \
             refuses a form another site posts there"
                .to_string(),
        ));
    }
    Ok(session.and_then(|s| s.absolute_ttl_secs).map(|refresh_max_age_secs| {
        fraiseql_auth::SessionCookies {
            refresh_max_age_secs,
        }
    }))
}

/// Build every state `[auth.local]` enables.
///
/// # Errors
///
/// Returns [`ServerError::ConfigError`] when a method is enabled without what it
/// needs: no database pool, no `[auth_issuer]`, a missing or send-less mailbox, or
/// a build lacking `inbound-email`.
pub async fn build_local_auth_states(
    local: &LocalAuthConfig,
    // Reason: only the inbound-email-gated mail sender reads the server config.
    #[cfg_attr(not(feature = "inbound-email"), allow(unused_variables))] config: &ServerConfig,
    issuer: Option<&crate::first_party::FirstPartyIssuer>,
    db_pool: Option<sqlx::PgPool>,
    tenant_claim: &str,
    lifetime: fraiseql_auth::SessionLifetime,
    session_cookies: Option<fraiseql_auth::SessionCookies>,
) -> Result<LocalAuthStates> {
    // Every enabled method persists something (credentials, enrollments, OTP
    // budgets, sessions), so all of them need the signing config and the pool.
    // Config first, infrastructure second: a typo in the config file should
    // surface before "your database is missing".
    let issuer = issuer.ok_or_else(|| {
        ServerError::ConfigError(
            "[auth.local] requires [auth_issuer]: every local sign-in mints sessions this \
             server itself signs and validates. Add [auth_issuer] to the server config."
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
    // The compiler refuses both shapes; this is the same check for a hand-written config.
    // Reason: only the inbound-email-gated OTP state below consumes the statement
    #[cfg_attr(not(feature = "inbound-email"), allow(unused_variables))]
    let otp_allowed_statement = local
        .otp_allowed_query
        .as_deref()
        .map(|query| {
            if !local.otp {
                return Err(ServerError::ConfigError(
                    "[auth.local] otp_allowed_query is set, but otp is not enabled.".to_string(),
                ));
            }
            fraiseql_core::schema::otp_allowed_statement(query).map_err(|problem| {
                ServerError::ConfigError(format!(
                    "[auth.local] otp_allowed_query is refused: {problem}: {query}"
                ))
            })
        })
        .transpose()?;
    local.check_email().map_err(ServerError::ConfigError)?;
    // Mint the claims the validator demands, or every login would "succeed" and then 401
    // on the first validated request. The routes that act on the caller's own account
    // validate against that same issuer, so what this server signs is exactly what it
    // accepts back (#945).
    let session_store: Arc<dyn fraiseql_auth::SessionStore> =
        Arc::new(minting_session_store(pool.clone(), issuer, tenant_claim, lifetime));
    // Anonymous sign-up mints through its own store, without `[auth.session]`: an
    // anonymous session keeps its fixed seven days and is never refreshable.
    let anon_session_store = local.anonymous.then(|| {
        Arc::new(minting_session_store(
            pool.clone(),
            issuer,
            tenant_claim,
            fraiseql_auth::SessionLifetime::default(),
        ))
    });
    let account_store = Arc::new(fraiseql_auth::PostgresAccountStore::new(pool.clone()));

    // The mail sender is built once and shared by OTP, password reset and email
    // verification.
    let needs_email = local.otp || local.password || local.email_verification;
    #[cfg(feature = "inbound-email")]
    let email_sender = if needs_email {
        let mailbox_name = local.email_from.as_deref().ok_or_else(|| {
            ServerError::ConfigError(
                "[auth.local] enables a mail-sending method but sets no email_from. Name the \
                 [mailbox.<name>] account whose SMTP half should deliver OTP codes, reset \
                 links and verification links."
                    .to_string(),
            )
        })?;
        Some(build_email_sender(local, config, mailbox_name)?)
    } else {
        None
    };
    #[cfg(not(feature = "inbound-email"))]
    if needs_email {
        return Err(missing_email_feature(if local.otp {
            "otp"
        } else if local.password {
            "password"
        } else {
            "email_verification"
        }));
    }

    // `needs_email` already returned above on a build without `inbound-email`,
    // so from here on an enabled mail method has a sender.
    #[cfg(feature = "inbound-email")]
    let otp = if local.otp {
        let delivery: Arc<dyn fraiseql_auth::EmailDelivery> = email_sender
            .clone()
            .ok_or_else(|| ServerError::ConfigError("[auth.local] otp needs email".into()))?;
        info!("Local OTP sign-in enabled (POST /auth/v1/otp, POST /auth/v1/verify)");
        let allow_gate = otp_allowed_statement.map(|statement| {
            fraiseql_auth::OtpAllowGate::new(
                Arc::new(fraiseql_auth::PgOtpAllowList::new(pool.clone(), statement)),
                Arc::clone(&delivery),
                fraiseql_auth::OTP_DELIVERY_QUEUE_CAPACITY,
            )
        });
        Some(Arc::new(fraiseql_auth::OtpRouteState {
            otp_store: Arc::new(fraiseql_auth::PgOtpStore::new(pool.clone())),
            email_delivery: delivery,
            session_store: Arc::clone(&session_store),
            // #367: OTP identities resolve through the account store, so the
            // same person's OTP, social and password sign-ins converge on one
            // account instead of a parallel `otp:<email>` pseudo-identity.
            account_store: Some(Arc::clone(&account_store) as Arc<dyn fraiseql_auth::AccountStore>),
            allow_gate,
            session_cookies,
        }))
    } else {
        None
    };
    #[cfg(not(feature = "inbound-email"))]
    let otp = None;

    let mfa = if local.mfa {
        info!("TOTP MFA enabled (POST /auth/v1/mfa/{{enroll,confirm,verify,unenroll}})");
        Some(Arc::new(fraiseql_auth::MfaRouteState {
            mfa_store: Arc::new(fraiseql_auth::PgMfaStore::new(pool.clone())),
            session_store: Arc::clone(&session_store),
            issuer: local.mfa_issuer.clone().unwrap_or_else(|| DEFAULT_MFA_ISSUER.to_string()),
            verify_limiter: Arc::new(fraiseql_auth::KeyedRateLimiter::new(
                fraiseql_auth::AuthRateLimitConfig::per_ip_strict(),
            )),
            session_cookies,
        }))
    } else {
        None
    };

    // One authenticator, shared by the password routes and the verification routes:
    // they operate on the same credential and token stores, and two instances would
    // be two configurations to keep in step.
    let authenticator = if local.password {
        // Reason: `mut` is only used by the inbound-email-gated sender attachments below
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
            authenticator = authenticator
                .with_email_sender(Arc::clone(&sender) as Arc<dyn fraiseql_auth::ResetEmailSender>);
            if local.email_verification {
                authenticator = authenticator.with_verification_email_sender(
                    sender as Arc<dyn fraiseql_auth::VerificationEmailSender>,
                );
            }
        }
        Some(Arc::new(authenticator))
    } else {
        None
    };

    let password = authenticator.as_ref().map(|authenticator| {
        info!("Local password sign-in enabled (POST /auth/v1/password/{{signup,login,reset}})");
        Arc::new(fraiseql_auth::LocalPasswordRouteState {
            authenticator: Arc::clone(authenticator),
            session_store: Arc::clone(&session_store),
            // The server's configured per-route rules limit these.
            rate_limiters: None,
            session_cookies,
        })
    });

    // #945. Refuse rather than mount a flow that dead-ends: verification proves the
    // address a *local* identity claims, and a link nobody can follow is not a
    // verification method. The CLI refuses both shapes at compile time; this is the
    // same check for a server booted from a hand-written or env-overridden config.
    let email_verification = if local.email_verification {
        let authenticator = authenticator.as_ref().ok_or_else(|| {
            ServerError::ConfigError(
                "[auth.local] email_verification = true requires password = true: \
                 verification proves the address a local password identity claims, and there \
                 is no such identity without it."
                    .to_string(),
            )
        })?;
        if local.verification_url_template.is_none() {
            return Err(ServerError::ConfigError(
                "[auth.local] email_verification = true requires verification_url_template — \
                 the verification link points at your front end, which FraiseQL cannot guess. \
                 Example: verification_url_template = \
                 \"https://app.example.com/verify-email?token={token}\""
                    .to_string(),
            ));
        }
        info!(
            "Email verification enabled (POST /auth/v1/email/verify/{{start,confirm}}, \
             authenticated)"
        );
        Some(Arc::new(fraiseql_auth::EmailVerificationRouteState {
            authenticator: Arc::clone(authenticator),
            // The server's configured per-route rules limit these.
            rate_limiters: None,
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
        // The server's configured per-client rule limits /auth/v1/signup, keyed on the
        // trusted-proxy client address; the state keeps no counters of its own.
        anon_session_store.map(|store| {
            Arc::new(
                fraiseql_auth::AnonSignupState::new_without_limiter(
                    store as Arc<dyn fraiseql_auth::SessionStore>,
                )
                .with_session_cookies(session_cookies),
            )
        })
    } else {
        None
    };

    Ok(LocalAuthStates {
        otp,
        mfa,
        password,
        anon,
        email_verification,
    })
}
