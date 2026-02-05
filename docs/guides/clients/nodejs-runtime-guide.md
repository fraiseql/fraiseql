# Node.js Runtime Client for FraiseQL

**Status:** ✅ Production Ready
**Audience:** Backend developers, Node.js API servers
**Reading Time:** 25-30 minutes
**Last Updated:** 2026-02-05

Complete guide for querying FraiseQL servers from Node.js backend services using the FraiseQL runtime client.

---

## Installation & Setup

### Prerequisites

- Node.js 16+
- FraiseQL server running
- Package manager (npm, yarn, or pnpm)

### Install Package

```bash
npm install @fraiseql/client

# or
yarn add @fraiseql/client
pnpm add @fraiseql/client
```

### Create Client Instance

```typescript
import { FraiseQLClient } from '@fraiseql/client';

const client = new FraiseQLClient({
  url: 'http://localhost:5000/graphql',
  timeout: 10000, // 10 second timeout
  retryPolicy: {
    maxRetries: 3,
    initialDelay: 100,
    maxDelay: 5000,
  },
});

export default client;
```

### With Authentication

```typescript
import { FraiseQLClient } from '@fraiseql/client';

const client = new FraiseQLClient({
  url: 'http://localhost:5000/graphql',
  headers: {
    'Authorization': `Bearer ${process.env.FRAISEQL_TOKEN}`,
    'X-API-Key': process.env.API_KEY,
  },
  timeout: 10000,
});

export default client;
```

---

## Queries

### Basic Query

```typescript
import client from './client';
import { gql } from '@fraiseql/client';

const GET_USERS = gql`
  query GetUsers {
    users {
      id
      name
      email
    }
  }
`;

async function fetchUsers() {
  try {
    const result = await client.query(GET_USERS);
    console.log('Users:', result.data.users);
    return result.data.users;
  } catch (error) {
    console.error('Query failed:', error);
    throw error;
  }
}
```

### Query with Variables

```typescript
const GET_USER_BY_ID = gql`
  query GetUserById($id: ID!) {
    user(id: $id) {
      id
      name
      email
      posts {
        id
        title
      }
    }
  }
`;

async function fetchUserById(userId: string) {
  const result = await client.query(GET_USER_BY_ID, {
    variables: { id: userId },
  });

  return result.data.user;
}
```

### Typed Queries (TypeScript)

```typescript
interface User {
  id: string;
  name: string;
  email: string;
}

interface GetUsersResponse {
  users: User[];
}

async function getUsers(): Promise<User[]> {
  const result = await client.query<GetUsersResponse>(GET_USERS);
  return result.data.users;
}

async function getUserById(id: string): Promise<User> {
  const result = await client.query<{ user: User }>(GET_USER_BY_ID, {
    variables: { id },
  });

  return result.data.user;
}
```

### Query with Custom Options

```typescript
async function fetchUsersWithOptions() {
  const result = await client.query(GET_USERS, {
    variables: {},
    fetchPolicy: 'network-only', // Skip cache
    timeout: 30000, // Override timeout
    headers: {
      'X-Request-ID': generateRequestId(),
    },
  });

  return result.data.users;
}
```

---

## Mutations

### Basic Mutation

```typescript
const CREATE_POST = gql`
  mutation CreatePost($title: String!, $content: String!) {
    createPost(title: $title, content: $content) {
      id
      title
      content
      createdAt
    }
  }
`;

interface CreatePostInput {
  title: string;
  content: string;
}

async function createPost(input: CreatePostInput) {
  const result = await client.mutation(CREATE_POST, {
    variables: input,
  });

  return result.data.createPost;
}
```

### Multiple Mutations

```typescript
const UPDATE_USER = gql`
  mutation UpdateUser($id: ID!, $name: String!) {
    updateUser(id: $id, name: $name) {
      id
      name
    }
  }
`;

async function updateUserBatch(updates: Array<{ id: string; name: string }>) {
  const results = await Promise.all(
    updates.map((update) =>
      client.mutation(UPDATE_USER, {
        variables: update,
      })
    )
  );

  return results.map((r) => r.data.updateUser);
}
```

### Mutation with Error Handling

```typescript
async function safeCreatePost(input: CreatePostInput) {
  try {
    const result = await client.mutation(CREATE_POST, {
      variables: input,
    });

    if (result.errors) {
      result.errors.forEach((error) => {
        console.error('GraphQL Error:', error.message);
      });
      throw new Error('Mutation had errors');
    }

    return result.data.createPost;
  } catch (error) {
    if (error instanceof NetworkError) {
      console.error('Network error:', error.message);
      // Retry or queue for later
    } else if (error instanceof ValidationError) {
      console.error('Validation error:', error.message);
    } else {
      throw error;
    }
  }
}
```

---

## Subscriptions

### Long-Polling Subscriptions

```typescript
const ON_POST_CREATED = gql`
  subscription OnPostCreated {
    postCreated {
      id
      title
      author {
        name
      }
    }
  }
