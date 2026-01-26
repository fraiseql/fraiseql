"""Advanced authorization and security decorators for FraiseQL.

This module provides decorators for implementing:
- Custom authorization rules with context variables
- Role-based access control (RBAC)
- Attribute-based access control (ABAC)
- Reusable authorization policies

All decorators are compile-time only - they output JSON schema for compilation.
NO runtime behavior or FFI.
"""

from __future__ import annotations

from dataclasses import dataclass, field as dataclass_field
from enum import Enum
from typing import TYPE_CHECKING, Any, Callable, TypeVar

from fraiseql.registry import SchemaRegistry

if TYPE_CHECKING:
    from collections.abc import Callable as CallableType

T = TypeVar("T")
F = TypeVar("F")


class RoleMatchStrategy(str, Enum):
    """Strategy for matching multiple roles."""

    ANY = "any"  # User must have at least one role
    ALL = "all"  # User must have all roles
    EXACTLY = "exactly"  # User must have exactly these roles


class AuthzPolicyType(str, Enum):
    """Type of authorization policy."""

    RBAC = "rbac"  # Role-based access control
    ABAC = "abac"  # Attribute-based access control
    CUSTOM = "custom"  # Custom rule expressions
    HYBRID = "hybrid"  # Hybrid approach


@dataclass
class AuthorizeConfig:
    """Configuration for custom authorization rules.

    Attributes:
        rule: Custom authorization rule expression with context variables
        policy: Reference to a named authorization policy
        description: Description of what this rule protects
        error_message: Custom error message when authorization fails
        recursive: Whether to apply rule hierarchically to child fields
        operations: Comma-separated list of operations this rule applies to
        cacheable: Whether to cache authorization decisions
        cache_duration_seconds: Cache duration in seconds
    """

    rule: str | None = None
    policy: str | None = None
    description: str | None = None
    error_message: str | None = None
    recursive: bool = False
    operations: str | None = None
    cacheable: bool = True
    cache_duration_seconds: int = 300


@dataclass
class RoleRequiredConfig:
    """Configuration for role-based access control.

    Attributes:
        roles: List of required roles
        strategy: Strategy for matching multiple roles (ANY, ALL, EXACTLY)
        hierarchy: Whether roles form a hierarchy
        description: Description of role requirements
        error_message: Custom error message when role check fails
        operations: Operations this rule applies to
        inherit: Whether to inherit role requirements from parent types
        cacheable: Whether to cache role validation results
        cache_duration_seconds: Cache duration in seconds
    """

    roles: list[str] = dataclass_field(default_factory=list)
    strategy: RoleMatchStrategy = RoleMatchStrategy.ANY
    hierarchy: bool = False
    description: str | None = None
    error_message: str | None = None
    operations: str | None = None
    inherit: bool = True
    cacheable: bool = True
    cache_duration_seconds: int = 600


@dataclass
class AuthzPolicyConfig:
    """Configuration for authorization policies.

    Attributes:
        name: Unique policy name
        description: Description of what this policy protects
        rule: Custom authorization rule expression
        attributes: List of attribute conditions for ABAC
        policy_type: Type of authorization policy
        cacheable: Whether to cache authorization decisions
        cache_duration_seconds: Cache duration in seconds
        recursive: Whether to apply policy recursively to nested types
        operations: Operations this policy applies to
        audit_logging: Whether to log access decisions
        error_message: Error message when policy check fails
    """

    name: str
    description: str | None = None
    rule: str | None = None
    attributes: list[str] = dataclass_field(default_factory=list)
    policy_type: AuthzPolicyType = AuthzPolicyType.CUSTOM
    cacheable: bool = True
    cache_duration_seconds: int = 300
    recursive: bool = False
    operations: str | None = None
    audit_logging: bool = True
    error_message: str | None = None


