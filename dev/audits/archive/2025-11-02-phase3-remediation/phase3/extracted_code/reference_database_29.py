# Extracted from: docs/reference/database.md
# Block number: 29
async def get_context(request: Request) -> dict:
    return {
        "tenant_id": extract_tenant(request),
        "contact_id": extract_user(request),
        "user_role": extract_role(request),  # Custom variable
    }
