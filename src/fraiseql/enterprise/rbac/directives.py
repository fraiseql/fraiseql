"""GraphQL directives for FraiseQL Enterprise RBAC (Role-Based Access Control).

These directives integrate with the PostgreSQL-cached PermissionResolver
to provide field-level authorization in GraphQL schemas.
"""

from typing import Any, Callable

try:
    import strawberry  # type: ignore[import]
    from strawberry.types import Info  # type: ignore[import]
except ImportError:
    # Strawberry not available - enterprise features disabled
    raise ImportError("strawberry is required for enterprise RBAC features")

from .resolver import PermissionResolver


@strawberry.directive(
    locations=[strawberry.directive_location.FIELD_DEFINITION],
    description="Require specific permission to access field",
)
def requires_permission(resource: str, action: str, check_constraints: bool = True) -> Callable:
    """Directive to enforce permission requirements on fields.

    Args:
        resource: Resource name (e.g., 'user', 'product', 'order')
        action: Action name (e.g., 'create', 'read', 'update', 'delete')
        check_constraints: Whether to evaluate permission constraints

    Usage:
        @strawberry.type
        class Query:
            @strawberry.field
            @requires_permission(resource="user", action="read")
            async def get_user(self, id: UUID) -> UserType:
                # Only users with 'user.read' permission can access this field
                return await get_user_by_id(id)
    """

    def directive_resolver(resolver: Callable[..., Any]) -> Callable:
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            info: Info = args[1]  # GraphQL Info is second arg
            context = info.context

            # Get user from context (required)
            user_id = context.get("user_id")
            if not user_id:
                raise PermissionError("Authentication required")

            # Get tenant from context (optional)
            tenant_id = context.get("tenant_id")

            # Get permission resolver from context or create new
            resolver_instance: PermissionResolver = context.get("permission_resolver")
            if not resolver_instance:
                # Fallback: create resolver (requires repo in context)
                repo = context.get("repo")
                if not repo:
                    raise RuntimeError("PermissionResolver or repo required in context")
                resolver_instance = PermissionResolver(repo)

            # Get user permissions (uses PostgreSQL cache)
            permissions = await resolver_instance.get_user_permissions(
                user_id=user_id, tenant_id=tenant_id
            )

            # Find matching permission
            matching_permission = None
            for p in permissions:
                if p.resource == resource and p.action == action:
                    matching_permission = p
                    break

            if not matching_permission:
                raise PermissionError(f"Permission denied: requires {resource}.{action}")

            # Evaluate constraints if present and requested
            if check_constraints and matching_permission.constraints:
                constraints_met = await _evaluate_constraints(
                    matching_permission.constraints, context, kwargs
                )
                if not constraints_met:
                    raise PermissionError(
                        f"Permission constraints not satisfied for {resource}.{action}"
                    )

            # Execute field resolver
            return await resolver(*args, **kwargs)

        return wrapper

    return directive_resolver


@strawberry.directive(
    locations=[strawberry.directive_location.FIELD_DEFINITION],
    description="Require specific role to access field",
)
def requires_role(role_name: str) -> Callable:
    """Directive to enforce role requirements on fields.

    Args:
        role_name: Required role name (e.g., 'admin', 'manager', 'viewer')

    Usage:
        @strawberry.type
        class Mutation:
            @strawberry.mutation
            @requires_role(role_name="admin")
            async def delete_user(self, id: UUID) -> MutationResult:
                # Only users with 'admin' role can access this field
                return await delete_user_by_id(id)
    """

    def directive_resolver(resolver: Callable[..., Any]) -> Callable:
        async def wrapper(*args: Any, **kwargs: Any) -> Any:
            info: Info = args[1]
            context = info.context

            # Get user from context (required)
            user_id = context.get("user_id")
            if not user_id:
                raise PermissionError("Authentication required")

            # Get tenant from context (optional)
            tenant_id = context.get("tenant_id")

            # Get permission resolver from context or create new
            resolver_instance: PermissionResolver = context.get("permission_resolver")
            if not resolver_instance:
                # Fallback: create resolver (requires repo in context)
                repo = context.get("repo")
                if not repo:
                    raise RuntimeError("PermissionResolver or repo required in context")
                resolver_instance = PermissionResolver(repo)

            # Get user roles (uses PostgreSQL cache)
            roles = await resolver_instance.get_user_roles(user_id=user_id, tenant_id=tenant_id)

            # Check if user has required role
            has_role = any(r.name == role_name for r in roles)

            if not has_role:
                raise PermissionError(f"Access denied: requires role '{role_name}'")

            return await resolver(*args, **kwargs)

        return wrapper

    return directive_resolver


async def _evaluate_constraints(
    constraints: dict[str, Any], context: dict[str, Any], field_args: dict[str, Any]
) -> bool:
    """Evaluate permission constraints against context and field arguments.

    Args:
        constraints: Constraint dictionary from permission
        context: GraphQL context (user_id, tenant_id, etc.)
        field_args: Arguments passed to the field resolver

    Returns:
        True if constraints are satisfied, False otherwise

    Examples of constraints:
        - {"own_data_only": true} - User can only access their own data
        - {"tenant_scoped": true} - Must be in same tenant as target
        - {"max_records": 100} - Can't fetch more than 100 records
        - {"department_only": true} - Must be in same department
    """
    # Constraint: own_data_only - can only access own data
    if constraints.get("own_data_only"):
        target_user_id = field_args.get("user_id") or field_args.get("id")
        if target_user_id and str(target_user_id) != str(context.get("user_id")):
            return False

    # Constraint: tenant_scoped - must be in same tenant
    if constraints.get("tenant_scoped"):
        target_tenant = field_args.get("tenant_id")
        if target_tenant and str(target_tenant) != str(context.get("tenant_id")):
            return False

    # Constraint: max_records - limit number of records
    if "max_records" in constraints:
        limit = field_args.get("limit", field_args.get("first", float("inf")))
        if limit > constraints["max_records"]:
            return False

    # Constraint: department_only - must be in same department
    if constraints.get("department_only"):
        # This would require additional context or database lookup
        # For now, assume department is in context
        target_dept = field_args.get("department_id")
        user_dept = context.get("department_id")
        if target_dept and user_dept and str(target_dept) != str(user_dept):
            return False

    # Constraint: time_restricted - only during certain hours
    if constraints.get("time_restricted"):
        # This would check current time against allowed hours
        # Implementation depends on specific time restriction format
        pass  # Placeholder for time-based constraints

    return True
