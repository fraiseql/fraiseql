//! Account linking — merge provider identities sharing the same verified email.
//!
//! When a user authenticates with two different `OAuth` providers (e.g. GitHub then Google)
//! using the same email address, this module ensures they receive the **same `user_id`**
//! rather than two separate user records.
//!
//! # How it works
//!
//! 1. After a successful `OAuth` token exchange, call [`AccountStore::link_or_create_user`] with
//!    the email (and its verified flag), provider name, and provider-specific user ID.
//! 2. The store resolves an identity key. Cross-provider linking happens **only** when the provider
//!    supplies a non-empty, verified email; otherwise the identity is keyed on `(provider,
//!    provider_id)` so that an absent or unverified email can never collapse two distinct provider
//!    identities into one account (see [`AccountStore::link_or_create_user`]).
//!    - **Existing account**: the new provider credential is linked to the existing account and the
//!      existing `user_id` is returned.
//!    - **New account**: a fresh `user_id` is generated, the account is stored, and the new
//!      `user_id` is returned.
//! 3. The caller creates or refreshes a session keyed by the returned `user_id`.

use std::collections::HashSet;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::{AuthError, Result},
};

mod postgres;
pub use postgres::{PostgresAccountStore, SCHEMA_SQL};

// ─── Domain types ─────────────────────────────────────────────────────────────

/// A single provider credential linked to an account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderLink {
    /// Provider name (e.g. `"github"`, `"google"`).
    pub provider:    String,
    /// Provider-specific user identifier (opaque string from the provider).
    pub provider_id: String,
}

/// A FraiseQL user account, potentially linked to multiple `OAuth` providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecord {
    /// Internal FraiseQL user identifier (stable across providers).
    pub user_id:   String,
    /// Verified email address shared across all linked providers, when the account
    /// is keyed on a verified email. `None` for accounts keyed on
    /// `(provider, provider_id)` because the provider supplied no verified email.
    pub email:     Option<String>,
    /// All provider credentials linked to this account.
    pub providers: Vec<ProviderLink>,
}

// ─── Trait ────────────────────────────────────────────────────────────────────

/// Storage backend for account linking.
///
/// Implementations must be `Send + Sync` and handle concurrent access safely.
///
/// # Implementations
///
/// - [`InMemoryAccountStore`] — for single-node deployments and testing.
// Reason: used as dyn Trait (Arc<dyn AccountStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait AccountStore: Send + Sync {
    /// Return the `user_id` for the given identity, creating or linking as needed.
    ///
    /// # Account-linking key (security-critical)
    ///
    /// Cross-provider account linking happens **only** when the provider supplies a
    /// non-empty, *verified* email. Otherwise each `(provider, provider_id)` pair is its
    /// own account:
    ///
    /// - `email = Some(non-empty)` **and** `email_verified = true` → identity is keyed on the
    ///   normalized email. A second provider presenting the same verified email links into the
    ///   existing account.
    /// - `email = None`, empty/whitespace, **or** `email_verified = false` → identity is keyed on
    ///   `(provider, provider_id)`. This is fail-closed: an absent or unverified email can never
    ///   collapse two distinct provider identities into one account, and can never link into
    ///   another user's email-keyed account (H26).
    ///
    /// # Semantics
    ///
    /// - If no account exists for the resolved identity key: creates a new account, stores the
    ///   `provider` / `provider_id` link, and returns the new `user_id`.
    /// - If an account already exists for the key:
    ///   - If the `provider` / `provider_id` pair is new, adds it as a linked credential.
    ///   - Returns the **existing** `user_id` (same as on first sign-in).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the backing store fails.
    async fn link_or_create_user(
        &self,
        email: Option<&str>,
        email_verified: bool,
        provider: &str,
        provider_id: &str,
    ) -> Result<AccountLinkResult>;

    /// Look up the full account record for a `user_id`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::TokenNotFound`] if no account exists for `user_id`.
    async fn get_account(&self, user_id: &str) -> Result<AccountRecord>;
}

/// Result from [`AccountStore::link_or_create_user`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountLinkResult {
    /// Stable internal user identifier.
    pub user_id: String,
    /// Whether a new account was created (`true`) or an existing one was linked (`false`).
    pub is_new:  bool,
    /// Whether a new provider link was added to an existing account.
    pub linked:  bool,
}

// ─── In-memory backend ────────────────────────────────────────────────────────

/// Thread-safe in-memory account store.
///
/// **Warning**: data is lost on process restart. For production use a persistent
/// backend (PostgreSQL, etc.). Suitable for single-node deployments and tests.
///
/// # Thread Safety
///
/// Uses `DashMap` for lock-free concurrent reads and fine-grained write locking.
pub struct InMemoryAccountStore {
    /// identity key → user_id (fast lookup). The key is either `email:<normalized>`
    /// for verified-email identities or `provider:<provider>\u{1f}<provider_id>` for
    /// email-less / unverified identities — see [`identity_key`].
    by_identity: DashMap<String, String>,
    /// user_id → AccountRecord
    by_user_id:  DashMap<String, AccountRecord>,
}

