# Framework Submission Guide

**Version**: 1.0.0
**Last Updated**: 2025-10-16

Welcome! Thank you for your interest in submitting your GraphQL framework to our benchmark suite. This guide ensures fair, reproducible, and credible performance comparisons.

---

## 🎯 Core Principles

Our benchmarks follow strict **fairness and reproducibility** standards:

1. ✅ **Same Hardware**: All frameworks run on identical Docker containers with identical resource limits
2. ✅ **Same Database**: Single PostgreSQL instance, same schema, same data
3. ✅ **Latest Versions**: Current stable releases (or specify version requirements)
4. ✅ **Optimal Configuration**: Each framework configured for best performance
5. ✅ **Transparency**: All code, configs, and raw data published
6. ✅ **Community Review**: Framework maintainers review and optimize their implementations

---

## 📋 Submission Requirements

### 1. Framework Information

Please provide:

```yaml
framework:
  name: "Your Framework Name"
  version: "1.2.3"  # Specific version to benchmark
  language: "Python/Java/Node.js/etc"
  repository: "https://github.com/your-org/your-framework"
  documentation: "https://docs.your-framework.com"
  license: "MIT/Apache-2.0/etc"

contacts:
  maintainer_name: "Your Name"
  maintainer_email: "you@example.com"
  maintainer_github: "@yourusername"
```

### 2. Docker Container

**Required**: A production-ready Dockerfile that:

- Runs your GraphQL server optimally configured
- Exposes a single HTTP endpoint (default: `http://0.0.0.0:8000/graphql`)
- Connects to PostgreSQL via environment variable `DATABASE_URL`
- Uses official base images (e.g., `python:3.11-slim`, `openjdk:17-slim`)
- Includes health check endpoint (e.g., `/health`)

**Example Dockerfile**:

```dockerfile
FROM python:3.11-slim

WORKDIR /app

# Install dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY . .

# Expose GraphQL endpoint
EXPOSE 8000

# Health check
HEALTHCHECK --interval=10s --timeout=3s \
  CMD curl -f http://localhost:8000/health || exit 1

# Run server with optimal settings
CMD ["uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000", "--workers", "4"]
```

### 3. GraphQL Schema Implementation

Your implementation must support the **benchmark schema** (provided below). All resolvers must:

- Return correct data from PostgreSQL
- Handle pagination correctly
- Implement N+1 query prevention (DataLoader, batching, etc.)
- Support filtering and sorting where applicable

**Benchmark Schema**:

```graphql
type Query {
  # Simple query: Fetch users with optional limit
  users(limit: Int, offset: Int): [User!]!

  # Single user lookup
  user(id: ID!): User

  # Complex filtering
  usersWhere(where: UserFilter, orderBy: OrderBy, limit: Int): [User!]!

  # N+1 test: Users with their posts
  usersWithPosts(limit: Int): [User!]!

  # Complex nested query
  posts(limit: Int, offset: Int): [Post!]!

  # Single post with author and comments
  post(id: ID!): Post
}

type Mutation {
  createUser(input: CreateUserInput!): User!
  updateUser(id: ID!, input: UpdateUserInput!): User!
  deleteUser(id: ID!): Boolean!

  createPost(input: CreatePostInput!): Post!
}

type User {
  id: ID!
  name: String!
  email: String!
  age: Int
  city: String
  createdAt: String!
  posts: [Post!]!  # Must prevent N+1 queries
}

type Post {
  id: ID!
  title: String!
  content: String!
  published: Boolean!
  authorId: ID!
  author: User!  # Must prevent N+1 queries
  comments: [Comment!]!  # Must prevent N+1 queries
  createdAt: String!
}

type Comment {
  id: ID!
  content: String!
  postId: ID!
  post: Post!
  authorId: ID!
  author: User!
  createdAt: String!
}

input UserFilter {
  age_gt: Int
  age_lt: Int
  city: String
  name_contains: String
}

input OrderBy {
  field: String!
  direction: Direction!
}

enum Direction {
  ASC
  DESC
}

input CreateUserInput {
  name: String!
  email: String!
  age: Int
  city: String
}

input UpdateUserInput {
  name: String
  email: String
  age: Int
  city: String
}

input CreatePostInput {
  title: String!
  content: String!
  published: Boolean!
  authorId: ID!
}
```

### 4. Database Connection

#### Option A: Shared PostgreSQL Instance (Default)

Your application connects to the shared benchmark PostgreSQL instance:

- Connect using `DATABASE_URL` environment variable
- Format: `postgresql://user:password@postgres:5432/benchmark_db`
- Use connection pooling (recommended pool size: 10-20 connections)
- Handle connection errors gracefully

**When to use**: Standard frameworks without special database requirements.

#### Option B: Custom Database Container (Advanced)

