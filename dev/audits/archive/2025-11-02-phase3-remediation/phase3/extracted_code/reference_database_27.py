# Extracted from: docs/reference/database.md
# Block number: 27
import jwt
from fastapi import Request


async def get_context(request: Request) -> dict:
    """Extract tenant and user from JWT."""
    auth_header = request.headers.get("authorization", "")

    if not auth_header.startswith("Bearer "):
        return {}  # Anonymous request

    token = auth_header.replace("Bearer ", "")
    decoded = jwt.decode(token, options={"verify_signature": False})

    return {"tenant_id": decoded.get("tenant_id"), "contact_id": decoded.get("user_id")}
