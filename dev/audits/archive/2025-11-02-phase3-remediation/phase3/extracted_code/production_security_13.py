# Extracted from: docs/production/security.md
# Block number: 13

from dataclasses import dataclass


@dataclass
class User:
    """User with PII protection."""

    id: UUID
    email: str
    name: str
    _ssn: str | None = None  # Private field
    _credit_card: str | None = None

    @property
    def ssn_masked(self) -> str | None:
        """Return masked SSN."""
        if not self._ssn:
            return None
        return f"***-**-{self._ssn[-4:]}"

    @property
    def credit_card_masked(self) -> str | None:
        """Return masked credit card."""
        if not self._credit_card:
            return None
        return f"****-****-****-{self._credit_card[-4:]}"


# GraphQL type
@type_
class UserGQL:
    id: UUID
    email: str
    name: str

    # Only admins can see full SSN
    @authorize_field(lambda obj, info: info.context["user"].has_role("admin"))
    async def ssn(self) -> str | None:
        return self._ssn

    # Everyone sees masked version
    async def ssn_masked(self) -> str | None:
        return self.ssn_masked
