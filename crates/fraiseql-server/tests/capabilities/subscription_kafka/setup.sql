CREATE SCHEMA app;

CREATE TABLE app.tb_note (id uuid PRIMARY KEY, title text NOT NULL);

CREATE VIEW app.v_note AS
    SELECT id, jsonb_build_object('id', id, 'title', title) AS data FROM app.tb_note;
