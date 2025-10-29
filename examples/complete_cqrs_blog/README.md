# FraiseQL Complete CQRS Blog Example

> **A production-ready example demonstrating FraiseQL's CQRS pattern with explicit sync**

This example showcases:
- ✅ **Database migrations** with `fraiseql migrate`
- ✅ **CQRS pattern** with `tb_*` (command) and `tv_*` (query) tables
- ✅ **Explicit sync** pattern (NO database triggers!)
- ✅ **Real-time metrics** for monitoring sync performance
- ✅ **GraphQL API** with Strawberry

## 🎯 What You'll Learn

1. **CQRS Architecture**: Separate command (write) and query (read) sides
2. **Explicit Sync Pattern**: Why we don't use triggers and how explicit sync gives you control
3. **Performance Monitoring**: Track sync operations and optimize your application
4. **Production Patterns**: How to structure a real-world FraiseQL application

---

## 🚀 Quick Start (5 Minutes)

### Prerequisites
- Docker & Docker Compose
- Git

### Run the Example

```bash
# 1. Clone and navigate
git clone https://github.com/yourusername/fraiseql.git
cd fraiseql/examples/complete_cqrs_blog

# 2. Start everything with Docker
docker-compose up

# 3. Wait for startup (you'll see "🚀 FraiseQL Blog API Ready!")

# 4. Visit GraphQL Playground
open http://localhost:8000/graphql
```

That's it! The example is now running with:
- PostgreSQL with sample data
- GraphQL API on port 8000
- Grafana dashboard on port 3000

---

## 📖 Understanding CQRS with FraiseQL

### The Problem: N+1 Queries

Traditional GraphQL frameworks suffer from N+1 query problems:

```graphql
query {
  posts {           # 1 query
    author {        # N queries (one per post!)
      name
    }
    comments {      # N queries again!
      author {      # N*M queries!!!
        name
      }
    }
  }
}
```

Result: **Hundreds of database queries for one GraphQL request.**

### The Solution: CQRS with Explicit Sync

FraiseQL uses **Command Query Responsibility Segregation (CQRS)**:

```
┌─────────────────────────────────────────────────────────────┐
│ FraiseQL CQRS Architecture                                   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  📝 Command Side (Writes):                                  │
│     tb_user, tb_post, tb_comment (normalized tables)       │
│          ↓                                                  │
│  🔄 Explicit Sync (YOUR CODE):                             │
│     await sync.sync_post([post_id])  👈 VISIBLE!           │
│          ↓                                                  │
│  📊 Query Side (Reads):                                     │
│     tv_user, tv_post, tv_comment (denormalized JSONB)      │
│          ↓                                                  │
│  ⚡ GraphQL Query:                                          │
│     ONE database query, sub-millisecond response           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Result**: The same GraphQL query above becomes **ONE database query** reading from denormalized JSONB.

---

## 🔧 How It Works

### Step 1: Normalized Command Tables (tb_*)

Write operations go to normalized tables:

```sql
-- Command side: Normalized for data integrity
CREATE TABLE tb_post (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    author_id UUID REFERENCES tb_user(id),
    published BOOLEAN
);
```

### Step 2: Explicit Sync

After writing, **explicitly** sync to query side:

```python
# Create a new post (write to command side)
post_id = await db.execute(
    "INSERT INTO tb_post (...) VALUES (...)",
    title, content, author_id
)

# EXPLICIT SYNC to query side 👈 THIS IS IN YOUR CODE!
await sync.sync_post([post_id], mode='incremental')
```

**Why explicit instead of triggers?**
- ✅ **Visibility**: Sync is in your code, not hidden in database
- ✅ **Testing**: Easy to mock sync in tests
- ✅ **Control**: Batch syncs, defer them, skip in special cases
- ✅ **Debugging**: See exactly when syncs happen
- ✅ **Performance**: 10-100x faster than triggers

### Step 3: Denormalized Query Tables (tv_*)

Read operations use denormalized JSONB:

```sql
-- Query side: Denormalized for fast reads
CREATE TABLE tv_post (
    id UUID PRIMARY KEY,
    data JSONB  -- Contains post + author + comments!
);

