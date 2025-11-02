# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 9
from fraiseql.fastapi import create_fraiseql_app


def get_graphql_context(request: Request) -> dict:
    """Build GraphQL context with tenant."""
    return {
        "request": request,
        "tenant_id": request.state.tenant_id,
        "user": request.state.user,  # From auth middleware
    }


app = create_fraiseql_app(types=[User, Order, Product], context_getter=get_graphql_context)
