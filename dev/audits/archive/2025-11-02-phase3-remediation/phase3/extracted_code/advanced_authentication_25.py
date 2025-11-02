# Extracted from: docs/advanced/authentication.md
# Block number: 25
from fraiseql.auth import Auth0Provider, CustomJWTProvider


class MultiAuthProvider:
    """Support multiple authentication providers."""

    def __init__(self):
        self.providers = {
            "auth0": Auth0Provider(domain="tenant.auth0.com", api_identifier="https://api.app.com"),
            "api_key": CustomJWTProvider(secret_key="api-key-secret", algorithm="HS256"),
        }

    async def validate_token(self, token: str) -> dict:
        """Try each provider until one succeeds."""
        errors = []

        for name, provider in self.providers.items():
            try:
                return await provider.validate_token(token)
            except Exception as e:
                errors.append(f"{name}: {e}")

        raise InvalidTokenError(f"All providers failed: {errors}")

    async def get_user_from_token(self, token: str) -> UserContext:
        """Extract user from first successful provider."""
        payload = await self.validate_token(token)

        # Determine provider from token and extract user
        if "iss" in payload and "auth0.com" in payload["iss"]:
            return await self.providers["auth0"].get_user_from_token(token)
        return await self.providers["api_key"].get_user_from_token(token)
