-- FraiseQL Saga Tables (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (UUID natural key), v_* (view)

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
        'sagaType', saga_type,
        'status', status,
        'createdAt', created_at,
        'updatedAt', updated_at,
        'errorMessage', error_message
    ) AS data
FROM tb_saga;

CREATE VIEW v_saga_step AS
SELECT
    pk_saga_step,
    jsonb_build_object(
        'id', id,
        'stepIndex', step_index,
        'name', name,
        'status', status,
        'createdAt', created_at,
        'completedAt', completed_at
    ) AS data
FROM tb_saga_step;

CREATE VIEW v_user AS
SELECT
    pk_user,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) AS data
FROM tb_user;

CREATE VIEW v_user_order_ledger AS
SELECT
    pk_ledger_entry,
    jsonb_build_object(
        'id', id,
        'orderId', order_id,
        'eventType', event_type,
        'amount', amount,
        'createdAt', created_at
    ) AS data
FROM tb_user_order_ledger;

-- Sample users
INSERT INTO tb_user (id, name, email) VALUES
  ('550e8400-e29b-41d4-a716-446655440001', 'Alice Johnson', 'alice@example.com'),
  ('550e8400-e29b-41d4-a716-446655440002', 'Bob Smith', 'bob@example.com'),
  ('550e8400-e29b-41d4-a716-446655440003', 'Carol White', 'carol@example.com')
ON CONFLICT DO NOTHING;
