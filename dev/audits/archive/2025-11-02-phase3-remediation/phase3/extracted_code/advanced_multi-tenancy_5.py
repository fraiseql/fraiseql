# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 5


def extract_tenant_from_subdomain(request) -> str:
    """Extract tenant from subdomain (e.g., acme.yourapp.com)."""
    host = request.headers.get("host", "")
    subdomain = host.split(".")[0]

    # Validate subdomain
    if subdomain in ["www", "api", "admin"]:
        raise ValueError("Invalid tenant subdomain")

    return subdomain


# Look up tenant ID from subdomain
async def resolve_tenant_id(subdomain: str) -> str:
    async with db.connection() as conn:
        result = await conn.execute("SELECT id FROM organizations WHERE subdomain = $1", subdomain)
        row = await result.fetchone()
        if not row:
            raise ValueError(f"Unknown tenant: {subdomain}")
        return row["id"]
