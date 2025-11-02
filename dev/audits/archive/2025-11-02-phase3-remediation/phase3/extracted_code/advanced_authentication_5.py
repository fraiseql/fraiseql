# Extracted from: docs/advanced/authentication.md
# Block number: 5
# Automatic validation process:
# 1. Fetch JWKS from https://your-tenant.auth0.com/.well-known/jwks.json
# 2. Verify signature using RS256 algorithm
# 3. Check audience matches api_identifier
# 4. Check issuer matches https://your-tenant.auth0.com/
# 5. Check token not expired (exp claim)
# 6. Extract user information into UserContext


async def validate_token(self, token: str) -> dict[str, Any]:
    """Validate Auth0 JWT token."""
    try:
        # Get signing key from JWKS (cached)
        signing_key = self.jwks_client.get_signing_key_from_jwt(token)

        # Decode and verify
        payload = jwt.decode(
            token,
            signing_key.key,
            algorithms=self.algorithms,
            audience=self.api_identifier,
            issuer=self.issuer,
        )

        return payload

    except jwt.ExpiredSignatureError:
        raise TokenExpiredError("Token has expired")
    except jwt.InvalidTokenError as e:
        raise InvalidTokenError(f"Invalid token: {e}")
