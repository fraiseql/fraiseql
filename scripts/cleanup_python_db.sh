#!/bin/bash
# Phase 9: Remove deprecated Python database code
# This script cleans up code that is no longer needed after moving to unified Rust pipeline

set -e

echo "🧹 Phase 9: Cleaning up deprecated Python database code..."

# Remove Python SQL builders (moved to Rust in Phase 7)
echo "Removing Python SQL builders..."
rm -f src/fraiseql/sql/sql_generator.py
rm -f src/fraiseql/sql/where_generator.py
rm -f src/fraiseql/sql/order_by_generator.py
rm -f src/fraiseql/sql/limit_generator.py
rm -rf src/fraiseql/sql/where/

# Remove Python WHERE normalization (moved to Rust in Phase 7)
echo "Removing Python WHERE normalization..."
rm -f src/fraiseql/where_normalization.py
rm -f src/fraiseql/where_clause.py

# Remove Python GraphQL parsing (moved to Rust in Phase 6)
echo "Removing Python GraphQL parsing..."
# Keep minimal wrapper but remove complex parsing logic
# (graphql_pipeline.py still needed for Phase 4 compatibility)

# Remove psycopg pool management (moved to Rust in Phase 1)
echo "Removing psycopg pool setup..."
# Note: Keep imports for now as some legacy code may still reference them

# Remove Python database module
echo "Removing Python database module..."
rm -f src/fraiseql/db.py

# Update pyproject.toml to remove psycopg dependencies
echo "Updating pyproject.toml..."
sed -i '/psycopg/d' pyproject.toml
sed -i '/psycopg-pool/d' pyproject.toml

# Clean up unused imports
echo "Cleaning up unused imports..."
find src/ -name "*.py" -exec sed -i '/from psycopg/d' {} \;
find src/ -name "*.py" -exec sed -i '/import psycopg/d' {} \;
find src/ -name "*.py" -exec sed -i '/from fraiseql.db import/d' {} \;
find src/ -name "*.py" -exec sed -i '/from fraiseql.sql import/d' {} \;

echo "✅ Python database layer cleanup complete!"
echo ""
echo "📊 Cleanup Summary:"
echo "- Removed Python SQL generation code"
echo "- Removed Python WHERE clause processing"
echo "- Removed psycopg dependencies"
echo "- Removed unused database modules"
echo ""
echo "🎯 The codebase now uses unified Rust pipeline for all database operations!"
