-- FraiseQL Observer System - Changelog GraphQL Exposure
-- This migration surfaces the observer change-log as queryable GraphQL types
-- (see issue #149). It installs:
--   - core.v_entity_change_log       : read projection over tb_entity_change_log
--   - core.v_transport_checkpoint    : read projection over tb_transport_checkpoint
--   - core.fn_upsert_transport_checkpoint : idempotent checkpoint upsert
--
-- Gating: applied only when `[changelog] expose = true` (the compiler injects the
-- matching GraphQL types under the same condition).
--
-- PREREQUISITES (best-effort migration — it does NOT create the base tables):
--   - core.tb_entity_change_log   : the observer change-log table (user/observer
--                                   install convention; see the observer guide).
--   - core.tb_transport_checkpoint: installed by 03_add_nats_transport.sql.
--   - app.mutation_response       : the standard FraiseQL mutation-result composite
--                                   (see docs/architecture/mutation-response.md).
-- The migration fails cleanly if a prerequisite is absent.
--
-- FraiseQL convention: a queryable view exposes all GraphQL fields inside a single
-- `data` JSONB column (the runtime projects `data->>'field'`); the additional
-- top-level columns are kept for indexed WHERE/ORDER BY and self-documentation.

-- ============================================================================
-- Entity Change Log View
-- ============================================================================
-- Cursor key is `pk_entity_change_log` (BIGINT). The GraphQL `entity_change_logs`
-- query paginates with `where: { pk_entity_change_log: { gt: $cursor } }
-- orderBy: [{ field: "pk_entity_change_log", direction: ASC }]`.

CREATE OR REPLACE VIEW core.v_entity_change_log AS
SELECT
    pk_entity_change_log,
    object_type,
    modification_type,
    created_at,
    jsonb_build_object(
        'id',                id,
        'pk_entity_change_log', pk_entity_change_log,
        'fk_customer_org',   fk_customer_org,
        'fk_contact',        fk_contact,
        'object_type',       object_type,
        'object_id',         object_id,
        'modification_type', modification_type,
        'change_status',     change_status,
        'object_data',       object_data,
        'extra_metadata',    extra_metadata,
        'created_at',        created_at
    ) AS data
FROM core.tb_entity_change_log;

-- ============================================================================
-- Transport Checkpoint View
-- ============================================================================
-- The checkpoint is keyed by `transport_name` (the consumer/transport identifier).

CREATE OR REPLACE VIEW core.v_transport_checkpoint AS
SELECT
    transport_name,
    last_pk,
    updated_at,
    jsonb_build_object(
        'transport_name', transport_name,
        'last_pk',        last_pk,
        'updated_at',     updated_at
    ) AS data
FROM core.tb_transport_checkpoint;

-- ============================================================================
-- Checkpoint Upsert Function
-- ============================================================================
-- Advances (or creates) a consumer's checkpoint. Returns `app.mutation_response`
-- so it slots into the FraiseQL mutation runner like any other mutation. Fields
-- are assigned by name (order-independent) per docs/architecture/mutation-response.md.
-- `core.tb_transport_checkpoint.transport_name` is the PRIMARY KEY, so the
-- ON CONFLICT target is always satisfied.

CREATE OR REPLACE FUNCTION core.fn_upsert_transport_checkpoint(
    p_transport_name text,
    p_last_pk        bigint
) RETURNS app.mutation_response
LANGUAGE plpgsql AS $$
DECLARE
    v_row      core.tb_transport_checkpoint%ROWTYPE;
    v_existing bigint;
    v_response app.mutation_response;
BEGIN
    SELECT last_pk INTO v_existing
    FROM core.tb_transport_checkpoint
    WHERE transport_name = p_transport_name;

    INSERT INTO core.tb_transport_checkpoint (transport_name, last_pk, updated_at)
    VALUES (p_transport_name, p_last_pk, NOW())
    ON CONFLICT (transport_name) DO UPDATE
        SET last_pk    = EXCLUDED.last_pk,
            updated_at = NOW()
    RETURNING * INTO v_row;

    v_response.succeeded     := true;
    -- A repeat call with the same cursor is an idempotent no-op.
    v_response.state_changed := v_existing IS DISTINCT FROM v_row.last_pk;
    v_response.message       := 'checkpoint upserted';
    v_response.entity_type   := 'TransportCheckpoint';
    v_response.entity        := jsonb_build_object(
        'transport_name', v_row.transport_name,
        'last_pk',        v_row.last_pk,
        'updated_at',     v_row.updated_at
    );
    v_response.updated_fields := ARRAY['last_pk', 'updated_at'];

    RETURN v_response;
END;
$$;

-- ============================================================================
-- Comments
-- ============================================================================

COMMENT ON VIEW core.v_entity_change_log IS
    'GraphQL read projection over tb_entity_change_log (#149). Cursor key: pk_entity_change_log.';

COMMENT ON VIEW core.v_transport_checkpoint IS
    'GraphQL read projection over tb_transport_checkpoint (#149). Keyed by transport_name.';

COMMENT ON FUNCTION core.fn_upsert_transport_checkpoint(text, bigint) IS
    'Idempotent checkpoint upsert backing the upsert_transport_checkpoint GraphQL mutation (#149).';
