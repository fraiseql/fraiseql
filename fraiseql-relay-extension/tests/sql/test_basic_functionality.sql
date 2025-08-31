-- FraiseQL Relay Extension - Basic Functionality Tests
--
-- This file tests the core functionality of the PostgreSQL extension

\echo 'Running FraiseQL Relay Extension Basic Tests...'

-- Clean slate
DROP EXTENSION IF EXISTS fraiseql_relay CASCADE;
CREATE EXTENSION fraiseql_relay;

-- Test 1: Extension Health Check
\echo 'Test 1: Extension Health Check'
DO $$
DECLARE
    health_result RECORD;
BEGIN
    SELECT * INTO health_result FROM core.fraiseql_relay_health();

    IF health_result.status != 'no_entities' THEN
        RAISE EXCEPTION 'Expected status "no_entities", got "%"', health_result.status;
    END IF;

    IF health_result.entities_registered != 0 THEN
        RAISE EXCEPTION 'Expected 0 entities, got %', health_result.entities_registered;
    END IF;

    IF health_result.v_nodes_exists != true THEN
        RAISE EXCEPTION 'Expected v_nodes view to exist';
    END IF;

    RAISE NOTICE 'Test 1: PASSED - Extension health check working';
END $$;

-- Test 2: Entity Registration
\echo 'Test 2: Entity Registration'
DO $$
BEGIN
    -- Register a test entity
    PERFORM core.register_entity(
        p_entity_name := 'TestUser',
        p_graphql_type := 'User',
        p_pk_column := 'pk_user',
        p_v_table := 'v_test_user',
        p_source_table := 'tb_test_user'
    );

    -- Check it was registered
    IF NOT EXISTS (
        SELECT 1 FROM core.tb_entity_registry
        WHERE entity_name = 'TestUser'
    ) THEN
        RAISE EXCEPTION 'Entity TestUser was not registered';
    END IF;

    RAISE NOTICE 'Test 2: PASSED - Entity registration working';
END $$;

-- Test 3: Registry Listing
\echo 'Test 3: Registry Listing'
DO $$
DECLARE
    entity_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO entity_count FROM core.list_registered_entities();

    IF entity_count != 1 THEN
        RAISE EXCEPTION 'Expected 1 registered entity, got %', entity_count;
    END IF;

    RAISE NOTICE 'Test 3: PASSED - Registry listing working';
END $$;

-- Test 4: Data Source Optimization
\echo 'Test 4: Data Source Optimization'
DO $$
DECLARE
    data_source TEXT;
    source_type TEXT;
BEGIN
    SELECT ds.data_source, ds.source_type
    INTO data_source, source_type
    FROM core.get_optimal_data_source('TestUser', 'single') ds;

    IF data_source != 'v_test_user' THEN
        RAISE EXCEPTION 'Expected data_source "v_test_user", got "%"', data_source;
    END IF;

    IF source_type != 'view' THEN
        RAISE EXCEPTION 'Expected source_type "view", got "%"', source_type;
    END IF;

    RAISE NOTICE 'Test 4: PASSED - Data source optimization working';
END $$;

-- Test 5: Create Mock Views for Testing
\echo 'Test 5: Creating Mock Data for Node Resolution Tests'
DO $$
BEGIN
    -- Create mock table
    CREATE TABLE IF NOT EXISTS tb_test_user (
        id SERIAL PRIMARY KEY,
        pk_user UUID DEFAULT gen_random_uuid(),
        email TEXT NOT NULL,
        name TEXT NOT NULL,
        created_at TIMESTAMPTZ DEFAULT NOW(),
        updated_at TIMESTAMPTZ DEFAULT NOW(),
        deleted_at TIMESTAMPTZ
    );

    -- Create mock view
    CREATE OR REPLACE VIEW v_test_user AS
    SELECT
        pk_user as id,
        jsonb_build_object(
            'id', pk_user,
            'email', email,
            'name', name,
            'created_at', created_at,
            'updated_at', updated_at
        ) as data,
        created_at,
        updated_at
    FROM tb_test_user
    WHERE deleted_at IS NULL;

    -- Insert test data
    INSERT INTO tb_test_user (pk_user, email, name) VALUES
        ('11111111-1111-1111-1111-111111111111', 'test1@example.com', 'Test User 1'),
        ('22222222-2222-2222-2222-222222222222', 'test2@example.com', 'Test User 2'),
        ('33333333-3333-3333-3333-333333333333', 'test3@example.com', 'Test User 3');

    RAISE NOTICE 'Test 5: PASSED - Mock data created';
