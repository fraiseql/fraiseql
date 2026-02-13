#!/bin/bash

# FraiseQL Design Quality Performance Testing
# Measures design analysis performance across different schema sizes

set -e

echo "==================================================================="
echo "FraiseQL Design Quality Performance Testing"
echo "==================================================================="
echo ""

# Test 1: CLI Performance - Minimal Schema
echo "[1/5] Testing CLI performance on minimal schema..."
time fraiseql lint <(cat << 'SCHEMA'
{
  "types": [
    {"name": "User", "fields": [{"name": "id", "type": "ID", "isPrimaryKey": true}]}
  ]
}
SCHEMA
) --json > /dev/null 2>&1 || true

# Test 2: CLI Performance - Typical Schema
echo ""
echo "[2/5] Testing CLI performance on typical schema..."
time fraiseql lint <(cat << 'SCHEMA'
{
  "subgraphs": [
    {"name": "users", "entities": ["User"]},
    {"name": "posts", "entities": ["Post"], "references": [{"type": "User"}]},
    {"name": "comments", "entities": ["Comment"], "references": [{"type": "User"}, {"type": "Post"}]}
  ],
  "types": [
    {"name": "User", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true},
      {"name": "name", "type": "String"},
      {"name": "email", "type": "String"}
    ]},
    {"name": "Post", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true},
      {"name": "title", "type": "String"},
      {"name": "content", "type": "String"}
    ]},
    {"name": "Comment", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true},
      {"name": "text", "type": "String"}
    ]}
  ]
}
SCHEMA
) --json > /dev/null 2>&1 || true

# Test 3: Rust lib performance
echo ""
echo "[3/5] Testing Rust library performance..."
cargo test --lib design:: --release 2>&1 | grep "test result:"

# Test 4: CLI performance with filters
echo ""
echo "[4/5] Testing CLI with filter flags..."
time fraiseql lint <(cat << 'SCHEMA'
{
  "subgraphs": [
    {"name": "a", "entities": ["User"]},
    {"name": "b", "entities": ["User", "Post"]}
  ]
}
SCHEMA
) --federation --cost --json > /dev/null 2>&1 || true

# Test 5: JSON output performance
echo ""
echo "[5/5] Testing JSON output performance..."
time fraiseql lint <(cat << 'SCHEMA'
{
  "types": [
    {"name": "Query", "fields": [{"name": "user", "type": "User"}]},
    {"name": "User", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true},
      {"name": "posts", "type": "[Post!]"}
    ]},
    {"name": "Post", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true},
      {"name": "comments", "type": "[Comment!]"}
    ]},
    {"name": "Comment", "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true}
    ]}
  ]
}
SCHEMA
) --json > /dev/null 2>&1 || true

echo ""
echo "==================================================================="
echo "Performance testing complete!"
echo "==================================================================="
