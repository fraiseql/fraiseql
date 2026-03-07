//! Multi-provider failover management.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use super::super::error::AuthError;

/// Multi-provider failover manager
#[derive(Debug, Clone)]
pub struct ProviderFailoverManager {
    /// Primary provider name
    primary_provider:   String,
    /// Fallback providers in priority order
    fallback_providers: Vec<String>,
    /// Providers currently unavailable
    // std::sync::Mutex is intentional: this lock is never held across .await.
    // Switch to tokio::sync::Mutex if that constraint ever changes.
    unavailable:        Arc<std::sync::Mutex<Vec<(String, DateTime<Utc>)>>>,
}

impl ProviderFailoverManager {
    /// Create new failover manager
    pub fn new(primary: String, fallbacks: Vec<String>) -> Self {
        Self {
            primary_provider:   primary,
            fallback_providers: fallbacks,
            unavailable:        Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Get next available provider
    pub fn get_available_provider(&self) -> std::result::Result<String, AuthError> {
        let unavailable = self.unavailable.lock().map_err(|_| AuthError::Internal {
            message: "failover manager mutex poisoned".to_string(),
        })?;
        let now = Utc::now();

        // Check if primary is available
        if !unavailable
            .iter()
            .any(|(name, exp)| name == &self.primary_provider && *exp > now)
        {
            return Ok(self.primary_provider.clone());
        }

        // Find first available fallback
        for fallback in &self.fallback_providers {
            if !unavailable.iter().any(|(name, exp)| name == fallback && *exp > now) {
                return Ok(fallback.clone());
            }
        }

        Err(AuthError::Internal {
            message: "no OAuth providers available".to_string(),
        })
    }

    /// Mark provider as unavailable
    pub fn mark_unavailable(
        &self,
        provider: String,
        duration_seconds: u64,
    ) -> std::result::Result<(), AuthError> {
        let mut unavailable = self.unavailable.lock().map_err(|_| AuthError::Internal {
            message: "failover manager mutex poisoned".to_string(),
        })?;
        unavailable.push((provider, Utc::now() + Duration::seconds(duration_seconds as i64)));
        Ok(())
    }

    /// Mark provider as available
    pub fn mark_available(&self, provider: &str) -> std::result::Result<(), AuthError> {
        let mut unavailable = self.unavailable.lock().map_err(|_| AuthError::Internal {
            message: "failover manager mutex poisoned".to_string(),
        })?;
        unavailable.retain(|(name, _)| name != provider);
        Ok(())
    }
}