impl InMemoryAccountStore {
    /// Create a new empty account store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_identity: DashMap::new(),
            by_user_id:  DashMap::new(),
        }
    }

    /// Return the number of accounts in the store (useful for tests).
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_user_id.len()
    }

    /// Return `true` if no accounts are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_user_id.is_empty()
    }
}

impl Default for InMemoryAccountStore {
    fn default() -> Self {
        Self::new()
    }
}

// Reason: async_trait required for dyn-compatibility; remove when RTN + Send is stable
#[async_trait]
impl AccountStore for InMemoryAccountStore {
    async fn link_or_create_user(
        &self,
        email: Option<&str>,
        email_verified: bool,
        provider: &str,
        provider_id: &str,
    ) -> Result<AccountLinkResult> {
        let logger = get_audit_logger();
        // Resolve the linking key. A verified, non-empty email links across providers;
        // anything else is keyed on (provider, provider_id) so distinct identities can
        // never collapse (H26).
        let verified_email = email.map(normalize_email).filter(|e| !e.is_empty() && email_verified);
        let key = identity_key(verified_email.as_deref(), provider, provider_id);
        let new_link = ProviderLink {
            provider:    provider.to_string(),
            provider_id: provider_id.to_string(),
        };

        // Check whether an account already exists for this identity.
        if let Some(existing_user_id) = self.by_identity.get(&key).map(|r| r.clone()) {
            let mut record = self.by_user_id.get_mut(&existing_user_id).ok_or_else(|| {
                AuthError::DatabaseError {
                    message: format!(
                        "account store inconsistency: identity '{key}' maps to missing user_id \
                         '{existing_user_id}'"
                    ),
                }
            })?;

            // Link the new provider if it isn't already present.
            let already_linked = record.providers.contains(&new_link);
            if !already_linked {
                record.providers.push(new_link);
                logger.log_success(
                    AuditEventType::AuthSuccess,
                    SecretType::SessionToken,
                    Some(existing_user_id.clone()),
                    &format!("account_linked:{provider}"),
                );
            }

            return Ok(AccountLinkResult {
                user_id: existing_user_id.clone(),
                is_new:  false,
                linked:  !already_linked,
            });
        }

        // No existing account — create a new one.
        let user_id = format!("user_{}", Uuid::new_v4().as_simple());
        let record = AccountRecord {
            user_id:   user_id.clone(),
            email:     verified_email,
            providers: vec![new_link],
        };
        self.by_identity.insert(key, user_id.clone());
        self.by_user_id.insert(user_id.clone(), record);

        logger.log_success(
            AuditEventType::SessionTokenCreated,
            SecretType::SessionToken,
            Some(user_id.clone()),
            &format!("account_created:{provider}"),
        );

        Ok(AccountLinkResult {
            user_id,
            is_new: true,
            linked: false,
        })
    }

    async fn get_account(&self, user_id: &str) -> Result<AccountRecord> {
        self.by_user_id.get(user_id).map(|r| r.clone()).ok_or(AuthError::TokenNotFound)
    }
}

// ─── Helper ───────────────────────────────────────────────────────────────────

/// Normalize an email address for storage and lookup.
///
/// Converts to lowercase and trims whitespace so that `Alice@Example.com` and
/// `alice@example.com` resolve to the same account.
#[must_use]
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Compute the account-linking key for an identity.
///
/// When `verified_email` is `Some`, the identity links across providers and is keyed on
/// `email:<normalized>`. When `None` (the provider supplied no verified, non-empty email),
/// the identity is unique to the `(provider, provider_id)` pair, keyed on
/// `provider:<provider>\u{1f}<provider_id>` (`\u{1f}`, the ASCII unit separator, cannot
/// appear in a provider name, so the two key spaces and distinct pairs never collide).
fn identity_key(verified_email: Option<&str>, provider: &str, provider_id: &str) -> String {
    match verified_email {
        Some(email) => format!("email:{email}"),
        None => format!("provider:{provider}\u{1f}{provider_id}"),
    }
}

// ─── Provider email-trust policy (#368) ─────────────────────────────────────────

