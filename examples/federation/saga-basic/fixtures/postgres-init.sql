-- FraiseQL saga example — all three subgraph stores, one PostgreSQL instance.
--
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (UUID natural key), v_* (view).
--
-- Three databases, one per subgraph, because the saga pattern only means something
-- when the stores cannot share a transaction:
--
--   fraiseql            saga coordination + users
--   fraiseql_orders     orders
--   fraiseql_inventory  products and reservations
--
-- Orders and inventory lived in MySQL until #940. FraiseQL has been PostgreSQL-only
-- since v2.15.0 (#374), so that topology demonstrated something the engine refuses.

-- =============================================================================
-- fraiseql — saga coordination and users
-- =============================================================================

DROP TABLE IF EXISTS tb_user_order_ledger CASCADE;
DROP TABLE IF EXISTS tb_saga_step CASCADE;
DROP TABLE IF EXISTS tb_saga CASCADE;
DROP TABLE IF EXISTS tb_user CASCADE;

CREATE TABLE tb_saga (
    pk_saga SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    saga_type VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    data JSONB,
    error_message TEXT
);

CREATE TABLE tb_saga_step (
    pk_saga_step SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_saga INTEGER NOT NULL REFERENCES tb_saga(pk_saga) ON DELETE CASCADE,
    step_index INT NOT NULL,
    name VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    input JSONB,
    output JSONB,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_tb_saga_status ON tb_saga(status);
CREATE INDEX idx_tb_saga_created_at ON tb_saga(created_at);
CREATE INDEX idx_tb_saga_id ON tb_saga(id);
CREATE INDEX idx_tb_saga_step_fk_saga ON tb_saga_step(fk_saga);
CREATE INDEX idx_tb_saga_step_status ON tb_saga_step(status);
CREATE INDEX idx_tb_saga_step_id ON tb_saga_step(id);

-- Users Service Tables
CREATE TABLE tb_user (
    pk_user SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tb_user_email ON tb_user(email);
CREATE INDEX idx_tb_user_id ON tb_user(id);

-- Ledger for order history
CREATE TABLE tb_user_order_ledger (
    pk_ledger_entry SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_user INTEGER NOT NULL REFERENCES tb_user(pk_user),
    order_id UUID,
    event_type VARCHAR(50), -- 'ORDER_CREATED', 'ORDER_CANCELLED'
    amount DECIMAL(10, 2),
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tb_user_order_ledger_fk_user ON tb_user_order_ledger(fk_user);
CREATE INDEX idx_tb_user_order_ledger_id ON tb_user_order_ledger(id);

-- Create views (Trinity Pattern v_* naming)
-- Returns pk_* (for internal joins) and data (JSONB for GraphQL)
CREATE VIEW v_saga AS
SELECT
    pk_saga,
    jsonb_build_object(
        'id', id,
        'saga_type', saga_type,
        'status', status,
        'created_at', created_at,
        'updated_at', updated_at,
        'error_message', error_message
    ) AS data
FROM tb_saga;

CREATE VIEW v_saga_step AS
SELECT
    pk_saga_step,
    jsonb_build_object(
        'id', id,
        'step_index', step_index,
        'name', name,
        'status', status,
        'created_at', created_at,
        'completed_at', completed_at
    ) AS data
FROM tb_saga_step;

CREATE VIEW v_user AS
SELECT
    pk_user,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM tb_user;

CREATE VIEW v_user_order_ledger AS
SELECT
    pk_ledger_entry,
    jsonb_build_object(
        'id', id,
        'order_id', order_id,
        'event_type', event_type,
        'amount', amount,
        'created_at', created_at
    ) AS data
FROM tb_user_order_ledger;

-- Sample users
INSERT INTO tb_user (id, name, email) VALUES
  ('550e8400-e29b-41d4-a716-446655440001', 'Alice Johnson', 'alice@example.com'),
  ('550e8400-e29b-41d4-a716-446655440002', 'Bob Smith', 'bob@example.com'),
  ('550e8400-e29b-41d4-a716-446655440003', 'Carol White', 'carol@example.com')
ON CONFLICT DO NOTHING;


-- =============================================================================
-- fraiseql_orders — the orders subgraph's own store
-- =============================================================================

CREATE DATABASE fraiseql_orders OWNER fraiseql;
\connect fraiseql_orders

CREATE TABLE tb_order (
    pk_order SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    user_id UUID NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    total DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tb_order_item (
    pk_order_item SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_order INTEGER NOT NULL REFERENCES tb_order(pk_order) ON DELETE CASCADE,
    product_id VARCHAR(36) NOT NULL,
    quantity INT NOT NULL,
    price DECIMAL(10, 2) NOT NULL
);

CREATE INDEX idx_tb_order_id ON tb_order(id);
CREATE INDEX idx_tb_order_user_id ON tb_order(user_id);
CREATE INDEX idx_tb_order_status ON tb_order(status);
CREATE INDEX idx_tb_order_item_id ON tb_order_item(id);
CREATE INDEX idx_tb_order_item_fk_order ON tb_order_item(fk_order);

CREATE VIEW v_order AS
SELECT
    pk_order,
    jsonb_build_object(
        'id', id,
        'user_id', user_id,
        'status', status,
        'total', total,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM tb_order;

CREATE VIEW v_order_item AS
SELECT
    pk_order_item,
    jsonb_build_object(
        'id', id,
        'product_id', product_id,
        'quantity', quantity,
        'price', price
    ) AS data
FROM tb_order_item;

-- =============================================================================
-- fraiseql_inventory — the inventory subgraph's own store
-- =============================================================================

\connect fraiseql
CREATE DATABASE fraiseql_inventory OWNER fraiseql;
\connect fraiseql_inventory

CREATE TABLE tb_product (
    pk_product SERIAL PRIMARY KEY,
    id VARCHAR(36) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    stock INT NOT NULL,
    price DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tb_reservation (
    pk_reservation SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    order_id UUID NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'reserved',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tb_reservation_item (
    pk_reservation_item SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_reservation INTEGER NOT NULL REFERENCES tb_reservation(pk_reservation) ON DELETE CASCADE,
    product_id VARCHAR(36) NOT NULL,
    quantity INT NOT NULL
);

CREATE INDEX idx_tb_product_id ON tb_product(id);
CREATE INDEX idx_tb_reservation_id ON tb_reservation(id);
CREATE INDEX idx_tb_reservation_order_id ON tb_reservation(order_id);
CREATE INDEX idx_tb_reservation_status ON tb_reservation(status);
CREATE INDEX idx_tb_reservation_item_id ON tb_reservation_item(id);
CREATE INDEX idx_tb_reservation_item_fk_reservation
    ON tb_reservation_item(fk_reservation);

CREATE VIEW v_product AS
SELECT
    pk_product,
    jsonb_build_object(
        'id', id,
        'name', name,
        'stock', stock,
        'price', price,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM tb_product;

CREATE VIEW v_reservation AS
SELECT
    pk_reservation,
    jsonb_build_object(
        'id', id,
        'order_id', order_id,
        'status', status,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM tb_reservation;

CREATE VIEW v_reservation_item AS
SELECT
    pk_reservation_item,
    jsonb_build_object(
        'id', id,
        'product_id', product_id,
        'quantity', quantity
    ) AS data
FROM tb_reservation_item;

-- Sample inventory
INSERT INTO tb_product (id, name, stock, price) VALUES
  ('prod-001', 'Laptop', 50, 999.99),
  ('prod-002', 'Mouse', 200, 29.99),
  ('prod-003', 'Keyboard', 150, 79.99);
