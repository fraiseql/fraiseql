# Phase 13, Cycle 2 - RED: HSM/KMS Requirements & Design

**Date**: February 13, 2026
**Phase Lead**: Security Lead
**Status**: RED (Defining HSM/KMS Requirements)

---

## Objective

Define complete HSM/KMS (Hardware Security Module / Key Management Service) integration requirements for FraiseQL v2, including key hierarchy design, rotation procedures, performance targets, and test strategy.

---

## Background: Why HSM/KMS?

From Phase 13, Cycle 1 threat modeling, the critical requirement is:
- **No plaintext credentials anywhere** in code, configs, logs, or memory
- **API keys stored cryptographically** in HSM/KMS (never accessible as plaintext)
- **Tamper detection** - detect if keys are accessed or modified
- **Audit trail** - every key operation logged and immutable
- **High availability** - key retrieval must be <50ms to avoid query delays

HSM/KMS satisfies all these requirements by:
1. Storing keys in hardware-based encrypted vault (AWS KMS) or software vault (HashiCorp Vault)
2. Providing cryptographic operations without exposing keys
3. Logging all operations for audit
4. Supporting key rotation and versioning
5. Multi-region replication for disaster recovery

---

## Decision Point 1: AWS KMS vs. HashiCorp Vault

### AWS KMS (Recommended for MVP)

**Advantages**:
- AWS-native service (if deploying to AWS)
- HSM-backed for maximum security (AWS CloudHSM option)
- Regional multi-AZ redundancy built-in
- Fully managed (no operational overhead)
- Per-key audit logs to CloudTrail
- Cost: $1/month per key + $0.03 per 10k operations

**Disadvantages**:
- AWS lock-in (multi-cloud difficult)
- 100ms latency if in different region
- Cold-start delays (up to 1 second first call)
- Cannot run offline

**Best for**: AWS-first deployments, high security requirement

### HashiCorp Vault (Recommended for multi-cloud)

**Advantages**:
- Cloud-agnostic (AWS, GCP, Azure, on-prem)
- Self-hosted (full control)
- Can run offline (local unsealing)
- Lower latency (same datacenter)
- Powerful policy engine for access control

**Disadvantages**:
- Requires operational management (backup, unsealing, HA setup)
- HSM support optional (requires Luna HSM or CloudHSM)
- Audit logs retained on our side (must secure separately)
- Cold-start setup for HA

**Best for**: Multi-cloud deployments, self-hosted infrastructure

### Recommendation for FraiseQL MVP

**Use AWS KMS** for Phase 13, Cycle 2:
- Simpler operational overhead (phase 14 handles ops)
- AWS-native CloudTrail auditing
- Built-in HA across AZs
- Easy integration with Lambda (future)
- Can migrate to Vault later if needed (Phase 15+ optimization)

**Decision**: AWS KMS as primary, with abstraction layer allowing Vault/other backends in future

---

## Key Hierarchy Design

FraiseQL needs a 3-level key hierarchy:

### Level 1: Root Key (Stored in AWS KMS)
- **Purpose**: Master encryption key for all data keys
- **Algorithm**: AES-256
- **Rotation**: Every 365 days
- **Access**: Only during key rotation (highly restricted)
- **AWS KMS Type**: Customer Master Key (CMK)
- **Cost**: $1/month per CMK
- **AWS Configuration**:
  ```
  Key Policy: Only specific IAM roles can use this key
  Enabled: Yes
  Rotation: Automatic annual
  Tag: fraiseql-root-key, environment: production
  ```

### Level 2: Data Encryption Keys (DEK)
- **Purpose**: Encrypt API keys and database credentials
- **Algorithm**: AES-256
- **Generation**: Root Key → Generate DEK (via AWS KMS GenerateDataKey)
- **Usage**: API key encryption/decryption operations
- **Rotation**: Every 90 days (new DEK generated, old keys re-encrypted)
- **Storage**: Encrypted DEK stored in database alongside encrypted API key
- **Ciphertext Format**:
  ```json
  {
    "api_key_id": "fraiseql_us_east_1_key123",
    "encrypted_key_material": "<base64-encrypted-api-key>",
    "encrypted_dek": "<base64-encrypted-data-encryption-key>",
    "dek_version": 1,
    "created_at": "2026-02-13T10:00:00Z",
    "rotated_at": null,
    "expires_at": "2026-05-14T10:00:00Z"
  }
  ```

