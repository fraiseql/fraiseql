CREATE SCHEMA app;

CREATE TABLE app.tb_pulled (id uuid PRIMARY KEY, page int NOT NULL);

CREATE VIEW app.v_pulled AS
    SELECT id, jsonb_build_object('id', id, 'page', page) AS data FROM app.tb_pulled;

CREATE FUNCTION app.fn_record_page(p_page int) RETURNS TABLE(succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE v uuid := gen_random_uuid();
BEGIN
    INSERT INTO app.tb_pulled VALUES (v, p_page);
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(jsonb_build_object('id', v, 'page', p_page), v, 'Pulled');
END $$;
