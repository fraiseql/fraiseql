-- ============================================================================
-- Test Schema for FraiseQL Mutation Patterns
-- ============================================================================
-- This schema provides tables for testing all mutation patterns.
-- Load this before running any examples.
-- ============================================================================

-- Enable required extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ============================================================================
-- Core Tables
-- ============================================================================

CREATE TABLE users (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    email text UNIQUE NOT NULL,
    name text NOT NULL,
    age integer,
    password_hash text,
    status text DEFAULT 'active' CHECK (status IN ('active', 'inactive', 'suspended')),
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now()
);

CREATE TABLE posts (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    title text NOT NULL,
    content text,
    status text DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now()
);

CREATE TABLE comments (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id uuid REFERENCES posts(id) ON DELETE CASCADE,
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    content text NOT NULL,
    created_at timestamptz DEFAULT now()
);

CREATE TABLE tags (
    id uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    name text UNIQUE NOT NULL,
    color text DEFAULT '#666666'
);

CREATE TABLE post_tags (
    post_id uuid REFERENCES posts(id) ON DELETE CASCADE,
    tag_id uuid REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);

-- ============================================================================
-- Indexes for Performance
-- ============================================================================

CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_status ON users(status);
CREATE INDEX idx_posts_user_id ON posts(user_id);
CREATE INDEX idx_posts_status ON posts(status);
CREATE INDEX idx_comments_post_id ON comments(post_id);
CREATE INDEX idx_comments_user_id ON comments(user_id);

-- ============================================================================
-- Mutation Response Type
-- ============================================================================

CREATE TYPE mutation_response AS (
    status text,
    message text,
    entity_id text,
    entity_type text,
    entity jsonb,
    updated_fields text[],
    cascade jsonb,
    metadata jsonb
);

-- ============================================================================
-- Helper Functions
-- ============================================================================

