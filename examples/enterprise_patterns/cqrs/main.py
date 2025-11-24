"""CQRS Enterprise Pattern Example - Main Application.

This demonstrates advanced enterprise patterns with FraiseQL:
- Command Query Responsibility Segregation (CQRS)
- Event sourcing preparation (audit log)
- Optimistic locking
- Database-level business logic
- Read/write separation
"""

# Import all types
from types import (
    AddProductInput,
    AuditLog,
    CancelOrderInput,
    CreateOrderInput,
    Customer,
    CustomerLifetimeValue,
    OrderItemDetails,
    OrderItemInput,
    OrderStatusTimeline,
    OrderSummary,
    Payment,
    ProcessPaymentInput,
    Product,
    ProductInventory,
    RevenueByProduct,
    UpdateOrderStatusInput,
    UpdateProductInventoryInput,
)

import uvicorn

from fraiseql import FraiseQL
from fraiseql.fastapi import create_app

# Initialize FraiseQL
app = FraiseQL(database_url="postgresql://localhost/cqrs_orders_demo")

# ============================================================================
# REGISTER TYPES
# ============================================================================

# Entity types (Read model)
app.register_type(Customer)
app.register_type(Product)
app.register_type(ProductInventory)
app.register_type(OrderSummary)
app.register_type(OrderItemDetails)
app.register_type(Payment)
app.register_type(RevenueByProduct)
app.register_type(CustomerLifetimeValue)
app.register_type(AuditLog)
app.register_type(OrderStatusTimeline)

# Input types (Write model)
app.register_input_type(OrderItemInput)
app.register_input_type(CreateOrderInput)
app.register_input_type(ProcessPaymentInput)
app.register_input_type(CancelOrderInput)
app.register_input_type(UpdateOrderStatusInput)
app.register_input_type(AddProductInput)
app.register_input_type(UpdateProductInventoryInput)

# ============================================================================
# REGISTER QUERIES (Read Side - Uses Views)
# ============================================================================

from queries import (
    Customer_orders,
    OrderItemDetails_product,
    OrderSummary_customer,
    OrderSummary_items,
    OrderSummary_payments,
    audit_log,
    customer,
    customer_lifetime_value,
    customers,
    order,
    order_by_number,
    order_items_details,
    order_status_timeline,
    orders_summary,
    payment,
    payments,
    product,
    product_inventory,
    products,
    revenue_by_product,
)

# Root queries
app.register_query(customer)
app.register_query(customers)
app.register_query(product)
app.register_query(products)
app.register_query(product_inventory)
app.register_query(order)
app.register_query(order_by_number)
app.register_query(orders_summary)
app.register_query(order_items_details)
app.register_query(payment)
app.register_query(payments)
app.register_query(revenue_by_product)
app.register_query(customer_lifetime_value)
app.register_query(audit_log)
app.register_query(order_status_timeline)

# Nested resolvers
app.register_field_resolver(Customer, "orders", Customer_orders)
app.register_field_resolver(OrderSummary, "customer", OrderSummary_customer)
app.register_field_resolver(OrderSummary, "items", OrderSummary_items)
app.register_field_resolver(OrderSummary, "payments", OrderSummary_payments)
app.register_field_resolver(OrderItemDetails, "product", OrderItemDetails_product)

# ============================================================================
# REGISTER MUTATIONS (Write Side - Uses PostgreSQL Functions)
# ============================================================================

from mutations import (
    add_product,
    cancel_order,
    create_order,
    process_payment,
    update_order_status,
    update_product_inventory,
)

app.register_mutation(create_order)
app.register_mutation(process_payment)
app.register_mutation(cancel_order)
app.register_mutation(update_order_status)
app.register_mutation(add_product)
app.register_mutation(update_product_inventory)

# ============================================================================
# CREATE FASTAPI APP
# ============================================================================

fastapi_app = create_app(
    app,
    database_url="postgresql://localhost/cqrs_orders_demo",
    enable_playground=True,
    cors_origins=["*"],  # Configure for your frontend
    pool_size=50,  # Larger pool for read-heavy workload
    max_overflow=20,
)


@fastapi_app.get("/")
async def root():
    """Root endpoint with API information."""
    return {
        "name": "FraiseQL CQRS Orders API",
        "version": "1.0.0",
        "pattern": "CQRS (Command Query Responsibility Segregation)",
        "graphql": "/graphql",
        "playground": "/graphql",
        "docs": "/docs",
        "features": [
            "Read views for optimized queries",
            "PostgreSQL functions for business logic",
            "Optimistic locking support",
            "Complete audit trail",
            "ACID guarantees on all mutations",
        ],
    }


@fastapi_app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy", "pattern": "CQRS"}


# ============================================================================
# RUN SERVER
# ============================================================================

if __name__ == "__main__":
    print("=" * 80)
    print("FraiseQL CQRS Enterprise Pattern Example")
    print("=" * 80)
    print()
    print("🏗️  Architecture: CQRS (Command Query Responsibility Segregation)")
    print()
    print("📖 Read Side (Queries):")
    print("  • Uses optimized database views (v_*)")
    print("  • Denormalized for performance")
    print("  • No business logic - just data retrieval")
    print("  • Can use read replicas in production")
    print()
    print("✍️  Write Side (Mutations):")
    print("  • Uses PostgreSQL functions (fn_*)")
    print("  • Business logic in database")
    print("  • ACID guarantees")
    print("  • Automatic audit logging")
    print("  • Optimistic locking support")
    print()
    print("📍 Endpoints:")
    print("  • GraphQL API:        http://localhost:8000/graphql")
    print("  • GraphQL Playground: http://localhost:8000/graphql")
    print("  • API Docs:           http://localhost:8000/docs")
    print("  • Health Check:       http://localhost:8000/health")
    print()
    print("💡 Example Query:")
    print()
    print("  # Get orders with full details (single query, denormalized view)")
    print("  query {")
    print("    ordersSummary(limit: 10) {")
    print("      orderNumber")
    print("      customerName")
    print("      customerEmail")
    print("      status")
    print("      total")
    print("      itemCount")
    print("      items {")
    print("        productName")
    print("        quantity")
    print("        unitPrice")
    print("      }")
    print("    }")
    print("  }")
    print()
    print("💡 Example Mutation:")
    print()
    print("  # Create order (atomic, with inventory validation)")
    print("  mutation {")
    print("    createOrder(input: {")
    print("      customerId: 1")
    print("      items: [")
    print("        { productId: 1, quantity: 2 }")
    print("        { productId: 3, quantity: 1 }")
    print("      ]")
    print("    }) {")
    print("      orderNumber")
    print("      total")
    print("      status")
    print("    }")
    print("  }")
    print()
    print("🔍 Analytics Queries:")
    print()
    print("  • revenueByProduct - Revenue analytics per product")
    print("  • customerLifetimeValue - Customer LTV rankings")
    print("  • auditLog - Complete change history")
    print("  • orderStatusTimeline - Order fulfillment metrics")
    print()
    print("=" * 80)
    print()

    uvicorn.run(fastapi_app, host="0.0.0.0", port=8000)
