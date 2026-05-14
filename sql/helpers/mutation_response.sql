-- ============================================================================
-- FraiseQL Mutation Response Builders
-- ============================================================================
-- Helper functions for constructing typed, 13-column mutation_response rows
-- in v2.2.0+. Installs into the `fraiseql` schema owned by FraiseQL.
--
-- Usage:
--   fraiseql setup --database postgres://localhost/db
--
-- Then in mutation functions:
--   RETURN QUERY SELECT * FROM fraiseql.mutation_ok(v_entity, v_id, 'User', v_changed, ARRAY['bio']);
--   RETURN QUERY SELECT * FROM fraiseql.mutation_err('not_found', 'User not found');
--
-- See: docs/architecture/mutation-response.md
-- ============================================================================

-- Create the fraiseql schema if it doesn't exist
CREATE SCHEMA IF NOT EXISTS fraiseql;

-- Comment on schema
COMMENT ON SCHEMA fraiseql IS
'FraiseQL-provided helpers and infrastructure. Owned by FraiseQL''s database role.';

-- ============================================================================
-- Version identifier for schema compatibility checking
-- ============================================================================
-- Returns the version of the FraiseQL mutation response protocol that these
-- helpers implement. The server can call this to detect version mismatches.
-- ============================================================================

CREATE OR REPLACE FUNCTION fraiseql.library_version()
RETURNS TEXT AS $$
BEGIN
    RETURN '2.2.0';
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.library_version() IS
'Returns the FraiseQL mutation response protocol version that these helpers implement.
Used by fraiseql setup to detect version mismatches between CLI and installed helpers.';

-- ============================================================================
-- fraiseql.mutation_ok() - Build success mutation responses
-- ============================================================================
-- Constructs a well-formed success row for the 13-column mutation_response
-- composite type. All 13 columns are populated; error columns are NULL.
--
-- Arguments:
--   entity           JSONB              - The full entity payload (required)
--   entity_id        UUID               - PK/UUID of affected entity (optional)
--   entity_type      TEXT               - GraphQL type name for cache invalidation (optional)
--   state_changed    BOOLEAN            - Did the database actually change? (default TRUE)
--   updated_fields   TEXT[]             - Field names that changed (optional, default NULL)
--   cascade          JSONB              - Cascade operations (graphql-cascade spec, optional)
--   metadata         JSONB              - Observability only (optional, default NULL)
--
-- Returns:
--   All 13 columns of mutation_response: succeeded, state_changed, error_class,
--   status_detail, http_status, message, entity_id, entity_type, entity,
--   updated_fields, cascade, error_detail, metadata
--
-- Semantics:
--   - succeeded is always TRUE
--   - state_changed is passed through (caller controls noop semantics)
--   - error columns (error_class, status_detail, http_status, message, error_detail)
--     are all NULL
--   - entity is always populated
-- ============================================================================

CREATE OR REPLACE FUNCTION fraiseql.mutation_ok(
    p_entity JSONB,
    p_entity_id UUID DEFAULT NULL,
    p_entity_type TEXT DEFAULT NULL,
    p_state_changed BOOLEAN DEFAULT TRUE,
    p_updated_fields TEXT[] DEFAULT NULL,
    p_cascade JSONB DEFAULT NULL,
    p_metadata JSONB DEFAULT NULL
)
RETURNS TABLE(
    succeeded BOOLEAN,
    state_changed BOOLEAN,
    error_class TEXT,
    status_detail TEXT,
    http_status SMALLINT,
    message TEXT,
    entity_id UUID,
    entity_type TEXT,
    entity JSONB,
    updated_fields TEXT[],
    cascade JSONB,
    error_detail JSONB,
    metadata JSONB
) AS $$
BEGIN
    RETURN QUERY SELECT
        TRUE::BOOLEAN,                -- succeeded
        p_state_changed::BOOLEAN,     -- state_changed
        NULL::TEXT,                   -- error_class
        NULL::TEXT,                   -- status_detail
        NULL::SMALLINT,               -- http_status
        NULL::TEXT,                   -- message
        p_entity_id::UUID,            -- entity_id
        p_entity_type::TEXT,          -- entity_type
        p_entity::JSONB,              -- entity
        p_updated_fields::TEXT[],     -- updated_fields
        p_cascade::JSONB,             -- cascade
        NULL::JSONB,                  -- error_detail
        p_metadata::JSONB;            -- metadata
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.mutation_ok(JSONB, UUID, TEXT, BOOLEAN, TEXT[], JSONB, JSONB) IS
'Build a success (succeeded=TRUE) mutation response with entity data.
Handles noop semantics via state_changed. See fraiseql.mutation_ok documentation.';

