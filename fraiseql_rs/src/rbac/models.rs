//! RBAC data models matching PostgreSQL schema.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role entity with hierarchical support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub parent_role_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub is_system: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Role {
    /// Create Role from tokio_postgres Row
    pub fn from_row(row: tokio_postgres::Row) -> Self {
        Self {
            id: Uuid::parse_str(&row.get::<_, String>(0)).unwrap_or_default(),
            name: row.get(1),
            description: row.get(2),
            parent_role_id: row.get::<_, Option<String>>(3).and_then(|s| Uuid::parse_str(&s).ok()),
            tenant_id: row.get::<_, Option<String>>(4).and_then(|s| Uuid::parse_str(&s).ok()),
            is_system: row.get(5),
            created_at: row.get::<_, chrono::NaiveDateTime>(6).and_utc(),
            updated_at: row.get::<_, chrono::NaiveDateTime>(7).and_utc(),
        }
    }
}

/// Permission entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: Uuid,
    pub resource: String,
    pub action: String,
    pub description: Option<String>,
    pub constraints: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl Permission {
    /// Check if permission matches resource:action pattern
    pub fn matches(&self, resource: &str, action: &str) -> bool {
        // Exact match
        if self.resource == resource && self.action == action {
            return true;
        }

        // Wildcard matching: resource:* or *:action
        if self.action == "*" && self.resource == resource {
            return true;
        }
        if self.resource == "*" && self.action == action {
            return true;
        }
        if self.resource == "*" && self.action == "*" {
            return true;
        }

        false
    }

    /// Create Permission from tokio_postgres Row
    pub fn from_row(row: tokio_postgres::Row) -> Self {
        Self {
            id: Uuid::parse_str(&row.get::<_, String>(0)).unwrap_or_default(),
            resource: row.get(1),
            action: row.get(2),
            description: row.get(3),
            constraints: row.get::<_, Option<String>>(4).and_then(|s| serde_json::from_str(&s).ok()),
            created_at: row.get::<_, chrono::NaiveDateTime>(5).and_utc(),
        }
    }
}

/// User-Role assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRole {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub granted_by: Option<Uuid>,
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl UserRole {
    /// Check if role assignment is still valid
    pub fn is_valid(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() < expires_at
        } else {
            true
        }
    }

    /// Create UserRole from tokio_postgres Row
    pub fn from_row(row: tokio_postgres::Row) -> Self {
        Self {
            id: Uuid::parse_str(&row.get::<_, String>(0)).unwrap_or_default(),
            user_id: Uuid::parse_str(&row.get::<_, String>(1)).unwrap_or_default(),
            role_id: Uuid::parse_str(&row.get::<_, String>(2)).unwrap_or_default(),
            tenant_id: row.get::<_, Option<String>>(3).and_then(|s| Uuid::parse_str(&s).ok()),
            granted_by: row.get::<_, Option<String>>(4).and_then(|s| Uuid::parse_str(&s).ok()),
            granted_at: row.get::<_, chrono::NaiveDateTime>(5).and_utc(),
            expires_at: row.get::<_, Option<chrono::NaiveDateTime>>(6).map(|dt| dt.and_utc()),
        }
    }
}

/// Role-Permission mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePermission {
    pub id: Uuid,
    pub role_id: Uuid,
    pub permission_id: Uuid,
    pub granted_at: DateTime<Utc>,
}