END $$;

-- Test 6: View Refresh
\echo 'Test 6: View Refresh'
DO $$
DECLARE
    nodes_count INTEGER;
BEGIN
    -- Refresh the v_nodes view
    PERFORM core.refresh_v_nodes_view();

    -- Check if nodes are now visible
    SELECT COUNT(*) INTO nodes_count FROM core.v_nodes;

    IF nodes_count != 3 THEN
        RAISE EXCEPTION 'Expected 3 nodes in v_nodes, got %', nodes_count;
    END IF;

    RAISE NOTICE 'Test 6: PASSED - View refresh working, % nodes found', nodes_count;
END $$;

-- Test 7: Node Resolution
\echo 'Test 7: Node Resolution'
DO $$
DECLARE
    result_typename TEXT;
    result_data JSONB;
    result_entity TEXT;
BEGIN
    -- Test resolving a known node
    SELECT __typename, data, entity_name
    INTO result_typename, result_data, result_entity
    FROM core.resolve_node('11111111-1111-1111-1111-111111111111'::uuid);

    IF result_typename != 'User' THEN
        RAISE EXCEPTION 'Expected typename "User", got "%"', result_typename;
    END IF;

    IF result_entity != 'TestUser' THEN
        RAISE EXCEPTION 'Expected entity_name "TestUser", got "%"', result_entity;
    END IF;

    IF (result_data->>'email') != 'test1@example.com' THEN
        RAISE EXCEPTION 'Expected email "test1@example.com", got "%"', result_data->>'email';
    END IF;

    RAISE NOTICE 'Test 7: PASSED - Node resolution working';
END $$;

-- Test 8: Smart Node Resolution
\echo 'Test 8: Smart Node Resolution'
DO $$
DECLARE
    result_typename TEXT;
    result_data JSONB;
    result_entity TEXT;
    source_used TEXT;
BEGIN
    SELECT __typename, data, entity_name, source_used
    INTO result_typename, result_data, result_entity, source_used
    FROM core.resolve_node_smart(
        '22222222-2222-2222-2222-222222222222'::uuid,
        '{"query_type": "single"}'::jsonb
    );

    IF result_typename != 'User' THEN
        RAISE EXCEPTION 'Expected typename "User", got "%"', result_typename;
    END IF;

    IF (result_data->>'name') != 'Test User 2' THEN
        RAISE EXCEPTION 'Expected name "Test User 2", got "%"', result_data->>'name';
    END IF;

    RAISE NOTICE 'Test 8: PASSED - Smart node resolution working (source: %)', source_used;
END $$;

-- Test 9: Global ID Encoding/Decoding
\echo 'Test 9: Global ID Encoding/Decoding'
DO $$
DECLARE
    encoded_id TEXT;
    decoded_result RECORD;
BEGIN
    -- Test encoding
    encoded_id := core.fraiseql_encode_global_id(
        'User',
        '11111111-1111-1111-1111-111111111111'::uuid
    );

    IF encoded_id IS NULL OR length(encoded_id) < 10 THEN
        RAISE EXCEPTION 'Global ID encoding failed, got "%"', encoded_id;
    END IF;

    -- Test decoding
    SELECT * INTO decoded_result FROM core.fraiseql_decode_global_id(encoded_id);

    IF decoded_result.typename != 'User' THEN
        RAISE EXCEPTION 'Decoded typename should be "User", got "%"', decoded_result.typename;
    END IF;

    IF decoded_result.local_id != '11111111-1111-1111-1111-111111111111'::uuid THEN
        RAISE EXCEPTION 'Decoded UUID mismatch';
    END IF;

    RAISE NOTICE 'Test 9: PASSED - Global ID encoding/decoding working';
