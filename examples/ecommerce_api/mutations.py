"""E-commerce API Mutations

Demonstrates FraiseQL's mutation system with complex business logic
"""

from typing import Optional
from uuid import UUID

from fraiseql import mutation

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
