# Blog API Database - CQRS Architecture

This directory contains the database schema for the Blog API using a CQRS (Command Query Responsibility Segregation) architecture.

## Structure

- **migrations/**: Database migration scripts
  - `001_initial_schema.sql`: Creates write-side tables (tb_* prefix)
  - `002_functions.sql`: SQL functions for mutations
  - `003_views.sql`: Read-side views (v_* prefix) with JSONB data

## Architecture Overview

### Write Side
- Tables prefixed with `tb_` store normalized data
- SQL functions handle all write operations (INSERT, UPDATE, DELETE)
- Functions accept JSON parameters for flexibility

### Read Side
- Views prefixed with `v_` provide denormalized data
- Each view has a `data` JSONB column containing all entity fields
- Views automatically include `__typename` for GraphQL type resolution

## Usage

1. Run migrations in order:
   ```bash
   psql -d blog_db -f migrations/001_initial_schema.sql
   psql -d blog_db -f migrations/002_functions.sql
   psql -d blog_db -f migrations/003_views.sql
   ```

2. The application will:
   - Call SQL functions for all mutations
   - Query views for all read operations
   - Automatically handle JSONB field extraction
