#!/usr/bin/env python3
"""Tests for the nested array registry system and decorators."""

from fraiseql.fields import fraise_field
from fraiseql.nested_array_filters import (
    auto_nested_array_filters,
    clear_registry,
    enable_nested_array_filtering,
    get_nested_array_filter,
    is_nested_array_filterable,
    list_registered_filters,
    nested_array_filterable,
    register_nested_array_filter,
)
from fraiseql.types import fraise_type


@fraise_type
class PrintServerModel:
    """Test print server type."""

    hostname: str
    ip_address: str | None = None
    status: str = "active"


@fraise_type
class NetworkDeviceModel:
    """Test network device type."""

    name: str
    model: str


class TestNestedArrayRegistry:
    """Test the nested array registry system."""

    def setup_method(self) -> None:
        """Clear registry before each test."""
        clear_registry()

    def test_manual_registration(self) -> None:
        """Test manual registration of nested array filters."""

        @fraise_type
        class NetworkConfig:
            servers: list[PrintServerModel] = fraise_field(default_factory=list)

        # Register manually
        register_nested_array_filter(NetworkConfig, "servers", PrintServerModel)

        # Verify registration
        assert is_nested_array_filterable(NetworkConfig, "servers")
        assert get_nested_array_filter(NetworkConfig, "servers") == PrintServerModel
        assert not is_nested_array_filterable(NetworkConfig, "nonexistent")
        assert get_nested_array_filter(NetworkConfig, "nonexistent") is None

    def test_auto_nested_array_filters_decorator(self) -> None:
        """Test the @auto_nested_array_filters decorator."""

        @auto_nested_array_filters
        @fraise_type
        class AutoNetworkConfig:
            servers: list[PrintServerModel] = fraise_field(default_factory=list)
            devices: list[NetworkDeviceModel] = fraise_field(default_factory=list)
            # This won't be registered as it's not a List type
            hostname: str = "default"
            # This won't be registered as it's not a FraiseQL type
            tags: list[str] = fraise_field(default_factory=list)

        # Both list[FraiseQLType] fields should be registered
        assert is_nested_array_filterable(AutoNetworkConfig, "servers")
        assert is_nested_array_filterable(AutoNetworkConfig, "devices")
        assert get_nested_array_filter(AutoNetworkConfig, "servers") == PrintServerModel
        assert get_nested_array_filter(AutoNetworkConfig, "devices") == NetworkDeviceModel

        # Non-list and non-FraiseQL fields should not be registered
        assert not is_nested_array_filterable(AutoNetworkConfig, "hostname")
        assert not is_nested_array_filterable(AutoNetworkConfig, "tags")

    def test_nested_array_filterable_decorator(self) -> None:
        """Test the @nested_array_filterable decorator with specific fields."""

        @nested_array_filterable("servers")  # Only register servers, not devices
        @fraise_type
        class SelectiveNetworkConfig:
            servers: list[PrintServerModel] = fraise_field(default_factory=list)
            devices: list[NetworkDeviceModel] = fraise_field(default_factory=list)

        # Only servers should be registered
        assert is_nested_array_filterable(SelectiveNetworkConfig, "servers")
        assert not is_nested_array_filterable(SelectiveNetworkConfig, "devices")
        assert get_nested_array_filter(SelectiveNetworkConfig, "servers") == PrintServerModel
        assert get_nested_array_filter(SelectiveNetworkConfig, "devices") is None

    def test_nested_array_filterable_multiple_fields(self) -> None:
        """Test @nested_array_filterable with multiple field names."""

        @nested_array_filterable("servers", "devices")
        @fraise_type
        class MultiSelectiveConfig:
            servers: list[PrintServerModel] = fraise_field(default_factory=list)
            devices: list[NetworkDeviceModel] = fraise_field(default_factory=list)
            other_stuff: list[PrintServerModel] = fraise_field(default_factory=list)

        # Both specified fields should be registered
        assert is_nested_array_filterable(MultiSelectiveConfig, "servers")
        assert is_nested_array_filterable(MultiSelectiveConfig, "devices")
        assert not is_nested_array_filterable(MultiSelectiveConfig, "other_stuff")

    def test_enable_nested_array_filtering_function(self) -> None:
        """Test the enable_nested_array_filtering function."""

        @fraise_type
        class ManualEnableConfig:
            servers: list[PrintServerModel] = fraise_field(default_factory=list)
            devices: list[NetworkDeviceModel] = fraise_field(default_factory=list)

        # Enable filtering after class definition
        enable_nested_array_filtering(ManualEnableConfig)

        assert is_nested_array_filterable(ManualEnableConfig, "servers")
        assert is_nested_array_filterable(ManualEnableConfig, "devices")

    def test_list_registered_filters(self) -> None:
        """Test listing all registered filters."""

        @auto_nested_array_filters
        @fraise_type
        class ConfigA:
            servers: list[PrintServerModel] = fraise_field(default_factory=list)

        @nested_array_filterable("devices")
        @fraise_type
        class ConfigB:
            devices: list[NetworkDeviceModel] = fraise_field(default_factory=list)

        filters = list_registered_filters()

        # Should have entries for both classes
        config_a_key = f"{ConfigA.__module__}.{ConfigA.__name__}"
        config_b_key = f"{ConfigB.__module__}.{ConfigB.__name__}"

        assert config_a_key in filters
        assert config_b_key in filters
        assert filters[config_a_key]["servers"] == "PrintServerModel"
        assert filters[config_b_key]["devices"] == "NetworkDeviceModel"

    def test_registry_isolation_by_type(self) -> None:
        """Test that different types don't interfere with each other."""

        @fraise_type
        class ConfigA:
            items: list[PrintServerModel] = fraise_field(default_factory=list)

        @fraise_type
        class ConfigB:
            items: list[NetworkDeviceModel] = fraise_field(default_factory=list)

        register_nested_array_filter(ConfigA, "items", PrintServerModel)
        register_nested_array_filter(ConfigB, "items", NetworkDeviceModel)

        # Same field name but different parent types should have different registrations
        assert get_nested_array_filter(ConfigA, "items") == PrintServerModel
        assert get_nested_array_filter(ConfigB, "items") == NetworkDeviceModel

    def test_clear_registry(self) -> None:
        """Test that clear_registry removes all registrations."""

        @auto_nested_array_filters
        @fraise_type
        class TestConfig:
            servers: list[PrintServerModel] = fraise_field(default_factory=list)

        # Verify registration exists
        assert is_nested_array_filterable(TestConfig, "servers")

        # Clear and verify removal
        clear_registry()
        assert not is_nested_array_filterable(TestConfig, "servers")
        assert list_registered_filters() == {}

    def test_decorator_error_handling(self) -> None:
        """Test that decorators handle errors gracefully."""

        # This should not crash even if type hints fail
        @auto_nested_array_filters
        class BadTypeHints:
            # This might cause get_type_hints to fail
            pass

        # Should not be registered due to error, but should not crash
        assert not is_nested_array_filterable(BadTypeHints, "anything")
