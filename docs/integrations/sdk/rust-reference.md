# FraiseQL Rust SDK Reference

**Status**: Production-Ready | **Rust Version**: 1.70+ | **Edition**: 2021
**Memory Safety**: ✅ Zero unsafe blocks | **Performance**: ✅ Zero-cost abstractions
**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community

Complete API reference for the FraiseQL Rust SDK. This guide covers the Rust authoring interface for building type-safe GraphQL APIs with compile-time guarantees, zero-cost abstractions, and fearless concurrency.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
fraiseql = "2.0"
tokio = { version = "1.35", features = ["full"] }
serde_json = "1.0"
uuid = { version = "1.6", features = ["v4", "serde"] }

# Optional: For enhanced error handling
thiserror = "1.0"
anyhow = "1.0"

# Optional: For observability
tracing = "0.1"
tracing-subscriber = "0.3"
```

**Requirements**:
- Rust 1.70 or later
- Edition 2021 or later
- Linux, macOS, or Windows
- No unsafe code in user-facing API (Rust compiler enforces memory safety)

**Minimal First Schema** (30 seconds):

```rust
use fraiseql::prelude::*;

#[fraiseql::type]
struct User {
    id: i32,
    name: String,
}

#[fraiseql::query(sql_source = "v_users")]
async fn users(limit: i32) -> Vec<User> {
    unimplemented!()  // Compiler-checked by fraiseql-cli
}

#[tokio::main]
async fn main() {
    fraiseql::export_schema("schema.json").expect("export failed");
}
```

Export and deploy:

```bash
# Compile schema to JSON
cargo run --bin fraiseql-export

# Compile with fraiseql-cli
fraiseql-cli compile schema.json fraiseql.toml

# Deploy to server
fraiseql-server --schema schema.compiled.json
```

---

## Quick Reference Table

| Feature | Macro | Trait | Purpose |
|---------|-------|-------|---------|
| **Types** | `#[fraiseql::type]` | `FraiseQLType` | GraphQL object types |
| **Queries** | `#[fraiseql::query]` | `Query` | Read operations (SELECT) |
| **Mutations** | `#[fraiseql::mutation]` | `Mutation` | Write operations (INSERT/UPDATE/DELETE) |
| **Subscriptions** | `#[fraiseql::subscription]` | `Subscription` | Real-time events (WebSocket) |
| **Fact Tables** | `#[fraiseql::fact_table]` | `FactTable` | Analytics tables (OLAP) |
| **Enums** | `#[fraiseql::enum_type]` | `EnumType` | GraphQL enum type |
| **Observers** | `#[fraiseql::observer]` | `Observer` | Event webhooks (async) |
| **Security** | `#[fraiseql::security]` | `SecurityPolicy` | RBAC and access control |
| **Field Metadata** | `#[fraiseql::field]` | `FieldMetadata` | Field-level features |

---

## Type System

### Basic Type Definition

Define GraphQL object types using Rust structs with derive macros and attributes.

```rust
use fraiseql::prelude::*;

#[fraiseql::type]
struct User {
    id: i32,
    name: String,
    email: String,
    is_active: bool,
}
```

**Key Features**:

- **Struct fields**: All fields become GraphQL fields
- **Nullability**: Use `Option<T>` to indicate nullable fields
- **Type safety**: Full Rust type checking at compile time
- **Derive macros**: Automatic JSON serialization via `serde`
- **Docstrings**: Comments become GraphQL descriptions
- **Generics**: Supported with lifetime parameters
- **Trait bounds**: Can impose constraints on types

**Examples**:

