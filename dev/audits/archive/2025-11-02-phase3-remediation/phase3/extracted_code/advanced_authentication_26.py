# Extracted from: docs/advanced/authentication.md
# Block number: 26
# Resource-based
"""orders:read"""  # Read orders

"orders:write"  # Create/update orders
"orders:delete"  # Delete orders
"orders:*"  # All order permissions

# Scope-based
"users:read:self"  # Read own user
"users:read:team"  # Read team users
"users:read:all"  # Read all users

# Admin override
"admin:all"  # All permissions