def authorize(
    func: F | None = None,
    *,
    rule: str | None = None,
    policy: str | None = None,
    description: str | None = None,
    error_message: str | None = None,
    recursive: bool = False,
    operations: str | None = None,
    cacheable: bool = True,
    cache_duration_seconds: int = 300,
) -> F | Callable[[F], F]:
    """Decorator to add custom authorization rules to queries, mutations, or types.

    This decorator registers authorization rules with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        func: Query, mutation, or type to protect
        rule: Custom authorization rule expression with context variables
            Examples: "isOwner($context.userId, $field.ownerId)"
                     "hasRole($context, 'admin') OR isOwner(...)"
        policy: Reference to a named authorization policy
        description: Description of what this rule protects
        error_message: Custom error message when authorization fails
        recursive: Whether to apply rule to nested types
        operations: Operations this rule applies to (read, create, update, delete)
        cacheable: Whether to cache authorization decisions
        cache_duration_seconds: Cache duration in seconds

    Returns:
        The original function/class (unmodified)

    Examples:
        >>> @fraiseql.type
        ... @fraiseql.authorize(rule="isOwner($context.userId, $field.ownerId)")
        ... class ProtectedNote:
        ...     '''User can only access their own notes.'''
        ...     id: str
        ...     content: str

        >>> @fraiseql.query
        ... @fraiseql.authorize(rule="hasRole($context, 'admin')")
        ... def admin_panel() -> AdminData:
        ...     pass

    Notes:
        - Rule expressions support context variables: $context.userId, $context.roles
        - Recursive rules apply to all nested fields
        - Use policy parameter to reference named @authz_policy decorators
    """

    def decorator(target: F) -> F:
        config = AuthorizeConfig(
            rule=rule,
            policy=policy,
            description=description,
            error_message=error_message,
            recursive=recursive,
            operations=operations,
            cacheable=cacheable,
            cache_duration_seconds=cache_duration_seconds,
        )

        # Store authorization config on the object
        # (Will be picked up during schema export)
        if hasattr(target, "__fraiseql_auth__"):
            # Merge with existing auth config
            existing = target.__fraiseql_auth__
            config = AuthorizeConfig(
                rule=rule or existing.rule,
                policy=policy or existing.policy,
                description=description or existing.description,
                error_message=error_message or existing.error_message,
                recursive=recursive or existing.recursive,
                operations=operations or existing.operations,
                cacheable=cacheable,
                cache_duration_seconds=cache_duration_seconds,
            )

        target.__fraiseql_auth__ = config
        return target

    # Support both @authorize and @authorize(...)
    if func is None:
        # Called with arguments: @authorize(rule="...")
        return decorator
    # Called without arguments: @authorize
    return decorator(func)


def role_required(
    func: F | None = None,
    *,
    roles: str | list[str] | None = None,
    strategy: RoleMatchStrategy | str = RoleMatchStrategy.ANY,
    hierarchy: bool = False,
    description: str | None = None,
    error_message: str | None = None,
    operations: str | None = None,
    inherit: bool = True,
    cacheable: bool = True,
    cache_duration_seconds: int = 600,
) -> F | Callable[[F], F]:
    """Decorator to add role-based access control to types or queries.

    This decorator registers role requirements with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        func: Type, query, or mutation to protect
        roles: Single role string or list of required roles
        strategy: How to match multiple roles (ANY, ALL, EXACTLY)
        hierarchy: Whether roles form a hierarchy
        description: Description of role requirements
        error_message: Custom error message when role check fails
        operations: Operations this rule applies to
        inherit: Whether to inherit role requirements from parent types
        cacheable: Whether to cache role validation
        cache_duration_seconds: Cache duration in seconds

    Returns:
        The original function/class (unmodified)

    Examples:
        >>> @fraiseql.type
        ... @fraiseql.role_required(roles="admin")
        ... class SystemSettings:
        ...     '''Only admins can access system settings.'''
        ...     database_url: str

        >>> @fraiseql.query
        ... @fraiseql.role_required(
        ...     roles=["manager", "director"],
        ...     strategy=fraiseql.RoleMatchStrategy.ANY
        ... )
        ... def salary_report() -> list[SalaryData]:
        ...     pass

    Notes:
        - ANY: User must have at least one role (default)
        - ALL: User must have all roles
        - EXACTLY: User must have exactly these roles
        - Hierarchies allow higher roles to inherit lower role permissions
    """

    # Normalize roles to list
    role_list: list[str] = []
    if isinstance(roles, str):
        role_list = [roles]
    elif isinstance(roles, list):
        role_list = roles
    elif roles is None:
        role_list = []

    # Normalize strategy
    if isinstance(strategy, str):
        strategy = RoleMatchStrategy(strategy)

    def decorator(target: F) -> F:
        config = RoleRequiredConfig(
            roles=role_list,
            strategy=strategy,
            hierarchy=hierarchy,
            description=description,
            error_message=error_message,
            operations=operations,
            inherit=inherit,
            cacheable=cacheable,
            cache_duration_seconds=cache_duration_seconds,
        )

        # Store role config on the object
        target.__fraiseql_roles__ = config
        return target

    # Support both @role_required and @role_required(...)
    if func is None:
        # Called with arguments: @role_required(roles="admin")
        return decorator
    # Called without arguments: @role_required
    return decorator(func)


