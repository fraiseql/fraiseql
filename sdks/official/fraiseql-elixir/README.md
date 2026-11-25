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
| `localized:` | boolean | false | A String stored as a locale map (`[locale]` in `fraiseql.toml`); any other type fails to compile |
| `scoped_like_parent:` | boolean | false | On a field embedding a type whose own query is scoped by `inject_params`: the embedded rows are scoped as the parent's are |

A field of an `is_input` type takes `nullable:`, `description:`, `deprecated:` and
`localized:`. `requires_scope:`, `requires_scopes:`, `computed:`, `vector_config:`,
`vector_distance:` and `scoped_like_parent:` configure an output field and raise an
`ArgumentError` there, naming the type, the field and each option.

### `argument` options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `nullable:` | boolean | false | Whether argument is optional |
| `description:` | string | nil | Human-readable description |
| `localized:` | boolean | false | A String received as a locale map; any other type fails to compile |

## Observers

An observer runs actions when a row of an entity is inserted, updated or deleted:

```elixir
fraiseql_observer :order_shipped,
  entity: "Order",
  event: "UPDATE",
  condition: "status == 'shipped'",
  actions: [
    FraiseQL.Observer.webhook(url_env: "ORDER_HOOK_URL", signing_secret_env: "ORDER_HOOK_KEY"),
    FraiseQL.Observer.slack("#orders", "Order {{ id }} shipped"),
    FraiseQL.Observer.email("ops@example.com", "Order shipped", "Order {{ id }} shipped")
  ],
  retry: %FraiseQL.RetryConfig{
    max_attempts: 5,
    backoff_strategy: "exponential",
    initial_delay_ms: 200,
    max_delay_ms: 5000
  }
```

A webhook takes exactly one of `url:` and `url_env:`. A Slack action posts to the URL in
`SLACK_WEBHOOK_URL` unless `webhook_url:` or `webhook_url_env:` names another. The email
sender is the server's SMTP configuration. An observer with no `retry:` runs on the
server's own policy; `retry:` takes a `%FraiseQL.RetryConfig{}` and nothing else.

## Type Mapping

| Elixir atom | GraphQL type |
|-------------|-------------|
| `:integer` | `Int` |
| `:bigint` | `BigInt` (a signed 64-bit integer, served as its digits) |
| `:int` | `Int` |
| `:float` | `Float` |
| `:boolean` | `Boolean` |
| `:bool` | `Boolean` |
| `:string` | `String` |
| `:id` | `ID` |
| `:datetime`, `:utc_datetime` | `DateTime` |
| `:naive_datetime` | `LocalDateTime` (a date and time with no offset, a PostgreSQL `timestamp`) |
| `:user_profile` (unknown) | `UserProfile` (PascalCase) |

Any unknown atom is automatically converted to PascalCase and used as-is.

### Decimal values in the client

A `Decimal` travels as a JSON string of its exact digits, `"1.50"`, in both directions.
`FraiseQL.Client` encodes with Jason, which writes a `%Decimal{}` variable as
`Decimal.to_string(d, :normal)`: every digit and the scale, never an exponent
(`Decimal.new("1E+3")` is sent as `"1000"`). A response holds each `Decimal` as that
string; `Decimal.new(value)` reads it exactly. Do not pass a float: the server refuses a
number with a fraction for a `Decimal`.

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
