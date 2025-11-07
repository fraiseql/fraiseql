"""GraphQL-compatible where input type generator.

This module provides utilities to dynamically generate GraphQL input types
that support operator-based filtering. These types can be used directly in
GraphQL resolvers and are automatically converted to SQL where types.
"""

from dataclasses import make_dataclass
from datetime import date, datetime
from decimal import Decimal
from typing import Any, Optional, TypeVar, Union, get_args, get_origin, get_type_hints
from uuid import UUID

from fraiseql import fraise_input
from fraiseql.fields import fraise_field
from fraiseql.sql.where_generator import safe_create_where_type

# Type variable for generic filter types
T = TypeVar("T")

# Cache for generated where input types to handle circular references
_where_input_cache: dict[type, type] = {}
# Stack to track types being generated to detect circular references
_generation_stack: set[type] = set()


# Base operator filter types for GraphQL inputs
@fraise_input
class StringFilter:
    """String field filter operations."""

    eq: str | None = None
    neq: str | None = None
    contains: str | None = None
    startswith: str | None = None
    endswith: str | None = None
    matches: str | None = None
    imatches: str | None = None
    not_matches: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None


@fraise_input
class ArrayFilter:
    """Array field filter operations."""

    eq: list | None = None
    neq: list | None = None
    isnull: bool | None = None
    contains: list | None = None
    contained_by: list | None = None
    overlaps: list | None = None
    len_eq: int | None = None
    len_neq: int | None = None
    len_gt: int | None = None
    len_gte: int | None = None
    len_lt: int | None = None
    len_lte: int | None = None
    any_eq: str | None = None
    all_eq: str | None = None


@fraise_input
class IntFilter:
    """Integer field filter operations."""

    eq: int | None = None
    neq: int | None = None
    gt: int | None = None
    gte: int | None = None
    lt: int | None = None
    lte: int | None = None
    in_: list[int] | None = fraise_field(default=None, graphql_name="in")
    nin: list[int] | None = None
    isnull: bool | None = None


@fraise_input
class FloatFilter:
    """Float field filter operations."""

    eq: float | None = None
    neq: float | None = None
    gt: float | None = None
    gte: float | None = None
    lt: float | None = None
    lte: float | None = None
    in_: list[float] | None = fraise_field(default=None, graphql_name="in")
    nin: list[float] | None = None
    isnull: bool | None = None


@fraise_input
class DecimalFilter:
    """Decimal field filter operations."""

    eq: Decimal | None = None
    neq: Decimal | None = None
    gt: Decimal | None = None
    gte: Decimal | None = None
    lt: Decimal | None = None
    lte: Decimal | None = None
    in_: list[Decimal] | None = fraise_field(default=None, graphql_name="in")
    nin: list[Decimal] | None = None
    isnull: bool | None = None


@fraise_input
class BooleanFilter:
    """Boolean field filter operations."""

    eq: bool | None = None
    neq: bool | None = None
    isnull: bool | None = None


@fraise_input
class UUIDFilter:
    """UUID field filter operations."""

    eq: UUID | None = None
    neq: UUID | None = None
    in_: list[UUID] | None = fraise_field(default=None, graphql_name="in")
    nin: list[UUID] | None = None
    isnull: bool | None = None


@fraise_input
class DateFilter:
    """Date field filter operations."""

    eq: date | None = None
    neq: date | None = None
    gt: date | None = None
    gte: date | None = None
    lt: date | None = None
    lte: date | None = None
    in_: list[date] | None = fraise_field(default=None, graphql_name="in")
    nin: list[date] | None = None
    isnull: bool | None = None


@fraise_input
class DateTimeFilter:
    """DateTime field filter operations."""

    eq: datetime | None = None
    neq: datetime | None = None
    gt: datetime | None = None
    gte: datetime | None = None
    lt: datetime | None = None
    lte: datetime | None = None
    in_: list[datetime] | None = fraise_field(default=None, graphql_name="in")
    nin: list[datetime] | None = None
    isnull: bool | None = None


# IPRange input type for network range filtering
@fraise_input
class IPRange:
    """IP address range input for network filtering operations."""

    from_: str = fraise_field(graphql_name="from")
    to: str


