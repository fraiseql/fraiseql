"""Subscription decorators and schema generation.

Runtime subscriptions are implemented in Rust fraiseql-server.
Python provides subscription decoration and GraphQL schema generation.
"""

from .decorators import subscription

__all__ = ["subscription"]
