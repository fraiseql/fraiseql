# FraiseQL Threat Model

## 1. Threat Actors

| Actor | Description | Trust Level |
|-------|-------------|-------------|
| **Unauthenticated external** | Internet-facing attacker with no credentials | Zero trust |
| **Authenticated but unauthorized tenant** | Valid JWT, but attempts to access another tenant's data | Low trust |
| **Compromised internal service** | Lateral movement from another pod/service inside the cluster | Conditional |
| **Malicious schema author** | Developer submitting a crafted `schema.json` at compile time | Supply chain |
| **Insider threat** | Operator with infrastructure access (Vault, database, CI) | High trust, monitored |

---

## 2. Trust Boundaries

| Boundary | Where it sits | Enforcement |
|----------|---------------|-------------|
| **Network perimeter** | TLS termination at ingress | `rustls 0.23` + `tokio-rustls 0.25`; panics on misconfigured cert (graceful Err since A30) |
| **Authentication boundary** | JWT/PKCE validation in `fraiseql-auth` | `jsonwebtoken` RS256/HS256; constant-time comparison (`subtle`) |
| **Tenant isolation boundary** | Row-Level Security WHERE clause composition | RLS always AND-ed with app WHERE; SecurityContext required by type system |
| **Compiled schema artifact** | Compiler → runtime handoff | `schema_format_version` check; planned `_content_hash` verification |
| **Secrets management boundary** | Vault token scope + credential rotation | `fraiseql-secrets`; token renewal on expiry |

---

## 3. Data Flow Diagram

```
Internet
   │ TLS (rustls 0.23)
   ▼
fraiseql-server (Axum 0.8)
   │
   ├── Authentication layer (fraiseql-auth)
   │     JWT/PKCE validation
   │     Rate limiting (in-memory or Redis)
   │     → SecurityContext{user_id, tenant_id, roles}
   │
   ├── Request validator (validation.rs)
   │     Alias amplification check (max 30)
   │     Query depth check (default 10)
   │     Query complexity error rate-limit
   │
   ├── Matcher (runtime/executor/)
   │     O(1) lookup via build_indexes()
   │     → QueryDefinition (compiled SQL template)
   │
   ├── RLS evaluator (security/rls_policy.rs)
   │     SecurityContext → per-user WHERE clause
   │     (always AND-ed, never OR-ed)
   │
   ├── Cache layer (cache/adapter/)
   │     Key = SHA-256(query + vars + rls_where + schema_version)
   │     RLS isolation guaranteed by key construction
   │
   └── Database (PostgreSQL)
         RLS enforced at DB level too
         Parameterized queries only (no string interpolation)
```

---

## 4. Mitigations Table

| Threat | Attack Vector | Mitigation | Location | Status |
|--------|---------------|------------|----------|--------|
| SQL injection | Malicious WHERE clause dimension | Path escaping + dimension allowlist | `db/path_escape.rs`, `compiler/aggregation.rs` | ✅ |
| SQL injection | Dynamic function names | Double-quoted PG identifiers | `postgres/where_generator.rs` | ✅ |
| Cross-tenant data leak | Shared cache entries | RLS WHERE in cache key | `cache/key.rs` | ✅ |
| Cross-tenant data leak | RLS bypass via refactor | `RlsWhereClause` newtype (planned U2) | `runtime/rls.rs` | ⚠️ planned |
| Token timing attack | Credential comparison | `subtle::ConstantTimeEq` | `constant_time.rs` | ✅ |
| PKCE state inspection | State parameter sniffing | AES-GCM state encryption | `state_encryption.rs` | ✅ |
| Brute force on auth | Credential stuffing | Token-bucket rate limiting | `middleware/rate_limit/` | ✅ |
| Alias amplification | 31+ aliases on one field | Hard cap at 30 aliases | `server/validation.rs:459` | ✅ |
| Query depth bomb | Deeply nested query | Max depth 10 (configurable) | `server/validation.rs:457` | ✅ |
| Fragment cycles | Recursive fragment refs | Cycle detection | `server/validation.rs` | ⚠️ verify |
| Introspection abuse | Schema enumeration via __schema | Introspection disable flag | `server/validation.rs` | ⚠️ verify |
| Resource amplification | Alias + batch combo | Alias cap + federation batch limit 1000 | `validation.rs`, `federation/` | ✅ |
| Credential exposure | Plaintext secrets | HashiCorp Vault integration | `fraiseql-secrets` | ✅ |
| Webhook replay | Timestamp-missing webhooks | Timestamp replay protection | `webhooks/` | ✅ |
| SASL/SCRAM attack | Protocol-level credential theft | SCRAM-SHA-256 only | `fraiseql-wire/auth/scram.rs` | ✅ |
| Template injection | Dynamic SQL in compiler IR | SQL fixed at compile time in IR | `compiler/ir.rs` | ✅ |
| RBAC resource enumeration | 403 reveals resource exists | 404 on denied access | `api/rbac_management/` | ✅ |
| Circuit breaker poisoning | Cascading federation failures | `parking_lot::Mutex` (no poison) | `federation/circuit_breaker.rs` | ✅ |

---

## 5. Known Gaps

See `docs/security/graphql-complexity-limits.md` for incomplete complexity protections:
- Fragment cycle detection: needs verification
- Introspection disable: needs verification
- Cost/complexity budget: not implemented (planned for v2.2.0)

---

## 6. Security Contact

Report vulnerabilities via GitHub Security Advisories on the FraiseQL repository.
For urgent issues, contact the maintainers directly via the repository's SECURITY.md policy.

---

## References

- `docs/adr/0007-crypto-algorithm-choices.md` — rationale for AES-GCM, SCRAM-SHA-256
- `crates/fraiseql-auth/src/constant_time.rs` — timing attack prevention
- `crates/fraiseql-server/src/validation.rs` — GraphQL query limits
- `crates/fraiseql-core/src/cache/key.rs` — cache security model
