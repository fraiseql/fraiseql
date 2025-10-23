#!/bin/bash

# LTREE Performance Benchmark Runner
# Sets up database, runs benchmark, and generates report

set -e

echo "🚀 LTREE Performance Benchmark"
echo "================================"

# Configuration
DB_NAME="fraiseql_test"
DB_USER="postgres"
DB_PASSWORD="password"
DB_HOST="localhost"
DB_PORT="5432"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if PostgreSQL is running
if ! pg_isready -h $DB_HOST -p $DB_PORT -U $DB_USER >/dev/null 2>&1; then
    echo -e "${RED}❌ PostgreSQL is not running on $DB_HOST:$DB_PORT${NC}"
    echo "Please start PostgreSQL and ensure the test database exists."
    exit 1
fi

echo "📊 Setting up benchmark database..."

# Create database if it doesn't exist
psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d postgres -c "CREATE DATABASE $DB_NAME;" 2>/dev/null || true

# Run setup SQL
psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -f 00_setup.sql

echo -e "${GREEN}✅ Database setup complete${NC}"

# Check data was inserted
ROW_COUNT=$(psql -h $DB_HOST -p $DB_PORT -U $DB_USER -d $DB_NAME -t -c "SELECT count(*) FROM ltree_benchmark;")
echo "📈 Dataset size: $ROW_COUNT rows"

# Run the benchmark
echo "🏃 Running performance benchmark..."
python ltree_benchmark.py

echo -e "${GREEN}✅ Benchmark complete!${NC}"
echo "📄 Check the results directory for detailed performance metrics."
