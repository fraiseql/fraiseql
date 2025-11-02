# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 16
import json
from datetime import datetime

from fraiseql import mutation


@mutation
@requires_permission("tenant:export")
async def export_tenant_data(info) -> str:
    """Export all tenant data as JSON."""
    tenant_id = info.context["tenant_id"]

    export_data = {
        "tenant_id": tenant_id,
        "exported_at": datetime.utcnow().isoformat(),
        "users": [],
        "orders": [],
        "products": [],
    }

    async with db.connection() as conn:
        # Export users
        result = await conn.execute("SELECT * FROM users WHERE tenant_id = $1", tenant_id)
        export_data["users"] = [dict(row) for row in await result.fetchall()]

        # Export orders
        result = await conn.execute("SELECT * FROM orders WHERE tenant_id = $1", tenant_id)
        export_data["orders"] = [dict(row) for row in await result.fetchall()]

        # Export products
        result = await conn.execute("SELECT * FROM products WHERE tenant_id = $1", tenant_id)
        export_data["products"] = [dict(row) for row in await result.fetchall()]

    # Save to file or return JSON
    export_json = json.dumps(export_data, default=str)
    return export_json
