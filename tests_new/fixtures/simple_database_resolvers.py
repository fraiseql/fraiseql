"""Simple database-backed GraphQL resolvers for E2E testing.

This module provides straightforward GraphQL resolvers that interact with the
real database using basic SQL queries, replacing mock implementations for
comprehensive E2E testing.
"""

import re
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional
from uuid import UUID

import fraiseql


@fraiseql.query
async def users(info, limit: int = 10, offset: int = 0) -> List[Dict[str, Any]]:
    """Get users from database."""
    # Get the database connection from context
    db_conn = info.context["db"]

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            SELECT id, username, email, role, is_active, email_verified,
                   created_at, updated_at, last_login_at, profile, preferences, metadata
            FROM v_user
            ORDER BY created_at DESC
            LIMIT %s OFFSET %s
        """,
            (limit, offset),
        )

        rows = await cursor.fetchall()
        return [dict(row) for row in rows]


@fraiseql.query
async def posts(
    info, limit: int = 10, offset: int = 0, where: Optional[Dict] = None
) -> List[Dict[str, Any]]:
    """Get posts from database."""
    db_conn = info.context["db"]

    # Build where clause
    where_clause = ""
    params = []

    if where:
        conditions = []
        if where.get("status") and where["status"].get("equals"):
            conditions.append("status = %s")
            params.append(where["status"]["equals"])
        where_clause = f"WHERE {' AND '.join(conditions)}" if conditions else ""

    # Add limit/offset params
    params.extend([limit, offset])

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            f"""
            SELECT id, title, slug, content, excerpt, author_id, status, featured,
                   created_at, updated_at, published_at, seo_metadata, custom_fields
            FROM v_post
            {where_clause}
            ORDER BY COALESCE(published_at, created_at) DESC
            LIMIT %s OFFSET %s
        """,
            params,
        )

        rows = await cursor.fetchall()
        return [dict(row) for row in rows]


@fraiseql.query
async def user(info, id: UUID) -> Optional[Dict[str, Any]]:
    """Get single user by ID."""
    db_conn = info.context["db"]

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            SELECT id, username, email, role, is_active, email_verified,
                   created_at, updated_at, last_login_at, profile, preferences, metadata
            FROM v_user
            WHERE id = %s
        """,
            (id,),
        )

        row = await cursor.fetchone()
        return dict(row) if row else None


@fraiseql.query
async def post(info, id: UUID) -> Optional[Dict[str, Any]]:
    """Get single post by ID."""
    db_conn = info.context["db"]

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            SELECT id, title, slug, content, excerpt, author_id, status, featured,
                   created_at, updated_at, published_at, seo_metadata, custom_fields
            FROM v_post
            WHERE id = %s
        """,
            (id,),
        )

        row = await cursor.fetchone()
        return dict(row) if row else None


@fraiseql.query
async def comments(
    info, limit: int = 10, offset: int = 0, where: Optional[Dict] = None
) -> List[Dict[str, Any]]:
    """Get comments from database."""
    db_conn = info.context["db"]

    # Build where clause
    where_clause = ""
    params = []

    if where:
        conditions = []
        if where.get("post_id"):
            conditions.append("post_id = %s")
            params.append(where["post_id"])
        if where.get("status"):
            conditions.append("status = %s")
            params.append(where["status"])
        where_clause = f"WHERE {' AND '.join(conditions)}" if conditions else ""

    # Add limit/offset params
    params.extend([limit, offset])

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            f"""
            SELECT id, post_id, author_id, parent_id, content, status,
                   created_at, updated_at, moderation_data
            FROM v_comment
            {where_clause}
            ORDER BY created_at ASC
            LIMIT %s OFFSET %s
        """,
            params,
        )

        rows = await cursor.fetchall()
        return [dict(row) for row in rows]


@fraiseql.query
async def tags(info, limit: int = 10, offset: int = 0) -> List[Dict[str, Any]]:
    """Get tags from database."""
    db_conn = info.context["db"]

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            SELECT id, name, slug, description, color, parent_id, sort_order, is_active, created_at
            FROM v_tag
            WHERE is_active = true
            ORDER BY sort_order ASC, name ASC
            LIMIT %s OFFSET %s
        """,
            (limit, offset),
        )

        rows = await cursor.fetchall()
        return [dict(row) for row in rows]


