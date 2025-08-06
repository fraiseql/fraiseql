"""Simple test to debug connection issues."""

import asyncio
import logging

import psycopg
from psycopg_pool import AsyncConnectionPool

logger = logging.getLogger(__name__)


async def test_connection():
    """Test basic database connection."""
    logger.debug("Testing connection to postgresql://localhost/blog_test")

    # Test direct connection
    try:
        async with (
            await psycopg.AsyncConnection.connect(
                "postgresql://localhost/blog_test",
            ) as conn,
            conn.cursor() as cur,
        ):
            await cur.execute("SELECT 1")
            result = await cur.fetchone()
            logger.debug("Direct connection works: %s", result)
    except Exception as e:
        logger.debug("Direct connection failed: %s", e)

    # Test pool connection
    try:
        async with AsyncConnectionPool(
            "postgresql://localhost/blog_test",
            min_size=1,
            max_size=5,
        ) as pool:
            logger.debug("Pool created")
            async with pool.connection() as conn, conn.cursor() as cur:
                await cur.execute("SELECT 1")
                result = await cur.fetchone()
                logger.debug("Pool connection works: %s", result)
    except Exception as e:
        logger.debug("Pool connection failed: %s", e)


if __name__ == "__main__":
    asyncio.run(test_connection())
