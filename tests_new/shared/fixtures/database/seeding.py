"""Test data seeding utilities for FraiseQL blog demos.

This module provides utilities to seed test databases with realistic blog data
for testing purposes.
"""

import json
import logging
import secrets
import uuid
from datetime import UTC, datetime
from typing import Any, Dict, List

import psycopg

logger = logging.getLogger(__name__)


class BlogDataSeeder:
    """Utility class for seeding blog test data."""

    def __init__(self, connection: psycopg.AsyncConnection):
        self.connection = connection

    async def seed_users(self, count: int = 5) -> List[Dict[str, Any]]:
        """Seed test users into the database."""
        users = []

        for i in range(count):
            # Add random suffix to avoid conflicts
            user_suffix = secrets.randbelow(9000) + 1000
            user_data = {
                "pk_user": str(uuid.uuid4()),
                "identifier": f"testuser_{user_suffix}_{i + 1}",
                "email": f"testuser{user_suffix}_{i + 1}@example.com",
                "name": f"Test User {user_suffix}-{i + 1}",
                "role": "author" if i < 3 else "user",
                "status": "active",
                "profile": {
                    "bio": f"Bio for test user {user_suffix}-{i + 1}",
                    "location": f"Test City {user_suffix}",
                    "website": f"https://testuser{user_suffix}.com",
                },
            }
            users.append(user_data)

        # Insert users using raw SQL
        async with self.connection.cursor() as cursor:
            for user in users:
                await cursor.execute(
                    """
                    INSERT INTO tb_user (
                        pk_user, identifier, email, password_hash,
                        role, is_active, profile, created_at, updated_at
                    )
                    VALUES (
                        %(pk_user)s, %(identifier)s, %(email)s, %(password_hash)s,
                        %(role)s, %(is_active)s, %(profile)s, NOW(), NOW()
                    )
                    """,
                    {
                        "pk_user": user["pk_user"],
                        "identifier": user["identifier"],
                        "email": user["email"],
                        "password_hash": "fake_hash_for_testing",  # Not used in tests
                        "role": user["role"],
                        "is_active": user["status"] == "active",
                        "profile": json.dumps(user["profile"]),
                    },
                )

        logger.info(f"✅ Seeded {count} test users")
        return users

    async def seed_posts(
        self, users: List[Dict[str, Any]], count_per_user: int = 2
    ) -> List[Dict[str, Any]]:
        """Seed test posts for the given users."""
        posts = []

        for i, user in enumerate(users[:3]):  # Only authors create posts
            for j in range(count_per_user):
                # Add random suffix to avoid conflicts
                post_suffix = secrets.randbelow(90000) + 10000
                post_data = {
                    "pk_post": str(uuid.uuid4()),
                    "identifier": f"test-post-{post_suffix}-{i + 1}-{j + 1}",
                    "title": f"Test Blog Post {post_suffix}-{i + 1}-{j + 1}",
                    "content": (
                        f"This is the content for test post {post_suffix}-{i + 1}-{j + 1}. "
                        "It contains multiple paragraphs to demonstrate the blog functionality. "
                        "This is a comprehensive post about testing FraiseQL."
                    ),
                    "excerpt": f"Excerpt for test post {post_suffix}-{i + 1}-{j + 1}...",
                    "status": "published" if j % 2 == 0 else "draft",
                    "author_id": user["pk_user"],
                    "author": user,
                }
                posts.append(post_data)

        # Insert posts
        async with self.connection.cursor() as cursor:
            for post in posts:
                await cursor.execute(
                    """
                    INSERT INTO tb_post (
                        pk_post, identifier, fk_author, title, content,
                        excerpt, status, published_at, created_at, updated_at
                    )
                    VALUES (
                        %(pk_post)s, %(identifier)s, %(fk_author)s, %(title)s, %(content)s,
                        %(excerpt)s, %(status)s, %(published_at)s, NOW(), NOW()
                    )
                    """,
                    {
                        "pk_post": post["pk_post"],
                        "identifier": post["identifier"],
                        "fk_author": post["author_id"],
                        "title": post["title"],
                        "content": post["content"],
                        "excerpt": post["excerpt"],
                        "status": post["status"],
                        "published_at": datetime.now(UTC)
                        if post["status"] == "published"
                        else None,
                    },
                )

        logger.info(f"✅ Seeded {len(posts)} test posts")
        return posts

    async def seed_tags(self, count: int = 8) -> List[Dict[str, Any]]:
        """Seed test tags."""
        tag_names = [
            "FraiseQL",
            "Testing",
            "Database",
            "GraphQL",
            "Python",
            "PostgreSQL",
            "Integration",
            "E2E",
        ]

        tags = []
        for name in tag_names[:count]:
            # Add random suffix to avoid conflicts with existing seed data
            clean_name = name.lower().replace(" ", "-")
            unique_id = f"{clean_name}-test-{secrets.randbelow(9000) + 1000}"
            tag_data = {
                "pk_tag": str(uuid.uuid4()),
                "identifier": unique_id,
                "name": f"{name} (Test)",
                "description": f"Test posts tagged with {name}",
            }
            tags.append(tag_data)

        # Insert tags
        async with self.connection.cursor() as cursor:
            for tag in tags:
                await cursor.execute(
                    """
                    INSERT INTO tb_tag (
                        pk_tag, identifier, name, description, created_at, updated_at
                    )
                    VALUES (%(pk_tag)s, %(identifier)s, %(name)s, %(description)s, NOW(), NOW())
                    """,
                    {
                        "pk_tag": tag["pk_tag"],
                        "identifier": tag["identifier"],
                        "name": tag["name"],
                        "description": tag["description"],
                    },
                )

        logger.info(f"✅ Seeded {count} test tags")
        return tags

    async def seed_comments(
        self, posts: List[Dict[str, Any]], users: List[Dict[str, Any]]
    ) -> List[Dict[str, Any]]:
        """Seed test comments for the given posts."""
        comments = []

        # Add 2-3 comments per published post
        published_posts = [p for p in posts if p["status"] == "published"]

        for i, post in enumerate(published_posts[:3]):  # Limit to first 3 posts
            for j in range(2):  # 2 comments per post
                user_idx = (i + j) % len(users)
                comment_data = {
                    "pk_comment": str(uuid.uuid4()),
                    "content": (
                        f"This is a test comment {j + 1} on post '{post['title']}'. "
                        "Great article, thanks for sharing!"
                    ),
                    "status": "approved",
                    "post_id": post["pk_post"],
                    "author_id": users[user_idx]["pk_user"],
                    "author": users[user_idx],
                }
                comments.append(comment_data)

        # Insert comments
        async with self.connection.cursor() as cursor:
            for comment in comments:
                await cursor.execute(
                    """
                    INSERT INTO tb_comment (
                        pk_comment, fk_post, fk_author, content, status, created_at, updated_at
                    )
                    VALUES (
                        %(pk_comment)s, %(fk_post)s, %(fk_author)s, 
                        %(content)s, %(status)s, NOW(), NOW()
                    )
                    """,
                    {
                        "pk_comment": comment["pk_comment"],
                        "fk_post": comment["post_id"],
                        "fk_author": comment["author_id"],
                        "content": comment["content"],
                        "status": comment["status"],
                    },
                )

        logger.info(f"✅ Seeded {len(comments)} test comments")
        return comments

    async def seed_all_data(self) -> Dict[str, List[Dict[str, Any]]]:
        """Seed all test data and return references."""
        logger.info("🌱 Starting full database seeding...")

        # Seed in proper order due to foreign key constraints
        users = await self.seed_users(5)
        posts = await self.seed_posts(users, 2)
        tags = await self.seed_tags(6)
        comments = await self.seed_comments(posts, users)

        # Associate some posts with tags
        await self.associate_posts_with_tags(posts[:4], tags[:3])

        logger.info("🎉 Database seeding completed successfully!")

        return {"users": users, "posts": posts, "tags": tags, "comments": comments}

    async def associate_posts_with_tags(
        self, posts: List[Dict[str, Any]], tags: List[Dict[str, Any]]
    ):
        """Associate posts with tags via the post_tag junction table."""
        associations = []

        for i, post in enumerate(posts):
            # Each post gets 1-2 tags
            tags_for_post = tags[i % len(tags) : (i % len(tags)) + 2]

            for tag in tags_for_post:
                association = {"post_id": post["pk_post"], "tag_id": tag["pk_tag"]}
                associations.append(association)

        # Insert associations
        async with self.connection.cursor() as cursor:
            for assoc in associations:
                await cursor.execute(
                    """
                    INSERT INTO tb_post_tag (fk_post, fk_tag, created_at)
                    VALUES (%(post_id)s, %(tag_id)s, NOW())
                """,
                    assoc,
                )

        logger.info(f"✅ Created {len(associations)} post-tag associations")

    async def cleanup_all_data(self):
        """Clean up all test data from the database."""
        logger.info("🧹 Cleaning up test data...")

        async with self.connection.cursor() as cursor:
            # Delete in reverse order due to foreign key constraints
            await cursor.execute("DELETE FROM tb_post_tag")
            await cursor.execute("DELETE FROM tb_comment")
            await cursor.execute("DELETE FROM tb_post")
            await cursor.execute("DELETE FROM tb_tag")
            await cursor.execute("DELETE FROM tb_user")

        logger.info("✅ Test data cleanup completed")
