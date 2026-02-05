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

### ✅ Scalable

Start with monolithic, grow to many domains:

```text
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
schema/
└── {domain_name}/
    ├── types.json          # Types, queries, mutations
    ├── queries.json        # (Optional) Query-only file
    ├── mutations.json      # (Optional) Mutation-only file
    └── README.md           # (Optional) Domain documentation
```text

### Simple: Single File per Domain

```text
schema/auth/types.json
{
  "types": [...],
  "queries": [...],
  "mutations": [...]
}
```text

### Advanced: Separate Files

```text
schema/auth/
├── types.json      # Type definitions only
├── queries.json    # Auth queries
└── mutations.json  # Login, logout, register
```text

## Configuration

### Option 1: Auto-Discovery (Recommended)

Automatically discover all domains in a directory:

```toml
[domain_discovery]
enabled = true
root_dir = "schema"
```text

Then compile:

```bash
fraiseql compile fraiseql.toml
```text

The compiler will:

1. Find all subdirectories in `schema/`
2. Load `types.json`, `queries.json`, `mutations.json` from each
3. Merge everything into one compiled schema
4. Validate cross-domain references

### Option 2: Explicit List

If you need fine-grained control:

```toml
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

## Example: E-Commerce

```text
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

### Auth Domain

```json
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

### Products Domain

```json
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

### Orders Domain (Cross-Domain References)

```json
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

## Best Practices

### 1. One Domain = One Responsibility

Good:

```text
schema/auth/ - Authentication only
schema/products/ - Product catalog only
```text

Bad:

```text
schema/everything/ - Types, products, auth, billing, ...
```text

### 2. Domain Owns Its Types

The **auth domain** should define `User`, not the orders domain:

```text
✅ schema/auth/types.json
   {"name": "User", "fields": [...]}

❌ schema/orders/types.json
   {"name": "User", "fields": [...]}  // Don't duplicate!
```text

### 3. Consumer Owns Cross-Domain Queries

The **orders domain** (consumer) should define cross-domain queries:

```text
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

### 4. Document Your Domains

Add `README.md` to each domain:

```markdown
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

### 5. Validate Your Schema

Always validate after adding/removing domains:

```bash
fraiseql compile fraiseql.toml --check
```text

This validates:

- No duplicate type names
- All referenced types are defined
- All queries reference existing types
- All mutations reference existing types

## Scaling Patterns

### Pattern 1: Simple (2-5 Domains)

```text
schema/
├── auth/
├── products/
└── orders/
```text

Use: `fraiseql compile fraiseql.toml`

### Pattern 2: Medium (5-15 Domains)

```text
schema/
├── auth/
├── products/
├── orders/
├── inventory/
├── billing/
└── shipping/
```text

Use: Auto-discovery with clear domain ownership

### Pattern 3: Large (15+ Domains)

```text
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

Consider:

- Nested domain structure
- Domain groups (commerce, services, etc.)
- Separate compilation per team
- Domain dependency documentation

## Migration Guide

### From Single Monolithic Schema

**Before**:

```text
schema.json (2000 lines)
```text

**After**:

```text
schema/
├── auth/types.json
├── products/types.json
├── orders/types.json
└── inventory/types.json
```text

**Step 1**: Create domain structure

```bash
mkdir -p schema/{auth,products,orders,inventory}
```text

**Step 2**: Split schema.json into domains

```bash
# Manually edit schema.json and split by domain
# Copy User, Session types to schema/auth/types.json
# Copy Product, Category types to schema/products/types.json
# etc.
```text

**Step 3**: Configure domain discovery

```toml
[domain_discovery]
enabled = true
root_dir = "schema"
```text

**Step 4**: Compile and verify

```bash
fraiseql compile fraiseql.toml
fraiseql compile fraiseql.toml --check
```text

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
cd examples/ecommerce
fraiseql compile fraiseql.toml
```text

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
# Check for duplicate types (future feature)
fraiseql validate domains

# Check for circular dependencies
fraiseql lint domains --detect-cycles

# Generate domain dependency graph
fraiseql docs domains --graph
```text

These are potential future enhancements to FraiseQL.

---

**See Also**:

- [MIGRATION_GUIDE.md](MIGRATION_GUIDE.md) - Detailed migration instructions
- `examples/` - Working examples
