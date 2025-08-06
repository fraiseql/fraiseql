"""SQLAlchemy models for benchmark testing"""

from sqlalchemy import Boolean, Column, DateTime, ForeignKey, Integer, Numeric, String, Text, func
from sqlalchemy.dialects.postgresql import JSONB
from sqlalchemy.dialects.postgresql import UUID as PGUUID
from sqlalchemy.ext.asyncio import AsyncAttrs, async_sessionmaker, create_async_engine
from sqlalchemy.ext.declarative import declarative_base
from sqlalchemy.orm import relationship

Base = declarative_base()


class User(Base, AsyncAttrs):
    __tablename__ = "users"
    __table_args__ = {"schema": "benchmark"}

    id = Column(PGUUID(as_uuid=True), primary_key=True)
    email = Column(String(255), unique=True, nullable=False)
    username = Column(String(100), unique=True, nullable=False)
    full_name = Column(String(255), nullable=False)
    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), server_default=func.now(), onupdate=func.now())
    is_active = Column(Boolean, default=True)
    extra_data = Column("metadata", JSONB, default={})

    # Relationships
    orders = relationship("Order", back_populates="user", lazy="select")
    reviews = relationship("Review", back_populates="user", lazy="select")
    cart_items = relationship("CartItem", back_populates="user", lazy="select")
    addresses = relationship("Address", back_populates="user", lazy="select")


class Category(Base, AsyncAttrs):
    __tablename__ = "categories"
    __table_args__ = {"schema": "benchmark"}

    id = Column(PGUUID(as_uuid=True), primary_key=True)
    name = Column(String(100), nullable=False)
    slug = Column(String(100), unique=True, nullable=False)
    description = Column(Text)
    parent_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.categories.id"))
    created_at = Column(DateTime(timezone=True), server_default=func.now())
    extra_data = Column("metadata", JSONB, default={})

    # Relationships
    parent = relationship("Category", remote_side=[id], lazy="select")
    products = relationship("Product", back_populates="category", lazy="select")


class Product(Base, AsyncAttrs):
    __tablename__ = "products"
    __table_args__ = {"schema": "benchmark"}

    id = Column(PGUUID(as_uuid=True), primary_key=True)
    sku = Column(String(100), unique=True, nullable=False)
    name = Column(String(255), nullable=False)
    description = Column(Text)
    price = Column(Numeric(10, 2), nullable=False)
    stock_quantity = Column(Integer, nullable=False, default=0)
    category_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.categories.id"))
    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), server_default=func.now(), onupdate=func.now())
    is_active = Column(Boolean, default=True)
    extra_data = Column("metadata", JSONB, default={})

    # Relationships
    category = relationship("Category", back_populates="products", lazy="select")
    reviews = relationship("Review", back_populates="product", lazy="select")
    order_items = relationship("OrderItem", back_populates="product", lazy="select")


class Order(Base, AsyncAttrs):
    __tablename__ = "orders"
    __table_args__ = {"schema": "benchmark"}

    id = Column(PGUUID(as_uuid=True), primary_key=True)
    order_number = Column(String(50), unique=True, nullable=False)
    user_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.users.id"), nullable=False)
    status = Column(String(50), nullable=False, default="pending")
    total_amount = Column(Numeric(10, 2), nullable=False)
    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), server_default=func.now(), onupdate=func.now())
    shipped_at = Column(DateTime(timezone=True))
    delivered_at = Column(DateTime(timezone=True))
    extra_data = Column("metadata", JSONB, default={})

    # Relationships
    user = relationship("User", back_populates="orders", lazy="select")
    items = relationship("OrderItem", back_populates="order", lazy="select")


class OrderItem(Base, AsyncAttrs):
    __tablename__ = "order_items"
    __table_args__ = {"schema": "benchmark"}

    id = Column(PGUUID(as_uuid=True), primary_key=True)
    order_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.orders.id"), nullable=False)
    product_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.products.id"), nullable=False)
    quantity = Column(Integer, nullable=False)
    unit_price = Column(Numeric(10, 2), nullable=False)
    total_price = Column(Numeric(10, 2), nullable=False)
    created_at = Column(DateTime(timezone=True), server_default=func.now())

    # Relationships
    order = relationship("Order", back_populates="items", lazy="select")
    product = relationship("Product", back_populates="order_items", lazy="select")


class Review(Base, AsyncAttrs):
    __tablename__ = "reviews"
    __table_args__ = {"schema": "benchmark"}

    id = Column(PGUUID(as_uuid=True), primary_key=True)
    product_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.products.id"), nullable=False)
    user_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.users.id"), nullable=False)
    rating = Column(Integer, nullable=False)
    title = Column(String(255))
    comment = Column(Text)
    created_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), server_default=func.now(), onupdate=func.now())
    is_verified_purchase = Column(Boolean, default=False)
    helpful_count = Column(Integer, default=0)

    # Relationships
    product = relationship("Product", back_populates="reviews", lazy="select")
    user = relationship("User", back_populates="reviews", lazy="select")


class CartItem(Base, AsyncAttrs):
    __tablename__ = "cart_items"
    __table_args__ = {"schema": "benchmark"}

    id = Column(PGUUID(as_uuid=True), primary_key=True)
    user_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.users.id"), nullable=False)
    product_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.products.id"), nullable=False)
    quantity = Column(Integer, nullable=False)
    added_at = Column(DateTime(timezone=True), server_default=func.now())
    updated_at = Column(DateTime(timezone=True), server_default=func.now(), onupdate=func.now())

    # Relationships
    user = relationship("User", back_populates="cart_items", lazy="select")
    product = relationship("Product", lazy="select")


class Address(Base, AsyncAttrs):
    __tablename__ = "addresses"
    __table_args__ = {"schema": "benchmark"}

    id = Column(PGUUID(as_uuid=True), primary_key=True)
    user_id = Column(PGUUID(as_uuid=True), ForeignKey("benchmark.users.id"), nullable=False)
    type = Column(String(50), nullable=False, default="shipping")
    street_address = Column(String(255), nullable=False)
    city = Column(String(100), nullable=False)
    state_province = Column(String(100))
    postal_code = Column(String(20))
    country = Column(String(2), nullable=False)
    is_default = Column(Boolean, default=False)
    created_at = Column(DateTime(timezone=True), server_default=func.now())

    # Relationships
    user = relationship("User", back_populates="addresses", lazy="select")


# Database setup
async def init_db(database_url: str):
    """Initialize database connection"""
    engine = create_async_engine(
        database_url.replace("postgresql://", "postgresql+asyncpg://"),
        echo=False,
        pool_size=20,
        max_overflow=10,
        pool_pre_ping=True,
    )

    async_session = async_sessionmaker(
        engine,
        expire_on_commit=False,
        autocommit=False,
        autoflush=False,
    )

    return engine, async_session