-- One query gets everything:
SELECT data FROM tv_post WHERE id = $1;
```

Result: **Zero N+1 queries, sub-millisecond response times.**

---

## 💻 Example Queries

### Query 1: Get Recent Posts

```graphql
query GetRecentPosts {
  posts(limit: 5) {
    id
    title
    author {
      username
      fullName
    }
    commentCount
    comments {
      content
      author {
        username
      }
    }
  }
}
```

**Database queries**: **ONE** (reads from `tv_post`)
**Response time**: **<1ms** (sub-millisecond!)

### Query 2: Get User with Stats

```graphql
query GetUser {
  user(id: "00000000-0000-0000-0000-000000000001") {
    username
    fullName
    publishedPostCount
    commentCount
  }
}
```

**Database queries**: **ONE** (reads from `tv_user`)
**Response time**: **<1ms**

### Mutation 1: Create a Post

```graphql
mutation CreatePost {
  createPost(
    title: "My New Post"
    content: "This is the content..."
    authorId: "00000000-0000-0000-0000-000000000001"
    published: true
  ) {
    id
    title
    author {
      username
    }
  }
}
```

**What happens**:
1. Insert into `tb_post` (command side)
2. **Explicit sync** to `tv_post` (query side)
3. **Explicit sync** author to `tv_user` (post count changed)
4. Return denormalized data from `tv_post`

**Total time**: **<10ms** (including 2 sync operations)

### Mutation 2: Add a Comment

```graphql
mutation AddComment {
  createComment(
    postId: "00000000-0000-0000-0001-000000000001"
    authorId: "00000000-0000-0000-0000-000000000002"
    content: "Great post!"
  ) {
    id
    content
    author {
      username
    }
  }
}
```

**What happens**:
1. Insert into `tb_comment` (command side)
2. **Explicit sync** post to `tv_post` (comment added)
3. **Explicit sync** author to `tv_user` (comment count changed)

---

## 📊 Monitoring & Metrics

### View Real-Time Metrics

```bash
# Sync performance metrics
curl http://localhost:8000/metrics | jq

# Example response:
{
  "sync_metrics_24h": {
    "overall": {
      "total_syncs": 1543,
      "avg_duration_ms": 8.2,
      "success_rate": 99.87
    },
    "by_entity": [
      {
        "entity_type": "post",
        "total_syncs": 523,
        "avg_duration_ms": 12.5,
        "success_rate": 100
      },
      {
        "entity_type": "user",
        "total_syncs": 156,
        "avg_duration_ms": 5.1,
        "success_rate": 99.4
      }
    ]
  }
}
```

### Query Metrics via GraphQL

```graphql
query SyncMetrics {
  syncMetrics(entityType: "post") {
    totalSyncs24h
    avgDurationMs
    successRate
    failures24h
  }
}
```

---

## 🏗️ Project Structure

```
complete_cqrs_blog/
├── app.py                     # FastAPI application with startup logic
├── schema.py                  # GraphQL schema (queries & mutations)
├── sync.py                    # Explicit sync functions (THE KEY!)
├── migrations/
│   ├── 001_initial_schema.sql # Database schema with tb_/tv_ tables
│   └── run_migrations.py      # Migration runner
├── docker-compose.yml         # Full stack: Postgres + API + Grafana
├── Dockerfile                 # Application container
├── requirements.txt           # Python dependencies
└── README.md                  # This file
```

### Key Files Explained

#### `sync.py` - The Heart of Explicit Sync

```python
class EntitySync:
    """Handles synchronization from tb_* to tv_* tables."""

    async def sync_post(self, post_ids: list[UUID], mode: str = "incremental"):
        """
        Sync posts from tb_post to tv_post.

        This is EXPLICIT - you call it from your mutation code!
        """
        # 1. Fetch data from command side (tb_*)
        # 2. Denormalize (join with related tables)
        # 3. Write to query side (tv_*)
        # 4. Log metrics for monitoring
```

#### `schema.py` - GraphQL with Explicit Sync

```python
@strawberry.mutation
async def create_post(self, info, title: str, ...) -> Post:
    # Step 1: Write to command side
    post_id = await conn.execute("INSERT INTO tb_post ...")

    # Step 2: EXPLICIT SYNC 👈 THIS IS THE KEY!
    await sync.sync_post([post_id])

    # Step 3: Read from query side
    return await conn.fetchrow("SELECT data FROM tv_post ...")
```

---

## 🧪 Testing the Example

### Test Query Performance

```bash
# Install httpie
pip install httpie

# Test a complex query
http POST http://localhost:8000/graphql \
  query='{ posts { title author { username } comments { content } } }'

# Check the response time in headers:
# X-Process-Time: 0.83ms  👈 Sub-millisecond!
```

### Test Mutations

```bash
# Create a new post
http POST http://localhost:8000/graphql \
  query='mutation { createPost(title: "Test", content: "...", authorId: "...") { id } }'

# Verify sync happened (check metrics)
http GET http://localhost:8000/metrics
```

### Load Testing

```bash
# Install wrk
brew install wrk  # or apt-get install wrk

# Test query load
wrk -t4 -c100 -d30s http://localhost:8000/graphql \
  -s query.lua

# Expected: 5000+ req/s with sub-millisecond latency
```

---

## 🎓 Learning More

### Why Explicit Sync?

**Common Question**: "Why not use database triggers to auto-sync?"

**Our Answer**:

| Triggers (Implicit)            | Explicit Sync (FraiseQL)       |
|--------------------------------|--------------------------------|
| ❌ Hidden (hard to debug)      | ✅ Visible in your code        |
| ❌ Hard to test (mocking DB)   | ✅ Easy to test (mock function)|
| ❌ No control (always runs)    | ✅ Full control (batch, defer) |
| ❌ Slow (triggers on each row) | ✅ Fast (batch operations)     |
| ❌ No metrics                  | ✅ Full observability          |

**Philosophy**: We believe explicit is better than implicit, especially in production systems where debugging and monitoring are critical.

### When to Sync?

```python
# ✅ DO: Sync immediately after write
post_id = await create_post(...)
await sync.sync_post([post_id])

