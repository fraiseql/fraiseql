-- Trinity Identifiers for Users Table
--
-- This is a concrete example showing how to apply Trinity pattern to a users table.
-- Assumes existing table structure:
--   users (
--     id UUID PRIMARY KEY (or similar),
--     username TEXT UNIQUE,
--     email TEXT,
--     ...
--   )

BEGIN;

-- ============================================================================
-- STEP 1: Add Trinity columns to existing users table
-- ============================================================================

-- Add IDENTITY primary key (modern PostgreSQL 10+ syntax)
-- Using GENERATED ALWAYS AS IDENTITY instead of deprecated SERIAL
ALTER TABLE users
    ADD COLUMN pk_user INT GENERATED ALWAYS AS IDENTITY;

-- Add new UUID column (separate from existing id)
-- Note: Rename based on your existing schema
-- Option A: If you already have 'id UUID', rename it to 'legacy_id' first
-- Option B: Use new name like 'uuid_id' initially
ALTER TABLE users
    ADD COLUMN uuid_id UUID;

-- Add human-readable identifier (username-based)
ALTER TABLE users
    ADD COLUMN identifier TEXT;

-- ============================================================================
-- STEP 2: Backfill data
-- ============================================================================

-- If you already have UUIDs in 'id' column, copy them
-- UPDATE users SET uuid_id = id WHERE uuid_id IS NULL;

-- Otherwise, generate new UUIDs
UPDATE users
SET uuid_id = gen_random_uuid()
WHERE uuid_id IS NULL;

-- Use username as identifier (lowercased for consistency)
UPDATE users
SET identifier = LOWER(username)
WHERE identifier IS NULL
  AND username IS NOT NULL;

-- ============================================================================
-- STEP 3: Handle schema transition
-- ============================================================================

-- Option A: If renaming existing 'id' column
-- This is the cleanest approach but requires downtime or careful planning
--
-- ALTER TABLE users RENAME COLUMN id TO legacy_id;
-- ALTER TABLE users RENAME COLUMN uuid_id TO id;

-- Option B: Keep both during transition (recommended)
-- You can drop 'legacy_id' later after verification

-- ============================================================================
-- STEP 4: Add constraints and indexes
-- ============================================================================

-- Make pk_user the primary key
-- First drop existing primary key if needed
-- ALTER TABLE users DROP CONSTRAINT users_pkey CASCADE;

ALTER TABLE users
    ADD CONSTRAINT users_pkey PRIMARY KEY (pk_user);

-- Add UUID constraints
ALTER TABLE users
    ALTER COLUMN uuid_id SET NOT NULL;

ALTER TABLE users
    ADD CONSTRAINT users_uuid_id_unique UNIQUE (uuid_id);

-- Add identifier constraints
-- Note: identifier can be NULL for users without username
ALTER TABLE users
    ADD CONSTRAINT users_identifier_unique UNIQUE (identifier)
    WHERE identifier IS NOT NULL;  -- Partial unique index

-- Create indexes
CREATE INDEX idx_users_uuid_id ON users(uuid_id);
CREATE INDEX idx_users_identifier ON users(identifier) WHERE identifier IS NOT NULL;

-- ============================================================================
-- STEP 5: Documentation
-- ============================================================================

COMMENT ON COLUMN users.pk_user IS
    'Internal SERIAL primary key for fast joins. Not exposed in GraphQL API.';

COMMENT ON COLUMN users.uuid_id IS
    'Public UUID identifier returned by GraphQL API. Renamed to "id" in API layer.';

COMMENT ON COLUMN users.identifier IS
    'Human-readable username for URL slugs (e.g., /users/@johndoe). Nullable.';

COMMIT;

-- ============================================================================
-- Post-Migration: Update Foreign Keys
-- ============================================================================

-- After this migration, update dependent tables to use pk_user instead of UUID
-- Example for a posts table:
--
-- ALTER TABLE posts ADD COLUMN pk_user INT REFERENCES users(pk_user);
-- UPDATE posts p SET pk_user = u.pk_user FROM users u WHERE p.user_id = u.uuid_id;
-- ALTER TABLE posts ALTER COLUMN pk_user SET NOT NULL;
-- CREATE INDEX idx_posts_pk_user ON posts(pk_user);
--
-- Later, after verification:
-- ALTER TABLE posts DROP COLUMN user_id;  -- Drop old UUID foreign key

-- ============================================================================
-- Verification Queries
-- ============================================================================

-- Check data integrity
-- SELECT COUNT(*) FROM users WHERE pk_user IS NULL;        -- Should be 0
-- SELECT COUNT(*) FROM users WHERE uuid_id IS NULL;        -- Should be 0
-- SELECT COUNT(*) FROM users WHERE identifier IS NULL;     -- May be non-zero if not all users have usernames

-- Sample data
-- SELECT pk_user, uuid_id, identifier, username, email FROM users LIMIT 5;

-- Check identifier uniqueness
-- SELECT identifier, COUNT(*) FROM users WHERE identifier IS NOT NULL GROUP BY identifier HAVING COUNT(*) > 1;
