"""Scope validation and formatting for field-level RBAC.

This module provides utilities for validating and formatting scope strings
used in field-level role-based access control (RBAC).

## Scope Naming Convention

Scopes follow a hierarchical naming pattern inspired by OAuth 2.0:

    {action}:{resource}

Where:
- `action`: The operation type (e.g., "read", "write", "admin", "custom_action")
- `resource`: The target resource, supporting multiple formats:
  - `Type.field`: Specific field of a GraphQL type (e.g., "User.email")
  - `Type.*`: All fields of a GraphQL type (e.g., "Post.*")
  - `*`: All resources (e.g., "read:*", "admin:*")
  - Custom: Any custom resource identifier (e.g., "hr:view_pii", "finance:audit")

## Examples

Valid scopes:
- `read:User.email` - Read access to User.email field
- `write:Post.title` - Write access to Post.title field
- `read:User.*` - Read access to all User fields
- `admin:*` - Admin access to all resources
- `hr:view_pii` - Custom HR PII viewing scope
- `finance:audit_log` - Custom finance audit scope

Invalid scopes:
- `readUser.email` - Missing colon separator
- `read:` - Empty resource
- `:User.email` - Empty action
- `read:User.` - Trailing dot (incomplete field name)

## Wildcard Matching

The runtime uses these patterns for scope matching:
- `read:Type.field` matches exactly that field
- `read:Type.*` matches any field in that type
- `read:*` matches any resource with that action
- `admin:*` matches any admin resource

## Scope Composition

Multiple scopes are typically combined in JWT tokens:
```python
scopes = ["read:User.email", "read:User.name", "write:Post.title"]
```

The runtime checks if any user scope matches a field's required scope.
"""

import re


class ScopeValidationError(ValueError):
    """Exception raised when scope validation fails.

    Raised when a scope string doesn't match the expected format or
    contains invalid characters.
    """

    pass


def validate_scope(scope: str | None) -> None:
    """Validate a scope string format.

    Args:
        scope: Scope string to validate (e.g., "read:User.email").
            None or empty string means field is public (no scope required).

    Raises:
        ScopeValidationError: If scope format is invalid

    Examples:
        >>> validate_scope("read:User.email")  # OK
        >>> validate_scope("write:Post.*")  # OK
        >>> validate_scope("admin:*")  # OK
        >>> validate_scope(None)  # OK - public field
        >>> validate_scope("")  # OK - public field
        >>> validate_scope("invalid")  # Raises ScopeValidationError
    """
    # None or empty string is acceptable (field is public)
    if scope is None or not scope or not scope.strip():
        return

    # Must contain exactly one colon separator
    if ":" not in scope:
        raise ScopeValidationError(
            f"Scope '{scope}' is missing ':' separator. "
            "Expected format: action:resource (e.g., 'read:User.email')"
        )

    parts = scope.split(":")
    if len(parts) != 2:
        raise ScopeValidationError(
            f"Scope '{scope}' has too many colons. "
            "Expected format: action:resource with single ':' separator"
        )

    action, resource = parts

    # Validate action part
    if not action:
        raise ScopeValidationError(
            f"Scope '{scope}' has empty action. "
            "Expected format: action:resource (e.g., 'read:User.email')"
        )

    if not _is_valid_identifier(action):
        raise ScopeValidationError(
            f"Scope action '{action}' contains invalid characters. "
            "Actions must use letters, numbers, underscores (e.g., 'read_admin')"
        )

    # Validate resource part
    if not resource:
        raise ScopeValidationError(
            f"Scope '{scope}' has empty resource. "
            "Expected format: action:resource (e.g., 'read:User.email')"
        )

    # Resource can be:
    # - A wildcard: "*"
    # - Type with fields: "User.email" or "User.*"
    # - Custom identifier: "view_pii" or "audit_log"
    if resource != "*" and not _is_valid_resource(resource):
        raise ScopeValidationError(
            f"Scope resource '{resource}' contains invalid characters or format. "
            "Resources must be: '*', 'TypeName.field', or 'TypeName.*' "
            "or a custom identifier using letters, numbers, underscores, dots (e.g., 'hr_view_pii')"
        )


def _is_valid_identifier(identifier: str) -> bool:
    """Check if identifier contains only valid characters.

    Valid characters: letters, numbers, underscores.

    Args:
        identifier: String to validate

    Returns:
        True if identifier is valid, False otherwise
    """
    return bool(re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", identifier))


def _is_valid_resource(resource: str) -> bool:
    """Check if resource format is valid.

    Valid formats:
    - Type.field (e.g., "User.email")
    - Type.* (e.g., "User.*")
    - Custom identifiers with underscores (e.g., "view_pii", "audit_log")

    Args:
        resource: Resource string to validate

    Returns:
        True if resource format is valid, False otherwise
    """
    # Wildcard type reference: "Type.*"
    if resource.endswith(".*"):
        type_name = resource[:-2]
        return bool(re.match(r"^[A-Z][a-zA-Z0-9_]*$", type_name))

    # Specific field reference: "Type.field"
    if "." in resource:
        parts = resource.split(".")
        if len(parts) != 2:
            return False
        type_name, field_name = parts
        return bool(re.match(r"^[A-Z][a-zA-Z0-9_]*$", type_name)) and bool(
            re.match(r"^[a-z_][a-zA-Z0-9_]*$", field_name)
        )

    # Custom identifier: "view_pii", "audit_log"
    return bool(re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", resource))


def describe_scope_format() -> str:
    """Return a human-readable description of valid scope formats.

    Returns:
        String describing valid scope formats with examples
    """
    return """Valid scope formats:
  action:Type.field    - Specific field (e.g., "read:User.email")
  action:Type.*        - All fields of type (e.g., "read:User.*")
  action:*             - All resources (e.g., "admin:*")
  action:custom_name   - Custom identifier (e.g., "hr:view_pii")

Actions: letters, numbers, underscores (e.g., "read", "write", "hr_admin")
Resources: same as actions, or Type.field format, or * wildcard"""
