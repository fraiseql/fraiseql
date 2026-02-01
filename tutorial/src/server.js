/**
 * FraiseQL Interactive Tutorial Server
 *
 * Provides:
 * - Curriculum chapters
 * - Query execution against FraiseQL server
 * - Schema exploration
 * - Progress tracking
 */

import express from 'express';
import cors from 'cors';
import bodyParser from 'body-parser';
import fetch from 'node-fetch';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { readFileSync } from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const app = express();
const port = process.env.TUTORIAL_PORT || 3001;
const fraiseqlApi = process.env.FRAISEQL_API_URL || 'http://fraiseql-server:8000';

// Middleware
app.use(cors());
app.use(bodyParser.json());

// ============================================================================
// Static Files
// ============================================================================
app.use(express.static(join(__dirname, '../web')));
app.use(express.static(join(__dirname, '../assets')));

// ============================================================================
// Health Check
// ============================================================================
app.get('/health', (req, res) => {
  res.json({ status: 'healthy', service: 'fraiseql-tutorial' });
});

// ============================================================================
// Tutorial API
// ============================================================================

// Get all chapters
app.get('/api/chapters', (req, res) => {
  const chapters = [
    {
      id: 1,
      title: 'What is FraiseQL?',
      description: 'Understand the core concept of compiled GraphQL',
      difficulty: 'beginner',
      duration: '2 min',
      completed: false,
    },
    {
      id: 2,
      title: 'How Compilation Works',
      description: 'Learn how FraiseQL transforms schemas to SQL',
      difficulty: 'beginner',
      duration: '3 min',
      completed: false,
    },
    {
      id: 3,
      title: 'Your First Query',
      description: 'Write and execute your first GraphQL query',
      difficulty: 'beginner',
      duration: '5 min',
      completed: false,
    },
    {
      id: 4,
      title: 'Filtering & WHERE Clauses',
      description: 'Filter results using GraphQL filters',
      difficulty: 'intermediate',
      duration: '5 min',
      completed: false,
    },
    {
      id: 5,
      title: 'Relationships & Joins',
      description: 'Query related data and understand N+1 elimination',
      difficulty: 'intermediate',
      duration: '5 min',
      completed: false,
    },
    {
      id: 6,
      title: 'What\'s Next?',
      description: 'Explore advanced topics and next steps',
      difficulty: 'intermediate',
      duration: '3 min',
      completed: false,
    },
  ];

  res.json(chapters);
});

