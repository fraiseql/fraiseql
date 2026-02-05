# FraiseQL Node.js SDK Reference

**Status**: Production-Ready | **Version**: 2.0.0 | **Node.js**: 18+ | **Module**: `FraiseQL-nodejs`

Complete API reference for the FraiseQL Node.js Runtime SDK. Provides Promise-based async client for executing pre-compiled GraphQL queries against FraiseQL servers. Runtime execution only—no schema authoring (use TypeScript SDK for that).

## Installation

```bash
# npm
npm install FraiseQL-nodejs

# yarn
yarn add FraiseQL-nodejs

# pnpm
pnpm add FraiseQL-nodejs
```

**Requirements:**

- Node.js 18 or higher
- `FraiseQL-nodejs` (npm package)
- Optional: TypeScript 5.0+ for type safety
- Optional: `@types/node` for Node.js types

**Module Systems:**

- **CommonJS** (`.js`, `require()`): Fully supported
- **ESM** (`.mjs`, `import`): Fully supported
- **Dual package**: Auto-detects based on `package.json` `"type"` field

## Quick Reference Table

| Feature | Method | Purpose |
|---------|--------|---------|
| **Connection** | `FraiseQLClient.connect()` | Initialize connection pool |
| **Query** | `client.query()` | Execute GraphQL query |
| **Mutation** | `client.mutate()` | Execute GraphQL mutation |
| **Subscription** | `client.subscribe()` | Subscribe to real-time events |
| **Batch** | `client.batch()` | Execute multiple queries |
| **Raw SQL** | `client.rawSql()` | Execute raw SQL (admin only) |
| **Type Check** | `client.validateInput()` | Runtime type validation |
| **Pool Management** | `client.getPool()` | Access connection pool |
| **Disconnect** | `client.disconnect()` | Clean shutdown |

## Client Initialization

### CommonJS Example

```javascript
const { FraiseQLClient } = require('FraiseQL-nodejs');

const client = new FraiseQLClient({
  schemaPath: './schema.compiled.json',
  database: {
    type: 'postgres',
    host: 'localhost',
    port: 5432,
    database: 'fraiseql_db',
    user: 'postgres',
    password: process.env.DB_PASSWORD,
  },
  poolSize: 20,
  requestTimeout: 30000,
  logLevel: 'info',
});

async function main() {
  await client.connect();
  try {
    // Queries here
  } finally {
    await client.disconnect();
  }
}

main().catch(console.error);
```

### ESM Example

```javascript
import { FraiseQLClient } from 'FraiseQL-nodejs';

const client = new FraiseQLClient({
  schemaPath: './schema.compiled.json',
  database: {
    type: 'postgres',
    host: 'localhost',
    port: 5432,
    database: 'fraiseql_db',
    user: 'postgres',
    password: process.env.DB_PASSWORD,
  },
  poolSize: 20,
  requestTimeout: 30000,
  logLevel: 'info',
});

async function main() {
  await client.connect();
  try {
    // Queries here
  } finally {
    await client.disconnect();
  }
}

main().catch(console.error);
```

### TypeScript Example

```typescript
import { FraiseQLClient, QueryResult, FraiseQLError } from 'FraiseQL-nodejs';

interface User {
  id: string;
  name: string;
  email: string;
  isActive: boolean;
}

const client = new FraiseQLClient({
  schemaPath: './schema.compiled.json',
  database: {
    type: 'postgres',
    host: 'localhost',
    port: 5432,
    database: 'fraiseql_db',
    user: 'postgres',
    password: process.env.DB_PASSWORD,
  },
  poolSize: 20,
  requestTimeout: 30000,
  logLevel: 'info',
});

async function main(): Promise<void> {
  await client.connect();
  try {
    const result = await client.query<User[]>('users', { limit: 10 });
    console.log(result.data);
  } catch (error) {
    console.error(error);
  } finally {
    await client.disconnect();
  }
}

main();
```

## Type System and Runtime Validation

### Dynamic Type Checking

FraiseQL performs runtime type validation at execution boundaries:

```javascript
// Type checking happens automatically
const result = await client.query('users', {
  limit: 10,              // ✅ Valid: number
  offset: 'invalid',      // ❌ Error: expected number
  status: 'active',       // ✅ Valid: string
});
```

### Input Validation

