-- FraiseQL Relay Extension v1.0
-- PostgreSQL-native GraphQL Relay specification compliance

-- Create core schema if it doesn't exist
CREATE SCHEMA IF NOT EXISTS core;

-- Entity Registry: Central metadata for all entities
CREATE TABLE core.tb_entity_registry (
    entity_name TEXT PRIMARY KEY,                    -- 'User', 'Contract', 'Post'
    graphql_type TEXT NOT NULL,                     -- GraphQL __typename

    -- Primary key mapping
    pk_column TEXT NOT NULL,                        -- 'pk_user', 'pk_contract'
    identifier_column TEXT,                         -- 'email', 'identifier' (optional)

    -- Data source hierarchy (performance optimization)
    turbo_function TEXT,                            -- 'turbo.fn_get_users' (fastest)
    lazy_cache_key_pattern TEXT,                    -- 'user:{id}', 'contract_items:{contract_id}'
    tv_table TEXT,                                  -- 'tv_user' (materialized table)
    mv_table TEXT,                                  -- 'mv_user_analytics' (materialized view)
    v_table TEXT NOT NULL,                          -- 'v_user' (real-time view, always available)
    source_table TEXT NOT NULL,                     -- 'tb_user' (command side)

    -- Entity metadata
    tenant_scoped BOOLEAN DEFAULT true,             -- Multi-tenant support
    soft_delete_column TEXT DEFAULT 'deleted_at',   -- Soft delete column name
    supports_pagination BOOLEAN DEFAULT true,       -- Supports cursor pagination
    supports_filtering BOOLEAN DEFAULT true,        -- Supports WHERE clauses

    -- Performance hints
    default_cache_layer TEXT DEFAULT 'v_table',     -- Default data source preference
    estimated_row_count INTEGER DEFAULT 1000,       -- For query optimization

    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    CONSTRAINT uq_graphql_type UNIQUE (graphql_type),
    CONSTRAINT valid_default_cache_layer CHECK (
        default_cache_layer IN ('turbo_function', 'lazy_cache', 'tv_table', 'mv_table', 'v_table')
    )
);

-- Index for fast entity lookups
CREATE INDEX idx_entity_registry_graphql_type ON core.tb_entity_registry(graphql_type);
CREATE INDEX idx_entity_registry_cache_layer ON core.tb_entity_registry(default_cache_layer);

-- Update timestamp trigger
CREATE OR REPLACE FUNCTION core.update_registry_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_registry_updated_at
    BEFORE UPDATE ON core.tb_entity_registry
    FOR EACH ROW
    EXECUTE FUNCTION core.update_registry_timestamp();

-- Global nodes view (dynamically generated)
-- This will be populated by refresh_v_nodes_view() function
CREATE VIEW core.v_nodes AS
SELECT
    NULL::UUID as id,
    NULL::TEXT as __typename,
    NULL::TEXT as entity_name,
    NULL::TEXT as source_table,
    NULL::JSONB as data,
    NULL::TIMESTAMPTZ as created_at,
    NULL::TIMESTAMPTZ as updated_at
WHERE FALSE; -- Initially empty, populated by refresh function

-- Function to register a new entity
CREATE OR REPLACE FUNCTION core.register_entity(
    p_entity_name TEXT,
    p_graphql_type TEXT,
    p_pk_column TEXT,
    p_v_table TEXT,
    p_source_table TEXT,
    p_tv_table TEXT DEFAULT NULL,
    p_mv_table TEXT DEFAULT NULL,
    p_turbo_function TEXT DEFAULT NULL,
    p_lazy_cache_key_pattern TEXT DEFAULT NULL,
    p_identifier_column TEXT DEFAULT NULL,
    p_tenant_scoped BOOLEAN DEFAULT true,
    p_soft_delete_column TEXT DEFAULT 'deleted_at'
) RETURNS VOID AS $$
BEGIN
    -- Insert or update entity registration
    INSERT INTO core.tb_entity_registry (
        entity_name, graphql_type, pk_column, v_table, source_table,
        tv_table, mv_table, turbo_function, lazy_cache_key_pattern,
        identifier_column, tenant_scoped, soft_delete_column
    ) VALUES (
        p_entity_name, p_graphql_type, p_pk_column, p_v_table, p_source_table,
        p_tv_table, p_mv_table, p_turbo_function, p_lazy_cache_key_pattern,
        p_identifier_column, p_tenant_scoped, p_soft_delete_column
    )
    ON CONFLICT (entity_name) DO UPDATE SET
        graphql_type = EXCLUDED.graphql_type,
        pk_column = EXCLUDED.pk_column,
        v_table = EXCLUDED.v_table,
        source_table = EXCLUDED.source_table,
        tv_table = EXCLUDED.tv_table,
        mv_table = EXCLUDED.mv_table,
        turbo_function = EXCLUDED.turbo_function,
        lazy_cache_key_pattern = EXCLUDED.lazy_cache_key_pattern,
        identifier_column = EXCLUDED.identifier_column,
        tenant_scoped = EXCLUDED.tenant_scoped,
        soft_delete_column = EXCLUDED.soft_delete_column,
        updated_at = NOW();

    -- Automatically refresh the v_nodes view
    PERFORM core.refresh_v_nodes_view();

    RAISE NOTICE 'Entity % registered successfully', p_entity_name;
