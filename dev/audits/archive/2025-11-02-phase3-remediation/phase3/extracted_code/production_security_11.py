# Extracted from: docs/production/security.md
# Block number: 11
from fraiseql import mutation

# JWT configuration
from fraiseql.auth import CustomJWTProvider

auth_provider = CustomJWTProvider(
    secret_key=os.getenv("JWT_SECRET_KEY"),  # NEVER hardcode
    algorithm="HS256",
    issuer="https://yourapp.com",
    audience="https://api.yourapp.com",
)

# Token expiration
ACCESS_TOKEN_TTL = 3600  # 1 hour
REFRESH_TOKEN_TTL = 2592000  # 30 days


# Token rotation
@mutation
async def refresh_access_token(info, refresh_token: str) -> dict:
    """Rotate access token using refresh token."""
    # Validate refresh token
    payload = await auth_provider.validate_token(refresh_token)

    # Check token type
    if payload.get("token_type") != "refresh":
        raise ValueError("Invalid token type")

    # Generate new access token
    new_access_token = generate_access_token(user_id=payload["sub"], ttl=ACCESS_TOKEN_TTL)

    # Optionally rotate refresh token too
    new_refresh_token = generate_refresh_token(user_id=payload["sub"], ttl=REFRESH_TOKEN_TTL)

    # Revoke old refresh token
    await revocation_service.revoke_token(payload)

    return {
        "access_token": new_access_token,
        "refresh_token": new_refresh_token,
        "token_type": "bearer",
    }
