# Implementing a Custom SessionStore

FraiseQL's authentication system is designed to be extensible. This guide shows how to implement a custom `SessionStore` for your preferred storage backend.

## Prerequisites

**Required Knowledge:**

- Rust language fundamentals (traits, async/await, error handling)
- FraiseQL authentication architecture and SessionStore trait
- Session management concepts and token storage
- Async Rust and async-trait macro usage
- Your chosen backend technology (Redis, DynamoDB, MongoDB, etc.)
- Hash algorithms and token security practices
- Error handling in Rust with Result/Error types

**Required Software:**

- Rust 1.75+ with full toolchain
- Cargo (included with Rust)
- Your backend SDK (redis-rs, aws-SDK-dynamodb, mongodb Rust driver, etc.)
- A text editor or IDE with Rust support
- Git for version control

**Required Infrastructure:**

- FraiseQL source code (for SessionStore trait definition)
- Your chosen session storage backend:
  - Redis: Redis 6+ server
  - DynamoDB: AWS account with DynamoDB access
  - MongoDB: MongoDB 4.0+ instance
  - PostgreSQL: PostgreSQL 12+
- Test database for validation
- Build environment with internet access for dependencies

**Optional but Recommended:**

- Redis CLI or similar for testing backend connectivity
- AWS CLI or cloud provider CLI tools
- Docker for running test backends (redis, mongo, dynamodb-local)
- Test framework (tokio test harness)
- Example implementations in other languages for reference

**Time Estimate:** 1-2 hours to implement basic SessionStore, 3-4 hours to add comprehensive error handling and testing

## Overview

The `SessionStore` trait defines four core methods:

```rust
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair>;
    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData>;
    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()>;
    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()>;
}
```

## Reference Implementations

FraiseQL includes two reference implementations:

1. **PostgresSessionStore** - For relational databases
2. **InMemorySessionStore** - For testing

## Example 1: Redis Session Store

Here's how to implement a Redis-backed session store:

```rust
use async_trait::async_trait;
use fraiseql_server::auth::{SessionStore, SessionData, TokenPair, Result, AuthError};
use fraiseql_server::auth::session::{generate_refresh_token, hash_token};
use redis::{Client, Commands};
use std::sync::Arc;

pub struct RedisSessionStore {
    client: redis::Client,
}

impl RedisSessionStore {
    pub fn new(client: redis::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session_data = serde_json::json!({
            "user_id": user_id,
            "issued_at": now,
            "expires_at": expires_at,
            "refresh_token_hash": refresh_token_hash,
        });

        let mut conn = self.client.get_connection()
            .map_err(|e| AuthError::SessionError {
                message: format!("Redis connection error: {}", e),
            })?;

        // Store in Redis with expiry
        let ttl = (expires_at - now) as usize;
        conn.set_ex(
            format!("session:{}", refresh_token_hash),
            session_data.to_string(),
            ttl,
        ).map_err(|e| AuthError::SessionError {
            message: format!("Failed to create session: {}", e),
        })?;

        // Also store user → sessions mapping for revoke_all
        conn.sadd(
            format!("user_sessions:{}", user_id),
            refresh_token_hash.clone(),
        ).map_err(|e| AuthError::SessionError {
            message: format!("Failed to track session: {}", e),
        })?;

        let expires_in = expires_at.saturating_sub(now);
        let access_token = format!("access_token_{}", uuid::Uuid::new_v4());

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in,
        })
    }

    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData> {
        let mut conn = self.client.get_connection()
            .map_err(|e| AuthError::SessionError {
                message: format!("Redis connection error: {}", e),
            })?;

        let session_str: String = conn.get(format!("session:{}", refresh_token_hash))
            .map_err(|_| AuthError::TokenNotFound)?;

        let session: SessionData = serde_json::from_str(&session_str)
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to parse session: {}", e),
            })?;

        Ok(session)
    }

    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()> {
        let mut conn = self.client.get_connection()
            .map_err(|e| AuthError::SessionError {
                message: format!("Redis connection error: {}", e),
            })?;

        // Get session to find user_id
        let session_str: String = conn.get(format!("session:{}", refresh_token_hash))
            .map_err(|_| AuthError::SessionError {
                message: "Session not found".to_string(),
            })?;

        let session: SessionData = serde_json::from_str(&session_str)
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to parse session: {}", e),
            })?;

        // Delete session and user mapping
        conn.del(format!("session:{}", refresh_token_hash))
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to revoke session: {}", e),
            })?;

        conn.srem(
            format!("user_sessions:{}", session.user_id),
            refresh_token_hash,
        ).map_err(|e| AuthError::SessionError {
            message: format!("Failed to update session tracking: {}", e),
        })?;

        Ok(())
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()> {
        let mut conn = self.client.get_connection()
            .map_err(|e| AuthError::SessionError {
                message: format!("Redis connection error: {}", e),
            })?;

        // Get all session hashes for this user
        let hashes: Vec<String> = conn.smembers(format!("user_sessions:{}", user_id))
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to get sessions: {}", e),
            })?;

        // Delete each session
        for hash in hashes {
            let _: () = conn.del(format!("session:{}", hash))
                .map_err(|e| AuthError::SessionError {
                    message: format!("Failed to revoke session: {}", e),
                })?;
        }

        // Delete the user's session set
        let _: () = conn.del(format!("user_sessions:{}", user_id))
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to clean up sessions: {}", e),
            })?;

        Ok(())
    }
}
```