-- Include the shared validation helpers.
--
-- `\ir`, not `\i`: `\i` resolves against the process's working directory, so the
-- include only ever landed for someone who happened to run psql from the repository
-- root — and since the documented command set no ON_ERROR_STOP, the miss printed one
-- error, kept going, and exited 0 with the helpers absent (#1051).
\ir ../../sql/helpers/mutation_validation.sql

-- ============================================================================
-- Sample Data
-- ============================================================================

INSERT INTO users (id, email, name, age) VALUES
    ('550e8400-e29b-41d4-a716-446655440000', 'john@example.com', 'John Doe', 30),
    ('550e8400-e29b-41d4-a716-446655440001', 'jane@example.com', 'Jane Smith', 25);

INSERT INTO posts (id, user_id, title, content, status) VALUES
    ('660e8400-e29b-41d4-a716-446655440000', '550e8400-e29b-41d4-a716-446655440000', 'First Post', 'Hello World!', 'published'),
    ('660e8400-e29b-41d4-a716-446655440001', '550e8400-e29b-41d4-a716-446655440001', 'Second Post', 'Another post', 'draft');

INSERT INTO tags (name, color) VALUES
    ('tutorial', '#4CAF50'),
    ('news', '#2196F3'),
    ('announcement', '#FF9800');

-- ============================================================================
-- Fixture for the patterns that need more than users/posts/comments (#1194)
-- ============================================================================
-- Four of the eighteen patterns could not be loaded against this file: three
-- wanted tables it never created, and one is a v2 cascade pattern needing the
-- shipped `fraiseql.*` builders and the 13-column `app.mutation_response`.
--
-- Note the two generations living side by side. The `mutation_response` above is
-- the 8-column composite the other patterns return. `app.mutation_response` below
-- is the 13-column v2 protocol type, and only `04-relationships/update-with-cascade`
-- uses it. CONTRIBUTING.md already records this family as lagging the v2 line;
-- this is what that looks like in practice.

-- --- 06-advanced/async-processing --------------------------------------------

CREATE TABLE jobs (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    job_type text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    result jsonb,
    error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    completed_at timestamptz
);

CREATE INDEX idx_jobs_status ON jobs (status);

-- --- 06-advanced/transaction-rollback ----------------------------------------

CREATE TABLE accounts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_name text NOT NULL,
    balance numeric(14,2) NOT NULL DEFAULT 0 CHECK (balance >= 0),
    daily_limit numeric(14,2) NOT NULL DEFAULT 1000.00,
    status text NOT NULL DEFAULT 'active',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE transfers (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    from_account_id uuid NOT NULL REFERENCES accounts(id),
    to_account_id uuid NOT NULL REFERENCES accounts(id),
    amount numeric(14,2) NOT NULL CHECK (amount > 0),
    status text NOT NULL DEFAULT 'completed',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_transfers_from ON transfers (from_account_id, created_at);

INSERT INTO accounts (id, owner_name, balance) VALUES
    ('550e8400-e29b-41d4-a716-446655440000', 'John Doe', 5000.00),
    ('660e8400-e29b-41d4-a716-446655440000', 'Jane Smith', 250.00);

-- --- 04-relationships/update-with-cascade ------------------------------------
-- The v2 cascade protocol: the `fraiseql.*` builders (what `fraiseql setup`
-- installs), the 13-column response type, and read views the builders can read
-- an entity from.

CREATE SCHEMA IF NOT EXISTS app;
CREATE SCHEMA IF NOT EXISTS graphql;

DO $$ BEGIN
    CREATE TYPE app.mutation_response AS (
        succeeded      boolean,
        state_changed  boolean,
        -- TEXT, not an enum: `fraiseql.mutation_ok`/`mutation_err` are declared
        -- `RETURNS TABLE(... error_class TEXT ...)`, and a function returning
        -- SETOF this type is rejected at runtime with `Returned type text does
        -- not match expected type ... in column 3` if the two disagree.
        error_class    text,
        status_detail  text,
        http_status    smallint,
        message        text,
        entity_id      uuid,
        entity_type    text,
        entity         jsonb,
        updated_fields text[],
        cascade        jsonb,
        error_detail   jsonb,
        metadata       jsonb);
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

-- `fraiseql setup` installs these; including them keeps the example loadable
-- without a separate CLI step.
\ir ../../sql/helpers/mutation_response.sql
\ir ../../sql/helpers/cascade.sql

CREATE TABLE categories (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL,
    status text NOT NULL DEFAULT 'active',
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE products (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    category_id uuid NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    name text NOT NULL,
    status text NOT NULL DEFAULT 'active',
    price numeric(10,2) NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_products_category ON products (category_id);

-- `security_invoker = true` is load-bearing, not decoration: `fraiseql.cascade_entity`
-- reads each entity through its view, so a default view — which runs as its owner —
-- would bypass base-table RLS and leak rows into the cascade that the caller cannot
-- see. The pattern file's own header says so.
CREATE VIEW v_category WITH (security_invoker = true) AS
SELECT
    id,
    jsonb_build_object(
        'id', id, 'name', name, 'status', status, 'updated_at', updated_at
    ) AS data
FROM categories;

CREATE VIEW v_product WITH (security_invoker = true) AS
SELECT
    id,
    jsonb_build_object(
        'id', id, 'name', name, 'status', status, 'price', price,
        'category_id', category_id, 'updated_at', updated_at
    ) AS data
FROM products;

INSERT INTO categories (id, name, status) VALUES
    ('660e8400-e29b-41d4-a716-446655440000', 'Electronics', 'active');

INSERT INTO products (id, category_id, name, price) VALUES
    ('770e8400-e29b-41d4-a716-446655440000', '660e8400-e29b-41d4-a716-446655440000', 'Laptop', 1299.00),
    ('770e8400-e29b-41d4-a716-446655440001', '660e8400-e29b-41d4-a716-446655440000', 'Headphones', 199.00);