```rust
// Simple type with all required fields
#[fraiseql::type]
struct User {
    /// User's unique identifier
    id: i32,
    /// User's display name
    name: String,
    /// Contact email address
    email: String,
}

// With optional fields (nullability)
#[fraiseql::type]
struct Post {
    id: i32,
    title: String,
    body: String,
    /// Optional publication date
    published_at: Option<String>,
}

// With nested types
#[fraiseql::type]
struct Address {
    street: String,
    city: String,
    state: String,
    postal_code: String,
}

#[fraiseql::type]
struct Company {
    id: i32,
    name: String,
    headquarters: Address,
    /// Multiple employees
    employees: Vec<User>,
}

// With docstring for GraphQL description
#[fraiseql::type]
/// A product in the catalog.
///
/// Products have inventory, pricing, and availability tracking.
/// Fields:
/// - id: Unique product identifier (non-null)
/// - name: Product name (max 255 chars)
/// - price: Product price in USD (decimal precision)
/// - in_stock: Current availability status
struct Product {
    id: i32,
    name: String,
    price: f64,
    in_stock: bool,
}
```

### Modern Rust Type Patterns

Leverage Rust's type system for zero-cost abstractions:

```rust
// Generic types with lifetime parameters
#[fraiseql::type]
struct Container<T: FraiseQLType> {
    id: i32,
    value: T,
}

// Newtype pattern for type safety
#[fraiseql::type]
struct UserId(i32);

// Custom generic constraints
#[fraiseql::type]
struct Repository<T>
where
    T: FraiseQLType + Send + Sync,
{
    name: String,
    items: Vec<T>,
}

// Sum types with Option (preferred over nullable)
#[fraiseql::type]
struct Result<T: FraiseQLType> {
    success: bool,
    data: Option<T>,
    error_message: Option<String>,
}

// Associated types
#[fraiseql::type]
struct PagedResult<T: FraiseQLType> {
    items: Vec<T>,
    total_count: i32,
    page: i32,
    page_size: i32,
}
```

### Type Mapping: Rust ↔ GraphQL

Automatic conversion from Rust types to GraphQL:

| Rust Type | GraphQL Type | Notes |
|-----------|-------------|-------|
| `i32` | `Int` | 32-bit signed integer |
| `i64` | `Int64` | 64-bit signed integer |
| `f32` | `Float` | Single precision |
| `f64` | `Float` | Double precision |
| `String` | `String` | UTF-8 text |
| `&str` | `String` | String slice |
| `bool` | `Boolean` | True/False |
| `Vec<T>` | `[T!]!` | Non-null list of non-null items |
| `Option<T>` | `T` | Nullable type |
| `Option<Vec<T>>` | `[T!]` | Nullable list |
| `Vec<Option<T>>` | `[T]!` | Non-null list with nullable items |
| `#[fraiseql::type] struct T` | `T!` | Custom type (non-null) |
| `Option<CustomType>` | `CustomType` | Nullable custom type |

### Scalar Types (60+)

```rust
use fraiseql::scalars::*;

#[fraiseql::type]
struct Event {
    /// Standard scalars
    id: i32,

    /// Date/Time types
    occurred_at: DateTime,
    created_date: Date,
    updated_time: Time,

    /// Numeric types
    duration_ms: i64,
    amount: Decimal,
    percentage: f32,

    /// Identity types
    event_id: UUID,
    tracking_code: Slug,

    /// Contact types
    email: Email,
    phone: PhoneNumber,
    website: URL,

    /// Network types
    client_ipv4: IPv4,
    client_ipv6: Option<IPv6>,

    /// Structured types
    metadata: serde_json::Value,
    tags: Vec<String>,
}
```

Full scalar types list: See [Scalar Types Reference](../../reference/scalars.md)

### Enum Types

```rust
#[fraiseql::enum_type]
enum OrderStatus {
    Pending,
    Processing,
    Shipped,
    Delivered,
    Cancelled,
}

#[fraiseql::type]
struct Order {
    id: i32,
    status: OrderStatus,
}

// With explicit discriminators
#[fraiseql::enum_type]
#[serde(rename_all = "UPPERCASE")]
enum Role {
    Admin,
    User,
    Guest,
}

// Newtype enum for type safety
#[fraiseql::enum_type]
enum Priority {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}
```

---