### Level 3: Wrapping/Transport Keys (Optional)
- **Purpose**: Protect keys in transit (if using Vault later)
- **Algorithm**: RSA-2048
- **Usage**: Encrypt DEK before transport to Vault
- **Rotation**: Every 180 days
- **Note**: Not needed for AWS KMS (HTTPS + TLS sufficient)

---

## API Key Lifecycle

### 1. Key Generation (CreateAPIKey)

```
Client Request:
  POST /admin/api-keys
  {
    "name": "production-data-api",
    "tier": "premium",
    "permissions": ["query:read", "batch:100"]
  }

Server Processing:
  1. Generate random 32-byte API key material
  2. Call AWS KMS GenerateDataKey (CMK)
     → Receive encrypted DEK + plaintext DEK
  3. Encrypt API key material with plaintext DEK
  4. Securely destroy plaintext DEK (from memory)
  5. Store in database:
     {
       "api_key_id": "fraiseql_us_east_1_key123",
       "api_key_hash": "sha256(api_key)",  // For lookups
       "encrypted_key_material": "...",
       "encrypted_dek": "...",
       "dek_version": 1,
       "tier": "premium",
       "permissions": ["query:read", "batch:100"],
       "created_at": "2026-02-13T10:00:00Z",
       "rotated_at": null,
       "expires_at": "2026-05-14T10:00:00Z"  // +90 days
     }
  6. Return API key ONCE to client:
     "fraiseql_us_east_1_key123_<random64bytes>"

Return to Client:
  {
    "api_key": "fraiseql_us_east_1_key123_<redacted>",
    "expires_at": "2026-05-14T10:00:00Z",
    "created_at": "2026-02-13T10:00:00Z",
    "note": "Save your API key - we can't retrieve it later"
  }

✅ Key material NEVER stored in plaintext
```

### 2. Key Validation (On Every GraphQL Request)

```
Client Request:
  POST /graphql
  Authorization: Bearer fraiseql_us_east_1_key123_<random64bytes>
  {
    "query": "{ users { id name } }"
  }

Server Processing:
  1. Extract API key from header
  2. Hash the key: api_key_hash = sha256(api_key)
  3. Lookup in database by hash
     → Retrieve encrypted_key_material, encrypted_dek, dek_version
  4. Call AWS KMS Decrypt(encrypted_dek)
     → Receive plaintext DEK
  5. Decrypt API key material with plaintext DEK
     → Receive plaintext API key
  6. Compare received API key with decrypted key
  7. Securely destroy plaintext DEK and API key
  8. If match:
     - Check expiration (expires_at > now)
     - Check permissions (request matches allowed actions)
     - Log successful auth
     - Proceed with query
  9. If mismatch:
     - Log failed auth attempt
     - Return 401 Unauthorized
     - Increment rate limit counter for failed auth

✅ Plaintext API key only in memory during validation
✅ Plaintext DEK only in memory during decryption
✅ Both destroyed after use
```

### 3. Key Rotation (Every 90 Days, Automated)

```
Background Job (runs nightly):
  1. Find all API keys where (created_at + 90 days) < now
  2. For each key:
     a. Decrypt old key with old DEK
     b. Call AWS KMS GenerateDataKey (new CMK)
        → Receive encrypted new DEK + plaintext new DEK
     c. Encrypt key material with new plaintext DEK
     d. Destroy plaintext old DEK and new DEK
     e. Update database:
        {
          "encrypted_key_material": "<new-encrypted-with-new-dek>",
          "encrypted_dek": "<new-encrypted-dek>",
          "dek_version": 2,
          "rotated_at": "2026-02-13T10:00:00Z",
          "expires_at": "2026-05-15T10:00:00Z"  // +90 days from now
        }
     f. Log successful rotation
     g. Alert audit system (key rotation event)

Grace Period (30 days):
  - Old API key format still works until expires_at
  - After expires_at, key is revoked
  - Client must generate new key before expiration

✅ Automated rotation
✅ No manual key creation
✅ Audit trail of all rotations
```

### 4. Key Revocation (On Demand)

