# Example Schemas

Real-world FraiseQL schema examples.

## 1. Blog Platform

Simple blog with posts, comments, and users.

```python
import fraiseql

# Define types
@fraiseql.type
class User:
    """A blog user."""
    id: int
    username: str
    email: str
    created_at: str

@fraiseql.type
class Post:
    """A blog post."""
    id: int
    title: str
    body: str
    author_id: int
    published_at: str | None
    created_at: str

@fraiseql.type
class Comment:
    """A comment on a post."""
    id: int
    body: str
    author_id: int
    post_id: int
    created_at: str

# Define queries
@fraiseql.query(sql_source="v_published_posts")
def posts(limit: int = 10, offset: int = 0) -> list[Post]:
    """Get all published posts."""
    pass

@fraiseql.query(sql_source="v_post_by_id")
def post(id: int) -> Post | None:
    """Get a single post by ID."""
    pass

@fraiseql.query(sql_source="v_user_by_id")
def user(id: int) -> User | None:
    """Get a user by ID."""
    pass

@fraiseql.query(sql_source="v_comments_by_post")
def comments(post_id: int, limit: int = 20) -> list[Comment]:
    """Get comments for a post."""
    pass

# Define mutations
@fraiseql.mutation(sql_source="fn_create_post", operation="CREATE")
def create_post(title: str, body: str, author_id: int) -> Post:
    """Create a new post."""
    pass

@fraiseql.mutation(sql_source="fn_publish_post", operation="UPDATE")
def publish_post(id: int) -> Post:
    """Publish a draft post."""
    pass

@fraiseql.mutation(sql_source="fn_create_comment", operation="CREATE")
def create_comment(body: str, author_id: int, post_id: int) -> Comment:
    """Add a comment to a post."""
    pass

if __name__ == "__main__":
    fraiseql.export_schema("blog_schema.json")
```

## 2. E-Commerce Platform

Products, orders, inventory, and customers.

```python
import fraiseql

# Types
@fraiseql.type
class Product:
    """A product in the catalog."""
    id: int
    sku: str
    name: str
    description: str | None
    price: float
    stock: int
    category: str
    created_at: str

@fraiseql.type
class Customer:
    """A customer account."""
    id: int
    name: str
    email: str
    country: str
    credit_limit: float
    created_at: str

@fraiseql.type
class Order:
    """A customer order."""
    id: int
    customer_id: int
    total: float
    status: str  # pending, shipped, delivered
    created_at: str
    shipped_at: str | None
    delivered_at: str | None

@fraiseql.type
class OrderItem:
    """Item in an order."""
    id: int
    order_id: int
    product_id: int
    quantity: int
    unit_price: float
    subtotal: float

# Queries
@fraiseql.query(sql_source="v_products")
def products(category: str | None = None, limit: int = 50) -> list[Product]:
    """Get all products, optionally filtered by category."""
    pass

@fraiseql.query(sql_source="v_product_by_id")
def product(id: int) -> Product | None:
    """Get a product by ID."""
    pass

@fraiseql.query(sql_source="v_customer_by_id")
def customer(id: int) -> Customer | None:
    """Get a customer by ID."""
    pass

@fraiseql.query(sql_source="v_customer_orders")
def customer_orders(customer_id: int) -> list[Order]:
    """Get all orders for a customer."""
    pass

@fraiseql.query(sql_source="v_order_by_id")
def order(id: int) -> Order | None:
    """Get an order by ID."""
    pass

@fraiseql.query(sql_source="v_order_items")
def order_items(order_id: int) -> list[OrderItem]:
    """Get items in an order."""
    pass

# Mutations
@fraiseql.mutation(sql_source="fn_create_customer", operation="CREATE")
def create_customer(name: str, email: str, country: str) -> Customer:
    """Create a new customer account."""
    pass

@fraiseql.mutation(sql_source="fn_create_order", operation="CREATE")
def create_order(customer_id: int) -> Order:
    """Create a new order."""
    pass

@fraiseql.mutation(sql_source="fn_add_item_to_order", operation="CREATE")
def add_item_to_order(order_id: int, product_id: int, quantity: int) -> OrderItem:
    """Add an item to an order."""
    pass

@fraiseql.mutation(sql_source="fn_ship_order", operation="UPDATE")
def ship_order(id: int) -> Order:
    """Mark an order as shipped."""
    pass

if __name__ == "__main__":
    fraiseql.export_schema("ecommerce_schema.json")
```