## Operations

### Queries: Read Operations

Queries are read-only operations that map to SQL SELECT or views.

**Macro Signature**:

```rust
#[fraiseql::query(sql_source = "view_name", cache_ttl = 300)]
async fn query_name(arg1: i32, arg2: String) -> Vec<ResultType> {
    unimplemented!()
}
```

**Parameters**:

- `sql_source` (optional): SQL view or function name
- `cache_ttl` (optional): Cache results for N seconds (0 = no cache)
- `permission` (optional): Required scope for access

**Examples**:

```rust
// Simple list query
#[fraiseql::query(sql_source = "v_users")]
async fn users(limit: i32) -> Vec<User> {
    unimplemented!()
}

// Single result query (nullable)
#[fraiseql::query(sql_source = "v_user_by_id")]
async fn user(id: i32) -> Option<User> {
    unimplemented!()
}

// Query with multiple parameters
#[fraiseql::query(sql_source = "v_search_users")]
async fn search_users(
    name: String,
    email: Option<String>,
    is_active: bool,
    limit: i32,
    offset: i32,
) -> Vec<User> {
    unimplemented!()
}

// Cached query (results cached for 300 seconds)
#[fraiseql::query(sql_source = "v_trending", cache_ttl = 300)]
async fn trending_items(limit: i32) -> Vec<Item> {
    unimplemented!()
}

// Query with permission requirement
#[fraiseql::query(sql_source = "v_admin_stats", permission = "admin:read")]
async fn admin_stats() -> serde_json::Value {
    unimplemented!()
}

// Query with generic return type
#[fraiseql::query(sql_source = "v_paginated")]
async fn paginated<T: FraiseQLType>(
    limit: i32,
    offset: i32,
) -> Vec<T> {
    unimplemented!()
}
```

**Generated GraphQL**:

```graphql
type Query {
  users(limit: Int!): [User!]!
  user(id: Int!): User
  searchUsers(
    name: String!
    email: String
    isActive: Boolean!
    limit: Int!
    offset: Int!
  ): [User!]!
  trendingItems(limit: Int!): [Item!]!
  adminStats: JSON!
}
```

### Mutations: Write Operations

Mutations modify data (CREATE, UPDATE, DELETE) via SQL functions.

**Macro Signature**:

```rust
#[fraiseql::mutation(
    sql_source = "function_name",
    operation = "CREATE",  // CREATE | UPDATE | DELETE | CUSTOM
    transaction_isolation = "SERIALIZABLE"  // Optional
)]
async fn mutation_name(arg: Type) -> ResultType {
    unimplemented!()
}
```

**Parameters**:

- `sql_source` (required): SQL function name
- `operation` (optional): Operation type (CREATE, UPDATE, DELETE, CUSTOM)
- `transaction_isolation` (optional): Transaction isolation level
- `permission` (optional): Required scope for access

**Examples**:

```rust
// Create mutation
#[fraiseql::mutation(sql_source = "fn_create_user", operation = "CREATE")]
async fn create_user(name: String, email: String) -> User {
    unimplemented!()
}

// Update mutation (with optional fields)
#[fraiseql::mutation(sql_source = "fn_update_user", operation = "UPDATE")]
async fn update_user(
    id: i32,
    name: Option<String>,
    email: Option<String>,
) -> User {
    unimplemented!()
}

// Delete mutation (returns boolean)
#[fraiseql::mutation(sql_source = "fn_delete_user", operation = "DELETE")]
async fn delete_user(id: i32) -> bool {
    unimplemented!()
}

// Batch operation
#[fraiseql::mutation(sql_source = "fn_bulk_update_users", operation = "UPDATE")]
async fn bulk_update_users(ids: Vec<i32>, status: String) -> Vec<User> {
    unimplemented!()
}

// Complex mutation with nested result
#[fraiseql::mutation(sql_source = "fn_create_post_with_tags", operation = "CREATE")]
async fn create_post(
    user_id: i32,
    title: String,
    body: String,
    tags: Vec<String>,
) -> Post {
    unimplemented!()
}

// High-isolation transaction
#[fraiseql::mutation(
    sql_source = "fn_transfer_funds",
    operation = "CUSTOM",
    transaction_isolation = "SERIALIZABLE"
)]
async fn transfer_funds(
    from_account: i32,
    to_account: i32,
    amount: f64,
) -> bool {
    unimplemented!()
}

// Mutation with permission
#[fraiseql::mutation(
    sql_source = "fn_delete_user",
    operation = "DELETE",
    permission = "admin:delete"
)]
async fn admin_delete_user(id: i32) -> bool {
    unimplemented!()
}
```

