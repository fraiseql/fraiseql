CREATE SCHEMA app;
CREATE TABLE app.tb_order (id uuid PRIMARY KEY);
CREATE VIEW app.v_order AS SELECT id, jsonb_build_object('id', id) AS data FROM app.tb_order;
