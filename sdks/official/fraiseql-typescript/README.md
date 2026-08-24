# FraiseQL v2 - TypeScript Schema Authoring

> Compiled GraphQL execution engine - Schema authoring in TypeScript

FraiseQL v2 is a high-performance GraphQL engine that compiles schemas at build-time for zero-cost query execution. This package provides **schema authoring in TypeScript** that generates JSON schemas consumed by the Rust compiler.

**Key Principle**: TypeScript is for **authoring only** - no runtime FFI, no language bindings. Just pure JSON generation.

## Architecture

```
TypeScript Code (decorators)
         ↓
    schema.json
         ↓
 fraiseql-cli compile
         ↓
 schema.compiled.json
         ↓
 Rust Runtime (fraiseql-server)
```

## Installation

```bash
npm install fraiseql
# or
yarn add fraiseql
# or
pnpm add fraiseql
```

**Requirements**: Node.js 18+

## Quick Start

### 1. Define Types

TypeScript erases types at runtime, so a decorator on a class can see the class's *name*
and nothing else — not its fields, not their GraphQL types, not their nullability. The
authoring surface is therefore explicit:

```typescript
import { registerTypeFields } from "fraiseql";

registerTypeFields(
  "User",
  [
    { name: "id", type: "Int", nullable: false },
    { name: "name", type: "String", nullable: false },
    { name: "email", type: "String", nullable: false },
  ],
  "A user of the system",
  { sqlSource: "v_user" }
);
```

Write an ordinary `interface User { … }` alongside it for your own code; FraiseQL does not
read it.

### 2. Define Queries

```typescript
import { registerQuery } from "fraiseql";

registerQuery(
  "users",
  "User",
  true,    // returns list
  false,   // not nullable
  [
    { name: "limit", type: "Int", nullable: false, default: 10 },
    { name: "offset", type: "Int", nullable: false, default: 0 },
  ],
  "Get all users",
  { sql_source: "v_user" }
);
```

### 3. Define Mutations

```typescript
import { registerMutation } from "fraiseql";

registerMutation(
  "createUser",
  "User",
  false,   // single item
  false,   // not nullable
  [
    { name: "name", type: "String", nullable: false },
    { name: "email", type: "String", nullable: false },
  ],
  "Create a new user",
  { sql_source: "fn_create_user", operation: "insert" }
);
```

### 4. Export Schema

```typescript
// At end of file
if (require.main === module) {
  fraiseql.exportSchema("schema.json");
}
```

### 5. Compile

```bash
# Generate compiled schema
fraiseql-cli compile schema.json

# Start server
fraiseql-server --schema schema.compiled.json --port 3000
```

## API Reference

### Authoring functions

