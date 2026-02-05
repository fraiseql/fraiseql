# FraiseQL TypeScript SDK Reference

**Status**: Production-Ready | **Version**: 2.0.0 | **Node.js**: 18+ | **TypeScript**: 5.0+

Complete API reference for the FraiseQL TypeScript SDK. Provides decorators and utilities for defining GraphQL schemas that compile to optimized SQL. TypeScript authoring only—no runtime FFI or native bindings.

## Installation

```bash
# npm
npm install fraiseql

# yarn
yarn add fraiseql

# pnpm
pnpm add fraiseql
```

**Requirements:**
- Node.js 18 or higher
- TypeScript 5.0+ (strict mode recommended)
- Decorators enabled in `tsconfig.json`:

```json
{
  "compilerOptions": {
    "experimentalDecorators": true,
    "target": "ES2020",
    "module": "commonjs",
    "strict": true,
    "moduleResolution": "node"
  }
}
```

## Quick Reference Table

| Feature | Method | Purpose |
|---------|--------|---------|
| **Types** | `@fraiseql.type()` | Define GraphQL type |
| **Queries** | `@fraiseql.query()` | Read operations (SELECT) |
| **Mutations** | `@fraiseql.mutation()` | Write operations (CREATE/UPDATE/DELETE) |
| **Subscriptions** | `@fraiseql.Subscription()` | Real-time events |
| **Fact Tables** | `@fraiseql.FactTable()` | Analytics tables with measures/dimensions |
| **Aggregates** | `@fraiseql.AggregateQuery()` | GROUP BY aggregations |
| **Enums** | `fraiseql.enum_()` | GraphQL enum type |
| **Interfaces** | `fraiseql.interface_()` | Shared field contracts |
| **Unions** | `fraiseql.union()` | Polymorphic return types |
| **Input Types** | `fraiseql.input()` | Structured parameters |
| **Field Metadata** | `requiresScope`, `deprecated` | Field-level features |
| **Schema Export** | `fraiseql.exportSchema()` | Generate schema.json |

## Type System

### Basic Type Definition

```typescript
import * as fraiseql from 'fraiseql';

@fraiseql.type()
class User {
  id!: number;
  name!: string;
  email!: string;
  isActive!: boolean;
}

// Register field metadata (required—TypeScript doesn't preserve runtime type info)
fraiseql.registerTypeFields('User', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'name', type: 'String', nullable: false },
  { name: 'email', type: 'Email', nullable: false },
  { name: 'isActive', type: 'Boolean', nullable: false },
]);
```

### Nullable and Optional Types

```typescript
// Nullable (can be null in GraphQL response)
{ name: 'middleName', type: 'String', nullable: true }  // Returns String | null

// Optional parameter (can be omitted in GraphQL query)
{ name: 'limit', type: 'Int', nullable: false, default: 10 }  // Defaults to 10

// Both nullable and optional
{ name: 'description', type: 'String', nullable: true }  // Can be null or omitted
```

### Generic Types and Arrays

```typescript
// Array types
{ name: 'tags', type: 'String', nullable: false, isList: true }     // [String!]
{ name: 'scores', type: 'Float', nullable: true, isList: true }     // [Float]

// Nested types
@fraiseql.type()
class Post {
  id!: number;
  author!: User;
}

fraiseql.registerTypeFields('Post', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'author', type: 'User', nullable: false },  // References User type
]);
```

### Enum Types

```typescript
const OrderStatus = fraiseql.enum_('OrderStatus', {
  PENDING: 'pending',
  PROCESSING: 'processing',
  SHIPPED: 'shipped',
  DELIVERED: 'delivered',
  CANCELLED: 'cancelled',
}, {
  description: 'Status of an order'
});

// Use in type
fraiseql.registerTypeFields('Order', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'status', type: 'OrderStatus', nullable: false },
]);
```

### Interface Types

