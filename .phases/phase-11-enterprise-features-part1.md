# Phase 11: Enterprise Features - Part 1 (RBAC, Audit, Multi-Tenancy)

**Objective**: Implement enterprise-grade access control, audit logging, and multi-tenant isolation

**Duration**: 2-3 weeks

**Estimated LOC**: 2000-2500 (traits, implementations, tests)

**Dependencies**: Phase 10 complete

---

## Success Criteria

- [ ] RBAC with role hierarchy (admin, user, guest)
- [ ] Field-level authorization with GraphQL directives (@require_permission)
- [ ] Audit logging with multiple backends (file, PostgreSQL, syslog)
- [ ] Multi-tenancy with tenant isolation
- [ ] Tenant-aware query execution
- [ ] Compliance audit trail queries
- [ ] Role/permission management API endpoints
- [ ] All tests passing
- [ ] Zero clippy warnings

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Request Layer                            │
│  Incoming JWT → Extract user context → Attach tenant_id    │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                 Execution Layer                             │
│  • Query planning (with tenant filter)                      │
│  • Permission checking (@require_permission)                │
│  • Field masking (sensitive fields)                          │
│  • Audit logging (before/after state)                        │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│                 Storage Layer                               │
│  PostgreSQL with tenant column + row-level security         │
└─────────────────────────────────────────────────────────────┘
```

---

## TDD Cycles

### Cycle 11.1: RBAC Data Model

**Objective**: Define roles, permissions, and user-role associations

#### Files
- `crates/fraiseql-core/src/rbac/mod.rs` (main module)
- `crates/fraiseql-core/src/rbac/model.rs` (data structures)
- `crates/fraiseql-core/src/rbac/permission.rs` (permission logic)

#### RED: Write tests
```rust
// crates/fraiseql-core/src/rbac/tests.rs

#[test]
fn test_role_hierarchy() {
    let admin = Role::new("admin", 0);
    let user = Role::new("user", 100);
    let guest = Role::new("guest", 200);

    assert!(admin.level() < user.level(), "admin should outrank user");
    assert!(user.level() < guest.level(), "user should outrank guest");
}

#[test]
fn test_permission_check() {
    let user = User::new("u1", "alice", Role::User);
    let perm = Permission::new("query:users:read");

    assert!(user.has_permission(&perm), "user should have query permission");
}

#[test]
fn test_field_visibility() {
    let guest = User::new("u1", "bob", Role::Guest);
    let sensitive_fields = vec!["email", "phone", "ssn"];

    for field in &sensitive_fields {
        assert!(!guest.can_see_field(field), "guest cannot see sensitive fields");
    }

    let admin = User::new("u2", "admin", Role::Admin);
    for field in &sensitive_fields {
        assert!(admin.can_see_field(field), "admin can see all fields");
    }
}
```

#### GREEN: Implement data model with policy-driven field masking

```rust
// crates/fraiseql-core/src/rbac/model.rs

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Admin,
    User,
    Guest,
}

