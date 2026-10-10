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
--   RETURN QUERY SELECT * FROM fraiseql.mutation_err('conflict', 'Email taken',
--                                                    p_entity_type => 'DuplicateEmailError');
--   RETURN QUERY SELECT * FROM fraiseql.mutation_err_entries('validation', 'Invalid order',
--       fraiseql.error_entry(422::smallint, format('%s_not_found', 'Order line'), 'No line'));
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
    RETURN '2.4.0';
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
--   entity_type      TEXT               - The declared error type this failure is (optional).
--                                        Required when the mutation's result union has two or
--                                        more error types: the runtime refuses to guess. Must
--                                        be one of the error types the mutation can return.
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
--   - entity_type is p_entity_type (NULL when omitted)
--   - All success columns (entity_id, entity, updated_fields, cascade, metadata) are NULL
--   - error_detail carries structured error data (e.g., field name, constraint)
-- ============================================================================

-- The stamp is the LAST parameter so every positional call binds unchanged. The 2.2.0
-- four-argument signature is dropped first: CREATE OR REPLACE cannot change an argument
-- list, and two overloads that both accept mutation_err('x', 'y') make that call ambiguous.
-- PL/pgSQL callers bind at call time, so the drop breaks none of them.
DROP FUNCTION IF EXISTS fraiseql.mutation_err(TEXT, TEXT, JSONB, SMALLINT);

CREATE OR REPLACE FUNCTION fraiseql.mutation_err(
    p_error_class TEXT,
    p_message TEXT DEFAULT '',
    p_error_detail JSONB DEFAULT NULL,
    p_http_status SMALLINT DEFAULT NULL,
    p_entity_type TEXT DEFAULT NULL
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
        p_entity_type::TEXT,           -- entity_type (the declared error type, if stamped)
        NULL::JSONB,                   -- entity (no entity on error)
        NULL::TEXT[],                  -- updated_fields
        NULL::JSONB,                   -- cascade
        p_error_detail::JSONB,         -- error_detail
        NULL::JSONB;                   -- metadata
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.mutation_err(TEXT, TEXT, JSONB, SMALLINT, TEXT) IS
'Build an error (succeeded=FALSE) mutation response with optional structured metadata.
error_class is required; message, error_detail, http_status and entity_type (the declared
error type this failure is) are optional.
See fraiseql.mutation_err documentation.';

-- ============================================================================
-- fraiseql.error_identifier() / error_entry() / mutation_err_entries()
-- ============================================================================
-- A failure's `error_detail` carries `{"errors": [entry, ...]}`. Each entry is
--
--   {"code": SMALLINT, "identifier": TEXT, "message": TEXT, "details": JSONB}
--
-- `details` is present only when given. Clients translate `identifier`
-- (`t('errors.' + identifier)`), so it must be a stable key: error_entry() builds it
-- from whatever the function has at hand (a human label, a type name) by
--
--   1. spelling ligatures out (ß -> ss, æ -> ae, ...) and removing accents
--      (Unicode NFD, then combining marks dropped; no extension needed, a UTF8
--      database is);
--   2. splitting camelCase (PaymentTerm -> Payment_Term, HTTPServer -> HTTP_Server);
--   3. lower-casing, and turning every run of other characters into one `_`,
--      trimmed at both ends.
--
-- The result matches ^[a-z][a-z0-9_]*$; an identifier that normalises to nothing
-- of that shape (empty, punctuation only, starting with a digit) raises 22023.
-- ============================================================================

CREATE OR REPLACE FUNCTION fraiseql.error_identifier(p_identifier TEXT)
RETURNS TEXT AS $$
DECLARE
    v_key TEXT := COALESCE(p_identifier, '');
BEGIN
    v_key := replace(replace(replace(replace(replace(replace(replace(replace(replace(
             replace(replace(replace(v_key,
        'ß', 'ss'), 'Æ', 'AE'), 'æ', 'ae'), 'Œ', 'OE'), 'œ', 'oe'), 'Ø', 'O'),
        'ø', 'o'), 'Đ', 'D'), 'đ', 'd'), 'Ł', 'L'), 'ł', 'l'), 'Þ', 'TH');
    v_key := regexp_replace(normalize(v_key, NFD), '[\u0300-\u036f]', '', 'g');
    v_key := regexp_replace(v_key, '([A-Z]+)([A-Z][a-z])', '\1_\2', 'g');
    v_key := regexp_replace(v_key, '([a-z0-9])([A-Z])', '\1_\2', 'g');
    v_key := btrim(regexp_replace(lower(v_key), '[^a-z0-9]+', '_', 'g'), '_');
    IF v_key !~ '^[a-z][a-z0-9_]*$' THEN
        RAISE EXCEPTION USING
            ERRCODE = '22023',
            MESSAGE = format('error identifier %L normalises to no translation key', p_identifier),
            HINT = 'An identifier needs a letter before any digit, e.g. ''order_not_found''.';
    END IF;
    RETURN v_key;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.error_identifier(TEXT) IS
'Normalise an error identifier into a translation key matching ^[a-z][a-z0-9_]*$:
accents removed, camelCase split, other characters collapsed to _. Raises 22023 when
nothing of that shape remains.';

CREATE OR REPLACE FUNCTION fraiseql.error_entry(
    p_code SMALLINT,
    p_identifier TEXT,
    p_message TEXT,
    p_details JSONB DEFAULT NULL
)
RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        'code', p_code,
        'identifier', fraiseql.error_identifier(p_identifier),
        'message', COALESCE(p_message, '')
    ) || CASE WHEN p_details IS NULL THEN '{}'::JSONB
              ELSE jsonb_build_object('details', p_details) END;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.error_entry(SMALLINT, TEXT, TEXT, JSONB) IS
