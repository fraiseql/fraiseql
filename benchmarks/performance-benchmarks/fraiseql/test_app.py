"""Minimal test app to debug FraiseQL setup."""

from typing import Optional

import fraiseql
from fraiseql import create_fraiseql_app, fraise_field
from fraiseql.fastapi import FraiseQLConfig


@fraiseql.type
class TestType:
    """A simple test type."""

    id: str
    name: str


@fraiseql.type
class Query:
    """Minimal query type."""

    # Simple field with static resolver
    hello: str = fraise_field(default="world", description="Hello world")

    # Test type field with default
    test_item: Optional[TestType] = fraise_field(default=None, description="Get test item")

    @staticmethod
    def resolve_test_item(root, info) -> TestType:
        return TestType(id="1", name="Test Item")


# Create minimal app
config = FraiseQLConfig(
    database_url="postgresql://benchmark:benchmark@localhost:5432/benchmark_db",
    auto_camel_case=True,
)

app = create_fraiseql_app(
    config=config,
    types=[TestType, Query],
    title="Test App",
)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
