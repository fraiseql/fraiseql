-- Users subgraph database — multi-tenant, composite-key federation (Trinity Pattern).
--
-- A user's identity here is the PAIR (organization_id, user_id), not a single column.
-- That is the point of the example: the federation key is `@key(fields: "organizationId
-- userId")`, so no subgraph can resolve a user without naming the tenant, and a
-- cross-tenant reference is not expressible in the graph at all.

DROP VIEW IF EXISTS v_user;
DROP VIEW IF EXISTS v_organization;
DROP TABLE IF EXISTS tb_user CASCADE;
DROP TABLE IF EXISTS tb_organization CASCADE;

CREATE TABLE tb_organization (
    pk_organization SERIAL PRIMARY KEY,
    id              UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    name            VARCHAR(255) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE tb_user (
    pk_user         SERIAL PRIMARY KEY,
    fk_organization INTEGER NOT NULL REFERENCES tb_organization(pk_organization),
    organization_id UUID NOT NULL,
    user_id         UUID NOT NULL DEFAULT gen_random_uuid(),
    name            VARCHAR(255) NOT NULL,
    email           VARCHAR(255) NOT NULL,
    role            VARCHAR(50) NOT NULL DEFAULT 'member',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The composite identity. Both halves are needed to name a user, which is exactly
    -- what the federation key says.
    UNIQUE (organization_id, user_id)
);

CREATE INDEX idx_tb_user_organization_id ON tb_user(organization_id);

CREATE VIEW v_organization AS
SELECT
    pk_organization,
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'created_at', created_at
    ) AS data
FROM tb_organization;

-- Both key columns are selected natively as well as inside `data`: an ID-typed query
-- argument compiles to `WHERE <arg> = $1::uuid` against a real column.
CREATE VIEW v_user AS
SELECT
    pk_user,
    organization_id,
    user_id,
    jsonb_build_object(
        'organization_id', organization_id,
        'user_id', user_id,
        'name', name,
        'email', email,
        'role', role,
        'created_at', created_at
    ) AS data
FROM tb_user;

-- Fixed UUIDs so the orders subgraph's seed can reference them and the README's queries
-- can be copied verbatim.
INSERT INTO tb_organization (id, name) VALUES
    ('00000000-0000-4000-8000-00000000000a', 'Acme Corp'),
    ('00000000-0000-4000-8000-00000000000b', 'Globex');

INSERT INTO tb_user (fk_organization, organization_id, user_id, name, email, role)
SELECT o.pk_organization, o.id, u.user_id, u.name, u.email, u.role
FROM (VALUES
    ('00000000-0000-4000-8000-00000000000a'::uuid, '10000000-0000-4000-8000-000000000001'::uuid, 'Alice Johnson', 'alice@acme.example',   'admin'),
    ('00000000-0000-4000-8000-00000000000a'::uuid, '10000000-0000-4000-8000-000000000002'::uuid, 'Bob Smith',     'bob@acme.example',     'member'),
    ('00000000-0000-4000-8000-00000000000b'::uuid, '10000000-0000-4000-8000-000000000003'::uuid, 'Carla Diaz',    'carla@globex.example', 'admin')
) AS u(organization_id, user_id, name, email, role)
JOIN tb_organization o ON o.id = u.organization_id;
