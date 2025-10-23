-- Common types and enums for E-commerce API

-- Order status enum
CREATE TYPE order_status AS ENUM (
    'pending',
    'confirmed',
    'processing',
    'shipped',
    'delivered',
    'cancelled',
    'refunded'
);

-- Payment status enum
CREATE TYPE payment_status AS ENUM (
    'pending',
    'processing',
    'completed',
    'failed',
    'refunded'
);

-- Address type enum
CREATE TYPE address_type AS ENUM (
    'billing',
    'shipping',
    'both'
);

-- Sequence for order numbers
CREATE SEQUENCE order_seq START 1;
