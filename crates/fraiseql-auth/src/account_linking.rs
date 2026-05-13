//! Account linking — merge provider identities sharing the same verified email.
//!
//! When a user authenticates with two different `OAuth` providers (e.g. GitHub then Google)
//! using the same email address, this module ensures they receive the **same `user_id`**
//! rather than two separate user records.
//!
//! # How it works
//!
//! 1. After a successful `OAuth` token exchange, call [`AccountStore::link_or_create_user`]
//!    with the verified email, provider name, and provider-specific user ID.
//! 2. The store checks whether an account with that email already exists.
//!    - **Existing account**: the new provider credential is linked to the existing
//!      account and the existing `user_id` is returned.
//!    - **New account**: a fresh `user_id` is generated, the account is stored, and
//!      the new `user_id` is returned.
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
    pub user_id:   String,
    /// Whether a new account was created (`true`) or an existing one was linked (`false`).
    pub is_new:    bool,
    /// Whether a new provider link was added to an existing account.
    pub linked:    bool,
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

        Ok(AccountLinkResult { user_id, is_new: true, linked: false })
    }

    async fn get_account(&self, user_id: &str) -> Result<AccountRecord> {
        self.by_user_id
            .get(user_id)
            .map(|r| r.clone())
            .ok_or(AuthError::TokenNotFound)
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
mod tests {
    use std::sync::Arc;

    use super::*;

    // ── Cycle 2 tests — account linking ───────────────────────────────────

    #[tokio::test]
    async fn test_first_sign_in_creates_new_account() {
        let store = InMemoryAccountStore::new();
        let result = store
            .link_or_create_user("alice@example.com", "github", "github_123")
            .await
            .unwrap();

        assert!(result.is_new, "first sign-in should create a new account");
        assert!(!result.linked, "no linking on brand-new account");
        assert!(result.user_id.starts_with("user_"), "user_id should have 'user_' prefix");
        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn test_github_then_google_same_email_returns_same_user_id() {
        // This is the primary Cycle 2 acceptance test.
        let store = InMemoryAccountStore::new();

        // Step 1: user signs in with GitHub
        let github_result = store
            .link_or_create_user("alice@example.com", "github", "github_123")
            .await
            .unwrap();
        assert!(github_result.is_new);
        let user_id = github_result.user_id.clone();

        // Step 2: same user signs in with Google (same email)
        let google_result = store
            .link_or_create_user("alice@example.com", "google", "google_456")
            .await
            .unwrap();
        assert!(!google_result.is_new, "second sign-in should not create a new account");
        assert!(google_result.linked, "Google should be linked to existing account");
        assert_eq!(
            google_result.user_id, user_id,
            "GitHub and Google sign-ins with same email must yield same user_id"
        );

        // Verify only one account record was created
        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn test_different_emails_create_different_accounts() {
        let store = InMemoryAccountStore::new();

        let alice = store
            .link_or_create_user("alice@example.com", "github", "github_alice")
            .await
            .unwrap();
        let bob = store
            .link_or_create_user("bob@example.com", "github", "github_bob")
            .await
            .unwrap();

        assert_ne!(alice.user_id, bob.user_id, "different emails must produce different user_ids");
        assert_eq!(store.len(), 2);
    }

    #[tokio::test]
    async fn test_same_provider_twice_does_not_duplicate_link() {
        let store = InMemoryAccountStore::new();

        // First sign-in
        store.link_or_create_user("alice@example.com", "github", "github_123").await.unwrap();

        // Same provider + same provider_id — should NOT add a duplicate link
        let second = store
            .link_or_create_user("alice@example.com", "github", "github_123")
            .await
            .unwrap();
        assert!(!second.is_new, "should not create a new account on second sign-in");
        assert!(!second.linked, "same provider/id should not count as newly linked");

        let record = store.get_account(&second.user_id).await.unwrap();
        assert_eq!(record.providers.len(), 1, "should still have only one provider link");
    }

    #[tokio::test]
    async fn test_multiple_providers_linked_to_single_account() {
        let store = InMemoryAccountStore::new();

        store
            .link_or_create_user("alice@example.com", "github", "github_123")
            .await
            .unwrap();
        store
            .link_or_create_user("alice@example.com", "google", "google_456")
            .await
            .unwrap();
        store
            .link_or_create_user("alice@example.com", "okta", "okta_789")
            .await
            .unwrap();

        let result = store
            .link_or_create_user("alice@example.com", "github", "github_123")
            .await
            .unwrap();
        let record = store.get_account(&result.user_id).await.unwrap();

        assert_eq!(record.providers.len(), 3, "all three providers should be linked");
        let providers: Vec<&str> = record.providers.iter().map(|p| p.provider.as_str()).collect();
        assert!(providers.contains(&"github"));
        assert!(providers.contains(&"google"));
        assert!(providers.contains(&"okta"));
    }

    #[tokio::test]
    async fn test_get_account_returns_correct_record() {
        let store = InMemoryAccountStore::new();
        let result = store
            .link_or_create_user("alice@example.com", "github", "github_123")
            .await
            .unwrap();

        let record = store.get_account(&result.user_id).await.unwrap();
        assert_eq!(record.email, "alice@example.com");
        assert_eq!(record.providers.len(), 1);
        assert_eq!(record.providers[0].provider, "github");
    }

    #[tokio::test]
    async fn test_get_account_unknown_user_id_returns_error() {
        let store = InMemoryAccountStore::new();
        let err = store.get_account("user_nonexistent").await.unwrap_err();
        assert!(
            matches!(err, AuthError::TokenNotFound),
            "unknown user_id should return TokenNotFound, got: {err:?}"
        );
    }

    #[test]
    fn test_normalize_email_lowercases() {
        assert_eq!(normalize_email("Alice@Example.COM"), "alice@example.com");
    }

    #[test]
    fn test_normalize_email_trims_whitespace() {
        assert_eq!(normalize_email("  alice@example.com  "), "alice@example.com");
    }

    #[test]
    fn test_normalize_email_idempotent() {
        let email = "alice@example.com";
        assert_eq!(normalize_email(email), normalize_email(&normalize_email(email)));
    }

    #[tokio::test]
    async fn test_account_store_as_trait_object() {
        let store: Arc<dyn AccountStore> = Arc::new(InMemoryAccountStore::new());
        let result = store
            .link_or_create_user("alice@example.com", "github", "github_123")
            .await
            .unwrap();
        assert!(result.is_new);
    }
}
