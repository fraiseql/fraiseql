"""Demonstration of auto-generated WhereInput and OrderBy types.

This example shows how Phase 1 and Phase 2 of auto-generation work:
- Phase 1: Basic lazy property generation
- Phase 2: Nested type handling
"""

from dataclasses import dataclass
from uuid import UUID, uuid4

import fraiseql

# Phase 1: Basic Auto-Generation
print("=== Phase 1: Basic Auto-Generation ===\n")


@fraiseql.type(sql_source="v_customer")
@dataclass
class Customer:
    """Customer type with auto-generated WhereInput and OrderBy."""

    id: UUID
    name: str
    email: str


# WhereInput and OrderBy are automatically available via lazy properties
print(f"Customer has WhereInput: {hasattr(Customer, 'WhereInput')}")
print(f"Customer has OrderBy: {hasattr(Customer, 'OrderBy')}")

# Access the auto-generated types (they're generated on first access)
CustomerWhereInput = Customer.WhereInput
CustomerOrderBy = Customer.OrderBy

print(f"\nCustomer.WhereInput: {CustomerWhereInput.__name__}")
print(f"Customer.OrderBy: {CustomerOrderBy.__name__}")

# Create instance to show it works
customer_filter = CustomerWhereInput(name={"contains": "Acme"})
print(f"\nCreated filter: {customer_filter}")

# Phase 2: Nested Type Auto-Generation
print("\n\n=== Phase 2: Nested Type Auto-Generation ===\n")


@fraiseql.type(sql_source="v_order")
@dataclass
class Order:
    """Order type with nested Customer reference."""

    id: UUID
    order_number: str
    customer_id: UUID
    customer: Customer | None  # Nested type


# OrderWhereInput should automatically include CustomerWhereInput for the nested field
OrderWhereInput = Order.WhereInput

print(f"Order.WhereInput: {OrderWhereInput.__name__}")
print(f"\nOrder.WhereInput fields: {list(OrderWhereInput.__annotations__.keys())}")

# Verify nested customer field exists
if "customer" in OrderWhereInput.__annotations__:
    customer_annotation = OrderWhereInput.__annotations__["customer"]
    print(f"\nNested 'customer' field type: {customer_annotation}")
    print("✓ Nested type auto-generation working!")

# Usage Example
print("\n\n=== Usage Example ===\n")


@fraiseql.type(sql_source="v_product")
@dataclass
class Product:
    id: UUID
    name: str
    price: float


# Before (manual approach - no longer needed):
# ProductWhereInput = create_graphql_where_input(Product)
# ProductOrderBy = create_graphql_order_by_input(Product)

# After (auto-generated - just use it):
print("Using auto-generated types in query signature:")
print("""
@fraiseql.query
async def products(
    where: Product.WhereInput | None = None,  # ✅ Auto-generated!
    order_by: Product.OrderBy | None = None,  # ✅ Auto-generated!
) -> list[Product]:
    return await db.find("v_product", where=where, order_by=order_by)
""")

# Demonstrate the types exist
print(f"Product.WhereInput: {Product.WhereInput}")
print(f"Product.OrderBy: {Product.OrderBy}")

print("\n✨ Auto-generation eliminates ~100+ lines of boilerplate!")
print("✨ Zero startup cost - types generated only when accessed")
print("✨ Handles nested types automatically")
