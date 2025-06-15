"""FraiseQL mutations following the PostgreSQL function-based pattern."""

from typing import Optional

from models import Order, ProductReview, User

from fraiseql import failure, fraise_input, mutation, success

# ============================================================================
# CREATE USER MUTATION
# ============================================================================


@fraise_input
class CreateUserInput:
    """Input for creating a new user."""

    email: str
    username: str
    full_name: str
    password: Optional[str] = None


@success
class CreateUserSuccess:
    """Success response for user creation."""

    message: str
    user: User  # Will be instantiated from object_data


@failure
class CreateUserError:
    """Error response for user creation."""

    message: str
    existing_username: Optional[str] = None  # From extra_metadata
    suggested_username: Optional[str] = None  # From extra_metadata


@mutation
class CreateUser:
    """Create a new user account.

    This mutation calls api_create_user PostgreSQL function which:
    1. Validates input
    2. Calls core_create_user to handle business logic
    3. Updates table view projections via sync_refresh_user_projection
    4. Returns standardized mutation_result
    """

    input: CreateUserInput
    success: CreateUserSuccess
    error: CreateUserError


# ============================================================================
# CREATE ORDER MUTATION
# ============================================================================


@fraise_input
class OrderItemInput:
    """Input for order items."""

    product_id: str
    quantity: int


@fraise_input
class CreateOrderInput:
    """Input for creating a new order."""

    user_id: str
    items: list[OrderItemInput]


@success
class CreateOrderSuccess:
    """Success response for order creation."""

    message: str
    order: Order  # Will be instantiated from object_data


@failure
class CreateOrderError:
    """Error response for order creation."""

    message: str
    product_id: Optional[str] = None  # For product not found errors
    product_name: Optional[str] = None  # For stock errors
    available: Optional[int] = None  # Available stock
    requested: Optional[int] = None  # Requested quantity


@mutation
class CreateOrder:
    """Create a new order.

    This mutation calls api_create_order PostgreSQL function which:
    1. Validates the user and products exist
    2. Checks product stock availability
    3. Creates the order and order items
    4. Updates product stock quantities
    5. Updates table view projections
    """

    input: CreateOrderInput
    success: CreateOrderSuccess
    error: CreateOrderError


# ============================================================================
# ADD PRODUCT REVIEW MUTATION
# ============================================================================


@fraise_input
class AddProductReviewInput:
    """Input for adding a product review."""

    user_id: str
    product_id: str
    rating: int  # 1-5
    title: Optional[str] = None
    comment: Optional[str] = None


@success
class AddProductReviewSuccess:
    """Success response for adding a review."""

    message: str
    review: ProductReview  # Will be instantiated from object_data


@failure
class AddProductReviewError:
    """Error response for adding a review."""

    message: str
    existing_review_id: Optional[str] = None  # If user already reviewed


@mutation
class AddProductReview:
    """Add a product review.

    This mutation calls api_add_product_review PostgreSQL function which:
    1. Validates the rating is between 1-5
    2. Checks if user already reviewed this product
    3. Creates the review
    4. Updates product and user projections with new stats
    """

    input: AddProductReviewInput
    success: AddProductReviewSuccess
    error: AddProductReviewError


# Example GraphQL mutations that will work:
"""
# Create a new user
mutation {
    createUser(input: {
        email: "john.doe@example.com"
        username: "johndoe"
        fullName: "John Doe"
    }) {
        ... on CreateUserSuccess {
            message
            user {
                id
                email
                username
                fullName
                createdAt
            }
        }
        ... on CreateUserError {
            message
            existingUsername
            suggestedUsername
        }
    }
}

# Create an order
mutation {
    createOrder(input: {
        userId: "123e4567-e89b-12d3-a456-426614174000"
        items: [
            {productId: "456e7890-e89b-12d3-a456-426614174000", quantity: 2}
            {productId: "789e0123-e89b-12d3-a456-426614174000", quantity: 1}
        ]
    }) {
        ... on CreateOrderSuccess {
            message
            order {
                id
                totalAmount
                status
                createdAt
                items {
                    productName
                    quantity
                    unitPrice
                    totalPrice
                }
            }
        }
        ... on CreateOrderError {
            message
            productName
            available
            requested
        }
    }
}

# Add a product review
mutation {
    addProductReview(input: {
        userId: "123e4567-e89b-12d3-a456-426614174000"
        productId: "456e7890-e89b-12d3-a456-426614174000"
        rating: 5
        title: "Excellent product!"
        comment: "Really happy with this purchase."
    }) {
        ... on AddProductReviewSuccess {
            message
            review {
                id
                rating
                title
                comment
                createdAt
            }
        }
        ... on AddProductReviewError {
            message
            existingReviewId
        }
    }
}
"""