```typescript
const Node = fraiseql.interface_('Node', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'createdAt', type: 'DateTime', nullable: false },
], {
  description: 'An object with globally unique ID'
});

// Implement interface in types
fraiseql.registerTypeFields('User', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'createdAt', type: 'DateTime', nullable: false },
  { name: 'name', type: 'String', nullable: false },
]);
```

### Union Types

```typescript
const SearchResult = fraiseql.union('SearchResult',
  ['User', 'Post', 'Comment'],
  { description: 'Result of a search query' }
);

fraiseql.registerQuery(
  'search',
  'SearchResult',      // Returns union
  true,                // Is list
  false,               // Not nullable
  [{ name: 'query', type: 'String', nullable: false }],
  'Search across content'
);
```

### Input Types

```typescript
const CreateUserInput = fraiseql.input('CreateUserInput', [
  { name: 'email', type: 'Email', nullable: false },
  { name: 'name', type: 'String', nullable: false },
  { name: 'role', type: 'String', nullable: false, default: 'user' },
], {
  description: 'Input for creating a new user'
});

fraiseql.registerMutation(
  'createUser',
  'User',
  false,                                        // Single item
  false,                                        // Not nullable
  [{ name: 'input', type: 'CreateUserInput', nullable: false }],
  'Create a new user'
);
```

## Operations

### Queries

Queries are read-only operations that map to SQL SELECT or views.

```typescript
@fraiseql.query({ sqlSource: 'v_users' })
function users(
  limit: number = 10,
  offset: number = 0,
  status?: string
): User[] {
  throw new Error('Not executed');
}

// Manual registration with full control
fraiseql.registerQuery(
  'users',
  'User',
  true,                // Returns list
  false,               // Not nullable
  [
    { name: 'limit', type: 'Int', nullable: false, default: 10 },
    { name: 'offset', type: 'Int', nullable: false, default: 0 },
    { name: 'status', type: 'String', nullable: true },
  ],
  'Get paginated user list',
  { sql_source: 'v_users' }
);
```

### Mutations

Mutations are write operations that map to SQL functions.

```typescript
@fraiseql.mutation({ sqlSource: 'fn_create_user', operation: 'CREATE' })
function createUser(email: string, name: string): User {
  throw new Error('Not executed');
}

fraiseql.registerMutation(
  'createUser',
  'User',
  false,               // Single item
  false,               // Not nullable
  [
    { name: 'email', type: 'Email', nullable: false },
    { name: 'name', type: 'String', nullable: false },
  ],
  'Create a new user',
  { sql_source: 'fn_create_user', operation: 'CREATE' }
);

// UPDATE operation
fraiseql.registerMutation(
  'updateUser',
  'User',
  false,
  false,
  [
    { name: 'id', type: 'ID', nullable: false },
    { name: 'email', type: 'Email', nullable: true },
    { name: 'name', type: 'String', nullable: true },
  ],
  'Update an existing user',
  { sql_source: 'fn_update_user', operation: 'UPDATE' }
);

// DELETE operation
fraiseql.registerMutation(
  'deleteUser',
  'Boolean',
  false,
  false,
  [{ name: 'id', type: 'ID', nullable: false }],
  'Delete a user by ID',
  { sql_source: 'fn_delete_user', operation: 'DELETE' }
);
```

### Subscriptions

Real-time subscriptions for database events (LISTEN/NOTIFY or CDC).

```typescript
fraiseql.registerSubscription(
  'userCreated',
  'User',
  false,                    // Single item
  false,                    // Not nullable
  [],                       // No filter arguments
  'Subscribe to new user registrations',
  { operation: 'CREATE' }
);

// With filtering
fraiseql.registerSubscription(
  'customerOrders',
  'Order',
  false,
  false,
  [
    { name: 'customerId', type: 'ID', nullable: false }
  ],
  'Subscribe to orders for a specific customer',
  { topic: 'orders', operation: 'CREATE' }
);

// Change Data Capture (all changes)
fraiseql.registerSubscription(
  'userChanges',
  'User',
  false,
  false,
  [],
  'Subscribe to all user changes',
  { operations: ['CREATE', 'UPDATE', 'DELETE'] }
);
```

