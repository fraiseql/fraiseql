# Common Schema Components

This directory contains shared database objects used across the entire schema.

## Contents

### 000_security/
Security-related extensions and configurations.

### 001_types/
- `0010_enums.sql` - Common enumerations (order_status, payment_status, address_type)
- `0011_schemas.sql` - PostgreSQL schema definitions (app, core)
- `0012_mutation_utils.sql` - Ultra-direct mutation response utilities

## Purpose

These files establish the foundation types, schemas, and utilities that other parts of the schema depend on. They should be applied first during database initialization.
