# FraiseQL Relay Extension - Migration Guide

This guide helps you integrate the FraiseQL Relay Extension into existing FraiseQL applications for full GraphQL Relay specification compliance.

## Overview

The FraiseQL Relay Extension provides:
- **Global Object Identification**: Every entity gets a globally unique identifier
- **Node Interface**: Standard `node(id: UUID!): Node` query for refetching objects
- **Performance Optimization**: C-optimized node resolution with multi-layer cache integration
- **Backward Compatibility**: Existing queries continue working unchanged

## Prerequisites

Before starting the migration:

1. **PostgreSQL 14+** with development headers
2. **Existing FraiseQL application** with established view patterns
3. **Superuser database access** (for extension installation)
4. **Entity views following FraiseQL patterns** (`v_*`, `tv_*`, etc.)

## Step 1: Install the PostgreSQL Extension

### Build and Install Extension

```bash
# Clone or download the extension code
cd fraiseql-relay-extension/src

# Build the extension
make clean && make

# Install (requires PostgreSQL admin privileges)
sudo make install
```

### Enable in Your Database

```sql
-- Connect to your application database
psql -d your_database

-- Create the extension
CREATE EXTENSION fraiseql_relay;

-- Verify installation
SELECT * FROM core.fraiseql_relay_health();
```

Expected output:
```
   status    | entities_registered | v_nodes_exists | last_refresh
-------------|---------------------|----------------|-------------
 no_entities |                   0 | t              |
```

## Step 2: Analyze Your Existing Schema

### Inventory Your Entities

First, identify all entities in your current FraiseQL application:

```sql
-- Find all views that could be entities
SELECT
    schemaname,
    viewname,
    CASE
        WHEN viewname LIKE 'v_%' THEN 'Real-time View'
        WHEN viewname LIKE 'tv_%' THEN 'Materialized Table'
        WHEN viewname LIKE 'mv_%' THEN 'Materialized View'
    END as view_type
FROM pg_views
WHERE viewname ~ '^(v_|tv_|mv_)'
ORDER BY viewname;

-- Find corresponding command-side tables
SELECT schemaname, tablename
FROM pg_tables
WHERE tablename LIKE 'tb_%'
ORDER BY tablename;
```

### Check Your Entity Patterns

Ensure your entities follow the FraiseQL "Sacred Trinity" pattern:

```sql
-- Example check for a User entity
\d tb_user

-- Should show:
-- id (INTEGER IDENTITY) - internal sequence
-- pk_user (UUID) - business primary key
-- identifier (TEXT) - human-readable ID (optional)
-- ... other fields
```

## Step 3: Register Your Entities

### Automatic Discovery (Recommended)

Use the Python integration to automatically discover and register entities:

```python
from fraiseql_relay_extension.python_integration import enable_relay_support

# Enable Relay with automatic discovery
relay = await enable_relay_support(
    schema=your_existing_schema,
    db_pool=your_db_pool,
    auto_register=True  # Automatically discovers entities
)

print("Auto-registration complete!")
```

### Manual Registration

For more control, register entities manually:

```sql
-- Register your User entity
SELECT core.register_entity(
    p_entity_name := 'User',
    p_graphql_type := 'User',
    p_pk_column := 'pk_user',
    p_v_table := 'v_user',
    p_source_table := 'tb_user',
    p_tv_table := 'tv_user',  -- If you have materialized tables
    p_identifier_column := 'email'
);

-- Register other entities...
SELECT core.register_entity(
    p_entity_name := 'Contract',
    p_graphql_type := 'Contract',
    p_pk_column := 'pk_contract',
    p_v_table := 'v_contract',
    p_source_table := 'tenant.tb_contract',  -- Note schema prefix
    p_tv_table := 'tenant.tv_contract',
    p_turbo_function := 'turbo.fn_get_contracts',  -- If using TurboRouter
    p_tenant_scoped := true
);
```

### Verify Registration

```sql
-- Check registered entities
SELECT * FROM core.list_registered_entities();

-- Test the unified view
SELECT id, __typename, entity_name
FROM core.v_nodes
LIMIT 5;
```

## Step 4: Update Your Python Code

### Add Node Interface to Your Types

Update your existing FraiseQL types to implement the Node interface:

```python
# Before
@fraiseql.type
class User:
    id: UUID  # This was already your global ID
    email: str
    name: str

# After - implement Node interface
from fraiseql_relay_extension.python_integration import Node

@fraiseql.type
class User(Node):  # Now implements Node interface
    id: UUID  # Global ID (required by Node)
    email: str
    name: str

    @classmethod
    def from_dict(cls, data: dict) -> "User":
        """Create instance from database JSONB data."""
        return cls(
            id=UUID(data["id"]),
            email=data["email"],
            name=data["name"]
        )
```

### Update Your Schema Creation

```python
# Before
schema = fraiseql.build_schema([User, Contract], queries=[...])

# After - enable Relay support
from fraiseql_relay_extension.python_integration import enable_relay_support

schema = fraiseql.build_schema([User, Contract], queries=[...])

# Add Relay support
relay = await enable_relay_support(schema, db_pool)
```

### Update Context Creation

```python
# Before
async def get_context(request):
    return {
        "db": CQRSRepository(request.app.state.db_pool),
        "user": getattr(request.state, "user", None)
    }

# After - use Relay context helper
async def get_context(request):
    return await relay.create_relay_context(request)
```

## Step 5: Test the Migration

### Basic Node Resolution Test

