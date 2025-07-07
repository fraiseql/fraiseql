#!/usr/bin/env python3
"""Minimal FraiseQL test - what a new user would try first"""

import fraiseql


# Define a simple type
@fraiseql.type
class Book:
    id: str
    title: str
    author: str


# Define a simple query
@fraiseql.query
async def books(info) -> list[Book]:
    return [
        Book(id="1", title="1984", author="George Orwell"),
        Book(id="2", title="The Great Gatsby", author="F. Scott Fitzgerald"),
    ]


if __name__ == "__main__":
    # Create the app
    app = fraiseql.create_fraiseql_app(
        types=[Book],
        production=False,
    )

    print("✅ App created successfully!")
    print("📚 Types registered:", [Book])
    print("🚀 Ready to serve GraphQL at /graphql")

    # For testing, don't actually run the server
    # import uvicorn
    # uvicorn.run(app, host="127.0.0.1", port=8000)
