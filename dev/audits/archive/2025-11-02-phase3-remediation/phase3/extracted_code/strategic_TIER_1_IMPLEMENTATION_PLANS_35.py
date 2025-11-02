# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 35
@strawberry.directive(locations=[strawberry.directive_location.FIELD_DEFINITION])
def requires_permission(resource: str, action: str, check_constraints: bool = True):
    """Permission directive with constraint evaluation."""

    def directive_resolver(resolver):
        async def wrapper(*args, **kwargs):
            info: Info = args[1]
            context = info.context

            resolver_instance = PermissionResolver(context["repo"])
            permissions = await resolver_instance.get_user_permissions(
                user_id=context["user_id"], tenant_id=context.get("tenant_id")
            )

            # Find matching permission
            matching_permission = None
            for p in permissions:
                if p.resource == resource and p.action == action:
                    matching_permission = p
                    break

            if not matching_permission:
                raise PermissionError(f"Permission denied: requires {resource}.{action}")

            # Evaluate constraints if present
            if check_constraints and matching_permission.constraints:
                constraints_met = await _evaluate_constraints(
                    matching_permission.constraints, context, kwargs
                )
                if not constraints_met:
                    raise PermissionError(
                        f"Permission constraints not satisfied for {resource}.{action}"
                    )

            return await resolver(*args, **kwargs)

        return wrapper

    return directive_resolver


async def _evaluate_constraints(constraints: dict, context: dict, field_args: dict) -> bool:
    """Evaluate permission constraints.

    Examples:
    - {"own_data_only": true} - can only access own data
    - {"tenant_scoped": true} - must be in same tenant
    - {"max_records": 100} - can't fetch more than 100 records
    """
    if constraints.get("own_data_only"):
        # Check if accessing own data
        target_user_id = field_args.get("user_id") or field_args.get("id")
        if target_user_id != context["user_id"]:
            return False

    if constraints.get("tenant_scoped"):
        # Check tenant match
        target_tenant = field_args.get("tenant_id")
        if target_tenant and target_tenant != context.get("tenant_id"):
            return False

    if "max_records" in constraints:
        # Check record limit
        limit = field_args.get("limit", float("inf"))
        if limit > constraints["max_records"]:
            return False

    return True