// Get chapter content
app.get('/api/chapters/:id', (req, res) => {
  const chapterId = parseInt(req.params.id);

  const chapters = {
    1: {
      id: 1,
      title: 'What is FraiseQL?',
      content: `
# What is FraiseQL?

FraiseQL is a **compiled GraphQL execution engine** that transforms your schema definitions into optimized SQL at build time.

## The Key Insight

Traditional GraphQL servers interpret queries at runtime:
- Read the query
- Validate against schema
- Generate SQL
- Execute
- Format results

FraiseQL does the heavy lifting at **compile time**:
- Validate schema
- Generate optimal SQL
- Verify all queries
- Create compiled schema

At runtime, it just executes pre-compiled queries.

## Why It Matters

✅ **Zero runtime overhead** - SQL is already optimized
✅ **Catch errors early** - Validation at compile time
✅ **Better performance** - No interpretation needed
✅ **Type safe** - Errors caught before deployment
✅ **Works with existing databases** - PostgreSQL, MySQL, SQLite, SQL Server

## Your Demo Stack

You're running a blog application with:
- **Users**: 3 sample users (Alice, Bob, Charlie)
- **Posts**: 4 sample posts (blog articles)
- **Database**: PostgreSQL (pre-populated)
- **Server**: FraiseQL (running at localhost:8000)

Try querying it in the next chapters!

## How FraiseQL Works

See the "Compiled SQL" tab after executing a query to understand the difference!
      `,
      sampleQuery: null,
      notes: 'This is the foundational concept. Take your time to understand it.',
    },
    2: {
      id: 2,
      title: 'How Compilation Works',
      content: `
# How Compilation Works

## The Compilation Pipeline

FraiseQL transforms your schema through these steps:

1. **Write Schema** (Python/TypeScript)
   - Define your types, fields, relationships
   - Specify database mappings
   - Configure security rules

2. **Generate JSON Schema**
   - Intermediate representation
   - Validated against FraiseQL rules

3. **Compile with fraiseql-cli**
   - Optimizes schema structure
   - Generates efficient SQL templates
   - Validates all queries

4. **Create Compiled Schema** (schema.compiled.json)
   - Ready to run at production
   - No further compilation needed
   - Immutable and reproducible

5. **Deploy & Execute**
   - Load in FraiseQL Server
   - Execute pre-optimized queries
   - No runtime interpretation

## What Gets Pre-Compiled?

The compiled schema contains:

| Component | Purpose |
|-----------|---------|
| **Type Definitions** | User, Post, Comment, etc. |
| **Field Mappings** | How to fetch each field from DB |
| **Relationships** | User → Posts, Author → Comments |
| **SQL Templates** | Pre-optimized SQL for each query pattern |
| **Validators** | Which queries are allowed |
| **Configuration** | Security, caching, rate limiting |

## Example: User with Posts

Your GraphQL query:
\`\`\`graphql
query GetUser {
  user(id: 1) {
    name
    email
    posts { title }
  }
}
\`\`\`

Gets pre-compiled to optimized SQL:
\`\`\`sql
SELECT u.id, u.name, u.email, p.title
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
WHERE u.id = 1
\`\`\`

**Zero N+1 problem!**
- Single query with JOIN
- Only requested fields
- Automatically optimized
- No runtime decisions

## Key Insight

By compiling at build time:
✅ Validate everything before deployment
✅ Generate optimal SQL once (not per request)
✅ Know performance characteristics upfront
✅ Eliminate runtime interpretation overhead
      `,
      sampleQuery: null,
      notes: 'Understanding compilation helps you appreciate performance gains.',
    },
    3: {
      id: 3,
      title: 'Your First Query',
      content: `
# Your First Query

Let's fetch all users from our blog database!

## The Query

Click "Execute Query" below to run:

\`\`\`graphql
query GetUsers {
  users(limit: 10) {
    id
    name
    email
    created_at
  }
}
\`\`\`

## What to Expect

You'll see a response like:

\`\`\`json
{
  "data": {
    "users": [
      {
        "id": 1,
        "name": "Alice Johnson",
        "email": "alice@example.com",
        "created_at": "2024-01-15T10:30:00Z"
      },
      ...
    ]
  }
}
\`\`\`

## Behind the Scenes

This query was pre-compiled to optimal SQL:

\`\`\`sql
SELECT id, name, email, created_at
FROM users
LIMIT 10;
\`\`\`

No N+1 queries, no extra fields, perfectly optimized!

## Try It

Modify the limit parameter or add more fields to explore!
      `,
      sampleQuery: 'query GetUsers { users(limit: 10) { id name email created_at } }',
      notes: 'This is live data from our PostgreSQL database.',
    },
    4: {
      id: 4,
      title: 'Filtering & WHERE Clauses',
      content: `
# Filtering & WHERE Clauses

## Using Filters

FraiseQL lets you filter results with GraphQL:

\`\`\`graphql
query GetPostsByAuthor {
  posts(filter: { author_id: 1 }, limit: 10) {
    id
    title
    created_at
    author {
      name
    }
  }
}
\`\`\`

## How It Works

The filter translates to SQL WHERE clauses:

\`\`\`sql
SELECT id, title, created_at, author
FROM posts
WHERE author_id = 1
LIMIT 10;
\`\`\`

## Available Operators

Depending on your schema and database:
- **Equality**: \`field: value\`
- **Comparison**: \`field_gt: 5\`, \`field_lt: 10\`
- **Membership**: \`field_in: [1, 2, 3]\`
- **Text**: \`field_like: "%pattern%"\`

## Try It Yourself

Modify the author_id to see different results!
      `,
      sampleQuery: 'query GetPostsByAuthor { posts(filter: { author_id: 1 }, limit: 10) { id title created_at } }',
      notes: 'Filters are compiled to efficient WHERE clauses.',
    },
    5: {
      id: 5,
      title: 'Relationships & Joins',
      content: `
# Relationships & Joins

## Understanding Relationships

Our blog schema has relationships:

\`\`\`
User (Author)
  ↓ (one-to-many)
Post (Article)
\`\`\`

A **User** can have many **Posts**:
- Alice has 2 posts
- Bob has 1 post
- Charlie has 1 post

When you query a post, you want the author information too.

## The N+1 Problem

Most GraphQL servers have a performance problem called **N+1 queries**:

### Traditional Approach (❌ Slow)

\`\`\`
Query all posts:
  SELECT * FROM posts           ← 1 query

For each of 20 posts:
  SELECT * FROM users WHERE id = ?  ← 20 queries!

Total: 21 database queries
Performance: O(N) - Gets worse with more posts
\`\`\`

### FraiseQL Approach (✅ Fast)

\`\`\`
Query with relationships compiled:
  SELECT p.*, u.*
  FROM posts p
  LEFT JOIN users u ON p.author_id = u.id

Total: 1 database query
Performance: O(1) - Same cost regardless of data size
\`\`\`

## How FraiseQL Eliminates N+1

1. **At Compile Time**: Analyzes your schema relationships
2. **Generates SQL**: Creates optimal JOIN queries
3. **At Runtime**: Executes single efficient query
4. **No Overhead**: Relationship fetching is free!

## Example Query

\`\`\`graphql
query AllPostsWithAuthors {
  posts(limit: 20) {
    id
    title
    content
    author {              ← This causes N+1 in traditional GraphQL!
      id
      name
      email
    }
  }
}
\`\`\`

## What FraiseQL Generates

\`\`\`sql
SELECT
  p.id, p.title, p.content,
  u.id, u.name, u.email
FROM posts p
LEFT JOIN users u ON p.author_id = u.id
LIMIT 20;
\`\`\`

Just **1 query** with an efficient JOIN!

## Key Benefits

✅ **Automatic Optimization**: No need to think about N+1
✅ **Consistent Performance**: Same speed for 1 or 1000 relationships
✅ **Type Safe**: Relationships defined in schema
✅ **No Manual Batching**: FraiseQL handles it for you

## Try It

Execute the query below to see relationships in action. Check the "Compiled SQL" tab to see how FraiseQL generated the JOIN query!
      `,
      sampleQuery: 'query AllPostsWithAuthors { posts(limit: 20) { id title author { id name email } } }',
      notes: 'FraiseQL eliminates the N+1 problem automatically at compile time.',
    },
    6: {
      id: 6,
      title: 'What\'s Next?',
      content: `
# What's Next?

Congratulations! You've learned the basics of FraiseQL! 🎉

## Topics to Explore

### 1. Advanced Queries
- Nested relationships
- Complex filtering
- Aggregations
- Pagination

### 2. Mutations
- Creating data
- Updating records
- Deleting data
- Transactions

### 3. Subscriptions
- Real-time updates
- WebSocket connections
- Change notifications

### 4. Federation
- Multi-service architecture
- Distributed queries
- Cross-service relationships

### 5. Security
- Authentication
- Authorization
- Rate limiting
- Field-level security

### 6. Performance
- Caching strategies
- Query optimization
- Connection pooling
- Monitoring

## Try Your Own Schema

Ready to build something? Here's how:

1. **Define your schema** (Python or TypeScript)
2. **Compile it** (\`fraiseql-cli compile\`)
3. **Deploy it** (Docker, Kubernetes, serverless)
4. **Query it** (GraphQL)

## Learn More

- **Docs**: https://github.com/anthropics/fraiseql/docs
- **Examples**: https://github.com/anthropics/fraiseql/examples
- **GitHub**: https://github.com/anthropics/fraiseql

## Next Steps

1. Try modifying the sample queries
2. Explore the Admin Dashboard
3. Read the full documentation
4. Build your first schema

Happy querying! 🚀
      `,
      sampleQuery: null,
      notes: 'You\'ve completed the basics. The world is your database!',
    },
  };

  const chapter = chapters[chapterId];
  if (!chapter) {
    return res.status(404).json({ error: 'Chapter not found' });
  }

  res.json(chapter);
});