# Simple mutations that return dictionaries (FraiseQL will handle the type conversion)
@fraiseql.mutation
async def create_user(info, input: Dict[str, Any]) -> Dict[str, Any]:
    """Create user in database."""
    db_conn = info.context["db"]

    user_id = uuid.uuid4()
    now = datetime.now()

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            INSERT INTO tb_user (pk_user, identifier, email, password_hash, role, is_active,
                                profile, created_at, updated_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
        """,
            (
                user_id,
                input["username"],
                input["email"],
                f"hashed_{input['password']}",
                input.get("role", "user"),
                True,
                input.get("profile", {}),
                now,
                now,
            ),
        )

    # Return the created user data
    return {
        "id": user_id,
        "username": input["username"],
        "email": input["email"],
        "role": input.get("role", "user"),
        "isActive": True,
        "emailVerified": False,
        "createdAt": now,
        "updatedAt": now,
        "lastLoginAt": None,
        "profile": input.get("profile", {}),
        "preferences": {},
        "metadata": {},
    }


@fraiseql.mutation
async def create_post(info, input: Dict[str, Any]) -> Dict[str, Any]:
    """Create post in database."""
    db_conn = info.context["db"]

    post_id = uuid.uuid4()
    now = datetime.now()
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", input["title"].lower()).strip("-")

    # Get author ID from input or use first user from seeds
    author_id = input.get("authorId")
    if not author_id:
        async with db_conn.cursor() as cursor:
            await cursor.execute("SELECT pk_user FROM tb_user ORDER BY created_at ASC LIMIT 1")
            first_user = await cursor.fetchone()
            author_id = first_user[0] if first_user else uuid.uuid4()

    # Determine published_at based on status
    published_at = now if input.get("status") == "PUBLISHED" else None

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            INSERT INTO tb_post (pk_post, identifier, fk_author, title, content, excerpt,
                                status, featured, published_at, seo_metadata, custom_fields,
                                created_at, updated_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
        """,
            (
                post_id,
                slug,
                author_id,
                input["title"],
                input["content"],
                input.get("excerpt", input["content"][:200]),
                input.get("status", "DRAFT"),
                input.get("featured", False),
                published_at,
                input.get("seoMetadata", {}),
                input.get("customFields", {}),
                now,
                now,
            ),
        )

    return {
        "id": post_id,
        "title": input["title"],
        "slug": slug,
        "content": input["content"],
        "excerpt": input.get("excerpt", input["content"][:200]),
        "authorId": author_id,
        "status": input.get("status", "DRAFT"),
        "featured": input.get("featured", False),
        "createdAt": now,
        "updatedAt": now,
        "publishedAt": published_at,
        "seoMetadata": input.get("seoMetadata", {}),
        "customFields": input.get("customFields", {}),
    }


@fraiseql.mutation
async def create_comment(info, input: Dict[str, Any]) -> Dict[str, Any]:
    """Create comment in database."""
    db_conn = info.context["db"]

    comment_id = uuid.uuid4()
    now = datetime.now()

    # Get author from context or use first user from seeds
    author_id = info.context.get("user_id")
    if not author_id:
        async with db_conn.cursor() as cursor:
            await cursor.execute("SELECT pk_user FROM tb_user ORDER BY created_at ASC LIMIT 1")
            first_user = await cursor.fetchone()
            author_id = first_user[0] if first_user else uuid.uuid4()

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            INSERT INTO tb_comment (pk_comment, fk_post, fk_author, fk_parent, content, status,
                                   created_at, updated_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
        """,
            (
                comment_id,
                input["postId"],
                author_id,
                input.get("parentId"),
                input["content"],
                "PENDING",
                now,
                now,
            ),
        )

    return {
        "id": comment_id,
        "postId": input["postId"],
        "authorId": author_id,
        "parentId": input.get("parentId"),
        "content": input["content"],
        "status": "PENDING",
        "createdAt": now,
        "updatedAt": now,
        "moderationData": {},
    }


@fraiseql.mutation
async def create_tag(info, input: Dict[str, Any]) -> Dict[str, Any]:
    """Create tag in database."""
    db_conn = info.context["db"]

    tag_id = uuid.uuid4()
    now = datetime.now()
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", input["name"].lower()).strip("-")

    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            INSERT INTO tb_tag (pk_tag, identifier, fk_parent, name, description, color,
                               sort_order, is_active, created_at)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
        """,
            (
                tag_id,
                slug,
                input.get("parentId"),
                input["name"],
                input.get("description"),
                input.get("color", "#6B7280"),
                input.get("sortOrder", 0),
                True,
                now,
            ),
        )

    return {
        "id": tag_id,
        "name": input["name"],
        "slug": slug,
        "description": input.get("description"),
        "color": input.get("color", "#6B7280"),
        "parentId": input.get("parentId"),
        "sortOrder": input.get("sortOrder", 0),
        "isActive": True,
        "createdAt": now,
    }


