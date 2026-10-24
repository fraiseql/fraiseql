CREATE SCHEMA app;

CREATE TABLE app.tb_order (id uuid PRIMARY KEY, label text NOT NULL);
CREATE TABLE app.tb_receipt (id uuid PRIMARY KEY, order_label text NOT NULL);

CREATE VIEW app.v_order AS
    SELECT id, jsonb_build_object('id', id, 'label', label) AS data FROM app.tb_order;
CREATE VIEW app.v_receipt AS
    SELECT id, jsonb_build_object('id', id, 'orderLabel', order_label) AS data FROM app.tb_receipt;

CREATE FUNCTION app.fn_create_order(p_label text) RETURNS TABLE(succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE v uuid := gen_random_uuid();
BEGIN
    INSERT INTO app.tb_order VALUES (v, p_label);
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        jsonb_build_object('id', v, 'label', p_label), v, 'Order');
END $$;

CREATE FUNCTION app.fn_record_receipt(p_order_label text) RETURNS TABLE(succeeded boolean, state_changed boolean, error_class text, status_detail text, http_status smallint, message text, entity_id uuid, entity_type text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb)
LANGUAGE plpgsql AS $$
DECLARE v uuid := gen_random_uuid();
BEGIN
    INSERT INTO app.tb_receipt VALUES (v, p_order_label);
    RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
        jsonb_build_object('id', v, 'orderLabel', p_order_label), v, 'Receipt');
END $$;
