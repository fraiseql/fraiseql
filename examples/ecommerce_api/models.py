"""E-commerce API Models

Demonstrates FraiseQL's type system with complex e-commerce entities
"""

from datetime import datetime
from decimal import Decimal
from typing import Any, Dict, List, Optional
from uuid import UUID

from pydantic import BaseModel, Field

from fraiseql import QueryType, register_type


# Base Types
class Category(BaseModel):
    id: UUID
    name: str
    slug: str
    description: Optional[str] = None
    parent_id: Optional[UUID] = None
    image_url: Optional[str] = None
    is_active: bool = True
    created_at: datetime
    updated_at: datetime


class ProductImage(BaseModel):
    id: UUID
    url: str
    alt_text: Optional[str] = None
    position: int = 0
    is_primary: bool = False


class ProductVariant(BaseModel):
    id: UUID
    sku: str
    name: str
    price: Decimal
    compare_at_price: Optional[Decimal] = None
    attributes: Dict[str, Any] = Field(default_factory=dict)
    inventory: Optional[Dict[str, int]] = None


class Product(BaseModel):
    id: UUID
    sku: str
    name: str
    slug: str
    description: Optional[str] = None
    short_description: Optional[str] = None
    category_id: Optional[UUID] = None
    brand: Optional[str] = None
    tags: List[str] = Field(default_factory=list)
    is_active: bool = True
    is_featured: bool = False
    created_at: datetime
    updated_at: datetime


# Enhanced Product Views
class ProductSearch(Product):
    category_name: Optional[str] = None
    category_slug: Optional[str] = None
    min_price: Optional[Decimal] = None
    max_price: Optional[Decimal] = None
    in_stock: bool = False
    total_inventory: int = 0
    review_count: int = 0
    average_rating: Optional[Decimal] = None
    primary_image_url: Optional[str] = None


class ProductDetail(Product):
    category: Optional[Dict[str, Any]] = None
    images: List[Dict[str, Any]] = Field(default_factory=list)
    variants: List[Dict[str, Any]] = Field(default_factory=list)
    review_summary: Dict[str, Any] = Field(default_factory=dict)


# Category Views
class CategoryTree(Category):
    level: int = 0
    path: List[UUID] = Field(default_factory=list)
    full_path: str = ""
    product_count: int = 0
    subcategories: List[Dict[str, Any]] = Field(default_factory=list)


# Customer Types
class Customer(BaseModel):
    id: UUID
    email: str
    first_name: Optional[str] = None
    last_name: Optional[str] = None
    phone: Optional[str] = None
    is_verified: bool = False
    is_active: bool = True
    tags: List[str] = Field(default_factory=list)
    metadata: Dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


class Address(BaseModel):
    id: UUID
    customer_id: UUID
    type: str  # billing, shipping, both
    first_name: str
    last_name: str
    company: Optional[str] = None
    address_line1: str
    address_line2: Optional[str] = None
    city: str
    state_province: Optional[str] = None
    postal_code: Optional[str] = None
    country_code: str
    phone: Optional[str] = None
    is_default: bool = False
    created_at: datetime
    updated_at: datetime


# Cart Types
class Cart(BaseModel):
    id: UUID
    customer_id: Optional[UUID] = None
    session_id: Optional[str] = None
    status: str = "active"
    expires_at: datetime
    metadata: Dict[str, Any] = Field(default_factory=dict)
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
    customer: Optional[Dict[str, Any]] = None
    items: List[Dict[str, Any]] = Field(default_factory=list)
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
    shipping_address_id: Optional[UUID] = None
    billing_address_id: Optional[UUID] = None
    notes: Optional[str] = None
    metadata: Dict[str, Any] = Field(default_factory=dict)
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
    customer: Dict[str, Any]
    shipping_address: Optional[Dict[str, Any]] = None
    billing_address: Optional[Dict[str, Any]] = None
    items: List[Dict[str, Any]] = Field(default_factory=list)


# Review Types
class Review(BaseModel):
    id: UUID
    product_id: UUID
    customer_id: UUID
    order_id: Optional[UUID] = None
    rating: int
    title: Optional[str] = None
    comment: Optional[str] = None
    is_verified_purchase: bool = False
    is_featured: bool = False
    helpful_count: int = 0
    not_helpful_count: int = 0
    status: str = "pending"
    created_at: datetime
    updated_at: datetime


class ProductReview(Review):
    customer: Dict[str, Any]
    product: Dict[str, Any]
    helpfulness_ratio: Optional[float] = None


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
    variant_id: Optional[UUID] = None
    priority: int = 0
    notes: Optional[str] = None
    created_at: datetime


class CustomerWishlist(Wishlist):
    item_count: int = 0
    items: List[Dict[str, Any]] = Field(default_factory=list)


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
    warehouse_location: Optional[str] = None
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
    description: Optional[str] = None
    discount_type: str  # percentage, fixed_amount
    discount_value: Decimal
    minimum_purchase_amount: Optional[Decimal] = None
    usage_limit: Optional[int] = None
    usage_count: int = 0
    customer_usage_limit: int = 1
    valid_from: datetime
    valid_until: Optional[datetime] = None
    is_active: bool = True
    applies_to: Dict[str, Any] = Field(default_factory=dict)
    created_at: datetime
    updated_at: datetime


# Customer Profile View
class CustomerProfile(Customer):
    total_orders: int = 0
    completed_orders: int = 0
    lifetime_value: Decimal = Decimal("0.00")
    last_order_date: Optional[datetime] = None
    address_count: int = 0
    wishlist_count: int = 0
    wishlist_items_count: int = 0
    review_count: int = 0
    average_rating_given: Optional[Decimal] = None
    has_active_cart: bool = False


# Mutation Result Types
class MutationResult(BaseModel):
    success: bool
    message: Optional[str] = None
    error: Optional[str] = None


class CartMutationResult(MutationResult):
    cart_id: Optional[UUID] = None
    cart_item_id: Optional[UUID] = None
    cart: Optional[Dict[str, Any]] = None


class OrderMutationResult(MutationResult):
    order_id: Optional[UUID] = None
    order_number: Optional[str] = None
    total_amount: Optional[Decimal] = None
    order: Optional[Dict[str, Any]] = None


class CustomerMutationResult(MutationResult):
    customer_id: Optional[UUID] = None
    customer: Optional[Dict[str, Any]] = None


class AddressMutationResult(MutationResult):
    address_id: Optional[UUID] = None


class ReviewMutationResult(MutationResult):
    review_id: Optional[UUID] = None
    is_verified_purchase: Optional[bool] = None


# Register all types with FraiseQL
@register_type
class EcommerceQuery(QueryType):
    # Product queries
    products: List[Product]
    product_search: List[ProductSearch]
    product_detail: List[ProductDetail]
    featured_products: List[Product]
    best_sellers: List[Product]
    related_products: List[Product]

    # Category queries
    categories: List[Category]
    category_tree: List[CategoryTree]

    # Customer queries
    customers: List[Customer]
    customer_profile: List[CustomerProfile]
    customer_addresses: List[Address]
    customer_orders: List[Order]
    customer_wishlists: List[CustomerWishlist]

    # Cart queries
    shopping_cart: List[ShoppingCart]

    # Order queries
    orders: List[Order]
    order_detail: List[OrderDetail]
    order_analytics: List[OrderAnalytics]

    # Review queries
    product_reviews: List[ProductReview]

    # Inventory queries
    inventory_alerts: List[InventoryAlert]

    # Coupon queries
    coupons: List[Coupon]
