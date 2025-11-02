# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 2
class IpAddressField(str, ScalarMarker):
    """Represents a validated IP address."""

    __slots__ = ()