// ============================================================================
// Query Execution
// ============================================================================

app.post('/api/execute', async (req, res) => {
  try {
    const { query, variables } = req.body;

    if (!query) {
      return res.status(400).json({ error: 'Query is required' });
    }

    // Forward to FraiseQL server
    const response = await fetch(`${fraiseqlApi}/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query, variables: variables || {} }),
    });

    const result = await response.json();
    res.json(result);
  } catch (error) {
    console.error('Query execution error:', error.message);
    res.status(500).json({
      error: 'Failed to execute query',
      message: error.message,
    });
  }
});

// ============================================================================
// Schema Exploration
// ============================================================================

// Get full schema with introspection
app.get('/api/schema', async (req, res) => {
  try {
    // Fetch introspection query from FraiseQL server
    const response = await fetch(`${fraiseqlApi}/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        query: `
          {
            __schema {
              types {
                name
                kind
                description
                fields {
                  name
                  description
                  type {
                    name
                    kind
                    ofType {
                      name
                      kind
                    }
                  }
                }
              }
            }
          }
        `,
      }),
    });

    const result = await response.json();
    res.json(result);
  } catch (error) {
    console.error('Schema fetch error:', error.message);
    res.status(500).json({
      error: 'Failed to fetch schema',
      message: error.message,
    });
  }
});

