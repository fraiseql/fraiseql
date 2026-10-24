CREATE SCHEMA app;

CREATE TABLE app.tb_tick (id uuid PRIMARY KEY, zone text NOT NULL);

CREATE VIEW app.v_tick AS
    SELECT id, jsonb_build_object('id', id, 'zone', zone) AS data FROM app.tb_tick;

CREATE FUNCTION app.fn_record_tick(p_zone text) RETURNS TABLE(succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE v uuid := gen_random_uuid();
BEGIN
    INSERT INTO app.tb_tick VALUES (v, p_zone);
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(jsonb_build_object('id', v, 'zone', p_zone), v, 'Tick');
END $$;
