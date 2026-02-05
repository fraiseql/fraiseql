-- Products database schema and seed data with trinity pattern

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Trinity pattern for products
CREATE TABLE tb_product (
    pk_product BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    id UUID NOT NULL UNIQUE DEFAULT uuid_generate_v4(),
    identifier TEXT NOT NULL UNIQUE,  -- Semantic identifier (SKU-like)
    name TEXT NOT NULL,
    description TEXT,
    price DECIMAL(10, 2) NOT NULL,
    stock INT NOT NULL DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Federation view for products
CREATE VIEW v_product AS
    SELECT
        id,
        identifier,
        name,
        description,
        price,
        stock,
        created_at
    FROM tb_product;

-- Seed data with explicit UUIDs
INSERT INTO tb_product (id, identifier, name, description, price, stock) VALUES
    ('750e8400-e29b-41d4-a716-446655440001', 'LAPTOP-001', 'Laptop', 'High-performance laptop', 999.99, 15),
    ('750e8400-e29b-41d4-a716-446655440002', 'MONITOR-001', 'Monitor', 'HD Display Monitor', 249.99, 30),
    ('750e8400-e29b-41d4-a716-446655440003', 'KEYBOARD-001', 'Keyboard', 'Mechanical Keyboard', 149.99, 50),
    ('750e8400-e29b-41d4-a716-446655440004', 'MOUSE-001', 'Mouse', 'Wireless Mouse', 49.99, 100),
    ('750e8400-e29b-41d4-a716-446655440005', 'HEADPHONES-001', 'Headphones', 'Noise-cancelling Headphones', 199.99, 25),
    ('750e8400-e29b-41d4-a716-446655440006', 'WEBCAM-001', 'Webcam', 'HD Webcam', 99.99, 40),
    ('750e8400-e29b-41d4-a716-446655440007', 'MICROPHONE-001', 'Microphone', 'USB Microphone', 79.99, 20),
    ('750e8400-e29b-41d4-a716-446655440008', 'USBHUB-001', 'USB Hub', 'Multi-port USB Hub', 29.99, 60),
    ('750e8400-e29b-41d4-a716-446655440009', 'POWERBANK-001', 'Power Bank', 'Portable Power Bank', 39.99, 80),
    ('750e8400-e29b-41d4-a716-446655440010', 'USbcable-001', 'USB Cable', 'High-speed USB Cable', 9.99, 200);
