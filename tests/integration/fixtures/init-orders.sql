-- Orders database schema and seed data with trinity pattern

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Trinity pattern for orders
CREATE TABLE tb_order (
    pk_order BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    id UUID NOT NULL UNIQUE DEFAULT uuid_generate_v4(),
    identifier TEXT NOT NULL UNIQUE,  -- Semantic identifier
    user_id UUID NOT NULL,  -- Federation reference to User.id
    status TEXT NOT NULL DEFAULT 'pending',
    total DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Order items junction table
CREATE TABLE tb_order_item (
    pk_order_item BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    order_id UUID NOT NULL,
    product_id UUID NOT NULL,
    quantity INT NOT NULL DEFAULT 1,
    price DECIMAL(10, 2) NOT NULL,
    FOREIGN KEY (order_id) REFERENCES tb_order(id)
);

-- Federation view for orders
CREATE VIEW v_order AS
    SELECT
        id,
        user_id,
        status,
        total,
        identifier,
        created_at
    FROM tb_order;

-- Seed data with explicit UUIDs
INSERT INTO tb_order (id, identifier, user_id, status, total) VALUES
    ('650e8400-e29b-41d4-a716-446655440001', 'order-1', '550e8400-e29b-41d4-a716-446655440001', 'completed', 99.99),
    ('650e8400-e29b-41d4-a716-446655440002', 'order-2', '550e8400-e29b-41d4-a716-446655440001', 'pending', 149.99),
    ('650e8400-e29b-41d4-a716-446655440003', 'order-3', '550e8400-e29b-41d4-a716-446655440002', 'completed', 49.99),
    ('650e8400-e29b-41d4-a716-446655440004', 'order-4', '550e8400-e29b-41d4-a716-446655440002', 'pending', 199.99),
    ('650e8400-e29b-41d4-a716-446655440005', 'order-5', '550e8400-e29b-41d4-a716-446655440003', 'completed', 299.99),
    ('650e8400-e29b-41d4-a716-446655440006', 'order-6', '550e8400-e29b-41d4-a716-446655440003', 'cancelled', 79.99),
    ('650e8400-e29b-41d4-a716-446655440007', 'order-7', '550e8400-e29b-41d4-a716-446655440004', 'pending', 59.99),
    ('650e8400-e29b-41d4-a716-446655440008', 'order-8', '550e8400-e29b-41d4-a716-446655440004', 'completed', 129.99),
    ('650e8400-e29b-41d4-a716-446655440009', 'order-9', '550e8400-e29b-41d4-a716-446655440005', 'completed', 89.99),
    ('650e8400-e29b-41d4-a716-446655440010', 'order-10', '550e8400-e29b-41d4-a716-446655440005', 'pending', 169.99);

INSERT INTO tb_order_item (order_id, product_id, quantity, price) VALUES
    ('650e8400-e29b-41d4-a716-446655440001', '750e8400-e29b-41d4-a716-446655440001', 1, 99.99),
    ('650e8400-e29b-41d4-a716-446655440002', '750e8400-e29b-41d4-a716-446655440002', 2, 74.99),
    ('650e8400-e29b-41d4-a716-446655440003', '750e8400-e29b-41d4-a716-446655440001', 1, 49.99),
    ('650e8400-e29b-41d4-a716-446655440004', '750e8400-e29b-41d4-a716-446655440003', 1, 199.99),
    ('650e8400-e29b-41d4-a716-446655440005', '750e8400-e29b-41d4-a716-446655440002', 3, 99.99),
    ('650e8400-e29b-41d4-a716-446655440006', '750e8400-e29b-41d4-a716-446655440001', 1, 79.99),
    ('650e8400-e29b-41d4-a716-446655440007', '750e8400-e29b-41d4-a716-446655440003', 1, 59.99),
    ('650e8400-e29b-41d4-a716-446655440008', '750e8400-e29b-41d4-a716-446655440001', 2, 64.99),
    ('650e8400-e29b-41d4-a716-446655440009', '750e8400-e29b-41d4-a716-446655440002', 1, 89.99),
    ('650e8400-e29b-41d4-a716-446655440010', '750e8400-e29b-41d4-a716-446655440003', 1, 169.99);
