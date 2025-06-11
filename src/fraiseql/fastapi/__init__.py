"""FastAPI integration for FraiseQL.

Provides seamless integration with FastAPI applications, including
development and production routers with different optimization levels.
"""

from fraiseql.fastapi.app import create_fraiseql_app
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.dependencies import get_current_user, get_db

__all__ = [
    "FraiseQLConfig",
    "create_fraiseql_app",
    "get_current_user",
    "get_db",
]
