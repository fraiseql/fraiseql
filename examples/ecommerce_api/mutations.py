"""E-commerce mutations demonstrating complex validation patterns.

This example showcases enterprise-grade patterns:
- Cross-entity validation (inventory, pricing, customer eligibility)
- Multi-layer validation (GraphQL, app, core, business rules)
- NOOP handling for business rule violations
- Comprehensive audit trails for financial data
- Transaction patterns with rollback handling

For simpler patterns, see ../blog_api/
For complete enterprise example, see ../enterprise_patterns/
"""

from datetime import datetime
from decimal import Decimal
from typing import Any, Optional
from uuid import UUID

# Import enterprise pattern types (would be defined in models.py)
from fraiseql import failure, input, mutation, success

from .models import (
    AddressMutationResult,
    CartMutationResult,
    CustomerMutationResult,
    OrderMutationResult,
    ReviewMutationResult,
)


# Cart Mutations
@mutation(
    name="addToCart",
    function="add_to_cart",
    description="Add a product variant to the shopping cart",
)
async def add_to_cart(
    variant_id: UUID,
    quantity: int,
    cart_id: Optional[UUID] = None,
    customer_id: Optional[UUID] = None,
    session_id: Optional[str] = None,
) -> CartMutationResult:
    """Add item to cart with inventory checking"""


@mutation(
    name="updateCartItem",
    function="update_cart_item",
    description="Update quantity of an item in the cart",
)
async def update_cart_item(
    cart_item_id: UUID,
    quantity: int,
    customer_id: Optional[UUID] = None,
    session_id: Optional[str] = None,
) -> CartMutationResult:
    """Update cart item quantity or remove if quantity is 0"""


@mutation(
    name="clearCart",
    function="clear_cart",
    description="Remove all items from the cart",
)
async def clear_cart(
    cart_id: UUID,
    customer_id: Optional[UUID] = None,
    session_id: Optional[str] = None,
) -> CartMutationResult:
    """Clear all items from cart"""


@mutation(
    name="applyCouponToCart",
    function="apply_coupon_to_cart",
    description="Apply a discount coupon to the cart",
)
async def apply_coupon_to_cart(
    cart_id: UUID,
    coupon_code: str,
    customer_id: Optional[UUID] = None,
    session_id: Optional[str] = None,
) -> CartMutationResult:
    """Apply coupon code to cart"""


# Order Mutations
@mutation(
    name="createOrder",
    function="create_order_from_cart",
    description="Create an order from the current cart",
)
async def create_order(
    cart_id: UUID,
    customer_id: UUID,
    shipping_address_id: UUID,
    billing_address_id: Optional[UUID] = None,
    payment_method: Optional[dict] = None,
    notes: Optional[str] = None,
) -> OrderMutationResult:
    """Convert cart to order with inventory reservation"""


@mutation(
    name="updateOrderStatus",
    function="update_order_status",
    description="Update the status of an order",
)
async def update_order_status(
    order_id: UUID,
    status: str,
    notes: Optional[str] = None,
) -> OrderMutationResult:
    """Update order status with validation"""


@mutation(
    name="processOrderPayment",
    function="process_order_payment",
    description="Process payment for an order",
)
async def process_order_payment(
    order_id: UUID,
    payment_details: dict,
) -> OrderMutationResult:
    """Process payment and update order status"""


@mutation(name="cancelOrder", function="cancel_order", description="Cancel an order")
async def cancel_order(
    order_id: UUID,
    customer_id: UUID,
    reason: str,
) -> OrderMutationResult:
    """Cancel order and release inventory"""


# Customer Mutations
@mutation(
    name="registerCustomer",
    function="register_customer",
    description="Register a new customer account",
)
async def register_customer(
    email: str,
    password: str,
    first_name: str,
    last_name: str,
    phone: Optional[str] = None,
) -> CustomerMutationResult:
    """Register new customer with email validation"""


@mutation(
    name="updateCustomerProfile",
    function="update_customer_profile",
    description="Update customer profile information",
)
async def update_customer_profile(
    customer_id: UUID,
    first_name: Optional[str] = None,
    last_name: Optional[str] = None,
    phone: Optional[str] = None,
    metadata: Optional[dict] = None,
) -> CustomerMutationResult:
    """Update customer profile fields"""


