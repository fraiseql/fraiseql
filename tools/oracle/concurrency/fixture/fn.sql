-- Mutation functions. Loaded after `fraiseql-cli setup`, which creates the
-- fraiseql.mutation_ok / fraiseql.mutation_err builders they return through.

-- Bumps the account's version and returns the account.
CREATE FUNCTION fn_bump_account(p_id uuid, p_tenant_id text)
RETURNS TABLE (succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
BEGIN
  UPDATE tb_account SET version = version + 1 WHERE id = p_id AND tenant_id::text = p_tenant_id;
  IF NOT FOUND THEN
    RETURN QUERY SELECT * FROM fraiseql.mutation_err('not_found', 'no such account');
    RETURN;
  END IF;
  RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
    (SELECT data FROM v_account WHERE v_account.id = p_id), p_id, 'Account');
END $$;

-- Bumps the version, then reports a failure.
CREATE FUNCTION fn_bump_fail(p_id uuid, p_tenant_id text)
RETURNS TABLE (succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
BEGIN
  UPDATE tb_account SET version = version + 1 WHERE id = p_id AND tenant_id::text = p_tenant_id;
  RETURN QUERY SELECT * FROM fraiseql.mutation_err('conflict', 'bumped then refused');
END $$;

-- Bumps the version, then sleeps p_ms before returning.
CREATE FUNCTION fn_slow_bump(p_id uuid, p_ms integer, p_tenant_id text)
RETURNS TABLE (succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
BEGIN
  UPDATE tb_account SET version = version + 1 WHERE id = p_id AND tenant_id::text = p_tenant_id;
  PERFORM pg_sleep(p_ms / 1000.0);
  RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
    (SELECT data FROM v_account WHERE v_account.id = p_id), p_id, 'Account');
END $$;

-- Sleeps p_ms, then records one side effect under p_tag.
CREATE FUNCTION fn_record_effect(p_tag text, p_ms integer, p_tenant_id text)
RETURNS TABLE (succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE n bigint;
BEGIN
  PERFORM pg_sleep(p_ms / 1000.0);
  INSERT INTO tb_effect (tag) VALUES (p_tag) RETURNING pk INTO n;
  RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
    jsonb_build_object('id', '00000000-0000-4000-8000-000000000001', 'tenant_id', p_tenant_id,
                       'name', p_tag, 'version', n, 'secret', 's', 'seen_tenant', null),
    '00000000-0000-4000-8000-000000000001'::uuid, 'Account');
END $$;

CREATE FUNCTION fn_create_item(p_fk_account uuid, p_seq bigint, p_tenant_id text)
RETURNS TABLE (succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE v uuid;
BEGIN
  INSERT INTO tb_item (tenant_id, fk_account, seq, label)
  VALUES (p_tenant_id::uuid, p_fk_account, p_seq, 'w' || p_seq) RETURNING id INTO v;
  RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
    (SELECT data FROM v_item WHERE v_item.id = v), v, 'Item');
END $$;

CREATE FUNCTION fn_delete_item(p_id uuid, p_tenant_id text)
RETURNS TABLE (succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE d jsonb;
BEGIN
  SELECT data INTO d FROM v_item WHERE v_item.id = p_id AND tenant_id::text = p_tenant_id;
  IF d IS NULL THEN
    RETURN QUERY SELECT * FROM fraiseql.mutation_err('not_found', 'no such item');
    RETURN;
  END IF;
  DELETE FROM tb_item WHERE id = p_id;
  RETURN QUERY SELECT * FROM fraiseql.mutation_ok(d, p_id, 'Item');
END $$;
