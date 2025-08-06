#!/bin/bash

# Test Database Setup Script
# Sets up a separate test database for running the blog API tests

DB_NAME="${TEST_DB_NAME:-blog_test}"
DB_USER="${DB_USER:-postgres}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"

echo "Setting up test database for Blog API..."
echo "Database: $DB_NAME"
echo "Host: $DB_HOST:$DB_PORT"
echo "User: $DB_USER"
echo ""

# Drop and recreate test database
echo "Dropping existing test database if exists..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "DROP DATABASE IF EXISTS $DB_NAME"

echo "Creating test database..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "CREATE DATABASE $DB_NAME"

# Run migrations
echo "Running migrations..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f ../db/migrations/001_initial_schema.sql
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f ../db/migrations/002_functions.sql
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f ../db/migrations/003_views.sql

echo ""
echo "✅ Test database setup complete!"
echo ""
echo "To run tests:"
echo "  cd .. && python -m pytest tests/"
echo ""
echo "Or with coverage:"
echo "  cd .. && python -m pytest tests/ --cov=. --cov-report=html"
