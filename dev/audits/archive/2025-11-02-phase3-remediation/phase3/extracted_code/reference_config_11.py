# Extracted from: docs/reference/config.md
# Block number: 11
# Auth0 configuration
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    auth_enabled=True,
    auth_provider="auth0",
    auth0_domain="myapp.auth0.com",
    auth0_api_identifier="https://api.myapp.com",
    auth0_algorithms=["RS256"],
)

# Development auth
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    environment="development",
    auth_provider="custom",
    dev_auth_username="admin",
    dev_auth_password="secret",
)
