"""Blog demo FraiseQL application."""

import fraiseql
from fraiseql.fastapi import create_fastapi_app

# Create FraiseQL app
app = fraiseql.create_app(
    database_url="postgresql://fraiseql_test:fraiseql_test@localhost:5432/fraiseql_test",
    enable_playground=True,
    enable_introspection=True,
)

# Create FastAPI app with GraphQL endpoint
fastapi_app = create_fastapi_app(app)
