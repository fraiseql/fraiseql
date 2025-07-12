"""Authentication module for FraiseQL."""

from fraiseql.auth.auth0 import Auth0Config, Auth0Provider
from fraiseql.auth.auth0_with_revocation import Auth0ProviderWithRevocation
from fraiseql.auth.base import AuthProvider, UserContext
from fraiseql.auth.decorators import requires_auth, requires_permission, requires_role

# Import non-Redis classes first
from fraiseql.auth.token_revocation import (
    InMemoryRevocationStore,
    RevocationConfig,
    TokenRevocationMixin,
    TokenRevocationService,
)

# Lazy import Redis-dependent classes
try:
    from fraiseql.auth.token_revocation import RedisRevocationStore

    _HAS_REDIS = True
except ImportError:
    _HAS_REDIS = False

    class RedisRevocationStore:
        def __init__(self, *args, **kwargs):
            raise ImportError(
                "Redis is required for RedisRevocationStore. "
                "Install it with: pip install fraiseql[redis]",
            )


__all__ = [
    "Auth0Config",
    "Auth0Provider",
    "Auth0ProviderWithRevocation",
    "AuthProvider",
    "InMemoryRevocationStore",
    "RedisRevocationStore",
    "RevocationConfig",
    "TokenRevocationMixin",
    "TokenRevocationService",
    "UserContext",
    "requires_auth",
    "requires_permission",
    "requires_role",
]
