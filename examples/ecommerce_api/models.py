"""E-commerce API Models

Demonstrates FraiseQL's type system with complex e-commerce entities
"""

from datetime import datetime
from decimal import Decimal
from typing import Any
from uuid import UUID

from pydantic import BaseModel, Field

from fraiseql import QueryType, register_type


# Base Types
class Category(BaseModel):
    id: UUID
    name: str
    slug: str
    description: str | None = None
    parent_id: UUID | None = None
    image_url: str | None = None
    is_active: bool = True
    created_at: datetime
    updated_at: datetime


class ProductImage(BaseModel):
    id: UUID
    url: str
    alt_text: str | None = None
    position: int = 0
    is_primary: bool = False


class ProductVariant(BaseModel):
    id: UUID
    sku: str
    name: str
    price: Decimal
    compare_at_price: Decimal | None = None
    attributes: dict[str, Any] = Field(default_factory=dict)
    inventory: dict[str, int] | None = None


class Product(BaseModel):
    id: UUID
    sku: str
    name: str
    slug: str
    description: str | None = None
    short_description: str | None = None
    category_id: UUID | None = None
    brand: str | None = None
    tags: list[str] = Field(default_factory=list)
    is_active: bool = True
    is_featured: bool = False
    created_at: datetime
    updated_at: datetime


# Enhanced Product Views
class ProductSearch(Product):
    category_name: str | None = None
    category_slug: str | None = None
    min_price: Decimal | None = None
    max_price: Decimal | None = None
    in_stock: bool = False
    total_inventory: int = 0
    review_count: int = 0
    average_rating: Decimal | None = None
    primary_image_url: str | None = None


class ProductDetail(Product):
    category: dict[str, Any] | None = None
    images: list[dict[str, Any]] = Field(default_factory=list)
    variants: list[dict[str, Any]] = Field(default_factory=list)
    review_summary: dict[str, Any] = Field(default_factory=dict)


# Category Views
class CategoryTree(Category):
    level: int = 0
    path: list[UUID] = Field(default_factory=list)
    full_path: str = ""
    product_count: int = 0
    subcategories: list[dict[str, Any]] = Field(default_factory=list)


# Customer Types
class Customer(BaseModel):
    id: UUID
    email: str
    first_name: str | None = None
    last_name: str | None = None
    phone: str | None = None
    is_verified: bool = False
    is_active: bool = True
    tags: list[str] = Field(default_factory=list)
    metadata: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class Address(BaseModel):
    id: UUID
    customer_id: UUID
    type: str  # billing, shipping, both
    first_name: str
    last_name: str
    company: str | None = None
    address_line1: str
    address_line2: str | None = None
    city: str
    state_province: str | None = None
    postal_code: str | None = None
    country_code: str
    phone: str | None = None
    is_default: bool = False
    created_at: datetime
    updated_at: datetime


# Cart Types
class Cart(BaseModel):
    id: UUID
    customer_id: UUID | None = None
    session_id: str | None = None
    status: str = "active"
    expires_at: datetime
    metadata: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class CartItem(BaseModel):
    id: UUID
    cart_id: UUID
    variant_id: UUID
    quantity: int
    price_at_time: Decimal
    created_at: datetime
    updated_at: datetime


# Shopping Cart View
class ShoppingCart(Cart):
    customer: dict[str, Any] | None = None
    items: list[dict[str, Any]] = Field(default_factory=list)
    item_count: int = 0
    total_quantity: int = 0
    subtotal: Decimal = Decimal("0.00")
    all_items_available: bool = True


# Order Types
class Order(BaseModel):
    id: UUID
    order_number: str
    customer_id: UUID
    status: str = "pending"
    subtotal: Decimal
    tax_amount: Decimal = Decimal("0.00")
    shipping_amount: Decimal = Decimal("0.00")
    discount_amount: Decimal = Decimal("0.00")
    total_amount: Decimal
    currency_code: str = "USD"
    payment_status: str = "pending"
    fulfillment_status: str = "unfulfilled"
    shipping_address_id: UUID | None = None
    billing_address_id: UUID | None = None
    notes: str | None = None
    metadata: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class OrderItem(BaseModel):
    id: UUID
    order_id: UUID
    variant_id: UUID
    quantity: int
    unit_price: Decimal
    total_price: Decimal
    discount_amount: Decimal = Decimal("0.00")
    tax_amount: Decimal = Decimal("0.00")
    created_at: datetime


# Order Detail View
class OrderDetail(Order):
    customer: dict[str, Any]
    shipping_address: dict[str, Any] | None = None
    billing_address: dict[str, Any] | None = None
    items: list[dict[str, Any]] = Field(default_factory=list)