@mutation(
    name="addCustomerAddress",
    function="add_customer_address",
    description="Add a new address to customer profile",
)
async def add_customer_address(
    customer_id: UUID,
    type: str,  # billing, shipping, both
    first_name: str,
    last_name: str,
    address_line1: str,
    city: str,
    country_code: str,
    company: Optional[str] = None,
    address_line2: Optional[str] = None,
    state_province: Optional[str] = None,
    postal_code: Optional[str] = None,
    phone: Optional[str] = None,
    is_default: bool = False,
) -> AddressMutationResult:
    """Add new address to customer account"""


# Wishlist Mutations
@mutation(
    name="addToWishlist",
    function="add_to_wishlist",
    description="Add a product to customer's wishlist",
)
async def add_to_wishlist(
    customer_id: UUID,
    product_id: UUID,
    variant_id: Optional[UUID] = None,
    wishlist_id: Optional[UUID] = None,
    priority: int = 0,
    notes: Optional[str] = None,
) -> dict:
    """Add product to wishlist"""


# Review Mutations
@mutation(
    name="submitReview",
    function="submit_review",
    description="Submit a product review",
)
async def submit_review(
    customer_id: UUID,
    product_id: UUID,
    rating: int,
    title: Optional[str] = None,
    comment: Optional[str] = None,
    order_id: Optional[UUID] = None,
) -> ReviewMutationResult:
    """Submit product review with optional order verification"""


@mutation(
    name="markReviewHelpful",
    function="mark_review_helpful",
    description="Mark a review as helpful or not helpful",
)
async def mark_review_helpful(
    review_id: UUID,
    is_helpful: bool,
    customer_id: Optional[UUID] = None,
    session_id: Optional[str] = None,
) -> dict:
    """Mark review helpfulness"""


# Enterprise Pattern Examples
# These demonstrate complex validation and cross-entity patterns

@mutation(function="app.process_order")
class ProcessOrderEnterprise:
    """Process order with comprehensive validation.

    Enterprise features demonstrated:
    - Cross-entity validation (inventory, customer credit, pricing)
    - Multi-step transaction with rollback handling
    - NOOP for business rule violations (insufficient inventory, credit limits)
    - Financial audit trails with precise change tracking
    - Real-time inventory reservation and payment processing
    """
    input: ProcessOrderInput
    success: ProcessOrderSuccess
    error: ProcessOrderError
    noop: ProcessOrderNoop  # For inventory/business rule issues


@mutation(function="app.update_inventory")
class UpdateInventoryEnterprise:
    """Update inventory with business rules.

    Validation layers:
    1. GraphQL: Type validation, required fields
    2. App layer: Basic bounds checking, format validation
    3. Core layer: Business rules, cross-entity consistency
    4. Database: Constraint validation, transaction integrity

    NOOP scenarios:
    - No actual quantity changes detected
    - Inventory levels would violate business rules
    - Concurrent modification conflicts (optimistic locking)
    """
    input: UpdateInventoryInput
    success: UpdateInventorySuccess
    error: UpdateInventoryError
    noop: UpdateInventoryNoop  # For no-change scenarios


@mutation(function="app.apply_discount")
class ApplyDiscountEnterprise:
    """Apply discount with eligibility validation.

    Complex validation example:
    - Customer eligibility (membership tier, purchase history)
    - Product eligibility (category restrictions, brand exclusions)
    - Temporal validation (valid date ranges, usage limits)
    - Quantity validation (minimum purchase requirements)
    - Cross-promotion conflicts (stackable vs exclusive discounts)
    """
    input: ApplyDiscountInput
    success: ApplyDiscountSuccess
    error: ApplyDiscountError
    noop: ApplyDiscountNoop  # For ineligible customers/products


# Legacy Pattern Examples (for comparison)
# Note: These show the old way. Use Enterprise classes above for new code.

async def process_order_legacy(
    info,
    cart_id: UUID,
    customer_id: UUID,
    payment_details: dict
) -> OrderMutationResult:
    """Legacy pattern - for comparison only.

    This shows the old resolver-based approach without:
    - Structured NOOP handling
    - Multi-layer validation
    - Comprehensive audit trails
    - Cross-entity validation patterns

    Compare with ProcessOrderEnterprise above to see the difference.
    """
    # Implementation would be similar to create_order function above


# Enterprise Pattern Type Definitions
# These would typically be in models.py but shown here for demonstration

