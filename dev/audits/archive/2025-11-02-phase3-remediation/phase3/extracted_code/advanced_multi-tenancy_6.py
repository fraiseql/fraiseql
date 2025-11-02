# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 6
def extract_tenant_from_header(request) -> str:
    """Extract tenant from X-Tenant-ID header."""
    tenant_id = request.headers.get("X-Tenant-ID")
    if not tenant_id:
        raise ValueError("Missing X-Tenant-ID header")
    return tenant_id
