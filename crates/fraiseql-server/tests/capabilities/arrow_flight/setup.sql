CREATE SCHEMA app;

CREATE TABLE app.tb_note (id uuid PRIMARY KEY, title text NOT NULL);

INSERT INTO app.tb_note VALUES (gen_random_uuid(), 'first'), (gen_random_uuid(), 'second');

CREATE VIEW app.v_note AS
    SELECT id, jsonb_build_object('id', id, 'title', title) AS data FROM app.tb_note;

-- The Arrow view Flight serves: columnar, one column per field.
CREATE VIEW va_note AS SELECT id, title FROM app.tb_note;
