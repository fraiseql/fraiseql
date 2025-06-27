"""Field-level authorization for GraphQL fields."""

from __future__ import annotations

import asyncio
import functools
import warnings
from typing import TYPE_CHECKING, Any, Protocol, TypeVar, Union

from graphql import GraphQLError

if TYPE_CHECKING:
    from collections.abc import Awaitable, Callable

    from graphql import GraphQLResolveInfo


T = TypeVar("T")


class FieldAuthorizationError(GraphQLError):
    """Raised when field authorization fails."""

    def __init__(self, message: str = "Not authorized to access this field") -> None:
        super().__init__(message, extensions={"code": "FIELD_AUTHORIZATION_ERROR"})


class PermissionCheck(Protocol):
    """Protocol for permission check functions."""

    def __call__(
        self,
        info: GraphQLResolveInfo,
        *args: Any,
        **kwargs: Any,
    ) -> Union[bool, Awaitable[bool]]:
        """Check if the field access is authorized."""
        ...


def authorize_field(
    permission_check: PermissionCheck,
    *,
    error_message: str | None = None,
) -> Callable[[T], T]:
    """Decorator to add field-level authorization to GraphQL fields.

    This decorator wraps field resolvers to check permissions before
    allowing access to the field.

    Args:
        permission_check: A callable that takes GraphQLResolveInfo and returns
            a boolean indicating if access is allowed. Can be sync or async.
        error_message: Optional custom error message for authorization failures.

    Returns:
        A decorator that wraps the field resolver with authorization logic.

    Example:
        ```python
        @fraise_type
        class User:
            name: str

            @field
            @authorize_field(lambda info: info.context.get("is_admin", False))
            def email(self) -> str:
                return self._email

            @field
            @authorize_field(
                lambda info: info.context.get("user_id") == self.id,
                error_message="You can only view your own phone number"
            )
            def phone(self) -> str:
                return self._phone
        ```
    """

    def decorator(func: T) -> T:
        """Wrap the field resolver with authorization logic."""
        # Get the actual function to check if it's async
        actual_func = func
        if hasattr(func, "__fraiseql_original_func__"):
            actual_func = func.__fraiseql_original_func__

        is_async = asyncio.iscoroutinefunction(actual_func)

        if is_async:

            @functools.wraps(func)
            async def async_auth_wrapper(root, info, *args: Any, **kwargs: Any) -> Any:
                # Check permission first
                if asyncio.iscoroutinefunction(permission_check):
                    authorized = await permission_check(info, *args, **kwargs)
                else:
                    authorized = permission_check(info, *args, **kwargs)

                if not authorized:
                    field_name = getattr(info, "field_name", "field")
                    raise FieldAuthorizationError(
                        error_message or f"Not authorized to access field '{field_name}'",
                    )

                # Call the original function
                return await func(root, info, *args, **kwargs)

            # Preserve field metadata
            if hasattr(func, "__fraiseql_field__"):
                async_auth_wrapper.__fraiseql_field__ = func.__fraiseql_field__
                async_auth_wrapper.__fraiseql_field_resolver__ = func.__fraiseql_field_resolver__
                async_auth_wrapper.__fraiseql_field_description__ = getattr(
                    func,
                    "__fraiseql_field_description__",
                    None,
                )
                if hasattr(func, "__fraiseql_original_func__"):
                    async_auth_wrapper.__fraiseql_original_func__ = func.__fraiseql_original_func__

            return async_auth_wrapper  # type: ignore[return-value]

        @functools.wraps(func)
        def sync_auth_wrapper(root, info, *args: Any, **kwargs: Any) -> Any:
            # Check permission first
            if asyncio.iscoroutinefunction(permission_check):
                # Warn about using async permission check with sync resolver
                warnings.warn(
                    f"Using async permission check with sync resolver '{func.__name__}'. "
                    "Consider making the resolver async for better performance.",
                    RuntimeWarning,
                    stacklevel=2,
                )

                # Handle async permission check in sync context
                try:
                    loop = asyncio.get_event_loop()
                    if loop.is_running():
                        # We're in an async context, create a task
                        # Store reference to avoid RUF006
                        _ = asyncio.ensure_future(permission_check(info, *args, **kwargs))  # noqa: RUF006
                        # This is not ideal but necessary for sync resolvers
                        authorized = asyncio.run_coroutine_threadsafe(
                            permission_check(info, *args, **kwargs),
                            loop,
                        ).result()
                    else:
                        # No running loop, use run_until_complete
                        authorized = loop.run_until_complete(
                            permission_check(info, *args, **kwargs),
                        )
                except RuntimeError:
                    # No event loop, create a new one
                    loop = asyncio.new_event_loop()
                    try:
                        authorized = loop.run_until_complete(
                            permission_check(info, *args, **kwargs),
                        )
                    finally:
                        loop.close()
            else:
                authorized = permission_check(info, *args, **kwargs)

            if not authorized:
                field_name = getattr(info, "field_name", "field")
                raise FieldAuthorizationError(
                    error_message or f"Not authorized to access field '{field_name}'",
                )

            # Call the original function
            return func(root, info, *args, **kwargs)

        # Preserve field metadata
        if hasattr(func, "__fraiseql_field__"):
            sync_auth_wrapper.__fraiseql_field__ = func.__fraiseql_field__
            sync_auth_wrapper.__fraiseql_field_resolver__ = func.__fraiseql_field_resolver__
            sync_auth_wrapper.__fraiseql_field_description__ = getattr(
                func,
                "__fraiseql_field_description__",
                None,
            )
            if hasattr(func, "__fraiseql_original_func__"):
                sync_auth_wrapper.__fraiseql_original_func__ = func.__fraiseql_original_func__

        return sync_auth_wrapper  # type: ignore[return-value]

    return decorator


