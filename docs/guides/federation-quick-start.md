<!-- Skip to main content -->
---
title: Federation Quick Start (5 Minutes)
description: Get a basic federation running in 5 minutes.
keywords: ["debugging", "implementation", "best-practices", "deployment", "tutorial"]
tags: ["documentation", "reference"]
---

# Federation Quick Start (5 Minutes)

**Status:** ✅ Production Ready
**Audience:** Developers, Architects
**Reading Time:** 5-7 minutes
**Last Updated:** 2026-02-05

Get a basic federation running in 5 minutes.

## Prerequisites

- Two FraiseQL instances
- PostgreSQL databases for each
- Apollo Router installed

## Step 1: Create Users Subgraph (1 minute)

```python
<!-- Code example in Python -->
# users_service/schema.py
from FraiseQL import type, key

@type
@key("id")
class User:
    id: str
    name: str
    email: str
```text
<!-- Code example in TEXT -->

```bash
<!-- Code example in BASH -->
# Generate schema
FraiseQL generate --language python

# Deploy
FraiseQL run --port 8001
```text
<!-- Code example in TEXT -->

---

## Step 2: Create Orders Subgraph (1 minute)

```python
<!-- Code example in Python -->
# orders_service/schema.py
from FraiseQL import type, key, extends, external

@type
@extends
@key("id")
class User:
    id: str = external()

@type
@key("id")
class Order:
    id: str
    user_id: str
    total: float
    user: User  # Reference to User from other subgraph
```text
<!-- Code example in TEXT -->

```bash
<!-- Code example in BASH -->
FraiseQL run --port 8002
```text
<!-- Code example in TEXT -->

---

## Step 3: Set Up Apollo Router (2 minutes)

```bash
<!-- Code example in BASH -->
# Install Apollo Router
curl -sSL https://install.apollographql.com | sh

# Create configuration
cat > supergraph.yaml << 'EOF'
federation_version: 2

subgraphs:
  users:
    routing_url: http://localhost:8001/graphql
    schema:
      file: users_schema.graphql

  orders:
    routing_url: http://localhost:8002/graphql
    schema:
      file: orders_schema.graphql
EOF

# Start router
rover supergraph compose --config supergraph.yaml > supergraph.graphql
apollo-router --config router.yaml
```text
<!-- Code example in TEXT -->

---

## Step 4: Test Federation (1 minute)

```bash
<!-- Code example in BASH -->
# Query through federation gateway
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ user(id: \"1\") { id name orders { id total } } }"
  }'
```text
<!-- Code example in TEXT -->

Expected response:

```json
<!-- Code example in JSON -->
{
  "data": {
    "user": {
      "id": "1",
      "name": "Alice",
      "orders": [
        {"id": "1", "total": 100.50}
      ]
    }
  }
}
```text
<!-- Code example in TEXT -->

---

## That's It

You now have a working federation! 🎉

### Next Steps

- Add more subgraphs (follow same pattern)
- Add mutations (see federation guide)
- Deploy to production (see deployment guide)
- Monitor with observability (see monitoring guide)

### Common Issues

**"Can't connect to subgraph"**
→ Check both services running: `curl http://localhost:8001/graphql`

**"Entity not found"**
→ Verify `@key` directive matches between services

**"Schema composition failed"**
→ Check schemas in `supergraph.graphql` for conflicts

See [Federation Guide](../integrations/federation/guide.md) for complete documentation.
