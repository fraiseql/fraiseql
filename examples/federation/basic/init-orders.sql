-- Orders subgraph database (Trinity Pattern).
--
-- This subgraph OWNS the Order entity, and EXTENDS the User entity that the users
-- subgraph owns: it contributes exactly one field, `User.orders`, and borrows the
-- `id` key as `@external`. That is the pattern federation exists for — two databases,
-- one graph, and no foreign key between them.
--
-- There is no `tb_user` here on purpose. The only thing this subgraph knows about a
-- user is the identifier the router hands it in a `_entities` representation.

DROP VIEW IF EXISTS v_user;
DROP VIEW IF EXISTS v_order;
DROP TABLE IF EXISTS tb_order CASCADE;

CREATE TABLE tb_order (
    pk_order   SERIAL PRIMARY KEY,
    id         UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL,
    status     VARCHAR(50) NOT NULL DEFAULT 'pending'
               CHECK (status IN ('pending', 'completed', 'cancelled')),
    total      NUMERIC(10, 2) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_tb_order_user_id ON tb_order(user_id);
CREATE INDEX idx_tb_order_status  ON tb_order(status);

-- `id` and `user_id` are selected natively as well as inside `data`: an ID-typed query
-- argument compiles to `WHERE <arg> = $1::uuid` against a real column, so the planner
-- uses the index instead of extracting from JSONB.
CREATE VIEW v_order AS
SELECT
    pk_order,
    id,
    user_id,
    jsonb_build_object(
        'id', id,
        'user_id', user_id,
        'status', status,
        'total', total,
        'created_at', created_at
    ) AS data
FROM tb_order;

-- The EXTENDED User: one row per user this subgraph has orders for, carrying only the
-- key and the field this subgraph contributes. The router resolves it through
-- `_entities`, passing the `id` it got from the users subgraph.
CREATE VIEW v_user AS
SELECT
    o.user_id AS id,
    jsonb_build_object(
        'id', o.user_id,
        'orders', jsonb_agg(
            jsonb_build_object(
                'id', o.id,
                'user_id', o.user_id,
                'status', o.status,
                'total', o.total,
                'created_at', o.created_at
            )
            ORDER BY o.created_at
        )
    ) AS data
FROM tb_order o
GROUP BY o.user_id;

INSERT INTO tb_order (id, user_id, status, total) VALUES
    ('aaaaaaaa-0001-4000-8000-000000000001', '11111111-1111-4111-8111-111111111111', 'completed',  99.99),
    ('aaaaaaaa-0002-4000-8000-000000000002', '11111111-1111-4111-8111-111111111111', 'completed', 149.99),
    ('aaaaaaaa-0003-4000-8000-000000000003', '11111111-1111-4111-8111-111111111111', 'pending',   199.99),
    ('aaaaaaaa-0004-4000-8000-000000000004', '22222222-2222-4222-8222-222222222222', 'completed', 249.99),
    ('aaaaaaaa-0005-4000-8000-000000000005', '22222222-2222-4222-8222-222222222222', 'pending',   299.99),
    ('aaaaaaaa-0006-4000-8000-000000000006', '33333333-3333-4333-8333-333333333333', 'completed',  59.99);
