"""Auto-discovery system for FraiseQL GraphQL items.

This module provides automatic discovery of decorated GraphQL items
(types, mutations, queries, subscriptions) from modules and packages.

The discovery system scans Python modules for items decorated with:
- @fraiseql.type
- @fraiseql.mutation
- @fraiseql.query
- @fraiseql.subscription
- @fraiseql.enum
- @fraiseql.input
- @fraiseql.interface

And automatically registers them to the AxumRegistry.

# Usage

## Discover from Single Module

```python
from fraiseql.axum.discovery import discover_from_module

result = discover_from_module("myapp.types")
print(result.summary())
# Output:
# Discovery Result for myapp.types:
#   Types: 3 (User, Post, Comment)
#   Mutations: 2 (createUser, deletePost)
#   Queries: 1 (getUsers)
#   Errors: 0
```

## Discover from Package

```python
from fraiseql.axum.discovery import discover_from_package

result = discover_from_package("myapp")
print(result.summary())
# Output:
# Discovery Result for myapp:
#   Types: 10 (User, Post, Comment, ...)
#   Mutations: 8 (...)
#   Queries: 5 (...)
#   Subscriptions: 2 (...)
#   Errors: 0
```

## Integration with Registry

Discovery automatically registers found items to AxumRegistry:

```python
from fraiseql.axum.discovery import discover_from_package
from fraiseql.axum.registry import AxumRegistry

result = discover_from_package("myapp")

if result.errors:
    print(f"Warning: {len(result.errors)} errors during discovery")

registry = AxumRegistry.get_instance()
print(f"Total items registered: {registry.count_registered()['total']}")
```

## Error Handling

Discovery continues even if individual modules fail to import:

```python
from fraiseql.axum.discovery import discover_from_package

result = discover_from_package("myapp")

if result.errors:
    print("Discovery encountered errors:")
    for error in result.errors:
        print(f"  - {error}")

# Types/mutations/etc are still populated for successful modules
print(f"Found {len(result.types_found)} types despite errors")
```
"""

import importlib
import importlib.util
import inspect
import logging
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Iterator

from fraiseql.axum.registry import AxumRegistry

logger = logging.getLogger(__name__)


