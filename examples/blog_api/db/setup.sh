#!/bin/bash

# Blog API Database Setup Script
# This script initializes the CQRS database structure

DB_NAME="${DB_NAME:-blog_db}"
DB_USER="${DB_USER:-postgres}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"

echo "Setting up Blog API database with CQRS architecture..."
echo "Database: $DB_NAME"
echo "Host: $DB_HOST:$DB_PORT"
echo "User: $DB_USER"
echo ""

# Function to run SQL file
run_sql() {
    local file=$1
    local description=$2
    echo "Running $description..."
    psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f "$file"
    if [ $? -eq 0 ]; then
        echo "✓ $description completed"
    else
        echo "✗ $description failed"
        exit 1
    fi
    echo ""
}

# Create database if it doesn't exist
echo "Creating database if not exists..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -tc "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'" | grep -q 1 || \
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "CREATE DATABASE $DB_NAME"

# Run migrations in order
run_sql "migrations/001_initial_schema.sql" "Initial schema (write-side tables)"
run_sql "migrations/002_functions.sql" "SQL functions for mutations"
run_sql "migrations/003_views.sql" "Read-side views"

echo "✅ Database setup complete!"
echo ""
echo "Next steps:"
echo "1. Update your .env file with DATABASE_URL=postgresql://$DB_USER@$DB_HOST:$DB_PORT/$DB_NAME"
echo "2. Run the application: python app.py"
