"""View mapping configuration for FraiseQL resolvers"""

# Map GraphQL field names to PostgreSQL view names
VIEW_MAPPINGS = {
    # QueryRoot fields
    "users": "v_users",  # List of users
    "user": "v_users",  # Single user (FraiseQL will add WHERE id = ?)
    "products": "v_products",  # List of products
    "product": "v_products",  # Single product
    "orders": "v_orders",  # List of orders
    "order": "v_orders",  # Single order
    "categories": "v_categories",  # List of categories
    # Nested fields (if needed for explicit mapping)
    "User.orders": "included_in_v_users",
    "Product.reviews": "included_in_v_products",
    "Order.orderItems": "included_in_v_orders",
}

# Field name mappings (GraphQL to Database)
FIELD_MAPPINGS = {
    # User fields
    "fullName": "full_name",
    "isActive": "is_active",
    "createdAt": "created_at",
    # Product fields
    "stockQuantity": "stock_quantity",
    "categoryId": "category_id",
    # Order fields
    "orderNumber": "order_number",
    "userId": "user_id",
    "totalAmount": "total_amount",
    "updatedAt": "updated_at",
    "shippedAt": "shipped_at",
    "deliveredAt": "delivered_at",
    # Order item fields
    "unitPrice": "unit_price",
    "totalPrice": "total_price",
    "productId": "product_id",
    "orderId": "order_id",
    # Review fields
    "isVerifiedPurchase": "is_verified_purchase",
    "helpfulCount": "helpful_count",
}

# Default view suffix pattern
DEFAULT_VIEW_PREFIX = "v_"
DEFAULT_VIEW_SUFFIX = ""  # Could be "_view" if needed
