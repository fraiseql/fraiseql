"""Tests for IPv4 address and subnet utilities."""

import pytest

from fraiseql.utils.ip_utils import ipv4_mask_len, is_ipv4_address


class TestIPv4AddressValidation:
    """Test IPv4 address validation functionality."""

    @pytest.mark.parametrize(
        "valid_ip",
        [
            "192.168.1.1",
            "10.0.0.1",
            "127.0.0.1",
            "255.255.255.255",
            "0.0.0.0",
            "172.16.0.1",
            "8.8.8.8",
            "1.1.1.1",
        ],
    )
    def test_valid_ipv4_addresses(self, valid_ip) -> None:
        """Test validation of valid IPv4 addresses."""
        assert is_ipv4_address(valid_ip) is True

    @pytest.mark.parametrize(
        "invalid_ip",
        [
            "256.1.1.1",  # Octet > 255
            "192.168.1",  # Too few octets
            "192.168.1.1.1",  # Too many octets
            "192.168.1.a",  # Non-numeric octet
            "192.168.1.-1",  # Negative octet
            "",  # Empty string
            "192.168.1.256",  # Last octet > 255
            "300.168.1.1",  # First octet > 255
            "192.300.1.1",  # Second octet > 255
            "192.168.300.1",  # Third octet > 255
            "abc.def.ghi.jkl",  # All non-numeric
            "192.168.1.1.0",  # Extra octets
            "192..1.1",  # Empty octet
            ".192.168.1.1",  # Leading dot
            "192.168.1.1.",  # Trailing dot
        ],
    )
    def test_invalid_ipv4_addresses(self, invalid_ip) -> None:
        """Test validation of invalid IPv4 addresses."""
        assert is_ipv4_address(invalid_ip) is False

    def test_ipv4_edge_cases(self) -> None:
        """Test edge cases for IPv4 validation."""
        # Test boundary values
        assert is_ipv4_address("0.0.0.0") is True
        assert is_ipv4_address("255.255.255.255") is True

        # Test just over boundary
        assert is_ipv4_address("256.0.0.0") is False
        assert is_ipv4_address("0.256.0.0") is False
        assert is_ipv4_address("0.0.256.0") is False
        assert is_ipv4_address("0.0.0.256") is False


class TestIPv4MaskLength:
    """Test IPv4 netmask length calculation."""

    @pytest.mark.parametrize(
        ("netmask", "expected_length"),
        [
            ("255.255.255.255", 32),  # /32 - single host
            ("255.255.255.254", 31),  # /31 - point-to-point
            ("255.255.255.252", 30),  # /30 - 4 addresses
            ("255.255.255.248", 29),  # /29 - 8 addresses
            ("255.255.255.240", 28),  # /28 - 16 addresses
            ("255.255.255.224", 27),  # /27 - 32 addresses
            ("255.255.255.192", 26),  # /26 - 64 addresses
            ("255.255.255.128", 25),  # /25 - 128 addresses
            ("255.255.255.0", 24),  # /24 - standard subnet
            ("255.255.254.0", 23),  # /23
            ("255.255.252.0", 22),  # /22
            ("255.255.248.0", 21),  # /21
            ("255.255.240.0", 20),  # /20
            ("255.255.224.0", 19),  # /19
            ("255.255.192.0", 18),  # /18
            ("255.255.128.0", 17),  # /17
            ("255.255.0.0", 16),  # /16 - Class B
            ("255.254.0.0", 15),  # /15
            ("255.252.0.0", 14),  # /14
            ("255.248.0.0", 13),  # /13
            ("255.240.0.0", 12),  # /12
            ("255.224.0.0", 11),  # /11
            ("255.192.0.0", 10),  # /10
            ("255.128.0.0", 9),  # /9
            ("255.0.0.0", 8),  # /8 - Class A
            ("254.0.0.0", 7),  # /7
            ("252.0.0.0", 6),  # /6
            ("248.0.0.0", 5),  # /5
            ("240.0.0.0", 4),  # /4
            ("224.0.0.0", 3),  # /3
            ("192.0.0.0", 2),  # /2
            ("128.0.0.0", 1),  # /1
            ("0.0.0.0", 0),  # /0 - default route
        ],
    )
    def test_valid_netmask_lengths(self, netmask, expected_length) -> None:
        """Test calculation of netmask lengths for valid masks."""
        assert ipv4_mask_len(netmask) == expected_length

    @pytest.mark.parametrize(
        "invalid_netmask",
        [
            "255.255.255.253",  # Invalid mask - not contiguous
            "255.255.255.251",  # Invalid mask - gaps
            "255.255.255.127",  # Invalid mask - not starting from left
            "255.255.254.255",  # Invalid mask - non-contiguous
            "255.254.255.0",  # Invalid mask - gaps in middle
            "128.255.255.255",  # Invalid mask - not left-aligned
            "256.255.255.255",  # Invalid IP address
            "255.256.255.255",  # Invalid IP address
            "255.255.256.255",  # Invalid IP address
            "255.255.255.256",  # Invalid IP address
            "not.an.ip.addr",  # Not an IP address
            "",  # Empty string
            "192.168.1.1",  # Valid IP but not a valid netmask
        ],
    )
    def test_invalid_netmasks(self, invalid_netmask) -> None:
        """Test that invalid netmasks raise ValueError."""
        with pytest.raises(ValueError, match="Invalid netmask"):
            ipv4_mask_len(invalid_netmask)

    def test_zero_netmask(self) -> None:
        """Test special case of 0.0.0.0 netmask."""
        assert ipv4_mask_len("0.0.0.0") == 0

    def test_netmask_error_message(self) -> None:
        """Test that error message includes the invalid netmask."""
        invalid_mask = "255.255.255.253"

        with pytest.raises(ValueError) as exc_info:
            ipv4_mask_len(invalid_mask)

        assert invalid_mask in str(exc_info.value)
        assert "Invalid netmask" in str(exc_info.value)

    def test_netmask_calculation_algorithm(self) -> None:
        """Test the bit manipulation algorithm directly."""
        # Test some known cases to verify the algorithm

        # 255.255.255.0 = 24 bits
        # Binary: 11111111.11111111.11111111.00000000
        # This should have 24 consecutive 1s from the left
        assert ipv4_mask_len("255.255.255.0") == 24

        # 255.240.0.0 = 12 bits
        # Binary: 11111111.11110000.00000000.00000000
        # This should have 12 consecutive 1s from the left
        assert ipv4_mask_len("255.240.0.0") == 12

    def test_common_subnet_masks(self) -> None:
        """Test commonly used subnet masks."""
        common_masks = {
            "255.255.255.0": 24,  # Most common
            "255.255.0.0": 16,  # Class B
            "255.0.0.0": 8,  # Class A
            "255.255.255.128": 25,  # Half of /24
            "255.255.255.192": 26,  # Quarter of /24
        }

        for mask, expected in common_masks.items():
            assert ipv4_mask_len(mask) == expected
