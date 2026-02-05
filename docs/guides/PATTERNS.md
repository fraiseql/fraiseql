<!-- Skip to main content -->
---
title: Common Patterns - Real-World Solutions
description: - GraphQL fundamentals (types, fields, queries, mutations)
keywords: ["workflow", "debugging", "implementation", "best-practices", "deployment", "saas", "realtime", "ecommerce"]
tags: ["documentation", "reference"]
---

# Common Patterns - Real-World Solutions

**Status:** ✅ Production Ready
**Audience:** Developers, Architects
**Reading Time:** 20-30 minutes
**Last Updated:** 2026-02-05

## Prerequisites

**Required Knowledge:**

- GraphQL fundamentals (types, fields, queries, mutations)
- FraiseQL schema definition and configuration (see [GETTING_STARTED.md](../GETTING_STARTED.md))
- Authentication and authorization concepts
- Multi-tenancy and data isolation patterns
- Caching strategies and trade-offs
- Pagination and filtering techniques
- Error handling best practices
- Database relationships and foreign keys

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- Your chosen SDK language:
  - Python 3.10+
  - TypeScript/Node.js 18+
  - Go 1.21+
  - Java 11+
  - Or any of the other 16 supported languages
- A code editor or IDE
- curl or Postman (for API testing)
- Git (optional, for version control)

**Required Infrastructure:**

- FraiseQL server running with your schema
- PostgreSQL, MySQL, SQLite, or SQL Server database
- Network connectivity to FraiseQL server
- Example data loaded in database (for testing patterns)

**Optional but Recommended:**

- Test database with sample data
- GraphQL IDE (GraphQL Playground, Apollo Sandbox, Postman)
- API monitoring tools
- Logging and debugging tools

**Time Estimate per Pattern:** 20-60 minutes depending on complexity

---

## Pattern 1: User Authentication

### Problem

How do I add user authentication to my GraphQL API?

### Solution

Implement a complete authentication flow: register → login → validate token → query protected data.

