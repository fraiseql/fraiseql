-- FraiseQL Relay Extension - Basic Setup Example
--
-- This example shows how to set up the extension and register basic entities

-- 1. Create the extension (requires superuser privileges)
CREATE EXTENSION IF NOT EXISTS fraiseql_relay;

-- 2. Check extension health
SELECT * FROM core.fraiseql_relay_health();

-- 3. Register basic entities
-- Example: User entity
SELECT core.register_entity(
    p_entity_name := 'User',
    p_graphql_type := 'User',
    p_pk_column := 'pk_user',
    p_v_table := 'v_user',
    p_source_table := 'tb_user',
    p_tv_table := 'tv_user',  -- optional materialized table
    p_identifier_column := 'email'
);

-- Example: Post entity
SELECT core.register_entity(
    p_entity_name := 'Post',
    p_graphql_type := 'Post',
    p_pk_column := 'pk_post',
    p_v_table := 'v_post',
    p_source_table := 'tb_post',
    p_tv_table := 'tv_post'
);

-- Example: Comment entity
SELECT core.register_entity(
    p_entity_name := 'Comment',
    p_graphql_type := 'Comment',
    p_pk_column := 'pk_comment',
    p_v_table := 'v_comment',
    p_source_table := 'tb_comment'
);

-- 4. List all registered entities
SELECT * FROM core.list_registered_entities();

-- 5. Test node resolution
-- Replace with actual UUIDs from your database
SELECT * FROM core.resolve_node('550e8400-e29b-41d4-a716-446655440000'::uuid);

-- 6. Query the unified nodes view
SELECT id, __typename, entity_name
FROM core.v_nodes
LIMIT 10;

-- 7. Test smart node resolution with cache layer optimization
SELECT * FROM core.resolve_node_smart(
    '550e8400-e29b-41d4-a716-446655440000'::uuid,
    '{"query_type": "single"}'::jsonb
);

-- 8. Check what cache layers are available for each entity
SELECT
    entity_name,
    graphql_type,
    CASE WHEN turbo_function IS NOT NULL THEN 'turbo' ELSE NULL END as has_turbo,
    CASE WHEN lazy_cache_key_pattern IS NOT NULL THEN 'lazy' ELSE NULL END as has_lazy,
    CASE WHEN tv_table IS NOT NULL THEN 'tv' ELSE NULL END as has_tv,
    CASE WHEN mv_table IS NOT NULL THEN 'mv' ELSE NULL END as has_mv,
    default_cache_layer
FROM core.tb_entity_registry
ORDER BY entity_name;

-- 9. Get optimal data source for different query types
SELECT * FROM core.get_optimal_data_source('User', 'single');
SELECT * FROM core.get_optimal_data_source('User', 'list');
SELECT * FROM core.get_optimal_data_source('User', 'analytics');

-- 10. Global ID encoding/decoding (if using base64 format)
SELECT core.fraiseql_encode_global_id('User', '550e8400-e29b-41d4-a716-446655440000'::uuid);
SELECT * FROM core.fraiseql_decode_global_id('VXNlcjo1NTBlODQwMC1lMjliLTQxZDQtYTcxNi00NDY2NTU0NDAwMDA=');

-- 11. Performance test - batch resolution
-- Create array of UUIDs to test batch resolution
SELECT * FROM core.fraiseql_resolve_nodes_batch(
    ARRAY[
        '550e8400-e29b-41d4-a716-446655440000'::uuid,
        '550e8400-e29b-41d4-a716-446655440001'::uuid,
        '550e8400-e29b-41d4-a716-446655440002'::uuid
    ]
);

-- 12. Refresh the nodes view (useful after schema changes)
SELECT core.refresh_v_nodes_view();

-- Check the refreshed view
SELECT COUNT(*) as total_nodes,
       COUNT(DISTINCT __typename) as unique_types
FROM core.v_nodes;
