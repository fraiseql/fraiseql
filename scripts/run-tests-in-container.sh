#!/bin/bash
# ABOUTME: Script to run tests inside container with PostgreSQL on socket
# ABOUTME: Configures test environment to use Unix socket connection

set -e

# Set database connection via Unix socket
export TEST_DATABASE_URL="postgresql://postgres@/fraiseql_test?host=/var/run/postgresql"

# Activate virtual environment
source .venv/bin/activate

# Run tests
echo "Running tests with socket connection..."
python -m pytest "$@"
