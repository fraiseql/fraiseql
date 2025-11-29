import pytest

pytestmark = pytest.mark.database

"""Test restricted filter types for exotic scalar types.

This ensures that problematic operators are not exposed for types that have
PostgreSQL normalization issues.
"""

from dataclasses import dataclass
from typing import get_type_hints

from fraiseql.sql.graphql_where_generator import (
    DateTimeFilter,
    LTreeFilter,
    MacAddressFilter,
    NetworkAddressFilter,
    StringFilter,
    _get_filter_type_for_field,
    create_graphql_where_input,
)
from fraiseql.types import CIDR, DateTime, IpAddress, LTree, MacAddress


@pytest.mark.unit
@dataclass
class NetworkDevice:
    """Test device with exotic scalar types."""

    name: str
    ip_address: IpAddress
    subnet: CIDR
    mac_address: MacAddress
    path: LTree
    created_at: DateTime


class TestRestrictedFilterTypes:
    """Test that restricted filter types are correctly assigned and configured."""

    def test_exotic_types_get_restricted_filters(self) -> None:
        """Test that exotic scalar types are assigned restricted filters."""
        type_hints = get_type_hints(NetworkDevice)

        # Test each exotic type gets the correct restricted filter
        assert _get_filter_type_for_field(type_hints["ip_address"]) == NetworkAddressFilter
        assert _get_filter_type_for_field(type_hints["subnet"]) == NetworkAddressFilter
        assert _get_filter_type_for_field(type_hints["mac_address"]) == MacAddressFilter
        assert _get_filter_type_for_field(type_hints["path"]) == LTreeFilter
        assert _get_filter_type_for_field(type_hints["created_at"]) == DateTimeFilter

        # Standard types should still use standard filters
        assert _get_filter_type_for_field(type_hints["name"]) == StringFilter

    def test_network_address_filter_restrictions(self) -> None:
        """Test that NetworkAddressFilter excludes problematic operators."""
        # Get available operators (exclude private/special attributes)
        operators = [
            attr
            for attr in dir(NetworkAddressFilter)
            if not attr.startswith("_") and not callable(getattr(NetworkAddressFilter, attr))
        ]

        # Should include basic operators
        assert "eq" in operators
        assert "neq" in operators
        assert "in_" in operators
        assert "nin" in operators
        assert "isnull" in operators

        # Should NOT include problematic string operators
        assert "contains" not in operators
        assert "startswith" not in operators
        assert "endswith" not in operators

    def test_mac_address_filter_restrictions(self) -> None:
        """Test that MacAddressFilter excludes problematic operators."""
        operators = [
            attr
            for attr in dir(MacAddressFilter)
            if not attr.startswith("_") and not callable(getattr(MacAddressFilter, attr))
        ]

        # Should include basic operators
        assert "eq" in operators
        assert "neq" in operators
        assert "in_" in operators
        assert "nin" in operators
        assert "isnull" in operators

        # Should NOT include problematic string operators
        assert "contains" not in operators
        assert "startswith" not in operators
        assert "endswith" not in operators

    def test_ltree_filter_restrictions(self) -> None:
        """Test that LTreeFilter has conservative operator set with ltree-specific operators."""
        operators = [
            attr
            for attr in dir(LTreeFilter)
            if not attr.startswith("_") and not callable(getattr(LTreeFilter, attr))
        ]

        # Should include basic comparison operators
        assert "eq" in operators
        assert "neq" in operators
        assert "in_" in operators  # List operators are safe for LTree
        assert "nin" in operators
        assert "isnull" in operators

        # Should include ltree-specific hierarchical operators
        assert "ancestor_of" in operators
        assert "descendant_of" in operators
        assert "matches_lquery" in operators
        assert "matches_ltxtquery" in operators

        # Should NOT include problematic string operators
        assert "contains" not in operators
        assert "startswith" not in operators
        assert "endswith" not in operators

    def test_generated_where_input_uses_restricted_filters(self) -> None:
        """Test that generated GraphQL where input uses restricted filters."""
        WhereInput = create_graphql_where_input(NetworkDevice)

        # Create an instance to verify the types
        WhereInput()
        type_hints = get_type_hints(WhereInput)

        # Check that the correct filter types are used
        # Extract actual filter type from Optional[FilterType]
        ip_filter_type = type_hints["ip_address"].__args__[0]
        subnet_filter_type = type_hints["subnet"].__args__[0]
        mac_filter_type = type_hints["mac_address"].__args__[0]
        path_filter_type = type_hints["path"].__args__[0]

        assert ip_filter_type == NetworkAddressFilter
        assert subnet_filter_type == NetworkAddressFilter
        assert mac_filter_type == MacAddressFilter
        assert path_filter_type == LTreeFilter

    def test_standard_types_unchanged(self) -> None:
        """Test that standard Python types are not affected by restrictions."""

        @dataclass
        class StandardTypes:
            name: str
            count: int
            price: float
            active: bool

        type_hints = get_type_hints(StandardTypes)

        # Standard types should get their normal filters
        assert _get_filter_type_for_field(type_hints["name"]) == StringFilter
        # Note: We could test IntFilter, FloatFilter etc. but StringFilter test is sufficient

        # Generate where input to ensure it works
        WhereInput = create_graphql_where_input(StandardTypes)
        where_input = WhereInput()

        # Should be able to create instance without issues
        assert where_input is not None

    def test_filter_instantiation(self) -> None:
        """Test that all restricted filters can be instantiated correctly."""
        # Test that we can create instances of all restricted filters
        network_filter = NetworkAddressFilter()
        assert network_filter.eq is None
        assert network_filter.neq is None
        assert network_filter.isnull is None

        mac_filter = MacAddressFilter()
        assert mac_filter.eq is None
        assert mac_filter.neq is None
        assert mac_filter.isnull is None

        ltree_filter = LTreeFilter()
        assert ltree_filter.eq is None
        assert ltree_filter.neq is None
        assert ltree_filter.isnull is None

    def test_backwards_compatibility(self) -> None:
        """Ensure existing code continues to work."""

        # Test that old usage patterns still work
        @dataclass
        class LegacyDevice:
            id: str
            name: str

        # Should still be able to generate where input
        LegacyWhereInput = create_graphql_where_input(LegacyDevice)
        legacy_where = LegacyWhereInput()

        assert legacy_where is not None

        # Standard string fields should still have all operators
        type_hints = get_type_hints(LegacyWhereInput)
        name_filter_type = type_hints["name"].__args__[0]
        assert name_filter_type == StringFilter

        # StringFilter should still have contains, startswith, etc.
        string_ops = [
            attr
            for attr in dir(StringFilter)
            if not attr.startswith("_") and not callable(getattr(StringFilter, attr))
        ]
        assert "contains" in string_ops
        assert "startswith" in string_ops
        assert "endswith" in string_ops
