# Intermediate Schema Format

The intermediate schema is the **contract between SDK authoring tools and the FraiseQL compiler**.

Language SDKs (Python, TypeScript, C#, Go, Rust) emit a `schema.json` file in this format.
`fraiseql-cli compile schema.json` reads it, validates it, and produces `schema.compiled.json`.

```
Python/TS/C#/Go/Rust decorators
          │
          ▼
     schema.json          ← intermediate schema format (this document)
          │
          ▼
fraiseql-cli compile
          │
          ▼
schema.compiled.json      ← consumed by fraiseql-server at runtime
```

The Rust definitions live in
`crates/fraiseql-cli/src/schema/intermediate/` (split across modules).

---

## Versioning

The root object has a `"version"` string field (default `"2.0.0"`). A change is
**breaking** if it removes a field, renames a field, or changes a field's type.
Adding optional fields is non-breaking.

SDK authors should pin to a minor version and test against the current compiler.

---

## Root Object — `IntermediateSchema`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | `string` | no (default `"2.0.0"`) | Schema format version |
| `types` | `IntermediateType[]` | no | GraphQL object types |
| `enums` | `IntermediateEnum[]` | no | GraphQL enum types |
| `input_types` | `IntermediateInputObject[]` | no | GraphQL input object types |
| `interfaces` | `IntermediateInterface[]` | no | GraphQL interface types |
| `unions` | `IntermediateUnion[]` | no | GraphQL union types |
| `queries` | `IntermediateQuery[]` | no | GraphQL query operations |
| `mutations` | `IntermediateMutation[]` | no | GraphQL mutation operations |
| `subscriptions` | `IntermediateSubscription[]` | no | GraphQL subscription operations |
| `fragments` | `IntermediateFragment[]?` | no | Reusable field selections |
| `directives` | `IntermediateDirective[]?` | no | Custom directive definitions |
| `fact_tables` | `IntermediateFactTable[]?` | no | Analytics fact tables |
| `aggregate_queries` | `IntermediateAggregateQuery[]?` | no | Analytics aggregate queries |
| `observers` | `IntermediateObserver[]?` | no | Database change event listeners |
| `custom_scalars` | `IntermediateScalar[]?` | no | Custom scalar type definitions |
| `security` | `object?` | no | Security config (from `fraiseql.toml`) |
| `observers_config` | `object?` | no | Observer backend config |
| `federation_config` | `object?` | no | Apollo Federation config |
| `subscriptions_config` | `SubscriptionsConfig?` | no | WebSocket limits/hooks |
| `validation_config` | `ValidationConfig?` | no | Query depth/complexity limits |
| `debug_config` | `DebugConfig?` | no | Debug/dev configuration |
| `mcp_config` | `McpConfig?` | no | Model Context Protocol config |
| `query_defaults` | `IntermediateQueryDefaults?` | no | Global auto-param defaults |

---

## Core Types

### `IntermediateType`

A GraphQL object type (e.g. `User`, `Post`).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | `string` | yes | Type name, e.g. `"User"` |
| `fields` | `IntermediateField[]` | yes | Fields on this type |
| `description` | `string?` | no | From docstring |
| `implements` | `string[]` | no | Interface names this type implements |
| `requires_role` | `string?` | no | Role required for introspection/access |
| `is_error` | `bool` | no | `true` if tagged with `@fraiseql.error` |
| `relay` | `bool` | no | `true` if this type implements Relay `Node` |

### `IntermediateField`

A single field within a type or input type.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | `string` | yes | Field name, e.g. `"id"` |
| `type` | `string` | yes | Field type name (serialised as `"type"`, not `"field_type"`) |
| `nullable` | `bool` | yes | Whether the field is nullable |
| `description` | `string?` | no | From docstring |
| `directives` | `IntermediateAppliedDirective[]?` | no | Applied directives |
| `requires_scope` | `string?` | no | JWT scope required to access this field |
| `on_deny` | `string?` | no | `"reject"` (default) or `"mask"` when scope check fails |

### `IntermediateEnum` / `IntermediateEnumValue`

```json
{
  "name": "OrderStatus",
  "description": "Possible states of an order",
  "values": [
    { "name": "PENDING" },
    { "name": "SHIPPED", "description": "Package shipped" },
    { "name": "LEGACY", "deprecated": { "reason": "Use SHIPPED" } }
  ]
}
```

### `IntermediateDeprecation`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `reason` | `string?` | no | Deprecation reason message |

### `IntermediateScalar`

Custom GraphQL scalar type.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | `string` | yes | Scalar name, e.g. `"DateTime"` |
| `description` | `string?` | no | Description |
| `validation_rules` | `ValidationRule[]?` | no | Server-side validation rules |
| `coerce_from` | `string[]?` | no | Accepted source types for coercion |
| `serialize_as` | `string?` | no | Wire representation type |

---

## Operations

### `IntermediateQuery`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | `string` | yes | Query name, e.g. `"users"` |
| `return_type` | `string` | yes | Return type name |
| `returns_list` | `bool` | no | `true` for list queries |
| `nullable` | `bool` | no | `true` if result is nullable |
| `arguments` | `IntermediateArgument[]` | no | Query arguments |
| `description` | `string?` | no | From docstring |
| `sql_source` | `string?` | no | Database view/table name |
| `auto_params` | `IntermediateAutoParams?` | no | Auto-generated pagination params |
| `deprecated` | `IntermediateDeprecation?` | no | Deprecation info |
| `jsonb_column` | `string?` | no | JSONB column name (`tv_*` pattern) |
| `relay` | `bool` | no | `true` for Relay connection queries |
| `inject` | `{[col]: source}` | no | Server-injected params (not exposed as args) |
| `cache_ttl_seconds` | `integer?` | no | Per-query cache TTL override |
| `additional_views` | `string[]` | no | Extra views read (for cache invalidation) |
| `requires_role` | `string?` | no | Role required to execute |
| `relay_cursor_type` | `string?` | no | `"uuid"` or `"int64"` (Relay only) |

### `IntermediateMutation`

Similar to `IntermediateQuery` with additional fields:

| Extra Field | Type | Description |
|-------------|------|-------------|
| `operation` | `string?` | `"create"`, `"update"`, `"delete"`, or custom |
| `invalidates` | `string[]` | View names to invalidate from cache after mutation |
| `input_type` | `string?` | Input object type name |

### `IntermediateArgument`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | `string` | yes | Argument name |
| `type` (serialised as `"type"`) | `string` | yes | Argument type name |
| `nullable` | `bool` | yes | Whether argument is optional |
| `description` | `string?` | no | From docstring |
| `default_value` | `any?` | no | Default value for optional args |

### `IntermediateAutoParams`

Controls auto-generated `limit`, `offset`, `order_by`, and `where` arguments.

### `IntermediateQueryDefaults`

Global defaults for auto-params, injected from `[query_defaults]` in `fraiseql.toml`.
Never present in `schema.json` — populated at compile time.

---

## Advanced Types

### `IntermediateInterface`

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Interface name |
| `fields` | `IntermediateField[]` | Interface fields |
| `description` | `string?` | Description |

### `IntermediateUnion`

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Union name |
| `types` | `string[]` | Member type names |
| `description` | `string?` | Description |

### `IntermediateInputObject` / `IntermediateInputField`

Input objects are used as mutation arguments. `IntermediateInputField` is identical
to `IntermediateField` but for input types.

---

## Subscriptions

### `IntermediateSubscription`

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Subscription name |
| `entity_type` | `string` | The entity type being subscribed to |
| `topic` | `string?` | Pub/sub topic name |
| `operation` | `string?` | `"created"`, `"updated"`, `"deleted"`, or custom |
| `filters` | `IntermediateSubscriptionFilter[]?` | Event filters |
| `description` | `string?` | Description |

### `IntermediateObserver`

Database change event listener (triggers from DB → subscription event).

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Observer name |
| `entity` | `string` | Entity type to watch |
| `on_create` / `on_update` / `on_delete` | `IntermediateObserverAction?` | Handlers |
| `retry_config` | `IntermediateRetryConfig?` | Retry policy |

---

## Fragments & Directives

### `IntermediateFragment`

Reusable field selection that can be spread into queries.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Fragment name |
| `on_type` | `string` | The type this fragment applies to |
| `fields` | `IntermediateFragmentField[]` | Field selections |

`IntermediateFragmentField` is an enum: either a plain field name (`Scalar`) or a
nested field selection (`Nested { name, fields }`).

### `IntermediateDirective`

Custom directive definition.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Directive name (without `@`) |
| `locations` | `string[]` | Where directive can be applied |
| `arguments` | `IntermediateArgument[]` | Directive arguments |
| `description` | `string?` | Description |

### `IntermediateAppliedDirective`

An instance of a directive applied to a field or type.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Directive name |
| `arguments` | `{[name]: value}?` | Argument values |

---

## Analytics

### `IntermediateFactTable`

OLAP-style fact table for aggregate queries.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Fact table name |
| `sql_source` | `string` | Underlying view/table |
| `measures` | `IntermediateMeasure[]` | Numeric measures |
| `dimensions` | `IntermediateDimensions?` | Dimension groupings |

### `IntermediateAggregateQuery`

Pre-defined aggregate query (COUNT, SUM, AVG, etc.).

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Query name |
| `fact_table` | `string` | Source fact table |
| `measures` | `string[]` | Measure names to aggregate |
| `group_by` | `IntermediateDimensionPath[]?` | Grouping dimensions |
| `filters` | `IntermediateFilter[]?` | WHERE conditions |

---

## Minimal valid example

```json
{
  "version": "2.0.0",
  "types": [
    {
      "name": "User",
      "fields": [
        { "name": "id",    "type": "Int",    "nullable": false },
        { "name": "email", "type": "String", "nullable": false },
        { "name": "name",  "type": "String", "nullable": true  }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_users"
    }
  ],
  "mutations": [
    {
      "name": "createUser",
      "return_type": "User",
      "operation": "create",
      "sql_source": "create_user",
      "arguments": [
        { "name": "email", "type": "String", "nullable": false },
        { "name": "name",  "type": "String", "nullable": true  }
      ],
      "invalidates": ["v_users"]
    }
  ]
}
```

---

## See also

- `crates/fraiseql-cli/src/schema/intermediate/` — Rust struct definitions (split across modules)
- [../authoring-ir-format.md](../authoring-ir-format.md) — practical guide for code generator authors
- [compiler.md](compiler.md) — how the compiler uses this format
- [overview.md](overview.md) — end-to-end architecture
