"""Strawberry GraphQL schema with SQLAlchemy models"""

from datetime import datetime
from decimal import Decimal
from typing import Optional
from uuid import UUID

from aiodataloader import DataLoader
from models import (
    Category as CategoryModel,
)
from models import (
    Order as OrderModel,
)
from models import (
    OrderItem as OrderItemModel,
)
from models import (
    Product as ProductModel,
)
from models import (
    Review as ReviewModel,
)
from models import (
    User as UserModel,
)
from sqlalchemy import func, select
from sqlalchemy.orm import selectinload

import strawberry


# Strawberry types
@strawberry.type
class Category:
    id: UUID
    name: str
    slug: str
    description: Optional[str]
    parent_id: Optional[UUID]

    @classmethod
    def from_model(cls, model: CategoryModel) -> "Category":
        return cls(
            id=model.id,
            name=model.name,
            slug=model.slug,
            description=model.description,
            parent_id=model.parent_id,
        )


@strawberry.type
class User:
    id: UUID
    email: str
    username: str
    full_name: str
    created_at: datetime
    is_active: bool

    @strawberry.field
    async def order_count(self, info) -> int:
        """Get order count for user"""
        session = info.context["session"]
        result = await session.execute(
            select(func.count(OrderModel.id)).where(OrderModel.user_id == self.id)
        )
        return result.scalar() or 0

    @strawberry.field
    async def total_spent(self, info) -> Decimal:
        """Get total amount spent by user"""
        session = info.context["session"]
        result = await session.execute(
            select(func.sum(OrderModel.total_amount)).where(OrderModel.user_id == self.id)
        )
        return result.scalar() or Decimal("0.00")

    @strawberry.field
    async def review_count(self, info) -> int:
        """Get review count for user"""
        session = info.context["session"]
        result = await session.execute(
            select(func.count(ReviewModel.id)).where(ReviewModel.user_id == self.id)
        )
        return result.scalar() or 0

    @strawberry.field
    async def average_rating(self, info) -> Optional[float]:
        """Get average rating given by user"""
        session = info.context["session"]
        result = await session.execute(
            select(func.avg(ReviewModel.rating)).where(ReviewModel.user_id == self.id)
        )
        avg = result.scalar()
        return float(avg) if avg else None

    @classmethod
    def from_model(cls, model: UserModel) -> "User":
        return cls(
            id=model.id,
            email=model.email,
            username=model.username,
            full_name=model.full_name,
            created_at=model.created_at,
            is_active=model.is_active,
        )


@strawberry.type
class ProductReviewUser:
    id: UUID
    username: str
    full_name: str

    @classmethod
    def from_model(cls, model: UserModel) -> "ProductReviewUser":
        return cls(id=model.id, username=model.username, full_name=model.full_name)


@strawberry.type
class Review:
    id: UUID
    rating: int
    title: Optional[str]
    comment: Optional[str]
    created_at: datetime
    user: ProductReviewUser

    @classmethod
    def from_model(cls, model: ReviewModel, user: UserModel) -> "Review":
        return cls(
            id=model.id,
            rating=model.rating,
            title=model.title,
            comment=model.comment,
            created_at=model.created_at,
            user=ProductReviewUser.from_model(user),
        )


@strawberry.type
class Product:
    id: UUID
    sku: str
    name: str
    description: Optional[str]
    price: Decimal
    stock_quantity: int
    category_id: Optional[UUID]

    @strawberry.field
    async def category(self, info) -> Optional[Category]:
        """Get product category"""
        if not self.category_id:
            return None

        loader = info.context["category_loader"]
        category = await loader.load(self.category_id)
        return Category.from_model(category) if category else None

    @strawberry.field
    async def average_rating(self, info) -> Optional[float]:
        """Get average product rating"""
        session = info.context["session"]
        result = await session.execute(
            select(func.avg(ReviewModel.rating)).where(ReviewModel.product_id == self.id)
        )
        avg = result.scalar()
        return float(avg) if avg else None

    @strawberry.field
    async def review_count(self, info) -> int:
        """Get review count for product"""
        session = info.context["session"]
        result = await session.execute(
            select(func.count(ReviewModel.id)).where(ReviewModel.product_id == self.id)
        )
        return result.scalar() or 0

    @strawberry.field
    async def reviews(self, info, limit: int = 10) -> list[Review]:
        """Get product reviews with limit"""
        session = info.context["session"]
        result = await session.execute(
            select(ReviewModel)
            .where(ReviewModel.product_id == self.id)
            .options(selectinload(ReviewModel.user))
            .order_by(ReviewModel.created_at.desc())
            .limit(limit)
        )
        reviews = result.scalars().all()
        return [Review.from_model(r, r.user) for r in reviews]

    @classmethod
    def from_model(cls, model: ProductModel) -> "Product":
        return cls(
            id=model.id,
            sku=model.sku,
            name=model.name,
            description=model.description,
            price=model.price,
            stock_quantity=model.stock_quantity,
            category_id=model.category_id,
        )


