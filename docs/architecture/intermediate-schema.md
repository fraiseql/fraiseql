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

The Rust definition lives in `crates/fraiseql-cli/src/schema/intermediate/`, and
`crates/fraiseql-cli/src/schema/seam.rs` is the single list of authorable top-level
sections that every loader and the TOML merger consume.

## This document is normative, and it is enforced

`IntermediateSchema` and every struct below it carry `#[serde(deny_unknown_fields)]`. A
key this document does not list is **not** ignored — it fails the compile with a message
naming the key and listing the accepted ones. That is deliberate: for two years the
combination of `#[serde(default)]` and no `deny_unknown_fields` meant a misspelled or
renamed key bound to an empty default and `fraiseql compile` reported success, which is
the mechanism behind nine separate shipped defects.

This document was itself one of the producers. It previously documented `inject` where
the compiler reads `inject_params`, and `invalidates` where the compiler reads
`invalidates_views` — and the PHP SDK emitted exactly those two keys, so every
PHP-authored mutation compiled with no cache invalidation and no injected predicate.
Its "minimal valid example" did not compile.

**The suite that keeps this honest** is `sdks/official/conformance/`: a canonical schema
authored through each of the eleven SDKs' public APIs, compiled by the real CLI, and
compared on what survived. `reference/full.json` and `reference/minimal.json` there are
the worked examples, and they are compiled on every CI run — so an example in this
document cannot rot into one that does not compile.

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
| `sources` | `SourceDefinition[]?` | no | Scheduled ingress sources (#573) |
| `inject_defaults` | `IntermediateInjectDefaults?` | no | Project-wide default injected params |
| `naming_convention` | `string` | no | `camelCase` (default) or `snake_case` |
| `session_variables` | `SessionVariablesConfig?` | no | Per-request `set_config()` injection |
| `rest_config` | `RestConfig?` | no | REST transport config (from `[rest]`) |
| `grpc_config` | `GrpcConfig?` | no | gRPC transport config (from `[grpc]`) |
| `hierarchies_config` | `HierarchiesConfig?` | no | ltree hierarchy definitions |
| `changelog_config` | `ChangelogConfig?` | no | Change-log GraphQL exposure |
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
| `sql_source` | `string?` | no | Backing view; normally bound on the *query*, needed here only for an owner-split federation entity |
| `inject_params` | `{[col]: source}` | no | Tenant/owner scoping for an entity **no query returns** (#1142). Same value shapes as the operation-level key. Consulted only on the `_entities` path, behind the backing query; see below |
| `is_error` | `bool` | no | `true` if tagged with `@fraiseql.error` |
| `is_input` | `bool` | no | `true` to declare a GraphQL **input** object; the compiler moves it into `input_types` and refuses `sql_source`/`relay`/`is_error`/`requires_role`/`implements` alongside it |
| `relay` | `bool` | no | `true` if this type implements Relay `Node`. Load-bearing: it synthesizes the `Node` interface and the `Edge`/`Connection`/`PageInfo` types |
| `embedded` | `bool` | no | `true` for a value object with no independent identity (#687) |
| `subscribable_tables` | `string[]?` | no | Base tables whose external writes feed this type's subscriptions (#366) |
| `subscribable_pre_image` | `bool` | no | Whether those capture triggers also record the pre-image |

#### Scoping an entity no query returns (#1142)

Tenant/owner scoping normally rides on the operation: a query declares `inject_params`, and
every read through it carries the predicate. A federation entity resolved only through
`_entities` — its relation supplied by the type-level `sql_source` above — has no such
operation, so before this key its author could declare `requires_role` (honoured from the
type) but had nowhere to declare tenant scoping. The compile succeeded and the annotation
covered nothing on that path.

Declaring `inject_params` on the type closes that. Both `_entities` consumers read it —
the gate that refuses an anonymous caller, and the builder that composes the per-row
`"tenant_id" = $N` predicate — each *behind* the backing query, so a query-backed entity is
unaffected.

Two rules follow from the merge:

- A type and its backing query may declare the same column, but only from the **same**
  source. Two different sources for one column is refused when the compiled schema loads,
  naming the query, the type, the column and both sources.
- With `[fraiseql.tenancy] mode = "row"`, a `@tenant_id`-annotated type that **no query
  returns** has the scoping auto-injected onto the type, exactly as a query would get it.
  A type a query returns is left alone — the query already carries it.

### `IntermediateField`

A single field within a type or input type.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | `string` | yes | Field name, e.g. `"id"` |
| `type` | `string` | yes | Field type name (serialised as `"type"`, not `"field_type"`) |
| `nullable` | `bool` | yes | Whether the field is nullable |
| `description` | `string?` | no | From docstring |
| `directives` | `IntermediateAppliedDirective[]?` | no | Applied directives |
| `requires_scope` | `string?` | no | JWT scope required to access this field. **Exactly one** — there is no `requires_scopes`; a multi-scope declaration cannot be honoured and is refused by the SDKs |
| `on_deny` | `string?` | no | `"reject"` (default) or `"mask"` when scope check fails |

There is **no** `computed` and **no** `deprecated` on a field. `computed` is an
authoring-time flag that an SDK's own CRUD generator consumes before export; field
deprecation has no member here (only enum values and input fields carry one). Emitting
either now fails the compile.

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
| `inject_params` | `{[col]: source}` | no | Server-injected params (not exposed as args). Values are `"jwt:<claim>"` or `{"source": "jwt", "claim": "<claim>"}`. **The key is `inject_params`, not `inject`** |
| `rest` | `{path, method?}?` | no | REST route override for this operation |
| `cache_ttl_seconds` | `integer?` | no | Per-query cache TTL override |
| `additional_views` | `string[]` | no | Extra views read (for cache invalidation) |
| `requires_role` | `string?` | no | Role required to execute |
| `relay_cursor_type` | `string?` | no | `"uuid"` or `"int64"` (Relay only) |

### `IntermediateMutation`

Similar to `IntermediateQuery` with additional fields:

| Extra Field | Type | Description |
|-------------|------|-------------|
| `operation` | `string?` | `CREATE`/`INSERT`, `UPDATE`, `DELETE` or `CUSTOM`. Matched **case-insensitively**, so `"insert"` and `"INSERT"` are the same verb; anything else is a hard error |
| `invalidates_views` | `string[]` | Views whose cached results are invalidated after this mutation. **The key is `invalidates_views`, not `invalidates`** |
| `invalidates_fact_tables` | `string[]` | Fact tables whose cached aggregates are invalidated. No inference fallback — an aggregate is only ever invalidated from this list |
| `changelog` | `bool` | Whether to write a Change-Spine row; defaults to `true` |
| `changelog_pre_image` | `bool` | Whether to record the entity's before-state |
| `input_style` | `string` | `"flatten"` (default) or `"jsonb"` |
| `cascade` | `bool` | Whether the success payload exposes the typed `cascade` field |

There is **no** `input_type`. A mutation names its input object in an argument's `type`.

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
| `return_type` | `string` | The entity type being subscribed to |
| `arguments` | `IntermediateArgument[]` | Arguments the client may supply (default `[]`) |
| `topic` | `string?` | Pub/sub topic name |
| `filter` | `IntermediateSubscriptionFilter?` | Event filter |
| `fields` | `string[]` | Fields to project from the event payload; all if omitted |
| `description` | `string?` | Description |
| `deprecated` | `IntermediateDeprecation?` | Deprecation info |

This struct denies unknown fields, so a key that is not in this table fails the **whole
document** at `fraiseql compile` rather than being ignored. In particular there is no
`entity_type`, no `filters` (it is singular) and no `operation` — the runtime subscription
model has no operation concept, and emitting one was #1024.

### `IntermediateSubscriptionFilter`

| Field | Type | Description |
|-------|------|-------------|
| `conditions` | `IntermediateFilterCondition[]` | Argument-to-payload-path bindings |

Each condition is `{"argument": string, "path": string}`, where `argument` is a
**reference to one of the subscription's own `arguments`** and `path` is a JSON pointer
into the event payload.

⚠ Spell `argument` exactly as the subscription declares it — the *translated* GraphQL
name (`orderId`), not the authoring-language parameter (`order_id`). A reference that
names no declared argument is a compile error (#1262). It used to be accepted and then
skipped at runtime, because the delivery loop cannot distinguish "this argument does not
exist" from "the client did not supply this optional argument", which it skips by design
— so the filter was not applied at all and the subscription delivered every event on its
topic.

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

## Worked examples

Both of these are the checked-in conformance fixtures, compiled on every CI run — so an
example here cannot rot into one that does not compile. Their canonical copies live at
`sdks/official/conformance/reference/`.

### Minimal

The smallest schema that compiles. Note what is **absent**: a section with no entries is
omitted entirely. It must never be emitted as `null` — the compiler hands an explicit
`null` to `Vec::deserialize` and fails with `invalid type: null, expected a sequence` and
no indication of which key is at fault.

```json
{
  "version": "2.0.0",
  "types": [
    {
      "name": "User",
      "sql_source": "v_user",
      "fields": [
        { "name": "id", "type": "ID", "nullable": false },
        { "name": "email", "type": "String", "nullable": false }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_user",
      "arguments": []
    }
  ]
}
```

### Full

Every construct the conformance suite holds the SDKs to.

```json
{
  "version": "2.0.0",
  "types": [
    {
      "name": "User",
      "sql_source": "v_user",
      "relay": true,
      "fields": [
        { "name": "id", "type": "ID", "nullable": false },
        { "name": "email", "type": "String", "nullable": false },
        { "name": "name", "type": "String", "nullable": true, "description": "The user's \"display\" name" },
        { "name": "salary", "type": "Float", "nullable": true, "requires_scope": "read:User.salary" }
      ]
    },
    {
      "name": "Order",
      "sql_source": "v_order",
      "fields": [
        { "name": "id", "type": "ID", "nullable": false },
        { "name": "total", "type": "Float", "nullable": false },
        { "name": "status", "type": "String", "nullable": false }
      ]
    },
    {
      "name": "UserNotFound",
      "sql_source": "v_user_not_found",
      "is_error": true,
      "fields": [
        { "name": "message", "type": "String", "nullable": false },
        { "name": "code", "type": "String", "nullable": false }
      ]
    }
  ],
  "enums": [
    {
      "name": "OrderStatus",
      "values": [
        { "name": "PENDING" },
        { "name": "SHIPPED" },
        { "name": "CANCELLED" }
      ]
    }
  ],
  "input_types": [
    {
      "name": "CreateUserInput",
      "fields": [
        { "name": "email", "type": "String", "nullable": false },
        { "name": "name", "type": "String", "nullable": true }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "return_type": "User",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_user",
      "arguments": []
    },
    {
      "name": "user",
      "return_type": "User",
      "returns_list": false,
      "nullable": true,
      "sql_source": "v_user",
      "arguments": [
        { "name": "id", "type": "ID", "nullable": false }
      ]
    },
    {
      "name": "tenantOrders",
      "return_type": "Order",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_order",
      "arguments": [],
      "inject_params": { "tenant_id": "jwt:tenant_id" },
      "cache_ttl_seconds": 300,
      "requires_role": "admin"
    }
  ],
  "mutations": [
    {
      "name": "createUser",
      "return_type": "User",
      "returns_list": false,
      "nullable": false,
      "sql_source": "fn_create_user",
      "operation": "INSERT",
      "arguments": [
        { "name": "email", "type": "String", "nullable": false },
        { "name": "name", "type": "String", "nullable": true }
      ],
      "invalidates_views": ["v_user", "v_user_summary"],
      "invalidates_fact_tables": ["tf_signup"]
    },
    {
      "name": "placeOrder",
      "return_type": "Order",
      "returns_list": false,
      "nullable": false,
      "sql_source": "fn_place_order",
      "operation": "INSERT",
      "arguments": [],
      "inject_params": { "user_id": "jwt:sub" },
      "invalidates_views": ["v_order_summary"],
      "invalidates_fact_tables": ["tf_sale"]
    }
  ]
}
```

---

## Conventions, settled once

These are the answers an SDK author needs before writing an exporter. Where an SDK's own
idiom conflicts with one, **the SDK adapts** — the compiler is the fixed point.

| Question | Answer |
|---|---|
| Section containers | Arrays of objects, each carrying its own `name`. Never a map keyed by name. |
| Key casing | `snake_case`, everywhere, including nested blocks. |
| Empty sections | Omit the key, or emit `[]`. **Never `null`.** |
| Nullability | An explicit `nullable` boolean on every field and argument. Never a `!` suffix on the type string. |
| Type strings | The bare GraphQL name (`String`, `User`), or a list wrapper (`[User!]`). `returns_list` is a separate boolean on operations, not `[...]` around `return_type`. |
| Unknown keys | Rejected. Every struct denies unknown fields. |
| Booleans that default false | Omit when false, so a schema not using the feature is byte-identical to one authored before it existed. |
| Mutation verbs | Case-insensitive within the closed set; an unrecognized verb is an error, not a fallback to `CUSTOM`. |

## See also

- `crates/fraiseql-cli/src/schema/intermediate.rs` — Rust struct definitions
- `docs/architecture/overview.md` — end-to-end architecture
