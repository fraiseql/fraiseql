<!-- Skip to main content -->
---
title: FraiseQL TypeScript SDK Reference
description: Complete API reference for the FraiseQL TypeScript SDK. Provides decorators and utilities for defining GraphQL schemas that compile to optimized SQL. TypeScript
keywords: ["framework", "directives", "types", "sdk", "schema", "scalars", "monitoring", "api"]
tags: ["documentation", "reference"]
---

# FraiseQL TypeScript SDK Reference

**Status**: Production-Ready | **Version**: 2.0.0 | **Node.js**: 18+ | **TypeScript**: 5.0+

Complete API reference for the FraiseQL TypeScript SDK. Provides decorators and utilities for defining GraphQL schemas that compile to optimized SQL. TypeScript authoring only—no runtime FFI or native bindings.

## Installation

```bash
<!-- Code example in BASH -->
# npm
npm install FraiseQL

# yarn
yarn add FraiseQL

# pnpm
pnpm add FraiseQL
```text
<!-- Code example in TEXT -->

**Requirements:**

- Node.js 18 or higher
- TypeScript 5.0+ (strict mode recommended)
- Decorators enabled in `tsconfig.json`:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "experimentalDecorators": true,
    "target": "ES2020",
    "module": "commonjs",
    "strict": true,
    "moduleResolution": "node"
  }
}
```text
<!-- Code example in TEXT -->

## Quick Reference Table

| Feature | Method | Purpose |
|---------|--------|---------|
| **Types** | `@FraiseQL.type()` | Define GraphQL type |
| **Queries** | `@FraiseQL.query()` | Read operations (SELECT) |
| **Mutations** | `@FraiseQL.mutation()` | Write operations (CREATE/UPDATE/DELETE) |
| **Subscriptions** | `@FraiseQL.Subscription()` | Real-time events |
| **Fact Tables** | `@FraiseQL.FactTable()` | Analytics tables with measures/dimensions |
| **Aggregates** | `@FraiseQL.AggregateQuery()` | GROUP BY aggregations |
| **Enums** | `FraiseQL.enum_()` | GraphQL enum type |
| **Interfaces** | `FraiseQL.interface_()` | Shared field contracts |
| **Unions** | `FraiseQL.union()` | Polymorphic return types |
| **Input Types** | `FraiseQL.input()` | Structured parameters |
| **Field Metadata** | `requiresScope`, `deprecated` | Field-level features |
| **Schema Export** | `FraiseQL.exportSchema()` | Generate schema.json |

## Type System

### Basic Type Definition

```typescript
<!-- Code example in TypeScript -->
import * as FraiseQL from 'FraiseQL';

@FraiseQL.type()
class User {
  id!: number;
  name!: string;
  email!: string;
  isActive!: boolean;
}

// Register field metadata (required—TypeScript doesn't preserve runtime type info)
FraiseQL.registerTypeFields('User', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'name', type: 'String', nullable: false },
  { name: 'email', type: 'Email', nullable: false },
  { name: 'isActive', type: 'Boolean', nullable: false },
]);
```text
<!-- Code example in TEXT -->

### Nullable and Optional Types

```typescript
<!-- Code example in TypeScript -->
// Nullable (can be null in GraphQL response)
{ name: 'middleName', type: 'String', nullable: true }  // Returns String | null

// Optional parameter (can be omitted in GraphQL query)
{ name: 'limit', type: 'Int', nullable: false, default: 10 }  // Defaults to 10

// Both nullable and optional
{ name: 'description', type: 'String', nullable: true }  // Can be null or omitted
```text
<!-- Code example in TEXT -->

### Generic Types and Arrays

```typescript
<!-- Code example in TypeScript -->
// Array types
{ name: 'tags', type: 'String', nullable: false, isList: true }     // [String!]
{ name: 'scores', type: 'Float', nullable: true, isList: true }     // [Float]

