# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 7
# Context from authenticated request
async def get_context(request: Request) -> dict:
    token = extract_jwt(request)
    return {"tenant_id": token["tenant_id"], "user_id": token["user_id"]}


# FraiseQL automatically executes:
# SET LOCAL app.tenant_id = '<tenant_id>';
# SET LOCAL app.contact_id = '<user_id>';
