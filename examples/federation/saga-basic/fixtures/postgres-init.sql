-- FraiseQL Saga Tables (Trinity Pattern)
-- Pattern: tb_* (table), pk_* (INTEGER primary key), id (UUID natural key), v_* (view)

DROP TABLE IF EXISTS tb_user_orders_ledger CASCADE;
DROP TABLE IF EXISTS tb_saga_steps CASCADE;
DROP TABLE IF EXISTS tb_sagas CASCADE;
DROP TABLE IF EXISTS tb_users CASCADE;

CREATE TABLE tb_sagas (
    pk_saga SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    saga_type VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    data JSONB,
    error_message TEXT
);

CREATE TABLE tb_saga_steps (
    pk_saga_step SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_saga INTEGER NOT NULL REFERENCES tb_sagas(pk_saga) ON DELETE CASCADE,
    step_index INT NOT NULL,
    name VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    input JSONB,
    output JSONB,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_tb_sagas_status ON tb_sagas(status);
CREATE INDEX idx_tb_sagas_created_at ON tb_sagas(created_at);
CREATE INDEX idx_tb_sagas_id ON tb_sagas(id);
CREATE INDEX idx_tb_saga_steps_fk_saga ON tb_saga_steps(fk_saga);
CREATE INDEX idx_tb_saga_steps_status ON tb_saga_steps(status);
CREATE INDEX idx_tb_saga_steps_id ON tb_saga_steps(id);

-- Users Service Tables
CREATE TABLE tb_users (
    pk_user SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tb_users_email ON tb_users(email);
CREATE INDEX idx_tb_users_id ON tb_users(id);

-- Ledger for order history
CREATE TABLE tb_user_orders_ledger (
    pk_ledger_entry SERIAL PRIMARY KEY,
    id UUID DEFAULT gen_random_uuid() UNIQUE NOT NULL,
    fk_user INTEGER NOT NULL REFERENCES tb_users(pk_user),
    order_id UUID,
    event_type VARCHAR(50), -- 'ORDER_CREATED', 'ORDER_CANCELLED'
    amount DECIMAL(10, 2),
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_tb_user_orders_ledger_fk_user ON tb_user_orders_ledger(fk_user);
CREATE INDEX idx_tb_user_orders_ledger_id ON tb_user_orders_ledger(id);

-- Create views (Trinity Pattern v_* naming)
CREATE VIEW v_sagas AS
SELECT pk_saga, id, saga_type, status, created_at, updated_at, data, error_message
FROM tb_sagas;

CREATE VIEW v_saga_steps AS
SELECT pk_saga_step, id, fk_saga, step_index, name, status, input, output, created_at, completed_at
FROM tb_saga_steps;

CREATE VIEW v_users AS
SELECT pk_user, id, name, email, created_at, updated_at
FROM tb_users;

CREATE VIEW v_user_orders_ledger AS
SELECT pk_ledger_entry, id, fk_user, order_id, event_type, amount, created_at
FROM tb_user_orders_ledger;

-- Sample users
INSERT INTO tb_users (id, name, email) VALUES
  ('550e8400-e29b-41d4-a716-446655440001', 'Alice Johnson', 'alice@example.com'),
  ('550e8400-e29b-41d4-a716-446655440002', 'Bob Smith', 'bob@example.com'),
  ('550e8400-e29b-41d4-a716-446655440003', 'Carol White', 'carol@example.com')
ON CONFLICT DO NOTHING;