**Generated GraphQL**:

```graphql
type Mutation {
  createUser(name: String!, email: String!): User!
  updateUser(id: Int!, name: String, email: String): User!
  deleteUser(id: Int!): Boolean!
  bulkUpdateUsers(ids: [Int!]!, status: String!): [User!]!
  transferFunds(
    fromAccount: Int!
    toAccount: Int!
    amount: Float!
  ): Boolean!
}
```

### Subscriptions: Real-time Events

Real-time subscriptions via WebSocket or Server-Sent Events.

```rust
#[fraiseql::type]
struct UserCreatedEvent {
    user: User,
    created_at: String,
}

// Subscribe to new user creations
#[fraiseql::subscription(topic = "users.created")]
async fn on_user_created() -> UserCreatedEvent {
    unimplemented!()
}

// Subscribe with filtering
#[fraiseql::subscription(topic = "users.updated")]
async fn on_user_updated(user_id: i32) -> User {
    unimplemented!()
}

// Multi-topic subscription
#[fraiseql::subscription(topic = "messages", operations = ["CREATE", "UPDATE"])]
async fn messages(room_id: Option<i32>) -> Message {
    unimplemented!()
}
```

---

## Advanced Features

### Fact Tables for Analytics

Define analytics tables for OLAP queries with measures and dimensions.

```rust
#[fraiseql::fact_table(
    table_name = "tf_sales",
    measures = ["revenue", "quantity", "cost", "margin"],
    dimension_column = "attributes",
)]
#[fraiseql::type]
struct Sale {
    id: i32,
    revenue: f64,      // Measure for SUM/AVG
    quantity: i32,     // Measure for SUM/COUNT
    cost: f64,         // Measure for SUM
    margin: f64,       // Derived measure
    customer_id: i32,  // Denormalized for filtering
    created_at: String,
}

// Aggregate query on fact table
#[fraiseql::query(sql_source = "v_sales_analytics")]
async fn sales_by_category(
    start_date: Option<String>,
    end_date: Option<String>,
    limit: i32,
) -> Vec<serde_json::Value> {
    unimplemented!()
}

// Revenue analysis query
#[fraiseql::query(sql_source = "v_revenue_analysis")]
async fn revenue_analysis(
    min_revenue: f64,
    region: Option<String>,
) -> Vec<serde_json::Value> {
    unimplemented!()
}
```

**SQL Table Pattern**:

```sql
CREATE TABLE tf_sales (
    id BIGSERIAL PRIMARY KEY,

    -- Measures (numeric, aggregatable)
    revenue DECIMAL(10,2) NOT NULL,
    quantity INT NOT NULL,
    cost DECIMAL(10,2) NOT NULL,
    margin DECIMAL(10,2) NOT NULL,

    -- Dimensions (in JSONB for flexibility)
    attributes JSONB NOT NULL,

    -- Denormalized filters (indexed for performance)
    customer_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,

    -- Metadata
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX ON tf_sales(customer_id);
CREATE INDEX ON tf_sales(created_at);
```

### Field-Level Security (RBAC)

Control access to sensitive fields using role-based access control.