```python
# Test that node resolution works
async def test_relay_integration():
    # Get a real UUID from your database
    user_id = "550e8400-e29b-41d4-a716-446655440000"  # Replace with real ID

    # Test node resolution
    context = await relay._ensure_context()
    user = await context.resolve_node(UUID(user_id))

    print(f"Resolved user: {user}")
    assert user is not None
    assert isinstance(user, User)
```

### GraphQL Query Test

```graphql
# Test the new node query
query TestNodeQuery {
  node(id: "550e8400-e29b-41d4-a716-446655440000") {
    __typename
    ... on User {
      id
      email
      name
    }
    ... on Contract {
      id
      title
      status
    }
  }
}
```

### Batch Resolution Test

```python
# Test batch resolution performance
async def test_batch_resolution():
    user_ids = [
        "550e8400-e29b-41d4-a716-446655440000",
        "550e8400-e29b-41d4-a716-446655440001",
        "550e8400-e29b-41d4-a716-446655440002"
    ]

    context = await relay._ensure_context()
    users = await context.resolve_nodes_batch([UUID(id) for id in user_ids])

    print(f"Batch resolved {len([u for u in users if u])} users")
```

## Step 6: Performance Optimization

### Cache Layer Integration

If you have existing TurboRouter or materialized tables, register them:

```sql
-- Update entity registration to include performance layers
SELECT core.register_entity(
    p_entity_name := 'User',
    p_graphql_type := 'User',
    p_pk_column := 'pk_user',
    p_v_table := 'v_user',
    p_source_table := 'tb_user',
    p_tv_table := 'tv_user',                    -- Materialized table
    p_turbo_function := 'turbo.fn_get_users',   -- TurboRouter function
    p_lazy_cache_key_pattern := 'user:{id}',    -- Lazy cache pattern
    p_default_cache_layer := 'turbo_function'   -- Prefer TurboRouter
);
```

### Monitor Performance

```sql
-- Check cache layer utilization
SELECT
    entity_name,
    CASE
        WHEN turbo_function IS NOT NULL THEN 'TurboRouter'
        WHEN tv_table IS NOT NULL THEN 'Materialized'
        ELSE 'Real-time'
    END as performance_tier
FROM core.tb_entity_registry
ORDER BY performance_tier, entity_name;

-- Test different cache layers
SELECT * FROM core.get_optimal_data_source('User', 'single');
SELECT * FROM core.get_optimal_data_source('User', 'list');
SELECT * FROM core.get_optimal_data_source('User', 'analytics');
```

## Step 7: Client Integration

### Update GraphQL Clients

Your existing GraphQL clients will continue working, but you can now use Relay features:

```typescript
// Relay/Apollo Client - node refetching
const { data } = useQuery(gql`
  query GetNode($id: UUID!) {
    node(id: $id) {
      __typename
      ... on User {
        id
        email
        name
      }
    }
  }
`, { variables: { id: userId } });

// Standard FraiseQL queries still work
const { data } = useQuery(gql`
  query GetUsers($limit: Int) {
    users(limit: $limit) {
      id
      email
      name
    }
  }
`);
```

## Troubleshooting

### Common Issues

**Extension Installation Failed**
```bash
# Check PostgreSQL development headers
sudo apt-get install postgresql-server-dev-all  # Ubuntu/Debian
sudo yum install postgresql-devel                # CentOS/RHEL
```

**No Entities Auto-Registered**
```sql
-- Check if views have the expected structure
SELECT * FROM information_schema.columns
WHERE table_name LIKE 'v_%'
AND column_name = 'data'  -- FraiseQL views should have JSONB data column
ORDER BY table_name;
```

**Node Resolution Returns NULL**
```sql
-- Check if UUID exists in v_nodes
SELECT * FROM core.v_nodes WHERE id = 'your-uuid-here';

-- Check entity registration
SELECT * FROM core.tb_entity_registry WHERE entity_name = 'YourEntity';
```

**Performance Issues**
```sql
-- Check if indexes were created
\d core.v_nodes

-- Refresh the view
SELECT core.refresh_v_nodes_view();
```

### Health Monitoring

```sql
-- Monitor extension health
SELECT * FROM core.fraiseql_relay_health();

-- Check registration completeness
SELECT
    COUNT(*) as total_entities,
    COUNT(*) FILTER (WHERE turbo_function IS NOT NULL) as has_turbo,
    COUNT(*) FILTER (WHERE tv_table IS NOT NULL) as has_materialized
FROM core.tb_entity_registry;
```

## Best Practices

1. **Start Small**: Begin with your most important entities
2. **Test Thoroughly**: Verify node resolution works for all entity types
3. **Monitor Performance**: Use the health checks and performance queries
4. **Gradual Rollout**: Enable Relay features incrementally in your clients
5. **Keep Views Updated**: Refresh v_nodes view after schema changes

## Rollback Strategy

If you need to rollback:

```sql
-- Remove the extension (this will drop all functions and tables)
DROP EXTENSION fraiseql_relay CASCADE;

-- Your original FraiseQL views and functions remain unchanged
```

The extension is designed to be non-invasive - your existing FraiseQL application continues working even if the extension is removed.

## Next Steps

After successful migration:

1. **Enable Advanced Features**: Explore lazy caching and TurboRouter integration
2. **Client Updates**: Update client applications to use Relay pagination
3. **Monitoring**: Set up health checks and performance monitoring
4. **Documentation**: Update your API documentation to include Node interface

## Support

For issues or questions:
- Check the `examples/` directory for working code samples
- Run `SELECT core.fraiseql_relay_health()` for diagnostic information
- Review the logs in your PostgreSQL error log for extension-specific messages
