"""FraiseQL Relay Extension - Python Integration Layer

This package provides seamless integration between the FraiseQL Relay PostgreSQL
extension and Python GraphQL applications using FraiseQL.
"""

from .discovery import EntityDiscovery, discover_and_register_entities
from .relay import RelayIntegration, enable_relay_support
from .types import GlobalID, Node, RelayContext

__version__ = "1.0.0"
__all__ = [
    "EntityDiscovery",
    "GlobalID",
    "Node",
    "RelayContext",
    "RelayIntegration",
    "discover_and_register_entities",
    "enable_relay_support",
]
