"""Enhanced registry pattern with dependency injection.

This module provides a scoped registry implementation that reduces
global state and import order dependencies.
"""

from contextlib import contextmanager
from dataclasses import dataclass, field
from typing import Protocol


class ResultRegistry(Protocol):
    """Protocol for result type registries."""

    def register(self, success_cls: type, error_cls: type) -> None:
        """Register a success/error type pair."""
        ...

    def get_error_type(self, success_cls: type) -> type | None:
        """Get the error type for a success type."""
        ...


@dataclass
class ScopedResultRegistry:
    """Scoped registry that can be injected.

    This registry maintains its own mappings and can optionally
    inherit from a parent registry, allowing for hierarchical scoping.
    """

    _mappings: dict[type, type] = field(default_factory=dict)
    _parent: ResultRegistry | None = None

    def register(self, success_cls: type, error_cls: type) -> None:
        """Register a success/error type pair in this scope."""
        self._mappings[success_cls] = error_cls

    def get_error_type(self, success_cls: type) -> type | None:
        """Get the error type for a success type.

        Checks local mappings first, then falls back to parent if available.
        """
        # Check local mappings first
        if success_cls in self._mappings:
            return self._mappings[success_cls]

        # Fall back to parent if available
        if self._parent:
            return self._parent.get_error_type(success_cls)

        return None

    def create_child(self) -> "ScopedResultRegistry":
        """Create a child registry for isolation."""
        return ScopedResultRegistry(_parent=self)

    def clear(self) -> None:
        """Clear all local mappings."""
        self._mappings.clear()

    def merge_from(self, other: "ScopedResultRegistry") -> None:
        """Merge mappings from another registry."""
        self._mappings.update(other._mappings)


@contextmanager
def isolated_registry():
    """Create an isolated registry scope.

    Usage:
        with isolated_registry() as registry:
            registry.register(MySuccess, MyError)
            # Use registry
        # Registry is cleaned up automatically
    """
    registry = ScopedResultRegistry()
    yield registry
    # Registry is automatically garbage collected


class RegistryManager:
    """Manages registry instances and provides dependency injection."""

    def __init__(self) -> None:
        self._default_registry = ScopedResultRegistry()
        self._current_registry = self._default_registry

    @property
    def current(self) -> ResultRegistry:
        """Get the current active registry."""
        return self._current_registry

    @contextmanager
    def scoped_registry(self, inherit: bool = True):
        """Create a scoped registry context.

        Args:
            inherit: Whether to inherit from the current registry
        """
        previous = self._current_registry

        new_registry = previous.create_child() if inherit else ScopedResultRegistry()

        self._current_registry = new_registry

        try:
            yield new_registry
        finally:
            self._current_registry = previous

    def reset(self) -> None:
        """Reset to default registry."""
        self._default_registry.clear()
        self._current_registry = self._default_registry


# Global registry manager instance
_registry_manager = RegistryManager()


def get_registry() -> ResultRegistry:
    """Get the current active registry."""
    return _registry_manager.current


def scoped_registry(inherit: bool = True):
    """Create a scoped registry context.

    Args:
        inherit: Whether to inherit from the current registry

    Usage:
        with scoped_registry() as registry:
            registry.register(MySuccess, MyError)
            # Use registry
    """
    return _registry_manager.scoped_registry(inherit)


def reset_registry() -> None:
    """Reset the global registry to empty state."""
    _registry_manager.reset()
