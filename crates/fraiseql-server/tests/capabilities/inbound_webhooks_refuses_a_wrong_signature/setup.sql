CREATE SCHEMA app;

CREATE TABLE app.tb_delivery (id uuid PRIMARY KEY, source text NOT NULL);

CREATE VIEW app.v_delivery AS
    SELECT id, jsonb_build_object('id', id, 'source', source) AS data FROM app.tb_delivery;

CREATE FUNCTION app.fn_record_delivery(p_source text) RETURNS TABLE(succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE v uuid := gen_random_uuid();
BEGIN
    INSERT INTO app.tb_delivery VALUES (v, p_source);
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(jsonb_build_object('id', v, 'source', p_source), v, 'Delivery');
END $$;
