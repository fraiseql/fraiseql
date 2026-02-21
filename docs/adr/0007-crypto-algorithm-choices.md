# ADR-0007: Cryptographic Algorithm Selection

## Status: Accepted

## Context

FraiseQL requires cryptography for multiple purposes: field-level encryption, OAuth state protection, webhook signature verification, and SCRAM authentication. Different algorithms have different security properties and performance characteristics. Consistency reduces cognitive load and maintenance burden.

## Decision

Use algorithm portfolio approach:

- **Field Encryption**: AES-256-GCM (NIST standard, hardware acceleration via AES-NI)
- **PKCE State Encryption**: ChaCha20-Poly1305 (software-only performance for lower-latency state generation)
- **Webhook Signatures**:
  - Discord/GitHub: Ed25519 (signature algorithm they use)
  - Others: HMAC-SHA256 (most providers support)
- **SCRAM Authentication**: HMAC-SHA256 with salted key derivation
- **Nonce Generation**: `getrandom::getrandom()` + `OsRng` (kernel entropy)
- **Constant-Time Operations**: `subtle` crate (timing attack prevention)

## Consequences

**Positive:**

- Algorithms chosen based on use case, not dogmatism
- Production-grade crypto with peer review
- Hardware acceleration where available
- Resistance to timing attacks

**Negative:**

- Multiple algorithms increase cognitive complexity
- More dependencies to maintain
- Requires cryptography expertise for changes

## Alternatives Considered

1. **Single algorithm everywhere**: Simplifies codebase but suboptimal for each use case
2. **libsodium wrapper**: Adds FFI overhead; Rust ecosystem avoids this
3. **ring crate only**: Fewer algorithms; good but doesn't support Ed25519
