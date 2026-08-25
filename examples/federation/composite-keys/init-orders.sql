-- Orders subgraph database — multi-tenant, composite-key federation (Trinity Pattern).
--
-- This subgraph OWNS Order and EXTENDS the User the users subgraph owns. The extension
-- borrows BOTH halves of the key: a user is (organization_id, user_id), so an order can
-- only ever be attached to a user within a named tenant.
--
-- There is no users table here. The only thing this subgraph learns about a user is the
-- key pair the router hands it in an `_entities` representation.

DROP VIEW IF EXISTS v_user;
DROP VIEW IF EXISTS v_order;
DROP TABLE IF EXISTS tb_order CASCADE;

CREATE TABLE tb_order (
    pk_order        SERIAL PRIMARY KEY,
    id              UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL,
    user_id         UUID NOT NULL,
    status          VARCHAR(50) NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'completed', 'cancelled')),
    total           NUMERIC(10, 2) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_tb_order_tenant ON tb_order(organization_id, user_id);

CREATE VIEW v_order AS
SELECT
    pk_order,
    id,
    organization_id,
    user_id,
    jsonb_build_object(
        'id', id,
        'organization_id', organization_id,
        'user_id', user_id,
        'status', status,
        'total', total,
        'created_at', created_at
    ) AS data
FROM tb_order;

-- The EXTENDED User, keyed by the same pair the users subgraph uses.
CREATE VIEW v_user AS
SELECT
    o.organization_id,
    o.user_id,
    jsonb_build_object(
        'organization_id', o.organization_id,
        'user_id', o.user_id,
        'orders', jsonb_agg(
            jsonb_build_object(
                'id', o.id,
                'organization_id', o.organization_id,
                'user_id', o.user_id,
                'status', o.status,
                'total', o.total,
                'created_at', o.created_at
            )
            ORDER BY o.created_at
        )
    ) AS data
FROM tb_order o
GROUP BY o.organization_id, o.user_id;

INSERT INTO tb_order (id, organization_id, user_id, status, total) VALUES
    ('bbbbbbbb-0001-4000-8000-000000000001', '00000000-0000-4000-8000-00000000000a', '10000000-0000-4000-8000-000000000001', 'completed', 120.00),
    ('bbbbbbbb-0002-4000-8000-000000000002', '00000000-0000-4000-8000-00000000000a', '10000000-0000-4000-8000-000000000001', 'pending',    75.50),
    ('bbbbbbbb-0003-4000-8000-000000000003', '00000000-0000-4000-8000-00000000000a', '10000000-0000-4000-8000-000000000002', 'completed',  42.00),
    ('bbbbbbbb-0004-4000-8000-000000000004', '00000000-0000-4000-8000-00000000000b', '10000000-0000-4000-8000-000000000003', 'completed', 310.25);
