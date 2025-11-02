# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 21
async def query(
    self,
    view_name: str,
    filters: dict[str, Any] | None = None,
    order_by: str | None = None,
    limit: int = 20,
    offset: int = 0,
) -> list[dict[str, Any]]:
    """Query entities with filtering.

    Converts GraphQL-style filters to SQL WHERE clauses:
    {
        "ip_address": {"isPrivate": True},
        "path": {"ancestor_of": "departments.engineering"}
    }
    """
    query_parts = [SQL("SELECT data FROM {} WHERE 1=1").format(SQL(view_name))]

    if filters:
        for key, value in filters.items():
            if isinstance(value, dict):
                # Map GraphQL field names to operator names
                # e.g., "nin" -> "notin"
                mapped_value = {}
                for op, val in value.items():
                    if op == "nin":
                        mapped_value["notin"] = val
                    else:
                        mapped_value[op] = val

                # Generate WHERE condition using operator strategies
                where_condition = _make_filter_field_composed(key, mapped_value, "data", None)
                if where_condition:
                    query_parts.append(SQL(" AND "))
                    query_parts.append(where_condition)

    return await cursor.execute(Composed(query_parts))