@input
class ProcessOrderInput:
    """Order processing with comprehensive validation."""
    cart_id: UUID
    customer_id: UUID
    shipping_address_id: UUID
    billing_address_id: Optional[UUID] = None
    payment_details: dict[str, Any]
    coupon_codes: Optional[list[str]] = None
    special_instructions: Optional[str] = None

    # Enterprise validation metadata
    _expected_total: Optional[Decimal] = None  # For price validation
    _inventory_reserved_until: Optional[datetime] = None  # For inventory checks


@success
class ProcessOrderSuccess:
    """Order processed successfully."""
    order_id: UUID
    order_number: str
    total_amount: Decimal
    payment_status: str
    estimated_delivery: Optional[datetime] = None

    # Enterprise audit information
    inventory_adjustments: list[dict[str, Any]]
    payment_transaction_id: str
    applied_discounts: list[dict[str, Any]]
    audit_trail: dict[str, Any]


@success
class ProcessOrderNoop:
    """Order processing was a no-op."""
    reason: str
    order_id: Optional[UUID] = None
    blocking_issues: list[dict[str, Any]]

    # Context for NOOP scenarios
    inventory_shortfalls: Optional[list[dict[str, Any]]] = None
    credit_limit_exceeded: Optional[dict[str, Any]] = None
    pricing_discrepancies: Optional[list[dict[str, Any]]] = None


@failure
class ProcessOrderError:
    """Order processing failed."""
    message: str
    error_code: str
    field_errors: Optional[dict[str, str]] = None

    # Enterprise error context
    validation_failures: list[dict[str, Any]]
    transaction_rollback_info: dict[str, Any]
    affected_entities: list[dict[str, Any]]


@input
class UpdateInventoryInput:
    """Inventory update with business rules."""
    product_variant_id: UUID
    quantity_change: int  # Can be negative
    reason_code: str  # 'restock', 'sale', 'damage', 'adjustment'
    reference_id: Optional[UUID] = None  # Order ID, return ID, etc.
    notes: Optional[str] = None

    # Enterprise validation
    _expected_current_quantity: Optional[int] = None  # For optimistic locking
    _force_negative: bool = False  # Allow negative inventory


@success
class UpdateInventorySuccess:
    """Inventory updated successfully."""
    product_variant_id: UUID
    previous_quantity: int
    new_quantity: int
    quantity_change: int

    # Business context
    reorder_point_triggered: bool = False
    low_stock_alert_sent: bool = False
    audit_trail: dict[str, Any]


@success
class UpdateInventoryNoop:
    """Inventory update was a no-op."""
    reason: str
    product_variant_id: UUID
    current_quantity: int
    attempted_change: int

    # NOOP context
    business_rule_violation: Optional[str] = None
    concurrent_modification: Optional[dict[str, Any]] = None


@failure
class UpdateInventoryError:
    """Inventory update failed."""
    message: str
    error_code: str
    product_variant_id: UUID

    # Detailed error context
    validation_failures: list[str]
    business_rule_violations: list[str]
    system_constraints: list[str]


@input
class ApplyDiscountInput:
    """Discount application with eligibility rules."""
    cart_id: UUID
    discount_code: Optional[str] = None
    discount_id: Optional[UUID] = None
    customer_id: UUID

    # Validation context
    _cart_total_for_validation: Optional[Decimal] = None
    _customer_tier: Optional[str] = None


@success
class ApplyDiscountSuccess:
    """Discount applied successfully."""
    discount_id: UUID
    discount_amount: Decimal
    discount_percentage: Optional[Decimal] = None
    cart_total_before: Decimal
    cart_total_after: Decimal

    # Discount details
    discount_rules_applied: list[dict[str, Any]]
    expiry_info: dict[str, Any]


@success
class ApplyDiscountNoop:
    """Discount application was a no-op."""
    reason: str
    discount_code: Optional[str] = None
    customer_id: UUID

    # Eligibility context
    customer_ineligible_reasons: list[str]
    product_restrictions: list[dict[str, Any]]
    temporal_restrictions: Optional[dict[str, Any]] = None


@failure
class ApplyDiscountError:
    """Discount application failed."""
    message: str
    error_code: str
    discount_code: Optional[str] = None

    # Error details
    eligibility_failures: list[str]
    system_errors: list[str]
    validation_context: dict[str, Any]
