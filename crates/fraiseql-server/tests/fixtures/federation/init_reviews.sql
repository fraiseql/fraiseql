-- Subgraph B (reviews): the federation _entities resolver reads the "user" table
-- directly (SELECT "id", reviewcount FROM "user"). Lowercase column matches the
-- lowercase GraphQL field so the unquoted field name round-trips through PostgreSQL.
CREATE TABLE IF NOT EXISTS "user" (
    id          TEXT PRIMARY KEY,
    reviewcount INTEGER NOT NULL DEFAULT 0,
    __typename  TEXT DEFAULT 'User'
);

INSERT INTO "user" (id, reviewcount) VALUES
    ('user-1', 42)
ON CONFLICT DO NOTHING;