```javascript
// Validate input before query
const validation = client.validateInput('createUser', {
  email: 'user@example.com',
  name: 'John Doe',
  role: 'admin',
});

if (!validation.valid) {
  console.error('Validation errors:', validation.errors);
  // errors: [{ field: 'role', message: 'Invalid enum value' }]
} else {
  const result = await client.mutate('createUser', {
    email: 'user@example.com',
    name: 'John Doe',
    role: 'admin',
  });
}
```

### JSDoc Type Hints

```javascript
/**
 * Get paginated user list
 * @param {number} limit - Items per page (default: 10)
 * @param {number} offset - Pagination offset (default: 0)
 * @param {string} [status] - Optional status filter
 * @returns {Promise<QueryResult<Array>>} Users matching criteria
 */
async function getUsers(limit = 10, offset = 0, status) {
  return client.query('users', { limit, offset, status });
}
```

## Query Operations

### Simple Query

```javascript
const result = await client.query('users', { limit: 10 });
console.log(result.data);      // Array of results
console.log(result.execution); // { duration: 45, cached: false }
```

### Query with Variables

```javascript
const result = await client.query('userById', {
  id: 'user-123',
  includeDetails: true,
});

console.log(result.data);
```

### Query with Authorization

```javascript
const result = await client.query('sensitiveData', {}, {
  authToken: 'Bearer eyJhbGc...',
  userId: 'user-123',
  scopes: ['read:admin', 'read:financial'],
});
```

### Batch Queries

```javascript
const results = await client.batch([
  { operation: 'query', name: 'users', args: { limit: 5 } },
  { operation: 'query', name: 'products', args: { limit: 5 } },
  { operation: 'query', name: 'orders', args: { limit: 5 } },
]);

// results: [{ data: [...] }, { data: [...] }, { data: [...] }]
```

## Mutation Operations

### CREATE Mutation

```javascript
const result = await client.mutate('createUser', {
  email: 'new@example.com',
  name: 'Alice Smith',
  role: 'user',
});

console.log(result.data);      // { id: 'user-456', email: '...', ... }
console.log(result.id);        // 'user-456'
```

### UPDATE Mutation

```javascript
const result = await client.mutate('updateUser', {
  id: 'user-123',
  email: 'updated@example.com',
  name: 'Bob Updated',
});
```

### DELETE Mutation

```javascript
const result = await client.mutate('deleteUser', {
  id: 'user-123',
});

console.log(result.data);      // true (success)
```

### Transactional Mutations

```javascript
const transaction = await client.transaction([
  { operation: 'mutate', name: 'createUser', args: { email: '1@example.com', name: 'User 1' } },
  { operation: 'mutate', name: 'createUser', args: { email: '2@example.com', name: 'User 2' } },
  { operation: 'mutate', name: 'createUser', args: { email: '3@example.com', name: 'User 3' } },
]);

console.log(transaction.results);  // All succeeded or rolled back
console.log(transaction.success);  // true/false
```

## Subscriptions

### WebSocket Subscription

```javascript
const subscription = await client.subscribe('userCreated', {}, {
  onMessage: (data) => {
    console.log('New user:', data);
  },
  onError: (error) => {
    console.error('Subscription error:', error);
  },
  onClose: () => {
    console.log('Subscription closed');
  },
});

// Later: unsubscribe
await subscription.unsubscribe();
```

### Filtered Subscription

```javascript
const subscription = await client.subscribe('customerOrders', {
  customerId: 'customer-123',
}, {
  onMessage: (order) => {
    console.log('Order received:', order);
  },
});
```

## Advanced Features

### Fact Tables and Analytics

```javascript
// Execute aggregation on fact table
const summary = await client.query('salesSummary', {
  groupBy: ['region', 'category'],
  where: 'saleDate >= "2026-01-01"',
  limit: 100,
});

console.log(summary.data);
// [
//   { region: 'North', category: 'Electronics', revenue: 50000, quantity: 120 },
//   { region: 'South', category: 'Clothing', revenue: 30000, quantity: 200 },
// ]
```

### Field-Level Metadata Access

```javascript
// Check available fields on a type
const userFields = client.getTypeMetadata('User');
console.log(userFields);
// {
//   id: { type: 'ID', nullable: false, requiresScope: [] },
//   email: { type: 'Email', nullable: false, requiresScope: ['read:user.email'] },
//   salary: { type: 'Decimal', nullable: false, requiresScope: ['read:admin'] },
// }

// Check required scopes for a field
const emailScopes = userFields.email.requiresScope;
```

