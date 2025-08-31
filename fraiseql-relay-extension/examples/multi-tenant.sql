-- FraiseQL Relay Extension - Multi-Tenant Setup Example
--
-- This example shows how to configure the extension for multi-tenant applications

-- 1. Enable extension
CREATE EXTENSION IF NOT EXISTS fraiseql_relay;

-- 2. Register tenant-scoped entities
-- User entity (tenant-scoped)
SELECT core.register_entity(
    p_entity_name := 'User',
    p_graphql_type := 'User',
    p_pk_column := 'pk_user',
    p_v_table := 'v_user',
    p_source_table := 'tenant.tb_user',  -- Note: tenant schema
    p_tv_table := 'tenant.tv_user',
    p_identifier_column := 'email',
    p_tenant_scoped := true,
    p_soft_delete_column := 'deleted_at'
);

-- Organization entity (global, not tenant-scoped)
SELECT core.register_entity(
    p_entity_name := 'Organization',
    p_graphql_type := 'Organization',
    p_pk_column := 'pk_organization',
    p_v_table := 'v_organization',
    p_source_table := 'public.tb_organization',
    p_identifier_column := 'name',
    p_tenant_scoped := false  -- Global entity
);

-- Contract entity (tenant-scoped with advanced caching)
SELECT core.register_entity(
    p_entity_name := 'Contract',
    p_graphql_type := 'Contract',
    p_pk_column := 'pk_contract',
    p_v_table := 'v_contract',
    p_source_table := 'tenant.tb_contract',
    p_tv_table := 'tenant.tv_contract',
    p_mv_table := 'tenant.mv_contract_summary',
    p_turbo_function := 'turbo.fn_get_contracts',
    p_lazy_cache_key_pattern := 'contract:{id}',
    p_tenant_scoped := true,
    p_default_cache_layer := 'tv_table'
);

-- Machine entity (tenant-scoped with full cache stack)
SELECT core.register_entity(
    p_entity_name := 'Machine',
    p_graphql_type := 'Machine',
    p_pk_column := 'pk_machine',
    p_v_table := 'v_machine',
    p_source_table := 'tenant.tb_machine',
    p_tv_table := 'tenant.tv_machine',
    p_mv_table := 'tenant.mv_machine_analytics',
    p_turbo_function := 'turbo.fn_get_machines',
    p_lazy_cache_key_pattern := 'machine:{id}:tenant:{tenant_id}',
    p_tenant_scoped := true,
    p_default_cache_layer := 'turbo_function'
);

-- 3. List entities by tenant scope
SELECT
    entity_name,
    graphql_type,
    tenant_scoped,
    source_table,
    default_cache_layer
FROM core.tb_entity_registry
ORDER BY tenant_scoped DESC, entity_name;

-- 4. Example: Multi-tenant node resolution
-- In a real application, tenant_id would come from the application context

-- Resolve user in tenant context
SELECT * FROM core.resolve_node_smart(
    '11111111-1111-1111-1111-111111111111'::uuid,
    '{"tenant_id": "22222222-2222-2222-2222-222222222222", "query_type": "single"}'::jsonb
);

-- Resolve organization (global entity)
SELECT * FROM core.resolve_node_smart(
    '33333333-3333-3333-3333-333333333333'::uuid,
    '{"query_type": "single"}'::jsonb
);

-- 5. Cache layer optimization for multi-tenant
-- Different cache strategies per entity type

-- High-traffic entities: Use TurboRouter
SELECT * FROM core.get_optimal_data_source(
    'Machine',
    'list',
    '{"tenant_id": "22222222-2222-2222-2222-222222222222"}'::jsonb
);

-- Medium-traffic: Use materialized tables
SELECT * FROM core.get_optimal_data_source(
    'Contract',
    'single',
    '{"tenant_id": "22222222-2222-2222-2222-222222222222"}'::jsonb
);

-- Low-traffic: Use real-time views
SELECT * FROM core.get_optimal_data_source(
    'User',
    'single',
    '{"tenant_id": "22222222-2222-2222-2222-222222222222"}'::jsonb
);

-- 6. Tenant isolation verification
-- Ensure nodes are properly isolated by tenant

-- Count nodes by tenant (if tenant_id is available in views)
SELECT
    __typename,
    COUNT(*) as node_count
FROM core.v_nodes
GROUP BY __typename
ORDER BY __typename;

-- 7. Performance monitoring for multi-tenant
-- Check entity distribution

SELECT
    entity_name,
    CASE
        WHEN turbo_function IS NOT NULL THEN 'High Performance'
        WHEN tv_table IS NOT NULL THEN 'Medium Performance'
        ELSE 'Standard Performance'
    END as performance_tier,
    tenant_scoped,
    estimated_row_count
FROM core.tb_entity_registry
ORDER BY performance_tier, entity_name;

-- 8. Batch resolution with tenant context
-- Example of resolving multiple entities from different tenants
SELECT * FROM core.fraiseql_resolve_nodes_batch(
    ARRAY[
        '11111111-1111-1111-1111-111111111111'::uuid, -- User
        '22222222-2222-2222-2222-222222222222'::uuid, -- Contract
        '33333333-3333-3333-3333-333333333333'::uuid  -- Organization
    ]
);

-- 9. Advanced: Entity relationship mapping
-- For entities that reference each other across tenants

-- Update Contract to reference Organization (cross-tenant)
SELECT core.register_entity(
    p_entity_name := 'ContractItem',
    p_graphql_type := 'ContractItem',
    p_pk_column := 'pk_contract_item',
    p_v_table := 'v_contract_item',
    p_source_table := 'tenant.tb_contract_item',
    p_tv_table := 'tenant.tv_contract_item',
    p_tenant_scoped := true,
    p_lazy_cache_key_pattern := 'contract_item:{id}:contract:{contract_id}'
);

-- 10. Cleanup example
-- Remove an entity registration
-- SELECT core.unregister_entity('ContractItem');

-- Check final state
SELECT * FROM core.fraiseql_relay_health();