TypeScript erases types at runtime. A decorator on a class or a function therefore cannot
see the field types, argument types or nullability FraiseQL needs, so the authoring surface
is a set of explicit `register*` functions. `@Query`, `@Mutation` and `@Subscription` exist
only to **refuse**: applying one throws, pointing at the function to call instead, rather
than registering a placeholder the compiler would emit as an empty type (#733).

A decorator cannot be applied to a bare `function` in TypeScript at all — that is a syntax
error, not a FraiseQL limitation.

#### `registerTypeFields(name, fields, description?, options?)`

```typescript
registerTypeFields(
  "User",
  [
    { name: "id", type: "ID", nullable: false },
    { name: "email", type: "String", nullable: false },
    { name: "salary", type: "Float", nullable: true, requiresScope: "read:User.salary" },
  ],
  "A user of the system",
  { sqlSource: "v_user", relay: true }
);
```

**Options**: `sqlSource`, `relay`, `jsonbColumn`, `isError`, `requiresRole`, `implements`,
`crud`, `cascade`. A field may carry `description` and `requiresScope` (exactly one scope —
more than one is refused, because the compiled schema and the runtime field filter each
hold a single required scope).

#### `registerQuery(name, returnType, returnsList, nullable, args, description?, config?)`

```typescript
registerQuery(
  "users",
  "User",
  true,   // returnsList
  false,  // nullable
  [{ name: "isActive", type: "Boolean", nullable: true }],
  "List users",
  { sql_source: "v_user", cache_ttl_seconds: 300, requires_role: "admin" }
);
```

The return type may name a type, an enum, a union or an interface declared in the same
document.

#### `registerMutation(name, returnType, returnsList, nullable, args, description?, config?)`

```typescript
registerMutation(
  "createUser",
  "User",
  false,
  false,
  [{ name: "email", type: "String", nullable: false }],
  "Create a user",
  {
    sql_source: "fn_create_user",
    operation: "insert",           // insert | update | delete
    inject_params: { tenant_id: "jwt:tenant_id" },
    invalidates_views: ["v_user"],
    invalidates_fact_tables: ["tf_signup"],
  }
);
```

#### `enum_(name, values, config?)` · `input(name, fields, config?)` · `union(name, members, config?)` · `interface_(name, fields, config?)`

Declare the remaining type kinds. Each takes the name first and returns the registered
definition.

#### `SchemaRegistry.registerFactTable(tableName, measures, dimensions, denormalizedFilters)`

Declare an analytics fact table. This is a `SchemaRegistry` static, not a top-level export
and not a decorator. Aggregate queries are **not** declared in `schema.json` — the compiler
refuses an `aggregate_queries` block (#956); declare each as an `[[analytics.queries]]`
entry in `fraiseql.toml` instead, where it becomes an ordinary view-backed query.

#### `@Subscription(config?)`

Mark a function as a GraphQL subscription for real-time events.

Subscriptions in FraiseQL are **compiled database event projections** sourced from LISTEN/NOTIFY or CDC, not resolver-based.

```typescript
registerSubscription(
  "orderCreated",
  "Order",                                        // entity type == return type
  [{ name: "userId", type: "ID", nullable: true }],
  "Fires when an order is created",
  {
    topic: "order_events",
    filter: { conditions: [{ argument: "userId", path: "$.user_id" }] },
  }
);
```

### Subscription Configuration

**SubscriptionOptions**:

- `topic`: Optional LISTEN/NOTIFY channel or CDC topic
- `filter`: `{ conditions: [{ argument, path }] }` — maps the subscription's own
  arguments onto JSON paths in the event payload
- `fields`: Subset of event fields to deliver; every field if omitted
- `deprecated`: `true`, or the reason as a string

The set is **closed**. It used to be an open `Record<string, unknown>` spread into the
emitted definition, so `operation: "CREATE"` — or a typo — travelled to `fraiseql compile`
and failed the whole document there, naming a key that appeared in no SDK type (#1024).
There is no `nullable` and no `operation`/`operations`: the runtime subscription model has
neither. Where a DML-verb filter is wanted, the event payload carries the verb and a
`filter` condition selects on it.

**Subscription Patterns**:

1. **Event Filtering** - Narrow by the event's own payload

```typescript
fraiseql.registerSubscription(
  "userEventsByVerb",
  "User",
  [{ name: "verb", type: "String", nullable: true }],
  "User events of one kind",
  { filter: { conditions: [{ argument: "verb", path: "$.op" }] } }
);
```

1. **Topic-Based Subscriptions** - Route to different channels

```typescript
fraiseql.registerSubscription(
  "criticalOrders",
  "Order",
  [],
  "High-priority orders",
  { topic: "orders.critical" }
);
```

1. **Filtered Subscriptions** - Target specific records

```typescript
fraiseql.registerSubscription(
  "customerOrders",
  "Order",
  [{ name: "customerId", type: "ID", nullable: false }],
  "Orders for specific customer",
  { filter: { conditions: [{ argument: "customerId", path: "$.customer_id" }] } }
);
```

1. **Field Projection** - Keep the stream narrow

```typescript
fraiseql.registerSubscription(
  "orderTotals",
  "Order",
  [],
  "Just the amounts",
  { topic: "orders", fields: ["id", "totalAmount"] }
);
```

1. **Change Data Capture (CDC)** - Capture all changes

```typescript
fraiseql.registerSubscription("userCDC", "User", [], "All user changes", {
  topic: "cdc",
});
```

### Type System Decorators

#### `enum_(name, values, config?)`

Define a GraphQL enum type.

```typescript
const OrderStatus = fraiseql.enum_("OrderStatus", {
  PENDING: "pending",
  SHIPPED: "shipped",
  DELIVERED: "delivered",
}, {
  description: "Status of an order"
});
```

Then use in types:

```typescript
fraiseql.registerTypeFields("Order", [
  { name: "id", type: "ID", nullable: false },
  { name: "status", type: "OrderStatus", nullable: false },
]);
```

#### `interface_(name, fields, config?)`

Define a GraphQL interface - shared fields for multiple types.

```typescript
const Node = fraiseql.interface_("Node", [
  { name: "id", type: "ID", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
], {
  description: "An object with a globally unique ID"
});
```

Types can implement interfaces:

```typescript
fraiseql.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false },
  { name: "createdAt", type: "DateTime", nullable: false },
  { name: "name", type: "String", nullable: false },
]);
```

#### `union(name, memberTypes, config?)`

Define a GraphQL union - polymorphic return type.

```typescript
const SearchResult = fraiseql.union("SearchResult",
  ["User", "Post", "Comment"],
  { description: "Result of a search query" }
);
```

Then use in queries:

```typescript
fraiseql.registerQuery(
  "search",
  "SearchResult",  // Returns union
  true,            // returns list
  false,           // not nullable
  [{ name: "query", type: "String", nullable: false }],
  "Search across content"
);
```

#### `input(name, fields, config?)`

Define a GraphQL input type - structured parameters.

```typescript
const CreateUserInput = fraiseql.input("CreateUserInput", [
  { name: "email", type: "Email", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "role", type: "String", nullable: false, default: "user" },
], {
  description: "Input for creating a new user"
});
```

Use in mutations:

```typescript
fraiseql.registerMutation(
  "createUser",
  "User",
  false,
  false,
  [{ name: "input", type: "CreateUserInput", nullable: false }],
  "Create a new user"
);
```

### Field-Level Metadata

Add access control, deprecation markers, and documentation to individual fields:

#### `field(options)`

Create field metadata for use with `registerTypeFields()`:

```typescript
fraiseql.registerTypeFields("User", [
  { name: "id", type: "ID", nullable: false },
  {
    name: "salary",
    type: "Decimal",
    nullable: false,
    requiresScope: "read:User.salary",
    description: "Annual salary (requires HR scope)"
  },
  {
    name: "oldEmail",
    type: "String",
    nullable: true,
    deprecated: "Use email instead",
    description: "Legacy email field (deprecated)"
  }
]);
```

**Field Metadata Options**:

- `requiresScope: string | string[]` - JWT scope(s) required to access this field (field-level access control)
- `deprecated: boolean | string` - Mark field as deprecated. Pass a string with migration guidance.
- `description: string` - Field documentation (appears in GraphQL schema)

**Use Cases**:

1. **PII Protection**: Require specific scopes for sensitive fields

```typescript
{
  name: "ssn",
  type: "String",
  nullable: false,
  requiresScope: "pii:read"  // Only users with pii:read scope can query this
}
```

1. **API Versioning**: Deprecate fields with migration guidance

```typescript
{
  name: "oldPrice",
  type: "Decimal",
  nullable: true,
  deprecated: "Use pricing.current instead - structure moved to pricing object"
}
```

1. **Schema Documentation**: Add rich field descriptions

```typescript
{
  name: "discount",
  type: "Decimal",
  nullable: false,
  description: "Discount percentage. Access requires orders:view_discounts scope.",
  requiresScope: "orders:view_discounts"
}
```

### Manual Registration Functions

When decorators alone don't provide enough type information:

#### `registerTypeFields(typeName, fields, description?)`

Register type field definitions.

```typescript
fraiseql.registerTypeFields("User", [
  { name: "id", type: "Int", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "email", type: "String", nullable: true },
]);
```

#### `registerQuery(name, returnType, returnsList, nullable, args, description?, config?)`

Register a query with full metadata.

```typescript
fraiseql.registerQuery(
  "users",
  "User",
  true,      // returns list
  false,     // not nullable
  [
    { name: "limit", type: "Int", nullable: false, default: 10 },
  ],
  "Get all users",
  { sql_source: "v_user" }
);
```

#### `registerMutation(name, returnType, returnsList, nullable, args, description?, config?)`

Register a mutation with full metadata.

```typescript
fraiseql.registerMutation(
  "createUser",
  "User",
  false,     // single item
  false,     // not nullable
  [
    { name: "name", type: "String", nullable: false },
  ],
  "Create a new user",
  { sql_source: "fn_create_user", operation: "insert" }
);
```

### Schema Export

#### `exportSchema(outputPath, options?)`

Export the schema to a JSON file.

```typescript
fraiseql.exportSchema("schema.json", { pretty: true });
```

#### `getSchemaDict()`

Get the schema as a JavaScript object.

```typescript
const schema = fraiseql.getSchemaDict();
console.log(schema.types);
console.log(schema.queries);
```

#### `exportSchemaToString(options?)`

Export schema to a JSON string.

```typescript
const json = fraiseql.exportSchemaToString({ pretty: true });
console.log(json);
```

## Supported GraphQL Types

### Scalars

- `Int` - 32-bit integer
- `Float` - Floating point number
- `String` - Text string
- `Boolean` - True/False
- `ID` - Unique identifier

### Modifiers

- `T[]` - List type (maps to `[T!]` in GraphQL)
- `T | null` - Nullable type
- `T | undefined` - Optional parameter

## Type Mapping

TypeScript types are converted to GraphQL types:

```typescript
// TypeScript    →  GraphQL
number          →  Float
string          →  String
boolean         →  Boolean
SomeClass       →  SomeClass (custom type)
T[]             →  [T!]      (list)
T | null        →  T         (nullable)
T | undefined   →  T         (optional param)
```

## Analytics Features

### Fact Tables

Fact tables are special analytics tables with:

- **Measures**: Numeric columns for aggregation (SUM, AVG, COUNT)
- **Dimensions**: JSONB column for flexible GROUP BY
- **Denormalized Filters**: Indexed columns for fast WHERE clauses

```typescript
import { SchemaRegistry } from "fraiseql";

SchemaRegistry.registerFactTable(
  "tf_sale",                                       // must start with "tf_"
  [
    { name: "revenue", sql_type: "Float", nullable: false },
    { name: "cost", sql_type: "Float", nullable: false },
  ],
  {
    name: "data",
    paths: [{ name: "category", json_path: "data->>'category'", data_type: "text" }],
  },
  [{ name: "occurred_at", sql_type: "Timestamp", indexed: true }]
);
```

A mutation keeps the facts current by declaring `invalidates_fact_tables: ["tf_sale"]`.

### Aggregate Queries

Queries that perform GROUP BY aggregations on fact tables. **They are not declared in
`schema.json`**: the compiler refuses an `aggregate_queries` block outright, because no
compile path lowered it into the compiled schema (#956). Declare each one in
`fraiseql.toml`, where it becomes an ordinary list-returning, view-backed query:

```toml
[[analytics.queries]]
name = "salesSummary"
fact_table = "tf_sale"
auto_group_by = true
auto_aggregates = true
```

These queries support:

- `groupBy`: Dimensions and temporal buckets
- `aggregates`: COUNT, SUM, AVG, MIN, MAX
- `where`: Pre-aggregation filters
- `having`: Post-aggregation filters
- `orderBy`: Sort results
- Pagination: `limit`, `offset`

## Examples

See the `examples/` directory:

- **basic_schema.ts** - Simple CRUD queries and mutations
- **analytics_schema.ts** - Fact tables
- **enums-example.ts** - Enum definitions and usage
- **types-advanced.ts** - Comprehensive type system example (enums, interfaces, unions, input types)
- **unions-interfaces-example.ts** - Interfaces, unions, and polymorphic queries
- **field-metadata.ts** - Field-level access control and documentation
- **subscriptions.ts** - Real-time subscriptions: event filtering, topics, CDC, alerts
- **ecommerce_with_observers.ts** - Observers reacting to database changes
- **scheduled_source.ts** - A scheduled ingress source and its Deno connector

Run examples:

```bash
npm run example:basic         # Generate basic schema
npm run example:analytics     # Generate analytics schema
npm run example:enums         # Generate enum example
npm run example:advanced      # Generate advanced types example
npm run example:metadata      # Generate field metadata example
npm run example:subscriptions # Generate subscriptions example
```

## Development

```bash
# Install dependencies
npm install

# Build
npm run build

# Run tests
npm test

# Watch mode
npm run test:watch

# Lint
npm run lint

# Format code
npm run format
```

## Testing

Tests verify:

- Type introspection and conversion
- Schema registration and retrieval
- Decorator functionality
- Schema JSON generation
- Analytics fact tables and aggregate queries

```bash
npm test
```

## Troubleshooting

### Issue: "Field type information not available"

**Cause**: TypeScript doesn't preserve type information at runtime by default.

**Solution**: Use `registerTypeFields()` or `registerQuery()`/`registerMutation()` with explicit type metadata.

```typescript
// Instead of relying on decorators alone:
fraiseql.registerTypeFields("User", [
  { name: "id", type: "Int", nullable: false },
  // ... other fields
]);
```

### Issue: "Factory not started: fraiseql-cli not found"

**Solution**: Install the CLI tool:

```bash
# Global installation
npm install -g fraiseql-cli

# Or use local version
npx fraiseql-cli compile schema.json
```

## Performance

- **Compile-time**: Negligible (< 100ms for typical schemas)
- **Runtime**: Zero overhead - SQL is compiled, not interpreted
- **Schema generation**: Fast JSON serialization

## Architecture Notes

### No Runtime FFI

This package generates **JSON only**. There's no FFI, no native bindings, no runtime dependencies on the Rust engine.

The workflow is:

1. Write TypeScript with decorators
2. Run `exportSchema()` to generate `schema.json`
3. Compile with `fraiseql-cli` to get `schema.compiled.json`
4. Deploy compiled schema to Rust runtime

### Why Manual Field Registration?

TypeScript's decorator system doesn't preserve generic type parameters at runtime. To provide full type information, we require explicit field registration. This is a limitation of the language, not the framework.

Future versions may use TypeScript 5.2+ metadata if decorators mature in the standard.

## License

MIT

## Support

- **Documentation**: <https://docs.fraiseql.io>
- **Issues**: <https://github.com/fraiseql/fraiseql/issues>
- **Examples**: See `examples/` directory

## Contributing

Contributions welcome! Please follow the contribution guidelines in the main repository.

---

**Remember**: FraiseQL TypeScript is for **authoring only**. Runtime execution happens in the Rust engine.
