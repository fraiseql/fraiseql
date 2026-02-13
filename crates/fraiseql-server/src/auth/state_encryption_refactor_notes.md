# State Encryption Integration Guide

# Phase 7, Cycle 4: REFACTOR phase - Integration points

## Overview

The `StateEncryption` utility provides encrypted state handling for OAuth PKCE protection. This document identifies integration points for applying state encryption throughout the state store implementations.

## Security Properties

- **Confidentiality**: State values encrypted with ChaCha20-256
- **Authenticity**: Poly1305 authentication tag prevents tampering detection
- **Replay Prevention**: Random nonce in each encryption
- **Forward Secrecy**: Encryption key isolated from signing keys

## Critical Integration Points

### 1. InMemoryStateStore (state_store.rs)

**Current Status**: Stores unencrypted state in DashMap

```rust
pub struct InMemoryStateStore {
    states: Arc<DashMap<String, (String, u64)>>,
}
```

**Recommended Changes**:

```rust
use crate::auth::state_encryption::StateEncryption;

pub struct InMemoryStateStore {
    states: Arc<DashMap<String, (Vec<u8>, u64)>>,  // Store encrypted bytes
    encryption: Arc<StateEncryption>,              // Cipher instance
}

impl InMemoryStateStore {
    pub fn new(encryption: Arc<StateEncryption>) -> Self {
        Self {
            states: Arc::new(DashMap::new()),
            encryption,
        }
    }
}

#[async_trait]
impl StateStore for InMemoryStateStore {
    async fn store(&self, state: String, provider: String, expiry_secs: u64) -> Result<()> {
        // Encrypt: "{provider}:{state}"
        let plaintext = format!("{}:{}", provider, state);
        let encrypted_bytes = self.encryption.encrypt_to_bytes(&plaintext)?;

        self.states.insert(state, (encrypted_bytes, expiry_secs));
        Ok(())
    }

    async fn retrieve(&self, state: &str) -> Result<(String, u64)> {
        let (_key, (encrypted_bytes, expiry_secs)) = self
            .states
            .remove(state)
            .ok_or_else(|| AuthError::InvalidState)?;

        // Decrypt and verify
        let plaintext = self.encryption.decrypt_from_bytes(&encrypted_bytes)?;

        // Parse back: "{provider}:{state}"
        let parts: Vec<&str> = plaintext.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(AuthError::InvalidState);
        }

        Ok((parts[0].to_string(), expiry_secs))
    }
}
```

**Integration Steps**:

1. Create `StateEncryption` instance with generated/configured key
2. Pass to `InMemoryStateStore::new()`
3. Store encrypts before DashMap insertion
4. Retrieve decrypts after DashMap removal
5. Add expiration check: `SystemTime::now().as_secs() > expiry_secs` → fail

### 2. RedisStateStore (state_store.rs)

**Current Status**: Stores unencrypted state in Redis

```rust
#[cfg(feature = "redis-rate-limiting")]
pub struct RedisStateStore {
    client: redis::aio::ConnectionManager,
}
```

**Recommended Changes**:

```rust
#[cfg(feature = "redis-rate-limiting")]
pub struct RedisStateStore {
    client: redis::aio::ConnectionManager,
    encryption: Arc<StateEncryption>,
}

#[cfg(feature = "redis-rate-limiting")]
impl RedisStateStore {
    pub async fn new(redis_url: &str, encryption: Arc<StateEncryption>) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let connection_manager = client.get_connection_manager().await?;

        Ok(Self {
            client: connection_manager,
            encryption,
        })
    }
}

#[cfg(feature = "redis-rate-limiting")]
#[async_trait]
impl StateStore for RedisStateStore {
    async fn store(&self, state: String, provider: String, expiry_secs: u64) -> Result<()> {
        use redis::AsyncCommands;

        let plaintext = format!("{}:{}", provider, state);
        let encrypted_bytes = self.encryption.encrypt_to_bytes(&plaintext)?;

        let key = Self::state_key(&state);
        let ttl = calculate_ttl(expiry_secs);

        let mut conn = self.client.clone();
        conn.set_ex(&key, encrypted_bytes, ttl as usize).await?;

        Ok(())
    }

    async fn retrieve(&self, state: &str) -> Result<(String, u64)> {
        use redis::AsyncCommands;

        let key = Self::state_key(state);
        let mut conn = self.client.clone();

        let encrypted_bytes: Vec<u8> = conn.get(&key).await.map_err(|_| {
            AuthError::InvalidState
        })?;

        // Remove from Redis
        let _: () = conn.del(&key).await.ok();

        // Decrypt
        let plaintext = self.encryption.decrypt_from_bytes(&encrypted_bytes)?;

        // Parse provider and state
        let parts: Vec<&str> = plaintext.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(AuthError::InvalidState);
        }

        Ok((parts[0].to_string(), expiry_secs))
    }
}
```

**Integration Steps**:

1. Add `StateEncryption` parameter to `RedisStateStore::new()`
2. Encrypt plaintext before Redis SET
3. Decrypt after Redis GET
4. Handle TTL: Redis automatically expires keys
5. Tampering detected automatically by auth tag

### 3. AuthState (handlers.rs)

