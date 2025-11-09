import pytest

"""Test that demonstrates the solution to the FraiseQL backend partial update issue."""


from fraiseql import fraise_input
from fraiseql.mutations.mutation_decorator import _to_dict
from fraiseql.types.definitions import UNSET


@pytest.mark.unit
@fraise_input
class UpdateRouterInputOld:
    """Old approach - causes the partial update issue."""

    id: str
    hostname: str | None = None  # Using None - will be sent as null!
    ip_address: str | None = None
    mac_address: str | None = None


@fraise_input
class UpdateRouterInputNew:
    """New approach - fixes the partial update issue."""

    id: str
    hostname: str | None = UNSET  # Using UNSET - excluded if not provided!
    ip_address: str | None = UNSET
    mac_address: str | None = UNSET
    location: str | None = UNSET


def test_partial_update_issue_demonstration() -> None:
    """Demonstrate the before/after behavior of the partial update fix."""
    # Simulate the user's scenario: only updating IP address

    # OLD APPROACH - causes the issue
    old_input = UpdateRouterInputOld(
        id="router-123",
        ip_address="192.168.1.100",
        # hostname and mac_address default to None
    )

    _to_dict(old_input)

    # NEW APPROACH - fixes the issue
    new_input = UpdateRouterInputNew(
        id="router-123",
        ip_address="192.168.1.100",
        # hostname and mac_address default to UNSET
    )

    new_dict = _to_dict(new_input)

    # Verify the fix
    assert "hostname" not in new_dict, "hostname should not be in JSONB"
    assert "mac_address" not in new_dict, "mac_address should not be in JSONB"
    assert new_dict["id"] == "router-123"
    assert new_dict["ip_address"] == "192.168.1.100"


def test_explicit_null_vs_unset() -> None:
    """Test the difference between explicit null and UNSET."""
    # Explicitly setting a field to None
    input_explicit_null = UpdateRouterInputNew(
        id="router-123",
        hostname=None,  # Explicitly set to None
        ip_address="192.168.1.100",
        # mac_address defaults to UNSET
    )

    dict_explicit_null = _to_dict(input_explicit_null)

    # Not providing a field at all
    input_unset = UpdateRouterInputNew(
        id="router-123",
        ip_address="192.168.1.100",
        # hostname defaults to UNSET
    )

    dict_unset = _to_dict(input_unset)

    # Verify the difference
    assert dict_explicit_null["hostname"] is None  # Explicit null is preserved
    assert "hostname" not in dict_unset  # UNSET is excluded


def test_real_world_scenario() -> None:
    """Test a realistic scenario with multiple fields."""
    # Scenario: Update IP and clear location, leave hostname and MAC unchanged
    input_obj = UpdateRouterInputNew(
        id="router-123",
        ip_address="10.0.0.100",  # Update IP
        location=None,  # Clear location (explicit null)
        # hostname and mac_address remain UNSET (unchanged)
    )

    result_dict = _to_dict(input_obj)

    # Verify
    assert len(result_dict) == 3  # Only id, ip_address, and location
    assert result_dict["id"] == "router-123"
    assert result_dict["ip_address"] == "10.0.0.100"
    assert result_dict["location"] is None  # Explicit null preserved
    assert "hostname" not in result_dict  # UNSET excluded
    assert "mac_address" not in result_dict  # UNSET excluded


if __name__ == "__main__":
    test_partial_update_issue_demonstration()
    test_explicit_null_vs_unset()
    test_real_world_scenario()