// Nested types
@FraiseQL.type()
class Post {
  id!: number;
  author!: User;
}

FraiseQL.registerTypeFields('Post', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'author', type: 'User', nullable: false },  // References User type
]);
```text
<!-- Code example in TEXT -->

### Enum Types

```typescript
<!-- Code example in TypeScript -->
const OrderStatus = FraiseQL.enum_('OrderStatus', {
  PENDING: 'pending',
  PROCESSING: 'processing',
  SHIPPED: 'shipped',
  DELIVERED: 'delivered',
  CANCELLED: 'cancelled',
}, {
  description: 'Status of an order'
});

// Use in type
FraiseQL.registerTypeFields('Order', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'status', type: 'OrderStatus', nullable: false },
]);
```text
<!-- Code example in TEXT -->

### Interface Types

```typescript
<!-- Code example in TypeScript -->
const Node = FraiseQL.interface_('Node', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'createdAt', type: 'DateTime', nullable: false },
], {
  description: 'An object with globally unique ID'
});

// Implement interface in types
FraiseQL.registerTypeFields('User', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'createdAt', type: 'DateTime', nullable: false },
  { name: 'name', type: 'String', nullable: false },
]);
```text
<!-- Code example in TEXT -->

### Union Types

```typescript
<!-- Code example in TypeScript -->
const SearchResult = FraiseQL.union('SearchResult',
  ['User', 'Post', 'Comment'],
  { description: 'Result of a search query' }
);

FraiseQL.registerQuery(
  'search',
  'SearchResult',      // Returns union
  true,                // Is list
  false,               // Not nullable
  [{ name: 'query', type: 'String', nullable: false }],
  'Search across content'
);
```text
<!-- Code example in TEXT -->

### Input Types

```typescript
<!-- Code example in TypeScript -->
const CreateUserInput = FraiseQL.input('CreateUserInput', [
  { name: 'email', type: 'Email', nullable: false },
  { name: 'name', type: 'String', nullable: false },
  { name: 'role', type: 'String', nullable: false, default: 'user' },
], {
  description: 'Input for creating a new user'
});

FraiseQL.registerMutation(
  'createUser',
  'User',
  false,                                        // Single item
  false,                                        // Not nullable
  [{ name: 'input', type: 'CreateUserInput', nullable: false }],
  'Create a new user'
);
```text
<!-- Code example in TEXT -->

## Operations

### Queries

Queries are read-only operations that map to SQL SELECT or views.

```typescript
<!-- Code example in TypeScript -->
@FraiseQL.query({ sqlSource: 'v_users' })
function users(
  limit: number = 10,
  offset: number = 0,
  status?: string
): User[] {
  throw new Error('Not executed');
}

