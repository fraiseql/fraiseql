# E-Commerce Example

A complete example of FraiseQL v2 with domain-driven schema organization.

## Schema Structure

This project demonstrates organizing a GraphQL schema across multiple domains:

```
schema/
├── auth/          # User authentication and sessions
│   └── types.json
├── products/      # Product catalog
│   └── types.json
├── orders/        # Order management
│   └── types.json
└── inventory/     # Stock tracking
    └── types.json
```

## Domains

### Auth Domain
- **Types**: User, Session
- **Queries**: getUser, getCurrentUser
- **Mutations**: login, logout

### Products Domain
- **Types**: Product, Category
- **Queries**: getProduct, listProducts, getCategory, listCategories
- **Mutations**: createProduct, updateProduct

### Orders Domain
- **Types**: Order, OrderItem
- **Queries**: getOrder, getUserOrders, getOrderItems
- **Mutations**: createOrder, updateOrderStatus

### Inventory Domain
- **Types**: Inventory, Warehouse
- **Queries**: getInventory, checkGlobalStock, listWarehouses
- **Mutations**: updateInventory, reserveInventory

## Running the Example

### 1. Compile the Schema

```bash
# From this directory
fraiseql compile fraiseql.toml
```

This will:
- Auto-discover all domains in the `schema/` directory
- Load `types.json` from each domain (auth, products, orders, inventory)
- Merge all types, queries, and mutations
- Generate `schema.compiled.json`

### 2. View the Compiled Schema

```bash
cat schema.compiled.json | jq .
```

## Configuration

The `fraiseql.toml` file uses domain discovery:

```toml
[domain_discovery]
enabled = true
root_dir = "schema"
```

This tells FraiseQL to:
1. Look in the `schema/` directory
2. Find all subdirectories (auth, products, orders, inventory)
3. Load `types.json`, `queries.json`, `mutations.json` from each
4. Merge everything into a single compiled schema

## Adding a New Domain

1. Create a new directory: `schema/new_domain/`
2. Add `types.json` with your types, queries, mutations
3. Run `fraiseql compile fraiseql.toml`
4. Done! The new domain is automatically discovered

Example:

```bash
mkdir -p schema/reviews
# Edit schema/reviews/types.json
fraiseql compile fraiseql.toml
```

## Best Practices

1. **One domain per directory** - Keep domains isolated
2. **Domain owns its types** - Auth domain defines User, not Products
3. **Consumer owns cross-domain queries** - Orders domain queries Products
4. **Explicit file structure** - Consistent naming (types.json, queries.json, mutations.json)
5. **Document your domain** - Add comments explaining domain purpose

## Files Generated

- `schema.compiled.json` - Complete compiled schema ready for runtime
- Contains merged types, queries, mutations from all domains
- Includes security configuration from fraiseql.toml

## Next Steps

- Add a `schema/payments/` domain for payment processing
- Create `schema/reviews/` domain for product reviews
- Add `schema/shipping/` domain for fulfillment
- Each new domain is automatically discovered and compiled

See `../../docs/DOMAIN_ORGANIZATION.md` for more details on organizing schemas by domain.
