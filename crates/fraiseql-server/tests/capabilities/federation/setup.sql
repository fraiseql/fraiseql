CREATE SCHEMA app;

CREATE TABLE app.tb_note (id uuid PRIMARY KEY, title text NOT NULL);

CREATE VIEW app.v_note AS
    SELECT id, jsonb_build_object('id', id, 'title', title) AS data FROM app.tb_note;

CREATE FUNCTION app.fn_create_note(p_title text) RETURNS TABLE(succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE v uuid := gen_random_uuid();
BEGIN
    INSERT INTO app.tb_note VALUES (v, p_title);
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(jsonb_build_object('id', v, 'title', p_title), v, 'Note');
END $$;

INSERT INTO app.tb_note VALUES ('00000000-0000-4000-8000-000000000001', 'seeded');
