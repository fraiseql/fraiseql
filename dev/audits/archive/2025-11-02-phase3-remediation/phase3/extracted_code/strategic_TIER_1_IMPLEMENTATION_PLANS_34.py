# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 34
# src/fraiseql/enterprise/rbac/directives.py


import strawberry
from strawberry.types import Info

from fraiseql.enterprise.rbac.resolver import PermissionResolver


@strawberry.directive(
    locations=[strawberry.directive_location.FIELD_DEFINITION],
    description="Require specific permission to access field",
)
def requires_permission(resource: str, action: str):
    """Directive to enforce permission requirements on fields."""

    def directive_resolver(resolver):
        async def wrapper(*args, **kwargs):
            info: Info = args[1]  # GraphQL Info is second arg
            context = info.context

            # Get user permissions
            resolver_instance = PermissionResolver(context["repo"])
            permissions = await resolver_instance.get_user_permissions(
                user_id=context["user_id"], tenant_id=context.get("tenant_id")
            )

            # Check if user has required permission
            has_permission = any(p.resource == resource and p.action == action for p in permissions)

            if not has_permission:
                raise PermissionError(f"Permission denied: requires {resource}.{action}")

            # Execute field resolver
            return await resolver(*args, **kwargs)

        return wrapper

    return directive_resolver


@strawberry.directive(
    locations=[strawberry.directive_location.FIELD_DEFINITION],
    description="Require specific role to access field",
)
def requires_role(role_name: str):
    """Directive to enforce role requirements on fields."""

    def directive_resolver(resolver):
        async def wrapper(*args, **kwargs):
            info: Info = args[1]
            context = info.context

            # Get user roles
            resolver_instance = PermissionResolver(context["repo"])
            roles = await resolver_instance.get_user_roles(
                user_id=context["user_id"], tenant_id=context.get("tenant_id")
            )

            # Check if user has required role
            has_role = any(r.name == role_name for r in roles)

            if not has_role:
                raise PermissionError(f"Access denied: requires role '{role_name}'")

            return await resolver(*args, **kwargs)

        return wrapper

    return directive_resolver
