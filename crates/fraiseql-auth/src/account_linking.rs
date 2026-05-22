//! Account linking — merge provider identities sharing the same verified email.
//!
//! When a user authenticates with two different `OAuth` providers (e.g. GitHub then Google)
//! using the same email address, this module ensures they receive the **same `user_id`**
//! rather than two separate user records.
//!
//! # How it works
//!
//! 1. After a successful `OAuth` token exchange, call [`AccountStore::link_or_create_user`] with
//!    the verified email, provider name, and provider-specific user ID.
//! 2. The store checks whether an account with that email already exists.
//!    - **Existing account**: the new provider credential is linked to the existing account and the
//!      existing `user_id` is returned.
//!    - **New account**: a fresh `user_id` is generated, the account is stored, and the new
//!      `user_id` is returned.
//! 3. The caller creates or refreshes a session keyed by the returned `user_id`.

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    audit::logger::{AuditEventType, SecretType, get_audit_logger},
    error::{AuthError, Result},
};

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
    /// Verified email address shared across all linked providers.
    pub email:     String,
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
    /// Return the `user_id` for the given email+provider pair, creating or linking as needed.
    ///
    /// # Semantics
    ///
    /// - If no account exists for `email`: creates a new account, stores the `provider` /
    ///   `provider_id` link, and returns the new `user_id`.
    /// - If an account already exists for `email`:
    ///   - If the `provider` / `provider_id` pair is new, adds it as a linked credential.
    ///   - Returns the **existing** `user_id` (same as on first sign-in).
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::DatabaseError`] if the backing store fails.
    async fn link_or_create_user(
        &self,
        email: &str,
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
    /// email → user_id (fast lookup by email)
    by_email:   DashMap<String, String>,
    /// user_id → AccountRecord
    by_user_id: DashMap<String, AccountRecord>,
}

impl InMemoryAccountStore {
    /// Create a new empty account store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_email:   DashMap::new(),
            by_user_id: DashMap::new(),
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
        email: &str,
        provider: &str,
        provider_id: &str,
    ) -> Result<AccountLinkResult> {
        let logger = get_audit_logger();
        let email_normalized = normalize_email(email);
        let new_link = ProviderLink {
            provider:    provider.to_string(),
            provider_id: provider_id.to_string(),
        };

        // Check whether an account already exists for this email.
        if let Some(existing_user_id) = self.by_email.get(&email_normalized).map(|r| r.clone()) {
            let mut record = self.by_user_id.get_mut(&existing_user_id).ok_or_else(|| {
                AuthError::DatabaseError {
                    message: format!(
                        "account store inconsistency: email '{}' maps to missing user_id '{}'",
                        email, existing_user_id
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
            email:     email_normalized.clone(),
            providers: vec![new_link],
        };
        self.by_email.insert(email_normalized, user_id.clone());
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests;
