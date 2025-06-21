"""Minimal FraiseQL app for benchmarking."""

from fastapi import FastAPI

import strawberry
from strawberry import Schema


@strawberry.type
class Query:
    @strawberry.field
    def hello(self) -> str:
        return "world"

    @strawberry.field
    def users(self, limit: int = 10) -> list[dict]:
        # Return static data for benchmarking
        return [
            {"id": i, "username": f"user{i}", "email": f"user{i}@example.com"}
            for i in range(1, limit + 1)
        ]

    @strawberry.field
    def products(self, limit: int = 10) -> list[dict]:
        # Return static data for benchmarking
        return [
            {"id": i, "name": f"Product {i}", "price": float(i * 10)} for i in range(1, limit + 1)
        ]


# Create GraphQL schema
schema = Schema(query=Query)

# Create FastAPI app
app = FastAPI()

# Add GraphQL endpoint
from strawberry.asgi import GraphQL

graphql_app = GraphQL(schema)
app.add_route("/graphql", graphql_app)
app.add_websocket_route("/graphql", graphql_app)


# Health endpoint
@app.get("/health")
async def health():
    return {"status": "healthy"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
