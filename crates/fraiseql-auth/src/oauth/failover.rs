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
    unavailable: Arc<std::sync::Mutex<Vec<(String, DateTime<Utc>)>>>,
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
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the mutex is poisoned or no providers are available.
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
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the mutex is poisoned.
    pub fn mark_unavailable(
        &self,
        provider: String,
        duration_seconds: u64,
    ) -> std::result::Result<(), AuthError> {
        let mut unavailable = self.unavailable.lock().map_err(|_| AuthError::Internal {
            message: "failover manager mutex poisoned".to_string(),
        })?;
        unavailable
            .push((provider, Utc::now() + Duration::seconds(duration_seconds.cast_signed())));
        Ok(())
    }

    /// Mark provider as available
    ///
    /// # Errors
    ///
    /// Returns `AuthError::Internal` if the mutex is poisoned.
    pub fn mark_available(&self, provider: &str) -> std::result::Result<(), AuthError> {
        let mut unavailable = self.unavailable.lock().map_err(|_| AuthError::Internal {
            message: "failover manager mutex poisoned".to_string(),
        })?;
        unavailable.retain(|(name, _)| name != provider);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primary_available_by_default() {
        let mgr = ProviderFailoverManager::new("primary".to_string(), vec!["fallback".to_string()]);
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "primary");
    }

    #[test]
    fn test_fallback_used_when_primary_unavailable() {
        let mgr = ProviderFailoverManager::new("primary".to_string(), vec!["fallback".to_string()]);
        mgr.mark_unavailable("primary".to_string(), 300)
            .expect("mark_unavailable must succeed");
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "fallback");
    }

    #[test]
    fn test_all_unavailable_returns_error() {
        let mgr = ProviderFailoverManager::new("primary".to_string(), vec!["fallback".to_string()]);
        mgr.mark_unavailable("primary".to_string(), 300).expect("must succeed");
        mgr.mark_unavailable("fallback".to_string(), 300).expect("must succeed");
        let result = mgr.get_available_provider();
        assert!(result.is_err(), "must return error when no providers are available");
    }

    #[test]
    fn test_mark_available_restores_provider() {
        let mgr = ProviderFailoverManager::new("primary".to_string(), vec!["fallback".to_string()]);
        mgr.mark_unavailable("primary".to_string(), 300).expect("must succeed");
        mgr.mark_available("primary").expect("must succeed");
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "primary", "primary must be available after mark_available");
    }

    #[test]
    fn test_no_fallbacks_returns_primary() {
        let mgr = ProviderFailoverManager::new("only".to_string(), vec![]);
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "only");
    }

    #[test]
    fn test_no_fallbacks_primary_unavailable_returns_error() {
        let mgr = ProviderFailoverManager::new("only".to_string(), vec![]);
        mgr.mark_unavailable("only".to_string(), 300).expect("must succeed");
        let result = mgr.get_available_provider();
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_fallbacks_in_order() {
        let mgr = ProviderFailoverManager::new(
            "primary".to_string(),
            vec!["fb1".to_string(), "fb2".to_string()],
        );
        mgr.mark_unavailable("primary".to_string(), 300).expect("must succeed");
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "fb1", "first fallback must be selected");

        mgr.mark_unavailable("fb1".to_string(), 300).expect("must succeed");
        let available = mgr.get_available_provider().expect("must succeed");
        assert_eq!(available, "fb2", "second fallback must be selected when first is unavailable");
    }
}