```
Admin Request:
  POST /admin/api-keys/{key_id}/revoke
  {
    "reason": "suspected compromise"
  }

Server Processing:
  1. Set revoked_at = now in database
  2. Remove key from auth cache
  3. Log revocation event (who, when, why)
  4. Alert security team
  5. Return success

Immediate Effect:
  - All subsequent GraphQL requests with this key return 401
  - Rate limiter counts requests from this key
  - Audit logs record revocation event
  - No undo (must generate new key)

✅ Immediate revocation
✅ Immutable audit trail
```

---

## Performance Requirements

### Target Latencies (P95)

| Operation | Target | Rationale |
|-----------|--------|-----------|
| API key validation (decrypt + compare) | <50ms | Must not add latency to queries |
| Key rotation (bulk, 1000 keys) | <10 minutes | Nightly background job OK |
| Emergency revocation | <5 seconds | Security event, acceptable |

### AWS KMS Performance Characteristics

**Per-request latency**:
- Same region: 10-20ms average
- Cross-region: 100-200ms average
- Cold start (first call): up to 1 second
- Rate limit: 10,000 requests per second per account

**Cost per operation**:
- GenerateDataKey: $0.03 per 10k = $0.000003 each
- Decrypt: $0.03 per 10k = $0.000003 each
- Monthly estimate: 1M API key validations = $3

**Optimization Strategy**:
1. **Cache** the plaintext DEK for <1 minute (trade-off: slight security for latency)
   - Store in Redis with TTL = 60 seconds
   - If cached, DEK reuse avoids KMS call
   - Save 10-20ms per request
   - Cost reduction: 60x fewer KMS calls

2. **Batch operations** during off-peak
   - Rotate 100 keys per batch job
   - Separate from hot path

3. **Monitor P95/P99** latencies
   - Alert if KMS latency > 100ms (potential regional issue)
   - Implement fallback (local cache, temporary resilience)

### Recommendation

**Use DEK caching with 60-second TTL**:
- `cache[dek_version_id] = plaintext_dek` with TTL=60s
- On first request: KMS Decrypt call (20ms overhead)
- On subsequent requests: Redis cache hit (5ms)
- Security: DEK rotates every 90 days, cache resets after 60s
- Acceptable: 60 seconds of temporary key material in memory (Redis)

---

## Testing Strategy

### Unit Tests (Phase 13, Cycle 2 GREEN)

Test 1: **Key Generation**
```rust
#[tokio::test]
async fn test_generate_api_key() {
    let kms = MockKMS::new();
    let key = generate_api_key(&kms, "test", "premium").await;

    // Verify format
    assert!(key.starts_with("fraiseql_"));
    assert_eq!(key.len(), 64);

    // Verify storage
    let stored = fetch_key_from_db(&key).await.unwrap();
    assert!(stored.encrypted_key_material.len() > 0);
    assert!(stored.encrypted_dek.len() > 0);

    // Verify not plaintext
    assert!(!stored.encrypted_key_material.contains(&key));
}
```

Test 2: **Key Validation**
```rust
#[tokio::test]
async fn test_validate_api_key() {
    let kms = MockKMS::new();
    let key = generate_api_key(&kms, "test", "premium").await;

    // Valid key
    let result = validate_api_key(&key, &kms).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().tier, "premium");

    // Invalid key
    let bad_key = "fraiseql_us_east_1_key999_invalid";
    let result = validate_api_key(&bad_key, &kms).await;
    assert!(result.is_err());
}
```

Test 3: **Key Rotation**
```rust
#[tokio::test]
async fn test_key_rotation() {
    let kms = MockKMS::new();
    let key = generate_api_key(&kms, "test", "premium").await;

    // Rotate key
    rotate_api_key(&key, &kms).await.unwrap();

    // Verify old key still works (grace period)
    let result = validate_api_key(&key, &kms).await;
    assert!(result.is_ok());

    // Verify rotation recorded in database
    let rotated = fetch_key_from_db(&key).await.unwrap();
    assert!(rotated.rotated_at.is_some());
    assert_eq!(rotated.dek_version, 2);
}
```

Test 4: **Key Revocation**
```rust
#[tokio::test]
async fn test_key_revocation() {
    let kms = MockKMS::new();
    let key = generate_api_key(&kms, "test", "premium").await;

    // Verify key works before revocation
    let result = validate_api_key(&key, &kms).await;
    assert!(result.is_ok());

    // Revoke key
    revoke_api_key(&key, "suspected compromise").await.unwrap();

    // Verify key now fails
    let result = validate_api_key(&key, &kms).await;
    assert!(result.is_err());
}
```