## Advanced Features

### Fact Tables for Analytics

```typescript
@fraiseql.FactTable({
  tableName: 'tf_sales',
  measures: ['revenue', 'quantity', 'cost'],
  dimensionPaths: [
    { name: 'region', json_path: "data->>'region'", data_type: 'text' },
    { name: 'category', json_path: "data->>'category'", data_type: 'text' },
    { name: 'saleDate', json_path: "data->>'date'", data_type: 'date' },
  ],
})
@fraiseql.type()
class Sale {
  id!: number;
  revenue!: number;
  quantity!: number;
  cost!: number;
  customerId!: number;
}

fraiseql.registerTypeFields('Sale', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'revenue', type: 'Decimal', nullable: false },
  { name: 'quantity', type: 'Int', nullable: false },
  { name: 'cost', type: 'Decimal', nullable: false },
  { name: 'customerId', type: 'ID', nullable: false },
]);
```

### Aggregate Queries

```typescript
fraiseql.registerQuery(
  'salesSummary',
  'Record<string, unknown>',
  true,                    // Returns list (aggregation rows)
  false,                   // Not nullable
  [
    { name: 'groupBy', type: '[String!]', nullable: true },
    { name: 'where', type: 'String', nullable: true },
    { name: 'limit', type: 'Int', nullable: true, default: 100 },
  ],
  'Sales aggregation by dimensions',
  {
    factTable: 'tf_sales',
    autoGroupBy: true,
    autoAggregates: true
  }
);
```

### Field-Level Security

```typescript
fraiseql.registerTypeFields('User', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'email', type: 'String', nullable: false },
  {
    name: 'salary',
    type: 'Decimal',
    nullable: false,
    requiresScope: 'read:User.salary',
    description: 'Annual salary (HR access only)'
  },
  {
    name: 'ssn',
    type: 'String',
    nullable: false,
    requiresScope: ['pii:read', 'admin'],
    description: 'Social security number'
  },
]);
```

### Field Deprecation

```typescript
fraiseql.registerTypeFields('Product', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'name', type: 'String', nullable: false },
  {
    name: 'oldPrice',
    type: 'Decimal',
    nullable: true,
    deprecated: 'Use pricing.current instead',
    description: 'Legacy price field (deprecated)'
  },
  { name: 'pricing', type: 'PricingObject', nullable: false },
]);
```

### Observers and Webhooks

```typescript
fraiseql.registerObserver(
  'onOrderCreated',
  'Order',
  ['CREATE'],
  {
    webhookUrl: 'https://example.com/webhooks/orders',
    retryPolicy: { maxAttempts: 3, backoffMs: 1000 },
    headers: { 'Authorization': 'Bearer ${SECRET_WEBHOOK_KEY}' },
  },
  'Notify external system when order created'
);
```

## Scalar Types Reference

FraiseQL supports 60+ scalar types with TypeScript mappings:

