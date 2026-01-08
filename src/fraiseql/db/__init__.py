"""FraiseQL database module - Python API layer for database operations.

This module provides a Python-first API for database access that coordinates
with the Rust execution pipeline. The module is organized to separate concerns:

- Python API Layer: repository.py (user-facing)
- Rust Coordination: executor.py (internal boundary)
- Query Building: query_builder methods (pure Python)
- Session Management: session methods (Postgres variables)
- Type Registry: registry.py (type management)
- Connection Pools: pool.py (pool factories)

Philosophy: "Python API Exposure + Rust Core"
- Users interact with a clean Python API (FraiseQLRepository)
- Internal Rust coordination is isolated and replaceable
- Clear separation between Python logic and Rust execution engine
"""

# Note: During Phase 5 refactoring, we're extracting modules from the monolithic db.py
# The original db.py file is renamed db_core.py to avoid conflicts with this __init__.py
# This __init__.py exports the public API from extracted modules

# Extract 1: Connection pool factories
# Re-export the original execute_via_rust_pipeline for backward compatibility with tests
from fraiseql.core.rust_pipeline import execute_via_rust_pipeline

# Extract 3: Rust coordination boundary
from fraiseql.db.executor import (
    _NULL_RESPONSE_CACHE,  # Internal cache for tests
    execute_query_via_rust,
    execute_transaction,
    is_rust_response_null,
)
from fraiseql.db.pool import (
    create_legacy_pool,
    create_production_pool,
    create_prototype_pool,
)

# Extract 2: Type registry and metadata management
from fraiseql.db.registry import (
    _table_metadata,
    _type_registry,
    clear_type_registry,
    register_type_for_view,
)

# Backward compatibility: old name for is_rust_response_null
_is_rust_response_null = is_rust_response_null

# Main repository class - imported from the interim location
# This will eventually be from fraiseql.db.repository, but during migration
# it comes from the core module
try:
    # Try to import from repository.py (if it exists)
    from fraiseql.db.repository import FraiseQLRepository
except ImportError:
    # Fall back to importing from core (monolithic db_core.py)
    # This maintains backward compatibility during the refactoring
    from fraiseql.db_core import FraiseQLRepository  # type: ignore[import,no-redef]

__all__ = [
    "_NULL_RESPONSE_CACHE",
    "FraiseQLRepository",
    "_is_rust_response_null",
    "_table_metadata",
    "_type_registry",
    "clear_type_registry",
    "create_legacy_pool",
    "create_production_pool",
    "create_prototype_pool",
    "execute_query_via_rust",
    "execute_transaction",
    "execute_via_rust_pipeline",
    "is_rust_response_null",
    "register_type_for_view",
]
