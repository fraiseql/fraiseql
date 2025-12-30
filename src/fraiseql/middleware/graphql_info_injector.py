"""GraphQL info auto-injection middleware for field selection.

This middleware automatically injects GraphQL info parameter into the context,
enabling Rust zero-copy field selection by default without explicit passing.

Features:
- Auto-injects info into context for db.find() access
- Maintains full backwards compatibility
- Allows explicit opt-out with info=None
- Follows industry standards (Strawberry, Apollo Server)
"""

import inspect
from functools import wraps
from typing import Any, Callable


class GraphQLInfoInjector:
    """Auto-injects GraphQL info into context for field selection."""

    @staticmethod
    def auto_inject(func: Callable) -> Callable:
        """Decorator to auto-inject info parameter into context.

        This decorator:
        1. Extracts the 'info' parameter from resolver arguments
        2. Stores it in info.context['graphql_info'] for db.find() access
        3. Allows explicit info=None to opt-out of field selection
        4. Maintains full backwards compatibility with explicit info=info pattern

        Args:
            func: The resolver function to decorate

        Returns:
            Decorated function that auto-injects info

        Example:
            @GraphQLInfoInjector.auto_inject
            async def users(info: GraphQLResolveInfo, limit: int = 100):
                db = info.context["db"]
                return await db.find("users", limit=limit)  # info auto-extracted
        """

        @wraps(func)
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            # Get function signature to extract info parameter
            sig = inspect.signature(func)

            # Extract info from function parameters if available
            if "info" in sig.parameters:
                # Bind the arguments to the signature
                bound = sig.bind(*args, **kwargs)
                bound.apply_defaults()
                info = bound.arguments.get("info")

                # Auto-inject into context if info was provided
                if info and hasattr(info, "context") and isinstance(info.context, dict):
                    # Store in context for db.find() to auto-extract
                    info.context["graphql_info"] = info

            # Call the original resolver
            return await func(*args, **kwargs)

        return wrapper

    def process_info(self, info):
        """Process GraphQL info object.

        Args:
            info: GraphQL info object

        Returns:
            Processed info object
        """
        return info

    def inject(self, func: Callable) -> Callable:
        """Decorator to inject info parameter into resolvers.

        Args:
            func: The resolver function to decorate

        Returns:
            Decorated function that injects info
        """

        @wraps(func)
        def sync_wrapper(*args, **kwargs) -> Any:
            # Get function signature to extract info parameter
            sig = inspect.signature(func)

            # Extract info from function parameters if available
            if "info" in sig.parameters:
                # Bind the arguments to the signature
                bound = sig.bind(*args, **kwargs)
                bound.apply_defaults()
                info = bound.arguments.get("info")

                # Process the info object
                if info:
                    info = self.process_info(info)

            # Call the original resolver
            return func(*args, **kwargs)

        return sync_wrapper