## 3. SaaS Analytics with Fact Tables

Multi-tenant SaaS with event analytics.

```python
import fraiseql

# Core types
@fraiseql.type
class Organization:
    """A SaaS organization (tenant)."""
    id: str
    name: str
    tier: str  # free, pro, enterprise
    subscription_status: str
    created_at: str

@fraiseql.type
class User:
    """A user within an organization."""
    id: str
    org_id: str
    name: str
    email: str
    role: str  # admin, member, viewer
    created_at: str

# Fact table for events
@fraiseql.fact_table(
    table_name="tf_events",
    measures=["value", "duration_ms"],
    dimension_paths=[
        {"name": "event_type", "json_path": "data->>'event_type'", "data_type": "text"},
        {"name": "page", "json_path": "data->>'page'", "data_type": "text"},
        {"name": "browser", "json_path": "data->>'browser'", "data_type": "text"},
        {"name": "country", "json_path": "data->>'country'", "data_type": "text"}
    ]
)
@fraiseql.type
class Event:
    """Analytics event fact table."""
    id: str
    org_id: str
    user_id: str
    value: float
    duration_ms: int
    occurred_at: str

# Queries
@fraiseql.query(sql_source="v_organization")
def organization(id: str) -> Organization | None:
    """Get organization details."""
    pass

@fraiseql.query(sql_source="v_org_users")
def org_users(org_id: str) -> list[User]:
    """Get all users in an organization."""
    pass

# Aggregate queries for analytics
@fraiseql.aggregate_query(
    fact_table="tf_events",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def event_analytics() -> list[dict]:
    """Flexible event analytics by type, page, browser, country."""
    pass

@fraiseql.aggregate_query(
    fact_table="tf_events",
    auto_group_by=True,
    auto_aggregates=True
)
@fraiseql.query
def event_trends() -> list[dict]:
    """Event trends over time."""
    pass

if __name__ == "__main__":
    fraiseql.export_schema("saas_schema.json")
```

## 4. Inventory Management System

Warehouses, products, stock levels, and movements.

```python
import fraiseql

@fraiseql.type
class Warehouse:
    """A physical warehouse location."""
    id: int
    name: str
    city: str
    country: str
    capacity: int
    created_at: str

@fraiseql.type
class Product:
    """A product."""
    id: int
    sku: str
    name: str
    unit_cost: float
    weight_kg: float

@fraiseql.type
class StockLevel:
    """Current stock level in a warehouse."""
    id: int
    warehouse_id: int
    product_id: int
    quantity: int
    last_updated: str

@fraiseql.type
class StockMovement:
    """A stock movement (in/out)."""
    id: int
    warehouse_id: int
    product_id: int
    quantity: int
    movement_type: str  # in, out, adjustment
    reason: str
    occurred_at: str

# Queries
@fraiseql.query(sql_source="v_warehouses")
def warehouses() -> list[Warehouse]:
    """Get all warehouses."""
    pass

@fraiseql.query(sql_source="v_stock_levels")
def stock_levels(warehouse_id: int | None = None) -> list[StockLevel]:
    """Get stock levels, optionally for a specific warehouse."""
    pass

@fraiseql.query(sql_source="v_low_stock")
def low_stock(threshold: int = 10) -> list[StockLevel]:
    """Find products below stock threshold."""
    pass

@fraiseql.query(sql_source="v_stock_movements")
def stock_movements(
    warehouse_id: int | None = None,
    days: int = 30
) -> list[StockMovement]:
    """Get recent stock movements."""
    pass

# Mutations
@fraiseql.mutation(sql_source="fn_receive_stock", operation="CREATE")
def receive_stock(warehouse_id: int, product_id: int, quantity: int) -> StockLevel:
    """Record incoming stock."""
    pass

@fraiseql.mutation(sql_source="fn_ship_stock", operation="CREATE")
def ship_stock(warehouse_id: int, product_id: int, quantity: int) -> StockLevel:
    """Record outgoing stock."""
    pass

@fraiseql.mutation(sql_source="fn_adjust_stock", operation="CREATE")
def adjust_stock(warehouse_id: int, product_id: int, quantity_delta: int) -> StockLevel:
    """Adjust stock for damages or losses."""
    pass

if __name__ == "__main__":
    fraiseql.export_schema("inventory_schema.json")
```

