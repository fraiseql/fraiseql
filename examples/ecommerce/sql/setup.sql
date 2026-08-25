-- FraiseQL E-Commerce Example — database setup (Trinity pattern)
-- PostgreSQL 14+
--
-- Naming: tb_* (table), pk_* (INTEGER surrogate key), fk_* (INTEGER foreign key),
-- id (UUID, the identity the GraphQL surface exposes), v_* (view).
--
-- Every view exposes three things the runtime reads:
--   pk_*  the internal key, used for joins
--   id    a NATIVE uuid column, so `product(id: …)` filters on an indexed column
--         rather than digging through JSONB
--   data  the JSONB payload, snake_case keys, projected to camelCase by the engine
--
-- Nested objects and lists are built INTO `data` (jsonb_build_object / jsonb_agg).
-- FraiseQL projects the selection set out of that blob; it does not issue a second
-- query per nested field.
--
-- Run it directly:
--   psql -v ON_ERROR_STOP=1 -f sql/setup.sql "$DATABASE_URL"
--
-- Docker mounts this file as an initdb script, where it runs exactly once on an
-- empty data directory.

\set ON_ERROR_STOP on

-- ---------------------------------------------------------------------------
-- Write side
-- ---------------------------------------------------------------------------

DROP VIEW IF EXISTS v_order_item CASCADE;
DROP VIEW IF EXISTS v_order CASCADE;
DROP VIEW IF EXISTS v_customer CASCADE;
DROP VIEW IF EXISTS v_product CASCADE;
DROP VIEW IF EXISTS v_category CASCADE;
DROP TABLE IF EXISTS tb_order_item CASCADE;
DROP TABLE IF EXISTS tb_order CASCADE;
DROP TABLE IF EXISTS tb_customer CASCADE;
DROP TABLE IF EXISTS tb_product CASCADE;
DROP TABLE IF EXISTS tb_category CASCADE;
DROP TYPE IF EXISTS order_status CASCADE;

CREATE TYPE order_status AS ENUM (
    'pending',
    'paid',
    'shipped',
    'delivered',
    'cancelled'
);

