-- Users database schema and seed data with trinity pattern

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Trinity pattern: pk_{entity} (surrogate) + id (federation key) + identifier (semantic)
CREATE TABLE tb_user (
    pk_user BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    id UUID NOT NULL UNIQUE DEFAULT uuid_generate_v4(),
    identifier TEXT NOT NULL UNIQUE,  -- Semantic identifier (email)
    email TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Federation view - exposes federation key and necessary fields
CREATE VIEW v_user AS
    SELECT
        id,
        email,
        name,
        identifier,
        created_at
    FROM tb_user;

-- Seed data with explicit UUIDs for test reproducibility
INSERT INTO tb_user (id, identifier, email, name) VALUES
    ('550e8400-e29b-41d4-a716-446655440001', 'alice@example.com', 'alice@example.com', 'Alice'),
    ('550e8400-e29b-41d4-a716-446655440002', 'bob@example.com', 'bob@example.com', 'Bob'),
    ('550e8400-e29b-41d4-a716-446655440003', 'charlie@example.com', 'charlie@example.com', 'Charlie'),
    ('550e8400-e29b-41d4-a716-446655440004', 'diana@example.com', 'diana@example.com', 'Diana'),
    ('550e8400-e29b-41d4-a716-446655440005', 'eve@example.com', 'eve@example.com', 'Eve'),
    ('550e8400-e29b-41d4-a716-446655440006', 'frank@example.com', 'frank@example.com', 'Frank'),
    ('550e8400-e29b-41d4-a716-446655440007', 'grace@example.com', 'grace@example.com', 'Grace'),
    ('550e8400-e29b-41d4-a716-446655440008', 'henry@example.com', 'henry@example.com', 'Henry'),
    ('550e8400-e29b-41d4-a716-446655440009', 'iris@example.com', 'iris@example.com', 'Iris'),
    ('550e8400-e29b-41d4-a716-446655440010', 'jack@example.com', 'jack@example.com', 'Jack');
