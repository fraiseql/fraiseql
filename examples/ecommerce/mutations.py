"""E-commerce mutations for FraiseQL example."""

import fraiseql
from fraiseql.mutations import mutation

from .models import (
    AddressError,
    AddressSuccess,
    AddToCartInput,
    AuthError,
    # Success/Error types
    AuthSuccess,
    CartError,
    CartSuccess,
    CheckoutInput,
    CreateAddressInput,
    CreateReviewInput,
    LoginInput,
    OrderError,
    OrderSuccess,
    # Inputs
    RegisterInput,
    ReviewError,
    ReviewSuccess,
    UpdateCartItemInput,
)

# Authentication mutations


@mutation
class Register:
    """Register a new user account."""

    input: RegisterInput
    success: AuthSuccess
    error: AuthError


@mutation
class Login:
    """Login to existing account."""

    input: LoginInput
    success: AuthSuccess
    error: AuthError


# Cart mutations


@mutation
class AddToCart:
    """Add product to shopping cart."""

    input: AddToCartInput
    success: CartSuccess
    error: CartError


@mutation
class UpdateCartItem:
    """Update quantity of cart item."""

    input: UpdateCartItemInput
    success: CartSuccess
    error: CartError


@mutation
class RemoveFromCart:
    """Remove item from cart."""

    input: UpdateCartItemInput  # Only need cart_item_id
    success: CartSuccess
    error: CartError


@mutation
class ClearCart:
    """Clear all items from cart."""

    success: CartSuccess
    error: CartError


# Order mutations


@mutation
class Checkout:
    """Complete checkout and create order."""

    input: CheckoutInput
    success: OrderSuccess
    error: OrderError


@mutation
class CancelOrder:
    """Cancel an order."""

    input: fraiseql.input(lambda: CancelOrderInput)
    success: OrderSuccess
    error: OrderError


# Address mutations


@mutation
class CreateAddress:
    """Create a new address."""

    input: CreateAddressInput
    success: AddressSuccess
    error: AddressError


@mutation
class UpdateAddress:
    """Update existing address."""

    input: fraiseql.input(lambda: UpdateAddressInput)
    success: AddressSuccess
    error: AddressError


@mutation
class DeleteAddress:
    """Delete an address."""

    input: fraiseql.input(lambda: DeleteAddressInput)
    success: AddressSuccess
    error: AddressError


# Review mutations


@mutation
class CreateReview:
    """Create a product review."""

    input: CreateReviewInput
    success: ReviewSuccess
    error: ReviewError


# Additional input types referenced above


@fraiseql.input
class CancelOrderInput:
    """Cancel order input."""

    order_id: fraiseql.UUID
    reason: fraiseql.str | None = None


@fraiseql.input
class UpdateAddressInput:
    """Update address input."""

    address_id: fraiseql.UUID
    label: fraiseql.str | None = None
    street1: fraiseql.str | None = None
    street2: fraiseql.str | None = None
    city: fraiseql.str | None = None
    state: fraiseql.str | None = None
    postal_code: fraiseql.str | None = None
    country: fraiseql.str | None = None
    is_default: fraiseql.bool | None = None


@fraiseql.input
class DeleteAddressInput:
    """Delete address input."""

    address_id: fraiseql.UUID