impl Role {
    pub fn level(&self) -> i32 {
        match self {
            Role::Admin => 0,
            Role::User => 100,
            Role::Guest => 200,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Permission {
    resource: String,
    action: String,
}

impl Permission {
    pub fn new(resource_action: &str) -> Self {
        let parts: Vec<&str> = resource_action.split(':').collect();
        Permission {
            resource: parts.get(0).unwrap_or(&"*").to_string(),
            action: parts.get(1).unwrap_or(&"*").to_string(),
        }
    }

    pub fn matches(&self, other: &Permission) -> bool {
        (self.resource == "*" || self.resource == other.resource)
            && (self.action == "*" || self.action == other.action)
    }
}

/// Field-level access policy (policy-driven, not hardcoded)
#[derive(Clone, Debug)]
pub struct FieldAccessPolicy {
    field_name: String,
    min_role_level: i32,  // 0=admin, 100=user, 200=guest, 999=nobody
    allowed_roles: Vec<Role>,
}

impl FieldAccessPolicy {
    pub fn new(field: &str, min_level: i32) -> Self {
        FieldAccessPolicy {
            field_name: field.to_string(),
            min_role_level: min_level,
            allowed_roles: vec![],
        }
    }

    pub fn with_roles(mut self, roles: Vec<Role>) -> Self {
        self.allowed_roles = roles;
        self
    }

    pub fn can_access(&self, role: &Role) -> bool {
        // Check both role level and explicit whitelist
        if !self.allowed_roles.is_empty() {
            return self.allowed_roles.contains(role);
        }
        role.level() <= self.min_role_level
    }
}

#[derive(Clone, Debug)]
pub struct User {
    id: String,
    name: String,
    role: Role,
    tenant_id: String,
}

impl User {
    pub fn has_permission(&self, permission: &Permission) -> bool {
        let role_perms = self.role.permissions();
        role_perms.iter().any(|p| p.matches(permission))
    }

    /// Query field access using policy-driven rules (NOT hardcoded)
    pub fn can_see_field(&self, policy: &FieldAccessPolicy) -> bool {
        policy.can_access(&self.role)
    }

    /// Get visible fields based on policies
    pub fn visible_fields(&self, policies: &[FieldAccessPolicy]) -> Vec<String> {
        policies
            .iter()
            .filter(|p| self.can_see_field(p))
            .map(|p| p.field_name.clone())
            .collect()
    }
}

impl Role {
    pub fn permissions(&self) -> Vec<Permission> {
        match self {
            Role::Admin => vec![Permission::new("*:*")],
            Role::User => vec![
                Permission::new("query:*"),
                Permission::new("mutation:limited"),
            ],
            Role::Guest => vec![
                Permission::new("query:public"),
            ],
        }
    }

    pub fn level(&self) -> i32 {
        match self {
            Role::Admin => 0,
            Role::User => 100,
            Role::Guest => 200,
        }
    }
}

/// Policy registry - define all field access policies in one place
pub struct PolicyRegistry {
    policies: std::collections::HashMap<String, FieldAccessPolicy>,
}

impl PolicyRegistry {
    pub fn new() -> Self {
        let mut registry = PolicyRegistry {
            policies: std::collections::HashMap::new(),
        };

        // Define policies declaratively (easy to update, audit, version-control)
        registry.register("email", FieldAccessPolicy::new("email", 100));  // User+
        registry.register("phone", FieldAccessPolicy::new("phone", 100));  // User+
        registry.register("ssn", FieldAccessPolicy::new("ssn", 0));        // Admin only
        registry.register("credit_card", FieldAccessPolicy::new("credit_card", 0)); // Admin only
        registry.register("api_key", FieldAccessPolicy::new("api_key", 0)); // Admin only
        registry.register("salary", FieldAccessPolicy::new("salary", 0));  // Admin only

        registry
    }

    pub fn register(&mut self, field: &str, policy: FieldAccessPolicy) {
        self.policies.insert(field.to_string(), policy);
    }

    pub fn get(&self, field: &str) -> Option<&FieldAccessPolicy> {
        self.policies.get(field)
    }

    pub fn all_policies(&self) -> impl Iterator<Item = &FieldAccessPolicy> {
        self.policies.values()
    }
}
```

#### REFACTOR
- Extract common permission patterns
- Use consistent naming

#### CLEANUP
- Remove debug prints
- Ensure comprehensive tests

---

### Cycle 11.2: GraphQL RBAC Directives

**Objective**: Add @require_permission directive to GraphQL schema

#### Files
- `crates/fraiseql-core/src/rbac/directive.rs` (directive parsing)
- `crates/fraiseql-server/src/routes/graphql.rs` (directive integration)

#### RED: Tests
```rust
#[test]
fn test_require_permission_directive() {
    let schema = r#"
    directive @require_permission(permission: String!) on FIELD_DEFINITION

    type Query {
        users: [User!]! @require_permission(permission: "query:users:read")
        adminPanel: String! @require_permission(permission: "admin:*")
    }
    "#;

    let parsed = parse_schema(schema).unwrap();
    let query = parsed.query_type();

    assert!(query.field("users").has_directive("require_permission"));
    assert!(query.field("adminPanel").has_directive("require_permission"));
}

#[tokio::test]
async fn test_unauthorized_field_access_denied() {
    let guest = User::new("u1", "guest", Role::Guest);
    let query = "query { adminPanel }";

    let result = executor.execute(query, guest).await;
    assert!(result.errors.len() > 0);
    assert!(result.errors[0].message.contains("permission"));
}
```

#### GREEN: Implement directive
```rust
// crates/fraiseql-core/src/rbac/directive.rs

pub struct RequirePermissionDirective {
    permission: String,
}

impl RequirePermissionDirective {
    pub fn validate(&self, user: &User) -> Result<(), String> {
        let perm = Permission::new(&self.permission);
        if user.has_permission(&perm) {
            Ok(())
        } else {
            Err(format!("User lacks permission: {}", self.permission))
        }
    }
}

// Execution middleware
impl ExecutionMiddleware for RBACMiddleware {
    async fn on_field_execution(
        &self,
        field: &Field,
        user: &User,
    ) -> Result<(), ExecutionError> {
        if let Some(directive) = field.get_directive("require_permission") {
            let perm_str = directive.arg("permission")?;
            let perm = Permission::new(&perm_str);

            if !user.has_permission(&perm) {
                return Err(ExecutionError::Unauthorized(format!(
                    "Missing permission: {}",
                    perm_str
                )));
            }
        }
        Ok(())
    }
}
```

#### CLEANUP
- Verify all directives recognized
- Check directive validation

---

### Cycle 11.3: Audit Logging Infrastructure

**Objective**: Create audit logging with multiple backends

#### Files
- `crates/fraiseql-server/src/audit/mod.rs`
- `crates/fraiseql-server/src/audit/logger.rs`
- `crates/fraiseql-server/src/audit/backends/file.rs`
- `crates/fraiseql-server/src/audit/backends/postgres.rs`
- `crates/fraiseql-server/src/audit/backends/syslog.rs`

#### RED: Tests
```rust
#[test]
fn test_audit_event_structure() {
    let event = AuditEvent::new(
        "user_login",
        "u1",
        "alice",
        "192.168.1.1",
        serde_json::json!({ "method": "password" }),
    );

    assert_eq!(event.event_type, "user_login");
    assert_eq!(event.user_id, "u1");
    assert!(!event.timestamp.is_empty());
}

#[tokio::test]
async fn test_audit_file_backend() {
    let backend = FileAuditBackend::new("/tmp/audit.log").unwrap();
    let event = AuditEvent::new("test_event", "u1", "alice", "127.0.0.1", json!({}));

    backend.log(event).await.unwrap();

    let content = fs::read_to_string("/tmp/audit.log").unwrap();
    assert!(content.contains("test_event"));
}

#[tokio::test]
async fn test_audit_postgres_backend() {
    let pool = setup_test_db().await;
    let backend = PostgresAuditBackend::new(pool);
    let event = AuditEvent::new("test_event", "u1", "alice", "127.0.0.1", json!({}));

    backend.log(event).await.unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE event_type = $1")
        .bind("test_event")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count.0, 1);
}
```

#### GREEN: Implement audit backends

**Audit event structure**:
```rust
pub struct AuditEvent {
    pub id: String,
    pub timestamp: String,
    pub event_type: String,
    pub user_id: String,
    pub username: String,
    pub ip_address: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub action: String,
    pub before_state: Option<Value>,
    pub after_state: Option<Value>,
    pub status: String,  // success, failure, denied
    pub error_message: Option<String>,
    pub metadata: Value,
}
```

**File backend**: Write JSON lines to file
**PostgreSQL backend**: Insert into audit_log table
**Syslog backend**: Send to syslog server

#### CLEANUP
- Verify all backends tested
- Check file permissions

---

### Cycle 11.4: Multi-Tenancy Data Model

**Objective**: Add tenant isolation to database and execution

#### Files
- `crates/fraiseql-core/src/tenancy/mod.rs`
- `crates/fraiseql-core/src/tenancy/model.rs`
- `crates/fraiseql-server/src/routes/graphql.rs` (tenant extraction)

#### RED: Tests
```rust
#[test]
fn test_tenant_isolation() {
    let tenant_a = TenantContext::new("tenant_a");
    let tenant_b = TenantContext::new("tenant_b");

    assert_ne!(tenant_a.id(), tenant_b.id());
}

#[tokio::test]
async fn test_query_includes_tenant_filter() {
    let tenant = TenantContext::new("tenant_a");
    let executor = setup_executor(&tenant);

    let query = "query { users { id name } }";
    let result = executor.execute(query).await.unwrap();

    // Verify generated SQL includes tenant_id filter
    // SELECT * FROM users WHERE tenant_id = $1
    assert!(result.sql.contains("tenant_id"));
}

#[tokio::test]
async fn test_cross_tenant_access_denied() {
    let tenant_a = TenantContext::new("tenant_a");
    let user_in_b = User::new("u1", "alice", Role::User, "tenant_b");

    let executor = setup_executor(&tenant_a);

    // User from tenant_b should not see data from tenant_a
    let result = executor.execute_as_user("query { users { id } }", &user_in_b).await;

    assert!(result.is_err());
}
```

#### GREEN: Implement multi-tenancy
```rust
pub struct TenantContext {
    id: String,
    created_at: DateTime<Utc>,
}

impl TenantContext {
    pub fn from_jwt(token: &str) -> Result<Self> {
        let claims = jwt::verify(token)?;
        Ok(TenantContext {
            id: claims.tenant_id.clone(),
            created_at: Utc::now(),
        })
    }
}

// Database schema
// ALTER TABLE users ADD COLUMN tenant_id UUID NOT NULL;
// ALTER TABLE users ADD CONSTRAINT users_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id);
// CREATE INDEX idx_users_tenant_id ON users(tenant_id);

// Query modifier
impl QueryModifier {
    pub fn add_tenant_filter(&self, query: &mut QueryBuilder, tenant_id: &str) {
        query.add_where_clause(&format!("tenant_id = '{}'", tenant_id));
    }
}
```

#### CLEANUP
- Verify tenant context flows through all queries
- Check schema migrations included

---

### Cycle 11.5: Role/Permission Management API

**Objective**: Create REST API endpoints for role/permission management

#### Files
- `crates/fraiseql-server/src/routes/api/rbac.rs`
- `crates/fraiseql-server/src/server.rs` (route registration)

#### Endpoints
```
POST   /api/v1/rbac/roles                  # Create role
GET    /api/v1/rbac/roles                  # List roles
GET    /api/v1/rbac/roles/{role_id}        # Get role
DELETE /api/v1/rbac/roles/{role_id}        # Delete role

POST   /api/v1/rbac/roles/{role_id}/permissions
GET    /api/v1/rbac/roles/{role_id}/permissions
DELETE /api/v1/rbac/roles/{role_id}/permissions/{perm_id}

POST   /api/v1/rbac/users/{user_id}/roles
GET    /api/v1/rbac/users/{user_id}/roles
DELETE /api/v1/rbac/users/{user_id}/roles/{role_id}

GET    /api/v1/audit/events               # Query audit log
GET    /api/v1/audit/events/{event_id}    # Get event details
```

#### RED: Tests
```rust
#[tokio::test]
async fn test_create_role_endpoint() {
    let client = setup_test_client().await;
    let admin_token = get_admin_token();

    let response = client
        .post("/api/v1/rbac/roles")
        .header("Authorization", &admin_token)
        .json(&json!({
            "name": "moderator",
            "level": 50,
            "permissions": ["query:*", "mutation:moderate"]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 201);
    let body = response.json::<RoleResponse>().await.unwrap();
    assert_eq!(body.name, "moderator");
}

#[tokio::test]
async fn test_audit_event_query() {
    let client = setup_test_client().await;
    let admin_token = get_admin_token();

    let response = client
        .get("/api/v1/audit/events?event_type=user_login&limit=10")
        .header("Authorization", &admin_token)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body = response.json::<AuditQueryResponse>().await.unwrap();
    assert!(body.events.len() > 0);
}
```

#### GREEN: Implement endpoints

Basic handlers for each endpoint with:
- Authorization checks (admin only)
- Input validation
- Database operations
- Error handling

#### CLEANUP
- All endpoints tested
- Consistent error responses

---

### Cycle 11.6: Audit Log Database Schema & Migrations

**Objective**: Create PostgreSQL schema for audit logging and multi-tenancy

#### Files
- `crates/fraiseql-server/migrations/0010_audit_log.sql`
- `crates/fraiseql-server/migrations/0011_tenants.sql`
- `crates/fraiseql-server/migrations/0012_rbac.sql`

#### Migrations
```sql
-- 0010_audit_log.sql
CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    event_type VARCHAR(255) NOT NULL,
    user_id UUID,
    username VARCHAR(255),
    ip_address INET,
    resource_type VARCHAR(255),
    resource_id UUID,
    action VARCHAR(255),
    before_state JSONB,
    after_state JSONB,
    status VARCHAR(50) NOT NULL,  -- success, failure, denied
    error_message TEXT,
    metadata JSONB
);

CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp DESC);
CREATE INDEX idx_audit_log_user_id ON audit_log(user_id);
CREATE INDEX idx_audit_log_event_type ON audit_log(event_type);

-- 0011_tenants.sql
CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata JSONB
);

ALTER TABLE users ADD COLUMN tenant_id UUID NOT NULL REFERENCES tenants(id);
CREATE INDEX idx_users_tenant_id ON users(tenant_id);

-- 0012_rbac.sql
CREATE TABLE roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name VARCHAR(255) NOT NULL,
    level INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

CREATE TABLE permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    description TEXT
);

