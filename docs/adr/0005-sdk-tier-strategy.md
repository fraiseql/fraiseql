# ADR-0005: SDK Tiering Strategy (6 Supported, 10 Community)

## Status: Accepted

## Context

FraiseQL v1 published 16 SDKs (.NET, Kotlin, Go, Ruby, PHP, Python, TypeScript, Rust, Java, Elixir, Swift, Dart, C++, R, Julia, Haskell) before v1.0. Maintaining 16 SDKs for a pre-v1.0 project creates unsustainable burden: each requires documentation, testing, changelog management, and security updates. Quality varies widely; several receive minimal use.

## Decision

Implement SDK tiering:

**Tier 1 (Officially Supported)**: Python, TypeScript, Java, Go
**Tier 2 (Maintained)**: PHP, Rust
**Community (Deprecated)**: .NET, Kotlin, Ruby, Elixir, Swift, Dart, C++, R, Julia, Haskell

Tier 1 languages receive active support. Tier 2 receives maintenance updates. Community SDKs archived with migration guides. JVM languages (Kotlin, Clojure) use Java SDK via interop.

## Consequences

**Positive:**

- Focused maintenance effort
- Higher quality Tier 1 SDKs
- Realistic support matrix
- Reduced security exposure

**Negative:**

- Some language communities lose direct support
- Developers must migrate if preferred language demoted
- Perception of reduced language coverage

## Alternatives Considered

1. **Support all 16**: Impossible to maintain quality long-term
2. **REST API only**: Eliminates type-safe SDKs; worse developer experience
3. **Community-driven all**: No vendor accountability; SDKs may bitrot
