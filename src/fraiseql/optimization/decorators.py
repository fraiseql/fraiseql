"""Decorators for DataLoader integration."""

import inspect
from collections.abc import Awaitable
from typing import Any, Callable, Type, TypeVar, get_type_hints

from fraiseql.optimization.dataloader import DataLoader
from fraiseql.optimization.registry import get_loader

F = TypeVar("F", bound=Callable[..., Awaitable[Any]])


def dataloader_field(
    loader_class: Type[DataLoader], *, key_field: str, description: str | None = None
) -> Callable[[F], F]:
    """Decorator to automatically use DataLoader for field resolution.

    This decorator automatically implements DataLoader-based field resolution,
    eliminating the need to manually call get_loader() in every field resolver.

    Args:
        loader_class: DataLoader class to use for loading
        key_field: Field name on the parent object containing the key to load
        description: Optional field description for GraphQL schema

    Usage:
        @fraiseql.type
        class Post:
            author_id: UUID

            @fraiseql.dataloader_field(UserDataLoader, key_field="author_id")
            async def author(self, info) -> Optional[User]:
                '''Load post author using DataLoader.'''
                pass  # Implementation is auto-generated

    The decorator will:
    1. Mark the method with metadata for schema building
    2. Auto-implement the method to use the specified DataLoader
    3. Handle key extraction from the parent object
    4. Return properly typed results
    """
    # Validation
    if not inspect.isclass(loader_class) or not issubclass(loader_class, DataLoader):
        raise ValueError("loader_class must be a DataLoader subclass")

    if not key_field:
        raise ValueError("key_field is required")

    def decorator(method: F) -> F:
        # Get method signature for validation
        sig = inspect.signature(method)
        hints = get_type_hints(method)

        # Validate method signature
        params = list(sig.parameters.keys())
        if len(params) < 2 or params[0] != "self" or params[1] != "info":
            raise ValueError(
                f"@dataloader_field decorated method {method.__name__} must have "
                "signature (self, info) -> ReturnType"
            )

        # Get return type for validation
        return_type = hints.get("return")
        if return_type is None:
            raise ValueError(
                f"@dataloader_field decorated method {method.__name__} must have "
                "a return type annotation"
            )

        # Create the auto-implemented resolver
        async def auto_resolver(self, info):
            """Auto-generated DataLoader resolver."""
            # SECURITY: Validate self object to prevent attribute injection attacks
            if not hasattr(self, key_field):
                raise AttributeError(
                    f"Object {type(self).__name__} does not have required field '{key_field}'. "
                    "This may indicate a security issue or misconfiguration."
                )

            # Get the key value from the parent object with validation
            key_value = getattr(self, key_field, None)
            if key_value is None:
                return None

            # SECURITY: Validate key_value type to prevent injection
            if not isinstance(key_value, (str, int, bytes, type(None))) and not hasattr(
                key_value, "__hash__"
            ):
                raise ValueError(
                    f"Key field '{key_field}' must be hashable, got {type(key_value)}"
                )

            # Get the DataLoader instance
            loader = get_loader(loader_class)

            # Load the value
            result_data = await loader.load(key_value)
            if result_data is None:
                return None

            # Convert result to proper type - SECURITY CRITICAL
            # Validate and sanitize before any type construction
            if result_data is None:
                return None

            try:
                # Handle Optional[Type] and similar generic types
                if hasattr(return_type, "__origin__"):
                    args = getattr(return_type, "__args__", ())
                    if args:
                        target_type = args[0]
                        # SECURITY: Only allow safe type construction
                        if hasattr(target_type, "from_dict") and callable(
                            target_type.from_dict
                        ):
                            if isinstance(result_data, dict):
                                return target_type.from_dict(result_data)
                        elif hasattr(target_type, "__annotations__") and isinstance(
                            result_data, dict
                        ):
                            # Only construct if we have annotations (dataclass-like)
                            annotations = getattr(target_type, "__annotations__", {})
                            filtered_data = {
                                k: v for k, v in result_data.items() if k in annotations
                            }
                            return target_type(**filtered_data)
                    return result_data

                # Handle direct type construction
                elif hasattr(return_type, "from_dict") and callable(
                    return_type.from_dict
                ):
                    if isinstance(result_data, dict):
                        return return_type.from_dict(result_data)
                elif hasattr(return_type, "__annotations__") and isinstance(
                    result_data, dict
                ):
                    # Only construct if we have annotations (dataclass-like)
                    annotations = getattr(return_type, "__annotations__", {})
                    filtered_data = {
                        k: v for k, v in result_data.items() if k in annotations
                    }
                    return return_type(**filtered_data)

                # Fallback: return raw data (safer than arbitrary construction)
                return result_data

            except Exception:
                # CRITICAL: Never expose internal errors to prevent information leakage
                raise RuntimeError(
                    f"DataLoader type conversion failed for {return_type.__name__ if hasattr(return_type, '__name__') else 'unknown type'}"
                ) from None

        # Preserve method metadata
        auto_resolver.__name__ = method.__name__
        auto_resolver.__doc__ = (
            method.__doc__ or f"Auto-generated DataLoader field for {key_field}"
        )
        auto_resolver.__annotations__ = method.__annotations__

        # Add DataLoader metadata for schema building
        auto_resolver.__fraiseql_dataloader__ = {
            "loader_class": loader_class,
            "key_field": key_field,
            "description": description,
            "original_method": method,
            "auto_generated": True,
        }

        # Mark as a field resolver
        auto_resolver.__fraiseql_field__ = True
        auto_resolver.__fraiseql_field_description__ = description

        return auto_resolver

    return decorator
