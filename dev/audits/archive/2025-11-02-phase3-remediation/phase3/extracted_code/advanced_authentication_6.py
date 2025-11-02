# Extracted from: docs/advanced/authentication.md
# Block number: 6
# Fetch full user profile
user_profile = await auth_provider.get_user_profile(
    user_id="auth0|507f1f77bcf86cd799439011", access_token=management_api_token
)
# Returns: {"user_id": "...", "email": "...", "name": "...", ...}

# Fetch user roles
roles = await auth_provider.get_user_roles(
    user_id="auth0|507f1f77bcf86cd799439011", access_token=management_api_token
)
# Returns: [{"id": "rol_...", "name": "admin", "description": "..."}]

# Fetch user permissions
permissions = await auth_provider.get_user_permissions(
    user_id="auth0|507f1f77bcf86cd799439011", access_token=management_api_token
)
# Returns: [{"permission_name": "users:write", "resource_server_identifier": "..."}]