CREATE TABLE role_permissions (
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id UUID NOT NULL REFERENCES permissions(id),
    PRIMARY KEY (role_id, permission_id)
);

CREATE TABLE user_roles (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, role_id)
);
```

#### Tests
```rust
#[tokio::test]
async fn test_migrations_apply() {
    let pool = setup_test_db().await;

    // Run all migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap();

    // Verify tables exist
    let result: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = $1"
    )
    .bind("audit_log")
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(result.0, 1);
}
```

#### CLEANUP
- Verify schema consistency
- Check index performance

---

## Database Schema Diagram

```
┌─────────────────────┐
│     tenants         │
├─────────────────────┤
│ id (PK)             │
│ name (UNIQUE)       │
│ created_at          │
│ metadata            │
└──────────┬──────────┘
           │
           │ 1:N
           ▼
┌─────────────────────┐         ┌──────────────────┐
│       users         │◄────────┤     roles        │
├─────────────────────┤         ├──────────────────┤
│ id (PK)             │  many:many
│ name                │         │ id (PK)          │
│ email               │         │ tenant_id (FK)   │
│ tenant_id (FK)      │         │ name             │
│ password_hash       │         │ level            │
│ created_at          │         │ created_at       │
└─────────────────────┘         └──────────────────┘
                                         │
                                    N:M relationship
                                         │
                                ┌────────▼────────┐
                                │   permissions   │
                                ├─────────────────┤
                                │ id (PK)         │
                                │ name (UNIQUE)   │
                                │ description     │
                                └─────────────────┘

┌──────────────────────────────┐
│       audit_log              │
├──────────────────────────────┤
│ id (PK)                      │
│ timestamp (indexed)          │
│ event_type (indexed)         │
│ user_id (indexed, FK)        │
│ username                     │
│ ip_address                   │
│ resource_type                │
│ resource_id                  │
│ action                       │
│ before_state (JSONB)         │
│ after_state (JSONB)          │
│ status                       │
│ error_message                │
│ metadata (JSONB)             │
└──────────────────────────────┘
```

---

## Verification

### Per-Cycle
```bash
cargo test --lib
cargo clippy --all-targets --all-features -- -D warnings
```

### Integration
```bash
# Database tests
TEST_DATABASE_URL=postgresql://localhost/fraiseql_test cargo nextest run rbac
TEST_DATABASE_URL=postgresql://localhost/fraiseql_test cargo nextest run audit
TEST_DATABASE_URL=postgresql://localhost/fraiseql_test cargo nextest run tenancy

# API tests
cargo nextest run routes::api::rbac
```

---

## Status

- [ ] Not Started
- [ ] In Progress (Cycle X)
- [ ] Complete

---

## Next Phase

→ Phase 12: Enterprise Features Part 2 (Secrets Management, Encryption)
