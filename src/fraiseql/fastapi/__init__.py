"""FastAPI integration for FraiseQL.

Provides seamless integration with FastAPI applications, including
development and production routers with different optimization levels.
"""

from fraiseql.fastapi.app import create_fraiseql_app, create_production_app
from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.dependencies import get_current_user, get_db
from fraiseql.fastapi.turbo import TurboQuery, TurboRegistry, TurboRouter

__all__ = [
    "FraiseQLConfig",
    "TurboQuery",
    "TurboRegistry",
    "TurboRouter",
    "create_fraiseql_app",
    "create_production_app",
    "get_current_user",
    "get_db",
]
