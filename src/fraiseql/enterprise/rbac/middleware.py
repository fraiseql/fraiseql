"""GraphQL middleware for FraiseQL Enterprise RBAC (Role-Based Access Control).

This middleware integrates the PostgreSQL-cached PermissionResolver with GraphQL
execution, providing context-aware permission checking, row-level filtering, and
automatic cache management.
"""

import logging
from typing import Any, Awaitable, Callable, Optional
from uuid import UUID

from .cache import PermissionCache
from .resolver import PermissionResolver
from .rust_row_constraints import RustRowConstraintResolver

logger = logging.getLogger(__name__)


class RbacMiddleware:
    """GraphQL middleware for RBAC authorization.

    This middleware:
    1. Extracts user/tenant context from GraphQL request
    2. Provides PermissionResolver instance in context
    3. Resolves row-level access constraints (when configured)
    4. Injects row-level filters into GraphQL context
    5. Manages request-level cache lifecycle
    6. Logs authorization events

    Usage:
        from fraiseql.enterprise.rbac.middleware import RbacMiddleware

        schema = strawberry.Schema(
            query=Query,
            mutation=Mutation,
            extensions=[RbacMiddleware()]
        )
    """

    def __init__(
        self,
        permission_resolver: Optional[PermissionResolver] = None,
        row_constraint_resolver: Optional[RustRowConstraintResolver] = None,
    ) -> None:
        """Initialize RBAC middleware.

        Args:
            permission_resolver: Optional pre-configured permission resolver.
                                 If None, will be created from context.
            row_constraint_resolver: Optional pre-configured row constraint resolver.
                                     If None, row-level filtering is disabled.
        """
        self.permission_resolver = permission_resolver
        self.row_constraint_resolver = row_constraint_resolver

    def resolve(
        self, next_: Callable[..., Awaitable[Any]], root: Any, info: Any, **kwargs: Any
    ) -> Awaitable[Any]:
        """Strawberry middleware resolver method."""
        return self._middleware(next_, root, info, **kwargs)

    async def _middleware(
        self, next_: Callable[..., Awaitable[Any]], root: Any, info: Any, **kwargs: Any
    ) -> Any:
        """Execute middleware logic.

        This method is called for every field resolution in the GraphQL query.
        We use it to set up the authorization context.
        """
        # Only run on root level (to avoid running on every field)
        if root is not None:
            return await next_(root, info, **kwargs)

        # Extract context from GraphQL info
        context = self._extract_context(info)

        # Add permission resolver to context if not already present
        if "permission_resolver" not in context:
            resolver = self._get_permission_resolver(context)
            if resolver:
                context["permission_resolver"] = resolver

        # Add row-level filters to context if resolver configured
        if "row_filters" not in context and self.row_constraint_resolver:
            row_filters = await self._get_row_filters(context, info)
            if row_filters:
                context["row_filters"] = row_filters

        # Execute the query/mutation
        try:
            result = await next_(root, info, **kwargs)

            # Clear request-level cache after request completion
            self._clear_request_cache(context)

            return result

        except Exception:
            # Clear cache even on errors
            self._clear_request_cache(context)
            raise

    def _extract_context(self, info: Any) -> dict[str, Any]:
        """Extract authorization context from GraphQL info.

        Args:
            info: GraphQL execution info object

        Returns:
            Context dictionary with user_id, tenant_id, etc.
        """
        context = getattr(info, "context", {}) or {}

        # Extract user information (customize based on your auth system)
        user_id = self._extract_user_id(context)
        tenant_id = self._extract_tenant_id(context)

        # Add to context if found
        if user_id:
            context["user_id"] = user_id
        if tenant_id:
            context["tenant_id"] = tenant_id

        return context

    def _extract_user_id(self, context: dict[str, Any]) -> Optional[UUID]:
        """Extract user ID from GraphQL context.

        Customize this method based on your authentication system.
        Common patterns:
        - JWT token in headers
        - Session-based auth
        - API key authentication

        Args:
            context: GraphQL context dictionary

        Returns:
            User ID if found, None otherwise
        """
        # Example: Extract from JWT token
        auth_header = context.get("request", {}).get("headers", {}).get("authorization")
        if auth_header and auth_header.startswith("Bearer "):
            # Parse JWT token to extract user_id
            # This is a placeholder - implement based on your auth system
            token = auth_header[7:]  # Remove 'Bearer ' prefix
            # user_id = decode_jwt_token(token).get('user_id')
            # return UUID(user_id) if user_id else None

        # Example: Extract from session
        session = context.get("session")
        if session:
            user_id = session.get("user_id")
            return UUID(user_id) if user_id else None

        # Example: Extract from context directly (for testing)
        user_id = context.get("user_id")
        return UUID(user_id) if user_id else None

    def _extract_tenant_id(self, context: dict[str, Any]) -> Optional[UUID]:
        """Extract tenant ID from GraphQL context.

        Args:
            context: GraphQL context dictionary

        Returns:
            Tenant ID if found, None otherwise
        """
        # Example: Extract from JWT token
        auth_header = context.get("request", {}).get("headers", {}).get("authorization")
        if auth_header and auth_header.startswith("Bearer "):
            # Parse JWT token to extract tenant_id
            # token = auth_header[7:]
            # tenant_id = decode_jwt_token(token).get('tenant_id')
            # return UUID(tenant_id) if tenant_id else None
            pass

        # Example: Extract from session
        session = context.get("session")
        if session:
            tenant_id = session.get("tenant_id")
            return UUID(tenant_id) if tenant_id else None

        # Example: Extract from headers
        tenant_header = context.get("request", {}).get("headers", {}).get("x-tenant-id")
        if tenant_header:
            try:
                return UUID(tenant_header)
            except ValueError:
                pass

        # Example: Extract from context directly (for testing)
        tenant_id = context.get("tenant_id")
        return UUID(tenant_id) if tenant_id else None

    def _get_permission_resolver(self, context: dict[str, Any]) -> Optional[PermissionResolver]:
        """Get or create PermissionResolver for the request.

        Args:
            context: GraphQL context dictionary

        Returns:
            PermissionResolver instance or None if cannot create
        """
        # Use pre-configured resolver if available
        if self.permission_resolver:
            return self.permission_resolver

        # Create from context
        repo = context.get("repo")
        if not repo:
            logger.warning("No repository in context, cannot create PermissionResolver")
            return None

        # Create resolver with cache
        try:
            resolver = PermissionResolver(repo)
            logger.debug("Created PermissionResolver for request")
            return resolver
        except Exception as e:
            logger.error(f"Failed to create PermissionResolver: {e}")
            return None

    async def _get_row_filters(
        self, context: dict[str, Any], info: Any
    ) -> Optional[dict[str, Any]]:
        """Resolve row-level filters for a request.

        This method queries the row constraint resolver to get any row-level
        access restrictions for the user on the queried table.

        Args:
            context: GraphQL context dictionary
            info: GraphQL execution info

        Returns:
            WHERE clause fragment for row filtering, or None if no constraints
        """
        resolver = self.row_constraint_resolver
        if not resolver:
            return None

        # Extract required context
        user_id = context.get("user_id")
        tenant_id = context.get("tenant_id")

        if not user_id:
            return None

        try:
            # Extract table name from GraphQL field name (best effort)
            table_name = self._extract_table_name(info)
            if not table_name:
                # If we can't determine table name, skip row filtering
                return None

            # Get row filters from Rust resolver
            row_filter = await resolver.get_row_filters(
                user_id,
                table_name,
                context.get("user_roles", []),
                tenant_id,
            )

            if not row_filter:
                return None

            # Convert RowFilter to WHERE clause fragment
            return {row_filter.field: {row_filter.operator: row_filter.value}}

        except Exception as e:
            logger.warning(f"Failed to resolve row constraints: {e}")
            return None

    def _extract_table_name(self, info: Any) -> Optional[str]:
        """Extract table name from GraphQL query field name.

        This is a heuristic approach to map GraphQL field names to table names.
        Can be customized based on your schema naming conventions.

        Args:
            info: GraphQL execution info

        Returns:
            Table name if determinable, None otherwise
        """
        # Get field name from GraphQL info
        field_name = getattr(info, "field_name", None)
        if not field_name:
            return None

        # For now, assume field name matches table name
        # In a real implementation, you might have a mapping dictionary:
        # FIELD_TO_TABLE = {"documents": "documents", "users": "users", ...}
        return field_name

    def _clear_request_cache(self, context: dict[str, Any]) -> None:
        """Clear request-level permission cache.

        This should be called at the end of each GraphQL request
        to prevent memory leaks and ensure fresh permissions
        for the next request.

        Args:
            context: GraphQL context dictionary
        """
        resolver = context.get("permission_resolver")
        if resolver and hasattr(resolver, "cache"):
            cache: PermissionCache = resolver.cache
            if hasattr(cache, "clear_request_cache"):
                cache.clear_request_cache()
                logger.debug("Cleared request-level permission cache")


# Convenience function for easy middleware setup
def create_rbac_middleware(
    permission_resolver: Optional[PermissionResolver] = None,
    row_constraint_resolver: Optional[RustRowConstraintResolver] = None,
) -> RbacMiddleware:
    """Create RBAC middleware instance.

    Args:
        permission_resolver: Optional pre-configured permission resolver
        row_constraint_resolver: Optional pre-configured row constraint resolver
                                 for row-level access filtering

    Returns:
        Configured RbacMiddleware instance

    Usage:
        from fraiseql.enterprise.rbac.middleware import create_rbac_middleware

        # Basic RBAC (field-level only)
        schema = strawberry.Schema(
            query=Query,
            mutation=Mutation,
            extensions=[create_rbac_middleware()]
        )

        # With row-level filtering
        from fraiseql.enterprise.rbac.rust_row_constraints import RustRowConstraintResolver

        row_resolver = RustRowConstraintResolver(pool)
        schema = strawberry.Schema(
            query=Query,
            mutation=Mutation,
            extensions=[create_rbac_middleware(row_constraint_resolver=row_resolver)]
        )
    """
    return RbacMiddleware(permission_resolver, row_constraint_resolver)