## 5. Multi-Tenant CRM

Companies, contacts, opportunities, and activities.

```python
import fraiseql

@fraiseql.type
class Company:
    """A company (B2B customer or prospect)."""
    id: str
    tenant_id: str
    name: str
    industry: str
    annual_revenue: float | None
    employees: int | None
    created_at: str

@fraiseql.type
class Contact:
    """A person at a company."""
    id: str
    tenant_id: str
    company_id: str
    name: str
    email: str
    phone: str | None
    title: str
    created_at: str

@fraiseql.type
class Opportunity:
    """A sales opportunity."""
    id: str
    tenant_id: str
    company_id: str
    name: str
    value: float
    stage: str  # lead, prospect, proposal, won, lost
    probability: float  # 0.0 to 1.0
    expected_close: str
    closed_at: str | None
    created_at: str

@fraiseql.type
class Activity:
    """A CRM activity (call, email, meeting)."""
    id: str
    tenant_id: str
    company_id: str
    contact_id: str | None
    type: str  # call, email, meeting, task
    subject: str
    description: str | None
    occurred_at: str
    created_at: str

# Queries
@fraiseql.query(sql_source="v_companies_by_tenant")
def companies(tenant_id: str, limit: int = 50) -> list[Company]:
    """Get all companies for a tenant."""
    pass

@fraiseql.query(sql_source="v_contacts_by_company")
def contacts(company_id: str) -> list[Contact]:
    """Get contacts for a company."""
    pass

@fraiseql.query(sql_source="v_opportunities_by_tenant")
def opportunities(tenant_id: str, stage: str | None = None) -> list[Opportunity]:
    """Get opportunities, optionally filtered by stage."""
    pass

@fraiseql.query(sql_source="v_recent_activities")
def activities(tenant_id: str, days: int = 30) -> list[Activity]:
    """Get recent activities."""
    pass

# Mutations
@fraiseql.mutation(sql_source="fn_create_company", operation="CREATE")
def create_company(tenant_id: str, name: str, industry: str) -> Company:
    """Create a new company."""
    pass

@fraiseql.mutation(sql_source="fn_add_contact", operation="CREATE")
def add_contact(company_id: str, name: str, email: str, title: str) -> Contact:
    """Add a contact to a company."""
    pass

@fraiseql.mutation(sql_source="fn_create_opportunity", operation="CREATE")
def create_opportunity(tenant_id: str, company_id: str, name: str, value: float) -> Opportunity:
    """Create a sales opportunity."""
    pass

@fraiseql.mutation(sql_source="fn_update_opportunity", operation="UPDATE")
def update_opportunity(id: str, stage: str, probability: float) -> Opportunity:
    """Update opportunity stage and probability."""
    pass

if __name__ == "__main__":
    fraiseql.export_schema("crm_schema.json")
```

## Getting Started with Examples

To use any of these examples:

1. **Create schema file** with the code above
2. **Export schema**:

   ```bash
   python blog_schema.py  # or your schema file
   ```

3. **Create SQL views/functions** for each `sql_source`
4. **Compile schema**:

   ```bash
   fraiseql-cli compile blog_schema.json -o schema.compiled.json
   ```

5. **Deploy server**:

   ```bash
   fraiseql-server --schema schema.compiled.json
   ```

## Customization

Adapt these examples for your needs:

- **Add fields**: Add type annotations to classes
- **Add queries**: Create new `@fraiseql.query` functions
- **Add mutations**: Create new `@fraiseql.mutation` functions
- **Add types**: Define new `@fraiseql.type` classes
- **Add fact tables**: For analytics, use `@fraiseql.fact_table`

See [Decorators Reference](DECORATORS_REFERENCE.md) for complete API documentation.
