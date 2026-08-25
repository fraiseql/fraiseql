-- Users subgraph database (Trinity Pattern).
--
-- This subgraph OWNS the User entity: it holds the identity and profile fields, and
-- the router resolves `User` here by its `@key(fields: "id")`.
--
-- Pattern: tb_* (table), pk_* (INTEGER surrogate key, never leaves the database),
-- id (UUID, the public/federation identity), v_* (view exposing `id` natively plus a
-- JSONB `data` column with snake_case keys that FraiseQL projects to camelCase).

DROP VIEW IF EXISTS v_user;
DROP TABLE IF EXISTS tb_user CASCADE;

CREATE TABLE tb_user (
    pk_user    SERIAL PRIMARY KEY,
    id         UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    name       VARCHAR(255) NOT NULL,
    email      VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_tb_user_email ON tb_user(email);

-- `id` is selected as a native column, not only as a JSONB key: an ID-typed query
-- argument compiles to `WHERE id = $1::uuid`, which needs the column to exist.
CREATE VIEW v_user AS
SELECT
    pk_user,
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) AS data
FROM tb_user;

-- Fixed UUIDs so the orders subgraph's seed can reference them and so the queries in
-- README.md can be copied verbatim.
INSERT INTO tb_user (id, name, email) VALUES
    ('11111111-1111-4111-8111-111111111111', 'Alice Johnson',  'alice@example.com'),
    ('22222222-2222-4222-8222-222222222222', 'Bob Smith',      'bob@example.com'),
    ('33333333-3333-4333-8333-333333333333', 'Charlie Brown',  'charlie@example.com');