# Restricted filter types for exotic scalar types that have normalization issues
@fraise_input
class NetworkAddressFilter:
    """Enhanced filter for IP addresses and CIDR with network-specific operations.

    Provides network-aware filtering operations like subnet matching, IP range queries,
    and private/public network detection. Basic string operations are excluded due to
    PostgreSQL inet/cidr type normalization issues.
    """

    # Basic equality operations
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Network-specific operations (v0.3.8+)
    inSubnet: str | None = None  # IP is in CIDR subnet  # noqa: N815
    inRange: IPRange | None = None  # IP is in range  # noqa: N815
    isPrivate: bool | None = None  # RFC 1918 private address  # noqa: N815
    isPublic: bool | None = None  # Non-private address  # noqa: N815
    isIPv4: bool | None = None  # IPv4 address  # noqa: N815
    isIPv6: bool | None = None  # IPv6 address  # noqa: N815

    # Advanced network classification (v0.6.1+)
    isLoopback: bool | None = None  # Loopback address (127.0.0.1, ::1)  # noqa: N815
    isMulticast: bool | None = None  # Multicast address (224.0.0.0/4, ff00::/8)  # noqa: N815
    isBroadcast: bool | None = None  # Broadcast address (255.255.255.255)  # noqa: N815
    isLinkLocal: bool | None = None  # Link-local address (169.254.0.0/16, fe80::/10)  # noqa: N815
    isDocumentation: bool | None = None  # RFC 3849/5737 documentation ranges  # noqa: N815
    isReserved: bool | None = None  # Reserved/unspecified address (0.0.0.0, ::)  # noqa: N815
    isCarrierGrade: bool | None = None  # Carrier-Grade NAT (100.64.0.0/10)  # noqa: N815
    isSiteLocal: bool | None = None  # Site-local IPv6 (fec0::/10 - deprecated)  # noqa: N815
    isUniqueLocal: bool | None = None  # Unique local IPv6 (fc00::/7)  # noqa: N815
    isGlobalUnicast: bool | None = None  # Global unicast address  # noqa: N815

    # Intentionally excludes: contains, startswith, endswith


@fraise_input
class MacAddressFilter:
    """Restricted filter for MAC addresses that only exposes working operators.

    Excludes string pattern matching due to PostgreSQL macaddr type normalization
    where values are automatically formatted to canonical form.
    """

    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None
    # Intentionally excludes: contains, startswith, endswith


@fraise_input
class LTreeFilter:
    """Filter for LTree hierarchical paths with full operator support.

    Provides both basic comparison operators and PostgreSQL ltree-specific
    hierarchical operators for path ancestry, descendancy, and pattern matching.
    """

    # Basic comparison operators
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # LTree-specific hierarchical operators
    ancestor_of: str | None = None  # @> - Is ancestor of path
    descendant_of: str | None = None  # <@ - Is descendant of path
    matches_lquery: str | None = None  # ~ - Matches lquery pattern
    matches_ltxtquery: str | None = None  # ? - Matches ltxtquery text pattern

    # Intentionally excludes: contains, startswith, endswith (use ltree operators instead)


@fraise_input
class DateRangeFilter:
    """Filter for PostgreSQL date range types with full operator support.

    Provides both basic comparison operators and PostgreSQL range-specific
    operators for containment, overlap, adjacency, and positioning queries.
    """

    # Basic comparison operators
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Range-specific operators
    contains_date: str | None = None  # @> - Range contains date/range
    overlaps: str | None = None  # && - Ranges overlap
    adjacent: str | None = None  # -|- - Ranges are adjacent

    # Range positioning operators
    strictly_left: str | None = None  # << - Strictly left of
    strictly_right: str | None = None  # >> - Strictly right of
    not_left: str | None = None  # &> - Does not extend to the left
    not_right: str | None = None  # &< - Does not extend to the right

    # Intentionally excludes string pattern matching (use range operators instead)


@fraise_input
class FullTextFilter:
    """Filter for PostgreSQL full-text search (tsvector) with comprehensive search operators.

    Provides PostgreSQL's full-text search capabilities including basic search,
    advanced query parsing, and relevance ranking.
    """

    # Basic search operators
    matches: str | None = None  # @@ with to_tsquery()
    plain_query: str | None = None  # @@ with plainto_tsquery()

    # Advanced query types
    phrase_query: str | None = None  # @@ with phraseto_tsquery()
    websearch_query: str | None = None  # @@ with websearch_to_tsquery()

    # Relevance ranking operators (format: "query:threshold")
    rank_gt: str | None = None  # ts_rank() >
    rank_gte: str | None = None  # ts_rank() >=
    rank_lt: str | None = None  # ts_rank() <
    rank_lte: str | None = None  # ts_rank() <=

    # Cover density ranking operators (format: "query:threshold")
    rank_cd_gt: str | None = None  # ts_rank_cd() >
    rank_cd_gte: str | None = None  # ts_rank_cd() >=
    rank_cd_lt: str | None = None  # ts_rank_cd() <
    rank_cd_lte: str | None = None  # ts_rank_cd() <=

    # Basic null check
    isnull: bool | None = None