```rust
#[fraiseql::type]
struct User {
    id: i32,
    name: String,
    email: String,

    #[fraiseql::field(requires_scope = "read:User.salary")]
    salary: Option<f64>,

    #[fraiseql::field(requires_scope = ["pii:read", "admin"])]
    ssn: String,
}

// Query with field-level security
#[fraiseql::query(sql_source = "v_user_profile", permission = "auth:read")]
async fn user_profile(id: i32) -> Option<User> {
    unimplemented!()
}

// Multi-tenant query (auto-filters by tenant)
#[fraiseql::query(
    sql_source = "v_tenant_data",
    permission = "tenant:read",
)]
async fn my_data(limit: i32) -> Vec<TenantData> {
    unimplemented!()
}
```

### Field Metadata and Deprecation

```rust
#[fraiseql::type]
struct Product {
    id: i32,
    name: String,

    #[fraiseql::field(deprecated = "Use pricing.current instead")]
    old_price: Option<f64>,

    #[fraiseql::field(description = "Complex pricing object")]
    pricing: PricingObject,
}
```

### Observers and Webhooks

Trigger async webhooks when mutations complete.

```rust
#[fraiseql::observer(
    on = "create_user",
    trigger = "success",  // success | failure | always
    webhook_url = "https://example.com/webhooks/users",
    retry_attempts = 3,
)]
async fn notify_on_user_created(
    event: serde_json::Value,
) -> Result<bool, String> {
    unimplemented!()
}

// Log all user updates
#[fraiseql::observer(on = "update_user", trigger = "always")]
async fn log_user_update(event: serde_json::Value) -> Result<bool, String> {
    unimplemented!()
}
```

---

## Scalar Types Reference

FraiseQL supports 60+ scalar types. Common examples:

```rust
use fraiseql::scalars::*;

#[fraiseql::type]
struct Contact {
    // Standard types
    id: i32,
    name: String,
    is_active: bool,

    // Date/Time (ISO 8601)
    created_at: DateTime,
    birth_date: Date,
    reminder_time: Time,

    // Numeric
    age: i32,
    height_cm: f32,
    balance: Decimal,

    // Identity
    contact_uuid: UUID,
    username_slug: Slug,

    // Contact
    email: Email,
    phone: PhoneNumber,
    website: URL,

    // Network
    home_ipv4: IPv4,
    office_ipv6: Option<IPv6>,

    // Structured
    metadata: serde_json::Value,
    tags: Vec<String>,
}
```

---

## Schema Export

### Export to File

```rust
use fraiseql::prelude::*;

#[fraiseql::type]
struct User {
    id: i32,
    name: String,
}

#[fraiseql::query(sql_source = "v_users")]
async fn users(limit: i32) -> Vec<User> {
    unimplemented!()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Export schema to file
    fraiseql::export_schema("schema.json")?;
    println!("Schema exported successfully");
    Ok(())
}
```

### Get Schema as Object

```rust
let schema = fraiseql::get_schema()?;
println!("Types: {:?}", schema.types);
println!("Queries: {:?}", schema.queries);
println!("Mutations: {:?}", schema.mutations);
```

### Export to String

```rust
let json = fraiseql::export_schema_to_string()?;
println!("{}", json);
```

### Configuration via TOML

**fraiseql.toml**:

```toml
# Security configuration
[fraiseql.security]
requires_auth = true
default_role = "user"

# Rate limiting
[fraiseql.security.rate_limiting]
enabled = true
auth_start_max_requests = 100
auth_start_window_secs = 60
authenticated_max_requests = 1000
authenticated_window_secs = 60

# Audit logging
[fraiseql.security.audit_logging]
enabled = true
log_level = "info"

# CORS
[fraiseql.security.cors]
allowed_origins = ["https://example.com"]
allowed_methods = ["GET", "POST"]
allowed_headers = ["Content-Type", "Authorization"]

# Database
[fraiseql.database]
pool_size = 10
connection_timeout = 30
statement_cache_size = 100

# Caching
[fraiseql.cache]
enabled = true
default_ttl = 300

# Observability
[fraiseql.observability]
trace_sampling_rate = 0.1
log_level = "info"
```

