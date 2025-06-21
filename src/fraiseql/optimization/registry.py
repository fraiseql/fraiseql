"""Registry for managing DataLoader instances per request."""

from __future__ import annotations

import inspect
from contextvars import ContextVar
from typing import Any, TypeVar

from fraiseql.optimization.dataloader import DataLoader

T = TypeVar("T", bound=DataLoader)

# Context variable for request-scoped registry
_loader_registry: ContextVar[LoaderRegistry | None] = ContextVar("loader_registry", default=None)


class LoaderRegistry:
    """Manages DataLoader instances for a request."""

    def __init__(self, db: Any) -> None:
        self.db = db
        self._loaders: dict[type[DataLoader], DataLoader] = {}
        self._custom_loaders: dict[str, DataLoader] = {}

    def get_loader(self, loader_class: type[T], **kwargs) -> T:
        """Get or create a DataLoader instance."""
        # Check if already exists
        if loader_class in self._loaders:
            return self._loaders[loader_class]

        # Create new instance
        loader = loader_class(db=self.db, **kwargs)
        self._loaders[loader_class] = loader

        return loader

    def register_loader(self, name: str, loader: DataLoader) -> None:
        """Register a custom loader instance."""
        self._custom_loaders[name] = loader

    def get_custom_loader(self, name: str) -> DataLoader | None:
        """Get a custom loader by name."""
        return self._custom_loaders.get(name)

    def clear_all(self) -> None:
        """Clear all loader caches."""
        # CRITICAL: Prevent memory leaks by properly clearing all references
        try:
            for loader in list(self._loaders.values()):
                loader.clear()
            for loader in list(self._custom_loaders.values()):
                loader.clear()
        finally:
            # Force clear dictionaries to prevent memory leaks
            self._loaders.clear()
            self._custom_loaders.clear()

    @classmethod
    def get_current(cls) -> LoaderRegistry | None:
        """Get the current request's registry."""
        return _loader_registry.get()

    @classmethod
    def set_current(cls, registry: LoaderRegistry) -> None:
        """Set the current request's registry."""
        _loader_registry.set(registry)


# Helper function for resolvers
def get_loader(loader_class: type[T], **kwargs) -> T:
    """Get a DataLoader for the current request."""
    registry = LoaderRegistry.get_current()
    if not registry:
        msg = (
            "No LoaderRegistry in context. This indicates a critical setup error - "
            "DataLoader registry was not properly initialized for this request. "
            "Ensure middleware is correctly configured."
        )
        raise RuntimeError(
            msg,
        )

    # SECURITY: Validate loader class to prevent injection
    if not inspect.isclass(loader_class) or not issubclass(loader_class, DataLoader):
        msg = f"Invalid loader class: {loader_class}"
        raise ValueError(msg)

    return registry.get_loader(loader_class, **kwargs)
