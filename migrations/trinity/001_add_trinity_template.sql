-- Trinity Identifiers Migration Template
--
-- This is a template for adding Trinity identifiers to an existing entity.
-- Replace 'entity_name' with your actual entity (e.g., 'users', 'posts', etc.)
--
-- Trinity Pattern:
--   pk_entity  INT GENERATED ALWAYS AS IDENTITY  - Internal primary key (fast joins)
--   id         UUID    - Public API (secure)
--   identifier TEXT    - Human-readable slug (optional)

BEGIN;

-- ============================================================================
-- STEP 1: Add Trinity columns
-- ============================================================================

-- Add IDENTITY primary key column (modern PostgreSQL syntax)
-- Using GENERATED ALWAYS AS IDENTITY instead of deprecated SERIAL
ALTER TABLE entity_name
    ADD COLUMN pk_entity INT GENERATED ALWAYS AS IDENTITY;

-- Add UUID column for public API (initially nullable for backfill)
ALTER TABLE entity_name
    ADD COLUMN id UUID;

-- Add identifier column for human-readable URLs (optional - remove if not needed)
ALTER TABLE entity_name
    ADD COLUMN identifier TEXT;

-- ============================================================================
-- STEP 2: Backfill existing data
-- ============================================================================

-- Generate UUIDs for existing rows
UPDATE entity_name
SET id = gen_random_uuid()
WHERE id IS NULL;

-- Backfill identifier (customize based on your entity)
-- Example: Use username, slug, or other human-readable field
UPDATE entity_name
SET identifier = LOWER(some_unique_field)
WHERE identifier IS NULL;

-- ============================================================================
-- STEP 3: Add constraints
-- ============================================================================

-- Make pk_entity the primary key
-- Note: You may need to drop existing primary key first
-- ALTER TABLE entity_name DROP CONSTRAINT entity_name_pkey;
ALTER TABLE entity_name
    ADD PRIMARY KEY (pk_entity);

-- Make id NOT NULL and UNIQUE
ALTER TABLE entity_name
    ALTER COLUMN id SET NOT NULL,
    ADD CONSTRAINT entity_name_id_unique UNIQUE (id);

-- Make identifier UNIQUE (if using it)
ALTER TABLE entity_name
    ADD CONSTRAINT entity_name_identifier_unique UNIQUE (identifier);

-- ============================================================================
-- STEP 4: Create indexes for performance
-- ============================================================================

-- Index on UUID (for public API lookups)
CREATE INDEX idx_entity_name_id ON entity_name(id);

-- Index on identifier (for slug-based lookups)
CREATE INDEX idx_entity_name_identifier ON entity_name(identifier);

-- ============================================================================
-- STEP 5: Add comments for documentation
-- ============================================================================

COMMENT ON COLUMN entity_name.pk_entity IS
    'Internal IDENTITY primary key for fast joins (not exposed in public API)';

COMMENT ON COLUMN entity_name.id IS
    'Public UUID identifier (exposed in GraphQL API)';

COMMENT ON COLUMN entity_name.identifier IS
    'Human-readable identifier for URLs (e.g., username, slug)';

COMMIT;

-- ============================================================================
-- Verification Queries (run after migration)
-- ============================================================================

-- Check for NULL values (should be 0)
-- SELECT COUNT(*) FROM entity_name WHERE pk_entity IS NULL;
-- SELECT COUNT(*) FROM entity_name WHERE id IS NULL;

-- Check for duplicate identifiers (should be 0)
-- SELECT identifier, COUNT(*) FROM entity_name GROUP BY identifier HAVING COUNT(*) > 1;

-- Sample query using new pk_entity
-- SELECT pk_entity, id, identifier FROM entity_name LIMIT 5;
