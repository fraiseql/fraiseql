# Extracted from: docs/core/database-api.md
# Block number: 35
from fraiseql.db.utils import get_tenant_column

tenant_info = get_tenant_column(view_name="v_orders")
# Returns: {"table": "tenant_id", "view": "tenant_id"}