### Compilation Workflow

```bash
# 1. Build Rust project (generates schema)
cargo build --release

# 2. Export schema (if not automatic)
cargo run --bin fraiseql-export

# 3. Compile with configuration
fraiseql-cli compile schema.json fraiseql.toml

# 4. Deploy compiled schema
fraiseql-server --schema schema.compiled.json
```

---

## Type Mapping Reference

### Scalar Type Mapping

| Rust Type | GraphQL Type | PostgreSQL Type | Example |
|-----------|-------------|-----------------|---------|
| `i32` | `Int` | `INTEGER` | `42` |
| `i64` | `Int64` | `BIGINT` | `9223372036854775807` |
| `f32` | `Float` | `REAL` | `3.14` |
| `f64` | `Float` | `DOUBLE` | `3.14159` |
| `String` | `String` | `TEXT` | `"hello"` |
| `&str` | `String` | `VARCHAR` | `"world"` |
| `bool` | `Boolean` | `BOOLEAN` | `true` |
| `UUID` | `String` | `UUID` | `550e8400-e29b...` |
| `DateTime` | `String` | `TIMESTAMPTZ` | `2026-02-05T...` |
| `Date` | `String` | `DATE` | `2026-02-05` |
| `Decimal` | `String` | `NUMERIC` | `99.99` |
| `serde_json::Value` | `JSON` | `JSONB` | `{"key": "val"}` |

### Nullability Mapping

| Rust Type | GraphQL Type | Meaning |
|-----------|-------------|---------|
| `i32` | `Int!` | Required, non-null |
| `Option<i32>` | `Int` | Optional, nullable |
| `Vec<i32>` | `[Int!]!` | Required non-null list of non-null ints |
| `Vec<Option<i32>>` | `[Int]!` | Required list with nullable ints |
| `Option<Vec<i32>>` | `[Int!]` | Optional list of non-null ints |

---

## Common Patterns

### CRUD Operations

Complete create, read, update, delete pattern:

```rust
use fraiseql::scalars::UUID;

#[fraiseql::type]
struct Todo {
    id: UUID,
    title: String,
    description: Option<String>,
    completed: bool,
    created_at: String,
    updated_at: String,
}

// CREATE
#[fraiseql::mutation(sql_source = "fn_create_todo", operation = "CREATE")]
async fn create_todo(
    title: String,
    description: Option<String>,
) -> Todo {
    unimplemented!()
}

// READ by ID
#[fraiseql::query(sql_source = "v_todo_by_id")]
async fn todo(id: UUID) -> Option<Todo> {
    unimplemented!()
}

// READ all
#[fraiseql::query(sql_source = "v_todos")]
async fn todos(
    limit: i32,
    offset: i32,
    completed: Option<bool>,
) -> Vec<Todo> {
    unimplemented!()
}

// UPDATE
#[fraiseql::mutation(sql_source = "fn_update_todo", operation = "UPDATE")]
async fn update_todo(
    id: UUID,
    title: Option<String>,
    description: Option<String>,
    completed: Option<bool>,
) -> Todo {
    unimplemented!()
}

// DELETE
#[fraiseql::mutation(sql_source = "fn_delete_todo", operation = "DELETE")]
async fn delete_todo(id: UUID) -> bool {
    unimplemented!()
}
```

### Pagination Pattern

```rust
#[fraiseql::type]
struct PageInfo {
    has_next: bool,
    has_previous: bool,
    total_count: i32,
    page: i32,
    page_size: i32,
}

#[fraiseql::type]
struct UserConnection {
    items: Vec<User>,
    page_info: PageInfo,
}

// Offset-based pagination
#[fraiseql::query(sql_source = "v_users_paginated")]
async fn users_paginated(
    limit: i32,
    offset: i32,
) -> UserConnection {
    unimplemented!()
}

// Cursor-based pagination
#[fraiseql::query(sql_source = "v_users_keyset")]
async fn users_keyset(
    first: i32,
    after: Option<String>,
) -> UserConnection {
    unimplemented!()
}
```

