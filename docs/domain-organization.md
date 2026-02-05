<!-- Skip to main content -->
---
title: Domain-Driven Schema Organization
description: FraiseQL v2 supports organizing GraphQL schemas across multiple **domains** - a pattern that mirrors how modern applications organize their code.
keywords: ["schema"]
tags: ["documentation", "reference"]
---

# Domain-Driven Schema Organization

FraiseQL v2 supports organizing GraphQL schemas across multiple **domains** - a pattern that mirrors how modern applications organize their code.

## What is a Domain?

A domain is a cohesive unit of business logic with its own types, queries, and mutations. Each domain:

- Lives in its own directory: `schema/{domain_name}/`
- Owns its types and queries
- Can depend on types from other domains
- Is independently testable and deployable

## Why Domain Organization?

### ✅ Mirrors Code Structure

Your GraphQL schema organization matches your application code:

```text
<!-- Code example in TEXT -->
src/
├── auth/          ← Domain
│   └── schema.py
├── products/      ← Domain
│   └── schema.py
└── orders/        ← Domain
    └── schema.py

Matches:

schema/
├── auth/          ← Domain
│   └── types.json
├── products/      ← Domain
│   └── types.json
└── orders/        ← Domain
    └── types.json
```text
<!-- Code example in TEXT -->

### ✅ Scalable

Start with monolithic, grow to many domains:

```text
<!-- Code example in TEXT -->
Small Project          Medium Project         Large Project
schema.json        →   schema/                schema/
                       ├── auth/              ├── auth/
                       ├── products/          ├── products/
                       └── orders/            ├── orders/
                                              ├── inventory/
                                              ├── payments/
                                              ├── shipping/
                                              └── analytics/
```text
<!-- Code example in TEXT -->

### ✅ Team-Friendly

Different teams can own different domains:

- **Auth Team** → `schema/auth/`
- **Products Team** → `schema/products/`
- **Order Fulfillment Team** → `schema/orders/`

### ✅ Independent Development

Teams work independently with clear contracts (types each domain exports).

### ✅ Easy to Navigate

"Where is the User type?" → Look in `schema/auth/types.json`

## Directory Structure

Each domain follows a consistent structure:

```text
<!-- Code example in TEXT -->
schema/
└── {domain_name}/
    ├── types.json          # Types, queries, mutations
    ├── queries.json        # (Optional) Query-only file
    ├── mutations.json      # (Optional) Mutation-only file
    └── README.md           # (Optional) Domain documentation
```text
<!-- Code example in TEXT -->

### Simple: Single File per Domain

```text
<!-- Code example in TEXT -->
schema/auth/types.json
{
  "types": [...],
  "queries": [...],
  "mutations": [...]
}
```text
<!-- Code example in TEXT -->

### Advanced: Separate Files

```text
<!-- Code example in TEXT -->
schema/auth/
├── types.json      # Type definitions only
├── queries.json    # Auth queries
└── mutations.json  # Login, logout, register
```text
<!-- Code example in TEXT -->

## Configuration

### Option 1: Auto-Discovery (Recommended)

Automatically discover all domains in a directory:

```toml
<!-- Code example in TOML -->
[domain_discovery]
enabled = true
root_dir = "schema"
```text
<!-- Code example in TEXT -->

Then compile:

```bash
<!-- Code example in BASH -->
FraiseQL compile FraiseQL.toml
```text
<!-- Code example in TEXT -->

The compiler will:

1. Find all subdirectories in `schema/`
2. Load `types.json`, `queries.json`, `mutations.json` from each
3. Merge everything into one compiled schema
4. Validate cross-domain references

### Option 2: Explicit List

If you need fine-grained control:

```toml
<!-- Code example in TOML -->
[includes]
types = [
  "schema/auth/types.json",
  "schema/products/types.json",
  "schema/orders/types.json"
]
queries = [
  "schema/auth/queries.json",
  "schema/products/queries.json"
]
mutations = [
  "schema/auth/mutations.json",
  "schema/orders/mutations.json"
]
```text
<!-- Code example in TEXT -->

## Example: E-Commerce

