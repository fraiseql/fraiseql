import pytest

"""Test that demonstrates the solution to the PrintOptim backend partial update issue."""

import json

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


def test_partial_update_issue_demonstration():
    """Demonstrate the before/after behavior of the partial update fix."""
    print("\n=== PrintOptim Backend Partial Update Issue Fix ===")

    # Simulate the user's scenario: only updating IP address
    print("\n1. User wants to update ONLY the IP address of a router:")
    print("   GraphQL: { id: '123', ipAddress: '192.168.1.100' }")

    # OLD APPROACH - causes the issue
    print("\n2. OLD APPROACH (with None defaults):")
    old_input = UpdateRouterInputOld(
        id="router-123",
        ip_address="192.168.1.100",
        # hostname and mac_address default to None
    )

    old_dict = _to_dict(old_input)
    print(f"   Python object: {old_input.__dict__}")
    print(f"   JSONB to PostgreSQL: {json.dumps(old_dict, indent=2)}")
    print("   Result: ❌ ALL fields are sent, including null values!")
    print("   PostgreSQL gets: hostname=null, mac_address=null")
    print("   UPDATE statement tries to set hostname=NULL → NOT NULL constraint violation!")

    # NEW APPROACH - fixes the issue
    print("\n3. NEW APPROACH (with UNSET defaults):")
    new_input = UpdateRouterInputNew(
        id="router-123",
        ip_address="192.168.1.100",
        # hostname and mac_address default to UNSET
    )

    new_dict = _to_dict(new_input)
    print(f"   Python object: {new_input.__dict__}")
    print(f"   JSONB to PostgreSQL: {json.dumps(new_dict, indent=2)}")
    print("   Result: ✅ Only provided fields are sent!")
    print("   PostgreSQL gets: only id and ip_address")
    print("   UPDATE statement only updates ip_address → hostname preserved!")

    # Verify the fix
    assert "hostname" not in new_dict, "hostname should not be in JSONB"
    assert "mac_address" not in new_dict, "mac_address should not be in JSONB"
    assert new_dict["id"] == "router-123"
    assert new_dict["ip_address"] == "192.168.1.100"

    print("\n4. PostgreSQL function can now use:")
    print("   IF p_input ? 'hostname' THEN  -- FALSE (field not present)")
    print("   IF p_input ? 'ip_address' THEN  -- TRUE (field present)")
    print("   → Perfect partial update!")


def test_explicit_null_vs_unset():
    """Test the difference between explicit null and UNSET."""
    print("\n=== Explicit NULL vs UNSET ===")

    # Explicitly setting a field to None
    input_explicit_null = UpdateRouterInputNew(
        id="router-123",
        hostname=None,  # Explicitly set to None
        ip_address="192.168.1.100",
        # mac_address defaults to UNSET
    )

    dict_explicit_null = _to_dict(input_explicit_null)
    print(f"Explicit None: {json.dumps(dict_explicit_null, indent=2)}")

    # Not providing a field at all
    input_unset = UpdateRouterInputNew(
        id="router-123",
        ip_address="192.168.1.100",
        # hostname defaults to UNSET
    )

    dict_unset = _to_dict(input_unset)
    print(f"UNSET (not provided): {json.dumps(dict_unset, indent=2)}")

    # Verify the difference
    assert dict_explicit_null["hostname"] is None  # Explicit null is preserved
    assert "hostname" not in dict_unset  # UNSET is excluded

    print("\nThis allows PostgreSQL to distinguish between:")
    print("- Clear a field (explicit null): UPDATE SET hostname = NULL")
    print("- Don't touch a field (UNSET): no UPDATE for hostname")


def test_real_world_scenario():
    """Test a realistic scenario with multiple fields."""
    print("\n=== Real-world Router Update Scenario ===")

    # Scenario: Update IP and clear location, leave hostname and MAC unchanged
    input_obj = UpdateRouterInputNew(
        id="router-123",
        ip_address="10.0.0.100",  # Update IP
        location=None,  # Clear location (explicit null)
        # hostname and mac_address remain UNSET (unchanged)
    )

    result_dict = _to_dict(input_obj)
    print(f"JSONB payload: {json.dumps(result_dict, indent=2)}")

    # Verify
    assert len(result_dict) == 3  # Only id, ip_address, and location
    assert result_dict["id"] == "router-123"
    assert result_dict["ip_address"] == "10.0.0.100"
    assert result_dict["location"] is None  # Explicit null preserved
    assert "hostname" not in result_dict  # UNSET excluded
    assert "mac_address" not in result_dict  # UNSET excluded

    print("\nPostgreSQL function logic:")
    print("- IF p_input ? 'ip_address' THEN → TRUE, update IP")
    print("- IF p_input ? 'location' THEN → TRUE, clear location")
    print("- IF p_input ? 'hostname' THEN → FALSE, preserve hostname")
    print("- IF p_input ? 'mac_address' THEN → FALSE, preserve MAC")
    print("✅ Perfect partial update with NOT NULL constraint safety!")


if __name__ == "__main__":
    test_partial_update_issue_demonstration()
    test_explicit_null_vs_unset()
    test_real_world_scenario()
    print("\n🎉 All tests passed! Partial update issue is fixed!")
