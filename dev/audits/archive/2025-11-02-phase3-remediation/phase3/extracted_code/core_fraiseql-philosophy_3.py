# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 3
async def get_context(request: Request) -> dict:
    """Custom context with user + auto database injection."""
    return {
        # Your custom context
        "user_id": extract_user_from_jwt(request),
        "tenant_id": extract_tenant_from_jwt(request),
        # No need to add "db" - FraiseQL adds it automatically!
    }


app = create_fraiseql_app(
    config=config,
    context_getter=get_context,  # Database still auto-injected
)