```text
<!-- Code example in TEXT -->
schema/
├── auth/
│   └── types.json       # User, Session, login, logout
├── products/
│   └── types.json       # Product, Category, listProducts, createProduct
├── orders/
│   └── types.json       # Order, OrderItem, createOrder, getOrder
└── inventory/
    └── types.json       # Inventory, checkStock, updateStock
```text
<!-- Code example in TEXT -->

### Auth Domain

```json
<!-- Code example in JSON -->
{
  "types": [
    {"name": "User", "fields": [...]},
    {"name": "Session", "fields": [...]}
  ],
  "queries": [
    {"name": "getUser", "return_type": "User"},
    {"name": "getCurrentUser", "return_type": "User"}
  ],
  "mutations": [
    {"name": "login", "return_type": "Session"},
    {"name": "logout", "return_type": "User"}
  ]
}
```text
<!-- Code example in TEXT -->

### Products Domain

```json
<!-- Code example in JSON -->
{
  "types": [
    {"name": "Product", "fields": [...]},
    {"name": "Category", "fields": [...]}
  ],
  "queries": [
    {"name": "getProduct", "return_type": "Product"},
    {"name": "listProducts", "return_type": "Product", "return_array": true}
  ],
  "mutations": [
    {"name": "createProduct", "return_type": "Product", "operation": "insert"}
  ]
}
```text
<!-- Code example in TEXT -->

### Orders Domain (Cross-Domain References)

```json
<!-- Code example in JSON -->
{
  "types": [
    {"name": "Order", "fields": [
      {"name": "userId", "type": "ID"},    # References User from auth domain
      {"name": "products", "type": "[Product]"}  # References Product from products domain
    ]},
    {"name": "OrderItem", "fields": [...]}
  ],
  "queries": [
    {"name": "getOrder", "return_type": "Order"},
    {"name": "getUserOrders", "return_type": "Order", "return_array": true}
  ],
  "mutations": [
    {"name": "createOrder", "return_type": "Order", "operation": "insert"}
  ]
}
```text
<!-- Code example in TEXT -->

## Best Practices

### 1. One Domain = One Responsibility

Good:

```text
<!-- Code example in TEXT -->
schema/auth/ - Authentication only
schema/products/ - Product catalog only
```text
<!-- Code example in TEXT -->

Bad:

```text
<!-- Code example in TEXT -->
schema/everything/ - Types, products, auth, billing, ...
```text
<!-- Code example in TEXT -->

### 2. Domain Owns Its Types

The **auth domain** should define `User`, not the orders domain:

```text
<!-- Code example in TEXT -->
✅ schema/auth/types.json
   {"name": "User", "fields": [...]}

❌ schema/orders/types.json
   {"name": "User", "fields": [...]}  // Don't duplicate!
```text
<!-- Code example in TEXT -->

### 3. Consumer Owns Cross-Domain Queries

The **orders domain** (consumer) should define cross-domain queries:

```text
<!-- Code example in TEXT -->
✅ schema/orders/queries.json
   {
     "name": "getOrderByUser",
     "return_type": "Order",
     "fields": [
       {"name": "userId", "type": "ID"}  // From auth domain
     ]
   }

❌ schema/auth/queries.json
   {
     "name": "getUserOrders",  // Orders domain owns this query
     ...
   }
```text
<!-- Code example in TEXT -->

### 4. Document Your Domains

Add `README.md` to each domain:

```markdown
<!-- Code example in MARKDOWN -->
# Auth Domain

Handles user authentication and session management.

## Types

- User: User account with email and profile
- Session: Authentication session token

## Queries

- getUser(id): Get user by ID
- getCurrentUser(): Get authenticated user

## Mutations

- login(email, password): Create session
- logout(): Invalidate session
- register(email, password): Create new user

## Dependencies
None - core domain

## Cross-Domain Usage

- Orders domain references User
- Products domain references User (vendor)
```text
<!-- Code example in TEXT -->

### 5. Validate Your Schema

Always validate after adding/removing domains:

```bash
<!-- Code example in BASH -->
FraiseQL compile FraiseQL.toml --check
```text
<!-- Code example in TEXT -->

This validates:

- No duplicate type names
- All referenced types are defined
- All queries reference existing types
- All mutations reference existing types

