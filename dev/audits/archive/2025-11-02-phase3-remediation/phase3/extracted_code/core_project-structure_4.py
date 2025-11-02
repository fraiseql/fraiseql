# Extracted from: docs/core/project-structure.md
# Block number: 4
# src/main.py
import os

from fraiseql import type

from .mutations.user_mutations import UserMutations
from .queries.user_queries import UserQueries


@type
class QueryRoot(UserQueries):
    """Root query type combining all query operations."""


@type
class MutationRoot(UserMutations):
    """Root mutation type combining all mutation operations."""


# Create the FastAPI app
app = fraiseql.create_fraiseql_app(
    queries=[QueryRoot],
    mutations=[MutationRoot],
    database_url=os.getenv("FRAISEQL_DATABASE_URL"),
)

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