// Manual registration with full control
FraiseQL.registerQuery(
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
```text
<!-- Code example in TEXT -->

### Mutations

Mutations are write operations that map to SQL functions.

```typescript
<!-- Code example in TypeScript -->
@FraiseQL.mutation({ sqlSource: 'fn_create_user', operation: 'CREATE' })
function createUser(email: string, name: string): User {
  throw new Error('Not executed');
}

FraiseQL.registerMutation(
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
FraiseQL.registerMutation(
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
FraiseQL.registerMutation(
  'deleteUser',
  'Boolean',
  false,
  false,
  [{ name: 'id', type: 'ID', nullable: false }],
  'Delete a user by ID',
  { sql_source: 'fn_delete_user', operation: 'DELETE' }
);
```text
<!-- Code example in TEXT -->

### Subscriptions

Real-time subscriptions for database events (LISTEN/NOTIFY or CDC).

```typescript
<!-- Code example in TypeScript -->
FraiseQL.registerSubscription(
  'userCreated',
  'User',
  false,                    // Single item
  false,                    // Not nullable
  [],                       // No filter arguments
  'Subscribe to new user registrations',
  { operation: 'CREATE' }
);

// With filtering
FraiseQL.registerSubscription(
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
FraiseQL.registerSubscription(
  'userChanges',
  'User',
  false,
  false,
  [],
  'Subscribe to all user changes',
  { operations: ['CREATE', 'UPDATE', 'DELETE'] }
);
```text
<!-- Code example in TEXT -->

## Advanced Features

### Fact Tables for Analytics

```typescript
<!-- Code example in TypeScript -->
@FraiseQL.FactTable({
  tableName: 'tf_sales',
  measures: ['revenue', 'quantity', 'cost'],
  dimensionPaths: [
    { name: 'region', json_path: "data->>'region'", data_type: 'text' },
    { name: 'category', json_path: "data->>'category'", data_type: 'text' },
    { name: 'saleDate', json_path: "data->>'date'", data_type: 'date' },
  ],
})
@FraiseQL.type()
class Sale {
  id!: number;
  revenue!: number;
  quantity!: number;
  cost!: number;
  customerId!: number;
}

FraiseQL.registerTypeFields('Sale', [
  { name: 'id', type: 'ID', nullable: false },
  { name: 'revenue', type: 'Decimal', nullable: false },
  { name: 'quantity', type: 'Int', nullable: false },
  { name: 'cost', type: 'Decimal', nullable: false },
  { name: 'customerId', type: 'ID', nullable: false },
]);
```text
<!-- Code example in TEXT -->

### Aggregate Queries

```typescript
<!-- Code example in TypeScript -->
FraiseQL.registerQuery(
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
```text
<!-- Code example in TEXT -->

### Field-Level Security

```typescript
<!-- Code example in TypeScript -->
FraiseQL.registerTypeFields('User', [
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
```text
<!-- Code example in TEXT -->

### Field Deprecation

```typescript
<!-- Code example in TypeScript -->
FraiseQL.registerTypeFields('Product', [
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
```text
<!-- Code example in TEXT -->

### Observers and Webhooks

```typescript
<!-- Code example in TypeScript -->
FraiseQL.registerObserver(
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
```text
<!-- Code example in TEXT -->

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
<!-- Code example in TypeScript -->
// At end of schema definition file
if (require.main === module) {
  FraiseQL.exportSchema('schema.json', { pretty: true });
  console.log('Schema exported to schema.json');
}
```text
<!-- Code example in TEXT -->

### Get Schema as Object

```typescript
<!-- Code example in TypeScript -->
const schema = FraiseQL.getSchemaDict();
console.log(schema.types);
console.log(schema.queries);
console.log(schema.mutations);
```text
<!-- Code example in TEXT -->

### Export to String

```typescript
<!-- Code example in TypeScript -->
const json = FraiseQL.exportSchemaToString({ pretty: true });
console.log(json);
```text
<!-- Code example in TEXT -->

### Schema.json Structure

```json
<!-- Code example in JSON -->
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
```text
<!-- Code example in TEXT -->

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
<!-- Code example in TypeScript -->
// Create
FraiseQL.registerMutation('createUser', 'User', false, false,
  [{ name: 'email', type: 'Email', nullable: false }],
  'Create user',
  { sql_source: 'fn_create_user', operation: 'CREATE' }
);

// Read (single)
FraiseQL.registerQuery('user', 'User', false, true,
  [{ name: 'id', type: 'ID', nullable: false }],
  'Get user by ID',
  { sql_source: 'fn_get_user' }
);

// Update
FraiseQL.registerMutation('updateUser', 'User', false, true,
  [
    { name: 'id', type: 'ID', nullable: false },
    { name: 'email', type: 'Email', nullable: true },
  ],
  'Update user',
  { sql_source: 'fn_update_user', operation: 'UPDATE' }
);

// Delete
FraiseQL.registerMutation('deleteUser', 'Boolean', false, false,
  [{ name: 'id', type: 'ID', nullable: false }],
  'Delete user',
  { sql_source: 'fn_delete_user', operation: 'DELETE' }
);
```text
<!-- Code example in TEXT -->

### Pagination

```typescript
<!-- Code example in TypeScript -->
FraiseQL.registerQuery(
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
```text
<!-- Code example in TEXT -->

### Filtering

```typescript
<!-- Code example in TypeScript -->
FraiseQL.registerQuery(
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
```text
<!-- Code example in TEXT -->

## Error Handling

FraiseQL uses typed errors:

```typescript
<!-- Code example in TypeScript -->
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
```text
<!-- Code example in TEXT -->

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
<!-- Code example in TypeScript -->
import * as FraiseQL from 'FraiseQL';
import { describe, it, expect } from 'vitest';

describe('Schema Definition', () => {
  it('should register User type', () => {
    FraiseQL.registerTypeFields('User', [
      { name: 'id', type: 'ID', nullable: false },
      { name: 'email', type: 'Email', nullable: false },
    ]);

    const schema = FraiseQL.getSchemaDict();
    expect(schema.types).toContainEqual(
      expect.objectContaining({ name: 'User' })
    );
  });

  it('should register users query', () => {
    FraiseQL.registerQuery(
      'users', 'User', true, false, [],
      'Get users'
    );

    const schema = FraiseQL.getSchemaDict();
    expect(schema.queries).toContainEqual(
      expect.objectContaining({ name: 'users', returnType: 'User' })
    );
  });

  it('should validate schema exports to JSON', () => {
    const json = FraiseQL.exportSchemaToString();
    const parsed = JSON.parse(json);
    expect(parsed.types).toBeDefined();
    expect(parsed.queries).toBeDefined();
  });
});
```text
<!-- Code example in TEXT -->

## Framework Integration

### NestJS

```typescript
<!-- Code example in TypeScript -->
import { Injectable } from '@nestjs/common';
import * as FraiseQL from 'FraiseQL';

@Injectable()
export class FraiseQLService {
  registerSchema() {
    FraiseQL.registerTypeFields('User', [
      { name: 'id', type: 'ID', nullable: false },
    ]);

    FraiseQL.registerQuery(
      'users', 'User', true, false, [],
      'Get all users'
    );
  }

  exportSchema(path: string) {
    FraiseQL.exportSchema(path);
  }
}
```text
<!-- Code example in TEXT -->

### Express

```typescript
<!-- Code example in TypeScript -->
import express from 'express';
import * as FraiseQL from 'FraiseQL';

const app = express();

// Define schema
FraiseQL.registerTypeFields('User', [
  { name: 'id', type: 'ID', nullable: false },
]);

// Export endpoint
app.get('/schema.json', (req, res) => {
  const json = FraiseQL.exportSchemaToString();
  res.json(JSON.parse(json));
});

app.listen(3000);
```text
<!-- Code example in TEXT -->

## Troubleshooting

### Common Setup Issues

#### Installation Problems

**Issue**: `npm ERR! 404 Not Found - GET https://registry.npmjs.org/FraiseQL`

**Solutions**:

```bash
<!-- Code example in BASH -->
# Check npm version
npm --version

# Clear npm cache
npm cache clean --force

# Try installing with specific version
npm install FraiseQL@2.0.0

# Use yarn or pnpm instead
yarn add FraiseQL
pnpm add FraiseQL
```text
<!-- Code example in TEXT -->

**Private registry**:

```bash
<!-- Code example in BASH -->
# If using private npm registry
npm config set registry https://your-registry.com
npm install FraiseQL
```text
<!-- Code example in TEXT -->

#### Module Resolution Issues

**Issue**: `Cannot find module 'FraiseQL'`

**Solution - Check tsconfig.json**:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "moduleResolution": "node",
    "target": "ES2020",
    "module": "commonjs",
    "strict": true,
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true
  }
}
```text
<!-- Code example in TEXT -->

**Issue**: `ESM vs CommonJS mismatch`

**Solution**:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "module": "commonjs",     // Not "esnext"
    "target": "ES2020"
  }
}
```text
<!-- Code example in TEXT -->

#### Decorator Configuration

**Issue**: `Experimental decorators are not supported`

**Solution - Enable decorators**:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true
  }
}
```text
<!-- Code example in TEXT -->

#### Version Compatibility

**Issue**: Running with Node.js 16 or lower

**Solution**:

```bash
<!-- Code example in BASH -->
# Check Node version (18+ required)
node --version

# Update Node
nvm install 18.0.0
nvm use 18.0.0
```text
<!-- Code example in TEXT -->

**Check TypeScript version** (5.0+ required):

```bash
<!-- Code example in BASH -->
npx tsc --version
npm install -D typescript@5.0.0
```text
<!-- Code example in TEXT -->

---

### Type System Issues

#### Type Mismatch Errors

**Issue**: `TS2322: Type 'string' is not assignable to type 'Email'`

**Cause**: Type annotation doesn't match registered fields

**Solution**:

```typescript
<!-- Code example in TypeScript -->
// ❌ Wrong - inconsistent type registration
@FraiseQL.type()
class User {
  email!: string;
}
FraiseQL.registerTypeFields('User', [
  { name: 'email', type: 'Email', nullable: false }  // Mismatch!
]);

// ✅ Correct - consistent types
@FraiseQL.type()
class User {
  email!: string;  // Store as string, but declare as Email type
}
FraiseQL.registerTypeFields('User', [
  { name: 'email', type: 'Email', nullable: false }
]);
```text
<!-- Code example in TEXT -->

#### Nullability Problems

**Issue**: `Property 'name' cannot be undefined but is accessed without assertion`

**Solution - Explicit null handling**:

```typescript
<!-- Code example in TypeScript -->
// ❌ Wrong - optional but should be explicit
@FraiseQL.type()
class User {
  name!: string;
}
FraiseQL.registerTypeFields('User', [
  { name: 'name', type: 'String', nullable: true }  // Should be nullable!
]);

// ✅ Correct
@FraiseQL.type()
class User {
  name?: string;  // Mark as optional in TypeScript
}
FraiseQL.registerTypeFields('User', [
  { name: 'name', type: 'String', nullable: true }
]);
```text
<!-- Code example in TEXT -->

#### Generic Type Issues

**Issue**: `Type parameter 'T' cannot be used in type registry`

**Cause**: FraiseQL doesn't support generic types

**Solution - Use concrete types**:

```typescript
<!-- Code example in TypeScript -->
// ❌ Won't work - generics not supported
class Paginated<T> {
  items: T[];
  total: number;
}

// ✅ Use concrete types instead
@FraiseQL.type()
class UserPage {
  items!: User[];
  total!: number;
}

FraiseQL.registerTypeFields('UserPage', [
  { name: 'items', type: 'User', nullable: false, isList: true },
  { name: 'total', type: 'Int', nullable: false },
]);
```text
<!-- Code example in TEXT -->

#### Schema Validation Errors

**Issue**: `Error: Type 'UnknownType' is not registered`

**Cause**: Type referenced but not registered

**Solution - Register all types**:

```typescript
<!-- Code example in TypeScript -->
// Define type
@FraiseQL.type()
class User {
  id!: number;
}

// Register it (this is required in TypeScript!)
FraiseQL.registerTypeFields('User', [
  { name: 'id', type: 'ID', nullable: false },
]);

// ✅ Now use it
@FraiseQL.query('getUser')
function getUser(): User {
  return { id: 1 };
}
```text
<!-- Code example in TEXT -->

---

### Runtime Errors

#### Query Execution Failures

**Issue**: `Error: Query execution failed`

**Debug with logging**:

```typescript
<!-- Code example in TypeScript -->
// Enable verbose logging
const server = await FraiseQLServer.fromCompiled('schema.compiled.json', {
  debug: true,
  logLevel: 'debug',
});

try {
  const result = await server.execute(query);
  console.log(result);
} catch (error) {
  console.error('Execution error:', error);
}
```text
<!-- Code example in TEXT -->

#### Async/Await Issues

**Issue**: `UnhandledPromiseRejectionWarning: Query execution failed`

**Solution - Always await**:

```typescript
<!-- Code example in TypeScript -->
// ❌ Wrong - not awaiting
const result = server.execute(query);  // Returns Promise
console.log(result);  // undefined!

// ✅ Correct - await Promise
const result = await server.execute(query);
console.log(result);  // Actual result
```text
<!-- Code example in TEXT -->

**Using async handlers**:

```typescript
<!-- Code example in TypeScript -->
app.post('/graphql', async (req, res, next) => {
  try {
    const result = await server.execute(req.body.query);
    res.json(result);
  } catch (error) {
    next(error);  // Pass to error middleware
  }
});
```text
<!-- Code example in TEXT -->

#### Connection Issues

**Issue**: `Error: Failed to connect to database`

**Check environment**:

```bash
<!-- Code example in BASH -->
# Verify DATABASE_URL is set
echo $DATABASE_URL

# Test connectivity
psql postgresql://user:pass@localhost/db -c "SELECT 1"
```text
<!-- Code example in TEXT -->

**Solution in code**:

```typescript
<!-- Code example in TypeScript -->
const server = await FraiseQLServer.fromCompiled('schema.compiled.json', {
  databaseUrl: process.env.DATABASE_URL,
}).catch((error) => {
  console.error('Failed to initialize server:', error);
  process.exit(1);
});
```text
<!-- Code example in TEXT -->

#### Timeout Problems

**Issue**: `TimeoutError: Operation exceeded 30000ms timeout`

**Solution - Increase timeout**:

```typescript
<!-- Code example in TypeScript -->
const server = await FraiseQLServer.fromCompiled('schema.compiled.json', {
  timeout: 60000,  // 60 seconds
  queryTimeout: 30000,  // Per-query timeout
});
```text
<!-- Code example in TEXT -->

**Or optimize queries**:

```typescript
<!-- Code example in TypeScript -->
// Add pagination to large datasets
@FraiseQL.query('getUsersPaginated')
function getUsersPaginated(limit: number = 20, offset: number = 0): User[] {
  return [];
}
```text
<!-- Code example in TEXT -->

---

### Performance Issues

#### ESM vs CommonJS Mismatch

**Issue**: `Cannot use import statement outside a module`

**Solution - Configure properly**:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "module": "commonjs",
    "target": "ES2020"
  }
}
```text
<!-- Code example in TEXT -->

Or for ESM:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "module": "esnext",
    "target": "ES2020"
  },
  "type": "module"
}
```text
<!-- Code example in TEXT -->

#### Type Checking Performance

**Issue**: `TypeScript compilation takes >30 seconds`

**Solution - Skip library type checking**:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true
  }
}
```text
<!-- Code example in TEXT -->

**Use incremental compilation**:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "incremental": true,
    "tsBuildInfoFile": ".tsbuildinfo"
  }
}
```text
<!-- Code example in TEXT -->

#### Build Size Issues

**Issue**: Output bundle is >1MB

**Solution - Tree-shake unused code**:

```typescript
<!-- Code example in TypeScript -->
// In build config (webpack, esbuild, etc.)
// Enable side-effect-free imports
import { type } from 'FraiseQL';  // Only import what you need
```text
<!-- Code example in TEXT -->

**Use esbuild for faster builds**:

```bash
<!-- Code example in BASH -->
esbuild src/index.ts --bundle --outfile=dist/bundle.js --minify
```text
<!-- Code example in TEXT -->

#### Query Performance

**Issue**: Queries execute slowly

**Enable caching**:

```typescript
<!-- Code example in TypeScript -->
const server = await FraiseQLServer.fromCompiled('schema.compiled.json', {
  cache: {
    enabled: true,
    ttl: 300,  // 5 minutes
  },
});
```text
<!-- Code example in TEXT -->

---

### Debugging Techniques

#### Enable Debug Logging

**Setup logging**:

```typescript
<!-- Code example in TypeScript -->
import * as FraiseQL from 'FraiseQL';

// Enable debug mode
const server = await FraiseQLServer.fromCompiled('schema.compiled.json', {
  debug: true,
  logLevel: 'debug',
});

// Or via environment
process.env.FRAISEQL_DEBUG = 'true';
process.env.RUST_LOG = 'FraiseQL=debug';
```text
<!-- Code example in TEXT -->

#### Use TypeScript Compiler Options

**Enable source maps**:

```json
<!-- Code example in JSON -->
{
  "compilerOptions": {
    "sourceMap": true,
    "inlineSources": true
  }
}
```text
<!-- Code example in TEXT -->

**Then debug**:

```bash
<!-- Code example in BASH -->
node --inspect-brk dist/index.js
# Opens Chrome DevTools at chrome://inspect
```text
<!-- Code example in TEXT -->

#### Inspect Generated Types

**Print generated types**:

```typescript
<!-- Code example in TypeScript -->
const schema = await FraiseQL.loadCompiledSchema('schema.compiled.json');
console.log(JSON.stringify(schema.types, null, 2));
```text
<!-- Code example in TEXT -->

**Validate schema**:

```typescript
<!-- Code example in TypeScript -->
const schema = await FraiseQL.validateSchema(schemaJson);
if (!schema.valid) {
  console.error('Schema errors:', schema.errors);
}
```text
<!-- Code example in TEXT -->

#### Network Traffic Inspection

**Using curl**:

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:3000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ user(id: 1) { id } }"}' \
  -v
```text
<!-- Code example in TEXT -->

**Using browser DevTools**:

- Open Network tab
- Send GraphQL request
- Inspect request/response headers and body

---

### Getting Help

#### GitHub Issues

Provide when reporting issues:

1. Node.js version: `node --version`
2. TypeScript version: `npx tsc --version`
3. FraiseQL version: `npm list FraiseQL`
4. tsconfig.json settings
5. Minimal reproducible example
6. Full error traceback

**Issue template**:

```markdown
<!-- Code example in MARKDOWN -->
**Environment**:
- Node.js: v18.16.0
- TypeScript: 5.0.4
- FraiseQL: 2.0.0

**Issue**:
[Describe problem]

**Reproduce**:
[Minimal code example]

**Error**:
[Full error message and stack trace]
```text
<!-- Code example in TEXT -->

#### Community Channels

- **GitHub Discussions**: Ask questions
- **Stack Overflow**: Tag with `FraiseQL` and `typescript`
- **Discord**: Real-time chat with maintainers

#### Performance Profiling

**Profile with Node.js**:

```bash
<!-- Code example in BASH -->
node --prof dist/index.js
node --prof-process isolate-*.log > profile.txt
```text
<!-- Code example in TEXT -->

**Use clinic.js**:

```bash
<!-- Code example in BASH -->
npm install -g clinic
clinic doctor -- node dist/index.js
```text
<!-- Code example in TEXT -->

---

## See Also

- [Python SDK Reference](./python-reference.md)
- [GraphQL Scalars Reference](../../reference/scalars.md)
- [Security & RBAC Guide](../../guides/authorization-quick-start.md)
- [Analytics & OLAP Guide](../../guides/analytics-patterns.md)
- [Architecture Principles](../../architecture/README.md)
- [TypeScript SDK GitHub](https://github.com/FraiseQL/FraiseQL-typescript)

---

**Remember:** TypeScript is for authoring only. The Rust compiler transforms your schema into optimized SQL. No runtime FFI or native bindings—just pure JSON schema generation.