If your framework has special database requirements (extensions, custom types, specialized configurations), you may provide your own database container:

**Requirements**:
1. **Same schema**: Must implement the exact table structure shown below
2. **Same data**: Use our data seeding scripts (provided)
3. **PostgreSQL only**: Must be PostgreSQL (same version as benchmark suite)
4. **Resource limits**: Your database gets same limits as shared instance
5. **Documentation**: Clearly explain why custom DB is needed
6. **Transparency**: Publish all custom configurations

**Example use cases**:
- Framework requires specific PostgreSQL extensions (PostGIS, pgvector, etc.)
- Framework uses custom PostgreSQL types
- Framework integrates with database-specific features (triggers, functions)

**Not allowed**:
- Using a different DBMS to gain unfair advantage
- Custom indexing beyond what's specified (unless you add same indexes to shared DB)
- Pre-computed materialized views or caches
- Database-level caching that other frameworks can't use

**Implementation**:

```yaml
# In your docker-compose.yml
services:
  your-framework-db:
    image: postgres:15
    environment:
      POSTGRES_DB: benchmark_db
      POSTGRES_USER: benchmark
      POSTGRES_PASSWORD: benchmark
    volumes:
      - ./database/schema.sql:/docker-entrypoint-initdb.d/01-schema.sql
      - ./database/seed.sql:/docker-entrypoint-initdb.d/02-seed.sql
      - ./database/your-custom-setup.sql:/docker-entrypoint-initdb.d/03-custom.sql
    # Same resource limits as shared database
    cpus: "2.0"
    mem_limit: "2g"
    shm_size: "256mb"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U benchmark"]
      interval: 10s
      timeout: 5s
      retries: 5

  your-framework:
    build: .
    environment:
      DATABASE_URL: postgresql://benchmark:benchmark@your-framework-db:5432/benchmark_db
    depends_on:
      your-framework-db:
        condition: service_healthy
```

**Documentation requirements** (in `optimizations.md`):

```markdown
## Custom Database Configuration

**Why custom DB is needed**: [Explain specific requirement, e.g., "Requires PostGIS extension for spatial queries"]

**Custom configurations**:
- Extensions: postgis, pg_trgm
- Custom types: None
- Additional indexes: None beyond standard schema
- Database settings: shared_buffers=512MB (same as shared instance)

**Fairness verification**:
- [ ] Same schema as benchmark suite
- [ ] Same seed data
- [ ] No additional indexes beyond standard
- [ ] No materialized views or pre-computation
- [ ] All custom SQL scripts published in repo
```

**Database Schema** (required for all submissions):

```sql
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    age INTEGER,
    city VARCHAR(255),
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    content TEXT,
    published BOOLEAN DEFAULT FALSE,
    author_id INTEGER REFERENCES users(id),
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE comments (
    id SERIAL PRIMARY KEY,
    content TEXT NOT NULL,
    post_id INTEGER REFERENCES posts(id),
    author_id INTEGER REFERENCES users(id),
    created_at TIMESTAMP DEFAULT NOW()
);

CREATE INDEX idx_posts_author_id ON posts(author_id);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_author_id ON comments(author_id);
```

### 5. Configuration Files

Include all necessary configuration files:

- `requirements.txt` / `package.json` / `pom.xml` / `build.gradle` (dependency manifest)
- Framework-specific config files
- Environment variable documentation

**Example Configuration Documentation**:

```markdown
## Environment Variables

- `DATABASE_URL`: PostgreSQL connection string (required)
- `PORT`: Server port (default: 8000)
- `WORKERS`: Number of worker processes (default: 4)
- `LOG_LEVEL`: Logging verbosity (default: "info")
- `POOL_SIZE`: Database connection pool size (default: 10)
```

### 6. Optimization Documentation

**Critical**: Document all optimizations you've applied:

```markdown
## Performance Optimizations

1. **N+1 Query Prevention**:
   - Using DataLoader with batch size of 100
   - Implemented in `resolvers/user.py:45`

2. **Connection Pooling**:
   - Pool size: 10 connections
   - Max overflow: 5
   - Pool timeout: 30 seconds

3. **Caching**:
   - No caching (for fair comparison)
   - OR: Document cache strategy with TTL

4. **Query Optimization**:
   - Using SELECT field lists (no SELECT *)
   - JOIN optimization for nested queries
   - Index-aware query generation

5. **Framework-Specific**:
   - [Any framework-specific optimizations]
```

### 7. Testing & Validation

Your submission must include:

**Correctness Tests**: Verify GraphQL queries return correct data

```python
# Example test (adapt to your framework)
def test_simple_users_query():
    query = """
    query {
        users(limit: 10) {
            id
            name
            email
        }
    }
    """
    response = execute_query(query)
    assert len(response["data"]["users"]) == 10
    assert all("id" in user for user in response["data"]["users"])
```

