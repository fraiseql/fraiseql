# Extracted from: docs/advanced/authentication.md
# Block number: 4
from fraiseql.auth import Auth0Config, Auth0Provider
from fraiseql.fastapi import create_fraiseql_app

# Method 1: Direct provider instantiation
auth_provider = Auth0Provider(
    domain="your-tenant.auth0.com",
    api_identifier="https://api.yourapp.com",
    algorithms=["RS256"],
    cache_jwks=True,  # Cache JWKS keys for 1 hour
)

# Method 2: Using config object
auth_config = Auth0Config(
    domain="your-tenant.auth0.com",
    api_identifier="https://api.yourapp.com",
    client_id="your_client_id",  # Optional: for Management API
    client_secret="your_client_secret",  # Optional: for Management API
    algorithms=["RS256"],
)

auth_provider = auth_config.create_provider()

# Create app with authentication
app = create_fraiseql_app(types=[User, Post, Order], auth_provider=auth_provider)
