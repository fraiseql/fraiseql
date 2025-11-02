# Extracted from: docs/reference/database.md
# Block number: 25
async def get_context(request: Request) -> dict:
    return {
        "tenant_id": extract_tenant_from_jwt(request),
        "contact_id": extract_user_from_jwt(request),
    }


app = create_fraiseql_app(
    config=config,
    context_getter=get_context,
    # ... other params
)
