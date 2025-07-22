"""Native authentication module for FraiseQL."""

from .factory import (
    add_security_middleware,
    apply_native_auth_schema,
    create_native_auth_provider,
    get_native_auth_router,
)
from .models import User
from .provider import NativeAuthProvider
from .tokens import InvalidTokenError, SecurityError, TokenExpiredError, TokenManager

__all__ = [
    "InvalidTokenError",
    "NativeAuthProvider",
    "SecurityError",
    "TokenExpiredError",
    "TokenManager",
    "User",
    "add_security_middleware",
    "apply_native_auth_schema",
    "create_native_auth_provider",
    "get_native_auth_router",
]
