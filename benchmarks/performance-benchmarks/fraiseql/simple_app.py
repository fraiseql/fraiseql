"""Simple FraiseQL app without database."""

from fraiseql import create_fraiseql_app, fraise_field, fraise_type
from fraiseql.fastapi import FraiseQLConfig


@fraise_type
class SimpleUser:
    """Simple user type."""

    id: str = fraise_field(default="1")
    name: str = fraise_field(default="Test User")
    email: str = fraise_field(default="test@example.com")


@fraise_type
class Query:
    """Root query type."""

    # Simple string field
    hello: str = fraise_field(default="world", description="Hello world")

    # Single user
    me: SimpleUser = fraise_field(
        default_factory=lambda: SimpleUser(), description="Get current user"
    )

    # List of users
    all_users: list[SimpleUser] = fraise_field(
        default_factory=lambda: [
            SimpleUser(id="1", name="Alice", email="alice@example.com"),
            SimpleUser(id="2", name="Bob", email="bob@example.com"),
        ],
        description="Get all users",
    )


# No database connection
config = FraiseQLConfig(
    database_url=None,  # No database
    auto_camel_case=True,
)

app = create_fraiseql_app(
    config=config,
    types=[SimpleUser, Query],
    title="Simple Test API",
)


# Add health endpoint
@app.get("/health")
async def health():
    return {"status": "healthy"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
