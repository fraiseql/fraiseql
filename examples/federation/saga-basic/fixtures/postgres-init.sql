-- FraiseQL Saga Tables
CREATE TABLE sagas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_type VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    data JSONB,
    error_message TEXT
);

CREATE TABLE saga_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_id UUID NOT NULL REFERENCES sagas(id) ON DELETE CASCADE,
    step_index INT NOT NULL,
    name VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    input JSONB,
    output JSONB,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ
);

CREATE INDEX idx_sagas_status ON sagas(status);
CREATE INDEX idx_sagas_created_at ON sagas(created_at);
CREATE INDEX idx_saga_steps_saga_id ON saga_steps(saga_id);
CREATE INDEX idx_saga_steps_status ON saga_steps(status);

-- Users Service Tables
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_users_email ON users(email);

-- Sample users
INSERT INTO users (id, name, email) VALUES
  ('550e8400-e29b-41d4-a716-446655440001', 'Alice Johnson', 'alice@example.com'),
  ('550e8400-e29b-41d4-a716-446655440002', 'Bob Smith', 'bob@example.com'),
  ('550e8400-e29b-41d4-a716-446655440003', 'Carol White', 'carol@example.com')
ON CONFLICT DO NOTHING;

-- Ledger for order history
CREATE TABLE user_orders_ledger (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    order_id UUID,
    event_type VARCHAR(50), -- 'ORDER_CREATED', 'ORDER_CANCELLED'
    amount DECIMAL(10, 2),
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_user_orders_ledger_user_id ON user_orders_ledger(user_id);