'Build one errors[] entry {code, identifier, message, details?}, the identifier
normalised by fraiseql.error_identifier().';

CREATE OR REPLACE FUNCTION fraiseql.mutation_err_entries(
    p_error_class TEXT,
    p_message TEXT,
    VARIADIC p_entries JSONB[]
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
    RETURN QUERY SELECT * FROM fraiseql.mutation_err(
        p_error_class,
        p_message,
        jsonb_build_object('errors', to_jsonb(p_entries))
    );
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.mutation_err_entries(TEXT, TEXT, JSONB[]) IS
'fraiseql.mutation_err() whose error_detail is {"errors": [entries]}; build each entry
with fraiseql.error_entry().';

-- ============================================================================
-- The 14-column forms: mutation_ok_result() / mutation_err_result() /
-- mutation_err_entries_result()
-- ============================================================================
-- A mutation that declares success fields (#1397) returns them in a `result jsonb`
-- column after the 13 above. Only such a mutation's function declares that column
-- (its own RETURNS TABLE or composite type); every other function keeps the 13-column
-- row and the builders above. Do not add `result` to a shared `mutation_response`
-- type that functions fill with the 13-column builders: they would stop matching it.
--
-- Each form delegates to its 13-column builder, so a success or an error row means the
-- same thing in both shapes:
--   mutation_ok_result(p_result, <mutation_ok's arguments>)  -- result = p_result
--   mutation_err_result(<mutation_err's arguments>)          -- result = NULL
--   mutation_err_entries_result(<mutation_err_entries' arguments>)
-- ============================================================================

CREATE OR REPLACE FUNCTION fraiseql.mutation_ok_result(
    p_result JSONB,
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
    metadata JSONB,
    result JSONB
) AS $$
BEGIN
    RETURN QUERY SELECT ok.*, p_result
    FROM fraiseql.mutation_ok(
        p_entity, p_entity_id, p_entity_type, p_state_changed,
        p_updated_fields, p_cascade, p_metadata
    ) AS ok;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.mutation_ok_result(JSONB, JSONB, UUID, TEXT, BOOLEAN, TEXT[], JSONB, JSONB) IS
'fraiseql.mutation_ok() with a 14th column, result = p_result: the success fields of a
mutation that declares them.';

CREATE OR REPLACE FUNCTION fraiseql.mutation_err_result(
    p_error_class TEXT,
    p_message TEXT DEFAULT '',
    p_error_detail JSONB DEFAULT NULL,
    p_http_status SMALLINT DEFAULT NULL,
    p_entity_type TEXT DEFAULT NULL
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
    metadata JSONB,
    result JSONB
) AS $$
BEGIN
    RETURN QUERY SELECT err.*, NULL::JSONB
    FROM fraiseql.mutation_err(
        p_error_class, p_message, p_error_detail, p_http_status, p_entity_type
    ) AS err;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.mutation_err_result(TEXT, TEXT, JSONB, SMALLINT, TEXT) IS
'fraiseql.mutation_err() with a 14th column, result = NULL, for a function whose row
declares result.';

CREATE OR REPLACE FUNCTION fraiseql.mutation_err_entries_result(
    p_error_class TEXT,
    p_message TEXT,
    VARIADIC p_entries JSONB[]
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
    metadata JSONB,
    result JSONB
) AS $$
BEGIN
    RETURN QUERY SELECT err.*, NULL::JSONB
    FROM fraiseql.mutation_err_entries(p_error_class, p_message, VARIADIC p_entries) AS err;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

COMMENT ON FUNCTION fraiseql.mutation_err_entries_result(TEXT, TEXT, JSONB[]) IS
'fraiseql.mutation_err_entries() with a 14th column, result = NULL, for a function whose
row declares result.';

-- ============================================================================
-- Permissions
-- ============================================================================
-- Grant EXECUTE per function (not `ON ALL FUNCTIONS`, which is a one-time snapshot
-- that would also blanket-grant any future function added to this schema). These are
-- pure IMMUTABLE response builders with no data access. The fraiseql schema itself is
-- owned by the FraiseQL database role and not writable by application code.

GRANT USAGE ON SCHEMA fraiseql TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.library_version() TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.mutation_ok(JSONB, UUID, TEXT, BOOLEAN, TEXT[], JSONB, JSONB) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.mutation_err(TEXT, TEXT, JSONB, SMALLINT, TEXT) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.error_identifier(TEXT) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.error_entry(SMALLINT, TEXT, TEXT, JSONB) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.mutation_err_entries(TEXT, TEXT, JSONB[]) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.mutation_ok_result(JSONB, JSONB, UUID, TEXT, BOOLEAN, TEXT[], JSONB, JSONB) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.mutation_err_result(TEXT, TEXT, JSONB, SMALLINT, TEXT) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.mutation_err_entries_result(TEXT, TEXT, JSONB[]) TO PUBLIC;

-- ============================================================================
-- Tests (run as: \i sql/helpers/mutation_response.sql)
-- ============================================================================

DO $$
BEGIN
    -- Test library_version
    ASSERT (SELECT fraiseql.library_version()) = '2.4.0',
        'library_version should return 2.4.0';

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
        ASSERT v_row.entity_type IS NULL, 'mutation_err should stamp nothing by default';
    END;

    -- Test mutation_err stamping the declared error type it produced
    DECLARE
        v_row RECORD;
    BEGIN
        SELECT * INTO v_row FROM fraiseql.mutation_err(
            'conflict', 'Email taken', p_entity_type => 'DuplicateEmailError');

        ASSERT v_row.entity_type = 'DuplicateEmailError',
            'mutation_err should stamp p_entity_type onto entity_type';
        ASSERT v_row.entity IS NULL, 'a stamped mutation_err still has entity=NULL';
    END;

    -- Test the 14-column forms: same row as their 13-column builder, plus result
    DECLARE
        v_row RECORD;
    BEGIN
        SELECT * INTO v_row FROM fraiseql.mutation_ok_result(
            '{"recovered_items": 3}'::JSONB, '{"id": "abc"}'::JSONB, p_entity_type => 'Order');
        ASSERT v_row.succeeded = TRUE, 'mutation_ok_result should return succeeded=TRUE';
        ASSERT v_row.entity_type = 'Order', 'mutation_ok_result should pass its arguments on';
        ASSERT v_row.result ->> 'recovered_items' = '3', 'mutation_ok_result should carry p_result';

        SELECT * INTO v_row FROM fraiseql.mutation_err_result('not_found', 'gone');
        ASSERT v_row.succeeded = FALSE, 'mutation_err_result should return succeeded=FALSE';
        ASSERT v_row.message = 'gone', 'mutation_err_result should pass its arguments on';
        ASSERT v_row.result IS NULL, 'mutation_err_result should leave result NULL';
    END;

    -- Test error_entry normalising its identifier into a translation key
    ASSERT fraiseql.error_entry(404::SMALLINT, 'Order line_not_found', 'x') ->> 'identifier'
        = 'order_line_not_found', 'error_entry should normalise a human label';
    ASSERT fraiseql.error_identifier('PaymentTerm') = 'payment_term',
        'error_identifier should split camelCase';

    RAISE NOTICE 'All mutation response tests passed!';
END;
$$;

-- ============================================================================
-- Finalization
-- ============================================================================

COMMENT ON SCHEMA fraiseql IS
'FraiseQL mutation response helpers and infrastructure. Installed by ''fraiseql setup''.
See: https://github.com/fraiseql/fraiseql/issues/230';
