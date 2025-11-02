# Extracted from: docs/advanced/authentication.md
# Block number: 9
from fraiseql.fastapi import create_fraiseql_app

# Create provider
auth_provider = CustomJWTProvider(
    secret_key="your-secret-key-keep-secure",
    algorithm="HS256",
    issuer="https://yourapp.com",
    audience="https://api.yourapp.com",
)

# Create app
app = create_fraiseql_app(types=[User, Post], auth_provider=auth_provider)
