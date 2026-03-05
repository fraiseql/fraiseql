# SDK Parity Matrix

## Purpose

Track which features each SDK tests, identify gaps, and prevent the SDK
ecosystem from silently drifting out of parity with the Rust runtime.

This document is a living record. Update it when:
- A new feature is added to the Python/TypeScript authoring layer
- A new SDK is promoted from community to official
- An SDK adds or removes test coverage

---

## Feature Coverage Matrix

For each feature, "✅" means the SDK has a functional test that exercises
the feature and validates the output. "⚠️" means partial (builds but no
assertion). "❌" means untested. "(skip)" means not applicable.

| Feature | Python | TypeScript | Go | Java | PHP | Rust | C# | Ruby | Dart | F# | Elixir |
|---------|--------|------------|-----|------|-----|------|-----|------|------|----|--------|
| `@type` basic | ✅ | (check) | ❌ | (check) | (check) | ✅ | (check) | (check) | (check) | (check) | (check) |
| `@type` with nested types | ✅ | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@query` with SQL source | ✅ | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@mutation` with invalidates | (check) | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@subscription` | (check) | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `fraiseql.field()` scope | ✅ | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@interface` | (check) | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@union` | (check) | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@enum` | (check) | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@input` | (check) | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@scalar` | (check) | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| `@error` | (check) | (check) | ❌ | (check) | (check) | (check) | (check) | (check) | (check) | (check) | (check) |
| Schema export to JSON | ✅ | (check) | ❌ | (check) | (check) | ✅ | (check) | (check) | (check) | (check) | ✅ |
| Schema roundtrip (export → CLI compile) | ✅ | (check) | ❌ | ❌ | ❌ | (check) | ❌ | ❌ | ❌ | ❌ | ❌ |
| Golden schema comparison | ✅ | (check) | ❌ | ❌ | ❌ | (check) | ❌ | ❌ | ❌ | ❌ | ❌ |

**Key gaps** (confirmed ❌):
- Go SDK: zero functional tests
- Roundtrip test (SDK → CLI compile): only Python and Rust
- Most SDKs lack golden schema comparison

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

Current official SDKs that do not meet this bar: Go (confirmed), others TBD
from Batch 4 audit.

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

## Update Process

After completing the Batch 4 SDK audit, fill in all "(check)" cells above
based on actual test file inspection. Replace "(check)" with ✅, ⚠️, or ❌.

Then create GitHub issues for each ❌ in the official SDK rows, labelled
`sdk-parity` and assigned to the SDK maintainer.
