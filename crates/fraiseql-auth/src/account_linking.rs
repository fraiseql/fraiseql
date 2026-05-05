//! Account linking — same email across providers maps to the same local user.
//!
//! When a user signs in with GitHub (email: `alice@example.com`) then later
//! signs in with Google (same email), both provider identities are linked to
//! the same local `user_id`. This module provides the [`UserStore`] trait and
//! an in-memory implementation for testing.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{error::Result, provider::UserInfo};

/// A linked provider identity for a local user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkedIdentity {
    /// OAuth provider name (e.g., "github", "google").
    pub provider:         String,
    /// The user's unique ID within the provider (e.g., GitHub user ID).
    pub provider_user_id: String,
    /// Email associated with this identity at link time.
    pub email:            String,
}

/// A local user record with one or more linked provider identities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalUser {
    /// Unique local user ID (UUID).
    pub id:         String,
    /// Primary email address.
    pub email:      String,
    /// Display name (from the first provider that supplied one).
    pub name:       Option<String>,
    /// Profile picture URL.
    pub picture:    Option<String>,
    /// Linked provider identities.
    pub identities: Vec<LinkedIdentity>,
    /// Unix timestamp when the user was created.
    pub created_at: u64,
}

/// User store trait — resolves provider identities to local users.
///
/// Implementations handle the core account-linking logic:
/// 1. If the provider+provider_user_id is already linked → return existing user.
/// 2. If the email matches an existing user → link this identity and return user.
/// 3. Otherwise → create a new local user with this identity.
// Reason: used as dyn Trait (Arc<dyn UserStore>); async_trait ensures Send bounds and
// dyn-compatibility async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait UserStore: Send + Sync {
    /// Resolve a provider identity to a local user.
    ///
    /// This is the main entry point for the callback flow. It handles:
    /// - Existing identity lookup (provider + provider_user_id)
    /// - Email-based account linking (same email → same user)
    /// - New user creation
    ///
    /// Returns the local user ID.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the store encounters an internal error.
    async fn find_or_create_user(
        &self,
        provider: &str,
        user_info: &UserInfo,
    ) -> Result<LocalUser>;

    /// Get a user by their local ID.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the store encounters an internal error.
    async fn get_user(&self, user_id: &str) -> Result<Option<LocalUser>>;

    /// List all identities linked to a local user.
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the store encounters an internal error.
    async fn list_identities(&self, user_id: &str) -> Result<Vec<LinkedIdentity>>;
}

/// In-memory user store for testing.
///
/// Thread-safe via `tokio::sync::RwLock`. Not suitable for production.
#[derive(Debug)]
pub struct InMemoryUserStore {
    /// Users keyed by local user ID.
    users: Arc<RwLock<HashMap<String, LocalUser>>>,
    /// Index: (provider, provider_user_id) → local user ID.
    identity_index: Arc<RwLock<HashMap<(String, String), String>>>,
    /// Index: email → local user ID (for account linking).
    email_index: Arc<RwLock<HashMap<String, String>>>,
}

impl InMemoryUserStore {
    /// Create a new empty in-memory user store.
    pub fn new() -> Self {
        Self {
            users:          Arc::new(RwLock::new(HashMap::new())),
            identity_index: Arc::new(RwLock::new(HashMap::new())),
            email_index:    Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Return the number of local users.
    pub async fn user_count(&self) -> usize {
        self.users.read().await.len()
    }
}

impl Default for InMemoryUserStore {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// Reason: UserStore is defined with #[async_trait]; all implementations must match
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl UserStore for InMemoryUserStore {
    async fn find_or_create_user(
        &self,
        provider: &str,
        user_info: &UserInfo,
    ) -> Result<LocalUser> {
        let identity_key = (provider.to_string(), user_info.id.clone());

        // 1. Check if this exact provider identity is already linked
        {
            let identity_index = self.identity_index.read().await;
            if let Some(user_id) = identity_index.get(&identity_key) {
                let users = self.users.read().await;
                if let Some(user) = users.get(user_id) {
                    return Ok(user.clone());
                }
            }
        }

        // 2. Check if the email matches an existing user (account linking)
        let email_lower = user_info.email.to_lowercase();
        {
            let email_index = self.email_index.read().await;
            if let Some(user_id) = email_index.get(&email_lower) {
                // Link this new identity to the existing user
                let mut users = self.users.write().await;
                if let Some(user) = users.get_mut(user_id) {
                    let new_identity = LinkedIdentity {
                        provider:         provider.to_string(),
                        provider_user_id: user_info.id.clone(),
                        email:            user_info.email.clone(),
                    };

                    // Avoid duplicate identities
                    if !user.identities.iter().any(|i| {
                        i.provider == provider && i.provider_user_id == user_info.id
                    }) {
                        user.identities.push(new_identity);
                    }

                    let linked_user = user.clone();

                    // Update identity index
                    drop(users);
                    let mut identity_index = self.identity_index.write().await;
                    identity_index.insert(identity_key, linked_user.id.clone());

                    return Ok(linked_user);
                }
            }
        }

        // 3. Create a new local user
        let user_id = uuid::Uuid::new_v4().to_string();
        let identity = LinkedIdentity {
            provider:         provider.to_string(),
            provider_user_id: user_info.id.clone(),
            email:            user_info.email.clone(),
        };

        let user = LocalUser {
            id:         user_id.clone(),
            email:      user_info.email.clone(),
            name:       user_info.name.clone(),
            picture:    user_info.picture.clone(),
            identities: vec![identity],
            created_at: unix_now(),
        };

        // Insert into all indices
        {
            let mut users = self.users.write().await;
            users.insert(user_id.clone(), user.clone());
        }
        {
            let mut identity_index = self.identity_index.write().await;
            identity_index.insert(identity_key, user_id.clone());
        }
        {
            let mut email_index = self.email_index.write().await;
            email_index.insert(email_lower, user_id);
        }

        Ok(user)
    }

    async fn get_user(&self, user_id: &str) -> Result<Option<LocalUser>> {
        let users = self.users.read().await;
        Ok(users.get(user_id).cloned())
    }

    async fn list_identities(&self, user_id: &str) -> Result<Vec<LinkedIdentity>> {
        let users = self.users.read().await;
        Ok(users
            .get(user_id)
            .map(|u| u.identities.clone())
            .unwrap_or_default())
    }
}
