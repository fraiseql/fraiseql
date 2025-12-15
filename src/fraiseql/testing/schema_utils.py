"""Utilities for GraphQL schema testing and manipulation.

Provides helpers for clearing caches, refreshing schemas, and managing
schema state during testing.
"""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from graphql import GraphQLSchema

logger = logging.getLogger(__name__)


def clear_fraiseql_caches() -> None:
    """Clear all FraiseQL internal caches.

    Clears:
    - Python GraphQL type cache (_graphql_type_cache)
    - Type-to-view mapping registry (_type_registry)
    - Python SchemaRegistry singleton
    - Rust schema registry (if available)

    This is useful when rebuilding the schema or resetting test state.

    Note:
        This does NOT clear FastAPI dependencies (db_pool, auth_provider).
        Use clear_fraiseql_state() for complete cleanup.

    Example:
        >>> clear_fraiseql_caches()
        >>> # Caches cleared, ready to rebuild schema
    """
    # Clear GraphQL type cache
    try:
        from fraiseql.core.graphql_type import _graphql_type_cache

        _graphql_type_cache.clear()
        logger.debug("Cleared GraphQL type cache")
    except ImportError:
        logger.debug("GraphQL type cache not available")
    except Exception as e:
        logger.warning(f"Failed to clear GraphQL type cache: {e}")

    # Clear type registry
    try:
        from fraiseql.db import _type_registry

        _type_registry.clear()
        logger.debug("Cleared type registry")
    except ImportError:
        logger.debug("Type registry not available")
    except Exception as e:
        logger.warning(f"Failed to clear type registry: {e}")

    # Clear Python SchemaRegistry
    try:
        from fraiseql.gql.builders import SchemaRegistry

        SchemaRegistry.get_instance().clear()
        logger.debug("Cleared Python SchemaRegistry")
    except ImportError:
        logger.debug("SchemaRegistry not available")
    except Exception as e:
        logger.warning(f"Failed to clear SchemaRegistry: {e}")

    # Reset Rust schema registry
    try:
        from fraiseql._fraiseql_rs import reset_schema_registry_for_testing

        reset_schema_registry_for_testing()
        logger.debug("Reset Rust schema registry")
    except ImportError:
        logger.debug("Rust schema registry not available")
    except Exception as e:
        logger.warning(f"Failed to reset Rust registry: {e}")


def clear_fraiseql_state() -> None:
    """Clear all FraiseQL state including caches and FastAPI dependencies.

    This performs a complete cleanup:
    1. All caches (via clear_fraiseql_caches())
    2. FastAPI global dependencies (db_pool, auth_provider, config)

    Use this for complete teardown in test fixtures.

    Example:
        >>> @pytest.fixture(scope="session", autouse=True)
        >>> def cleanup_after_tests():
        >>>     yield
        >>>     clear_fraiseql_state()
    """
    # Clear all caches first
    clear_fraiseql_caches()

    # Reset FastAPI global dependencies
    try:
        from fraiseql.fastapi.dependencies import (
            set_auth_provider,
            set_db_pool,
            set_fraiseql_config,
        )

        set_db_pool(None)
        set_auth_provider(None)
        set_fraiseql_config(None)
        logger.debug("Reset FastAPI dependencies")
    except ImportError:
        logger.debug("FastAPI dependencies not available")
    except Exception as e:
        logger.warning(f"Failed to reset FastAPI dependencies: {e}")


def validate_schema_refresh(
    old_schema: GraphQLSchema,
    new_schema: GraphQLSchema,
    *,
    expect_new_types: bool = False,
) -> dict[str, set[str]]:
    """Validate that a schema refresh preserved existing elements.

    Args:
        old_schema: The schema before refresh
        new_schema: The schema after refresh
        expect_new_types: If True, verify new schema has MORE types than old

    Returns:
        Dictionary with:
        - "preserved_types": Type names present in both schemas
        - "new_types": Type names only in new schema
        - "lost_types": Type names only in old schema (should be empty!)

    Raises:
        AssertionError: If schema refresh lost types or mutations

    Example:
        >>> old = app.state.graphql_schema
        >>> await app.refresh_schema()
        >>> new = app.state.graphql_schema
        >>> result = validate_schema_refresh(old, new, expect_new_types=True)
        >>> assert len(result["lost_types"]) == 0
    """
    old_types = set(old_schema.type_map.keys())
    new_types = set(new_schema.type_map.keys())

    preserved = old_types & new_types
    added = new_types - old_types
    lost = old_types - new_types

    # Validate no types were lost
    if lost:
        logger.error(f"Schema refresh lost types: {lost}")
        raise AssertionError(f"Schema refresh lost {len(lost)} types: {lost}")

    # Validate mutations preserved
    if old_schema.mutation_type and new_schema.mutation_type:
        old_mutations = set(old_schema.mutation_type.fields.keys())
        new_mutations = set(new_schema.mutation_type.fields.keys())
        lost_mutations = old_mutations - new_mutations

        if lost_mutations:
            logger.error(f"Schema refresh lost mutations: {lost_mutations}")
            raise AssertionError(
                f"Schema refresh lost {len(lost_mutations)} mutations: {lost_mutations}"
            )

    # Log summary
    logger.info(f"Schema refresh: {len(preserved)} preserved, {len(added)} added, {len(lost)} lost")

    if expect_new_types and not added:
        logger.warning("Expected new types but none were added")

    return {
        "preserved_types": preserved,
        "new_types": added,
        "lost_types": lost,
    }
