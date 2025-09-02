"""Operator functions for building SQL WHERE conditions.

This module provides a simple operator registry using function mapping
instead of complex strategy classes.
"""

from typing import Callable

from psycopg.sql import SQL, Composed

from fraiseql.sql.where.core.field_detection import FieldType

from . import basic, lists, ltree, mac_address, network, nulls, text

# Simple operator mapping - much cleaner than complex strategy pattern
OPERATOR_MAP: dict[tuple[FieldType, str], Callable[[SQL, any], Composed]] = {
    # Basic operators for any field type
    (FieldType.ANY, "eq"): basic.build_eq_sql,
    (FieldType.ANY, "neq"): basic.build_neq_sql,
    (FieldType.ANY, "gt"): basic.build_gt_sql,
    (FieldType.ANY, "gte"): basic.build_gte_sql,
    (FieldType.ANY, "lt"): basic.build_lt_sql,
    (FieldType.ANY, "lte"): basic.build_lte_sql,
    # Text operators
    (FieldType.STRING, "contains"): text.build_contains_sql,
    (FieldType.STRING, "startswith"): text.build_startswith_sql,
    (FieldType.STRING, "endswith"): text.build_endswith_sql,
    # List operators for any field type
    (FieldType.ANY, "in_"): lists.build_in_sql,
    (FieldType.ANY, "in"): lists.build_in_sql,  # Handle both in_ and in
    (FieldType.ANY, "notin"): lists.build_notin_sql,
    # Null operators
    (FieldType.ANY, "isnull"): nulls.build_isnull_sql,
    # IP address specific operators - this is the key fix!
    (FieldType.IP_ADDRESS, "eq"): network.build_ip_eq_sql,
    (FieldType.IP_ADDRESS, "neq"): network.build_ip_neq_sql,
    (FieldType.IP_ADDRESS, "in_"): network.build_ip_in_sql,
    (FieldType.IP_ADDRESS, "in"): network.build_ip_in_sql,
    (FieldType.IP_ADDRESS, "notin"): network.build_ip_notin_sql,
    (FieldType.IP_ADDRESS, "inSubnet"): network.build_in_subnet_sql,
    (FieldType.IP_ADDRESS, "isPrivate"): network.build_is_private_sql,
    (FieldType.IP_ADDRESS, "isPublic"): network.build_is_public_sql,
    # MAC address specific operators
    (FieldType.MAC_ADDRESS, "eq"): mac_address.build_mac_eq_sql,
    (FieldType.MAC_ADDRESS, "neq"): mac_address.build_mac_neq_sql,
    (FieldType.MAC_ADDRESS, "in_"): mac_address.build_mac_in_sql,
    (FieldType.MAC_ADDRESS, "in"): mac_address.build_mac_in_sql,
    (FieldType.MAC_ADDRESS, "notin"): mac_address.build_mac_notin_sql,
    # LTree hierarchical path operators
    (FieldType.LTREE, "eq"): ltree.build_ltree_eq_sql,
    (FieldType.LTREE, "neq"): ltree.build_ltree_neq_sql,
    (FieldType.LTREE, "in_"): ltree.build_ltree_in_sql,
    (FieldType.LTREE, "in"): ltree.build_ltree_in_sql,
    (FieldType.LTREE, "notin"): ltree.build_ltree_notin_sql,
    (FieldType.LTREE, "ancestor_of"): ltree.build_ancestor_of_sql,
    (FieldType.LTREE, "descendant_of"): ltree.build_descendant_of_sql,
    (FieldType.LTREE, "matches_lquery"): ltree.build_matches_lquery_sql,
    (FieldType.LTREE, "matches_ltxtquery"): ltree.build_matches_ltxtquery_sql,
}


def get_operator_function(field_type: FieldType, operator: str) -> Callable[[SQL, any], Composed]:
    """Get the function to build SQL for this operator.

    Args:
        field_type: The detected field type
        operator: The operator name (e.g., 'eq', 'contains')

    Returns:
        Function that builds SQL for this operator

    Raises:
        ValueError: If operator is not supported
    """
    # Try specific field type first
    if (field_type, operator) in OPERATOR_MAP:
        return OPERATOR_MAP[(field_type, operator)]

    # Fall back to generic operator
    if (FieldType.ANY, operator) in OPERATOR_MAP:
        return OPERATOR_MAP[(FieldType.ANY, operator)]

    raise ValueError(f"Unsupported operator '{operator}' for field type '{field_type.value}'")
