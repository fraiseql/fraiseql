# Extracted from: docs/advanced/authentication.md
# Block number: 19
from fraiseql.auth import Auth0ProviderWithRevocation

# Auth0 with revocation support
auth_provider = Auth0ProviderWithRevocation(
    domain="your-tenant.auth0.com",
    api_identifier="https://api.yourapp.com",
    revocation_service=revocation_service,
)

# Revoke specific token
await auth_provider.logout(token_payload)

# Revoke all user tokens (logout all sessions)
await auth_provider.logout_all_sessions(user_id)
