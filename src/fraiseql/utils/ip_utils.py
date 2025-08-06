"""IPv4 Address and Subnet Mask Utility Module."""

import ipaddress

IPV4_OCTETS = 4
MAX_OCTET_VALUE = 256
MAX_IPV4_VALUE = 0xFFFFFFFF


def is_ipv4_address(dotquad: str) -> bool:
    """Validate an IPv4 address in dotted-quad notation."""
    octets = dotquad.split(".")
    return len(octets) == IPV4_OCTETS and all(
        o.isdigit() and 0 <= int(o) < MAX_OCTET_VALUE for o in octets
    )


def ipv4_mask_len(dotquad: str) -> int:
    """Return the number of bits set in the IPv4 netmask."""
    if not is_ipv4_address(dotquad):
        msg = f"Invalid netmask: {dotquad}"
        raise ValueError(msg)

    a, b, c, d = (int(octet) for octet in dotquad.split("."))
    mask = a << 24 | b << 16 | c << 8 | d

    if mask == 0:
        return 0

    m = mask & -mask
    right0bits = -1
    while m:
        m >>= 1
        right0bits += 1

    if mask | ((1 << right0bits) - 1) != MAX_IPV4_VALUE:
        msg = f"Invalid netmask: {dotquad}"
        raise ValueError(msg)

    return 32 - right0bits


def prefix_length_to_netmask(prefix_length: int | str) -> str:
    """Convert a prefix length to a netmask string."""
    prefix = int(prefix_length)
    net = ipaddress.IPv4Network(f"0.0.0.0/{prefix}")
    return str(net.netmask)


def netmask_to_prefix_length(netmask: str) -> int:
    """Convert a netmask string to prefix length."""
    netmask_int = int(ipaddress.IPv4Address(netmask))
    prefix_length = 0
    while netmask_int & (1 << (31 - prefix_length)):
        prefix_length += 1
    return prefix_length


def parse_subnet_mask(value: str | None) -> ipaddress.IPv4Address | None:
    """Parse a subnet mask string into an IPv4Address if valid."""
    if not value:
        return None

    try:
        _ = ipv4_mask_len(value)
    except ValueError as err:
        msg = f"Invalid netmask: {value}"
        raise ValueError(msg) from err

    return ipaddress.IPv4Address(value)


__all__ = [
    "ipv4_mask_len",
    "is_ipv4_address",
    "netmask_to_prefix_length",
    "parse_subnet_mask",
    "prefix_length_to_netmask",
]
