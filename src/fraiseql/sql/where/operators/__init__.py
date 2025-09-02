"""Operator functions for building SQL WHERE conditions.

This module provides a simple operator registry using function mapping
instead of complex strategy classes.
"""

from typing import Callable

from psycopg.sql import SQL, Composed

from fraiseql.sql.where.core.field_detection import FieldType

from . import (
    basic,
    date,
    date_range,
    datetime,
    email,
    hostname,
    lists,
    ltree,
    mac_address,
    network,
    nulls,
    port,
    text,
)

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
    (FieldType.ANY, "nin"): lists.build_notin_sql,
    # Null operators
    (FieldType.ANY, "isnull"): nulls.build_isnull_sql,
    # IP address specific operators - this is the key fix!
    (FieldType.IP_ADDRESS, "eq"): network.build_ip_eq_sql,
    (FieldType.IP_ADDRESS, "neq"): network.build_ip_neq_sql,
    (FieldType.IP_ADDRESS, "in_"): network.build_ip_in_sql,
    (FieldType.IP_ADDRESS, "in"): network.build_ip_in_sql,
    (FieldType.IP_ADDRESS, "notin"): network.build_ip_notin_sql,
    (FieldType.IP_ADDRESS, "nin"): network.build_ip_notin_sql,
    (FieldType.IP_ADDRESS, "inSubnet"): network.build_in_subnet_sql,
    (FieldType.IP_ADDRESS, "isPrivate"): network.build_is_private_sql,
    (FieldType.IP_ADDRESS, "isPublic"): network.build_is_public_sql,
    # MAC address specific operators
    (FieldType.MAC_ADDRESS, "eq"): mac_address.build_mac_eq_sql,
    (FieldType.MAC_ADDRESS, "neq"): mac_address.build_mac_neq_sql,
    (FieldType.MAC_ADDRESS, "in_"): mac_address.build_mac_in_sql,
    (FieldType.MAC_ADDRESS, "in"): mac_address.build_mac_in_sql,
    (FieldType.MAC_ADDRESS, "notin"): mac_address.build_mac_notin_sql,
    (FieldType.MAC_ADDRESS, "nin"): mac_address.build_mac_notin_sql,
    # LTree hierarchical path operators
    (FieldType.LTREE, "eq"): ltree.build_ltree_eq_sql,
    (FieldType.LTREE, "neq"): ltree.build_ltree_neq_sql,
    (FieldType.LTREE, "in_"): ltree.build_ltree_in_sql,
    (FieldType.LTREE, "in"): ltree.build_ltree_in_sql,
    (FieldType.LTREE, "notin"): ltree.build_ltree_notin_sql,
    (FieldType.LTREE, "nin"): ltree.build_ltree_notin_sql,
    (FieldType.LTREE, "ancestor_of"): ltree.build_ancestor_of_sql,
    (FieldType.LTREE, "descendant_of"): ltree.build_descendant_of_sql,
    (FieldType.LTREE, "matches_lquery"): ltree.build_matches_lquery_sql,
    (FieldType.LTREE, "matches_ltxtquery"): ltree.build_matches_ltxtquery_sql,
    # DateRange operators for temporal range operations
    (FieldType.DATE_RANGE, "eq"): date_range.build_daterange_eq_sql,
    (FieldType.DATE_RANGE, "neq"): date_range.build_daterange_neq_sql,
    (FieldType.DATE_RANGE, "in_"): date_range.build_daterange_in_sql,
    (FieldType.DATE_RANGE, "in"): date_range.build_daterange_in_sql,
    (FieldType.DATE_RANGE, "notin"): date_range.build_daterange_notin_sql,
    (FieldType.DATE_RANGE, "nin"): date_range.build_daterange_notin_sql,
    (FieldType.DATE_RANGE, "contains_date"): date_range.build_contains_date_sql,
    (FieldType.DATE_RANGE, "overlaps"): date_range.build_overlaps_sql,
    (FieldType.DATE_RANGE, "adjacent"): date_range.build_adjacent_sql,
    (FieldType.DATE_RANGE, "strictly_left"): date_range.build_strictly_left_sql,
    (FieldType.DATE_RANGE, "strictly_right"): date_range.build_strictly_right_sql,
    (FieldType.DATE_RANGE, "not_left"): date_range.build_not_left_sql,
    (FieldType.DATE_RANGE, "not_right"): date_range.build_not_right_sql,
    # Hostname operators for DNS hostname validation
    (FieldType.HOSTNAME, "eq"): hostname.build_hostname_eq_sql,
    (FieldType.HOSTNAME, "neq"): hostname.build_hostname_neq_sql,
    (FieldType.HOSTNAME, "in_"): hostname.build_hostname_in_sql,
    (FieldType.HOSTNAME, "in"): hostname.build_hostname_in_sql,
    (FieldType.HOSTNAME, "notin"): hostname.build_hostname_notin_sql,
    (FieldType.HOSTNAME, "nin"): hostname.build_hostname_notin_sql,
    # Email operators for email address validation
    (FieldType.EMAIL, "eq"): email.build_email_eq_sql,
    (FieldType.EMAIL, "neq"): email.build_email_neq_sql,
    (FieldType.EMAIL, "in_"): email.build_email_in_sql,
    (FieldType.EMAIL, "in"): email.build_email_in_sql,
    (FieldType.EMAIL, "notin"): email.build_email_notin_sql,
    (FieldType.EMAIL, "nin"): email.build_email_notin_sql,
    # Port operators for network port validation and comparison
    (FieldType.PORT, "eq"): port.build_port_eq_sql,
    (FieldType.PORT, "neq"): port.build_port_neq_sql,
    (FieldType.PORT, "in_"): port.build_port_in_sql,
    (FieldType.PORT, "in"): port.build_port_in_sql,
    (FieldType.PORT, "notin"): port.build_port_notin_sql,
    (FieldType.PORT, "nin"): port.build_port_notin_sql,
    (FieldType.PORT, "gt"): port.build_port_gt_sql,
    (FieldType.PORT, "gte"): port.build_port_gte_sql,
    (FieldType.PORT, "lt"): port.build_port_lt_sql,
    (FieldType.PORT, "lte"): port.build_port_lte_sql,
    # DateTime operators for ISO 8601 datetime with timezone support
    (FieldType.DATETIME, "eq"): datetime.build_datetime_eq_sql,
    (FieldType.DATETIME, "neq"): datetime.build_datetime_neq_sql,
    (FieldType.DATETIME, "in_"): datetime.build_datetime_in_sql,
    (FieldType.DATETIME, "in"): datetime.build_datetime_in_sql,
    (FieldType.DATETIME, "notin"): datetime.build_datetime_notin_sql,
    (FieldType.DATETIME, "nin"): datetime.build_datetime_notin_sql,
    (FieldType.DATETIME, "gt"): datetime.build_datetime_gt_sql,
    (FieldType.DATETIME, "gte"): datetime.build_datetime_gte_sql,
    (FieldType.DATETIME, "lt"): datetime.build_datetime_lt_sql,
    (FieldType.DATETIME, "lte"): datetime.build_datetime_lte_sql,
    # Date operators for ISO 8601 date format
    (FieldType.DATE, "eq"): date.build_date_eq_sql,
    (FieldType.DATE, "neq"): date.build_date_neq_sql,
    (FieldType.DATE, "in_"): date.build_date_in_sql,
    (FieldType.DATE, "in"): date.build_date_in_sql,
    (FieldType.DATE, "notin"): date.build_date_notin_sql,
    (FieldType.DATE, "nin"): date.build_date_notin_sql,
    (FieldType.DATE, "gt"): date.build_date_gt_sql,
    (FieldType.DATE, "gte"): date.build_date_gte_sql,
    (FieldType.DATE, "lt"): date.build_date_lt_sql,
    (FieldType.DATE, "lte"): date.build_date_lte_sql,
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
