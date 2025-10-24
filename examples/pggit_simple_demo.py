"""Minimal pgGit Demo - The simplest possible example

This shows the absolute minimum code needed to get FraiseQL working
"""

from datetime import UTC, datetime

import fraiseql


# 1. Define a type
@fraiseql.type
class Commit:
    hash: str
    message: str
    author: str
    timestamp: datetime


# 2. Create a query
@fraiseql.query
async def commits(info) -> list[Commit]:
    """Get all commits"""
    return [
        Commit(
            hash="abc123",
            message="Initial commit",
            author="dev@example.com",
            timestamp=datetime.now(tz=UTC),
        ),
        Commit(
            hash="def456",
            message="Add feature X",
            author="dev@example.com",
            timestamp=datetime.now(tz=UTC),
        ),
    ]


# 3. Create and run the app
if __name__ == "__main__":
    import uvicorn

    # THIS is the correct API - not build_schema()!
    app = fraiseql.create_fraiseql_app(
        types=[Commit],
        production=False,  # Enables GraphQL Playground
    )

    uvicorn.run(app, port=8000)
