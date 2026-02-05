<!-- Skip to main content -->
---
title: Migration Guide: Single File to Domain Organization
description: This guide walks you through migrating from a monolithic `schema.json` to domain-driven organization.
keywords: []
tags: ["documentation", "reference"]
---

# Migration Guide: Single File to Domain Organization

This guide walks you through migrating from a monolithic `schema.json` to domain-driven organization.

## Overview

**Before**:

```text
<!-- Code example in TEXT -->
schema.json (all types, queries, mutations in one file)
FraiseQL.toml
```text
<!-- Code example in TEXT -->

**After**:

```text
<!-- Code example in TEXT -->
schema/
├── {domain1}/
│   └── types.json
├── {domain2}/
│   └── types.json
└── {domain3}/
    └── types.json
FraiseQL.toml (updated with domain discovery)
```text
<!-- Code example in TEXT -->

## Step-by-Step Migration

### Step 1: Plan Your Domains

Analyze your schema and identify 3-10 domain boundaries.

Examples:

- auth: User, Session, Role
- products: Product, Category, Inventory
- orders: Order, OrderItem, Shipment

### Step 2: Create Directory Structure

```bash
<!-- Code example in BASH -->
mkdir -p schema/{auth,products,orders}
```text
<!-- Code example in TEXT -->

### Step 3: Split Types.json Into Domains

For each domain:

1. Extract types, queries, mutations
2. Create `schema/{domain}/types.json`
3. Validate JSON with `jq . schema/{domain}/types.json`

### Step 4: Update FraiseQL.toml

Replace includes with domain discovery:

```toml
<!-- Code example in TOML -->
[domain_discovery]
enabled = true
root_dir = "schema"
```text
<!-- Code example in TEXT -->

### Step 5: Compile and Validate

```bash
<!-- Code example in BASH -->
FraiseQL compile FraiseQL.toml
FraiseQL compile FraiseQL.toml --check
```text
<!-- Code example in TEXT -->

### Step 6: Compare Output

Verify type/query counts match original schema.

### Step 7: Commit

```bash
<!-- Code example in BASH -->
git add schema/ FraiseQL.toml
git commit -m "refactor: migrate to domain-based organization"
```text
<!-- Code example in TEXT -->

## Validation

```bash
<!-- Code example in BASH -->
# Check type count
jq '.types | length' schema.compiled.json

# Check all query types are defined
jq '.queries[] | .return_type' schema.compiled.json | sort | uniq > query_types.txt
jq '.types[] | .name' schema.compiled.json | sort | uniq > defined_types.txt
comm -23 query_types.txt defined_types.txt  # Should be empty
```text
<!-- Code example in TEXT -->

## Rollback

```bash
<!-- Code example in BASH -->
cp schema.json.bak schema.json
cp FraiseQL.toml.bak FraiseQL.toml
FraiseQL compile FraiseQL.toml
```text
<!-- Code example in TEXT -->

## Estimated Time

- Small (10-50 types): 30 minutes
- Medium (50-200 types): 1-2 hours
- Large (200+ types): 2-4 hours

See [DOMAIN_ORGANIZATION.md](DOMAIN_ORGANIZATION.md) for detailed best practices.