@strawberry.type
class OrderProduct:
    id: UUID
    sku: str
    name: str
    price: Decimal

    @classmethod
    def from_model(cls, model: ProductModel) -> "OrderProduct":
        return cls(id=model.id, sku=model.sku, name=model.name, price=model.price)


@strawberry.type
class OrderItem:
    id: UUID
    quantity: int
    unit_price: Decimal
    total_price: Decimal
    product: OrderProduct

    @classmethod
    def from_model(cls, model: OrderItemModel, product: ProductModel) -> "OrderItem":
        return cls(
            id=model.id,
            quantity=model.quantity,
            unit_price=model.unit_price,
            total_price=model.total_price,
            product=OrderProduct.from_model(product),
        )


@strawberry.type
class OrderUser:
    id: UUID
    email: str
    username: str
    full_name: str

    @classmethod
    def from_model(cls, model: UserModel) -> "OrderUser":
        return cls(
            id=model.id, email=model.email, username=model.username, full_name=model.full_name
        )


@strawberry.type
class Order:
    id: UUID
    order_number: str
    user_id: UUID
    status: str
    total_amount: Decimal
    created_at: datetime

    @strawberry.field
    async def user(self, info) -> OrderUser:
        """Get order user using DataLoader"""
        loader = info.context["user_loader"]
        user = await loader.load(self.user_id)
        return OrderUser.from_model(user)

    @strawberry.field
    async def items(self, info) -> list[OrderItem]:
        """Get order items"""
        session = info.context["session"]
        result = await session.execute(
            select(OrderItemModel)
            .where(OrderItemModel.order_id == self.id)
            .options(selectinload(OrderItemModel.product))
            .order_by(OrderItemModel.created_at)
        )
        items = result.scalars().all()
        return [OrderItem.from_model(item, item.product) for item in items]

    @strawberry.field
    async def item_count(self, info) -> int:
        """Get item count for order"""
        session = info.context["session"]
        result = await session.execute(
            select(func.count(OrderItemModel.id)).where(OrderItemModel.order_id == self.id)
        )
        return result.scalar() or 0

    @classmethod
    def from_model(cls, model: OrderModel) -> "Order":
        return cls(
            id=model.id,
            order_number=model.order_number,
            user_id=model.user_id,
            status=model.status,
            total_amount=model.total_amount,
            created_at=model.created_at,
        )


# Input types
@strawberry.input
class ProductFilterInput:
    category_id: Optional[UUID] = None
    min_price: Optional[Decimal] = None
    max_price: Optional[Decimal] = None
    in_stock: Optional[bool] = None
    search: Optional[str] = None


@strawberry.input
class OrderFilterInput:
    user_id: Optional[UUID] = None
    status: Optional[str] = None
    created_after: Optional[datetime] = None
    created_before: Optional[datetime] = None


@strawberry.input
class PaginationInput:
    limit: int = 20
    offset: int = 0


@strawberry.input
class OrderByInput:
    field: str
    direction: str = "ASC"


# DataLoader factories
def create_user_loader(session):
    async def batch_load_users(user_ids: list[UUID]) -> list[Optional[UserModel]]:
        result = await session.execute(select(UserModel).where(UserModel.id.in_(user_ids)))
        users_by_id = {user.id: user for user in result.scalars()}
        return [users_by_id.get(uid) for uid in user_ids]

    return DataLoader(batch_load_users)


def create_category_loader(session):
    async def batch_load_categories(category_ids: list[UUID]) -> list[Optional[CategoryModel]]:
        result = await session.execute(
            select(CategoryModel).where(CategoryModel.id.in_(category_ids))
        )
        categories_by_id = {cat.id: cat for cat in result.scalars()}
        return [categories_by_id.get(cid) for cid in category_ids]

    return DataLoader(batch_load_categories)


def create_product_loader(session):
    async def batch_load_products(product_ids: list[UUID]) -> list[Optional[ProductModel]]:
        result = await session.execute(select(ProductModel).where(ProductModel.id.in_(product_ids)))
        products_by_id = {prod.id: prod for prod in result.scalars()}
        return [products_by_id.get(pid) for pid in product_ids]

    return DataLoader(batch_load_products)
