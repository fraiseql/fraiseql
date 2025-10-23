#!/bin/bash

set -e

DB_NAME="quickstart_test_$(date +%s)"

createdb "$DB_NAME"

psql "$DB_NAME" < examples/quickstart_5min_schema.sql

DATABASE_URL="postgresql://localhost/$DB_NAME" python examples/quickstart_5min.py &

SERVER_PID=$!

sleep 5

RESPONSE=$(curl -s -X POST http://localhost:8000/graphql -H "Content-Type: application/json" -d '{"query": "{ notes { id title } }"}')

if echo "$RESPONSE" | grep -q "Welcome to FraiseQL"; then

    echo "Test passed!"

    kill $SERVER_PID

    dropdb "$DB_NAME"

    exit 0

else

    echo "Test failed: $RESPONSE"

    kill $SERVER_PID

    dropdb "$DB_NAME"

    exit 1

fi
