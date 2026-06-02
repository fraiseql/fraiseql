-- Subgraph A (users): the federation _entities resolver reads the "user" table
-- directly (SELECT "id", name FROM "user"); the user(id) query reads v_user (jsonb data).
CREATE TABLE IF NOT EXISTS "user" (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    __typename TEXT DEFAULT 'User'
);

INSERT INTO "user" (id, name) VALUES
    ('user-1',   'Alice'),
    ('user-bob', 'Bob')
ON CONFLICT DO NOTHING;

CREATE OR REPLACE VIEW v_user AS
SELECT id,
       name,
       jsonb_build_object('id', id, 'name', name) AS data
FROM "user";