END $$;

-- Test 10: Entity Unregistration
\echo 'Test 10: Entity Unregistration'
DO $$
DECLARE
    entity_count INTEGER;
BEGIN
    -- Register a temporary entity
    PERFORM core.register_entity(
        p_entity_name := 'TempEntity',
        p_graphql_type := 'Temp',
        p_pk_column := 'pk_temp',
        p_v_table := 'v_temp',
        p_source_table := 'tb_temp'
    );

    -- Verify it exists
    SELECT COUNT(*) INTO entity_count FROM core.tb_entity_registry;
    IF entity_count != 2 THEN
        RAISE EXCEPTION 'Expected 2 entities after registration, got %', entity_count;
    END IF;

    -- Unregister it
    PERFORM core.unregister_entity('TempEntity');

    -- Verify it's gone
    SELECT COUNT(*) INTO entity_count FROM core.tb_entity_registry;
    IF entity_count != 1 THEN
        RAISE EXCEPTION 'Expected 1 entity after unregistration, got %', entity_count;
    END IF;

    RAISE NOTICE 'Test 10: PASSED - Entity unregistration working';
END $$;

-- Test 11: Advanced Registration with Cache Layers
\echo 'Test 11: Advanced Registration with Cache Layers'
DO $$
DECLARE
    reg_result RECORD;
BEGIN
    -- Register entity with all cache layers
    PERFORM core.register_entity(
        p_entity_name := 'AdvancedUser',
        p_graphql_type := 'AdvancedUser',
        p_pk_column := 'pk_user',
        p_v_table := 'v_test_user',
        p_source_table := 'tb_test_user',
        p_tv_table := 'tv_test_user',
        p_mv_table := 'mv_test_user',
        p_turbo_function := 'turbo.fn_get_users',
        p_lazy_cache_key_pattern := 'user:{id}',
        p_identifier_column := 'email',
        p_tenant_scoped := true,
        p_default_cache_layer := 'turbo_function'
    );

    -- Verify advanced registration
    SELECT * INTO reg_result FROM core.tb_entity_registry WHERE entity_name = 'AdvancedUser';

    IF reg_result.turbo_function != 'turbo.fn_get_users' THEN
        RAISE EXCEPTION 'Turbo function not registered correctly';
    END IF;

    IF reg_result.lazy_cache_key_pattern != 'user:{id}' THEN
        RAISE EXCEPTION 'Lazy cache pattern not registered correctly';
    END IF;

    IF reg_result.default_cache_layer != 'turbo_function' THEN
        RAISE EXCEPTION 'Default cache layer not set correctly';
    END IF;

    RAISE NOTICE 'Test 11: PASSED - Advanced registration with cache layers working';
END $$;

-- Test 12: Health Check After Full Setup
\echo 'Test 12: Final Health Check'
DO $$
DECLARE
    health_result RECORD;
BEGIN
    SELECT * INTO health_result FROM core.fraiseql_relay_health();

    IF health_result.status != 'healthy' THEN
        RAISE EXCEPTION 'Expected final status "healthy", got "%"', health_result.status;
    END IF;

    IF health_result.entities_registered < 2 THEN
        RAISE EXCEPTION 'Expected at least 2 entities registered';
    END IF;

    RAISE NOTICE 'Test 12: PASSED - Final health check: % entities registered',
                 health_result.entities_registered;
END $$;

-- Summary
\echo ''
\echo '=========================================='
\echo 'FraiseQL Relay Extension Basic Tests'
\echo 'All tests PASSED! ✓'
\echo ''
\echo 'Extension is ready for use.'
\echo 'Run the performance tests next:'
\echo 'psql -d your_db -f tests/sql/test_performance.sql'
\echo '=========================================='