| GraphQL Type | TypeScript | SQL | Example |
|--------------|------------|-----|---------|
| `Int` | `number` | `INT` | `42` |
| `Float` | `number` | `FLOAT` | `3.14` |
| `String` | `string` | `VARCHAR` | `"hello"` |
| `Boolean` | `boolean` | `BOOLEAN` | `true` |
| `ID` | `string` | `UUID/INT` | `"user-123"` |
| `DateTime` | `Date` | `TIMESTAMP` | `new Date()` |
| `Date` | `Date` | `DATE` | `new Date()` |
| `Time` | `Date` | `TIME` | `new Date()` |
| `Decimal` | `string` | `DECIMAL` | `"99.99"` |
| `JSON` | `object` | `JSONB` | `{}` |
| `Email` | `string` | `VARCHAR` | `"user@example.com"` |
| `URL` | `string` | `VARCHAR` | `"https://example.com"` |
| `UUID` | `string` | `UUID` | `"550e8400-e29b-41d4-a716-446655440000"` |
| `Phone` | `string` | `VARCHAR` | `"+1-555-0100"` |
| `IPv4` | `string` | `INET` | `"192.168.1.1"` |
| `IPv6` | `string` | `INET` | `"2001:0db8:85a3::8a2e:0370:7334"` |
| `Slug` | `string` | `VARCHAR` | `"my-post-title"` |
| `Markdown` | `string` | `TEXT` | `"# Hello"` |
| `HTML` | `string` | `TEXT` | `"<div>..."` |

See [Scalars Reference](../../reference/scalars.md) for the complete 60+ type list.

## Schema Export

### Export to File

```typescript
// At end of schema definition file
if (require.main === module) {
  fraiseql.exportSchema('schema.json', { pretty: true });
  console.log('Schema exported to schema.json');
}
```

### Get Schema as Object

```typescript
const schema = fraiseql.getSchemaDict();
console.log(schema.types);
console.log(schema.queries);
console.log(schema.mutations);
```

### Export to String

```typescript
const json = fraiseql.exportSchemaToString({ pretty: true });
console.log(json);
```

### Schema.json Structure

```json
{
  "types": [
    {
      "name": "User",
      "kind": "OBJECT",
      "fields": [
        { "name": "id", "type": "ID!", "nullable": false },
        { "name": "name", "type": "String!", "nullable": false }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": "User",
      "returnsList": true,
      "nullable": false,
      "args": []
    }
  ],
  "mutations": [],
  "subscriptions": []
}
```

## Type Mapping

TypeScript to GraphQL type conversion:

| TypeScript | GraphQL | Nullable |
|------------|---------|----------|
| `number` | `Float` | `Float!` |
| `string` | `String` | `String!` |
| `boolean` | `Boolean` | `Boolean!` |
| `Date` | `DateTime` | `DateTime!` |
| `T[]` | `[T!]` | `[T!]!` |
| `T \| null` | `T` | `T` (nullable) |
| `T \| undefined` | `T` | `T` (optional param) |
| `User` (class) | `User` | `User!` |
| `Record<K, V>` | `JSON` | `JSON!` |

## Common Patterns

### CRUD Operations

```typescript
// Create
fraiseql.registerMutation('createUser', 'User', false, false,
  [{ name: 'email', type: 'Email', nullable: false }],
  'Create user',
  { sql_source: 'fn_create_user', operation: 'CREATE' }
);

// Read (single)
fraiseql.registerQuery('user', 'User', false, true,
  [{ name: 'id', type: 'ID', nullable: false }],
  'Get user by ID',
  { sql_source: 'fn_get_user' }
);

// Update
fraiseql.registerMutation('updateUser', 'User', false, true,
  [
    { name: 'id', type: 'ID', nullable: false },
    { name: 'email', type: 'Email', nullable: true },
  ],
  'Update user',
  { sql_source: 'fn_update_user', operation: 'UPDATE' }
);

// Delete
fraiseql.registerMutation('deleteUser', 'Boolean', false, false,
  [{ name: 'id', type: 'ID', nullable: false }],
  'Delete user',
  { sql_source: 'fn_delete_user', operation: 'DELETE' }
);
```

### Pagination

```typescript
fraiseql.registerQuery(
  'users',
  'User',
  true,          // List
  false,         // Not nullable
  [
    { name: 'limit', type: 'Int', nullable: false, default: 10 },
    { name: 'offset', type: 'Int', nullable: false, default: 0 },
    { name: 'sort', type: 'String', nullable: true, default: 'id' },
    { name: 'order', type: 'String', nullable: true, default: 'ASC' },
  ],
  'Get paginated users',
  { sql_source: 'v_users' }
);
```

