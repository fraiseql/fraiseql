# Extracted from: docs/guides/troubleshooting-decision-tree.md
# Block number: 5
from fraiseql.fastapi import create_fraiseql_app


async def get_context(request):
    # Extract JWT token
    token = request.headers.get("Authorization", "").replace("Bearer ", "")

    # Decode token
    user = decode_jwt(token)

    # Return context with user and roles
    return {"user": user, "roles": user.get("roles", []), "request": request}


app = create_fraiseql_app(..., context_getter=get_context)
