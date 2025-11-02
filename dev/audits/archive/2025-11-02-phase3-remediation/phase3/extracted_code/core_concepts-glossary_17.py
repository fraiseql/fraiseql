# Extracted from: docs/core/concepts-glossary.md
# Block number: 17
import fraiseql


@fraiseql.field
@fraiseql.authorized(roles=["admin"])
def sensitive_field(user: User, info) -> str:
    """Only admins can access this field."""
    return user.sensitive_data