# Review Types
class Review(BaseModel):
    id: UUID
    product_id: UUID
    customer_id: UUID
    order_id: UUID | None = None
    rating: int
    title: str | None = None
    comment: str | None = None
    is_verified_purchase: bool = False
    is_featured: bool = False
    helpful_count: int = 0
    not_helpful_count: int = 0
    status: str = "pending"
    created_at: datetime
    updated_at: datetime


class ProductReview(Review):
    customer: dict[str, Any]
    product: dict[str, Any]
    helpfulness_ratio: float | None = None


# Wishlist Types
class Wishlist(BaseModel):
    id: UUID
    customer_id: UUID
    name: str = "My Wishlist"
    is_public: bool = False
    created_at: datetime
    updated_at: datetime


class WishlistItem(BaseModel):
    id: UUID
    wishlist_id: UUID
    product_id: UUID
    variant_id: UUID | None = None
    priority: int = 0
    notes: str | None = None
    created_at: datetime


class CustomerWishlist(Wishlist):
    item_count: int = 0
    items: list[dict[str, Any]] = Field(default_factory=list)


# Analytics Types
class OrderAnalytics(BaseModel):
    order_date: datetime
    order_count: int
    unique_customers: int
    revenue: Decimal
    average_order_value: Decimal
    subtotal: Decimal
    tax_collected: Decimal
    shipping_collected: Decimal
    discounts_given: Decimal
    completed_orders: int
    cancelled_orders: int
    paid_orders: int


# Inventory Types
class InventoryAlert(BaseModel):
    id: UUID
    variant_id: UUID
    quantity: int
    reserved_quantity: int
    warehouse_location: str | None = None
    low_stock_threshold: int = 10
    updated_at: datetime
    variant_sku: str
    variant_name: str
    product_id: UUID
    product_name: str
    product_sku: str
    available_quantity: int
    stock_status: str  # out_of_stock, low_stock, in_stock


# Coupon Types
class Coupon(BaseModel):
    id: UUID
    code: str
    description: str | None = None
    discount_type: str  # percentage, fixed_amount
    discount_value: Decimal
    minimum_purchase_amount: Decimal | None = None
    usage_limit: int | None = None
    usage_count: int = 0
    customer_usage_limit: int = 1
    valid_from: datetime
    valid_until: datetime | None = None
    is_active: bool = True
    applies_to: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


# Customer Profile View
class CustomerProfile(Customer):
    total_orders: int = 0
    completed_orders: int = 0
    lifetime_value: Decimal = Decimal("0.00")
    last_order_date: datetime | None = None
    address_count: int = 0
    wishlist_count: int = 0
    wishlist_items_count: int = 0
    review_count: int = 0
    average_rating_given: Decimal | None = None
    has_active_cart: bool = False


# Mutation Result Types
class MutationResult(BaseModel):
    success: bool
    message: str | None = None
    error: str | None = None


class CartMutationResult(MutationResult):
    cart_id: UUID | None = None
    cart_item_id: UUID | None = None
    cart: dict[str, Any] | None = None


class OrderMutationResult(MutationResult):
    order_id: UUID | None = None
    order_number: str | None = None
    total_amount: Decimal | None = None
    order: dict[str, Any] | None = None


class CustomerMutationResult(MutationResult):
    customer_id: UUID | None = None
    customer: dict[str, Any] | None = None


class AddressMutationResult(MutationResult):
    address_id: UUID | None = None


class ReviewMutationResult(MutationResult):
    review_id: UUID | None = None
    is_verified_purchase: bool | None = None


# Register all types with FraiseQL
@register_type
class EcommerceQuery(QueryType):
    # Product queries
    products: list[Product]
    product_search: list[ProductSearch]
    product_detail: list[ProductDetail]
    featured_products: list[Product]
    best_sellers: list[Product]
    related_products: list[Product]

    # Category queries
    categories: list[Category]
    category_tree: list[CategoryTree]

    # Customer queries
    customers: list[Customer]
    customer_profile: list[CustomerProfile]
    customer_addresses: list[Address]
    customer_orders: list[Order]
    customer_wishlists: list[CustomerWishlist]

    # Cart queries
    shopping_cart: list[ShoppingCart]

    # Order queries
    orders: list[Order]
    order_detail: list[OrderDetail]
    order_analytics: list[OrderAnalytics]

    # Review queries
    product_reviews: list[ProductReview]

    # Inventory queries
    inventory_alerts: list[InventoryAlert]

    # Coupon queries
    coupons: list[Coupon]
