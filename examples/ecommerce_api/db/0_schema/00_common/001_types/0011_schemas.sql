-- PostgreSQL schemas for CQRS separation
-- Following FraiseQL v1 patterns with app/core separation

-- Core schema: Business logic, data operations
CREATE SCHEMA IF NOT EXISTS core;

-- App schema: API layer, input validation, response formatting
CREATE SCHEMA IF NOT EXISTS app;

-- Grant permissions
GRANT USAGE ON SCHEMA core TO ecommerce_user;
GRANT USAGE ON SCHEMA app TO ecommerce_user;

-- Set search path for functions
-- ALTER DATABASE ecommerce_db SET search_path TO public, app, core;