CREATE TABLE tb_category (
    pk_category  SERIAL PRIMARY KEY,
    id           UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    name         VARCHAR(100) NOT NULL,
    slug         VARCHAR(100) NOT NULL UNIQUE,
    description  TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tb_product (
    pk_product    SERIAL PRIMARY KEY,
    id            UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    sku           VARCHAR(50) NOT NULL UNIQUE,
    name          VARCHAR(200) NOT NULL,
    description   TEXT NOT NULL,
    fk_category   INTEGER NOT NULL REFERENCES tb_category(pk_category),
    price         NUMERIC(10, 2) NOT NULL CHECK (price >= 0),
    stock         INTEGER NOT NULL DEFAULT 0 CHECK (stock >= 0),
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tb_customer (
    pk_customer  SERIAL PRIMARY KEY,
    id           UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    email        VARCHAR(255) NOT NULL UNIQUE,
    first_name   VARCHAR(100) NOT NULL,
    last_name    VARCHAR(100) NOT NULL,
    country      VARCHAR(2) NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tb_order (
    pk_order      SERIAL PRIMARY KEY,
    id            UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    order_number  VARCHAR(20) NOT NULL UNIQUE,
    fk_customer   INTEGER NOT NULL REFERENCES tb_customer(pk_customer),
    status        order_status NOT NULL DEFAULT 'pending',
    currency      VARCHAR(3) NOT NULL DEFAULT 'EUR',
    placed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE tb_order_item (
    pk_order_item  SERIAL PRIMARY KEY,
    id             UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    fk_order       INTEGER NOT NULL REFERENCES tb_order(pk_order) ON DELETE CASCADE,
    fk_product     INTEGER NOT NULL REFERENCES tb_product(pk_product),
    quantity       INTEGER NOT NULL CHECK (quantity > 0),
    unit_price     NUMERIC(10, 2) NOT NULL CHECK (unit_price >= 0)
);

CREATE INDEX idx_tb_product_fk_category ON tb_product(fk_category);
CREATE INDEX idx_tb_product_is_active ON tb_product(is_active);
CREATE INDEX idx_tb_order_fk_customer ON tb_order(fk_customer);
CREATE INDEX idx_tb_order_status ON tb_order(status);
CREATE INDEX idx_tb_order_item_fk_order ON tb_order_item(fk_order);
CREATE INDEX idx_tb_order_item_fk_product ON tb_order_item(fk_product);

-- ---------------------------------------------------------------------------
-- Seed data: 5 categories, 12 products, 5 customers, 7 orders
--
-- Fixed UUIDs, so every id in this example's README and .graphql files is stable
-- across a rebuild. A real application leaves `id` to its DEFAULT.
-- ---------------------------------------------------------------------------

INSERT INTO tb_category (id, name, slug, description) VALUES
    ('a1000000-0000-4000-8000-000000000001', 'Coffee',      'coffee',      'Single-origin beans and blends'),
    ('a1000000-0000-4000-8000-000000000002', 'Tea',         'tea',         'Loose-leaf tea'),
    ('a1000000-0000-4000-8000-000000000003', 'Brewing',     'brewing',     'Grinders, kettles and filters'),
    ('a1000000-0000-4000-8000-000000000004', 'Glassware',   'glassware',   'Cups, carafes and servers'),
    ('a1000000-0000-4000-8000-000000000005', 'Subscriptions', 'subscriptions', 'Recurring deliveries');

INSERT INTO tb_product (id, sku, name, description, fk_category, price, stock, is_active) VALUES
    ('b2000000-0000-4000-8000-000000000001', 'COF-ETH-250', 'Ethiopia Yirgacheffe 250g', 'Washed, floral, notes of bergamot.',      1,  14.50, 120, TRUE),
    ('b2000000-0000-4000-8000-000000000002', 'COF-COL-250', 'Colombia Huila 250g',       'Washed, caramel and red apple.',          1,  12.00, 210, TRUE),
    ('b2000000-0000-4000-8000-000000000003', 'COF-BRA-1KG', 'Brazil Cerrado 1kg',        'Natural, chocolate and hazelnut.',        1,  38.00,  45, TRUE),
    ('b2000000-0000-4000-8000-000000000004', 'COF-DEC-250', 'Swiss Water Decaf 250g',    'Decaffeinated without solvents.',         1,  15.00,   0, TRUE),
    ('b2000000-0000-4000-8000-000000000005', 'TEA-SEN-100', 'Sencha 100g',               'Steamed Japanese green tea.',             2,   9.50,  80, TRUE),
    ('b2000000-0000-4000-8000-000000000006', 'TEA-EAR-100', 'Earl Grey 100g',            'Ceylon base with bergamot oil.',          2,   7.50, 140, TRUE),
    ('b2000000-0000-4000-8000-000000000007', 'BRW-GRD-01',  'Hand Grinder',              'Conical burr, 40 clicks.',                3,  79.00,  30, TRUE),
    ('b2000000-0000-4000-8000-000000000008', 'BRW-KET-09',  'Gooseneck Kettle 0.9L',     'Variable temperature, 1200W.',            3,  89.00,  18, TRUE),
    ('b2000000-0000-4000-8000-000000000009', 'BRW-FIL-100', 'Paper Filters (100)',       'Size 02, oxygen-bleached.',               3,   6.00, 500, TRUE),
    ('b2000000-0000-4000-8000-00000000000a', 'GLS-CUP-200', 'Double-Wall Cup 200ml',     'Borosilicate, dishwasher safe.',          4,  16.00,  64, TRUE),
    ('b2000000-0000-4000-8000-00000000000b', 'GLS-CAR-600', 'Serving Carafe 600ml',      'Heat-resistant, stackable lid.',          4,  24.00,  22, TRUE),
    ('b2000000-0000-4000-8000-00000000000c', 'SUB-MON-250', 'Monthly 250g Subscription', 'One bag a month, cancel any time.',       5,  13.00,   0, FALSE);

INSERT INTO tb_customer (id, email, first_name, last_name, country) VALUES
    ('c3000000-0000-4000-8000-000000000001', 'ada@example.com',     'Ada',     'Lovelace',  'GB'),
    ('c3000000-0000-4000-8000-000000000002', 'grace@example.com',   'Grace',   'Hopper',    'US'),
    ('c3000000-0000-4000-8000-000000000003', 'alan@example.com',    'Alan',    'Turing',    'GB'),
    ('c3000000-0000-4000-8000-000000000004', 'katherine@example.com', 'Katherine', 'Johnson', 'US'),
    ('c3000000-0000-4000-8000-000000000005', 'edsger@example.com',  'Edsger',  'Dijkstra',  'NL');

INSERT INTO tb_order (id, order_number, fk_customer, status, currency, placed_at) VALUES
    ('d4000000-0000-4000-8000-000000000001', 'ORD-1001', 1, 'delivered', 'EUR', TIMESTAMPTZ '2026-01-08 09:15:00+00'),
    ('d4000000-0000-4000-8000-000000000002', 'ORD-1002', 1, 'shipped',   'EUR', TIMESTAMPTZ '2026-02-11 14:02:00+00'),
    ('d4000000-0000-4000-8000-000000000003', 'ORD-1003', 2, 'delivered', 'EUR', TIMESTAMPTZ '2026-02-19 08:44:00+00'),
    ('d4000000-0000-4000-8000-000000000004', 'ORD-1004', 3, 'paid',      'EUR', TIMESTAMPTZ '2026-03-02 17:30:00+00'),
    ('d4000000-0000-4000-8000-000000000005', 'ORD-1005', 4, 'pending',   'EUR', TIMESTAMPTZ '2026-03-14 11:05:00+00'),
    ('d4000000-0000-4000-8000-000000000006', 'ORD-1006', 5, 'cancelled', 'EUR', TIMESTAMPTZ '2026-03-21 19:48:00+00'),
    ('d4000000-0000-4000-8000-000000000007', 'ORD-1007', 2, 'delivered', 'EUR', TIMESTAMPTZ '2026-04-05 07:12:00+00');

INSERT INTO tb_order_item (fk_order, fk_product, quantity, unit_price) VALUES
    (1,  1, 2, 14.50),
    (1,  9, 1,  6.00),
    (2,  7, 1, 79.00),
    (3,  2, 3, 12.00),
    (3, 10, 2, 16.00),
    (4,  8, 1, 89.00),
    (4,  9, 2,  6.00),
    (5,  5, 1,  9.50),
    (6,  3, 1, 38.00),
    (7,  6, 4,  7.50),
    (7, 11, 1, 24.00);

-- ---------------------------------------------------------------------------
-- Read side
-- ---------------------------------------------------------------------------

CREATE VIEW v_category AS
SELECT
    c.pk_category,
    c.id,
    jsonb_build_object(
        'id',            c.id,
        'name',          c.name,
        'slug',          c.slug,
        'description',   c.description,
        'product_count', (SELECT count(*) FROM tb_product p WHERE p.fk_category = c.pk_category),
        'created_at',    c.created_at
    ) AS data
FROM tb_category c;

CREATE VIEW v_product AS
SELECT
    p.pk_product,
    p.id,
    jsonb_build_object(
        'id',          p.id,
        'sku',         p.sku,
        'name',        p.name,
        'description', p.description,
        'price',       p.price,
        'stock',       p.stock,
        'in_stock',    p.stock > 0,
        'is_active',   p.is_active,
        'created_at',  p.created_at,
        'category',    jsonb_build_object(
            'id',          c.id,
            'name',        c.name,
            'slug',        c.slug,
            'description', c.description
        )
    ) AS data
FROM tb_product p
JOIN tb_category c ON c.pk_category = p.fk_category;

CREATE VIEW v_customer AS
SELECT
    cu.pk_customer,
    cu.id,
    jsonb_build_object(
        'id',          cu.id,
        'email',       cu.email,
        'first_name',  cu.first_name,
        'last_name',   cu.last_name,
        'full_name',   cu.first_name || ' ' || cu.last_name,
        'country',     cu.country,
        'created_at',  cu.created_at,
        'order_count', (SELECT count(*) FROM tb_order o WHERE o.fk_customer = cu.pk_customer),
        -- Lifetime value counts only orders that were actually paid for.
        'lifetime_value', COALESCE((
            SELECT sum(i.quantity * i.unit_price)
            FROM tb_order o
            JOIN tb_order_item i ON i.fk_order = o.pk_order
            WHERE o.fk_customer = cu.pk_customer
              AND o.status IN ('paid', 'shipped', 'delivered')
        ), 0)
    ) AS data
FROM tb_customer cu;

CREATE VIEW v_order_item AS
SELECT
    i.pk_order_item,
    i.id,
    jsonb_build_object(
        'id',          i.id,
        'quantity',    i.quantity,
        'unit_price',  i.unit_price,
        'total_price', i.quantity * i.unit_price,
        'product',     jsonb_build_object(
            'id',   p.id,
            'sku',  p.sku,
            'name', p.name
        )
    ) AS data
FROM tb_order_item i
JOIN tb_product p ON p.pk_product = i.fk_product;

CREATE VIEW v_order AS
SELECT
    o.pk_order,
    o.id,
    jsonb_build_object(
        'id',           o.id,
        'order_number', o.order_number,
        -- The GraphQL surface publishes OrderStatus, whose value names are
        -- uppercase; the storage enum is lowercase. The view is where the two meet.
        'status',       upper(o.status::text),
        'currency',     o.currency,
        'placed_at',    o.placed_at,
        'item_count',   (SELECT count(*) FROM tb_order_item i WHERE i.fk_order = o.pk_order),
        'total',        COALESCE((
            SELECT sum(i.quantity * i.unit_price)
            FROM tb_order_item i
            WHERE i.fk_order = o.pk_order
        ), 0),
        'customer',     jsonb_build_object(
            'id',         cu.id,
            'email',      cu.email,
            'first_name', cu.first_name,
            'last_name',  cu.last_name,
            'full_name',  cu.first_name || ' ' || cu.last_name,
            'country',    cu.country
        ),
        'items', COALESCE((
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id',          i.id,
                    'quantity',    i.quantity,
                    'unit_price',  i.unit_price,
                    'total_price', i.quantity * i.unit_price,
                    'product',     jsonb_build_object(
                        'id',   p.id,
                        'sku',  p.sku,
                        'name', p.name
                    )
                )
                ORDER BY i.pk_order_item
            )
            FROM tb_order_item i
            JOIN tb_product p ON p.pk_product = i.fk_product
            WHERE i.fk_order = o.pk_order
        ), '[]'::jsonb)
    ) AS data
FROM tb_order o
JOIN tb_customer cu ON cu.pk_customer = o.fk_customer;
