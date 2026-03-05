# SDK Parity Matrix

## Purpose

Track which features each SDK tests, identify gaps, and prevent the SDK
ecosystem from silently drifting out of parity with the Rust runtime.

This document is a living record. Update it when:
- A new feature is added to the Python/TypeScript authoring layer
- A new SDK is promoted from community to official
- An SDK adds or removes test coverage

---

## Batch 4 Audit Results (2026-03-05)

All 11 official SDK CI workflows were audited. **All run a test runner** —
none are build-only. Key findings:

| SDK | CI file | Test command | Functional tests | Test file count |
|-----|---------|-------------|------------------|-----------------|
| Python | `python-sdk.yml` | `uv run pytest tests -v` | ✅ Yes | 10 test files |
| TypeScript | `typescript-sdk.yml` | `npm test` (jest) | ✅ Yes | 13 test files |
| Go | `go-sdk.yml` | `go test -v -race ./...` | ✅ Yes | 8 `*_test.go` files |
| Java | `java-sdk.yml` | `mvn -B verify` | ✅ Yes (Maven verify runs tests) | Maven standard structure |
| PHP | `php-sdk.yml` | `vendor/bin/phpunit` | ✅ Yes | 17 test files |
| Rust | `rust-sdk.yml` | `cargo test --all-features` | ✅ Yes | 3 test files |
| C# | `csharp-sdk.yml` | `dotnet test --no-build --configuration Release` | ✅ Yes | .NET test project |
| Ruby | `ruby-sdk.yml` | `bundle exec rspec` | ✅ Yes | RSpec suite |
| Dart | `dart-sdk.yml` | `dart test` | ✅ Yes | Test directory present |
| F# | `fsharp-sdk.yml` | `dotnet test --collect:"XPlat Code Coverage"` | ✅ Yes | .NET test project |
| Elixir | `elixir-sdk.yml` | `mix test` (matrix: Elixir 1.15/1.16/1.17 × OTP 26/27) | ✅ Yes | ExUnit suite + Dialyzer |

**SDK-2 conclusion**: Go SDK was already fixed — CI runs `go test -v -race ./...` with
8 inline test files covering: completeness, custom scalars, export types, golden schema,
parity schema, registry, scope extraction, and types.

---

## Feature Coverage Matrix

For each feature, "✅" means the SDK has a functional test that exercises
the feature and validates the output. "⚠️" means partial (builds but no
assertion). "❌" means untested. "(skip)" means not applicable.

| Feature | Python | TypeScript | Go | Java | PHP | Rust | C# | Ruby | Dart | F# | Elixir |
|---------|--------|------------|-----|------|-----|------|-----|------|------|----|--------|
| `@type` basic | ✅ | ✅ | ✅ | ⚠️ | ✅ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ✅ |
| `@type` with nested types | ✅ | ✅ | ✅ | ⚠️ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `@query` with SQL source | ✅ | ✅ | ✅ | ⚠️ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `@mutation` with invalidates | ✅ | ✅ | ✅ | ⚠️ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `@subscription` | ✅ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `fraiseql.field()` scope | ✅ | ✅ | ✅ | ⚠️ | ✅ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ✅ |
| `@interface` | ✅ | ✅ | ⚠️ | ⚠️ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `@union` | ✅ | ✅ | ⚠️ | ⚠️ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `@enum` | ✅ | ✅ | ⚠️ | ⚠️ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `@input` | ✅ | ✅ | ⚠️ | ⚠️ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `@scalar` | ✅ | ✅ | ✅ | ⚠️ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| `@error` | ✅ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| Schema export to JSON | ✅ | ✅ | ✅ | ⚠️ | ✅ | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ✅ |
| Schema roundtrip (export → CLI compile) | ✅ | ⚠️ | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Golden schema comparison | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |

**Remaining gaps** in official SDKs (⚠️ = test exists but depth insufficient; ❌ = untested):
- Java, C#, F#, Ruby, Dart: golden schema comparison and roundtrip missing
- `@subscription`, `@interface`, `@union`, `@enum`, `@input`, `@error`: only Python, TypeScript, PHP have full test coverage

---

## Community SDK Audit

| SDK | Test files before Batch 4 | Schema roundtrip test added |
|-----|--------------------------|---------------------------|
| Clojure | `export_types_test.clj`, `scope_extraction_test.clj` | ✅ `schema_roundtrip_test.clj` |
| NodeJS | `export-types.test.ts`, `scope-extraction.test.ts` | ✅ `schema-roundtrip.test.ts` |
| Groovy | `SchemaSpec.groovy`, `Phase18Cycle18ScopeExtractionSpec.groovy` | ✅ `SchemaRoundtripSpec.groovy` |
| Kotlin | `ExportTypesTest.kt`, `Phase18Cycle12ScopeExtractionTest.kt` | ✅ `SchemaRoundtripTest.kt` |
| Scala | `ExportTypesSpec.scala`, `Phase18Cycle17ScopeExtractionSpec.scala` | ✅ `SchemaRoundtripSpec.scala` |
| Swift | `ExportTypesTests.swift`, `Phase18Cycle19ScopeExtractionTests.swift` | ✅ `SchemaRoundtripTests.swift` |
| Elixir | `export_types_test.exs`, `scope_extraction_test.exs` | archived (DEPRECATED) |
| Dart | `export_types_test.dart`, `scope_extraction_test.dart` | archived (DEPRECATED) |
| Ruby | see official SDK | see official SDK |

---

## Minimum Bar for "Official" SDK

For an SDK to be listed as "official" and included in the release announcement,
it must have:

1. **Decorator tests** — at least one test per decorator type that validates
   the generated JSON shape
2. **Export test** — `registry.export()` produces valid JSON parseable as `schema.json`
3. **Golden schema test** — a known-good `schema.json` fixture committed to the
   repository; the export must match it
4. **CI gate** — all three test types run in CI on every PR that touches the SDK
5. **Roundtrip test** (recommended) — exported schema passes `fraiseql-cli compile`
   without validation errors

Current official SDKs that fully meet this bar: Python, TypeScript, Go, Rust.
SDKs that meet the CI gate but lack golden/roundtrip: Java, PHP, C#, Ruby, Dart, F#, Elixir.

---

## Parity Test Protocol

When a new feature is added to the authoring layer (Python SDK), the following
must happen before the feature is documented as available:

1. Python SDK test for the feature is written and passes
2. If the feature changes the `schema.json` format, the golden fixture is updated
3. Within one release cycle, TypeScript SDK test is added
4. Other official SDKs are given a tracked issue to add the test, with a
   "parity deadline" of two release cycles

---

## Duplication Resolution (SDK-4)

Both Elixir and Dart appeared in `sdks/official/` and `sdks/community/`.
Resolution:

| SDK | Authoritative | Archived |
|-----|--------------|---------|
| Elixir | `sdks/official/fraiseql-elixir/` | `sdks/community/fraiseql-elixir/` → `sdks/archived/` |
| Dart | `sdks/official/fraiseql-dart/` | `sdks/community/fraiseql-dart/` → `sdks/archived/` |

The community versions had `DEPRECATED.md` files explaining migration to
the official HTTP-based approach for v2.