### RBAC Authorization

```javascript
const result = await client.query('users', { limit: 10 }, {
  authToken: 'Bearer token...',
  userId: 'user-123',
  scopes: ['read:user', 'read:admin'],
});

// Fields requiring 'read:user' are accessible
// Fields requiring 'read:admin' are filtered out if scope missing
```

### Observers and Webhooks

```javascript
// Client receives webhook notifications when they fire
const subscription = await client.subscribe('orderCreatedWebhook', {}, {
  onMessage: (event) => {
    console.log('Webhook fired:', event.type, event.data);
  },
});
```

## Scalar Types

FraiseQL supports 60+ scalar types with Node.js runtime mappings:

| GraphQL Type | Node.js Type | Example |
|--------------|--------------|---------|
| `Int` | `number` | `42` |
| `Float` | `number` | `3.14` |
| `String` | `string` | `"hello"` |
| `Boolean` | `boolean` | `true` |
| `ID` | `string` | `"user-123"` |
| `DateTime` | `Date` | `new Date()` |
| `Date` | `Date` | `new Date()` |
| `Time` | `Date` | `new Date()` |
| `Decimal` | `string` | `"99.99"` |
| `JSON` | `object` | `{}` |
| `Email` | `string` | `"user@example.com"` |
| `URL` | `string` | `"https://example.com"` |
| `UUID` | `string` | `"550e8400-e29b-41d4-a716-446655440000"` |
| `Phone` | `string` | `"+1-555-0100"` |
| `IPv4` | `string` | `"192.168.1.1"` |

See [Scalars Reference](../../reference/scalars.md) for complete 60+ type list.

## Express.js Integration

### Middleware Setup

```javascript
const express = require('express');
const { FraiseQLClient } = require('FraiseQL-nodejs');

const app = express();
const client = new FraiseQLClient({
  schemaPath: './schema.compiled.json',
  database: { /* ... */ },
});

// Middleware: attach client to request
app.use(async (req, res, next) => {
  req.FraiseQL = client;
  next();
});

// Ensure connected
app.listen(3000, async () => {
  await client.connect();
  console.log('Server running');
});
```

### GraphQL Endpoint

```javascript
app.post('/graphql', express.json(), async (req, res) => {
  try {
    const { query, variables } = req.body;
    const result = await req.FraiseQL.query(query, variables, {
      authToken: req.headers.authorization,
      userId: req.user?.id,
      scopes: req.user?.scopes,
    });
    res.json(result);
  } catch (error) {
    res.status(400).json({ errors: [{ message: error.message }] });
  }
});
```

### REST API Endpoints

```javascript
app.get('/api/users/:id', async (req, res) => {
  try {
    const result = await req.FraiseQL.query('userById', {
      id: req.params.id,
    });
    if (!result.data) return res.status(404).json({ error: 'Not found' });
    res.json(result.data);
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

app.post('/api/users', express.json(), async (req, res) => {
  try {
    const result = await req.FraiseQL.mutate('createUser', req.body);
    res.status(201).json(result.data);
  } catch (error) {
    res.status(400).json({ error: error.message });
  }
});
```

## Error Handling

### Try-Catch Pattern

```javascript
try {
  const result = await client.query('users', { limit: 10 });
  console.log(result.data);
} catch (error) {
  if (error instanceof FraiseQLError) {
    console.error('FraiseQL Error:', error.code, error.message);
    if (error.code === 'VALIDATION_ERROR') {
      console.error('Fields:', error.fields);
    } else if (error.code === 'AUTHORIZATION_ERROR') {
      console.error('Required scopes:', error.requiredScopes);
    }
  } else {
    console.error('Unknown error:', error);
  }
}
```

### Error Types

```javascript
// VALIDATION_ERROR
{ code: 'VALIDATION_ERROR', fields: { email: 'Invalid format' } }

// AUTHORIZATION_ERROR
{ code: 'AUTHORIZATION_ERROR', requiredScopes: ['read:user'] }

// AUTHENTICATION_ERROR
{ code: 'AUTHENTICATION_ERROR', message: 'Missing token' }

// NOT_FOUND
{ code: 'NOT_FOUND', message: 'User not found' }

// RATE_LIMIT
{ code: 'RATE_LIMIT', retryAfter: 60 }

// DATABASE_ERROR
{ code: 'DATABASE_ERROR', dbCode: 'CONSTRAINT_VIOLATION' }
```

