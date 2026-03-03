# Changelog

## [2.0.0] — 2026-03-03

### Added

- `use FraiseQL.Schema` macro DSL for compile-time schema authoring
- `fraiseql_type/2,3`, `fraiseql_query/2,3`, `fraiseql_mutation/2,3` macros
- `field/3` and `argument/3` macros for use inside type/query/mutation blocks
- `FraiseQL.SchemaExporter` — converts schema module to intermediate `schema.json`
- `mix fraiseql.export` Mix task for CLI-driven schema export
- `FraiseQL.TypeMapper` with full GraphQL type mapping and camelCase/PascalCase helpers
- `FraiseQL.FieldDefinition`, `FraiseQL.ArgumentDefinition`, `FraiseQL.TypeDefinition`,
  `FraiseQL.QueryDefinition`, `FraiseQL.MutationDefinition`, `FraiseQL.IntermediateSchema` structs
- Full typespec coverage on all public functions
- Dialyzer configuration in `mix.exs`
- CI workflow (`.github/workflows/elixir-sdk.yml`) with Elixir 1.15–1.17 matrix

### Changed

- Agent-based API moved to `FraiseQL.Schema.Legacy` (backward compatible)
- Version bumped from `1.0.0` to `2.0.0`
- Relocated from `sdks/community/fraiseql-elixir` to `sdks/official/fraiseql-elixir`
- Elixir requirement raised from `~> 1.14` to `~> 1.15`
- Added `dialyxir ~> 1.4` as a dev dependency

### Removed

- `DEPRECATED.md` — no longer relevant
- `ELIXIR_FEATURE_PARITY.md` — no longer relevant
- `test/fraiseql/scope_extraction_test.exs` — superseded by new test suite