@fraise_input
class JSONBFilter:
    """Filter for PostgreSQL JSONB with comprehensive operator support.

    Provides PostgreSQL's JSONB capabilities including key existence,
    containment, JSONPath queries, and deep path access.
    """

    # Basic comparison operators
    eq: Any | None = None  # Exact equality (accepts dict or list)
    neq: Any | None = None  # Not equal (accepts dict or list)
    isnull: bool | None = None  # Null check

    # Key existence operators
    has_key: str | None = None  # ? operator
    has_any_keys: list[str] | None = None  # ?| operator
    has_all_keys: list[str] | None = None  # ?& operator

    # Containment operators
    contains: Any | None = None  # @> operator (accepts dict or list)
    contained_by: Any | None = None  # <@ operator (accepts dict or list)

    # JSONPath operators
    path_exists: str | None = None  # @? operator
    path_match: str | None = None  # @@ operator


def _get_filter_type_for_field(
    field_type: type, parent_class: type | None = None, field_name: str | None = None
) -> type:
    """Get the appropriate filter type for a field type."""
    # Handle Optional types FIRST before any other checks
    origin = get_origin(field_type)

    # For Python 3.10+, we need to check for UnionType as well
    import types

    if origin is Union or (hasattr(types, "UnionType") and isinstance(field_type, types.UnionType)):
        args = get_args(field_type)
        # Filter out None type
        non_none_types = [arg for arg in args if arg is not type(None)]
        if non_none_types:
            field_type = non_none_types[0]
            origin = get_origin(field_type)

    # Check for full-text search fields by name (before type checking)
    # This allows detecting tsvector fields which are usually str type in Python
    if field_name:
        field_lower = field_name.lower()
        fulltext_patterns = [
            "search_vector",
            "searchvector",
            "tsvector",
            "ts_vector",
            "fulltext_vector",
            "fulltextvector",
            "text_search",
            "textsearch",
            "search_index",
            "searchindex",
        ]
        if any(pattern in field_lower for pattern in fulltext_patterns):
            return FullTextFilter

    # Check if it's a List type
    if origin is list:
        # Use ArrayFilter for list/array fields
        return ArrayFilter

    # Check if this is a FraiseQL type (nested object)
    if hasattr(field_type, "__fraiseql_definition__"):
        # Check cache first
        if field_type in _where_input_cache:
            return _where_input_cache[field_type]

        # Check for circular reference
        if field_type in _generation_stack:
            # For circular references, we'll use a placeholder that will be resolved later
            # Store the deferred type for later resolution
            return type(f"_Deferred_{field_type.__name__}WhereInput", (), {})

        # PHASE 2 ENHANCEMENT: Check if type has auto-generated WhereInput property
        # This allows lazy properties to handle nested types naturally
        if hasattr(field_type, "WhereInput"):
            try:
                # Use the lazy property - it will generate on access
                # This breaks circular dependencies naturally
                nested_where_input = field_type.WhereInput
                return nested_where_input
            except Exception:
                # If lazy property fails, fall through to manual generation
                pass

        # Generate nested where input type recursively
        # Since we're already inside the module, we can call the function directly
        # without circular import issues
        nested_where_input = create_graphql_where_input(field_type)
        return nested_where_input

    # First check for FraiseQL scalar types that need restricted filters
    # Import at runtime to avoid circular imports
    try:
        from fraiseql.types import CIDR, DateTime, IpAddress, LTree, MacAddress
        from fraiseql.types.scalars.daterange import DateRangeField

        exotic_type_mapping = {
            IpAddress: NetworkAddressFilter,
            CIDR: NetworkAddressFilter,
            MacAddress: MacAddressFilter,
            LTree: LTreeFilter,
            DateTime: DateTimeFilter,  # Use existing DateTimeFilter for FraiseQL DateTime
            DateRangeField: DateRangeFilter,
        }

        # Check if this is one of our exotic scalar types
        if field_type in exotic_type_mapping:
            return exotic_type_mapping[field_type]

    except ImportError:
        # FraiseQL scalar types not available, continue with standard mapping
        pass

    # Map Python types to filter types
    type_mapping = {
        str: StringFilter,
        int: IntFilter,
        float: FloatFilter,
        Decimal: DecimalFilter,
        bool: BooleanFilter,
        UUID: UUIDFilter,
        date: DateFilter,
        datetime: DateTimeFilter,
        dict: JSONBFilter,  # JSONB fields are typically dict type in Python
    }

    return type_mapping.get(field_type, StringFilter)  # Default to StringFilter


