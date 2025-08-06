"""Debug duplicate email test."""

import asyncio
import logging
import sys
from pathlib import Path
from uuid import uuid4

import psycopg

logger = logging.getLogger(__name__)

sys.path.insert(0, str(Path(__file__).resolve().parent))

from db import BlogRepository


async def test_duplicate_email():
    """Test duplicate email handling."""
    async with await psycopg.AsyncConnection.connect(
        "postgresql://localhost/blog_test",
    ) as conn:
        repo = BlogRepository(conn)

        # Clean up
        await conn.execute("TRUNCATE TABLE tb_users CASCADE")

        # Create first user
        email = f"test_{uuid4()}@example.com"
        result1 = await repo.create_user(
            {"email": email, "name": "First User", "bio": "First bio"},
        )
        logger.debug("First user creation: %s", result1)

        # Try to create duplicate
        result2 = await repo.create_user(
            {"email": email, "name": "Duplicate User", "bio": "Second bio"},
        )
        logger.debug("Duplicate user creation: %s", result2)

        # Check what's in the database
        async with conn.cursor() as cur:
            await cur.execute(
                "SELECT id, email, name FROM tb_users WHERE email = %s",
                (email,),
            )
            users = await cur.fetchall()
            logger.debug("Users with email %s: %s", email, users)


if __name__ == "__main__":
    asyncio.run(test_duplicate_email())
