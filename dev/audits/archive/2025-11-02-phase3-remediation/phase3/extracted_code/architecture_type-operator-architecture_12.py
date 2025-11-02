# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 12
def _apply_type_cast(
    self, path_sql: SQL, val: Any, op: str, field_type: type | None = None
) -> SQL | Composed:
    """Apply appropriate type casting to the JSONB path."""
    # IP address types - special handling
    if field_type and is_ip_address_type(field_type) and op in ("eq", "neq", ...):
        return Composed([SQL("host("), path_sql, SQL("::inet)")])

    # MAC addresses - detect from value when field_type missing
    if looks_like_mac_address_value(val, op):
        return Composed([SQL("("), path_sql, SQL(")::macaddr")])

    # IP addresses - detect from value (production CQRS pattern)
    if looks_like_ip_address_value(val, op):
        return Composed([SQL("("), path_sql, SQL(")::inet")])

    # LTree paths - detect from value
    if looks_like_ltree_value(val, op):
        return Composed([SQL("("), path_sql, SQL(")::ltree")])

    # DateRange values - detect from value
    if looks_like_daterange_value(val, op):
        return Composed([SQL("("), path_sql, SQL(")::daterange")])

    # Numeric values
    if isinstance(val, (int, float, Decimal)):
        return Composed([SQL("("), path_sql, SQL(")::numeric")])

    # Datetime values
    if isinstance(val, datetime):
        return Composed([SQL("("), path_sql, SQL(")::timestamp")])