## Testing Patterns

### Jest Testing with Mocks

```javascript
const { FraiseQLClient } = require('FraiseQL-nodejs');
jest.mock('FraiseQL-nodejs');

describe('User API', () => {
  let client;

  beforeEach(() => {
    client = new FraiseQLClient();
    client.query = jest.fn().mockResolvedValue({
      data: [{ id: '1', name: 'John' }],
    });
  });

  it('should fetch users', async () => {
    const result = await client.query('users', { limit: 10 });
    expect(result.data).toHaveLength(1);
    expect(client.query).toHaveBeenCalledWith('users', { limit: 10 });
  });
});
```

### Integration Testing

```javascript
const { FraiseQLClient } = require('FraiseQL-nodejs');

describe('Database Integration', () => {
  let client;

  beforeAll(async () => {
    client = new FraiseQLClient({
      schemaPath: './schema.compiled.json',
      database: { /* test db */ },
    });
    await client.connect();
  });

  afterAll(async () => {
    await client.disconnect();
  });

  it('should create and retrieve user', async () => {
    const created = await client.mutate('createUser', {
      email: 'test@example.com',
      name: 'Test User',
    });

    const retrieved = await client.query('userById', {
      id: created.data.id,
    });

    expect(retrieved.data.email).toBe('test@example.com');
  });
});
```

### Mocha Testing

```javascript
const { FraiseQLClient } = require('FraiseQL-nodejs');
const { expect } = require('chai');

describe('FraiseQL Client', () => {
  let client;

  before(async () => {
    client = new FraiseQLClient({
      schemaPath: './schema.compiled.json',
      database: { /* ... */ },
    });
    await client.connect();
  });

  it('should query users', async () => {
    const result = await client.query('users', { limit: 5 });
    expect(result.data).to.be.an('array');
    expect(result.execution.duration).to.be.a('number');
  });

  after(async () => {
    await client.disconnect();
  });
});
```

## Common Patterns

### CRUD Operations

```javascript
// Create
const user = await client.mutate('createUser', {
  email: 'user@example.com',
  name: 'John Doe',
});

// Read
const retrieved = await client.query('userById', { id: user.data.id });

// Update
await client.mutate('updateUser', {
  id: user.data.id,
  name: 'Jane Doe',
});

// Delete
await client.mutate('deleteUser', { id: user.data.id });
```

### Pagination

```javascript
async function getPagedUsers(pageNumber, pageSize = 20) {
  const offset = (pageNumber - 1) * pageSize;
  return client.query('users', {
    limit: pageSize,
    offset: offset,
    sort: 'createdAt',
    order: 'DESC',
  });
}
```

### Multi-Tenancy

```javascript
// Include tenant ID in every query
const result = await client.query('tenantUsers', {
  tenantId: 'tenant-123',
  limit: 10,
}, {
  userId: 'user-456',
  tenantId: 'tenant-123',
});
```

## See Also

