#!/usr/bin/env python
"""Admin Panel Example - Main Application.

Complete admin panel for customer support, operations, and sales teams.
"""

import os
from contextlib import asynccontextmanager

import uvicorn
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware

# Import models to register types
from models import (
    AdminUser,
    AuditLogEntry,
    CustomerInfo,
    CustomerUpdateInput,
    Deal,
    DealUpdateInput,
    OperationsMetrics,
    Order,
    OrderItem,
    OrderStatusUpdateInput,
    SalesMetrics,
    SupportTicket,
)
from mutations import (
    assign_ticket,
    create_deal,
    mark_order_shipped,
    refund_order,
    update_customer_status,
    update_deal_stage,
    update_order_status,
    update_ticket_status,
)

# Import queries and mutations
from queries import (
    audit_log,
    audit_log_for_entity,
    customer_by_id,
    customer_search,
    customer_support_tickets,
    deals,
    my_pipeline,
    operations_metrics,
    order_by_id,
    orders,
    orders_needing_attention,
    sales_metrics,
    support_tickets,
)

from fraiseql.fastapi import FraiseQLConfig, create_fraiseql_app

# Database URL from environment
DATABASE_URL = os.getenv(
    "DATABASE_URL", "postgresql://localhost/admin_panel_demo"
)

# CORS origins for admin frontend
CORS_ORIGINS = os.getenv("CORS_ORIGINS", "http://localhost:3000,http://localhost:8080").split(",")


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan manager."""
    print("=" * 70)
    print("Admin Panel Starting...")
    print("=" * 70)
    yield
    print("\nAdmin Panel Shutting Down...")


# Configure FraiseQL
config = FraiseQLConfig(
    database_url=DATABASE_URL,
    enable_playground=True,
    cors_origins=CORS_ORIGINS,
    pool_size=20,
    max_overflow=10,
)

# Create FraiseQL FastAPI app
app = create_fraiseql_app(
    config=config,
    title="Admin Panel API",
    version="1.0.0",
    description="Internal admin panel for customer support, operations, and sales",
    lifespan=lifespan,
)

# Register all GraphQL types
app.register_type(CustomerInfo)
app.register_type(SupportTicket)
app.register_type(Order)
app.register_type(OrderItem)
app.register_type(OperationsMetrics)
app.register_type(SalesMetrics)
app.register_type(Deal)
app.register_type(AdminUser)
app.register_type(AuditLogEntry)

# Register input types
app.register_input_type(CustomerUpdateInput)
app.register_input_type(OrderStatusUpdateInput)
app.register_input_type(DealUpdateInput)

# Register queries
app.register_query(customer_search)
app.register_query(customer_by_id)
app.register_query(support_tickets)
app.register_query(customer_support_tickets)
app.register_query(operations_metrics)
app.register_query(orders)
app.register_query(order_by_id)
app.register_query(orders_needing_attention)
app.register_query(sales_metrics)
app.register_query(deals)
app.register_query(my_pipeline)
app.register_query(audit_log)
app.register_query(audit_log_for_entity)

# Register mutations
app.register_mutation(update_customer_status)
app.register_mutation(update_ticket_status)
app.register_mutation(assign_ticket)
app.register_mutation(update_order_status)
app.register_mutation(mark_order_shipped)
app.register_mutation(update_deal_stage)
app.register_mutation(create_deal)
app.register_mutation(refund_order)


# Additional FastAPI routes
@app.get("/")
async def root():
    """API information endpoint."""
    return {
        "name": "Admin Panel API",
        "version": "1.0.0",
        "graphql": "/graphql",
        "playground": "/graphql",
        "docs": "/docs",
        "dashboards": {
            "customer_support": "Customer search, support tickets",
            "operations": "Order management, fulfillment metrics",
            "sales": "Pipeline, deals, sales metrics",
        },
    }


@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy", "service": "admin-panel"}


# Authentication middleware (simplified for example)
@app.middleware("http")
async def add_admin_context(request: Request, call_next):
    """Add admin user context to requests.

    In production, this would:
    1. Validate JWT token
    2. Load admin user from database
    3. Check role permissions
    """
    # For demo purposes, use header-based auth
    admin_email = request.headers.get("X-Admin-User", "admin@example.com")

    # In production: Decode JWT, load user from DB, check permissions
    request.state.user = {
        "id": "11111111-1111-1111-1111-111111111111",
        "email": admin_email,
        "role": "admin",  # Would come from JWT/database
    }

    response = await call_next(request)
    return response


if __name__ == "__main__":
    print("=" * 70)
    print("FraiseQL Admin Panel")
    print("=" * 70)
    print()
    print("🎯 Dashboards:")
    print("  • Customer Support - Search customers, manage tickets")
    print("  • Operations       - Order fulfillment, inventory")
    print("  • Sales            - Pipeline, deals, metrics")
    print()
    print("📍 Endpoints:")
    print("  • GraphQL API:        http://localhost:8000/graphql")
    print("  • GraphQL Playground: http://localhost:8000/graphql")
    print("  • API Docs:           http://localhost:8000/docs")
    print("  • Health Check:       http://localhost:8000/health")
    print()
    print("🔒 Security Features:")
    print("  • Role-based access control")
    print("  • Automatic audit logging")
    print("  • Read-only database views")
    print()
    print("💡 Example Query:")
    print()
    print("  query SearchCustomers {")
    print('    customerSearch(query: "john@example.com") {')
    print("      id")
    print("      email")
    print("      name")
    print("      subscriptionStatus")
    print("      totalSpent")
    print("      ticketCount")
    print("    }")
    print("  }")
    print()
    print("=" * 70)
    print()

    uvicorn.run(app, host="0.0.0.0", port=8000, log_level="info")
