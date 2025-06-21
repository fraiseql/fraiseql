"""Fixed FraiseQL benchmark application."""

import json
import os

from fraiseql import create_fraiseql_app, fraise_field, fraise_type
from fraiseql.fastapi import FraiseQLConfig


@fraise_type
class User:
    id: str  # UUID
    username: str
    email: str
    fullName: str
    createdAt: str


@fraise_type
class Product:
    id: str  # UUID
    name: str
    price: float
    stockQuantity: int
    categoryId: str  # UUID


@fraise_type
class Query:
    # Simple health check field
    health: str = fraise_field(default="healthy", description="Health check")

    # Benchmark queries
    users: list[User] = fraise_field(
        default_factory=list, description="List users with automatic filtering"
    )

    async def resolve_users(self, info, where=None, order_by=None, limit=None, offset=None):
        """Resolver for users field."""
        db = info.context.get("db")
        if not db:
            return []

        pool = db.get_pool()

        # Build SQL query
        query_parts = ["SELECT data FROM v_users WHERE 1=1"]
        params = []
        param_count = 1

        # Handle limit and offset
        if limit is not None:
            query_parts.append(f" LIMIT ${param_count}")
            params.append(limit)
            param_count += 1
        if offset is not None:
            query_parts.append(f" OFFSET ${param_count}")
            params.append(offset)
            param_count += 1

        # Build final query
        query = "".join(query_parts)

        # Execute using asyncpg directly
        async with pool.acquire() as conn:
            # Set up JSON decoding for asyncpg
            await conn.set_type_codec(
                "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
            )
            rows = await conn.fetch(query, *params)

            # Convert to User instances using from_dict
            return [User.from_dict(row["data"]) for row in rows]

    products: list[Product] = fraise_field(
        default_factory=list, description="List products with automatic filtering"
    )

    async def resolve_products(self, info, where=None, order_by=None, limit=None, offset=None):
        """Resolver for products field."""
        db = info.context.get("db")
        if not db:
            return []

        pool = db.get_pool()

        # Build SQL query
        query_parts = ["SELECT data FROM v_products WHERE 1=1"]
        params = []
        param_count = 1

        # Handle limit and offset
        if limit is not None:
            query_parts.append(f" LIMIT ${param_count}")
            params.append(limit)
            param_count += 1
        if offset is not None:
            query_parts.append(f" OFFSET ${param_count}")
            params.append(offset)
            param_count += 1

        # Build final query
        query = "".join(query_parts)

        # Execute using asyncpg directly
        async with pool.acquire() as conn:
            # Set up JSON decoding for asyncpg
            await conn.set_type_codec(
                "jsonb", encoder=json.dumps, decoder=json.loads, schema="pg_catalog"
            )
            rows = await conn.fetch(query, *params)

            # Convert to Product instances using from_dict
            return [Product.from_dict(row["data"]) for row in rows]


# Create app
config = FraiseQLConfig(
    database_url=os.environ.get("DATABASE_URL", "postgresql://localhost/fraiseql"),
    auto_camel_case=True,
)

app = create_fraiseql_app(
    config=config,
    types=[User, Product, Query],
    title="FraiseQL Benchmark API",
)


@app.get("/health")
async def health_check():
    return {"status": "healthy"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