### Schema Definition

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "User",
      "fields": [
        { "name": "id", "type": "ID", "nonNull": true },
        { "name": "email", "type": "String", "nonNull": true },
        { "name": "name", "type": "String", "nonNull": true },
        { "name": "createdAt", "type": "DateTime", "nonNull": true }
      ]
    },
    {
      "name": "AuthPayload",
      "fields": [
        { "name": "token", "type": "String", "nonNull": true },
        { "name": "user", "type": "User", "nonNull": true }
      ]
    }
  ],
  "queries": [
    {
      "name": "me",
      "returnType": "User",
      "isList": false,
      "args": []
    }
  ],
  "mutations": [
    {
      "name": "register",
      "args": [
        { "name": "email", "type": "String", "nonNull": true },
        { "name": "password", "type": "String", "nonNull": true },
        { "name": "name", "type": "String", "nonNull": true }
      ],
      "returnType": "AuthPayload",
      "isList": false
    },
    {
      "name": "login",
      "args": [
        { "name": "email", "type": "String", "nonNull": true },
        { "name": "password", "type": "String", "nonNull": true }
      ],
      "returnType": "AuthPayload",
      "isList": false
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Implementation

```rust
<!-- Code example in RUST -->
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};
use serde::{Serialize, Deserialize};
use bcrypt::{hash, verify};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    user_id: String,
    exp: usize,
}

pub async fn register(
    email: String,
    password: String,
    name: String,
    db: &Database,
) -> Result<AuthPayload> {
    // Validate email format
    if !email.contains('@') {
        return Err(FraiseQLError::Validation {
            message: "Invalid email format".to_string(),
            path: Some("email".to_string()),
        });
    }

    // Check if user exists
    let existing = db.query_user_by_email(&email).await?;
    if existing.is_some() {
        return Err(FraiseQLError::Validation {
            message: "Email already registered".to_string(),
            path: Some("email".to_string()),
        });
    }

    // Hash password with bcrypt
    let hashed_password = hash(&password, 12)
        .map_err(|e| FraiseQLError::Validation {
            message: format!("Password hashing failed: {}", e),
            path: None,
        })?;

    // Create user in database
    let user = db.create_user(
        &email,
        &hashed_password,
        &name,
    ).await?;

    // Generate JWT token
    let token = generate_token(&user.id)?;

    Ok(AuthPayload {
        token,
        user,
    })
}

pub async fn login(
    email: String,
    password: String,
    db: &Database,
) -> Result<AuthPayload> {
    // Find user by email
    let user = db.query_user_by_email(&email).await?
        .ok_or_else(|| FraiseQLError::Validation {
            message: "Invalid credentials".to_string(),
            path: None,
        })?;

    // Verify password
    let password_valid = verify(&password, &user.password_hash)
        .map_err(|_| FraiseQLError::Validation {
            message: "Invalid credentials".to_string(),
            path: None,
        })?;

    if !password_valid {
        return Err(FraiseQLError::Validation {
            message: "Invalid credentials".to_string(),
            path: None,
        });
    }

    // Generate JWT token
    let token = generate_token(&user.id)?;

    Ok(AuthPayload {
        token,
        user,
    })
}

pub async fn me(token: &str, db: &Database) -> Result<User> {
    let user_id = validate_token(token)?;
    db.query_user_by_id(&user_id).await?
        .ok_or_else(|| FraiseQLError::Database {
            message: "User not found".to_string(),
            code: None,
        })
}

fn generate_token(user_id: &str) -> Result<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let claims = Claims {
        user_id: user_id.to_string(),
        exp: (now + 86400) as usize, // 24 hours
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret("your-secret-key".as_ref()),
    ).map_err(|e| FraiseQLError::Validation {
        message: format!("Token generation failed: {}", e),
        path: None,
    })
}

fn validate_token(token: &str) -> Result<String> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret("your-secret-key".as_ref()),
        &Validation::default(),
    )
    .map(|data| data.claims.user_id)
    .map_err(|e| FraiseQLError::Validation {
        message: format!("Invalid token: {}", e),
        path: None,
    })
}
```text
<!-- Code example in TEXT -->

### Usage

```graphql
<!-- Code example in GraphQL -->
# Register
mutation {
  register(
    email: "alice@example.com"
    password: "secure-password"
    name: "Alice"
  ) {
    token
    user {
      id
      name
      email
    }
  }
}

# Login
mutation {
  login(
    email: "alice@example.com"
    password: "secure-password"
  ) {
    token
    user {
      id
      name
      email
    }
  }
}

# Get current user (with Authorization header)
query {
  me {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

### Trade-offs & Security

**JWT vs Sessions**:

- JWT: Stateless, scales horizontally, no server storage
- Sessions: Stateful, easier to revoke, more control

**FraiseQL recommends JWT** for simplicity and scalability.

**Security Considerations**:

- ✅ Hash passwords with bcrypt (cost 12+)
- ✅ Use HTTPS only (TLS 1.3+)
- ✅ Store secret key in environment (not git)
- ✅ Set token expiration (24 hours recommended)
- ✅ Refresh tokens for long sessions
- ✅ Validate email format before storing

---

## Pattern 2: Pagination

### Problem

How do I handle large result sets without overwhelming the client or server?

### Solution

Implement cursor-based pagination for efficient data retrieval.

### Schema Definition

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "UserConnection",
      "fields": [
        { "name": "edges", "type": "UserEdge", "nonNull": false },
        { "name": "pageInfo", "type": "PageInfo", "nonNull": true }
      ]
    },
    {
      "name": "UserEdge",
      "fields": [
        { "name": "node", "type": "User", "nonNull": true },
        { "name": "cursor", "type": "String", "nonNull": true }
      ]
    },
    {
      "name": "PageInfo",
      "fields": [
        { "name": "hasNextPage", "type": "Boolean", "nonNull": true },
        { "name": "hasPreviousPage", "type": "Boolean", "nonNull": true },
        { "name": "startCursor", "type": "String", "nonNull": false },
        { "name": "endCursor", "type": "String", "nonNull": false }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": "UserConnection",
      "isList": false,
      "args": [
        { "name": "first", "type": "Int", "nonNull": false },
        { "name": "after", "type": "String", "nonNull": false },
        { "name": "last", "type": "Int", "nonNull": false },
        { "name": "before", "type": "String", "nonNull": false }
      ]
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Implementation

```rust
<!-- Code example in RUST -->
use base64::{Engine, engine::general_purpose};

pub struct PaginationArgs {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub last: Option<i32>,
    pub before: Option<String>,
}

pub struct UserConnection {
    pub edges: Vec<UserEdge>,
    pub page_info: PageInfo,
}

pub struct UserEdge {
    pub node: User,
    pub cursor: String,
}

pub struct PageInfo {
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
}

pub async fn get_users(
    args: PaginationArgs,
    db: &Database,
) -> Result<UserConnection> {
    // Validate arguments
    let first = args.first.unwrap_or(10).min(100); // Default 10, max 100
    if first < 0 {
        return Err(FraiseQLError::Validation {
            message: "first must be positive".to_string(),
            path: Some("first".to_string()),
        });
    }

    // Decode cursor if provided
    let after_offset = if let Some(cursor) = args.after {
        decode_cursor(&cursor)?
    } else {
        0
    };

    // Fetch one extra to determine if there are more results
    let total_fetch = (first + 1) as usize;
    let mut users = db.query_users(after_offset, total_fetch).await?;

    // Determine if there are more pages
    let has_next_page = users.len() > first as usize;
    if has_next_page {
        users.pop(); // Remove the extra item we fetched
    }

    // Create edges with cursors
    let edges: Vec<UserEdge> = users.iter()
        .enumerate()
        .map(|(idx, user)| {
            let cursor_offset = after_offset + idx as i32 + 1;
            UserEdge {
                node: user.clone(),
                cursor: encode_cursor(cursor_offset),
            }
        })
        .collect();

    let start_cursor = edges.first().map(|e| e.cursor.clone());
    let end_cursor = edges.last().map(|e| e.cursor.clone());

    Ok(UserConnection {
        edges,
        page_info: PageInfo {
            has_next_page,
            has_previous_page: after_offset > 0,
            start_cursor,
            end_cursor,
        },
    })
}

fn encode_cursor(offset: i32) -> String {
    general_purpose::STANDARD.encode(offset.to_string())
}

fn decode_cursor(cursor: &str) -> Result<i32> {
    let decoded = general_purpose::STANDARD.decode(cursor)
        .map_err(|_| FraiseQLError::Validation {
            message: "Invalid cursor format".to_string(),
            path: Some("after".to_string()),
        })?;

    let offset_str = String::from_utf8(decoded)
        .map_err(|_| FraiseQLError::Validation {
            message: "Invalid cursor encoding".to_string(),
            path: Some("after".to_string()),
        })?;

    offset_str.parse()
        .map_err(|_| FraiseQLError::Validation {
            message: "Invalid cursor value".to_string(),
            path: Some("after".to_string()),
        })
}
```text
<!-- Code example in TEXT -->

### Usage

```graphql
<!-- Code example in GraphQL -->
query GetFirstPage {
  users(first: 10) {
    edges {
      node {
        id
        name
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}

query GetNextPage {
  users(first: 10, after: "MTA=") {
    edges {
      node {
        id
        name
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```text
<!-- Code example in TEXT -->

### Performance Characteristics

| Scenario | Performance | Notes |
|----------|-------------|-------|
| First page (10 items) | ~5ms | Single database query |
| Mid-range (offset 10k) | ~50ms | Index scan, not full table |
| Last page (offset 1M) | ~500ms | Index scan from end |

**Optimization**:

- ✅ Add database index on creation date
- ✅ Use offset-based cursor for small pages
- ✅ Consider keyset pagination for very large datasets

---

## Pattern 3: Filtering & Search

### Problem

How do I add search and filtering to my GraphQL API?

### Solution

Implement multiple filter types and combine them efficiently.

### Schema Definition

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "UserFilter",
      "fields": [
        { "name": "name", "type": "String", "nonNull": false },
        { "name": "email", "type": "String", "nonNull": false },
        { "name": "createdAfter", "type": "DateTime", "nonNull": false },
        { "name": "createdBefore", "type": "DateTime", "nonNull": false }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": "User",
      "isList": true,
      "args": [
        { "name": "filter", "type": "UserFilter", "nonNull": false },
        { "name": "search", "type": "String", "nonNull": false }
      ]
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Implementation

```rust
<!-- Code example in RUST -->
pub struct UserFilter {
    pub name: Option<String>,
    pub email: Option<String>,
    pub created_after: Option<DateTime>,
    pub created_before: Option<DateTime>,
}

pub async fn get_users(
    filter: Option<UserFilter>,
    search: Option<String>,
    db: &Database,
) -> Result<Vec<User>> {
    let mut query = "SELECT * FROM users WHERE 1=1".to_string();
    let mut params: Vec<String> = Vec::new();

    // Apply filter
    if let Some(f) = filter {
        if let Some(name) = f.name {
            query.push_str(" AND name ILIKE ${}");
            params.push(format!("%{}%", name));
        }

        if let Some(email) = f.email {
            query.push_str(" AND email = ${}");
            params.push(email);
        }

        if let Some(after) = f.created_after {
            query.push_str(" AND created_at >= ${}");
            params.push(after.to_rfc3339());
        }

        if let Some(before) = f.created_before {
            query.push_str(" AND created_at <= ${}");
            params.push(before.to_rfc3339());
        }
    }

    // Apply full-text search
    if let Some(q) = search {
        query.push_str(
            " AND (name ILIKE ${} OR email ILIKE ${})"
        );
        let search_term = format!("%{}%", q);
        params.push(search_term.clone());
        params.push(search_term);
    }

    query.push_str(" ORDER BY created_at DESC LIMIT 100");

    db.query_raw(&query, &params).await
}
```text
<!-- Code example in TEXT -->

### Usage

```graphql
<!-- Code example in GraphQL -->
# Search by name
query {
  users(filter: { name: "alice" }) {
    id
    name
    email
  }
}

# Filter by date range
query {
  users(filter: {
    createdAfter: "2026-01-01T00:00:00Z"
    createdBefore: "2026-01-31T23:59:59Z"
  }) {
    id
    name
    createdAt
  }
}

# Combine filter and search
query {
  users(
    filter: { createdAfter: "2026-01-01T00:00:00Z" }
    search: "alice@example.com"
  ) {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

### Full-Text Search Performance

```sql
<!-- Code example in SQL -->
-- Create index for fast searches
CREATE INDEX idx_users_name_search ON users USING GIN (
  to_tsvector('english', name || ' ' || email)
);
```text
<!-- Code example in TEXT -->

With index:

- Unfiltered search: ~100ms
- Filtered search: ~20ms
- Multiple filters: ~50ms

---

## Pattern 4: Real-Time Updates (Subscriptions)

### Problem

How do I add WebSocket subscriptions for real-time updates?

### Solution

Implement subscriptions using WebSocket protocol.

### Schema Definition

```json
<!-- Code example in JSON -->
{
  "subscriptions": [
    {
      "name": "userCreated",
      "returnType": "User",
      "isList": false,
      "args": []
    },
    {
      "name": "userUpdated",
      "returnType": "User",
      "isList": false,
      "args": [
        { "name": "userId", "type": "ID", "nonNull": true }
      ]
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Implementation

```rust
<!-- Code example in RUST -->
use tokio::sync::mpsc;
use futures_util::{Stream, StreamExt};

pub struct Subscription;

pub async fn user_created(
    publisher: &EventPublisher,
) -> Result<impl Stream<Item = Result<User>>> {
    let (tx, rx) = mpsc::channel(100);

    // Subscribe to user creation events
    publisher.subscribe("user:created", move |user: User| {
        let tx = tx.clone();
        async move {
            let _ = tx.send(Ok(user)).await;
        }
    }).await?;

    Ok(rx.into_stream())
}

pub async fn user_updated(
    user_id: String,
    publisher: &EventPublisher,
) -> Result<impl Stream<Item = Result<User>>> {
    let (tx, rx) = mpsc::channel(100);
    let topic = format!("user:{}:updated", user_id);

    // Subscribe to specific user updates
    publisher.subscribe(&topic, move |user: User| {
        let tx = tx.clone();
        async move {
            let _ = tx.send(Ok(user)).await;
        }
    }).await?;

    Ok(rx.into_stream())
}

// Publish events
pub async fn create_user_and_notify(
    name: String,
    email: String,
    db: &Database,
    publisher: &EventPublisher,
) -> Result<User> {
    let user = db.create_user(&name, &email).await?;

    // Notify all subscribers
    publisher.publish("user:created", user.clone()).await?;

    Ok(user)
}

pub async fn update_user_and_notify(
    user_id: String,
    name: Option<String>,
    db: &Database,
    publisher: &EventPublisher,
) -> Result<User> {
    let user = db.update_user(&user_id, name).await?;

    // Notify subscribers for this user
    let topic = format!("user:{}:updated", user_id);
    publisher.publish(&topic, user.clone()).await?;

    Ok(user)
}
```text
<!-- Code example in TEXT -->

### Usage

```graphql
<!-- Code example in GraphQL -->
# Subscribe to new users
subscription {
  userCreated {
    id
    name
    email
  }
}

# Subscribe to updates for specific user
subscription {
  userUpdated(userId: "123") {
    id
    name
    email
    updatedAt
  }
}
```text
<!-- Code example in TEXT -->

### Scaling Subscriptions

For multi-server deployments, use Redis Pub/Sub:

```rust
<!-- Code example in RUST -->
let publisher = RedisPublisher::new(
    redis_client,
    "FraiseQL:events"
).await?;
```text
<!-- Code example in TEXT -->

---

## Pattern 5: File Uploads

### Problem

How do I handle file uploads in a GraphQL API?

### Solution

Implement file upload handling with S3 storage.

### Schema Definition

```json
<!-- Code example in JSON -->
{
  "types": [
    {
      "name": "Upload",
      "fields": [
        { "name": "filename", "type": "String", "nonNull": true },
        { "name": "mimetype", "type": "String", "nonNull": true },
        { "name": "size", "type": "Int", "nonNull": true }
      ]
    }
  ],
  "mutations": [
    {
      "name": "uploadUserAvatar",
      "args": [
        { "name": "userId", "type": "ID", "nonNull": true },
        { "name": "file", "type": "Upload", "nonNull": true }
      ],
      "returnType": "User",
      "isList": false
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Implementation

```rust
<!-- Code example in RUST -->
pub struct FileUpload {
    pub filename: String,
    pub mimetype: String,
    pub content: Vec<u8>,
}

pub async fn upload_user_avatar(
    user_id: String,
    file: FileUpload,
    s3: &S3Client,
    db: &Database,
) -> Result<User> {
    // Validate file
    if file.content.len() > 5_000_000 {
        return Err(FraiseQLError::Validation {
            message: "File size exceeds 5MB limit".to_string(),
            path: Some("file".to_string()),
        });
    }

    if !["image/jpeg", "image/png", "image/webp"]
        .contains(&file.mimetype.as_str())
    {
        return Err(FraiseQLError::Validation {
            message: "Only JPEG, PNG, or WebP allowed".to_string(),
            path: Some("file".to_string()),
        });
    }

    // Generate unique filename
    let filename = format!(
        "avatars/{}/{}-{}",
        user_id,
        chrono::Utc::now().timestamp(),
        uuid::Uuid::new_v4()
    );

    // Upload to S3
    let url = s3.put_object(
        &filename,
        &file.content,
        &file.mimetype,
    ).await?;

    // Update user avatar URL in database
    let user = db.update_user_avatar(&user_id, &url).await?;

    Ok(user)
}
```text
<!-- Code example in TEXT -->

### Client Usage

```graphql
<!-- Code example in GraphQL -->
mutation UploadAvatar($userId: ID!, $file: Upload!) {
  uploadUserAvatar(userId: $userId, file: $file) {
    id
    name
    avatarUrl
  }
}
```text
<!-- Code example in TEXT -->

JavaScript client:

```javascript
<!-- Code example in JAVASCRIPT -->
const input = document.querySelector('input[type="file"]');
const formData = new FormData();

formData.append('operations', JSON.stringify({
  query: `mutation UploadAvatar($file: Upload!) { ... }`,
  variables: { userId: '123', file: null }
}));

formData.append('map', JSON.stringify({
  0: ['variables.file']
}));

formData.append('0', input.files[0]);

fetch('/graphql', {
  method: 'POST',
  body: formData
});
```text
<!-- Code example in TEXT -->

---

## Pattern 6: Caching

### Problem

How do I cache query results to reduce database load?

### Solution

Implement multi-layer caching strategy.

### Schema Definition (with cache directives)

```json
<!-- Code example in JSON -->
{
  "queries": [
    {
      "name": "user",
      "returnType": "User",
      "isList": false,
      "args": [
        { "name": "id", "type": "ID", "nonNull": true }
      ],
      "cache": {
        "ttl": 300,
        "tags": ["user"]
      }
    },
    {
      "name": "users",
      "returnType": "User",
      "isList": true,
      "args": [],
      "cache": {
        "ttl": 60,
        "tags": ["users"]
      }
    }
  ],
  "mutations": [
    {
      "name": "updateUser",
      "returnType": "User",
      "invalidateTags": ["user", "users"]
    }
  ]
}
```text
<!-- Code example in TEXT -->

### Implementation

```rust
<!-- Code example in RUST -->
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Duration, SystemTime};

pub struct CacheEntry<T> {
    value: T,
    expires_at: SystemTime,
    tags: Vec<String>,
}

pub struct QueryCache<T> {
    data: Arc<RwLock<HashMap<String, CacheEntry<T>>>>,
}

impl<T: Clone> QueryCache<T> {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, key: &str) -> Option<T> {
        let cache = self.data.read().await;

        if let Some(entry) = cache.get(key) {
            if entry.expires_at > SystemTime::now() {
                return Some(entry.value.clone());
            }
        }

        None
    }

    pub async fn set(
        &self,
        key: String,
        value: T,
        ttl: Duration,
        tags: Vec<String>,
    ) {
        let mut cache = self.data.write().await;

        cache.insert(key, CacheEntry {
            value,
            expires_at: SystemTime::now() + ttl,
            tags,
        });
    }

    pub async fn invalidate_by_tag(&self, tag: &str) {
        let mut cache = self.data.write().await;

        cache.retain(|_, entry| {
            !entry.tags.contains(&tag.to_string())
        });
    }
}

pub async fn get_user(
    id: String,
    cache: &QueryCache<User>,
    db: &Database,
) -> Result<User> {
    let cache_key = format!("user:{}", id);

    // Check cache
    if let Some(user) = cache.get(&cache_key).await {
        return Ok(user);
    }

    // Query database
    let user = db.query_user(&id).await?;

    // Store in cache (5 minutes)
    cache.set(
        cache_key,
        user.clone(),
        Duration::from_secs(300),
        vec!["user".to_string(), format!("user:{}", id)],
    ).await;

    Ok(user)
}

pub async fn update_user(
    id: String,
    name: Option<String>,
    cache: &QueryCache<User>,
    db: &Database,
) -> Result<User> {
    // Update in database
    let user = db.update_user(&id, name).await?;

    // Invalidate related caches
    cache.invalidate_by_tag("user").await;
    cache.invalidate_by_tag("users").await;

    Ok(user)
}
```text
<!-- Code example in TEXT -->

### Caching Strategy

**Layer 1: In-Memory Cache**

- Speed: <1ms
- Cost: Memory usage
- Best for: Static queries, user profiles

**Layer 2: Redis Cache**

- Speed: ~5-10ms
- Cost: Redis infrastructure
- Best for: Cross-server consistency

**Layer 3: HTTP Cache Headers**

- Speed: Browser cache
- Cost: Network requests
- Best for: Public data

### Performance Impact

```text
<!-- Code example in TEXT -->
Without cache:

- Query time: 50ms
- Database load: 100 queries/sec

With L1 cache (50% hit rate):

- Query time: 25ms (average)
- Database load: 50 queries/sec
- Reduction: 50%

With L1+L2 cache (80% hit rate):

- Query time: 10ms (average)
- Database load: 20 queries/sec
- Reduction: 80%
```text
<!-- Code example in TEXT -->

---

## Troubleshooting

### "JWT token validation failing: 'Invalid token signature'"

**Cause:** Token signed with different key or issuer mismatch.

**Diagnosis:**

1. Check token issuer: `echo $JWT_ISSUER`
2. Verify public key: Compare with OAuth provider
3. Decode token: `jwt decode $token` (check `iss` claim)

**Solutions:**

- Verify JWT_ISSUER environment variable matches provider
- Ensure public key is current (providers rotate keys)
- Check token expiration: `jq '.exp' token.json`
- Regenerate token if expired

### "Pagination cursor returning empty or wrong records"

**Cause:** Cursor encoding/decoding mismatch or data ordering changed.

**Diagnosis:**

1. Decode cursor: `base64 -d cursor`
2. Verify sort order matches: `SELECT * FROM users ORDER BY created_at, id LIMIT 10;`
3. Check if records were deleted/reordered

**Solutions:**

- Ensure consistent sort order: `ORDER BY created_at DESC, id DESC`
- Don't change sort order mid-pagination
- Use stable cursor (record ID + timestamp)
- Handle deleted records gracefully (skip and get next)

### "Full-text search not finding results"

**Cause:** Index not created or query format wrong.

**Diagnosis:**

1. Check if index exists: `SELECT * FROM pg_indexes WHERE tablename = 'users';`
2. Test search manually: `SELECT * FROM users WHERE to_tsvector(name) @@ to_tsquery('john');`
3. Verify column contains data: `SELECT COUNT(*) FROM users WHERE name IS NOT NULL;`

**Solutions:**

- Create full-text search index: `CREATE INDEX idx_user_search ON users USING GIN(to_tsvector('english', name || ' ' || email));`
- Use query syntax: `&` (AND), `|` (OR), `!` (NOT)
- Index must be functional for performance
- For stemming: Use language-specific dictionary

### "Subscription WebSocket connection drops unexpectedly"

**Cause:** Connection timeout, server restart, or network issue.

**Diagnosis:**

1. Check server logs for connection drops
2. Verify network connection: `ping server`
3. Check WebSocket URL: `wss://...` for production, `ws://...` for local

**Solutions:**

- Implement reconnection logic in client
- Increase connection timeout if needed
- Use persistent connections (TCP keepalive)
- For server restarts: Graceful shutdown closes connections cleanly
- Monitor connection health: Send heartbeats every 30 seconds

### "File upload fails: 'Multipart form data parsing error'"

**Cause:** Request format incorrect or file too large.

**Diagnosis:**

1. Check Content-Type header: Should be `multipart/form-data`
2. Check file size: Compare to server limits
3. Verify field name matches schema

**Solutions:**

- Use correct Content-Type: `multipart/form-data`
- Check max file size setting in FraiseQL.toml
- Ensure file field name matches GraphQL input type
- For large files: Implement chunked upload

### "Cache hit rate is low (<30%)"

**Cause:** Cache key too specific or cache size too small.

**Diagnosis:**

1. Monitor cache metrics: `SELECT hit_rate FROM cache_stats;`
2. Check cache size: `SELECT pg_size_pretty(pg_total_relation_size('cache_table'));`
3. Analyze popular queries: Which queries run most frequently?

**Solutions:**

- Increase cache size: More memory for L1 cache
- Simplify cache key: Make key less dependent on exact values
- Increase TTL for L2 cache: Let results stay cached longer
- Pre-warm cache: Load frequently-accessed data at startup
- For L2 (Redis): Monitor memory usage and eviction policy

### "Real-time subscription updates have latency >2 seconds"

**Cause:** Database polling interval too large or WebSocket overhead.

**Diagnosis:**

1. Check polling interval setting: `[subscriptions] poll_interval_ms = ?`
2. Monitor network latency: `ping subscription_server`
3. Check database query performance: `EXPLAIN ANALYZE SELECT ...;`

**Solutions:**

- Reduce polling interval: 100-500ms is typical
- Use CDC (Change Data Capture) instead of polling for better latency
- Optimize database query (add indexes)
- Ensure WebSocket is directly to server (not through heavy proxy)
- Use batching: Combine multiple changes into single update

### "Pattern implementation doesn't match example - authentication failing"

**Cause:** Environment setup missing or configuration incorrect.

**Diagnosis:**

1. Follow setup guide: [Authentication Setup](../integrations/authentication/README.md)
2. Check environment variables: `env | grep OAUTH`
3. Verify credentials in OAuth provider console

**Solutions:**

- Ensure all prerequisites from guide are met
- Check example matches your language SDK
- Test with curl first before implementation
- Enable debug logging: `RUST_LOG=debug`
- Review Security Checklist for common mistakes

---

## Summary

You now know how to implement:

✅ User authentication with JWT tokens
✅ Cursor-based pagination for large datasets
✅ Filtering and full-text search
✅ Real-time updates with subscriptions
✅ File uploads to cloud storage
✅ Multi-layer caching strategies

## Next Steps

- **Ready to deploy?** → [Deployment Guide](../deployment/guide.md)
- **Need help?** → [Troubleshooting Guide](../TROUBLESHOOTING.md)
- **Want more patterns?** → Explore more guides in the [guides](../guides/) directory

---

## See Also

**Related Guides:**

- **[Authorization Quick Start](./authorization-quick-start.md)** — Field-level RBAC and role-based access control
- **[Testing Strategy](./testing-strategy.md)** — Unit, integration, and end-to-end testing for patterns
- **[Consistency Model](./consistency-model.md)** — Understanding data consistency in federation
- **[Performance Tuning](../operations/performance-tuning-runbook.md)** — Optimizing pattern implementations
- **[Schema Design Best Practices](./schema-design-best-practices.md)** — Designing schemas for common patterns

**Integration Guides:**

- **[Authentication Providers](../integrations/authentication/provider-selection-guide.md)** — Choosing OAuth2/OIDC providers
- **[Federation Guide](../integrations/federation/guide.md)** — Implementing federation patterns
- **[Arrow Flight Quick Start](./arrow-flight-quick-start.md)** — Exporting pattern results as columnar data

**Deployment & Operations:**

- **[Production Deployment](./production-deployment.md)** — Deploying pattern implementations to production
- **[Monitoring & Observability](./monitoring.md)** — Observing pattern behavior in production
- **[Security Deployment Checklist](../guides/production-security-checklist.md)** — Hardening patterns for security

**Troubleshooting:**

- **[Troubleshooting Decision Tree](../guides/troubleshooting-decision-tree.md)** — Route to correct guide for your problem
- **[Troubleshooting Guide](../TROUBLESHOOTING.md)** — FAQ and common solutions

---

**Questions?** See [TROUBLESHOOTING.md](../TROUBLESHOOTING.md) for FAQ and solutions, or open an issue on [GitHub](https://github.com/FraiseQL/FraiseQL-v2).