@dataclass
class DiscoveryResult:
    """Result of GraphQL item discovery.

    Attributes:
        types_found: List of discovered type classes
        inputs_found: List of discovered input classes
        enums_found: List of discovered enum classes
        interfaces_found: List of discovered interface classes
        mutations_found: List of discovered mutation callables/classes
        queries_found: List of discovered query callables
        subscriptions_found: List of discovered subscription callables
        errors: List of exceptions encountered during discovery
        source: Name of the module/package being discovered
    """

    source: str
    types_found: list[type[Any]] = None
    inputs_found: list[type[Any]] = None
    enums_found: list[type[Any]] = None
    interfaces_found: list[type[Any]] = None
    mutations_found: list[Any] = None
    queries_found: list[Callable[..., Any]] = None
    subscriptions_found: list[Callable[..., Any]] = None
    errors: list[Exception] = None

    def __post_init__(self):
        """Initialize empty lists if not provided."""
        if self.types_found is None:
            self.types_found = []
        if self.inputs_found is None:
            self.inputs_found = []
        if self.enums_found is None:
            self.enums_found = []
        if self.interfaces_found is None:
            self.interfaces_found = []
        if self.mutations_found is None:
            self.mutations_found = []
        if self.queries_found is None:
            self.queries_found = []
        if self.subscriptions_found is None:
            self.subscriptions_found = []
        if self.errors is None:
            self.errors = []

    def register_to_registry(self) -> None:
        """Register all discovered items to AxumRegistry.

        Examples:
            ```python
            result = discover_from_package("myapp")
            result.register_to_registry()

            registry = AxumRegistry.get_instance()
            print(f"Registered {registry.count_registered()['total']} items")
            ```
        """
        registry = AxumRegistry.get_instance()

        if self.types_found:
            registry.register_types(self.types_found)

        if self.inputs_found:
            for input_ in self.inputs_found:
                registry.register_input(input_)

        if self.enums_found:
            for enum_ in self.enums_found:
                registry.register_enum(enum_)

        if self.interfaces_found:
            for iface in self.interfaces_found:
                registry.register_interface(iface)

        if self.mutations_found:
            registry.register_mutations(self.mutations_found)

        if self.queries_found:
            registry.register_queries(self.queries_found)

        if self.subscriptions_found:
            registry.register_subscriptions(self.subscriptions_found)

        logger.info(f"Registered {self.count_total()} items to AxumRegistry")

    def count_total(self) -> int:
        """Get total count of discovered items."""
        return (
            len(self.types_found)
            + len(self.inputs_found)
            + len(self.enums_found)
            + len(self.interfaces_found)
            + len(self.mutations_found)
            + len(self.queries_found)
            + len(self.subscriptions_found)
        )

    def summary(self) -> str:
        """Get human-readable discovery summary.

        Returns:
            Formatted summary string

        Examples:
            ```python
            result = discover_from_package("myapp")
            print(result.summary())
            ```
        """
        lines = [f"Discovery Result for {self.source}:"]

        if self.types_found:
            type_names = ", ".join(t.__name__ for t in self.types_found)
            lines.append(f"  Types: {len(self.types_found)} ({type_names})")

        if self.inputs_found:
            input_names = ", ".join(t.__name__ for t in self.inputs_found)
            lines.append(f"  Inputs: {len(self.inputs_found)} ({input_names})")

        if self.enums_found:
            enum_names = ", ".join(t.__name__ for t in self.enums_found)
            lines.append(f"  Enums: {len(self.enums_found)} ({enum_names})")

        if self.interfaces_found:
            iface_names = ", ".join(t.__name__ for t in self.interfaces_found)
            lines.append(f"  Interfaces: {len(self.interfaces_found)} ({iface_names})")

        if self.mutations_found:
            mut_names = ", ".join(m.__name__ for m in self.mutations_found)
            lines.append(f"  Mutations: {len(self.mutations_found)} ({mut_names})")

        if self.queries_found:
            query_names = ", ".join(q.__name__ for q in self.queries_found)
            lines.append(f"  Queries: {len(self.queries_found)} ({query_names})")

        if self.subscriptions_found:
            sub_names = ", ".join(s.__name__ for s in self.subscriptions_found)
            lines.append(f"  Subscriptions: {len(self.subscriptions_found)} ({sub_names})")

        if self.errors:
            lines.append(f"  Errors: {len(self.errors)}")
            for error in self.errors:
                lines.append(f"    - {type(error).__name__}: {error}")

        if self.count_total() == 0 and not self.errors:
            lines.append("  (no items found)")

        return "\n".join(lines)


def discover_from_module(module_name: str) -> DiscoveryResult:
    """Discover GraphQL items in a single module.

    Scans a module for decorated GraphQL items and returns them
    without automatically registering them.

    Args:
        module_name: Fully qualified module name (e.g., "myapp.types")

    Returns:
        DiscoveryResult with found items and any errors

    Examples:
        ```python
        from fraiseql.axum.discovery import discover_from_module

        result = discover_from_module("myapp.types")
        print(result.summary())

        # Manually register if needed
        result.register_to_registry()
        ```
    """
    result = DiscoveryResult(source=module_name)

    # Try to import the module
    try:
        module = importlib.import_module(module_name)
    except ImportError as e:
        result.errors.append(e)
        logger.warning(f"Failed to import module {module_name}: {e}")
        return result

    # Scan module for GraphQL items
    try:
        for name, obj in inspect.getmembers(module):
            # Skip private items
            if name.startswith("_"):
                continue

            # Check if it's a fraiseql type
            if _is_fraiseql_type(obj):
                result.types_found.append(obj)
            elif _is_fraiseql_input(obj):
                result.inputs_found.append(obj)
            elif _is_fraiseql_enum(obj):
                result.enums_found.append(obj)
            elif _is_fraiseql_interface(obj):
                result.interfaces_found.append(obj)
            elif _is_fraiseql_mutation(obj):
                result.mutations_found.append(obj)
            elif _is_fraiseql_query(obj):
                result.queries_found.append(obj)
            elif _is_fraiseql_subscription(obj):
                result.subscriptions_found.append(obj)
    except Exception as e:
        result.errors.append(e)
        logger.error(f"Error scanning module {module_name}: {e}")

    logger.debug(
        f"Discovered {result.count_total()} items in {module_name}, "
        f"{len(result.errors)} errors"
    )

    return result


