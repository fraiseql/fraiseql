# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 19
@fraise_input
class NetworkAddressFilter:
    """Enhanced filter for IP addresses - EXCLUDES pattern matching operators."""

    # Basic operations
    eq: str | None = None
    neq: str | None = None
    in_: list[str] | None = fraise_field(default=None, graphql_name="in")
    nin: list[str] | None = None
    isnull: bool | None = None

    # Network-specific operations
    inSubnet: str | None = None  # IP is in CIDR subnet
    inRange: IPRange | None = None  # IP is in range
    isPrivate: bool | None = None  # RFC 1918 private
    isPublic: bool | None = None  # Non-private
    isIPv4: bool | None = None  # IPv4-specific
    isIPv6: bool | None = None  # IPv6-specific
    isLoopback: bool | None = None
    isLinkLocal: bool | None = None
    isMulticast: bool | None = None
    isDocumentation: bool | None = None
    isCarrierGrade: bool | None = None
    # NOTE: contains, startswith, endswith are INTENTIONALLY EXCLUDED