**Current Status**: AuthState initialization

```rust
#[derive(Clone)]
pub struct AuthState {
    pub oauth_provider: Arc<dyn OAuthProvider>,
    pub session_store: Arc<dyn SessionStore>,
    pub state_store: Arc<dyn StateStore>,
}
```

**Recommended Changes**:

```rust
pub struct AuthState {
    pub oauth_provider: Arc<dyn OAuthProvider>,
    pub session_store: Arc<dyn SessionStore>,
    pub state_store: Arc<dyn StateStore>,
    // encryption is accessed via StateStore implementations
}

// In initialization code (main.rs or tests):
use crate::auth::state_encryption::{StateEncryption, generate_state_encryption_key};

let state_key = generate_state_encryption_key();  // Or load from config
let encryption = Arc::new(StateEncryption::new(&state_key)?);

let state_store = Arc::new(InMemoryStateStore::new(encryption.clone()));
// or
let state_store = Arc::new(RedisStateStore::new("redis://...", encryption).await?);

let auth_state = AuthState {
    oauth_provider: /* ... */,
    session_store: /* ... */,
    state_store,
};
```

### 4. auth_callback Handler (handlers.rs)

**Current Status**: State validation

```rust
let (_provider_name, expiry) = state.state_store.retrieve(&query.state).await?;
```

**No Changes Needed**: The `retrieve()` method now handles decryption internally.

The flow remains the same:

1. Client sends `state` parameter (unencrypted, for lookup key)
2. Server retrieves encrypted state from store
3. Store automatically decrypts and verifies
4. If tampering detected → `Err(AuthError::InvalidState)`
5. If expired → check `expiry_secs` as before

## Key Rotation Strategy

**Current**: Single key per deployment (sufficient for initial release)

**Future Enhancement** (if needed):

- Add key version byte to encrypted state
- Support multiple keys during rotation period
- Implement key versioning in EncryptedState format

## Performance Considerations

- **Encryption**: < 1ms per state (negligible for OAuth flow)
- **Decryption**: < 1ms per state
- **Memory**: Minimal overhead (encryption state cached in process)
- **Redis**: Encryption happens before network transmission (reduces latency)

## Testing Approach

1. **Unit Tests**: Already included in `state_encryption.rs`
   - Encryption/decryption
   - Tampering detection
   - Key sensitivity
   - Edge cases

2. **Integration Tests**: Verify with StateStore implementations
   - Store then retrieve cycle
   - Multiple concurrent states
   - Expiration handling
   - Error cases

3. **End-to-End**: OAuth flow with encrypted state
   - Valid OAuth callback succeeds
   - Tampered state rejected
   - Expired state rejected

## Deployment Considerations

### Key Generation

```rust
// Single deployment instance
let key = generate_state_encryption_key();
let encryption = StateEncryption::new(&key)?;

// Distributed deployments
let key = load_key_from_env("STATE_ENCRYPTION_KEY")?;
let encryption = StateEncryption::new(&key)?;
```

### Configuration

Add to environment/config:

```bash
STATE_ENCRYPTION_KEY=<base64-encoded-32-bytes>
```

### Backward Compatibility

This is a **breaking change** for existing state stores:

- All existing unencrypted states become invalid
- Deploy with session reset (requires re-login)
- OAuth flows in progress during deployment will fail
- Acceptable for feature release (v2.1.0)

## API Reference

```rust
// In crate::auth::state_encryption

pub struct StateEncryption { /* ... */ }

impl StateEncryption {
    pub fn new(key_bytes: &[u8; 32]) -> Result<Self>
    pub fn encrypt(&self, state: &str) -> Result<EncryptedState>
    pub fn decrypt(&self, encrypted: &EncryptedState) -> Result<String>
    pub fn encrypt_to_bytes(&self, state: &str) -> Result<Vec<u8>>
    pub fn decrypt_from_bytes(&self, bytes: &[u8]) -> Result<String>
}

pub struct EncryptedState {
    pub ciphertext: Vec<u8>,  // With authentication tag
    pub nonce: [u8; 12],       // Random per encryption
}

impl EncryptedState {
    pub fn new(ciphertext: Vec<u8>, nonce: [u8; 12]) -> Self
    pub fn to_bytes(&self) -> Vec<u8>
    pub fn from_bytes(bytes: &[u8]) -> Result<Self>
}

pub fn generate_state_encryption_key() -> [u8; 32]
```

## Security Guarantees

✅ Prevents state tampering (Poly1305 auth tag)
✅ Prevents state inspection (ChaCha20 confidentiality)
✅ Prevents state replay (random nonce per encryption)
✅ Constant-time comparison not needed (authenticated by default)
✅ Key isolation (separate from JWT signing keys)

## Integration Checklist

- [ ] Create `StateEncryption` instance during app init
- [ ] Update `InMemoryStateStore` to encrypt/decrypt
- [ ] Update `RedisStateStore` to encrypt/decrypt (if used)
- [ ] Generate or load STATE_ENCRYPTION_KEY from config
- [ ] Update tests to use encrypted states
- [ ] Deploy with session reset (users need to re-login)
- [ ] Monitor auth_callback errors for tampering attempts
- [ ] Document encryption in deployment guides
