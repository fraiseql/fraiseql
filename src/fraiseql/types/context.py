"""Type-safe GraphQL context for FraiseQL resolvers.

This module provides a typed GraphQLContext dataclass that enables IDE autocompletion
and type-safe access to GraphQL execution context, replacing the unsafe pattern of
accessing context dictionary keys.

Example:
    ```python
    from fraiseql.types.context import GraphQLContext
    from graphql import GraphQLResolveInfo

    @fraiseql.query
    async def get_user(info: GraphQLResolveInfo, id: str) -> User:
        context: GraphQLContext = info.context
        user = await context.db.find_one("users", {"id": id})
        return user
    ```
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from fraiseql.auth.base import UserContext
    from fraiseql.cqrs.repository import FraiseQLRepository
    from fraiseql.fastapi.config import FraiseQLConfig
    from fraiseql.utils.dataloader import LoaderRegistry


@dataclass
class GraphQLContext:
    """Type-safe GraphQL execution context for resolvers.

    This dataclass provides IDE autocompletion and type checking for GraphQL context
    access, replacing unsafe dictionary key access with typed attribute access.

    Attributes:
        db: FraiseQL repository instance for database operations.
        user: Current authenticated user context (None if unauthenticated).
        request: FastAPI Request object (if available in HTTP context).
        response: FastAPI Response object (if available in HTTP context).
        loader_registry: DataLoader registry for batch query optimization.
        config: FraiseQL configuration object.
        authenticated: Whether the current user is authenticated.
        _extras: Additional custom context data not defined in standard fields.

    Example:
        ```python
        @fraiseql.query
        async def get_user(info: GraphQLResolveInfo, id: str) -> User:
            context: GraphQLContext = info.context

            # Type-safe access to database
            user = await context.db.find_one("users", {"id": id})

            # Type-safe access to user context
            if context.authenticated:
                current_user_id = context.user.user_id

            return user
        ```

    Note:
        The dataclass is designed to work seamlessly with FastAPI integration,
        where the context is built by fraiseql.fastapi.dependencies.build_graphql_context.
        For backward compatibility, the dataclass can also be initialized from a
        dictionary using the from_dict class method.
    """

    # Required fields
    db: FraiseQLRepository

    # Optional authentication
    user: UserContext | None = None

    # Optional HTTP integration
    request: Any | None = None
    response: Any | None = None

    # Optional loaders and configuration
    loader_registry: LoaderRegistry | None = None
    config: FraiseQLConfig | None = None

    # Authentication status
    authenticated: bool = False

    # Additional custom data
    _extras: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, context_dict: dict[str, Any]) -> GraphQLContext:
        """Create a GraphQLContext from a context dictionary.

        This class method allows creating a typed context from the untyped
        dictionary returned by build_graphql_context, or from custom context
        dictionaries.

        Args:
            context_dict: Dictionary with context data. Should contain at least 'db'.

        Returns:
            Initialized GraphQLContext instance.

        Raises:
            KeyError: If required 'db' key is missing from context_dict.
            TypeError: If 'db' is not a FraiseQLRepository instance.

        Example:
            ```python
            # Create from build_graphql_context output
            context_dict = await build_graphql_context(db, user, trace_context)
            context = GraphQLContext.from_dict(context_dict)

            # Or with custom data
            context = GraphQLContext.from_dict({
                "db": my_repository,
                "user": my_user,
                "custom_field": "custom_value"
            })
            ```
        """
        if "db" not in context_dict:
            msg = "Context dictionary must contain 'db' key"
            raise KeyError(msg)

        db = context_dict.pop("db")
        user = context_dict.pop("user", None)
        request = context_dict.pop("request", None)
        response = context_dict.pop("response", None)
        loader_registry = context_dict.pop("loader_registry", None)
        config = context_dict.pop("config", None)
        authenticated = context_dict.pop("authenticated", False)

        return cls(
            db=db,
            user=user,
            request=request,
            response=response,
            loader_registry=loader_registry,
            config=config,
            authenticated=authenticated,
            _extras=context_dict,  # Remaining keys go to extras
        )

    def to_dict(self) -> dict[str, Any]:
        """Convert context back to dictionary format.

        This method is useful for backward compatibility with code that expects
        a dictionary context, or for passing context to functions that accept
        dictionary contexts.

        Returns:
            Dictionary representation of the context with all fields.

        Example:
            ```python
            context: GraphQLContext = build_context(...)
            context_dict = context.to_dict()
            # context_dict["db"] is now accessible as context_dict["db"]
            ```
        """
        return {
            "db": self.db,
            "user": self.user,
            "request": self.request,
            "response": self.response,
            "loader_registry": self.loader_registry,
            "config": self.config,
            "authenticated": self.authenticated,
            **self._extras,
        }

    def get_extra(self, key: str, default: Any = None) -> Any:
        """Get a custom context field from extras.

        Args:
            key: The key to retrieve from extras.
            default: Default value if key is not found.

        Returns:
            The value associated with the key, or default if not found.

        Example:
            ```python
            context: GraphQLContext = info.context
            custom_value = context.get_extra("request_id", "unknown")
            ```
        """
        return self._extras.get(key, default)

    def set_extra(self, key: str, value: Any) -> None:
        """Set a custom context field in extras.

        Args:
            key: The key to set.
            value: The value to associate with the key.

        Example:
            ```python
            context: GraphQLContext = info.context
            context.set_extra("request_id", uuid4())
            ```
        """
        self._extras[key] = value


def build_context(
    db: FraiseQLRepository,
    *,
    user: UserContext | None = None,
    request: Any | None = None,
    response: Any | None = None,
    loader_registry: LoaderRegistry | None = None,
    config: FraiseQLConfig | None = None,
    authenticated: bool | None = None,
    **extras: Any,
) -> GraphQLContext:
    """Build a type-safe GraphQL context.

    This helper function provides a convenient way to create GraphQLContext instances
    with IDE autocompletion and type checking. It's useful for custom context building
    in non-FastAPI environments or when you need to override default context values.

    Args:
        db: FraiseQL repository instance (required).
        user: Current authenticated user context (default: None).
        request: FastAPI Request object (default: None).
        response: FastAPI Response object (default: None).
        loader_registry: DataLoader registry for batch optimization (default: None).
        config: FraiseQL configuration object (default: None).
        authenticated: Authentication status (default: inferred from user).
        **extras: Additional custom context fields.

    Returns:
        Initialized and type-safe GraphQLContext instance.

    Example:
        ```python
        from fraiseql.types.context import build_context

        # Basic usage
        context = build_context(db=repository)

        # With user and custom fields
        context = build_context(
            db=repository,
            user=current_user,
            request=http_request,
            request_id="req_12345",
            tenant_id="tenant_abc"
        )

        # Access with full type-safety and IDE help
        user_id = context.user.user_id
        custom_value = context.get_extra("request_id")
        ```
    """
    # Infer authentication status if not provided
    if authenticated is None:
        authenticated = user is not None

    return GraphQLContext(
        db=db,
        user=user,
        request=request,
        response=response,
        loader_registry=loader_registry,
        config=config,
        authenticated=authenticated,
        _extras=extras,
    )