def authz_policy(
    name: str,
    *,
    description: str | None = None,
    rule: str | None = None,
    attributes: list[str] | None = None,
    policy_type: AuthzPolicyType | str = AuthzPolicyType.CUSTOM,
    cacheable: bool = True,
    cache_duration_seconds: int = 300,
    recursive: bool = False,
    operations: str | None = None,
    audit_logging: bool = True,
    error_message: str | None = None,
) -> Callable[[T], T]:
    """Decorator to define a reusable authorization policy.

    Policies can be referenced by @authorize(policy="name") decorators.
    This enables centralized authorization logic.

    This decorator registers the policy with the schema registry for JSON export.
    NO runtime behavior - only used for schema compilation.

    Args:
        name: Unique policy name (required)
        description: Description of what this policy protects
        rule: Custom authorization rule expression
        attributes: List of attribute conditions for ABAC policies
        policy_type: Type of policy (RBAC, ABAC, CUSTOM, HYBRID)
        cacheable: Whether to cache authorization decisions
        cache_duration_seconds: Cache duration in seconds
        recursive: Whether to apply recursively to nested types
        operations: Operations this policy applies to
        audit_logging: Whether to log access decisions
        error_message: Custom error message when policy check fails

    Returns:
        Decorator function

    Examples:
        >>> @fraiseql.authz_policy(
        ...     name="piiAccess",
        ...     description="Access to Personally Identifiable Information",
        ...     rule="hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')"
        ... )
        ... class PIIAccessPolicy:
        ...     pass

        >>> @fraiseql.type
        ... class Customer:
        ...     id: str
        ...     name: str
        ...     # References the policy defined above
        ...     @fraiseql.authorize(policy="piiAccess")
        ...     email: str

        >>> @fraiseql.authz_policy(
        ...     name="financialData",
        ...     policy_type=fraiseql.AuthzPolicyType.ABAC,
        ...     attributes=[
        ...         "clearance_level >= 2",
        ...         "department == 'finance'"
        ...     ]
        ... )
        ... class FinancialDataPolicy:
        ...     pass

    Notes:
        - Policy names must be unique across the schema
        - Policies are centralized for consistency and maintenance
        - RBAC: Role-based (checks user roles)
        - ABAC: Attribute-based (checks user attributes)
        - CUSTOM: Custom rule expressions
        - HYBRID: Combines RBAC and ABAC
    """

    # Normalize policy_type
    if isinstance(policy_type, str):
        policy_type = AuthzPolicyType(policy_type)

    def decorator(cls: T) -> T:
        config = AuthzPolicyConfig(
            name=name,
            description=description or cls.__doc__,
            rule=rule,
            attributes=attributes or [],
            policy_type=policy_type,
            cacheable=cacheable,
            cache_duration_seconds=cache_duration_seconds,
            recursive=recursive,
            operations=operations,
            audit_logging=audit_logging,
            error_message=error_message,
        )

        # Register policy with schema registry
        # (Implementation depends on registry structure)
        SchemaRegistry.register_authz_policy(
            name=name,
            description=config.description,
            rule=config.rule,
            attributes=config.attributes,
            policy_type=config.policy_type.value,
            cacheable=config.cacheable,
            cache_duration_seconds=config.cache_duration_seconds,
            recursive=config.recursive,
            operations=config.operations,
            audit_logging=config.audit_logging,
            error_message=config.error_message,
        )

        # Store policy config on class
        cls.__fraiseql_policy__ = config
        return cls

    return decorator
