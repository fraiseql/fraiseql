"""TurboRouter Example - Main Application.

This demonstrates how to set up and use TurboRouter for high-performance
query execution.
"""

import uvicorn
from schema import Post, User
from turbo_config import setup_turbo_router

from fraiseql import FraiseQL
from fraiseql.fastapi import create_app

# Initialize FraiseQL
app = FraiseQL(database_url="postgresql://localhost/turborouter_demo")


# Register types
app.register_type(User)
app.register_type(Post)


# Import queries
from queries import post, posts, user, users

app.register_query(user)
app.register_query(users)
app.register_query(post)
app.register_query(posts)


# Create FastAPI app
fastapi_app = create_app(
    app,
    database_url="postgresql://localhost/turborouter_demo",
    enable_playground=True,
)

# Setup TurboRouter
turbo_registry = setup_turbo_router(app)
print(f"TurboRouter initialized with {len(turbo_registry._queries)} registered queries")


if __name__ == "__main__":
    print("=" * 60)
    print("TurboRouter Example Server")
    print("=" * 60)
    print()
    print("TurboRouter provides 2-4x performance improvement by:")
    print("  ✅ Bypassing GraphQL parsing/validation")
    print("  ✅ Using pre-compiled SQL templates")
    print("  ✅ Zero Python object instantiation")
    print("  ✅ Direct JSON passthrough")
    print()
    print("Server starting at: http://localhost:8000")
    print("GraphQL Playground: http://localhost:8000/graphql")
    print()
    print("Try these queries:")
    print("  - GetUser: query GetUser($id: Int!) { user(id: $id) { name email } }")
    print("  - GetPosts: query GetPosts($limit: Int!) { posts(limit: $limit) { title } }")
    print()
    print("Check response headers for 'x-execution-mode: turbo'")
    print("=" * 60)

    uvicorn.run(fastapi_app, host="0.0.0.0", port=8000)