### Filtering

```typescript
fraiseql.registerQuery(
  'usersByStatus',
  'User',
  true,
  false,
  [
    { name: 'status', type: 'String', nullable: false },
    { name: 'minCreatedAt', type: 'DateTime', nullable: true },
    { name: 'maxCreatedAt', type: 'DateTime', nullable: true },
  ],
  'Get users filtered by status and date range',
  { sql_source: 'fn_users_by_status' }
);
```

## Error Handling

FraiseQL uses typed errors:

```typescript
// Example error response
{
  "errors": [
    {
      "message": "Invalid input",
      "extensions": {
        "code": "VALIDATION_ERROR",
        "field": "email",
        "details": "Must be a valid email"
      }
    }
  ]
}
```

### Common Error Codes

- `VALIDATION_ERROR` - Input validation failed
- `AUTHENTICATION_ERROR` - Missing or invalid credentials
- `AUTHORIZATION_ERROR` - Insufficient permissions
- `NOT_FOUND` - Resource not found
- `DATABASE_ERROR` - Database operation failed
- `PARSE_ERROR` - GraphQL query parse error
- `RATE_LIMIT` - Rate limit exceeded

## Testing

### Jest/Vitest Test Patterns

```typescript
import * as fraiseql from 'fraiseql';
import { describe, it, expect } from 'vitest';

describe('Schema Definition', () => {
  it('should register User type', () => {
    fraiseql.registerTypeFields('User', [
      { name: 'id', type: 'ID', nullable: false },
      { name: 'email', type: 'Email', nullable: false },
    ]);

    const schema = fraiseql.getSchemaDict();
    expect(schema.types).toContainEqual(
      expect.objectContaining({ name: 'User' })
    );
  });

  it('should register users query', () => {
    fraiseql.registerQuery(
      'users', 'User', true, false, [],
      'Get users'
    );

    const schema = fraiseql.getSchemaDict();
    expect(schema.queries).toContainEqual(
      expect.objectContaining({ name: 'users', returnType: 'User' })
    );
  });

  it('should validate schema exports to JSON', () => {
    const json = fraiseql.exportSchemaToString();
    const parsed = JSON.parse(json);
    expect(parsed.types).toBeDefined();
    expect(parsed.queries).toBeDefined();
  });
});
```

## Framework Integration

### NestJS

```typescript
import { Injectable } from '@nestjs/common';
import * as fraiseql from 'fraiseql';

@Injectable()
export class FraiseQLService {
  registerSchema() {
    fraiseql.registerTypeFields('User', [
      { name: 'id', type: 'ID', nullable: false },
    ]);

    fraiseql.registerQuery(
      'users', 'User', true, false, [],
      'Get all users'
    );
  }

  exportSchema(path: string) {
    fraiseql.exportSchema(path);
  }
}
```

### Express

```typescript
import express from 'express';
import * as fraiseql from 'fraiseql';

const app = express();

// Define schema
fraiseql.registerTypeFields('User', [
  { name: 'id', type: 'ID', nullable: false },
]);

// Export endpoint
app.get('/schema.json', (req, res) => {
  const json = fraiseql.exportSchemaToString();
  res.json(JSON.parse(json));
});

app.listen(3000);
```

## See Also

- [Python SDK Reference](./python-reference.md)
- [GraphQL Scalars Reference](../../reference/scalars.md)
- [Security & RBAC Guide](../../guides/security-and-rbac.md)
- [Analytics & OLAP Guide](../../guides/analytics-olap.md)
- [Architecture Principles](../../guides/ARCHITECTURE_PRINCIPLES.md)
- [TypeScript SDK GitHub](https://github.com/fraiseql/fraiseql-typescript)

---

**Remember:** TypeScript is for authoring only. The Rust compiler transforms your schema into optimized SQL. No runtime FFI or native bindings—just pure JSON schema generation.
