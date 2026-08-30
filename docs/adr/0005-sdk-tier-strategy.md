# ADR-0005: SDK Tiering Strategy (6 Supported, 10 Community)

## Status: Superseded by [ADR-0019](0019-sdk-publication-boundary.md) (2026-08-30)

> The tiering below had drifted from the repository for years: four SDKs it calls
> "Community (Deprecated)" are official and score 18/19, F# is missing from it
> entirely, and the two languages it names Tier 1 (Supported) alongside Python and
> TypeScript — Java and Go — had never been published to any registry. ADR-0019
> replaces the tier vocabulary with a checkable property: published, or source-only.
> Retained for the history of the decision.

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