`;

async function subscribeToPostsLongPoll() {
  const subscription = await client.subscribe(ON_POST_CREATED);

  for await (const message of subscription) {
    if (message.type === 'data') {
      console.log('New post:', message.data.postCreated);
      // Process new post
    } else if (message.type === 'error') {
      console.error('Subscription error:', message.error);
    }
  }
}
```

### WebSocket Subscriptions

```typescript
import { FraiseQLWSClient } from '@fraiseql/client/ws';

const wsClient = new FraiseQLWSClient({
  url: 'ws://localhost:5000/graphql',
  reconnect: true,
  reconnectInterval: 5000,
});

wsClient.subscribe(ON_POST_CREATED, {
  onNext: (data) => {
    console.log('New post:', data.postCreated);
  },
  onError: (error) => {
    console.error('Subscription error:', error);
  },
  onComplete: () => {
    console.log('Subscription complete');
  },
});
```

---

## Batch Queries

### Execute Multiple Queries in One Request

```typescript
const GET_STATS = gql`
  query GetStats {
    userCount
    postCount
    commentCount
  }
`;

const GET_RECENT_POSTS = gql`
  query GetRecentPosts {
    posts(limit: 10) {
      id
      title
      createdAt
    }
  }
`;

async function fetchDashboardData() {
  const [statsResult, postsResult] = await Promise.all([
    client.query(GET_STATS),
    client.query(GET_RECENT_POSTS),
  ]);

  return {
    stats: statsResult.data,
    recentPosts: postsResult.data.posts,
  };
}

// Or batch in single request
async function fetchDashboardBatched() {
  const result = await client.batch([
    { query: GET_STATS },
    { query: GET_RECENT_POSTS },
  ]);

  return {
    stats: result[0].data,
    recentPosts: result[1].data.posts,
  };
}
```

---

## Connection Pooling & Caching

### Connection Pool

```typescript
import { FraiseQLClient, ConnectionPool } from '@fraiseql/client';

const pool = new ConnectionPool({
  url: 'http://localhost:5000/graphql',
  poolSize: 10,
  timeout: 10000,
});

async function executeWithPooling(query: string) {
  const connection = await pool.acquire();
  try {
    return await connection.query(query);
  } finally {
    pool.release(connection);
  }
}
```

### Response Caching

```typescript
const client = new FraiseQLClient({
  url: 'http://localhost:5000/graphql',
  cache: {
    enabled: true,
    ttl: 60000, // 1 minute
    maxSize: 100, // Cache up to 100 responses
  },
});

// First call - fetches from server
await client.query(GET_USERS);

// Second call (within 1 min) - returns cached response
await client.query(GET_USERS);
```

---

## Error Handling

### Network Errors

```typescript
import {
  NetworkError,
  ValidationError,
  TimeoutError,
  FraiseQLError,
} from '@fraiseql/client';

async function resilientQuery() {
  try {
    return await client.query(GET_USERS);
  } catch (error) {
    if (error instanceof TimeoutError) {
      console.error('Request timeout');
      // Retry with longer timeout
    } else if (error instanceof NetworkError) {
      console.error('Network error:', error.message);
      // Implement backoff retry
    } else if (error instanceof ValidationError) {
      console.error('Validation error:', error.message);
      // Log and investigate query
    } else if (error instanceof FraiseQLError) {
      console.error('FraiseQL error:', error.code, error.message);
    }
  }
}
```

### Retry with Backoff

```typescript
async function queryWithRetry(
  query: any,
  variables?: any,
  maxRetries = 3
): Promise<any> {
  let lastError: any;

  for (let i = 0; i < maxRetries; i++) {
    try {
      return await client.query(query, { variables });
    } catch (error) {
      lastError = error;

      // Exponential backoff
      const delay = Math.pow(2, i) * 1000; // 1s, 2s, 4s
      console.log(`Retry attempt ${i + 1} after ${delay}ms`);
      await new Promise((resolve) => setTimeout(resolve, delay));
    }
  }

  throw lastError;
}
```

---

## Integration with Express.js

### GraphQL Endpoint Wrapper

```typescript
import express from 'express';
import client from './client';

const app = express();
app.use(express.json());

// Generic GraphQL query endpoint
app.post('/api/graphql', async (req, res) => {
  const { query, variables } = req.body;

  try {
    const result = await client.query(query, { variables });
    res.json(result);
  } catch (error) {
    res.status(500).json({
      errors: [{ message: error.message }],
    });
  }
});

// Specific data endpoint
app.get('/api/users', async (req, res) => {
  try {
    const users = await getUsers();
    res.json(users);
  } catch (error) {
    res.status(500).json({ error: error.message });
  }
});

app.listen(3000);
```

### Middleware for FraiseQL Queries

