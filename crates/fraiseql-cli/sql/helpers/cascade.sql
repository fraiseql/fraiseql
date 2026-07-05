-- FraiseQL cascade builders (graphql-cascade spec)
--
-- Assemble the `cascade` JSONB that a mutation function returns in its
-- `app.mutation_response`. FraiseQL's runtime consumes the spec-nested shape these
-- builders emit — `{ updated: [{__typename,id,operation,entity}],
-- deleted: [{__typename,id,deletedAt}], invalidations: [...], metadata: {...} }` —
-- projects and field-authorizes every entity, and enforces the response limits.
--
-- RLS boundary (IMPORTANT): `cascade_entity` reads each entity from its
-- RLS-protected VIEW (`v_*`), never a base table and never under SECURITY DEFINER.
-- That is what makes cascade row-visibility match a query's: a row the caller
-- cannot see is not returned by the view, so it never rides in the cascade. The
-- runtime enforces field-level authorization on top, but it cannot re-check row
-- visibility — so the paved path (these builders) must read through the views.

CREATE SCHEMA IF NOT EXISTS fraiseql;

-- Build one created/updated cascade entry: `{__typename, id, operation, entity}`.
-- `p_view_name` MUST be an RLS-protected view (`v_*`). Returns NULL when the row is
-- not visible to the caller (or gone), so `build_cascade` can omit it.
CREATE OR REPLACE FUNCTION fraiseql.cascade_entity(
    p_typename TEXT,
    p_id UUID,
    p_operation TEXT,        -- 'CREATED' or 'UPDATED'
    p_view_name TEXT         -- an RLS-protected view (v_*), NEVER a base table
) RETURNS JSONB AS $$
DECLARE
    v_entity_data JSONB;
BEGIN
    -- Read through the RLS view so an invisible row yields no data (never rides in
    -- the cascade). NEVER read `tb_*` directly or wrap this in SECURITY DEFINER —
    -- either bypasses RLS and no runtime check can catch it.
    EXECUTE format('SELECT data FROM %I WHERE id = $1', p_view_name)
    INTO v_entity_data
    USING p_id;

    IF v_entity_data IS NULL THEN
        RETURN NULL;   -- not visible to the caller (or gone) → omit
    END IF;

    RETURN jsonb_build_object(
        '__typename', p_typename,
        'id',         p_id,
        'operation',  p_operation,
        'entity',     v_entity_data
    );
END;
$$ LANGUAGE plpgsql;

-- Build one deleted cascade entry: `{__typename, id, deletedAt}`. A deleted row has
-- no entity body (it is gone), so this carries only the identity + deletion time.
CREATE OR REPLACE FUNCTION fraiseql.deleted_entity(
    p_typename TEXT,
    p_id UUID,
    p_deleted_at TIMESTAMPTZ DEFAULT now()
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        '__typename', p_typename,
        'id',         p_id,
        'deletedAt',  p_deleted_at
    );
END;
$$ LANGUAGE plpgsql;

-- Build one client-side cache-invalidation hint: `{queryName, strategy, scope}`.
CREATE OR REPLACE FUNCTION fraiseql.cascade_invalidation(
    p_query_name TEXT,
    p_strategy TEXT DEFAULT 'INVALIDATE',   -- INVALIDATE | REFETCH | REMOVE
    p_scope TEXT DEFAULT 'PREFIX'           -- EXACT | PREFIX | PATTERN | ALL
) RETURNS JSONB AS $$
BEGIN
    RETURN jsonb_build_object(
        'queryName', p_query_name,
        'strategy',  p_strategy,
        'scope',     p_scope
    );
END;
$$ LANGUAGE plpgsql;

-- Assemble the cascade envelope from the entry arrays. NULL entries (an
-- invisible-to-the-caller `cascade_entity`) are dropped, so an invisible entity is
-- omitted rather than shipped as a JSON null. `metadata` is auto-populated when not
-- supplied; the runtime owns `affectedCount`/`truncated`/`timestamp` on the wire.
CREATE OR REPLACE FUNCTION fraiseql.build_cascade(
    p_updated JSONB DEFAULT '[]'::jsonb,
    p_deleted JSONB DEFAULT '[]'::jsonb,
    p_invalidations JSONB DEFAULT '[]'::jsonb,
    p_metadata JSONB DEFAULT NULL
) RETURNS JSONB AS $$
DECLARE
    v_updated JSONB;
    v_metadata JSONB;
BEGIN
    SELECT COALESCE(jsonb_agg(e), '[]'::jsonb) INTO v_updated
    FROM jsonb_array_elements(COALESCE(p_updated, '[]'::jsonb)) e
    WHERE e IS NOT NULL AND e <> 'null'::jsonb;

    v_metadata := COALESCE(p_metadata, jsonb_build_object(
        'timestamp',     now(),
        'depth',         1,
        'affectedCount', jsonb_array_length(v_updated) + jsonb_array_length(COALESCE(p_deleted, '[]'::jsonb))
    ));

    RETURN jsonb_build_object(
        'updated',       v_updated,
        'deleted',       COALESCE(p_deleted, '[]'::jsonb),
        'invalidations', COALESCE(p_invalidations, '[]'::jsonb),
        'metadata',      v_metadata
    );
END;
$$ LANGUAGE plpgsql;

GRANT EXECUTE ON FUNCTION fraiseql.cascade_entity(TEXT, UUID, TEXT, TEXT) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.deleted_entity(TEXT, UUID, TIMESTAMPTZ) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.cascade_invalidation(TEXT, TEXT, TEXT) TO PUBLIC;
GRANT EXECUTE ON FUNCTION fraiseql.build_cascade(JSONB, JSONB, JSONB, JSONB) TO PUBLIC;
