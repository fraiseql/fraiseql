# TurboRouter Example

This example demonstrates FraiseQL's **TurboRouter** feature - a high-performance query execution mode that bypasses GraphQL parsing and validation for pre-registered queries.

## What is TurboRouter?

TurboRouter provides near-zero overhead query execution by:

1. **Pre-registering queries** with their SQL templates
2. **Hash-based lookup** (<0.1ms) instead of GraphQL parsing (~0.5-1ms)
3. **Direct SQL execution** with parameter mapping
4. **JSON passthrough** - No Python object instantiation

## Performance Comparison

| Stage | Standard GraphQL | TurboRouter |
|-------|-----------------|-------------|
| Query Parsing | 0.5ms | **0ms** |
| Schema Validation | 0.3ms | **0ms** |
| Query Planning | 0.2ms | **0ms** |
| Hash Lookup | - | **0.05ms** |
| SQL Execution | 5ms | 5ms |
| Object Instantiation | 2-8ms | **0ms** |
| Serialization | 2-6ms | **0ms** |
| **Total** | 10-20ms | **~5.1ms** |

**Result: 2-4x faster** for simple queries

## Files in This Example

- `main.py` - FastAPI app with TurboRouter setup
- `schema.py` - GraphQL type definitions
- `queries.py` - Query resolvers
- `turbo_config.py` - TurboRouter configuration and query registration
- `schema.sql` - PostgreSQL database schema

## Running the Example

```bash
# 1. Create database
createdb turborouter_demo

# 2. Initialize schema
psql turborouter_demo < schema.sql

# 3. Install dependencies
pip install fraiseql fastapi uvicorn

# 4. Run the server
python main.py
```

## Testing TurboRouter

```bash
# Open GraphQL Playground
open http://localhost:8000/graphql

# Compare standard vs turbo performance
# Look for "x-execution-mode: turbo" in response headers
```

## GraphQL Queries to Try

### Get User (Registered with TurboRouter)

```graphql
query GetUser($id: Int!) {
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
```

**Variables:**
```json
{"id": 1}
```

**Performance:** ~5-10ms (TurboRouter active)

### Get Posts (Registered with TurboRouter)

```graphql
query GetPosts($limit: Int!) {
  posts(limit: $limit) {
    id
    title
    content
    author {
      name
    }
  }
}
```

**Variables:**
```json
{"limit": 10}
```

**Performance:** ~5-15ms (TurboRouter active)

### Complex Query (Falls back to standard)

```graphql
query ComplexQuery {
  users {
    id
    name
    posts {
      id
      comments {
        id
        text
      }
    }
  }
}
```

**Performance:** ~20-30ms (Standard GraphQL execution)

## How It Works

### 1. Register Queries

```python
from fraiseql.fastapi import TurboQuery, TurboRegistry

registry = TurboRegistry(max_size=1000)

# Register a query with SQL template
user_query = TurboQuery(
    graphql_query="""
        query GetUser($id: Int!) {
            user(id: $id) { id name email }
        }
    """,
    sql_template="""
        SELECT jsonb_build_object(
            'id', id,
            'name', name,
            'email', email
        ) as data
        FROM v_users
        WHERE id = %(id)s
    """,
    param_mapping={"id": "id"}  # GraphQL var -> SQL param
)

registry.register(user_query)
```

### 2. Execution Flow

```
Request with query hash
  ↓
Hash lookup in registry (<0.1ms)
  ↓
Found? → Execute SQL template directly
         Wrap result in JSONPassthrough
         Return to client
  ↓
Not found? → Standard GraphQL execution
             (with parsing/validation)
```

### 3. JSON Passthrough

```python
# Instead of instantiating User objects:
users = [User(id=1, name="Alice"), User(id=2, name="Bob")]

# TurboRouter returns wrapped dicts:
users = [
    JSONPassthrough({"id": 1, "name": "Alice"}),
    JSONPassthrough({"id": 2, "name": "Bob"})
]
```

**Result:** Zero serialization overhead

## Configuration Options

```python
from fraiseql.fastapi import TurboRouterConfig

config = TurboRouterConfig(
    enabled=True,
    max_cache_size=1000,
    ttl_seconds=3600,
    auto_register=True,  # Auto-register simple queries
    json_passthrough=True,  # Enable zero-copy passthrough
)
```

## When to Use TurboRouter

✅ **Good for:**
- High-traffic queries (user profiles, product details)
- Simple, frequently-called queries
- Mobile apps with pre-defined queries
- APIs with stable query patterns

❌ **Not for:**
- Complex queries with nested resolvers
- Queries with custom business logic
- Dynamic queries built at runtime
- Development/debugging (use standard mode)

## Integration with APQ

TurboRouter works seamlessly with Automatic Persisted Queries (APQ):

1. Client sends query hash
2. Server checks TurboRouter registry
3. If registered → Execute via TurboRouter
4. If not → Store in APQ cache for next time

**Best of both worlds:** Client-driven caching + server-side optimization

## Monitoring

```python
from fraiseql.fastapi import get_turbo_stats

stats = get_turbo_stats()
print(f"Cache hits: {stats['hits']}")
print(f"Cache misses: {stats['misses']}")
print(f"Hit rate: {stats['hit_rate']:.2%}")
print(f"Avg turbo latency: {stats['avg_latency_ms']:.2f}ms")
```

## Best Practices

1. **Register your top 20-30 queries** (80/20 rule)
2. **Monitor hit rates** - Should be >80% for registered queries
3. **Use in production only** - Keep standard mode for development
4. **Combine with APQ** - For maximum performance
5. **Version your queries** - Include version in operation name

## Security Notes

- TurboRouter uses **parameterized queries** (SQL injection safe)
- **Same authorization** as standard GraphQL
- **Validates all inputs** before SQL execution
- **Rate limiting** applies normally

## Performance Tips

1. **Pre-warm cache** on deployment
2. **Use connection pooling**
3. **Enable JSON passthrough** for best performance
4. **Profile and register** your slowest queries
5. **Monitor query patterns** - Register emerging hot queries

## Learn More

- [TurboRouter Documentation](https://fraiseql.readthedocs.io/advanced/turbo-router/)
- [JSON Passthrough Guide](https://fraiseql.readthedocs.io/advanced/performance/)
- [APQ Integration](https://fraiseql.readthedocs.io/features/apq-multi-tenant/)
