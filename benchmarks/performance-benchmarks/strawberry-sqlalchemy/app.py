"""Strawberry GraphQL + SQLAlchemy benchmark application"""

import json
from contextlib import asynccontextmanager
from datetime import datetime
from pathlib import Path
from typing import Optional

from fastapi import FastAPI, Response
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
from models import (
    init_db,
)
from prometheus_client import Counter, Histogram, generate_latest
from pydantic_settings import BaseSettings
from schema import (
    Order,
    OrderByInput,
    OrderFilterInput,
    PaginationInput,
    Product,
    ProductFilterInput,
    User,
    create_category_loader,
    create_product_loader,
    create_user_loader,
)
from sqlalchemy import and_, func, or_, select
from sqlalchemy.orm import selectinload

import strawberry
from strawberry.fastapi import GraphQLRouter


class Settings(BaseSettings):
    database_url: str = "postgresql://benchmark:benchmark@postgres:5432/benchmark_db"
    enable_metrics: bool = True
    results_dir: str = "/app/results"


settings = Settings()

# Prometheus metrics
query_counter = Counter("graphql_queries_total", "Total GraphQL queries", ["operation", "type"])
query_histogram = Histogram(
    "graphql_query_duration_seconds", "GraphQL query duration", ["operation", "type"]
)
db_query_counter = Counter("database_queries_total", "Total database queries")
db_query_histogram = Histogram("database_query_duration_seconds", "Database query duration")