/// Set of `OAuth`/OIDC providers whose `email_verified` assertion FraiseQL trusts for
/// **cross-provider auto-linking**.
///
/// # Why this exists (the H26 risk, one level up)
///
/// [`AccountStore::link_or_create_user`] merges two provider identities onto one account
/// when they present the same *verified* email (H26). That is only safe if the provider
/// asserting `email_verified = true` actually verified the address. A misconfigured or
/// self-hosted IdP — or any provider that lets a user self-assert their email — can claim
/// `email_verified = true` for an address it never owned and thereby link into another
/// user's account (account takeover). This policy is the gate: a provider's verified claim
/// is honored for auto-linking **only** when the provider is in the trusted set. An
/// untrusted provider's claim is treated as unverified, so its identity is keyed on
/// `(provider, provider_id)` and can never collapse into an existing email-keyed account
/// (fail-closed — the same posture as H26).
///
/// # Both sides of the merge
///
/// The gate reasons about the *incoming* provider, but the merge is also safe on the
/// *existing*-account side by construction: only a verified email from a trusted source
/// ever enters the merge-able `email:<normalized>` key space. Unverified identities
/// (local-password and phone sign-ups pass `email_verified = false`; see [`crate::local_password`])
/// live in the `(provider, provider_id)` key space and are therefore structurally never
/// absorbed by a later trusted sign-in — closing the classic pre-hijack where an
/// attacker pre-seeds an unverified local account under the victim's email.
///
/// # Default trusted set
///
/// [`TrustedEmailProviders::default`] (and [`builtin_default`](Self::builtin_default))
/// trust exactly:
///
/// - `google` — Google issues the OIDC `email_verified` claim in the signed ID token and sets it
///   from its own verification of the address (or domain ownership for Workspace); it is meaningful
///   to rely on.
/// - `apple` — Apple issues the address itself (including Private Relay aliases) and always
///   verifies ownership, so its `email_verified` claim is authoritative.
///
/// Deliberately **excluded** from the default (opt in explicitly once vetted):
///
/// - `azure_ad` / Microsoft — the `email` claim is **not** reliably verified and is tenant-mutable
///   (the *nOAuth* class, 2023), so it must not auto-link by default.
/// - `github` — a verified primary email requires the `/user/emails` second-hop, which is not yet
///   implemented; its `email_verified` is fail-closed to `false`.
/// - any generic/custom OIDC provider — FraiseQL cannot vouch for an operator-run IdP.
///
/// # Overriding (up *and* down)
///
/// Trust is trivially adjustable in either direction and is meant to read explicitly at
/// the wiring site:
///
/// ```
/// use fraiseql_auth::TrustedEmailProviders;
///
/// // Add a vetted provider on top of the defaults.
/// let extended = TrustedEmailProviders::default().trust("keycloak");
/// assert!(extended.is_trusted("keycloak") && extended.is_trusted("google"));
///
/// // Drop a default.
/// let narrowed = TrustedEmailProviders::default().distrust("apple");
/// assert!(!narrowed.is_trusted("apple"));
///
/// // High-assurance deployments: trust no one, in one call.
/// let strict = TrustedEmailProviders::none();
/// assert!(!strict.is_trusted("google"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedEmailProviders {
    /// Normalized (lowercased, trimmed) provider names trusted to assert email verification.
    providers: HashSet<String>,
}

impl TrustedEmailProviders {
    /// The built-in default trusted set: `google` and `apple` (see the type docs for the
    /// per-provider rationale). Equivalent to [`TrustedEmailProviders::default`].
    #[must_use]
    pub fn builtin_default() -> Self {
        Self::only(["google", "apple"])
    }

    /// Trust **no** provider. Every social identity is keyed on `(provider, provider_id)`
    /// and can never auto-merge on email — the one-call "trust no one" posture for
    /// high-assurance or regulated deployments.
    #[must_use]
    pub fn none() -> Self {
        Self {
            providers: HashSet::new(),
        }
    }

    /// Trust exactly the given providers, **replacing** the default set.
    #[must_use]
    pub fn only(providers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            providers: providers.into_iter().map(|p| normalize_provider(&p.into())).collect(),
        }
    }

    /// Add a provider to the trusted set (builder-style).
    #[must_use]
    pub fn trust(mut self, provider: impl Into<String>) -> Self {
        self.providers.insert(normalize_provider(&provider.into()));
        self
    }

    /// Remove a provider from the trusted set (builder-style) — e.g. to drop a default.
    #[must_use]
    pub fn distrust(mut self, provider: &str) -> Self {
        self.providers.remove(&normalize_provider(provider));
        self
    }

    /// Return `true` if `provider` is trusted to assert email verification for
    /// auto-linking. Matching is case- and surrounding-whitespace-insensitive.
    #[must_use]
    pub fn is_trusted(&self, provider: &str) -> bool {
        self.providers.contains(&normalize_provider(provider))
    }

    /// Return `true` if no provider is trusted (the "trust no one" posture).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl Default for TrustedEmailProviders {
    /// The built-in default trusted set ([`TrustedEmailProviders::builtin_default`]).
    fn default() -> Self {
        Self::builtin_default()
    }
}

/// Normalize a provider name for trust comparison (trim + lowercase), matching the way
/// providers register their names (e.g. `"google"`).
fn normalize_provider(provider: &str) -> String {
    provider.trim().to_lowercase()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests;
