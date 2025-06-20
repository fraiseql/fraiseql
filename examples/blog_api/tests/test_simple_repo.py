"""Simple repository test to debug issues."""

import asyncio
import logging

# Add parent directory to path
import sys
from pathlib import Path
from uuid import uuid4

import psycopg
import pytest

logger = logging.getLogger(__name__)

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from db import BlogRepository


@pytest.mark.asyncio
async def test_simple_create_user():
    """Test creating a user with direct connection."""
    # Create direct connection
    async with await psycopg.AsyncConnection.connect(
        "postgresql://localhost/blog_test",
    ) as conn:
        repo = BlogRepository(conn)

        # Clean up first
        await conn.execute("TRUNCATE TABLE tb_users CASCADE")

        # Create user
        result = await repo.create_user(
            {
                "email": f"test_{uuid4()}@example.com",
                "name": "Test User",
                "bio": "Test bio",
            },
        )

        assert result["success"] is True
        assert "user_id" in result

        # Verify user exists - debug first
        async with conn.cursor() as cur:
            await cur.execute(
                "SELECT data FROM v_users WHERE id = %s", (result["user_id"],),
            )
            row = await cur.fetchone()
            logger.debug(f"Raw data from view: {row}")

        # Now try to get via repository
        try:
            user = await repo.get_user_by_id(result["user_id"])
            assert user is not None
            assert user.name == "Test User"
        except Exception as e:
            logger.debug(f"Error getting user: {e}")
            # Get raw data
            async with conn.cursor() as cur:
                await cur.execute(
                    "SELECT * FROM tb_users WHERE id = %s", (result["user_id"],),
                )
                raw = await cur.fetchone()
                logger.debug(f"Raw user data: {raw}")


if __name__ == "__main__":
    asyncio.run(test_simple_create_user())