END;
$$ LANGUAGE plpgsql;

-- Function to dynamically generate v_nodes view
CREATE OR REPLACE FUNCTION core.refresh_v_nodes_view()
RETURNS VOID AS $$
DECLARE
    v_sql TEXT;
    v_union_parts TEXT[];
    r RECORD;
    v_count INTEGER;
BEGIN
    -- Initialize array for UNION parts
    v_union_parts := ARRAY[]::TEXT[];

    -- Build UNION parts for each registered entity
    FOR r IN
        SELECT * FROM core.tb_entity_registry
        WHERE v_table IS NOT NULL
        ORDER BY entity_name
    LOOP
        -- Choose optimal data source (prefer tv_ over v_)
        v_union_parts := array_append(v_union_parts, format($template$
        SELECT
            %I as id,                    -- pk_entity column (UUID)
            %L as __typename,            -- GraphQL type name
            %L as entity_name,           -- Internal entity name for routing
            %L as source_table,          -- Command side table name
            data,                        -- JSONB data from view (assumes views have 'data' column)
            created_at,
            updated_at
        FROM %I                          -- Use tv_table if available, otherwise v_table
        WHERE %I IS NULL                 -- Soft delete check
        $template$,
            r.pk_column,                               -- id field
            r.graphql_type,                           -- __typename
            r.entity_name,                            -- entity_name
            r.source_table,                           -- source_table
            COALESCE(r.tv_table, r.v_table),         -- Prefer tv_ over v_
            COALESCE(r.soft_delete_column, 'deleted_at') -- soft delete column
        ));
    END LOOP;

    -- Get count of registered entities
    SELECT COUNT(*) INTO v_count FROM core.tb_entity_registry WHERE v_table IS NOT NULL;

    IF v_count = 0 THEN
        -- No entities registered, create empty view
        v_sql := $view$
        CREATE OR REPLACE VIEW core.v_nodes AS
        SELECT
            NULL::UUID as id,
            NULL::TEXT as __typename,
            NULL::TEXT as entity_name,
            NULL::TEXT as source_table,
            NULL::JSONB as data,
            NULL::TIMESTAMPTZ as created_at,
            NULL::TIMESTAMPTZ as updated_at
        WHERE FALSE
        $view$;
    ELSE
        -- Build complete view with UNION ALL
        v_sql := 'CREATE OR REPLACE VIEW core.v_nodes AS ' ||
                 array_to_string(v_union_parts, ' UNION ALL ');
    END IF;

    -- Execute the dynamic SQL
    EXECUTE v_sql;

    -- Recreate indexes on the view
    BEGIN
        DROP INDEX IF EXISTS core.idx_v_nodes_id;
        DROP INDEX IF EXISTS core.idx_v_nodes_typename;

        -- Only create indexes if we have data
        IF v_count > 0 THEN
            CREATE INDEX idx_v_nodes_id ON core.v_nodes(id);
            CREATE INDEX idx_v_nodes_typename ON core.v_nodes(__typename);
            CREATE INDEX idx_v_nodes_entity_name ON core.v_nodes(entity_name);
        END IF;
    EXCEPTION WHEN OTHERS THEN
        -- Ignore index creation errors (views can't always be indexed)
        RAISE NOTICE 'Could not create indexes on v_nodes view: %', SQLERRM;
    END;

    RAISE NOTICE 'v_nodes view refreshed with % entities', v_count;
END;
$$ LANGUAGE plpgsql;

-- Function to get optimal data source for an entity
CREATE OR REPLACE FUNCTION core.get_optimal_data_source(
    p_entity_name TEXT,
    p_query_type TEXT DEFAULT 'single',  -- 'single', 'list', 'count', 'analytics'
    p_context JSONB DEFAULT '{}'::jsonb
) RETURNS TABLE(
    data_source TEXT,
    source_type TEXT,  -- 'turbo', 'lazy_cache', 'tv', 'mv', 'view'
    cache_key TEXT
) AS $$
DECLARE
    r core.tb_entity_registry%ROWTYPE;
BEGIN
    -- Get entity registration
    SELECT * INTO r FROM core.tb_entity_registry WHERE entity_name = p_entity_name;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Entity % not registered in core.tb_entity_registry', p_entity_name;
    END IF;

    -- Priority resolution based on query type and available sources
    CASE p_query_type
        WHEN 'single' THEN
            -- Single entity: prefer lazy cache > tv > v
            IF r.lazy_cache_key_pattern IS NOT NULL THEN
                RETURN QUERY SELECT
                    r.lazy_cache_key_pattern,
                    'lazy_cache'::TEXT,
                    replace(r.lazy_cache_key_pattern, '{id}', COALESCE(p_context->>'id', 'unknown'));
            ELSIF r.tv_table IS NOT NULL THEN
                RETURN QUERY SELECT r.tv_table, 'tv'::TEXT, NULL::TEXT;
            ELSE
                RETURN QUERY SELECT r.v_table, 'view'::TEXT, NULL::TEXT;
            END IF;

        WHEN 'list' THEN
            -- List queries: prefer turbo > tv > v
            IF r.turbo_function IS NOT NULL THEN
                RETURN QUERY SELECT r.turbo_function, 'turbo'::TEXT, NULL::TEXT;
            ELSIF r.tv_table IS NOT NULL THEN
                RETURN QUERY SELECT r.tv_table, 'tv'::TEXT, NULL::TEXT;
            ELSE
                RETURN QUERY SELECT r.v_table, 'view'::TEXT, NULL::TEXT;
            END IF;

        WHEN 'analytics', 'count' THEN
            -- Analytics: prefer mv > tv > v
            IF r.mv_table IS NOT NULL THEN
                RETURN QUERY SELECT r.mv_table, 'mv'::TEXT, NULL::TEXT;
            ELSIF r.tv_table IS NOT NULL THEN
                RETURN QUERY SELECT r.tv_table, 'tv'::TEXT, NULL::TEXT;
            ELSE
                RETURN QUERY SELECT r.v_table, 'view'::TEXT, NULL::TEXT;
            END IF;

        ELSE
            -- Default: use registered preference or fallback to v_table
            CASE r.default_cache_layer
                WHEN 'turbo_function' THEN
                    RETURN QUERY SELECT COALESCE(r.turbo_function, r.v_table), 'turbo'::TEXT, NULL::TEXT;
                WHEN 'tv_table' THEN
                    RETURN QUERY SELECT COALESCE(r.tv_table, r.v_table), 'tv'::TEXT, NULL::TEXT;
                WHEN 'mv_table' THEN
                    RETURN QUERY SELECT COALESCE(r.mv_table, r.v_table), 'mv'::TEXT, NULL::TEXT;
                ELSE
                    RETURN QUERY SELECT r.v_table, 'view'::TEXT, NULL::TEXT;
            END CASE;
    END CASE;
END;
$$ LANGUAGE plpgsql STABLE;

-- Basic node resolver (will be replaced by C implementation for performance)
CREATE OR REPLACE FUNCTION core.resolve_node(node_id UUID)
RETURNS TABLE(__typename TEXT, data JSONB, entity_name TEXT) AS $$
BEGIN
    RETURN QUERY
    SELECT v.__typename, v.data, v.entity_name
    FROM core.v_nodes v
    WHERE v.id = node_id
    LIMIT 1;
END;
$$ LANGUAGE plpgsql STABLE;

-- Smart node resolver with cache layer optimization
CREATE OR REPLACE FUNCTION core.resolve_node_smart(
    node_id UUID,
    p_context JSONB DEFAULT '{}'::jsonb
) RETURNS TABLE(__typename TEXT, data JSONB, entity_name TEXT, source_used TEXT) AS $$
DECLARE
    v_entity_name TEXT;
    v_data_source TEXT;
    v_source_type TEXT;
    v_cache_key TEXT;
    v_result RECORD;
BEGIN
    -- First, find the entity type from v_nodes (fast lookup)
    SELECT v.entity_name INTO v_entity_name
    FROM core.v_nodes v
    WHERE v.id = node_id
    LIMIT 1;

    IF v_entity_name IS NULL THEN
        -- Entity not found
        RETURN;
    END IF;

    -- Get optimal data source for this entity
    SELECT ds.data_source, ds.source_type, ds.cache_key
    INTO v_data_source, v_source_type, v_cache_key
    FROM core.get_optimal_data_source(
        v_entity_name,
        'single',
        jsonb_build_object('id', node_id::text)
    ) ds;

    -- For now, always use v_nodes (C implementation will optimize this)
    RETURN QUERY
    SELECT v.__typename, v.data, v.entity_name, v_source_type as source_used
    FROM core.v_nodes v
    WHERE v.id = node_id
    LIMIT 1;
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to list all registered entities (for debugging/admin)
CREATE OR REPLACE FUNCTION core.list_registered_entities()
RETURNS TABLE(
    entity_name TEXT,
    graphql_type TEXT,
    has_turbo BOOLEAN,
    has_lazy_cache BOOLEAN,
    has_tv_table BOOLEAN,
    has_mv_table BOOLEAN,
    default_cache_layer TEXT,
    created_at TIMESTAMPTZ
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        r.entity_name,
        r.graphql_type,
        (r.turbo_function IS NOT NULL) as has_turbo,
        (r.lazy_cache_key_pattern IS NOT NULL) as has_lazy_cache,
        (r.tv_table IS NOT NULL) as has_tv_table,
        (r.mv_table IS NOT NULL) as has_mv_table,
        r.default_cache_layer,
        r.created_at
    FROM core.tb_entity_registry r
    ORDER BY r.entity_name;
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to unregister an entity
CREATE OR REPLACE FUNCTION core.unregister_entity(p_entity_name TEXT)
RETURNS VOID AS $$
BEGIN
    DELETE FROM core.tb_entity_registry WHERE entity_name = p_entity_name;

    -- Refresh the view
    PERFORM core.refresh_v_nodes_view();

    RAISE NOTICE 'Entity % unregistered', p_entity_name;
END;
$$ LANGUAGE plpgsql;

-- Function to check extension health
CREATE OR REPLACE FUNCTION core.fraiseql_relay_health()
RETURNS TABLE(
    status TEXT,
    entities_registered INTEGER,
    v_nodes_exists BOOLEAN,
    last_refresh TIMESTAMPTZ
) AS $$
DECLARE
    v_count INTEGER;
    v_view_exists BOOLEAN;
BEGIN
    -- Count registered entities
    SELECT COUNT(*) INTO v_count FROM core.tb_entity_registry;

    -- Check if v_nodes view exists and is queryable
    BEGIN
        PERFORM COUNT(*) FROM core.v_nodes LIMIT 1;
        v_view_exists := TRUE;
    EXCEPTION WHEN OTHERS THEN
        v_view_exists := FALSE;
    END;

    RETURN QUERY
    SELECT
        CASE
            WHEN v_count > 0 AND v_view_exists THEN 'healthy'
            WHEN v_count > 0 AND NOT v_view_exists THEN 'degraded'
            ELSE 'no_entities'
        END as status,
        v_count as entities_registered,
        v_view_exists as v_nodes_exists,
        (SELECT MAX(updated_at) FROM core.tb_entity_registry) as last_refresh;
END;
$$ LANGUAGE plpgsql;

-- Grant permissions (adjust as needed for your security model)
-- These are basic permissions - adjust for production use
GRANT USAGE ON SCHEMA core TO PUBLIC;
GRANT SELECT ON core.tb_entity_registry TO PUBLIC;
GRANT SELECT ON core.v_nodes TO PUBLIC;
GRANT EXECUTE ON FUNCTION core.resolve_node(UUID) TO PUBLIC;
GRANT EXECUTE ON FUNCTION core.resolve_node_smart(UUID, JSONB) TO PUBLIC;
GRANT EXECUTE ON FUNCTION core.get_optimal_data_source(TEXT, TEXT, JSONB) TO PUBLIC;
GRANT EXECUTE ON FUNCTION core.list_registered_entities() TO PUBLIC;
GRANT EXECUTE ON FUNCTION core.fraiseql_relay_health() TO PUBLIC;

-- For entity registration (typically admin/application role only)
-- GRANT INSERT, UPDATE, DELETE ON core.tb_entity_registry TO fraiseql_admin;
-- GRANT EXECUTE ON FUNCTION core.register_entity(...) TO fraiseql_admin;
-- GRANT EXECUTE ON FUNCTION core.unregister_entity(TEXT) TO fraiseql_admin;
-- GRANT EXECUTE ON FUNCTION core.refresh_v_nodes_view() TO fraiseql_admin;

-- Initial setup complete
DO $$
BEGIN
    RAISE NOTICE 'FraiseQL Relay Extension v1.0 installed successfully';
    RAISE NOTICE 'Use core.register_entity() to register your entities';
    RAISE NOTICE 'Check core.fraiseql_relay_health() for status';
END $$;