-- ============================================================================
-- fraiseql.mutation_err() - Build error mutation responses
-- ============================================================================
-- Constructs a well-formed error row for the 13-column mutation_response
-- composite type. succeeded=FALSE, state_changed=FALSE, entity is NULL.
--
-- Arguments:
--   error_class      TEXT               - Typed error classification (required)
--                                        Examples: 'not_found', 'validation', 'conflict',
--                                        'unauthorized', 'rate_limited', 'internal_error'
--   message          TEXT               - Human-readable error summary (optional, default '')
--   error_detail     JSONB              - Structured error metadata (optional)
--                                        Example: {"field": "email", "reason": "duplicate"}
--   http_status      SMALLINT           - HTTP status code (optional, auto-mapped from
--                                        error_class if omitted)
--
-- Returns:
--   All 13 columns of mutation_response: succeeded, state_changed, error_class,
--   status_detail, http_status, message, entity_id, entity_type, entity,
--   updated_fields, cascade, error_detail, metadata
--
-- Semantics:
--   - succeeded is always FALSE
--   - state_changed is always FALSE (mutation failed before any DB change)
--   - error_class is set to p_error_class (required)
--   - message is set to p_message if provided, else empty string
--   - All success columns (entity_id, entity_type, entity, updated_fields, cascade,
--     metadata) are NULL
--   - error_detail carries structured error data (e.g., field name, constraint)
-- ============================================================================

CREATE OR REPLACE FUNCTION fraiseql.mutation_err(
    p_error_class TEXT,
    p_message TEXT DEFAULT '',
    p_error_detail JSONB DEFAULT NULL,
    p_http_status SMALLINT DEFAULT NULL
)
RETURNS TABLE(
    succeeded BOOLEAN,
    state_changed BOOLEAN,
    error_class TEXT,
    status_detail TEXT,
    http_status SMALLINT,
    message TEXT,
    entity_id UUID,
    entity_type TEXT,
    entity JSONB,
    updated_fields TEXT[],
    cascade JSONB,
    error_detail JSONB,
    metadata JSONB
) AS $$
BEGIN
    RETURN QUERY SELECT
        FALSE::BOOLEAN,                -- succeeded
        FALSE::BOOLEAN,                -- state_changed (always false on error)
        p_error_class::TEXT,           -- error_class
        NULL::TEXT,                    -- status_detail
        p_http_status::SMALLINT,       -- http_status (caller can omit)
        COALESCE(p_message, '')::TEXT, -- message (default to empty string)
        NULL::UUID,                    -- entity_id
        NULL::TEXT,                    -- entity_type
        NULL::JSONB,                   -- entity (no entity on error)
        NULL::TEXT[],                  -- updated_fields
        NULL::JSONB,                   -- cascade
        p_error_detail::JSONB,         -- error_detail
        NULL::JSONB;                   -- metadata
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.mutation_err(TEXT, TEXT, JSONB, SMALLINT) IS
'Build an error (succeeded=FALSE) mutation response with optional structured metadata.
error_class is required; message, error_detail, and http_status are optional.
See fraiseql.mutation_err documentation.';

-- ============================================================================
-- Permissions
-- ============================================================================
-- Grant EXECUTE on all functions to PUBLIC so application roles can call them
-- without explicit grants. The fraiseql schema itself is owned by the
-- FraiseQL database role and not writable by application code.

GRANT USAGE ON SCHEMA fraiseql TO PUBLIC;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA fraiseql TO PUBLIC;

-- ============================================================================
-- Tests (run as: \i sql/helpers/mutation_response.sql)
-- ============================================================================

