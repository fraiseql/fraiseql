# Extracted from: docs/production/security.md
# Block number: 2
from fraiseql import mutation
from fraiseql.security import ValidationResult


class UserInputValidator:
    """Validate user inputs."""

    @staticmethod
    def validate_user_id(user_id: str) -> ValidationResult:
        """Validate UUID format."""
        import uuid

        try:
            uuid.UUID(user_id)
            return ValidationResult(valid=True)
        except ValueError:
            return ValidationResult(valid=False, error="Invalid user ID format")

    @staticmethod
    def validate_email(email: str) -> ValidationResult:
        """Validate email format."""
        import re

        pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
        if re.match(pattern, email):
            return ValidationResult(valid=True)
        return ValidationResult(valid=False, error="Invalid email format")


# Usage in resolver
@mutation
async def update_user(info, user_id: str, email: str) -> User:
    # Validate inputs
    user_id_valid = UserInputValidator.validate_user_id(user_id)
    if not user_id_valid.valid:
        raise ValueError(user_id_valid.error)

    email_valid = UserInputValidator.validate_email(email)
    if not email_valid.valid:
        raise ValueError(email_valid.error)

    # Safe to proceed
    return await update_user_email(user_id, email)