def combine_permissions(*checks: PermissionCheck) -> PermissionCheck:
    """Combine multiple permission checks with AND logic.

    All permission checks must pass for access to be granted.

    Args:
        *checks: Variable number of permission check functions.

    Returns:
        A combined permission check function.

    Example:
        ```python
        is_authenticated = lambda info: info.context.get("user") is not None
        is_admin = lambda info: info.context.get("is_admin", False)

        @field
        @authorize_field(combine_permissions(is_authenticated, is_admin))
        def sensitive_data(self) -> str:
            return "secret"
        ```
    """

    async def async_combined_check(info: GraphQLResolveInfo, *args: Any, **kwargs: Any) -> bool:
        for check in checks:
            if asyncio.iscoroutinefunction(check):
                result = await check(info, *args, **kwargs)
            else:
                result = check(info, *args, **kwargs)

            if not result:
                return False
        return True

    def sync_combined_check(info: GraphQLResolveInfo, *args: Any, **kwargs: Any) -> bool:
        for check in checks:
            if asyncio.iscoroutinefunction(check):
                # Handle async checks in sync context
                loop = asyncio.new_event_loop()
                try:
                    result = loop.run_until_complete(check(info, *args, **kwargs))
                finally:
                    loop.close()
            else:
                result = check(info, *args, **kwargs)

            if not result:
                return False
        return True

    # Return async version if any check is async
    if any(asyncio.iscoroutinefunction(check) for check in checks):
        return async_combined_check
    return sync_combined_check


def any_permission(*checks: PermissionCheck) -> PermissionCheck:
    """Combine multiple permission checks with OR logic.

    At least one permission check must pass for access to be granted.

    Args:
        *checks: Variable number of permission check functions.

    Returns:
        A combined permission check function.

    Example:
        ```python
        is_admin = lambda info: info.context.get("is_admin", False)
        is_owner = lambda info: info.context.get("user_id") == self.id

        @field
        @authorize_field(any_permission(is_admin, is_owner))
        def email(self) -> str:
            return self._email
        ```
    """

    async def async_any_check(info: GraphQLResolveInfo, *args: Any, **kwargs: Any) -> bool:
        for check in checks:
            if asyncio.iscoroutinefunction(check):
                result = await check(info, *args, **kwargs)
            else:
                result = check(info, *args, **kwargs)

            if result:
                return True
        return False

    def sync_any_check(info: GraphQLResolveInfo, *args: Any, **kwargs: Any) -> bool:
        for check in checks:
            if asyncio.iscoroutinefunction(check):
                # Handle async checks in sync context
                loop = asyncio.new_event_loop()
                try:
                    result = loop.run_until_complete(check(info, *args, **kwargs))
                finally:
                    loop.close()
            else:
                result = check(info, *args, **kwargs)

            if result:
                return True
        return False

    # Return async version if any check is async
    if any(asyncio.iscoroutinefunction(check) for check in checks):
        return async_any_check
    return sync_any_check