def discover_from_package(package_name: str) -> DiscoveryResult:
    """Discover GraphQL items in a package and subpackages.

    Recursively scans all Python modules in a package for decorated
    GraphQL items. Continues scanning even if individual modules fail.

    Args:
        package_name: Fully qualified package name (e.g., "myapp")

    Returns:
        DiscoveryResult with all found items and any errors

    Examples:
        ```python
        from fraiseql.axum.discovery import discover_from_package

        result = discover_from_package("myapp")
        print(result.summary())

        # Auto-register all found items
        result.register_to_registry()
        ```

        With error handling:
        ```python
        result = discover_from_package("myapp")

        if result.errors:
            print(f"Warning: {len(result.errors)} errors during discovery")

        # Continue despite errors
        result.register_to_registry()
        ```
    """
    result = DiscoveryResult(source=package_name)

    # Get package location
    try:
        package = importlib.import_module(package_name)
    except ImportError as e:
        result.errors.append(e)
        logger.error(f"Failed to import package {package_name}: {e}")
        return result

    package_path = package.__path__[0] if hasattr(package, "__path__") else None

    if not package_path:
        # Not a package, just import as module
        return discover_from_module(package_name)

    # Walk all Python files in package
    try:
        for module_name in _walk_package_modules(package_name, Path(package_path)):
            module_result = discover_from_module(module_name)

            # Aggregate results
            result.types_found.extend(module_result.types_found)
            result.inputs_found.extend(module_result.inputs_found)
            result.enums_found.extend(module_result.enums_found)
            result.interfaces_found.extend(module_result.interfaces_found)
            result.mutations_found.extend(module_result.mutations_found)
            result.queries_found.extend(module_result.queries_found)
            result.subscriptions_found.extend(module_result.subscriptions_found)
            result.errors.extend(module_result.errors)
    except Exception as e:
        result.errors.append(e)
        logger.error(f"Error walking package {package_name}: {e}")

    logger.debug(
        f"Discovered {result.count_total()} items in package {package_name}, "
        f"{len(result.errors)} errors"
    )

    return result


# ===== Helper Functions =====


def _is_fraiseql_type(obj: Any) -> bool:
    """Check if object is a @fraiseql.type decorated class."""
    if not inspect.isclass(obj):
        return False

    # Check for _fraiseql_type marker (set by decorator)
    return hasattr(obj, "_fraiseql_type") and getattr(obj, "_fraiseql_type", False)


def _is_fraiseql_input(obj: Any) -> bool:
    """Check if object is a @fraiseql.input decorated class."""
    if not inspect.isclass(obj):
        return False

    return hasattr(obj, "_fraiseql_input") and getattr(obj, "_fraiseql_input", False)


def _is_fraiseql_enum(obj: Any) -> bool:
    """Check if object is a @fraiseql.enum decorated class."""
    if not inspect.isclass(obj):
        return False

    return hasattr(obj, "_fraiseql_enum") and getattr(obj, "_fraiseql_enum", False)


def _is_fraiseql_interface(obj: Any) -> bool:
    """Check if object is a @fraiseql.interface decorated class."""
    if not inspect.isclass(obj):
        return False

    return hasattr(obj, "_fraiseql_interface") and getattr(obj, "_fraiseql_interface", False)


def _is_fraiseql_mutation(obj: Any) -> bool:
    """Check if object is a @fraiseql.mutation decorated function/class."""
    # Check for _fraiseql_mutation marker
    return hasattr(obj, "_fraiseql_mutation") and getattr(obj, "_fraiseql_mutation", False)


def _is_fraiseql_query(obj: Any) -> bool:
    """Check if object is a @fraiseql.query decorated function."""
    # Check for _fraiseql_query marker
    return hasattr(obj, "_fraiseql_query") and getattr(obj, "_fraiseql_query", False)


def _is_fraiseql_subscription(obj: Any) -> bool:
    """Check if object is a @fraiseql.subscription decorated function."""
    # Check for _fraiseql_subscription marker
    return hasattr(obj, "_fraiseql_subscription") and getattr(
        obj, "_fraiseql_subscription", False
    )


def _walk_package_modules(package_name: str, package_path: Path) -> Iterator[str]:
    """Walk all Python modules in a package recursively.

    Args:
        package_name: Fully qualified package name
        package_path: File system path to package

    Yields:
        Fully qualified module names
    """
    for py_file in package_path.rglob("*.py"):
        # Skip __pycache__ and private files
        if "__pycache__" in py_file.parts or py_file.name.startswith("_"):
            continue

        # Convert file path to module name
        relative_path = py_file.relative_to(package_path.parent)
        module_name = str(relative_path.with_suffix("")).replace("/", ".")

        yield module_name
