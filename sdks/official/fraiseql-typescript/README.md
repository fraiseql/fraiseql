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

```typescript
import * as fraiseql from "fraiseql";

@fraiseql.type()
class User {
  id!: number;
  name!: string;
  email!: string;
}

// Register fields (TypeScript doesn't preserve type info at runtime)
fraiseql.registerTypeFields("User", [
  { name: "id", type: "Int", nullable: false },
  { name: "name", type: "String", nullable: false },
  { name: "email", type: "String", nullable: false },
]);
```

### 2. Define Queries

```typescript
@fraiseql.query({ sqlSource: "v_user" })
function users(limit: number = 10, offset: number = 0): User[] {
  throw new Error("Not executed");
}

fraiseql.registerQuery(
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
@fraiseql.mutation({ sqlSource: "fn_create_user", operation: "CREATE" })
function createUser(name: string, email: string): User {
  throw new Error("Not executed");
}

fraiseql.registerMutation(
  "createUser",
  "User",
  false,   // single item
  false,   // not nullable
  [
    { name: "name", type: "String", nullable: false },
    { name: "email", type: "String", nullable: false },
  ],
  "Create a new user",
  { sql_source: "fn_create_user", operation: "CREATE" }
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

### Decorators

#### `@Type(config?)`

Mark a class as a GraphQL type.

```typescript
@fraiseql.type()
class User {
  id!: number;
  name!: string;
}
```

**Note**: Decorators alone don't capture field types. Use `registerTypeFields()` to provide field metadata.

#### `@Query(config)`

Mark a function as a GraphQL query.

```typescript
@fraiseql.query({ sqlSource: "v_user" })
function users(limit: number = 10): User[] {
  throw new Error("Not executed");
}
```

**Config Options**:

- `sqlSource`: SQL view/table name (required for data operations)
- `autoParams`: Auto-parameter configuration
- Other custom configuration

#### `@Mutation(config)`

Mark a function as a GraphQL mutation.

```typescript
@fraiseql.mutation({ sqlSource: "fn_create_user", operation: "CREATE" })
function createUser(name: string): User {
  throw new Error("Not executed");
}
```

**Config Options**:

- `sqlSource`: SQL function name (required)
- `operation`: "CREATE" | "UPDATE" | "DELETE" | "CUSTOM"
- Other custom configuration

#### `@FactTable(config)`

Mark a class as a fact table for analytics.

```typescript
@fraiseql.FactTable({
  tableName: "tf_sales",
  measures: ["revenue", "quantity"],
  dimensionPaths: [
    {
      name: "category",
      json_path: "data->>'category'",
      data_type: "text",
    },
  ],
})
@fraiseql.type()
class Sale {
  id!: number;
  revenue!: number;
  quantity!: number;
}
```

#### `@AggregateQuery(config)`

Mark a function as an aggregate query on a fact table.

```typescript
@fraiseql.AggregateQuery({
  factTable: "tf_sales",
  autoGroupBy: true,
  autoAggregates: true,
})
@fraiseql.query()
function salesAggregate(): Record<string, unknown>[] {
  throw new Error("Not executed");
}
```

#### `@Subscription(config?)`

Mark a function as a GraphQL subscription for real-time events.

Subscriptions in FraiseQL are **compiled database event projections** sourced from LISTEN/NOTIFY or CDC, not resolver-based.

```typescript
@fraiseql.Subscription({ topic: "order_events" })
function orderCreated(userId?: string): Order {
  pass;
}
```

### Subscription Configuration

**SubscriptionConfig Options**:

- `entityType`: Entity type being subscribed to (defaults to return type)
- `topic`: Optional topic/channel name for filtering events
- `operation`: Single event type filter - "CREATE" | "UPDATE" | "DELETE"
- `operations`: Multiple event type filters - ["CREATE", "UPDATE", "DELETE"]

**Manual Registration**:

```typescript
fraiseql.registerSubscription(
  "orderCreated",        // name
  "Order",               // entityType
  false,                 // nullable
  [
    { name: "userId", type: "String", nullable: true }
  ],                     // filter arguments
  "Subscribe to new orders",
  { topic: "order_events", operation: "CREATE" }
);
```

**Subscription Patterns**:

1. **Event Type Filtering** - Subscribe to specific operations

```typescript
fraiseql.registerSubscription(
  "userCreated",
  "User",
  false,
  [],
  "New user registrations",
  { operation: "CREATE" }  // Only CREATE events
);
```

2. **Topic-Based Subscriptions** - Route to different channels

```typescript
fraiseql.registerSubscription(
  "criticalOrders",
  "Order",
  false,
  [],
  "High-priority orders",
  { topic: "orders.critical", operation: "CREATE" }
);
```

3. **Filtered Subscriptions** - Target specific records

```typescript
fraiseql.registerSubscription(
  "customerOrders",
  "Order",
  false,
  [{ name: "customerId", type: "ID", nullable: false }],  // Filter by customer
  "Orders for specific customer"
);
```

4. **Change Data Capture (CDC)** - Capture all changes

```typescript
fraiseql.registerSubscription(
  "userCDC",
  "User",
  false,
  [],
  "All user changes",
  { operations: ["CREATE", "UPDATE", "DELETE"] }
);
```

5. **Alerts and Notifications** - Complex filtering

```typescript
fraiseql.registerSubscription(
  "unusualOrders",
  "Order",
  false,
  [
    { name: "minAmount", type: "Decimal", nullable: false },
    { name: "timeWindowMinutes", type: "Int", nullable: true }
  ],
  "Alert on high-value orders",
  { operation: "CREATE" }
);
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

2. **API Versioning**: Deprecate fields with migration guidance

```typescript
{
  name: "oldPrice",
  type: "Decimal",
  nullable: true,
  deprecated: "Use pricing.current instead - structure moved to pricing object"
}
```

3. **Schema Documentation**: Add rich field descriptions

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
  { sql_source: "fn_create_user", operation: "CREATE" }
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
@fraiseql.FactTable({
  tableName: "tf_sales",           // Must start with "tf_"
  measures: ["revenue", "cost"],   // Numeric columns
  dimensionPaths: [
    {
      name: "category",
      json_path: "data->>'category'",
      data_type: "text",
    },
  ],
})
@fraiseql.type()
class Sale {
  id!: number;
  revenue!: number;
  cost!: number;
  customerId!: string;
}
```

### Aggregate Queries

Queries that perform GROUP BY aggregations on fact tables:

```typescript
@fraiseql.AggregateQuery({
  factTable: "tf_sales",
  autoGroupBy: true,       // Auto-generate groupBy fields
  autoAggregates: true,    // Auto-generate aggregate functions
})
@fraiseql.query()
function salesSummary(): Record<string, unknown>[] {
  throw new Error("Not executed");
}
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
- **analytics_schema.ts** - Fact tables and aggregate queries
- **enums-example.ts** - Enum definitions and usage
- **types-advanced.ts** - Comprehensive type system example (enums, interfaces, unions, input types)
- **unions-interfaces-example.ts** - Interfaces, unions, and polymorphic queries
- **field-metadata.ts** - Field-level access control, deprecation, and documentation
- **subscriptions.ts** - Real-time subscriptions: event filtering, topics, CDC, alerts
- **comprehensive-example.ts** - Full-featured schema with all FraiseQL capabilities

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
