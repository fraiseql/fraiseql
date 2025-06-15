-- FraiseQL Database Initialization Script
-- This script sets up the database for FraiseQL benchmarking

-- Enable extensions in public schema first
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Create benchmark schema if not exists
CREATE SCHEMA IF NOT EXISTS benchmark;

-- Set search path
SET search_path TO benchmark, public;

-- Load base schema
\i /docker-entrypoint-initdb.d/schema.sql

-- Load seed data
\i /docker-entrypoint-initdb.d/seed-data.sql

-- Load FraiseQL-specific tables, views and functions
\i /docker-entrypoint-initdb.d/fraiseql-tables-and-views.sql
\i /docker-entrypoint-initdb.d/fraiseql-functions.sql

-- Grant permissions
GRANT USAGE ON SCHEMA benchmark TO benchmark;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA benchmark TO benchmark;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA benchmark TO benchmark;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA benchmark TO benchmark;