**N+1 Query Test**: Verify DataLoader/batching works

```python
def test_n_plus_one_prevention():
    query = """
    query {
        users(limit: 10) {
            id
            name
            posts {
                id
                title
            }
        }
    }
    """
    # Enable query logging
    response = execute_query(query)

    # Should execute exactly 2 queries:
    # 1. SELECT users
    # 2. SELECT posts WHERE author_id IN (...)
    assert query_count == 2  # Not 11 queries (1 + 10)
```

**Load Test**: Verify server handles concurrent requests

```bash
# Must handle 1000 concurrent requests without errors
wrk -t4 -c1000 -d30s http://localhost:8000/graphql \
  -s scripts/simple_query.lua
```

---

## 📦 Submission Package Structure

Submit your framework as a **pull request** or **GitHub repository** with this structure:

```
frameworks/
└── your-framework-name/
    ├── Dockerfile                    # Production-ready container
    ├── docker-compose.yml            # Optional: Local testing
    ├── README.md                     # Framework-specific docs
    ├── optimizations.md              # Performance optimizations applied
    ├── src/                          # Application code
    │   ├── schema.graphql            # GraphQL schema
    │   ├── resolvers/                # GraphQL resolvers
    │   ├── models/                   # Database models/DAOs
    │   └── main.py|js|java           # Server entry point
    ├── tests/                        # Correctness tests
    │   ├── test_correctness.py
    │   └── test_n_plus_one.py
    ├── requirements.txt              # Dependencies
    └── .env.example                  # Environment variable template
```

---

## 🧪 Benchmark Scenarios

Your framework will be tested on these scenarios:

### 1. Simple Query (P0)
```graphql
query {
  users(limit: 10) {
    id
    name
    email
  }
}
```
**Measures**: Basic framework overhead, latency (p50, p95, p99)

### 2. N+1 Query Test (P0)
```graphql
query {
  users(limit: 50) {
    id
    name
    posts {
      id
      title
    }
  }
}
```
**Measures**: DataLoader effectiveness, database query count

### 3. Complex Filtering (P1)
```graphql
query {
  usersWhere(
    where: { age_gt: 18, city: "New York" }
    orderBy: { field: "name", direction: ASC }
    limit: 20
  ) {
    id
    name
    age
    city
  }
}
```
**Measures**: SQL generation efficiency, query planning time

### 4. Mutations (P1)
```graphql
mutation {
  createUser(input: {
    name: "John Doe"
    email: "john@example.com"
    age: 30
    city: "Boston"
  }) {
    id
    name
    email
  }
}
```
**Measures**: Write performance, validation overhead

### 5. Deep Nesting (P2)
```graphql
query {
  posts(limit: 10) {
    id
    title
    author {
      id
      name
    }
    comments {
      id
      content
      author {
        id
        name
      }
    }
  }
}
```
**Measures**: Complex query optimization, resolver efficiency

---

## 🐳 Docker Compose Integration

Your submission will be integrated into our `docker-compose.yml`:

```yaml
services:
  your-framework:
    build:
      context: ./frameworks/your-framework-name
      dockerfile: Dockerfile
    ports:
      - "8000:8000"
    environment:
      DATABASE_URL: postgresql://benchmark:benchmark@postgres:5432/benchmark_db
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 10s
      timeout: 3s
      retries: 3
    # Fair resource limits (same for all frameworks)
    cpus: "2.0"
    mem_limit: "2g"
    networks:
      - benchmark-net
```

---

## ✅ Submission Checklist

Before submitting, ensure:

- [ ] Dockerfile builds successfully (`docker build -t your-framework .`)
- [ ] Server starts and serves GraphQL endpoint (`docker run -p 8000:8000 your-framework`)
- [ ] All GraphQL queries return correct data (run correctness tests)
- [ ] N+1 queries are prevented (run query count tests)
- [ ] Health check endpoint responds (`curl http://localhost:8000/health`)
- [ ] Database connection works via `DATABASE_URL`
- [ ] All optimizations documented in `optimizations.md`
- [ ] README includes setup instructions
- [ ] License is compatible with benchmark suite (MIT/Apache/BSD)
- [ ] No hardcoded credentials or secrets

---

## 🚀 Submission Process

### Option 1: Pull Request (Recommended)

1. Fork this repository
2. Create your framework directory: `frameworks/your-framework-name/`
3. Implement GraphQL server following this guide
4. Test locally with our database schema
5. Submit pull request with title: `[Framework] Add YourFramework v1.2.3`
6. Include benchmark results from local testing (optional but helpful)

### Option 2: External Repository

If your framework is complex or has proprietary components:

1. Create a public GitHub repository with your implementation
2. Open an issue in this repository with link to your submission
3. Include Dockerfile and all requirements from this guide
4. We'll review and integrate into benchmark suite

---

## 🔍 Review Process

After submission, we will:

1. **Code Review** (1-3 days)
   - Verify correctness tests pass
   - Check N+1 query prevention
   - Review optimizations

2. **Preliminary Benchmarks** (1-2 days)
   - Run all benchmark scenarios
   - Verify reproducibility (±5% variance)
   - Check resource usage

3. **Feedback & Iteration** (as needed)
   - Share preliminary results with you (privately)
   - Give you opportunity to optimize
   - Re-run benchmarks after changes

4. **Final Integration** (1 day)
   - Merge into main benchmark suite
   - Publish results publicly
   - Credit your contribution

**Estimated turnaround**: 1-2 weeks from submission to publication

---

## 📊 Results Presentation

Your framework will appear in benchmark results:

```markdown
## Simple Query Latency (Lower is Better)

| Framework     | Version | p50 (ms) | p95 (ms) | p99 (ms) | Throughput (req/s) |
|---------------|---------|----------|----------|----------|--------------------|
| FraiseQL      | 0.1.0   | 0.8      | 1.5      | 2.1      | 12,500             |
| YourFramework | 1.2.3   | 5.2      | 8.7      | 12.3     | 3,200              |
| Strawberry    | 0.220.0 | 98.0     | 132.0    | 145.0    | 850                |
```

Results include:
- Latency percentiles (p50, p95, p99)
- Throughput (requests per second)
- Database query counts
- Memory usage
- CPU usage

---

## 🤝 Post-Publication

After your framework is benchmarked:

- You can reference results in your documentation (with attribution)
- We encourage you to optimize and submit updates
- We'll re-run benchmarks quarterly with latest versions
- You can contest results by providing improved implementations

---

## ❓ FAQs

### Q: What if my framework doesn't support X feature?

**A**: Document limitations in README. We'll benchmark what your framework supports and note gaps. Partial implementations are acceptable if documented.

### Q: Can I use caching to improve performance?

**A**: Only if you document cache strategy (TTL, invalidation, size). Prefer no caching for fairness, or implement same cache for all frameworks.

### Q: My framework is faster with custom database queries. Can I use them?

**A**: Yes, if your framework's value proposition is custom query optimization. Document this clearly. We test frameworks as they're intended to be used.

### Q: What if results show my framework is slower?

**A**: We show **tradeoffs**, not just speed. If your framework is slower but easier to use, more type-safe, or has better tooling, we'll document that. Honest results build credibility.

### Q: Can I see results before publication?

**A**: Yes! We share preliminary results privately and give you 1-2 weeks to optimize before public release.

### Q: What if I find an error in benchmarks?

**A**: Open an issue! We fix errors immediately and re-run benchmarks. Credibility depends on accuracy.

### Q: Can I provide my own database container?

**A**: **Yes, if you have a legitimate technical requirement** (e.g., need PostgreSQL extensions, custom types, database-specific features).

**Requirements**:
- Must be PostgreSQL (same version as benchmark suite)
- Must use exact same schema and seed data
- Must have same resource limits (2 CPU, 2GB RAM)
- Must document why custom DB is needed
- Cannot add custom indexes or optimizations not available to other frameworks

**What's allowed**:
✅ PostgreSQL extensions (PostGIS, pg_trgm, etc.) if your framework requires them
✅ Custom PostgreSQL types if your framework uses them
✅ Database-level features (triggers, functions) if they're part of your framework's value proposition

**What's NOT allowed**:
❌ Different DBMS (MySQL, MongoDB, etc.) to gain unfair advantage
❌ Additional indexes beyond standard schema (unless you propose adding them to shared DB)
❌ Pre-computed materialized views or aggregations
❌ Database-level caching that other frameworks can't use
❌ Higher resource limits than shared database

**Fairness principle**: Custom database configs are allowed when they're **required** for your framework to function, not to artificially boost performance. If your optimization could benefit other frameworks, propose adding it to the shared database instead.

---

## 📞 Support

Questions? Reach out:

- **GitHub Issues**: [graphql-benchmarks/issues](https://github.com/your-org/graphql-benchmarks/issues)
- **Email**: benchmarks@your-domain.com
- **Discord**: [Join our server](#)

We're here to help you showcase your framework fairly!

---

## 📜 License

All submitted code must be compatible with our MIT license. By submitting, you agree that:

1. Your implementation code is licensed under MIT (or compatible)
2. We can publish benchmark results publicly
3. We can modify your implementation for fairness (with your review)
4. You retain copyright of your framework code

---

**Thank you for contributing to fair, reproducible GraphQL benchmarks!**

*Together, we help developers choose the right framework for their needs.*