# Global database objects
engine = None
async_session = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Manage application lifecycle"""
    global engine, async_session

    # Initialize database
    engine, async_session = await init_db(settings.database_url)

    # Create results directory
    Path(settings.results_dir).mkdir(parents=True, exist_ok=True)

    yield

    # Cleanup
    await engine.dispose()


@strawberry.type
class Query:
    @strawberry.field
    async def users(self, info, limit: int = 20, offset: int = 0) -> list[User]:
        """Get all users with pagination"""
        with query_histogram.labels("users", "query").time():
            query_counter.labels("users", "query").inc()

            async with async_session() as session:
                result = await session.execute(
                    select(UserModel)
                    .order_by(UserModel.created_at.desc())
                    .limit(limit)
                    .offset(offset)
                )
                users = result.scalars().all()
                return [User.from_model(user) for user in users]

    @strawberry.field
    async def user(self, info, id: strawberry.ID) -> Optional[User]:
        """Get a single user by ID"""
        with query_histogram.labels("user", "query").time():
            query_counter.labels("user", "query").inc()

            async with async_session() as session:
                result = await session.execute(select(UserModel).where(UserModel.id == id))
                user = result.scalar_one_or_none()
                return User.from_model(user) if user else None

    @strawberry.field
    async def products(
        self,
        info,
        filter: Optional[ProductFilterInput] = None,
        pagination: Optional[PaginationInput] = None,
        order_by: Optional[OrderByInput] = None,
    ) -> list[Product]:
        """Get products with filtering, pagination, and sorting"""
        with query_histogram.labels("products", "query").time():
            query_counter.labels("products", "query").inc()

            async with async_session() as session:
                query = select(ProductModel)

                # Apply filters
                if filter:
                    conditions = []
                    if filter.category_id:
                        conditions.append(ProductModel.category_id == filter.category_id)
                    if filter.min_price is not None:
                        conditions.append(ProductModel.price >= filter.min_price)
                    if filter.max_price is not None:
                        conditions.append(ProductModel.price <= filter.max_price)
                    if filter.in_stock is not None:
                        if filter.in_stock:
                            conditions.append(ProductModel.stock_quantity > 0)
                        else:
                            conditions.append(ProductModel.stock_quantity == 0)
                    if filter.search:
                        conditions.append(ProductModel.name.ilike(f"%{filter.search}%"))

                    if conditions:
                        query = query.where(and_(*conditions))

                # Apply ordering
                if order_by:
                    field_map = {
                        "name": ProductModel.name,
                        "price": ProductModel.price,
                        "createdAt": ProductModel.created_at,
                        "stockQuantity": ProductModel.stock_quantity,
                    }
                    order_field = field_map.get(order_by.field, ProductModel.name)
                    if order_by.direction.upper() == "DESC":
                        query = query.order_by(order_field.desc())
                    else:
                        query = query.order_by(order_field)
                else:
                    query = query.order_by(ProductModel.name)

                # Apply pagination
                if pagination:
                    query = query.limit(pagination.limit).offset(pagination.offset)
                else:
                    query = query.limit(20)

                result = await session.execute(query)
                products = result.scalars().all()
                return [Product.from_model(product) for product in products]

    @strawberry.field
    async def product(self, info, id: strawberry.ID) -> Optional[Product]:
        """Get a single product by ID with reviews"""
        with query_histogram.labels("product", "query").time():
            query_counter.labels("product", "query").inc()

            async with async_session() as session:
                result = await session.execute(
                    select(ProductModel)
                    .where(ProductModel.id == id)
                    .options(selectinload(ProductModel.reviews).selectinload(ReviewModel.user))
                )
                product = result.scalar_one_or_none()
                return Product.from_model(product) if product else None

    @strawberry.field
    async def orders(
        self,
        info,
        filter: Optional[OrderFilterInput] = None,
        pagination: Optional[PaginationInput] = None,
    ) -> list[Order]:
        """Get orders with filtering and pagination"""
        with query_histogram.labels("orders", "query").time():
            query_counter.labels("orders", "query").inc()

            async with async_session() as session:
                query = select(OrderModel)

                # Apply filters
                if filter:
                    conditions = []
                    if filter.user_id:
                        conditions.append(OrderModel.user_id == filter.user_id)
                    if filter.status:
                        conditions.append(OrderModel.status == filter.status)
                    if filter.created_after:
                        conditions.append(OrderModel.created_at >= filter.created_after)
                    if filter.created_before:
                        conditions.append(OrderModel.created_at <= filter.created_before)

                    if conditions:
                        query = query.where(and_(*conditions))

                # Apply ordering
                query = query.order_by(OrderModel.created_at.desc())

                # Apply pagination
                if pagination:
                    query = query.limit(pagination.limit).offset(pagination.offset)
                else:
                    query = query.limit(20)

                result = await session.execute(query)
                orders = result.scalars().all()
                return [Order.from_model(order) for order in orders]

    @strawberry.field
    async def order(self, info, id: strawberry.ID) -> Optional[Order]:
        """Get a single order by ID with all items"""
        with query_histogram.labels("order", "query").time():
            query_counter.labels("order", "query").inc()

            async with async_session() as session:
                result = await session.execute(
                    select(OrderModel)
                    .where(OrderModel.id == id)
                    .options(
                        selectinload(OrderModel.user),
                        selectinload(OrderModel.items).selectinload(OrderItemModel.product),
                    )
                )
                order = result.scalar_one_or_none()
                return Order.from_model(order) if order else None

    @strawberry.field
    async def search_products(self, info, query: str, limit: int = 20) -> list[Product]:
        """Full-text search on products"""
        with query_histogram.labels("search_products", "query").time():
            query_counter.labels("search_products", "query").inc()

            async with async_session() as session:
                result = await session.execute(
                    select(ProductModel)
                    .where(
                        or_(
                            ProductModel.name.ilike(f"%{query}%"),
                            ProductModel.description.ilike(f"%{query}%"),
                        )
                    )
                    .limit(limit)
                )
                products = result.scalars().all()
                return [Product.from_model(product) for product in products]

    @strawberry.field
    async def user_orders(
        self, info, user_id: strawberry.ID, limit: int = 20, offset: int = 0
    ) -> list[Order]:
        """Get all orders for a specific user"""
        with query_histogram.labels("user_orders", "query").time():
            query_counter.labels("user_orders", "query").inc()

            async with async_session() as session:
                result = await session.execute(
                    select(OrderModel)
                    .where(OrderModel.user_id == user_id)
                    .order_by(OrderModel.created_at.desc())
                    .limit(limit)
                    .offset(offset)
                )
                orders = result.scalars().all()
                return [Order.from_model(order) for order in orders]


# Custom context getter
async def get_context():
    async with async_session() as session:
        return {
            "session": session,
            "user_loader": create_user_loader(session),
            "category_loader": create_category_loader(session),
            "product_loader": create_product_loader(session),
        }


# Create GraphQL schema
schema = strawberry.Schema(query=Query)

# Create FastAPI app
app = FastAPI(lifespan=lifespan)

# Add GraphQL route
graphql_app = GraphQLRouter(
    schema,
    context_getter=get_context,
)
app.include_router(graphql_app, prefix="/graphql")


# Health check endpoint
@app.get("/health")
async def health_check():
    """Health check endpoint"""
    try:
        async with async_session() as session:
            result = await session.execute(select(func.count(UserModel.id)))
            result.scalar()
        return {"status": "healthy", "timestamp": datetime.utcnow().isoformat()}
    except Exception as e:
        return Response(
            content=json.dumps({"status": "unhealthy", "error": str(e)}),
            status_code=503,
            media_type="application/json",
        )


# Metrics endpoint
@app.get("/metrics")
async def metrics():
    """Prometheus metrics endpoint"""
    if settings.enable_metrics:
        return Response(content=generate_latest(), media_type="text/plain")
    return {"error": "Metrics disabled"}


# Benchmark result writer
async def write_benchmark_result(test_name: str, result: dict):
    """Write benchmark results to file"""
    timestamp = datetime.utcnow().isoformat()
    result_file = Path(settings.results_dir) / f"{test_name}_{timestamp}.json"

    with result_file.open("w") as f:
        json.dump(
            {
                "framework": "strawberry-sqlalchemy",
                "test": test_name,
                "timestamp": timestamp,
                "result": result,
            },
            f,
            indent=2,
        )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
