//! Thread-safe registry for custom scalar implementations.
//!
//! This module provides a global registry for managing custom scalar implementations
//! at runtime, allowing applications to register their own scalar types.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use super::custom_scalar::CustomScalar;

/// Thread-safe registry for custom scalar implementations.
///
/// Uses `Arc<RwLock<HashMap>>` for concurrent read access with exclusive write access.
pub struct CustomScalarRegistry {
    scalars: Arc<RwLock<HashMap<String, Arc<dyn CustomScalar>>>>,
}

impl CustomScalarRegistry {
    /// Create a new custom scalar registry.
    pub fn new() -> Self {
        Self {
            scalars: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a custom scalar implementation.
    ///
    /// # Arguments
    ///
    /// * `scalar` - The scalar implementation to register
    ///
    /// # Errors
    ///
    /// Returns an error if a scalar with the same name is already registered.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::validation::{CustomScalarRegistry, CustomScalar};
    /// use fraiseql_core::error::Result;
    /// use serde_json::Value;
    /// use std::sync::Arc;
    ///
    /// #[derive(Debug)]
    /// struct Email;
    /// impl CustomScalar for Email {
    ///     fn name(&self) -> &str { "Email" }
    ///     fn serialize(&self, v: &Value) -> Result<Value> { Ok(v.clone()) }
    ///     fn parse_value(&self, v: &Value) -> Result<Value> { Ok(v.clone()) }
    ///     fn parse_literal(&self, v: &Value) -> Result<Value> { Ok(v.clone()) }
    /// }
    ///
    /// let registry = CustomScalarRegistry::new();
    /// registry.register(Arc::new(Email)).unwrap();
    /// assert!(registry.has_scalar("Email").unwrap());
    /// ```
    pub fn register(&self, scalar: Arc<dyn CustomScalar>) -> crate::error::Result<()> {
        let name = scalar.name().to_string();

        let mut scalars = self.scalars.write().map_err(|_| {
            crate::error::FraiseQLError::internal("Failed to acquire write lock on scalar registry")
        })?;

        if scalars.contains_key(&name) {
            return Err(crate::error::FraiseQLError::validation(format!(
                "Scalar \"{}\" is already registered",
                name
            )));
        }

        scalars.insert(name, scalar);
        Ok(())
    }

    /// Get a registered scalar by name.
    ///
    /// Returns `None` if the scalar is not registered.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the read lock cannot be acquired.
    pub fn get_scalar(&self, name: &str) -> crate::error::Result<Option<Arc<dyn CustomScalar>>> {
        let scalars = self.scalars.read().map_err(|_| {
            crate::error::FraiseQLError::internal("Failed to acquire read lock on scalar registry")
        })?;

        Ok(scalars.get(name).cloned())
    }

    /// Check if a scalar is registered.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the read lock cannot be acquired.
    pub fn has_scalar(&self, name: &str) -> crate::error::Result<bool> {
        let scalars = self.scalars.read().map_err(|_| {
            crate::error::FraiseQLError::internal("Failed to acquire read lock on scalar registry")
        })?;

        Ok(scalars.contains_key(name))
    }

    /// Get all registered scalar names.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the read lock cannot be acquired.
    pub fn get_scalar_names(&self) -> crate::error::Result<Vec<String>> {
        let scalars = self.scalars.read().map_err(|_| {
            crate::error::FraiseQLError::internal("Failed to acquire read lock on scalar registry")
        })?;

        Ok(scalars.keys().cloned().collect())
    }

    /// Unregister a scalar by name (useful for testing).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the write lock cannot be acquired.
    pub fn unregister(&self, name: &str) -> crate::error::Result<()> {
        let mut scalars = self.scalars.write().map_err(|_| {
            crate::error::FraiseQLError::internal("Failed to acquire write lock on scalar registry")
        })?;

        scalars.remove(name);
        Ok(())
    }

    /// Clear all registered scalars (useful for testing).
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the write lock cannot be acquired.
    pub fn clear(&self) -> crate::error::Result<()> {
        let mut scalars = self.scalars.write().map_err(|_| {
            crate::error::FraiseQLError::internal("Failed to acquire write lock on scalar registry")
        })?;

        scalars.clear();
        Ok(())
    }
}

impl Default for CustomScalarRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CustomScalarRegistry {
    fn clone(&self) -> Self {
        Self {
            scalars: Arc::clone(&self.scalars),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[derive(Debug)]
    struct TestScalar;

    impl CustomScalar for TestScalar {
        #[allow(clippy::unnecessary_literal_bound)] // Reason: trait requires &str return type
        fn name(&self) -> &str {
            "Test"
        }

        fn serialize(&self, value: &serde_json::Value) -> crate::error::Result<serde_json::Value> {
            Ok(value.clone())
        }

        fn parse_value(
            &self,
            value: &serde_json::Value,
        ) -> crate::error::Result<serde_json::Value> {
            Ok(value.clone())
        }

        fn parse_literal(
            &self,
            ast: &serde_json::Value,
        ) -> crate::error::Result<serde_json::Value> {
            Ok(ast.clone())
        }
    }

    #[test]
    fn test_register_scalar() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry.register(scalar).unwrap_or_else(|e| panic!("first registration should succeed: {e}"));
        assert!(registry.has_scalar("Test").unwrap());
    }

    #[test]
    fn test_prevent_duplicate_registration() {
        let registry = CustomScalarRegistry::new();
        let scalar1: Arc<dyn CustomScalar> = Arc::new(TestScalar);
        let scalar2: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry.register(scalar1).unwrap_or_else(|e| panic!("first registration should succeed: {e}"));
        assert!(
            matches!(registry.register(scalar2), Err(crate::error::FraiseQLError::Validation { .. })),
            "duplicate registration should return Validation error"
        );
    }

    #[test]
    fn test_get_scalar() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry.register(scalar.clone()).unwrap();
        assert!(registry.get_scalar("Test").unwrap().is_some());
        assert!(registry.get_scalar("NotFound").unwrap().is_none());
    }

    #[test]
    fn test_unregister_scalar() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry.register(scalar).unwrap();
        assert!(registry.has_scalar("Test").unwrap());

        registry.unregister("Test").unwrap();
        assert!(!registry.has_scalar("Test").unwrap());
    }

    #[test]
    fn test_clear_scalars() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry.register(scalar).unwrap();
        registry.clear().unwrap();

        assert!(registry.get_scalar_names().unwrap().is_empty());
    }
}
