"""Test for camel_to_snake conversion bug with numbered ID fields.

This test specifically targets the bug where camelCase field names like
'dns1Id' are incorrectly converted to 'dns_1' instead of 'dns_1_id'.
"""

import pytest

from fraiseql.utils.naming import camel_to_snake


class TestCamelToSnakeNumberedIdBug:
    """Test cases for the numbered ID field conversion bug."""

    def test_camel_to_snake_simple_cases(self) -> None:
        """Test that simple camelCase conversion works correctly."""
        # These should work correctly (not affected by the bug)
        assert camel_to_snake("userName") == "user_name"
        assert camel_to_snake("gatewayId") == "gateway_id"
        assert camel_to_snake("routerId") == "router_id"
        assert camel_to_snake("serverId") == "server_id"

    def test_camel_to_snake_numbered_id_bug(self) -> None:
        """Test the specific bug with numbered ID fields.

        This test will FAIL initially, demonstrating the bug.
        The current conversion produces dns1_id but should produce dns_1_id.
        """
        # These are the failing cases from the bug report
        # RED TEST - these should fail initially
        assert camel_to_snake("dns1Id") == "dns_1_id"  # Currently produces "dns1_id"
        assert camel_to_snake("dns2Id") == "dns_2_id"  # Currently produces "dns2_id"
        assert camel_to_snake("backup1Id") == "backup_1_id"  # Currently produces "backup1_id"

        # Additional test cases for the pattern
        assert camel_to_snake("server1Id") == "server_1_id"  # Currently produces "server1_id"
        assert camel_to_snake("host3Id") == "host_3_id"  # Currently produces "host3_id"
        assert camel_to_snake("backup10Id") == "backup_10_id"  # Currently produces "backup10_id"
        assert camel_to_snake("primary1Id") == "primary_1_id"  # Currently produces "primary1_id"

    def test_camel_to_snake_edge_cases(self) -> None:
        """Test edge cases that should continue working."""
        # These should not be affected by our fix
        assert camel_to_snake("HTTPTimeout") == "http_timeout"
        assert camel_to_snake("isActive") == "is_active"
        assert camel_to_snake("getUserById") == "get_user_by_id"

        # Numbers in other positions should work
        assert camel_to_snake("user1Name") == "user_1_name"
        assert camel_to_snake("server2Status") == "server_2_status"

    def test_fixed_behavior(self) -> None:
        """Test documenting the FIXED behavior.

        After fixing the bug, the function should correctly convert numbered ID fields.
        """
        # The fix should now correctly handle numbered ID fields
        assert camel_to_snake("dns1Id") == "dns_1_id"  # FIXED behavior
        assert camel_to_snake("dns2Id") == "dns_2_id"  # FIXED behavior

        # Additional validation of the fix
        assert camel_to_snake("backup1Id") == "backup_1_id"
        assert camel_to_snake("server10Id") == "server_10_id"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