## Scaling Patterns

### Pattern 1: Simple (2-5 Domains)

```text
<!-- Code example in TEXT -->
schema/
├── auth/
├── products/
└── orders/
```text
<!-- Code example in TEXT -->

Use: `FraiseQL compile FraiseQL.toml`

### Pattern 2: Medium (5-15 Domains)

```text
<!-- Code example in TEXT -->
schema/
├── auth/
├── products/
├── orders/
├── inventory/
├── billing/
└── shipping/
```text
<!-- Code example in TEXT -->

Use: Auto-discovery with clear domain ownership

### Pattern 3: Large (15+ Domains)

```text
<!-- Code example in TEXT -->
schema/
├── core/
│   └── auth/      # Multi-level nesting
│   └── users/
├── commerce/
│   └── products/
│   └── orders/
│   └── inventory/
├── services/
│   └── billing/
│   └── shipping/
└── analytics/
```text
<!-- Code example in TEXT -->

Consider:

- Nested domain structure
- Domain groups (commerce, services, etc.)
- Separate compilation per team
- Domain dependency documentation

## Migration Guide

### From Single Monolithic Schema

**Before**:

```text
<!-- Code example in TEXT -->
schema.json (2000 lines)
```text
<!-- Code example in TEXT -->

**After**:

```text
<!-- Code example in TEXT -->
schema/
├── auth/types.json
├── products/types.json
├── orders/types.json
└── inventory/types.json
```text
<!-- Code example in TEXT -->

**Step 1**: Create domain structure

```bash
<!-- Code example in BASH -->
mkdir -p schema/{auth,products,orders,inventory}
```text
<!-- Code example in TEXT -->

**Step 2**: Split schema.json into domains

```bash
<!-- Code example in BASH -->
# Manually edit schema.json and split by domain
# Copy User, Session types to schema/auth/types.json
# Copy Product, Category types to schema/products/types.json
# etc.
```text
<!-- Code example in TEXT -->

**Step 3**: Configure domain discovery

```toml
<!-- Code example in TOML -->
[domain_discovery]
enabled = true
root_dir = "schema"
```text
<!-- Code example in TEXT -->

**Step 4**: Compile and verify

```bash
<!-- Code example in BASH -->
FraiseQL compile FraiseQL.toml
FraiseQL compile FraiseQL.toml --check
```text
<!-- Code example in TEXT -->

## Examples

Three complete examples are included:

1. **ecommerce** - Product catalog with auth, orders, inventory
   - 4 domains: auth, products, orders, inventory
   - Demonstrates cross-domain references
   - See: `examples/ecommerce/`

2. **saas** - Multi-tenant SaaS platform
   - 4 domains: accounts, billing, teams, integrations
   - Shows multi-tenancy pattern with accountId
   - See: `examples/saas/`

3. **multitenant** - Simple multi-tenant structure
   - 3 domains: core, tenants, resources
   - Minimal example showing tenant isolation
   - See: `examples/multitenant/`

Build any example:

```bash
<!-- Code example in BASH -->
cd examples/ecommerce
FraiseQL compile FraiseQL.toml
```text
<!-- Code example in TEXT -->

## Troubleshooting

### "Type 'X' not found"

The type is defined in another domain but not exported. Make sure the domain's types.json includes the type.

### "Duplicate type 'X'"

Two domains define the same type. Only one domain should define each type - the domain that "owns" that business concept.

### "Domain directory not found"

Check that `root_dir` in `[domain_discovery]` points to your schema directory.

### Slow compilation with many domains

- Check file counts (should be <100 files typically)
- Verify disk I/O is not bottleneck
- Consider grouping domains into subdirectories

## Advanced: Domain Linting

To enforce domain best practices, add validation:

```bash
<!-- Code example in BASH -->
# Check for duplicate types (future feature)
FraiseQL validate domains

# Check for circular dependencies
FraiseQL lint domains --detect-cycles

# Generate domain dependency graph
FraiseQL docs domains --graph
```text
<!-- Code example in TEXT -->

These are potential future enhancements to FraiseQL.

---

**See Also**:

- [migration-guide.md](migration-guide.md) - Detailed migration instructions
- `examples/` - Working examples
