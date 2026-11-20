CREATE SCHEMA app;

CREATE TABLE app.tb_note (id uuid PRIMARY KEY, title text NOT NULL);

CREATE VIEW app.v_note AS
    SELECT id, jsonb_build_object('id', id, 'title', title) AS data FROM app.tb_note;

-- An earlier release created the cursor table unqualified, in the schema that led that
-- deployment's search_path, and a mailbox advanced its cursor there.
CREATE TABLE public._fraiseql_source_cursor (
    source_name  TEXT        PRIMARY KEY,
    cursor_value JSONB,
    version      BIGINT      NOT NULL DEFAULT 0,
    tenant_id    UUID,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
INSERT INTO public._fraiseql_source_cursor (source_name, cursor_value, version)
    VALUES ('email:support', '{"uid_validity": 1, "last_uid": 40}', 40);
