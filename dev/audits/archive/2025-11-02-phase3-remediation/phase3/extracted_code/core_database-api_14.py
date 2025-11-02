# Extracted from: docs/core/database-api.md
# Block number: 14
@dataclass
class QueryOptions:
    aggregations: dict[str, str] | None = None
    order_by: OrderByInstructions | None = None
    dimension_key: str | None = None
    pagination: PaginationInput | None = None
    filters: dict[str, object] | None = None
    where: ToSQLProtocol | None = None
    ignore_tenant_column: bool = False
