# Extracted from: docs/advanced/authentication.md
# Block number: 8
from typing import Any

import jwt

from fraiseql.auth import AuthProvider, InvalidTokenError, TokenExpiredError, UserContext


class CustomJWTProvider(AuthProvider):
    """Custom JWT authentication provider."""

    def __init__(
        self,
        secret_key: str,
        algorithm: str = "HS256",
        issuer: str | None = None,
        audience: str | None = None,
    ):
        self.secret_key = secret_key
        self.algorithm = algorithm
        self.issuer = issuer
        self.audience = audience

    async def validate_token(self, token: str) -> dict[str, Any]:
        """Validate JWT token with secret key."""
        try:
            payload = jwt.decode(
                token,
                self.secret_key,
                algorithms=[self.algorithm],
                audience=self.audience,
                issuer=self.issuer,
                options={
                    "verify_signature": True,
                    "verify_exp": True,
                    "verify_aud": self.audience is not None,
                    "verify_iss": self.issuer is not None,
                },
            )
            return payload

        except jwt.ExpiredSignatureError:
            raise TokenExpiredError("Token has expired")
        except jwt.InvalidTokenError as e:
            raise InvalidTokenError(f"Invalid token: {e}")

    async def get_user_from_token(self, token: str) -> UserContext:
        """Extract UserContext from token payload."""
        payload = await self.validate_token(token)

        return UserContext(
            user_id=UUID(payload.get("sub", payload.get("user_id"))),
            email=payload.get("email"),
            name=payload.get("name"),
            roles=payload.get("roles", []),
            permissions=payload.get("permissions", []),
            metadata={
                k: v
                for k, v in payload.items()
                if k
                not in [
                    "sub",
                    "user_id",
                    "email",
                    "name",
                    "roles",
                    "permissions",
                    "exp",
                    "iat",
                    "iss",
                    "aud",
                ]
            },
        )
