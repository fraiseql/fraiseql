-- ==============================================================================
-- COMMON: Extensions and Types
-- ==============================================================================
-- File: 00_schema/00_common.sql
-- Layer: Common (shared infrastructure)
-- Contains: Extensions, types, enums
-- ==============================================================================

-- Extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Enums (if needed for future expansion)
-- CREATE TYPE post_status AS ENUM ('draft', 'published', 'archived');
