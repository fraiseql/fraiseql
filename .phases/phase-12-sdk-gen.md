# Phase 12: `fraiseql-sdk-gen` Crate

## Objective

Ship a new `fraiseql-sdk-gen` crate that generates typed client SDKs
(TypeScript and Python) from a compiled `schema.json`. Expose it via the CLI
(`fraiseql generate`) and via an HTTP endpoint (`GET /admin/sdk/generate`).

## Status

[ ] Not Started

## Background

SpecQL's `20260427-specql-platform-gaps/phase-15-sdk-generation-api.md` is
explicitly blocked on this crate. That SpecQL phase currently returns `501 Not
Implemented` stubs. Once `fraiseql-sdk-gen` ships, SpecQL P15 upgrades the
stubs to real generation.

Additionally, the "Supabase for AI" positioning requires that agents can
generate a typed client immediately after deploying a backend — SDK generation
is the last step in the zero-step provisioning loop.

The existing CLI already has `fraiseql generate <language>` commands for
TypeScript/Python/Go/etc. as authoring-side generators. This crate is the
*runtime-side* SDK generator: takes a compiled schema (not a source schema) and
produces a typed client that speaks GraphQL to the live API.

## Success Criteria

- [ ] `fraiseql generate typescript --schema schema.json --output ./client` produces
      a working TypeScript client
- [ ] `fraiseql generate python --schema schema.json --output ./client` produces
      a working Python client
- [ ] Generated TypeScript: strict types, no `any`, passes `tsc --strict`
- [ ] Generated Python: type annotations, passes `mypy --strict`
- [ ] `GET /admin/sdk/generate?language=typescript` returns `.tar.gz` archive
- [ ] Round-trip test: compile a schema, generate SDK, make a query with the SDK
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean

---

## TDD Cycles

### Cycle 1: `fraiseql-sdk-gen` crate scaffold + TypeScript generation

**New crate**: `crates/fraiseql-sdk-gen/`

**RED**:

- `typescript_generates_interface_for_type` — schema with `User { id: ID, name: String }`,
  assert generated `.ts` contains `interface User { id: string; name: string; }`
- `typescript_generates_query_function` — schema with `users: [User]` query,
  assert generated `.ts` contains `export async function users(...)`
- `typescript_generates_mutation_function`
- `typescript_output_passes_tsc_strict` — shell out to `tsc --strict --noEmit`
  on generated output (integration test, `#[ignore = "requires tsc"]`)

**GREEN**:

- `fraiseql-sdk-gen/src/lib.rs`: `pub struct SdkGenerator { schema: CompiledSchema }`
- `fraiseql-sdk-gen/src/typescript.rs`: `TypeScriptGenerator` that walks the
  compiled schema and renders:
  - `interface {TypeName}` for each type
  - `export async function {queryName}(variables: {...}): Promise<{ReturnType}>`
  - A `FraiseQLClient` class wrapping `fetch` for query dispatch
- Output is a `HashMap<String, String>` (filename → content)

**REFACTOR**: Extract a `CodeWriter` helper (indentation, line joining) shared
by TypeScript and Python generators.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 2: Python generation

**Crate**: `fraiseql-sdk-gen`

**RED**:

- `python_generates_dataclass_for_type`
- `python_generates_query_function_with_type_annotations`
- `python_output_passes_mypy_strict` — integration test, `#[ignore = "requires mypy"]`

**GREEN**:

- `fraiseql-sdk-gen/src/python.rs`: `PythonGenerator` that renders:
  - `@dataclass` for each type (`from __future__ import annotations` for forward refs)
  - `async def {query_name}(...) -> {ReturnType}:` using `httpx.AsyncClient`
  - A `FraiseQLClient` class
- Python output uses `from __future__ import annotations` + `X | None` style
  (no `Optional`)

**REFACTOR**: `SdkGenerator::generate(language: Language) -> HashMap<String, String>`
as a unified entry point dispatching to `TypeScriptGenerator` or `PythonGenerator`.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 3: CLI integration

**Crate**: `fraiseql-cli`

**RED**:

- `cli_generate_typescript_creates_output_files`
- `cli_generate_python_creates_output_files`
- `cli_generate_unsupported_language_returns_error`

**GREEN**:

- Add `fraiseql generate-sdk --language <ts|python> --schema <path> --output <dir>`
  command (distinct from existing `generate` which is authoring-side)
- Or extend existing `fraiseql generate` to accept `--runtime-sdk` flag
- Wire to `fraiseql_sdk_gen::SdkGenerator::generate()`
- Write files to output directory, print manifest to stdout

**REFACTOR**: Consider whether `generate-sdk` and `generate` (authoring) should
be subcommands of a unified `fraiseql codegen` top-level command for clarity.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 4: HTTP endpoint + archive packaging

**Crate**: `fraiseql-server`

**RED**:

- `sdk_generate_endpoint_returns_tar_gz_for_typescript`
- `sdk_generate_endpoint_returns_tar_gz_for_python`
- `sdk_generate_endpoint_requires_admin_key`
- `sdk_generate_endpoint_returns_404_if_no_schema`

**GREEN**:

- `GET /admin/sdk/generate?language=typescript` (or `POST` with body)
- Loads current compiled schema from server state
- Runs `SdkGenerator::generate(language)` in a blocking task
- Packs result into `.tar.gz` using `async-tar` or `flate2`
- Returns with `Content-Type: application/gzip`,
  `Content-Disposition: attachment; filename="fraiseql-client-ts.tar.gz"`

**REFACTOR**: The tar packing logic is generic — extract to `sdk_gen::archive(files)`.

**CLEANUP**: Clippy, fmt, doc. Add endpoint to OpenAPI spec.

---

## Dependencies

- Requires: Phase 9 merged (stable schema format)
- Parallel with: Phase 11 (independent crate, no shared infra)
- Unblocks:
  - SpecQL `20260427-specql-platform-gaps/` Phase 15 (SDK Generation API)
    upgrades from 501 stubs to real generation

## Version target

v2.3.0