- [TypeScript SDK Reference](./typescript-reference.md) - Schema authoring (authoring language)
- [GraphQL Scalars Reference](../../reference/scalars.md) - Complete scalar types list
- [Security & RBAC Guide](../../guides/authorization-quick-start.md) - Authorization patterns
- [Analytics & OLAP Guide](../../guides/analytics-patterns.md) - Fact tables and aggregations
- [Architecture Principles](../../architecture/README.md) - System design
- [Node.js SDK GitHub](https://github.com/FraiseQL/FraiseQL-nodejs)

---

## Troubleshooting

### Common Setup Issues

#### npm Registry Issues

**Issue**: `npm ERR! 404 Not Found - GET https://registry.npmjs.org/FraiseQL-nodejs`

**Solution**:

```bash
npm install @FraiseQL/nodejs@latest
npm cache clean --force
npm install
```

#### Module Not Found

**Issue**: `Cannot find module '@FraiseQL/nodejs'`

**Solution**:

```bash
npm install @FraiseQL/nodejs
node -e "console.log(require('@FraiseQL/nodejs'))"
```

#### Node Version Issues

**Issue**: `Unexpected token` or similar parser error

**Check Node.js version** (14+ required):

```bash
node --version
```

**Update Node.js**:

```bash
nvm install 18
nvm use 18
```

#### ESM/CommonJS Issues

**Issue**: `ERR_REQUIRE_ESM: require() of ES modules is not supported`

**Solution - Use correct module system**:

```json
{
  "type": "module"
}
```

Or for CommonJS:

```javascript
// ✅ CommonJS
const { Server } = require('@FraiseQL/nodejs');

const server = Server.fromCompiled('schema.compiled.json');
```

---

### Runtime Errors

#### Promise/Async Issues

**Issue**: `UnhandledPromiseRejectionWarning`

**Solution - Handle promises**:

```javascript
// ❌ Wrong - unhandled rejection
server.execute(query);

// ✅ Correct - handle rejection
server.execute(query)
  .catch(err => console.error('Error:', err));

// Or with async/await
try {
  const result = await server.execute(query);
} catch (error) {
  console.error('Error:', error);
}
```

#### Type Errors at Runtime

**Issue**: `TypeError: Cannot read property 'id' of undefined`

**Solution - Check types**:

```javascript
// ❌ Risky
const userId = result.data.user.id;

// ✅ Safe
const userId = result?.data?.user?.id || null;
```

#### Connection Issues

**Issue**: `Error: ECONNREFUSED - Connection refused`

**Check environment**:

```bash
echo $DATABASE_URL
psql $DATABASE_URL -c "SELECT 1"
```

**Set URL**:

```javascript
process.env.DATABASE_URL = 'postgresql://...';
const server = Server.fromCompiled('schema.compiled.json');
```

#### Timeout Issues

**Issue**: `TimeoutError: Operation timed out after 30000ms`

**Solution - Increase timeout**:

```javascript
const server = Server.fromCompiled('schema.compiled.json', {
  timeout: 60000,  // 60 seconds
});

// Per-query
await server.execute(query, { timeout: 30000 });
```

---

### Performance Issues

#### Memory Leaks

**Issue**: `FATAL ERROR: CALL_AND_RETRY_LAST Allocation failed - JavaScript heap out of memory`

**Debug with clinic.js**:

```bash
npm install -g clinic
clinic doctor -- node app.js
```

**Solutions**:

- Paginate large result sets
- Close connections properly
- Use connection pooling

#### Slow Queries

**Issue**: Queries take >5 seconds

**Enable caching**:

```javascript
const server = Server.fromCompiled('schema.compiled.json', {
  cache: {
    enabled: true,
    ttl: 300  // 5 minutes
  }
});
```

#### Build Size

**Issue**: Bundle is >5MB

**Optimize**:

```bash
# Tree-shake
import { Server } from '@FraiseQL/nodejs';  // Only what needed

# Check bundle
npm ls
```

---

### Debugging Techniques

#### Logging

```javascript
const server = Server.fromCompiled('schema.compiled.json', {
  debug: true,
  logLevel: 'debug'
});

// Or use env
process.env.FRAISEQL_DEBUG = 'true';
process.env.RUST_LOG = 'FraiseQL=debug';
```

#### Inspect Results

```javascript
const result = await server.execute(query);
console.log(JSON.stringify(result, null, 2));
```

#### Network Debugging

```bash
curl -X POST http://localhost:3000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query":"{ user(id: 1) { id } }"}' \
  -v
```

---

### Getting Help

#### GitHub Issues

Provide:

1. Node.js version: `node -v`
2. npm version: `npm -v`
3. FraiseQL version: `npm list @FraiseQL/nodejs`
4. Minimal code example
5. Full error trace

---

## See Also

- [TypeScript SDK Reference](./typescript-reference.md) - Schema authoring
- [Security & RBAC Guide](../../guides/authorization-quick-start.md) - Authorization patterns
- [Analytics & OLAP Guide](../../guides/analytics-patterns.md) - Fact tables and aggregations
- [Architecture Principles](../../architecture/README.md) - System design
- [Node.js SDK GitHub](https://github.com/FraiseQL/FraiseQL-nodejs)

---

**Remember:** The Node.js SDK is for runtime query execution only. Use the TypeScript SDK for schema authoring. Configuration flows from TypeScript decorators → compiled schema → Node.js client execution.

**Last Updated**: 2026-02-05 | **Maintained By**: FraiseQL Community | **Status**: Production Ready ✅