# ✅ DO: Batch multiple syncs
post_ids = await create_many_posts(...)
await sync.sync_post(post_ids)  # Batch sync

# ✅ DO: Skip sync for background tasks
if not is_background_task:
    await sync.sync_post([post_id])

# ❌ DON'T: Forget to sync (your queries will be stale)
post_id = await create_post(...)
# Oops! Forgot to sync - users won't see the new post!
```

### Performance Tips

1. **Batch syncs** when creating multiple entities:
   ```python
   post_ids = []
   for data in batch:
       post_id = await create_post_record(data)
       post_ids.append(post_id)

   # Sync once for all posts (faster!)
   await sync.sync_post(post_ids)
   ```

2. **Defer syncs** for low-priority updates:
   ```python
   # High priority: sync immediately
   await sync.sync_post([post_id])

   # Low priority: add to queue for later
   await sync_queue.add(post_id)
   ```

3. **Monitor sync performance**:
   ```python
   # Check metrics to find slow syncs
   metrics = await get_sync_metrics()
   if metrics["avg_duration_ms"] > 50:
       logger.warning("Sync is getting slow!")
   ```

---

## 🚀 Next Steps

### 1. Explore the Code

```bash
# Read the sync implementation
cat sync.py

# Read the GraphQL mutations
cat schema.py

# Read the database schema
cat migrations/001_initial_schema.sql
```

### 2. Modify the Example

Try adding a new entity (e.g., "Category"):
1. Add `tb_category` and `tv_category` tables
2. Create `sync_category()` function
3. Add GraphQL types and mutations
4. Test it!

### 3. Benchmark It

Compare FraiseQL with other frameworks:
- Run the same queries in Hasura
- Run the same queries in Postgraphile
- Compare response times

**Expected**: FraiseQL should be **5-20x faster**.

### 4. Deploy to Production

This example is production-ready! Just:
1. Set environment variables
2. Use production PostgreSQL
3. Enable SSL
4. Setup monitoring (Grafana)
5. Deploy with Docker/Kubernetes

---

## 📚 Documentation

### FraiseQL Documentation
- **Main Docs**: https://fraiseql.dev/docs
- **CQRS Pattern**: https://fraiseql.dev/docs/architecture/cqrs
- **Explicit Sync**: https://fraiseql.dev/docs/guides/explicit-sync
- **Performance**: https://fraiseql.dev/docs/performance

### Related Projects
- **confiture**: https://github.com/fraiseql/confiture - Migration management
- **jsonb_ivm**: https://github.com/fraiseql/jsonb_ivm - Incremental View Maintenance
- **pg_fraiseql_cache**: https://github.com/fraiseql/pg_fraiseql_cache - Cache invalidation

---

## 🐛 Troubleshooting

### Database connection issues

```bash
# Check if Postgres is running
docker-compose ps

# Check database logs
docker-compose logs postgres

# Connect to database manually
docker-compose exec postgres psql -U fraiseql -d blog_demo
```

### Sync not working

```bash
# Check sync logs
curl http://localhost:8000/metrics

# Look for failures in sync_log table
docker-compose exec postgres psql -U fraiseql -d blog_demo \
  -c "SELECT * FROM sync_log WHERE success = false ORDER BY created_at DESC LIMIT 10;"
```

### Slow queries

```bash
# Check query performance
curl http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ posts { ... } }"}' \
  -w "\nTime: %{time_total}s\n"

# Check if tv_* tables have data
docker-compose exec postgres psql -U fraiseql -d blog_demo \
  -c "SELECT COUNT(*) FROM tv_post;"
```

---

## 🤝 Contributing

Found an issue or want to improve the example?
1. Open an issue: https://github.com/yourusername/fraiseql/issues
2. Submit a PR: https://github.com/yourusername/fraiseql/pulls

---

## 📝 License

MIT License - see LICENSE file for details

---

## 🌟 Summary

This example demonstrates FraiseQL's **revolutionary approach to GraphQL**:

✅ **Zero N+1 queries** (CQRS pattern)
✅ **Explicit sync** (full visibility and control)
✅ **Sub-millisecond queries** (denormalized JSONB)
✅ **Production-ready** (monitoring, metrics, health checks)
✅ **Developer-friendly** (clear, testable, debuggable)

**The result**: A GraphQL API that's **10-100x faster** than traditional frameworks, with **industrial-grade control** over data synchronization.

**Ready to build with FraiseQL?** Visit https://fraiseql.dev to learn more!