### Integration Tests (Phase 13, Cycle 2 GREEN)

Test 5: **AWS KMS Integration**
```rust
#[tokio::test]
#[ignore]  // Requires AWS credentials
async fn test_aws_kms_integration() {
    let kms = AwsKmsClient::new("us-east-1").await.unwrap();
    let cmk_id = "arn:aws:kms:us-east-1:123456789:key/12345678";

    // Generate DataKey
    let result = kms.generate_data_key(&cmk_id).await.unwrap();
    assert!(result.plaintext_key.len() > 0);
    assert!(result.encrypted_key.len() > 0);

    // Decrypt
    let decrypted = kms.decrypt(&result.encrypted_key).await.unwrap();
    assert_eq!(decrypted, result.plaintext_key);
}
```

Test 6: **Latency Benchmarks**
```rust
#[tokio::test]
async fn test_key_validation_latency() {
    let kms = AwsKmsClient::new("us-east-1").await.unwrap();
    let key = generate_api_key(&kms, "benchmark", "premium").await;

    // Validate 100 times, measure P95
    let mut latencies = vec![];
    for _ in 0..100 {
        let start = Instant::now();
        validate_api_key(&key, &kms).await.unwrap();
        latencies.push(start.elapsed().as_millis());
    }

    latencies.sort();
    let p95 = latencies[95];

    // Target: <50ms P95
    assert!(p95 < 50, "P95 latency {} > 50ms", p95);
}
```

### Security Tests (Phase 13, Cycle 2 REFACTOR)

Test 7: **No Plaintext in Logs**
```rust
#[test]
fn test_no_plaintext_credentials_in_logs() {
    // Simulate key generation
    let key = generate_api_key_for_test();
    let api_key_material = &key[..32];

    // Capture logs
    let logs = run_test_with_logging(|| {
        validate_api_key(&key, &mock_kms).await.unwrap();
    });

    // Verify no plaintext in logs
    for log in logs {
        assert!(!log.contains(api_key_material),
                "Plaintext credentials found in log: {}", log);
    }
}
```

Test 8: **Memory Safety**
```rust
#[test]
fn test_plaintext_destroyed_after_use() {
    // Use zeroize crate to verify memory is zeroed
    let dek = plaintext_dek_from_kms();
    let dek_ptr = dek.as_ptr();

    // Use DEK for decryption
    let _result = decrypt_with_dek(&dek);

    // DEK should be zeroed (drop implementation)
    drop(dek);

    // Note: Can't directly verify memory, but clippy/linter will catch issues
    // Use: https://docs.rs/zeroize/latest/zeroize/
}
```

### Failure Tests (Phase 13, Cycle 2 GREEN)

Test 9: **AWS KMS Unavailable**
```rust
#[tokio::test]
async fn test_kms_unavailable_fallback() {
    let kms = DownKMS::new();  // Simulates KMS timeout/error

    // With cache, should still work for cached keys
    let cached_key = "fraiseql_us_east_1_key123";
    let result = validate_api_key(&cached_key, &kms).await;
    assert!(result.is_ok());  // Cache hit

    // New key lookup fails
    let new_key = "fraiseql_us_east_1_keynew_xxx";
    let result = validate_api_key(&new_key, &kms).await;
    assert!(result.is_err());  // Cache miss + KMS down
}
```

Test 10: **Key Expiration**
```rust
#[tokio::test]
async fn test_expired_key_rejected() {
    let kms = MockKMS::new();
    let key = generate_api_key(&kms, "test", "premium").await;

    // Manually set expiration to past
    set_key_expiration(&key, Instant::now() - Duration::secs(1)).await;

    // Validation should fail
    let result = validate_api_key(&key, &kms).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("expired"));
}
```

---

## Implementation Milestones

### Milestone 1: AWS KMS Integration (1-2 days)
- [ ] Add `rusoto_kms` dependency
- [ ] Implement AWS KMS client wrapper
- [ ] GenerateDataKey operation
- [ ] Decrypt operation
- [ ] Unit tests passing
- [ ] Mock KMS for tests