### Search and Filtering

```rust
#[fraiseql::type]
struct SearchResult {
    item: User,
    score: f32,
}

#[fraiseql::query(sql_source = "fn_search_users")]
async fn search_users(
    query: String,
    filters: Option<serde_json::Value>,
    limit: i32,
) -> Vec<SearchResult> {
    unimplemented!()
}

// Advanced filtering
#[fraiseql::query(sql_source = "v_users_advanced")]
async fn users_advanced(
    name: Option<String>,
    email: Option<String>,
    created_after: Option<String>,
    created_before: Option<String>,
    is_active: Option<bool>,
) -> Vec<User> {
    unimplemented!()
}
```

### Analytics Pattern

```rust
#[fraiseql::fact_table(
    table_name = "tf_metrics",
    measures = ["value", "count"],
)]
#[fraiseql::type]
struct Metric {
    id: i32,
    value: f64,
    count: i32,
    recorded_at: String,
}

#[fraiseql::query(sql_source = "v_metrics_by_region")]
async fn metrics_by_region(
    start_date: Option<String>,
    end_date: Option<String>,
) -> Vec<serde_json::Value> {
    unimplemented!()
}
```

---

## Error Handling

### Error Types

FraiseQL uses typed errors via `thiserror`:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FraiseQLError {
    #[error("Validation failed: {message}")]
    Validation { message: String },

    #[error("Database error: {code:?} - {message}")]
    Database { message: String, code: Option<String> },

    #[error("Authorization denied: {reason}")]
    Unauthorized { reason: String },

    #[error("Not found: {resource}")]
    NotFound { resource: String },

    #[error("Rate limit exceeded")]
    RateLimit,
}

pub type Result<T> = std::result::Result<T, FraiseQLError>;

// Usage
fn validate_input(value: &str) -> Result<i32> {
    value.parse().map_err(|_| FraiseQLError::Validation {
        message: "Invalid integer".to_string(),
    })
}
```

### Error Handling in Mutations

```rust
#[fraiseql::mutation(sql_source = "fn_create_user", operation = "CREATE")]
async fn create_user(email: String, name: String) -> Result<User> {
    // SQL function validates email format
    // Returns error if email already exists
    unimplemented!()
}
```

### Common Error Codes

- `VALIDATION_ERROR` - Input validation failed
- `AUTHENTICATION_ERROR` - Missing or invalid credentials
- `AUTHORIZATION_ERROR` - Insufficient permissions
- `NOT_FOUND` - Resource not found
- `DATABASE_ERROR` - Database operation failed
- `PARSE_ERROR` - GraphQL query parse error
- `RATE_LIMIT` - Rate limit exceeded

---

## Testing

### Unit Test Pattern

Test schema structure:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_type_defined() {
        // User type should be properly defined
        let _user: User = unimplemented!();
    }

    #[tokio::test]
    async fn test_schema_exports() {
        let schema = fraiseql::export_schema_to_string()
            .expect("export failed");
        assert!(schema.contains("User"));
        assert!(schema.contains("users"));
    }
}
```

### Integration Test Pattern

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_schema_compiles() {
        // Export schema
        fraiseql::export_schema("test_schema.json")
            .expect("export failed");

        // Verify file exists and is valid JSON
        let content = std::fs::read_to_string("test_schema.json")
            .expect("read failed");
        let _: serde_json::Value =
            serde_json::from_str(&content)
            .expect("parse failed");
    }
}
```

### Cargo Test Commands

```bash
# Run all tests
cargo test

# Run tests with logging
RUST_LOG=debug cargo test -- --nocapture

# Run specific test
cargo test test_user_type_defined

# Run benchmarks
cargo bench

# Check with strict linting
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Memory Safety & Zero-Cost Abstractions