def _convert_filter_to_dict(filter_obj: Any) -> dict[str, Any]:
    """Convert a filter object to a dictionary for SQL where type."""
    if filter_obj is None:
        return {}

    # Check if this is already a plain dict - return it directly
    if isinstance(filter_obj, dict):
        return filter_obj

    # Check if this is a nested where input (has _target_class and _to_sql_where)
    if hasattr(filter_obj, "_target_class") and hasattr(filter_obj, "_to_sql_where"):
        # This is a nested where input, convert it recursively
        nested_where = filter_obj._to_sql_where()
        return {"__nested__": nested_where}

    result = {}
    # Check if it's a FraiseQL type with __gql_fields__
    if hasattr(filter_obj, "__gql_fields__"):
        for field_name in filter_obj.__gql_fields__:
            value = getattr(filter_obj, field_name)
            if value is not None:
                # Handle 'in_' field mapping to 'in'
                if field_name == "in_":
                    result["in"] = value
                else:
                    result[field_name] = value
    # Fallback for regular objects - use __dict__
    elif hasattr(filter_obj, "__dict__"):
        for field_name, value in filter_obj.__dict__.items():
            if value is not None:
                # Handle 'in_' field mapping to 'in'
                if field_name == "in_":
                    result["in"] = value
                else:
                    result[field_name] = value

    return result


def _convert_graphql_input_to_where_type(graphql_input: Any, target_class: type) -> Any:
    """Convert a GraphQL where input to SQL where type."""
    if graphql_input is None:
        return None

    # Create SQL where type
    SqlWhereType = safe_create_where_type(target_class)
    where_obj = SqlWhereType()

    # Convert each field
    # Check if it's a FraiseQL type with __gql_fields__
    if hasattr(graphql_input, "__gql_fields__"):
        for field_name in graphql_input.__gql_fields__:
            filter_value = getattr(graphql_input, field_name)
            if filter_value is not None:
                # Handle logical operators specially
                if field_name in ("OR", "AND"):
                    # These are lists of WhereInput objects or dicts
                    if isinstance(filter_value, list):
                        converted_list = []
                        for item in filter_value:
                            if hasattr(item, "_to_sql_where"):
                                # WhereInput object
                                converted_list.append(item._to_sql_where())
                            elif isinstance(item, dict):
                                # Plain dict - convert it recursively
                                converted_list.append(item)
                        setattr(where_obj, field_name, converted_list)
                elif field_name == "NOT":
                    # This is a single WhereInput object
                    if hasattr(filter_value, "_to_sql_where"):
                        setattr(where_obj, field_name, filter_value._to_sql_where())
                # Check if this is a nested where input
                elif hasattr(filter_value, "_target_class") and hasattr(
                    filter_value, "_to_sql_where"
                ):
                    # Convert nested where input recursively
                    nested_where = filter_value._to_sql_where()
                    setattr(where_obj, field_name, nested_where)
                else:
                    # Convert filter object to operator dict
                    operator_dict = _convert_filter_to_dict(filter_value)
                    if operator_dict:
                        setattr(where_obj, field_name, operator_dict)
                    else:
                        # If the filter is empty, set to None instead of empty dict
                        setattr(where_obj, field_name, None)
    # Fallback for regular objects
    elif hasattr(graphql_input, "__dict__"):
        for field_name, filter_value in graphql_input.__dict__.items():
            if filter_value is not None:
                # Handle logical operators specially
                if field_name in ("OR", "AND"):
                    # These are lists of WhereInput objects or dicts
                    if isinstance(filter_value, list):
                        converted_list = []
                        for item in filter_value:
                            if hasattr(item, "_to_sql_where"):
                                # WhereInput object
                                converted_list.append(item._to_sql_where())
                            elif isinstance(item, dict):
                                # Plain dict - convert it recursively
                                converted_list.append(item)
                        setattr(where_obj, field_name, converted_list)
                elif field_name == "NOT":
                    # This is a single WhereInput object
                    if hasattr(filter_value, "_to_sql_where"):
                        setattr(where_obj, field_name, filter_value._to_sql_where())
                # Check if this is a nested where input
                elif hasattr(filter_value, "_target_class") and hasattr(
                    filter_value, "_to_sql_where"
                ):
                    # Convert nested where input recursively
                    nested_where = filter_value._to_sql_where()
                    setattr(where_obj, field_name, nested_where)
                else:
                    # Convert filter object to operator dict
                    operator_dict = _convert_filter_to_dict(filter_value)
                    if operator_dict:
                        setattr(where_obj, field_name, operator_dict)
                    else:
                        # If the filter is empty, set to None instead of empty dict
                        setattr(where_obj, field_name, None)

    return where_obj


