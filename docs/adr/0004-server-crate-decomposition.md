# ADR-0004: Server Crate Decomposition (auth, webhooks, secrets)

## Status: Accepted

## Context

`fraiseql-server` grew to 175+ modules spanning authentication (38 files), webhook delivery (19 files), and secrets management (21 files). This monolithic structure creates tight coupling, makes testing slow (entire crate rebuilds on any change), and obscures ownership. Developers can't depend on just secrets management without pulling in auth subsystem.

## Decision

Extract three specialized crates from fraiseql-server:

1. **fraiseql-auth** (38 modules): OAuth, OIDC, JWT, SCRAM, MFA
2. **fraiseql-webhooks** (19 modules): Delivery, retry policy, event filtering
3. **fraiseql-secrets** (21 modules): Field encryption, key rotation, Vault integration

`fraiseql-server` depends on these and re-exports their public APIs for backward compatibility. Each crate has independent testing and dependency management.

## Consequences

**Positive:**

- Independent compilation (faster iteration)
- Clear ownership boundaries
- Smaller crates reduce dependency bloat
- Reusable in other projects

**Negative:**

- Re-export layer adds one level of indirection
- Cross-crate testing more complex
- More crates to version and release

## Alternatives Considered

1. **Keep monolithic server**: Simpler but slower builds and unclear architecture
2. **Extract to separate binaries**: Over-decomposition; adds network overhead
3. **No exports, direct dependencies**: Forces all users to manage cross-crate dependencies