@fraiseql.mutation
async def update_post(info, id: UUID, input: Dict[str, Any]) -> Dict[str, Any]:
    """Update post in database."""
    db_conn = info.context["db"]
    now = datetime.now()

    # Get existing post
    async with db_conn.cursor() as cursor:
        await cursor.execute("SELECT * FROM tb_post WHERE pk_post = %s", (id,))
        existing_post = await cursor.fetchone()
        if not existing_post:
            raise Exception("Post not found")

    # Build update fields
    update_fields = []
    update_values = []

    if input.get("title") is not None:
        update_fields.append("title = %s")
        update_values.append(input["title"])

    if input.get("content") is not None:
        update_fields.append("content = %s")
        update_values.append(input["content"])

    if input.get("excerpt") is not None:
        update_fields.append("excerpt = %s")
        update_values.append(input["excerpt"])

    if input.get("status") is not None:
        update_fields.append("status = %s")
        update_values.append(input["status"])

        # Set published_at if publishing
        if input["status"] == "PUBLISHED" and not existing_post["published_at"]:
            update_fields.append("published_at = %s")
            update_values.append(now)

    if input.get("featured") is not None:
        update_fields.append("featured = %s")
        update_values.append(input["featured"])

    # Always update updated_at
    update_fields.append("updated_at = %s")
    update_values.append(now)

    # Add WHERE clause
    update_values.append(id)

    if update_fields:
        async with db_conn.cursor() as cursor:
            await cursor.execute(
                f"UPDATE tb_post SET {', '.join(update_fields)} WHERE pk_post = %s", update_values
            )

    # Return updated post data
    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            SELECT id, title, slug, content, excerpt, author_id, status, featured,
                   created_at, updated_at, published_at, seo_metadata, custom_fields
            FROM v_post WHERE id = %s
        """,
            (id,),
        )
        updated_post = await cursor.fetchone()
        return dict(updated_post)


@fraiseql.mutation
async def publish_post(info, id: UUID) -> Dict[str, Any]:
    """Publish post in database."""
    return await update_post(info, id, {"status": "PUBLISHED"})


@fraiseql.mutation
async def update_comment(info, id: UUID, input: Dict[str, Any]) -> Dict[str, Any]:
    """Update comment in database."""
    db_conn = info.context["db"]
    now = datetime.now()

    # Build update fields
    update_fields = []
    update_values = []

    if input.get("content") is not None:
        update_fields.append("content = %s")
        update_values.append(input["content"])

    if input.get("status") is not None:
        update_fields.append("status = %s")
        update_values.append(input["status"])

        # Add moderation data if approving/rejecting
        if input["status"] in ["APPROVED", "REJECTED"]:
            update_fields.append("moderation_data = %s")
            moderation_data = {
                "moderatedAt": now.isoformat(),
                "moderatedBy": info.context.get("user_id", "system"),
                "reason": f"Marked as {input['status'].lower()}",
            }
            update_values.append(moderation_data)

    # Always update updated_at
    update_fields.append("updated_at = %s")
    update_values.append(now)

    # Add WHERE clause
    update_values.append(id)

    if update_fields:
        async with db_conn.cursor() as cursor:
            await cursor.execute(
                f"UPDATE tb_comment SET {', '.join(update_fields)} WHERE pk_comment = %s",
                update_values,
            )

    # Return updated comment data
    async with db_conn.cursor() as cursor:
        await cursor.execute(
            """
            SELECT id, post_id, author_id, parent_id, content, status,
                   created_at, updated_at, moderation_data
            FROM v_comment WHERE id = %s
        """,
            (id,),
        )
        updated_comment = await cursor.fetchone()
        return dict(updated_comment)