def create_graphql_where_input(cls: type, name: str | None = None) -> type:
    """Create a GraphQL-compatible where input type with operator filters.

    Args:
        cls: The dataclass or fraise_type to generate filters for
        name: Optional name for the generated input type (defaults to {ClassName}WhereInput)

    Returns:
        A new dataclass decorated with @fraise_input that supports operator-based filtering

    Example:
        ```python
        @fraise_type
        class User:
            id: UUID
            name: str
            age: int
            is_active: bool

        UserWhereInput = create_graphql_where_input(User)

        # Usage in resolver
        @fraiseql.query
        async def users(info, where: UserWhereInput | None = None) -> list[User]:
            return await info.context["db"].find("user_view", where=where)
        ```
    """
    # Check cache first (only for unnamed types to allow custom names)
    if name is None and cls in _where_input_cache:
        return _where_input_cache[cls]

    # Add to generation stack to detect circular references
    _generation_stack.add(cls)

    try:
        # Get type hints from the class
        try:
            type_hints = get_type_hints(cls)
        except Exception:
            # Fallback for classes that might not have proper annotations
            type_hints = {}
            for key, value in cls.__annotations__.items():
                type_hints[key] = value

        # Generate field definitions for the input type
        field_definitions = []
        field_defaults = {}
        deferred_fields = {}  # For circular references

        for field_name, field_type in type_hints.items():
            # Skip private fields
            if field_name.startswith("_"):
                continue

            # Get the appropriate filter type
            filter_type = _get_filter_type_for_field(
                field_type, parent_class=cls, field_name=field_name
            )

            # Check if this is a deferred type (circular reference)
            if hasattr(filter_type, "__name__") and filter_type.__name__.startswith("_Deferred_"):
                # Store for later resolution
                deferred_fields[field_name] = field_type
                # Use StringFilter as temporary placeholder
                filter_type = StringFilter

            # Add as optional field
            field_definitions.append((field_name, Optional[filter_type], None))
            field_defaults[field_name] = None

        # Generate class name
        class_name = name or f"{cls.__name__}WhereInput"

        # Add logical operators fields using safer types for GraphQL schema generation
        # These will work at runtime but won't break GraphQL type conversion
        logical_fields = [
            ("OR", Optional[list], None),
            ("AND", Optional[list], None),
            (
                "NOT",
                Optional[dict],
                None,
            ),  # Use dict instead of object for better GraphQL compatibility
        ]

        # Add logical operators to field definitions
        field_definitions.extend(logical_fields)
        field_defaults.update({field_name: default for field_name, _, default in logical_fields})

        # Create the dataclass
        WhereInputClass = make_dataclass(
            class_name,
            field_definitions,
            bases=(),
            frozen=False,
        )

        # Add the fraise_input decorator
        WhereInputClass = fraise_input(WhereInputClass)

        # Cache before processing deferred fields (only for unnamed types)
        if name is None:
            _where_input_cache[cls] = WhereInputClass

        # Process deferred fields (circular references)
        for field_name, field_type in deferred_fields.items():
            # Now that we're cached, try to get the actual where input type
            if hasattr(field_type, "__fraiseql_definition__") and field_type in _where_input_cache:
                # Update the field annotation
                WhereInputClass.__annotations__[field_name] = Optional[
                    _where_input_cache[field_type]
                ]
                # Update the dataclass field
                if hasattr(WhereInputClass, "__dataclass_fields__"):
                    from dataclasses import MISSING, Field

                    field = Field(
                        default=None,
                        default_factory=MISSING,
                        init=True,
                        repr=True,
                        hash=None,
                        compare=True,
                        metadata={},
                    )
                    field.name = field_name
                    field.type = Optional[_where_input_cache[field_type]]
                    WhereInputClass.__dataclass_fields__[field_name] = field

        # Add conversion method
        WhereInputClass._target_class = cls
        WhereInputClass._to_sql_where = lambda self: _convert_graphql_input_to_where_type(self, cls)

        # Add helpful docstring
        WhereInputClass.__doc__ = (
            f"GraphQL where input type for {cls.__name__} with operator-based filtering."
        )

        return WhereInputClass

    finally:
        # Remove from generation stack
        _generation_stack.discard(cls)
