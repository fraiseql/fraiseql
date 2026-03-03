# Changelog

## [2.0.0] — 2025-03-03

### Added

- `[GraphQLType]` attribute for annotating C# classes as GraphQL types with `Name`, `SqlSource`,
  `Description`, `IsInput`, `Relay`, and `IsError` properties
- `[GraphQLField]` attribute for annotating properties with `Type`, `Nullable`, `Description`,
  `Resolver`, `Scope`, and `Scopes` properties
- `SchemaRegistry` thread-safe singleton for reflection-based type, query, and mutation registration
- `TypeMapper` for automatic C# type to GraphQL type conversion (int/long → Int, string → String,
  Guid → ID, DateTime → String, etc.)
- `QueryBuilder` fluent API: `.ReturnType()`, `.ReturnsList()`, `.SqlSource()`, `.Argument()`,
  `.CacheTtlSeconds()`, `.Description()`, `.Register()`, `.Build()`
- `MutationBuilder` fluent API: `.ReturnType()`, `.SqlSource()`, `.Operation()`, `.Argument()`,
  `.Description()`, `.Register()`, `.Build()`
- `SchemaExporter` static class: `Export(bool pretty)`, `ExportToFile(string path)`, `ToSchema()`,
  `Serialize(IntermediateSchema, bool)`
- `SchemaBuilder` fluent code-first API: `.Type()`, `.Query()`, `.Mutation()`, `.ToSchema()`,
  `.Export()`, `.ExportToFile()`
- `FraiseQL.Tool` dotnet global tool: `fraiseql export <assembly.dll> --output schema.json`
- Intermediate schema models (`IntermediateSchema`, `IntermediateType`, `IntermediateField`,
  `IntermediateQuery`, `IntermediateMutation`, `IntermediateArgument`) with snake_case JSON keys
- CI workflow (`.github/workflows/csharp-sdk.yml`) with test and NuGet publish jobs

### Removed

- Old v1 dictionary-based `Schema.RegisterType` API
- Development archaeology files (`DEPRECATED.md`, `CSHARP_FEATURE_PARITY.md`,
  `Phase18Cycle15ScopeExtractionTests.cs`)