```typescript
import { Request, Response, NextFunction } from 'express';
import client from './client';

// Middleware to inject client
export function fraiseqlMiddleware(req: Request, res: Response, next: NextFunction) {
  req.fraiseqlClient = client;
  next();
}

// Usage
app.use(fraiseqlMiddleware);

app.get('/api/user/:id', async (req, res) => {
  const user = await req.fraiseqlClient.query(GET_USER_BY_ID, {
    variables: { id: req.params.id },
  });
  res.json(user);
});
```

---

## Integration with Fastify

```typescript
import Fastify from 'fastify';
import client from './client';

const fastify = Fastify();

fastify.register(async (fastify) => {
  fastify.post('/graphql', async (request, reply) => {
    const { query, variables } = request.body;

    try {
      const result = await client.query(query, { variables });
      return result;
    } catch (error) {
      throw fastify.httpErrors.internalServerError(error.message);
    }
  });

  fastify.get('/users', async (request, reply) => {
    const result = await client.query(GET_USERS);
    return result.data.users;
  });
});

fastify.listen({ port: 3000 });
```

---

## Integration with Nest.js

### Create Service

```typescript
import { Injectable } from '@nestjs/common';
import { FraiseQLClient } from '@fraiseql/client';
import { GET_USERS, GET_USER_BY_ID } from './queries';

@Injectable()
export class UsersService {
  private client: FraiseQLClient;

  constructor() {
    this.client = new FraiseQLClient({
      url: process.env.FRAISEQL_URL,
    });
  }

  async getUsers() {
    const result = await this.client.query(GET_USERS);
    return result.data.users;
  }

  async getUserById(id: string) {
    const result = await this.client.query(GET_USER_BY_ID, {
      variables: { id },
    });
    return result.data.user;
  }
}
```

### Create Controller

```typescript
import { Controller, Get, Param } from '@nestjs/common';
import { UsersService } from './users.service';

@Controller('api/users')
export class UsersController {
  constructor(private readonly usersService: UsersService) {}

  @Get()
  getUsers() {
    return this.usersService.getUsers();
  }

  @Get(':id')
  getUserById(@Param('id') id: string) {
    return this.usersService.getUserById(id);
  }
}
```

---

## Testing

### Mock Client for Unit Tests

```typescript
import { jest } from '@jest/globals';
import { FraiseQLClient } from '@fraiseql/client';

describe('UserService', () => {
  let service: UsersService;
  let mockClient: jest.Mocked<FraiseQLClient>;

  beforeEach(() => {
    mockClient = {
      query: jest.fn(),
      mutation: jest.fn(),
    } as any;

    service = new UsersService();
    service.client = mockClient;
  });

  it('should fetch users', async () => {
    mockClient.query.mockResolvedValueOnce({
      data: {
        users: [
          { id: '1', name: 'Alice', email: 'alice@example.com' },
        ],
      },
    });

    const users = await service.getUsers();

    expect(users).toHaveLength(1);
    expect(users[0].name).toBe('Alice');
  });
});
```

### Integration Tests

```typescript
import { describe, it, expect, beforeAll, afterAll } from '@jest/globals';
import client from './client';

describe('FraiseQL Integration', () => {
  beforeAll(async () => {
    // Setup test server or connect to test instance
  });

  afterAll(async () => {
    // Cleanup
  });

  it('should query users from real server', async () => {
    const result = await client.query(GET_USERS);
    expect(result.data.users).toBeDefined();
    expect(Array.isArray(result.data.users)).toBe(true);
  });

  it('should create post', async () => {
    const result = await client.mutation(CREATE_POST, {
      variables: {
        title: 'Test Post',
        content: 'Test Content',
      },
    });

    expect(result.data.createPost.id).toBeDefined();
    expect(result.data.createPost.title).toBe('Test Post');
  });
});
```

---

## Performance Optimization

### Query Complexity Analysis

```typescript
async function complexQuery() {
  const result = await client.query(COMPLEX_QUERY, {
    complexity: {
      maxComplexity: 1000, // Limit query complexity
      onExceeded: 'reject', // or 'warn'
    },
  });

  return result;
}
```

### Request Deduplication

```typescript
// Automatically deduplicate identical concurrent requests
const client = new FraiseQLClient({
  url: 'http://localhost:5000/graphql',
  deduplicateRequests: true,
});

// Both calls will use the same network request
const [users1, users2] = await Promise.all([
  client.query(GET_USERS),
  client.query(GET_USERS), // Deduplicated
]);
```

### Persistent Queries

```typescript
// Use pre-defined query IDs to reduce payload
const result = await client.query({
  id: 'GetUsers', // Predefined query ID
  variables: {},
});
```

---

## See Also

**Related Guides:**
- **[CLI Query Tool](./cli-query-guide.md)** - Command-line queries
- **[Real-Time Patterns](../PATTERNS.md)** - Subscription support
- **[Production Deployment](../production-deployment.md)** - Running FraiseQL

**Framework Guides:**
- **[Express.js Documentation](https://expressjs.com/)**
- **[Fastify Documentation](https://www.fastify.io/)**
- **[Nest.js Documentation](https://docs.nestjs.com/)**

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
