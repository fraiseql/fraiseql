# FraiseQL Elixir SDK

Compile-time schema authoring for the [FraiseQL](https://fraiseql.com) compiled GraphQL engine.

Use this SDK to declare your GraphQL types, queries, and mutations in Elixir. The SDK generates
a `schema.json` file that `fraiseql compile` transforms into an optimised, SQL-backed GraphQL server.

## Installation

Add `:fraiseql` to your `mix.exs` dependencies:

```elixir
def deps do
  [
    {:fraiseql, "~> 2.0"}
  ]
end
```

Then run:

```bash
mix deps.get
```

## Quick Start

### 1. Define a schema module

```elixir
defmodule MyApp.Schema do
  use FraiseQL.Schema

  fraiseql_type "Author", sql_source: "v_author", description: "A blog author" do
    field :id,   :id,     nullable: false
    field :name, :string, nullable: false
    field :bio,  :string, nullable: true
  end

  fraiseql_query :authors,
    return_type: "Author",
    returns_list: true,
    sql_source: "v_author"

  fraiseql_query :author, return_type: "Author", sql_source: "v_author" do
    argument :id, :id, nullable: false
  end

  fraiseql_mutation :create_author,
    return_type: "Author",
    sql_source: "fn_create_author",
    operation: "insert" do
    argument :name, :string, nullable: false
    argument :bio,  :string, nullable: true
  end
end
```

### 2. Export to `schema.json`

```bash
mix fraiseql.export --module MyApp.Schema
```

Or from Elixir code:

```elixir
MyApp.Schema.export_to_file!("schema.json")
```

### 3. Compile with FraiseQL

```bash
fraiseql compile schema.json
```

This produces `schema.compiled.json` which you load into the FraiseQL Rust server.

## Macro Reference

| Macro | Required options | Purpose |
|-------|-----------------|---------|
| `fraiseql_type/2` | `sql_source:` | Register a type with no fields |
| `fraiseql_type/3` | `sql_source:` | Register a type with a `do` block of `field/3` calls |
| `fraiseql_query/2` | `return_type:`, `sql_source:` | Register a query with no arguments |
| `fraiseql_query/3` | `return_type:`, `sql_source:` | Register a query with a `do` block of `argument/3` calls |
| `fraiseql_mutation/2` | `return_type:`, `sql_source:`, `operation:` | Register a mutation with no arguments |
| `fraiseql_mutation/3` | `return_type:`, `sql_source:`, `operation:` | Register a mutation with a `do` block of `argument/3` calls |
| `field/3` | name atom, type atom | Declare a field inside a `fraiseql_type` block |
| `argument/3` | name atom, type atom | Declare an argument inside a query/mutation block |

### `fraiseql_type` options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `sql_source:` | string | required | Underlying view or table name |
| `description:` | string | nil | Human-readable description |
| `relay:` | boolean | false | Enable Relay pagination |
| `is_input:` | boolean | false | Mark as GraphQL input type |
| `is_error:` | boolean | false | Mark as mutation error shape |

### `fraiseql_query` options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `return_type:` | string | required | GraphQL return type name |
| `sql_source:` | string | required | Underlying view or table name |
| `returns_list:` | boolean | false | Whether query returns a list |
| `nullable:` | boolean | false | Whether result can be null |
| `cache_ttl_seconds:` | integer | nil | Cache TTL in seconds |
| `description:` | string | nil | Human-readable description |

### `fraiseql_mutation` options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `return_type:` | string | required | GraphQL return type name |
| `sql_source:` | string | required | Underlying function name |
| `operation:` | string | required | One of `"insert"`, `"update"`, `"delete"` |
| `description:` | string | nil | Human-readable description |

### `field` options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `nullable:` | boolean | false | Whether field is nullable |
| `description:` | string | nil | Human-readable description |
| `requires_scope:` | string | nil | Single OAuth scope required |
| `requires_scopes:` | list | nil | List of OAuth scopes (any one satisfies) |

### `argument` options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `nullable:` | boolean | false | Whether argument is optional |
| `description:` | string | nil | Human-readable description |

## Type Mapping

| Elixir atom | GraphQL type |
|-------------|-------------|
| `:integer` | `Int` |
| `:int` | `Int` |
| `:float` | `Float` |
| `:boolean` | `Boolean` |
| `:bool` | `Boolean` |
| `:string` | `String` |
| `:id` | `ID` |
| `:datetime` | `DateTime` |
| `:user_profile` (unknown) | `UserProfile` (PascalCase) |

Any unknown atom is automatically converted to PascalCase and used as-is.

## Mix Task Reference

```
mix fraiseql.export [options]
```

| Option | Alias | Default | Description |
|--------|-------|---------|-------------|
| `--module` | `-m` | required | Schema module to export, e.g. `MyApp.Schema` |
| `--output` | `-o` | `schema.json` | Output file path |
| `--compact` | | false | Write compact single-line JSON |

## Legacy API

The previous Agent-based API is preserved as `FraiseQL.Schema.Legacy` for backward compatibility.
New code should use `use FraiseQL.Schema` instead.

## Requirements

- Elixir 1.15 or higher
- OTP 26 or higher

## License

Apache License 2.0

## Links

- [FraiseQL documentation](https://fraiseql.com/docs)
- [GitHub repository](https://github.com/fraiseql/fraiseql/tree/main/sdks/official/fraiseql-elixir)