### Using Redis Session Store

```rust
use redis::Client;
use fraiseql_server::auth::AuthState;

let redis_client = Client::open("redis://127.0.0.1/")?;
let session_store = Arc::new(RedisSessionStore::new(redis_client));

let auth_state = AuthState {
    oauth_provider,
    session_store,
    state_store: Arc::new(dashmap::DashMap::new()),
};
```

## Example 2: DynamoDB Session Store

For AWS DynamoDB:

```rust
use async_trait::async_trait;
use fraiseql_server::auth::{SessionStore, SessionData, TokenPair, Result, AuthError};
use fraiseql_server::auth::session::{generate_refresh_token, hash_token};
use aws_sdk_dynamodb::types::AttributeValue;
use std::sync::Arc;

pub struct DynamoDbSessionStore {
    client: aws_sdk_dynamodb::Client,
    table_name: String,
}

impl DynamoDbSessionStore {
    pub fn new(client: aws_sdk_dynamodb::Client, table_name: String) -> Self {
        Self { client, table_name }
    }

    pub async fn init(&self) -> Result<()> {
        // Create table if not exists
        // (similar to PostgreSQL init)
        Ok(())
    }
}

#[async_trait]
impl SessionStore for DynamoDbSessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.client
            .put_item()
            .table_name(&self.table_name)
            .item("refresh_token_hash", AttributeValue::S(refresh_token_hash.clone()))
            .item("user_id", AttributeValue::S(user_id.to_string()))
            .item("issued_at", AttributeValue::N(now.to_string()))
            .item("expires_at", AttributeValue::N(expires_at.to_string()))
            .item("ttl", AttributeValue::N(expires_at.to_string())) // For TTL
            .send()
            .await
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to create session: {}", e),
            })?;

        let expires_in = expires_at.saturating_sub(now);
        let access_token = format!("access_token_{}", uuid::Uuid::new_v4());

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in,
        })
    }

    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData> {
        let response = self.client
            .get_item()
            .table_name(&self.table_name)
            .key("refresh_token_hash", AttributeValue::S(refresh_token_hash.to_string()))
            .send()
            .await
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to get session: {}", e),
            })?;

        let item = response.item().ok_or(AuthError::TokenNotFound)?;

        let user_id = item.get("user_id")
            .and_then(|v| v.as_s().ok())
            .ok_or(AuthError::TokenNotFound)?
            .clone();

        let issued_at = item.get("issued_at")
            .and_then(|v| v.as_n().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or(AuthError::TokenNotFound)?;

        let expires_at = item.get("expires_at")
            .and_then(|v| v.as_n().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or(AuthError::TokenNotFound)?;

        Ok(SessionData {
            user_id,
            issued_at,
            expires_at,
            refresh_token_hash: refresh_token_hash.to_string(),
        })
    }

    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()> {
        self.client
            .delete_item()
            .table_name(&self.table_name)
            .key("refresh_token_hash", AttributeValue::S(refresh_token_hash.to_string()))
            .send()
            .await
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to revoke session: {}", e),
            })?;

        Ok(())
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()> {
        // Query all sessions for user
        let response = self.client
            .query()
            .table_name(&self.table_name)
            .index_name("user_id-index")
            .key_condition_expression("user_id = :uid")
            .expression_attribute_values(":uid", AttributeValue::S(user_id.to_string()))
            .send()
            .await
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to query sessions: {}", e),
            })?;

        // Delete each session
        for item in response.items().unwrap_or(&vec![]) {
            if let Some(hash_val) = item.get("refresh_token_hash") {
                if let Ok(hash) = hash_val.as_s() {
                    let _ = self.revoke_session(hash).await;
                }
            }
        }

        Ok(())
    }
}
```

## Example 3: MongoDB Session Store

For MongoDB:

```rust
use async_trait::async_trait;
use fraiseql_server::auth::{SessionStore, SessionData, TokenPair, Result, AuthError};
use fraiseql_server::auth::session::{generate_refresh_token, hash_token};
use mongodb::{Client, bson::doc};
use std::sync::Arc;

pub struct MongoDbSessionStore {
    client: mongodb::Client,
    db_name: String,
}

impl MongoDbSessionStore {
    pub fn new(client: mongodb::Client, db_name: String) -> Self {
        Self { client, db_name }
    }

    pub async fn init(&self) -> Result<()> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<SessionData>("sessions");

        collection.create_index(
            mongodb::IndexModel::builder()
                .keys(doc! { "expires_at": 1 })
                .options(mongodb::options::IndexOptions::builder()
                    .expire_after(std::time::Duration::from_secs(0))
                    .build())
                .build(),
            None,
        ).await.map_err(|e| AuthError::SessionError {
            message: format!("Failed to create index: {}", e),
        })?;

        Ok(())
    }
}

#[async_trait]
impl SessionStore for MongoDbSessionStore {
    async fn create_session(&self, user_id: &str, expires_at: u64) -> Result<TokenPair> {
        let refresh_token = generate_refresh_token();
        let refresh_token_hash = hash_token(&refresh_token);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = SessionData {
            user_id: user_id.to_string(),
            issued_at: now,
            expires_at,
            refresh_token_hash: refresh_token_hash.clone(),
        };

        let db = self.client.database(&self.db_name);
        let collection = db.collection("sessions");

        collection.insert_one(session, None).await
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to create session: {}", e),
            })?;

        let expires_in = expires_at.saturating_sub(now);
        let access_token = format!("access_token_{}", uuid::Uuid::new_v4());

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in,
        })
    }

    async fn get_session(&self, refresh_token_hash: &str) -> Result<SessionData> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<SessionData>("sessions");

        collection.find_one(
            doc! { "refresh_token_hash": refresh_token_hash },
            None
        ).await
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to get session: {}", e),
            })?
            .ok_or(AuthError::TokenNotFound)
    }

    async fn revoke_session(&self, refresh_token_hash: &str) -> Result<()> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<SessionData>("sessions");

        collection.delete_one(
            doc! { "refresh_token_hash": refresh_token_hash },
            None
        ).await
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to revoke session: {}", e),
            })?;

        Ok(())
    }

    async fn revoke_all_sessions(&self, user_id: &str) -> Result<()> {
        let db = self.client.database(&self.db_name);
        let collection = db.collection::<SessionData>("sessions");

        collection.delete_many(
            doc! { "user_id": user_id },
            None
        ).await
            .map_err(|e| AuthError::SessionError {
                message: format!("Failed to revoke all sessions: {}", e),
            })?;

        Ok(())
    }
}
```

## Best Practices

1. **Always hash the refresh token** before storing (using `hash_token()`)
2. **Store issued_at and expires_at** as Unix timestamps (u64)
3. **Return meaningful error messages** (helps with debugging)
4. **Support concurrent access** (use Arc, thread-safe types)
5. **Index by user_id** for efficient `revoke_all_sessions`
6. **Index by refresh_token_hash** for fast lookups
7. **Set TTL/expiry** at the database level if supported
8. **Test with multiple concurrent sessions** for the same user
9. **Handle database connection failures** gracefully
10. **Document your implementation** with examples

## Testing Your Implementation

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_session() {
        let store = YourSessionStore::new(/* ... */);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tokens = store.create_session("user123", now + 3600).await.unwrap();
        let hash = fraiseql_server::auth::session::hash_token(&tokens.refresh_token);

        let session = store.get_session(&hash).await.unwrap();
        assert_eq!(session.user_id, "user123");
    }

    #[tokio::test]
    async fn test_revoke_session() {
        let store = YourSessionStore::new(/* ... */);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tokens = store.create_session("user123", now + 3600).await.unwrap();
        let hash = fraiseql_server::auth::session::hash_token(&tokens.refresh_token);

        assert!(store.revoke_session(&hash).await.is_ok());
        assert!(store.get_session(&hash).await.is_err());
    }

    #[tokio::test]
    async fn test_revoke_all_sessions() {
        let store = YourSessionStore::new(/* ... */);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let tokens1 = store.create_session("user123", now + 3600).await.unwrap();
        let tokens2 = store.create_session("user123", now + 3600).await.unwrap();

        assert!(store.revoke_all_sessions("user123").await.is_ok());

        let hash1 = fraiseql_server::auth::session::hash_token(&tokens1.refresh_token);
        let hash2 = fraiseql_server::auth::session::hash_token(&tokens2.refresh_token);

        assert!(store.get_session(&hash1).await.is_err());
        assert!(store.get_session(&hash2).await.is_err());
    }
}
```

## See Also

- [API Reference](./API-REFERENCE.md)

---

**Next Step**: Implement your session store and pass it to `AuthState`.
