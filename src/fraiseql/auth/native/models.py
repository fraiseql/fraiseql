"""User model for native authentication."""

import re
import uuid
from datetime import datetime
from typing import Any, Optional

from passlib.hash import argon2
from psycopg import AsyncCursor
from psycopg.types.json import Json

# Configure Argon2id with secure parameters
# time_cost=2 rounds, memory_cost=100MB, parallelism=8 threads
argon2_hasher = argon2.using(
    type="id",  # Use Argon2id variant
    time_cost=2,
    memory_cost=102400,  # ~100MB
    parallelism=8,
    digest_size=32,
)


class User:
    """User model for native authentication."""

    def __init__(
        self,
        email: str,
        password: str | None = None,
        name: str | None = None,
        roles: list[str] | None = None,
        permissions: list[str] | None = None,
        metadata: dict[str, Any] | None = None,
        is_active: bool = True,
        email_verified: bool = False,
        id: str | None = None,
        password_hash: str | None = None,
        created_at: datetime | None = None,
        updated_at: datetime | None = None,
    ):
        """Initialize a User instance.

        Args:
            email: User's email address
            password: Plain text password (will be hashed)
            name: User's display name
            roles: List of roles assigned to user
            permissions: List of permissions granted to user
            metadata: Additional user metadata (JSONB)
            is_active: Whether user account is active
            email_verified: Whether email has been verified
            id: User ID (UUID)
            password_hash: Hashed password (for loading from DB)
            created_at: Creation timestamp
            updated_at: Last update timestamp
        """
        self.id = id or str(uuid.uuid4())
        self.email = email.lower().strip()
        self.name = name
        self.roles = roles or []
        self.permissions = permissions or []
        self.metadata = metadata or {}
        self.is_active = is_active
        self.email_verified = email_verified
        self.created_at = created_at
        self.updated_at = updated_at

        # Handle password - either hash a new one or use existing hash
        if password_hash:
            self._password_hash = password_hash
        elif password:
            self._password_hash = self._hash_password(password)
        else:
            self._password_hash = None

    @staticmethod
    def _hash_password(password: str) -> str:
        """Hash a password using Argon2id."""
        return argon2_hasher.hash(password)

    def verify_password(self, password: str) -> bool:
        """Verify a password against the stored hash."""
        if not self._password_hash or not password:
            return False
        return argon2_hasher.verify(password, self._password_hash)

    def set_password(self, new_password: str) -> None:
        """Update user's password (hash and store internally)."""
        if not self.validate_password(new_password):
            raise ValueError("Password does not meet security requirements")
        self._password_hash = argon2_hasher.hash(new_password)
        # Update the updated_at timestamp
        from datetime import UTC, datetime

        self.updated_at = datetime.now(UTC)

    @staticmethod
    def validate_password(password: str) -> bool:
        """Validate password meets security requirements.

        Requirements:
        - At least 8 characters long
        - Contains at least one uppercase letter
        - Contains at least one lowercase letter
        - Contains at least one number
        - Contains at least one special character
        """
        if not password or len(password) < 8:
            return False

        if not re.search(r"[A-Z]", password):
            return False

        if not re.search(r"[a-z]", password):
            return False

        if not re.search(r"\d", password):
            return False

        if not re.search(r'[!@#$%^&*(),.?":{}|<>]', password):
            return False

        return True

    @staticmethod
    def validate_email(email: str) -> bool:
        """Validate email format."""
        if not email:
            return False

        # Basic email regex
        pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
        return bool(re.match(pattern, email.strip()))

    async def save(self, cursor: AsyncCursor, schema: str) -> None:
        """Save user to database."""
        await cursor.execute(
            f"""
            INSERT INTO {schema}.tb_user (
                pk_user, email, password_hash, name,
                roles, permissions, metadata,
                is_active, email_verified
            ) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
            RETURNING created_at, updated_at
        """,
            (
                self.id,
                self.email,
                self._password_hash,
                self.name,
                self.roles,
                self.permissions,
                Json(self.metadata),
                self.is_active,
                self.email_verified,
            ),
        )

        # Fetch the timestamps set by the database
        row = await cursor.fetchone()
        if row:
            self.created_at = row[0]
            self.updated_at = row[1]

    async def update(self, cursor: AsyncCursor, schema: str) -> None:
        """Update user in database."""
        await cursor.execute(
            f"""
            UPDATE {schema}.tb_user
            SET name = %s,
                roles = %s,
                permissions = %s,
                metadata = %s,
                is_active = %s,
                email_verified = %s,
                updated_at = CURRENT_TIMESTAMP
            WHERE pk_user = %s
        """,
            (
                self.name,
                self.roles,
                self.permissions,
                Json(self.metadata),
                self.is_active,
                self.email_verified,
                self.id,
            ),
        )

    async def deactivate(self, cursor: AsyncCursor, schema: str) -> None:
        """Deactivate user account."""
        self.is_active = False
        await cursor.execute(
            f"""
            UPDATE {schema}.tb_user
            SET is_active = false,
                updated_at = CURRENT_TIMESTAMP
            WHERE pk_user = %s
        """,
            (self.id,),
        )

    async def verify_email(self, cursor: AsyncCursor, schema: str) -> None:
        """Mark email as verified."""
        self.email_verified = True
        await cursor.execute(
            f"""
            UPDATE {schema}.tb_user
            SET email_verified = true,
                updated_at = CURRENT_TIMESTAMP
            WHERE pk_user = %s
        """,
            (self.id,),
        )

    @classmethod
    async def get_by_email(cls, cursor: AsyncCursor, schema: str, email: str) -> Optional["User"]:
        """Get user by email address."""
        await cursor.execute(
            f"""
            SELECT pk_user, email, password_hash, name,
                   roles, permissions, metadata,
                   is_active, email_verified,
                   created_at, updated_at
            FROM {schema}.tb_user
            WHERE email = %s
        """,
            (email.lower().strip(),),
        )

        row = await cursor.fetchone()
        if not row:
            return None

        return cls(
            id=str(row[0]),
            email=row[1],
            password_hash=row[2],
            name=row[3],
            roles=row[4] or [],
            permissions=row[5] or [],
            metadata=row[6] or {},
            is_active=row[7],
            email_verified=row[8],
            created_at=row[9],
            updated_at=row[10],
        )

    @classmethod
    async def get_by_id(cls, cursor: AsyncCursor, schema: str, user_id: str) -> Optional["User"]:
        """Get user by ID."""
        await cursor.execute(
            f"""
            SELECT pk_user, email, password_hash, name,
                   roles, permissions, metadata,
                   is_active, email_verified,
                   created_at, updated_at
            FROM {schema}.tb_user
            WHERE pk_user = %s
        """,
            (user_id,),
        )

        row = await cursor.fetchone()
        if not row:
            return None

        return cls(
            id=str(row[0]),
            email=row[1],
            password_hash=row[2],
            name=row[3],
            roles=row[4] or [],
            permissions=row[5] or [],
            metadata=row[6] or {},
            is_active=row[7],
            email_verified=row[8],
            created_at=row[9],
            updated_at=row[10],
        )
