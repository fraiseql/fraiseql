-- Blog Demo Enterprise Database Extensions
-- Essential PostgreSQL extensions for the multi-tenant blog platform

-- Enable UUID generation
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Case-insensitive text type for emails and usernames
CREATE EXTENSION IF NOT EXISTS "citext";

-- Trigram matching for search functionality
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- Unaccent for slug generation and search
CREATE EXTENSION IF NOT EXISTS "unaccent";

-- Row Level Security (RLS) for multi-tenancy
-- Note: RLS is built into PostgreSQL, no extension needed

-- Set timezone for application
SET timezone = 'UTC';