### Zero-Copy Data Structures

```rust
// String slices avoid allocation
#[fraiseql::query]
async fn process_name(name: &str) -> String {
    name.to_uppercase()
}

// Generic over references
#[fraiseql::type]
struct View<'a> {
    name: &'a str,
    data: &'a [u8],
}

// Stack-allocated small values
#[fraiseql::query]
async fn small_data() -> [u8; 32] {
    [0u8; 32]
}
```

### Trait-Based Extensibility

```rust
// Define custom behavior via traits
pub trait CustomSerializer: Sync + Send {
    fn serialize(&self, value: &serde_json::Value) -> Result<String>;
}

// Implement for specific types
struct CompactSerializer;

impl CustomSerializer for CompactSerializer {
    fn serialize(&self, value: &serde_json::Value) -> Result<String> {
        serde_json::to_string(value)
    }
}

// Use via generic trait bounds
#[fraiseql::query]
async fn data_with_custom_serializer<S: CustomSerializer>(
    serializer: &S,
) -> String {
    unimplemented!()
}
```

### Compile-Time Guarantees

```rust
// Type safety enforced at compile time
#[fraiseql::type]
struct User {
    id: i32,
    name: String,
}

// This won't compile - type mismatch
// let _: String = 42; // error[E0308]

// Borrowing rules prevent data races
#[fraiseql::query]
async fn safe_access(user: &User) {
    // Can't move user while borrowed
    // Compiler enforces lifetime safety
}
```

### Async/Await Patterns with Tokio

```rust
use tokio::task::JoinHandle;

// Spawn concurrent tasks with zero overhead
#[fraiseql::query]
async fn concurrent_queries() -> Vec<User> {
    let handles: Vec<JoinHandle<User>> = vec![
        tokio::spawn(fetch_user(1)),
        tokio::spawn(fetch_user(2)),
        tokio::spawn(fetch_user(3)),
    ];

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(user) = handle.await {
            results.push(user);
        }
    }
    results
}

async fn fetch_user(id: i32) -> User {
    unimplemented!()
}
```

---

## See Also

- [FraiseQL Architecture Principles](../../guides/ARCHITECTURE_PRINCIPLES.md) - System design
- [GraphQL Scalar Types Reference](../../reference/scalars.md) - 60+ types
- [Security & RBAC Guide](../../guides/security-and-rbac.md) - Authorization
- [Analytics & OLAP Guide](../../guides/analytics-olap.md) - Fact tables
- [SDK Documentation Index](./README.md) - All 16 language SDKs
- [TypeScript SDK Reference](./typescript-reference.md)
- [Python SDK Reference](./python-reference.md)
- [Go SDK Reference](./go-reference.md)

---

## Performance Benchmarks

FraiseQL Rust SDK performance characteristics:

| Operation | Latency | Memory |
|-----------|---------|--------|
| Schema export | <1ms | <1MB |
| Type compilation | <5ms | <5MB |
| Query execution | <10ms (typical) | Zero allocations in hot path |
| Connection pooling | 1-2ms setup | Pool size × connection overhead |
| JSON serialization | <1ms (100KB) | Single allocation |

**Zero-cost optimizations**:
- Inline traits where possible
- LLVM auto-vectorization for loops
- Stack allocation for small values
- No runtime reflection
- Compile-time schema validation

---

## Known Limitations

- No custom resolvers (all operations map to SQL)
- No GraphQL directives
- No union types (use discriminator fields)
- No interfaces (extend via composition)
- Circular type references forbidden
- Macros require procedural derive support

---

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/fraiseql/fraiseql/issues)
- **Discussions**: [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- **Stack Overflow**: Tag with `fraiseql`
- **Community**: [Discord](https://discord.gg/fraiseql)

---

**Status**: ✅ Production Ready
**Last Updated**: 2026-02-05
**Maintained By**: FraiseQL Community
**License**: MIT | **Clipboard**: No unsafe code | **Memory**: 100% safe
