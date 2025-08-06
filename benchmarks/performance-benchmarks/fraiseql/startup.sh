#!/bin/bash
# Startup script for FraiseQL benchmark app

echo "Waiting for PostgreSQL to be ready..."
until pg_isready -h postgres-bench -p 5432 -U benchmark; do
  echo "PostgreSQL is unavailable - sleeping"
  sleep 2
done

echo "PostgreSQL is ready!"

# Set PostgreSQL password
export PGPASSWORD=benchmark

# Check if views exist
echo "Checking if FraiseQL views exist..."
VIEW_COUNT=$(psql -h postgres-bench -U benchmark -d benchmark_db -t -c "SELECT COUNT(*) FROM information_schema.views WHERE table_schema = 'benchmark' AND table_name LIKE 'v_%'")

if [ "$VIEW_COUNT" -eq "0" ]; then
  echo "Views not found. The database should be initialized with the proper SQL scripts."
else
  echo "Found $VIEW_COUNT views in benchmark schema."
fi

echo "Starting ULTRA-optimized FraiseQL with multi-tier connection pools..."
# Use ultra-optimized app with all performance optimizations
exec uvicorn ultra_optimized_app:app --host 0.0.0.0 --port 8000 --workers 1 --loop asyncio --http httptools --no-access-log --no-server-header --no-date-header