// Get types summary (for schema explorer)
app.get('/api/schema/types', async (req, res) => {
  try {
    const response = await fetch(`${fraiseqlApi}/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        query: `
          {
            __schema {
              types {
                name
                kind
                description
              }
            }
          }
        `,
      }),
    });

    const result = await response.json();

    // Filter to only custom types (exclude built-in types)
    if (result.data && result.data.__schema) {
      const types = result.data.__schema.types.filter(t =>
        !t.name.startsWith('__') &&
        ['OBJECT', 'INTERFACE', 'ENUM', 'SCALAR'].includes(t.kind)
      );
      res.json({ types });
    } else {
      res.json({ types: [] });
    }
  } catch (error) {
    console.error('Schema types fetch error:', error.message);
    res.status(500).json({
      error: 'Failed to fetch schema types',
      message: error.message,
    });
  }
});

// Get type details (fields, relationships)
app.get('/api/schema/type/:name', async (req, res) => {
  try {
    const typeName = req.params.name;

    const response = await fetch(`${fraiseqlApi}/graphql`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        query: `
          {
            __type(name: "${typeName}") {
              name
              kind
              description
              fields {
                name
                description
                type {
                  name
                  kind
                  ofType {
                    name
                    kind
                  }
                }
              }
            }
          }
        `,
      }),
    });

    const result = await response.json();
    res.json(result.data);
  } catch (error) {
    console.error('Type detail fetch error:', error.message);
    res.status(500).json({
      error: 'Failed to fetch type details',
      message: error.message,
    });
  }
});

// ============================================================================
// Serve Main HTML
// ============================================================================

app.get('/', (req, res) => {
  res.sendFile(join(__dirname, '../web/index.html'));
});

app.get('/chapters/:id', (req, res) => {
  res.sendFile(join(__dirname, '../web/index.html'));
});

// ============================================================================
// Error Handling
// ============================================================================

app.use((err, req, res, next) => {
  console.error('Server error:', err);
  res.status(500).json({
    error: 'Internal server error',
    message: process.env.NODE_ENV === 'development' ? err.message : undefined,
  });
});

app.use((req, res) => {
  res.status(404).json({ error: 'Not found' });
});

// ============================================================================
// Start Server
// ============================================================================

app.listen(port, () => {
  console.log(`\n✅ FraiseQL Tutorial Server running on http://localhost:${port}`);
  console.log(`📚 API: http://localhost:${port}/api`);
  console.log(`🗣️  FraiseQL Server: ${fraiseqlApi}`);
  console.log(`\n🌐 Open your browser: http://localhost:${port}\n`);
});
