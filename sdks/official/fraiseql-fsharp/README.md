# FraiseQL F# SDK

F# schema authoring SDK for [FraiseQL](https://github.com/fraiseql/fraiseql) — define GraphQL schemas with attributes or the idiomatic `fraiseql { }` computation expression DSL.

## Installation

```bash
dotnet add package FraiseQL.FSharp
```

## Two Authoring Approaches

### 1. Attribute-Based (familiar for C#/.NET teams)

Decorate your types with `[<GraphQLType>]` and fields with `[<GraphQLField>]`, then register them with `SchemaRegistry` and export to `schema.json`:

```fsharp
open FraiseQL

[<GraphQLType(Name = "Author", SqlSource = "v_author", Description = "A blog author")>]
type AuthorEntity() =
    [<GraphQLField(Nullable = false)>]
    member val Id: System.Guid = System.Guid.Empty with get, set

    [<GraphQLField(Nullable = false)>]
    member val Name: string = "" with get, set

    [<GraphQLField(Nullable = true)>]
    member val Bio: string = null with get, set

// Registration
SchemaRegistry.register typeof<AuthorEntity>

// Add queries using the pipe-friendly QueryBuilder
QueryBuilder.query "authors"
|> QueryBuilder.returnType "Author"
|> QueryBuilder.returnsList true
|> QueryBuilder.sqlSource "v_author"
|> QueryBuilder.register

// Add mutations using MutationBuilder
MutationBuilder.mutation "createAuthor"
|> MutationBuilder.returnType "Author"
|> MutationBuilder.sqlSource "fn_create_author"
|> MutationBuilder.operation "insert"
|> MutationBuilder.register

// Export to schema.json
SchemaExporter.exportToFile "schema.json"
```

### 2. Computation Expression DSL (idiomatic F#)

The `fraiseql { }` builder produces an immutable `IntermediateSchema` value without touching global state. Recommended for new F# projects:

```fsharp
open FraiseQL
open FraiseQL.Dsl

let schema =
    fraiseql {
        type' (TypeCEBuilder("Author") {
            sqlSource "v_author"
            description "A blog author"
            field (FieldBuilder("id", "ID") { nullable false })
            field (FieldBuilder("name", "String") { nullable false })
            field (FieldBuilder("bio", "String") { nullable true })
        })
        query (QueryCEBuilder("authors") {
            returnType "Author"
            returnsList true
            sqlSource "v_author"
        })
        query (QueryCEBuilder("authorById") {
            returnType "Author"
            sqlSource "v_author"
            arg "id" "ID" false
        })
        mutation (MutationCEBuilder("createAuthor") {
            returnType "Author"
            sqlSource "fn_create_author"
            operation "insert"
            arg "name" "String" false
        })
    }

// Export the schema value to disk
SchemaExporter.exportSchemaToFile "schema.json" schema
```

## Type Mapping

| F# Type | GraphQL Type |
|---------|-------------|
| `int`, `int64`, `int16` | `Int` |
| `float`, `double`, `float32` | `Float` |
| `decimal` | `Float` |
| `bool` | `Boolean` |
| `string` | `String` |
| `System.Guid` | `ID` |
| `System.DateTime` | `DateTime` |
| `System.DateTimeOffset` | `DateTime` |
| `T option` | Nullable `T` |
| `T list`, `T array` | `[T]` |
| Any other type | Type name (e.g. `MyRecord`) |

## JSON Output Format

Both approaches produce a `schema.json` in the format consumed by `fraiseql compile`:

```json
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Author",
      "sql_source": "v_author",
      "description": "A blog author",
      "fields": [
        { "name": "id", "type": "ID", "nullable": false },
        { "name": "name", "type": "String", "nullable": false },
        { "name": "bio", "type": "String", "nullable": true }
      ],
      "is_input": false,
      "relay": false,
      "is_error": false
    }
  ],
  "queries": [
    {
      "name": "authors",
      "return_type": "Author",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_author",
      "arguments": []
    }
  ],
  "mutations": [
    {
      "name": "createAuthor",
      "return_type": "Author",
      "sql_source": "fn_create_author",
      "operation": "insert",
      "arguments": []
    }
  ]
}
```

## CLI Tool

Install the `fraiseql-schema-fsharp` dotnet tool to export schemas from compiled assemblies:

```bash
dotnet tool install --global FraiseQL.Tool.FSharp
```

```
Usage:
  fraiseql-schema-fsharp export <assembly.dll> [--output <path>] [--compact]

Commands:
  export    Load a .NET assembly and generate schema.json

Options:
  --output <path>    Output file path (default: schema.json)
  --compact          Write compact (non-indented) JSON
  --version          Show version information
  --help, -h         Show this help message
```

Example:

```bash
fraiseql-schema-fsharp export MyApp.dll --output out/schema.json
```

## Integration with fraiseql compile

After generating `schema.json`, pass it to the FraiseQL Rust compiler:

```bash
fraiseql compile schema.json --output schema.compiled.json
```

The compiled schema is then loaded by the FraiseQL server at startup to initialize all GraphQL query handlers.

## License

MIT — see [LICENSE](../../../LICENSE) for details.