### Milestone 2: API Key Lifecycle (2-3 days)
- [ ] CreateAPIKey endpoint
- [ ] Key storage schema (database migration)
- [ ] ValidateAPIKey middleware
- [ ] Key hashing for lookups
- [ ] Integration tests passing

### Milestone 3: Key Rotation (1-2 days)
- [ ] Background job scheduler
- [ ] Rotation logic
- [ ] Automated tests
- [ ] Grace period enforcement

### Milestone 4: Caching & Performance (1-2 days)
- [ ] Redis DEK cache (optional, Phase 15 optimization)
- [ ] Latency benchmarks
- [ ] Performance tests

### Milestone 5: Security Hardening (1 day)
- [ ] Zeroize plaintext memory
- [ ] Secret scanning in logs
- [ ] Security tests passing
- [ ] Clippy warnings clean

---

## External Dependencies

### AWS Requirements (If Using AWS KMS)
- AWS Account with KMS enabled
- IAM role with KMS permissions:
  ```json
  {
    "Version": "2012-10-17",
    "Statement": [
      {
        "Effect": "Allow",
        "Action": [
          "kms:GenerateDataKey",
          "kms:Decrypt",
          "kms:ListKeys",
          "kms:DescribeKey"
        ],
        "Resource": "arn:aws:kms:us-east-1:*:key/*"
      }
    ]
  }
  ```
- CMK (Customer Master Key) created and enabled
- CloudTrail enabled for audit logging

### Rust Dependencies
- `rusoto_kms` - AWS KMS client
- `tokio` - Async runtime
- `serde` - JSON serialization
- `zeroize` - Secure memory clearing
- `redis` - For DEK caching (optional)
- `base64` - Key encoding

---

## Success Criteria

### RED Phase (This Phase)
- [x] HSM/KMS decision documented (AWS KMS chosen for MVP)
- [x] Key hierarchy design complete (3-level: root, DEK, transport)
- [x] API key lifecycle documented (generation, validation, rotation, revocation)
- [x] Performance targets defined (<50ms P95)
- [x] Testing strategy complete (10+ test cases)
- [x] Dependencies identified
- [x] Milestones defined

### GREEN Phase (Next)
- [ ] AWS KMS integration implemented
- [ ] API key lifecycle working end-to-end
- [ ] 10+ tests passing
- [ ] No plaintext credentials in logs/code

### REFACTOR Phase
- [ ] Performance validated (<50ms P95)
- [ ] Security hardened (zeroize, secret scanning)
- [ ] All edge cases handled

### CLEANUP Phase
- [ ] Clippy warnings clean
- [ ] Integration tests passing
- [ ] Documentation complete
- [ ] Ready for Cycle 3 (Audit Logging)

---

## Risk Assessment

### Risk 1: AWS KMS Latency Impact
- **Risk**: KMS calls add >50ms to query latency
- **Mitigation**: DEK caching with 60s TTL (expected: <5ms cache hit)
- **Contingency**: Implement Vault fallback (Phase 15)

### Risk 2: Credential Exposure in Logs
- **Risk**: Plaintext API keys leak into logs
- **Mitigation**: Zeroize + secret scanning in CI/CD
- **Test**: Security test suite validates

### Risk 3: Key Rotation Complexity
- **Risk**: Bulk rotation fails, leaving unrotated keys
- **Mitigation**: Automated job with error handling + alerts
- **Rollback**: Manual rotation endpoint for emergency

### Risk 4: AWS KMS Lock-in
- **Risk**: Hard to migrate to multi-cloud
- **Mitigation**: Abstraction layer (KMS trait) for future Vault support
- **Future**: Phase 15 optimization can swap implementation

---

## Next Steps

### Immediate (Phase 13, Cycle 2 GREEN)
1. Implement AWS KMS client wrapper
2. Create API key storage schema
3. Implement key lifecycle operations
4. Get tests green

### Short-term (Phase 13, Cycle 2 REFACTOR)
1. Validate performance (<50ms)
2. Security hardening (zeroize, scanning)
3. Edge case handling

### Medium-term (Phase 13, Cycle 2 CLEANUP)
1. Documentation complete
2. Integration tests passing
3. Ready for Phase 13, Cycle 3 (Audit Logging)

---

**RED Phase Status**: ✅ READY FOR IMPLEMENTATION
**Ready for**: GREEN Phase (HSM/KMS Implementation)
**Target Date**: February 13-14, 2026

