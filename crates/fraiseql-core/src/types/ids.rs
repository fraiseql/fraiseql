//! Strongly-typed wrappers for domain identifiers.
//!
//! Using newtypes for identifiers prevents accidental transposition with other string-typed IDs
//! at compile time. For example:
//!
//! ```ignore
//! // ❌ This compiles and is a silent bug:
//! fn create_session(user_id: String, tenant_id: String) -> Result<()>
//! create_session(tenant_id, user_id)  // Arguments transposed!
//!
//! // ✅ This is caught at compile time:
//! fn create_session(user_id: UserId, tenant_id: TenantId) -> Result<()>
//! create_session(tenant_id, user_id)  // compiler error!
//! ```

/// A strongly-typed wrapper for user identifiers.
///
/// Using a newtype prevents accidental transposition with other string-typed IDs
/// at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UserId(pub String);

impl UserId {
    /// Creates a new `UserId` from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the inner string value.
    #[must_use] 
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for UserId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for UserId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A strongly-typed wrapper for tenant identifiers.
///
/// Using a newtype prevents accidental transposition with other string-typed IDs
/// at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TenantId(pub String);

impl TenantId {
    /// Creates a new `TenantId` from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the inner string value.
    #[must_use] 
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for TenantId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TenantId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A strongly-typed wrapper for organization identifiers.
///
/// Using a newtype prevents accidental transposition with other string-typed IDs
/// at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct OrgId(pub String);

impl OrgId {
    /// Creates a new `OrgId` from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the inner string value.
    #[must_use] 
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for OrgId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for OrgId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for OrgId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A strongly-typed wrapper for subscription identifiers.
///
/// Using a newtype prevents accidental transposition with other string-typed IDs
/// at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SubscriptionId(pub String);

impl SubscriptionId {
    /// Creates a new `SubscriptionId` from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the inner string value.
    #[must_use] 
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for SubscriptionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SubscriptionId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// A strongly-typed wrapper for connection identifiers.
///
/// Using a newtype prevents accidental transposition with other string-typed IDs
/// at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ConnectionId(pub String);

impl ConnectionId {
    /// Creates a new `ConnectionId` from any string-like value.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the inner string value.
    #[must_use] 
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for ConnectionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ConnectionId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}