DO $$
BEGIN
    -- Test library_version
    ASSERT (SELECT fraiseql.library_version()) = '2.2.0',
        'library_version should return 2.2.0';

    -- Test mutation_ok with all parameters
    DECLARE
        v_row RECORD;
    BEGIN
        SELECT * INTO v_row FROM fraiseql.mutation_ok(
            '{"id": "abc"}'::JSONB,
            'f47ac10b-58cc-4372-a567-0e02b2c3d479'::UUID,
            'User',
            TRUE,
            ARRAY['bio'],
            '{"action": "cascade_delete"}'::JSONB,
            '{"trace_id": "xyz"}'::JSONB
        );

        ASSERT v_row.succeeded = TRUE, 'mutation_ok should return succeeded=TRUE';
        ASSERT v_row.state_changed = TRUE, 'mutation_ok should return state_changed=TRUE';
        ASSERT v_row.error_class IS NULL, 'mutation_ok should have error_class=NULL';
        ASSERT v_row.entity_type = 'User', 'mutation_ok should preserve entity_type';
        ASSERT v_row.entity ->> 'id' = 'abc', 'mutation_ok should preserve entity';
        ASSERT v_row.updated_fields[1] = 'bio', 'mutation_ok should preserve updated_fields';
    END;

    -- Test mutation_ok with minimal parameters (noop)
    DECLARE
        v_row RECORD;
    BEGIN
        SELECT * INTO v_row FROM fraiseql.mutation_ok('{"id": "abc"}'::JSONB);

        ASSERT v_row.succeeded = TRUE, 'mutation_ok minimal should return succeeded=TRUE';
        ASSERT v_row.state_changed = TRUE, 'mutation_ok default should have state_changed=TRUE';
        ASSERT v_row.entity_id IS NULL, 'mutation_ok should allow NULL entity_id';
        ASSERT v_row.entity_type IS NULL, 'mutation_ok should allow NULL entity_type';
    END;

    -- Test mutation_ok with noop semantics (state_changed=FALSE)
    DECLARE
        v_row RECORD;
    BEGIN
        SELECT * INTO v_row FROM fraiseql.mutation_ok(
            '{"id": "abc"}'::JSONB,
            NULL::UUID,
            'User',
            FALSE,  -- No state change
            ARRAY[]::TEXT[]  -- Empty updated_fields
        );

        ASSERT v_row.state_changed = FALSE, 'mutation_ok should support noop (state_changed=FALSE)';
        ASSERT array_length(v_row.updated_fields, 1) IS NULL,
            'mutation_ok should accept empty updated_fields array';
    END;

    -- Test mutation_err with all parameters
    DECLARE
        v_row RECORD;
    BEGIN
        SELECT * INTO v_row FROM fraiseql.mutation_err(
            'validation',
            'Email is invalid',
            '{"field": "email"}'::JSONB,
            422::SMALLINT
        );

        ASSERT v_row.succeeded = FALSE, 'mutation_err should return succeeded=FALSE';
        ASSERT v_row.state_changed = FALSE, 'mutation_err should return state_changed=FALSE';
        ASSERT v_row.error_class = 'validation', 'mutation_err should preserve error_class';
        ASSERT v_row.message = 'Email is invalid', 'mutation_err should preserve message';
        ASSERT v_row.http_status = 422, 'mutation_err should preserve http_status';
        ASSERT v_row.entity IS NULL, 'mutation_err should have entity=NULL';
    END;

    -- Test mutation_err with minimal parameters
    DECLARE
        v_row RECORD;
    BEGIN
        SELECT * INTO v_row FROM fraiseql.mutation_err('not_found');

        ASSERT v_row.succeeded = FALSE, 'mutation_err minimal should return succeeded=FALSE';
        ASSERT v_row.error_class = 'not_found', 'mutation_err should accept error_class only';
        ASSERT v_row.message = '', 'mutation_err should default message to empty string';
        ASSERT v_row.http_status IS NULL, 'mutation_err should allow NULL http_status';
    END;

    RAISE NOTICE 'All mutation response tests passed!';
END;
$$;

-- ============================================================================
-- Finalization
-- ============================================================================

COMMENT ON SCHEMA fraiseql IS
'FraiseQL mutation response helpers and infrastructure. Installed by ''fraiseql setup''.
See: https://github.com/fraiseql/fraiseql/issues/230';
