# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 7
def extract_tenant_from_jwt(request) -> str:
    """Extract tenant from JWT token."""
    token = request.headers.get("Authorization", "").replace("Bearer ", "")
    payload = jwt.decode(token, verify=False)  # Already verified by auth middleware
    tenant_id = payload.get("tenant_id")
    if not tenant_id:
        raise ValueError("Token missing tenant_id claim")
    return tenant_id
