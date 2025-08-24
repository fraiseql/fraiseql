"""Real database-backed GraphQL client for E2E testing.

This module provides GraphQL resolvers that interact with the actual database
tables created by the blog demo schema, replacing mock implementations with
real database operations for comprehensive E2E testing.
"""

import re
import uuid
from datetime import datetime
from typing import List, Optional
from uuid import UUID

import fraiseql


# Real database-backed queries
@fraiseql.query
async def users(
    info,
    limit: int = 10,
    offset: int = 0,
    where: Optional[dict] = None,
    order_by: Optional[List[dict]] = None,
):
    """Get users from database."""
    db = info.context["db"]

    # Build where conditions for the actual table structure
    where_conditions = {}
    if where:
        if where.get("id"):
            where_conditions["pk_user"] = where["id"]
        if where.get("username"):
            where_conditions["identifier"] = where["username"]
        if where.get("email"):
            where_conditions["email"] = where["email"]
        if where.get("role"):
            where_conditions["role"] = where["role"]
        if where.get("is_active") is not None:
            where_conditions["is_active"] = where["is_active"]

    # Build order by
    order_by_str = "created_at DESC"
    if order_by:
        order_clauses = []
        for order in order_by:
            direction = "DESC" if order.get("direction", "ASC").upper() == "DESC" else "ASC"
            field = order.get("field", "created_at")

            if field == "username":
                order_clauses.append(f"identifier {direction}")
            elif field == "email":
                order_clauses.append(f"email {direction}")
            elif field == "created_at":
                order_clauses.append(f"created_at {direction}")

        if order_clauses:
            order_by_str = ", ".join(order_clauses)

    # Build query dynamically
    params = list(where_conditions.values())
    where_clause = ""
    if where_conditions:
        conditions = []
        for i, key in enumerate(where_conditions.keys(), 1):
            conditions.append(f"{key} = ${i}")
        where_clause = f"WHERE {' AND '.join(conditions)}"

    limit_param = len(params) + 1
    offset_param = len(params) + 2
    params.extend([limit, offset])

    # Use the connection directly for raw SQL
    async with db.cursor() as cursor:
        await cursor.execute(
            f"""
            SELECT id, username, email, role, is_active, email_verified,
                   created_at, updated_at, last_login_at, profile, preferences, metadata
            FROM v_user
            {where_clause}
            ORDER BY {order_by_str}
            LIMIT ${limit_param} OFFSET ${offset_param}
        """,
            params,
        )

        users_data = await cursor.fetchall()
        return [dict(row) for row in users_data]


@fraiseql.query
async def posts(
    info,
    limit: int = 10,
    offset: int = 0,
    where: Optional[dict] = None,
    order_by: Optional[List[dict]] = None,
):
    """Get posts from database."""
    db = info.context["db"]

    # Build where conditions for the actual table structure
    where_conditions = {}
    if where:
        if where.get("id"):
            where_conditions["pk_post"] = where["id"]
        if where.get("author_id"):
            where_conditions["author_id"] = where["author_id"]
        if where.get("status"):
            where_conditions["status"] = where["status"]
        if where.get("featured") is not None:
            where_conditions["featured"] = where["featured"]
        if where.get("title_contains"):
            # Use ILIKE for case-insensitive search
            where_conditions["title"] = f"%{where['title_contains']}%"

    # Build order by
    order_by_str = "created_at DESC"
    if order_by:
        order_clauses = []
        for order in order_by:
            direction = "DESC" if order.get("direction", "DESC").upper() == "DESC" else "ASC"
            field = order.get("field", "created_at")

            if field == "title":
                order_clauses.append(f"title {direction}")
            elif field == "created_at":
                order_clauses.append(f"created_at {direction}")
            elif field == "published_at":
                order_clauses.append(f"published_at {direction}")

        if order_clauses:
            order_by_str = ", ".join(order_clauses)

    # Handle special case for title search
    where_clause = ""
    params = []
    param_count = 0

    if where_conditions:
        conditions = []
        for key, value in where_conditions.items():
            param_count += 1
            if key == "title":
                conditions.append(f"title ILIKE ${param_count}")
            else:
                conditions.append(f"{key} = ${param_count}")
            params.append(value)
        where_clause = f"WHERE {' AND '.join(conditions)}"

    # Add limit/offset params
    limit_param = param_count + 1
    offset_param = param_count + 2
    params.extend([limit, offset])

    # Query the view
    result = await db.execute(
        f"""
        SELECT id, title, slug, content, excerpt, author_id, status, featured,
               created_at, updated_at, published_at, seo_metadata, custom_fields
        FROM v_post
        {where_clause}
        ORDER BY {order_by_str}
        LIMIT ${limit_param} OFFSET ${offset_param}
        """,
        *params,
    )

    posts_data = await result.fetchall()
    return [dict(row) for row in posts_data]


@fraiseql.query
async def user(info, id: UUID):
    """Get single user by ID."""
    db = info.context["db"]

    result = await db.execute(
        """
        SELECT id, username, email, role, is_active, email_verified,
               created_at, updated_at, last_login_at, profile, preferences, metadata
        FROM v_user
        WHERE id = $1
        """,
        id,
    )

    user_data = await result.fetchone()
    return dict(user_data) if user_data else None


@fraiseql.query
async def post(info, id: UUID):
    """Get single post by ID."""
    db = info.context["db"]

    result = await db.execute(
        """
        SELECT id, title, slug, content, excerpt, author_id, status, featured,
               created_at, updated_at, published_at, seo_metadata, custom_fields
        FROM v_post
        WHERE id = $1
        """,
        id,
    )

    post_data = await result.fetchone()
    return dict(post_data) if post_data else None


@fraiseql.query
async def comments(
    info,
    limit: int = 10,
    offset: int = 0,
    where: Optional[dict] = None,
    order_by: Optional[List[dict]] = None,
):
    """Get comments from database."""
    db = info.context["db"]

    # Build where conditions
    where_conditions = {}
    if where:
        if where.get("id"):
            where_conditions["pk_comment"] = where["id"]
        if where.get("post_id"):
            where_conditions["post_id"] = where["post_id"]
        if where.get("author_id"):
            where_conditions["author_id"] = where["author_id"]
        if where.get("status"):
            where_conditions["status"] = where["status"]
        if where.get("parent_id"):
            where_conditions["parent_id"] = where["parent_id"]

    # Build order by
    order_by_str = "created_at ASC"
    if order_by:
        order_clauses = []
        for order in order_by:
            direction = "DESC" if order.get("direction", "ASC").upper() == "DESC" else "ASC"
            field = order.get("field", "created_at")

            if field in ["created_at", "updated_at"]:
                order_clauses.append(f"{field} {direction}")

        if order_clauses:
            order_by_str = ", ".join(order_clauses)

    # Build query
    where_clause = ""
    params = []
    if where_conditions:
        conditions = []
        for i, (key, value) in enumerate(where_conditions.items(), 1):
            conditions.append(f"{key} = ${i}")
            params.append(value)
        where_clause = f"WHERE {' AND '.join(conditions)}"

    # Add limit/offset
    limit_param = len(params) + 1
    offset_param = len(params) + 2
    params.extend([limit, offset])

    result = await db.execute(
        f"""
        SELECT id, post_id, author_id, parent_id, content, status,
               created_at, updated_at, moderation_data
        FROM v_comment
        {where_clause}
        ORDER BY {order_by_str}
        LIMIT ${limit_param} OFFSET ${offset_param}
        """,
        *params,
    )

    comments_data = await result.fetchall()
    return [dict(row) for row in comments_data]


@fraiseql.query
async def tags(info, limit: int = 10, offset: int = 0):
    """Get tags from database."""
    db = info.context["db"]

    result = await db.execute(
        """
        SELECT id, name, slug, description, color, parent_id, sort_order, is_active, created_at
        FROM v_tag
        WHERE is_active = true
        ORDER BY sort_order ASC, name ASC
        LIMIT $1 OFFSET $2
        """,
        limit,
        offset,
    )

    tags_data = await result.fetchall()
    return [dict(row) for row in tags_data]


# Real database-backed mutations
@fraiseql.mutation
async def create_user(info, input: dict):
    """Create user in database."""
    db = info.context["db"]

    user_id = uuid.uuid4()
    now = datetime.now()

    # Hash password (mock implementation for testing)
    password_hash = f"hashed_{input['password']}"

    await db.execute(
        """
        INSERT INTO tb_user (pk_user, identifier, email, password_hash, role, is_active,
                            profile, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        """,
        user_id,
        input["username"],
        input["email"],
        password_hash,
        input.get("role", "user"),
        True,
        input.get("profile", {}),
        now,
        now,
    )

    # Return the created user data
    return {
        "id": user_id,
        "username": input["username"],
        "email": input["email"],
        "role": input.get("role", "user"),
        "is_active": True,
        "email_verified": False,
        "created_at": now,
        "updated_at": now,
        "last_login_at": None,
        "profile": input.get("profile", {}),
        "preferences": {},
        "metadata": {},
    }


@fraiseql.mutation
async def create_post(info, input: dict):
    """Create post in database."""
    db = info.context["db"]

    post_id = uuid.uuid4()
    now = datetime.now()
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", input["title"].lower()).strip("-")

    # Get author ID from input or context
    author_id = input.get("author_id")
    if not author_id:
        # For testing, use the first user from seed data
        result = await db.execute("SELECT pk_user FROM tb_user ORDER BY created_at ASC LIMIT 1")
        first_user = await result.fetchone()
        author_id = first_user["pk_user"] if first_user else uuid.uuid4()

    # Determine published_at based on status
    published_at = now if input.get("status") == "published" else None

    await db.execute(
        """
        INSERT INTO tb_post (pk_post, identifier, fk_author, title, content, excerpt,
                            status, featured, published_at, seo_metadata, custom_fields,
                            created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        """,
        post_id,
        slug,
        author_id,
        input["title"],
        input["content"],
        input.get("excerpt", input["content"][:200]),
        input.get("status", "draft"),
        input.get("featured", False),
        published_at,
        input.get("seo_metadata", {}),
        input.get("custom_fields", {}),
        now,
        now,
    )

    return {
        "id": post_id,
        "title": input["title"],
        "slug": slug,
        "content": input["content"],
        "excerpt": input.get("excerpt", input["content"][:200]),
        "author_id": author_id,
        "status": input.get("status", "draft"),
        "featured": input.get("featured", False),
        "created_at": now,
        "updated_at": now,
        "published_at": published_at,
        "seo_metadata": input.get("seo_metadata", {}),
        "custom_fields": input.get("custom_fields", {}),
    }


@fraiseql.mutation
async def create_comment(info, input: dict):
    """Create comment in database."""
    db = info.context["db"]

    comment_id = uuid.uuid4()
    now = datetime.now()

    # Get author from context or use test user
    author_id = info.context.get("user_id")
    if not author_id:
        # For testing, use the first user from seed data
        result = await db.execute("SELECT pk_user FROM tb_user ORDER BY created_at ASC LIMIT 1")
        first_user = await result.fetchone()
        author_id = first_user["pk_user"] if first_user else uuid.uuid4()

    await db.execute(
        """
        INSERT INTO tb_comment (pk_comment, fk_post, fk_author, fk_parent, content, status,
                               created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        """,
        comment_id,
        input["post_id"],
        author_id,
        input.get("parent_id"),
        input["content"],
        "pending",
        now,
        now,
    )

    return {
        "id": comment_id,
        "post_id": input["post_id"],
        "author_id": author_id,
        "parent_id": input.get("parent_id"),
        "content": input["content"],
        "status": "pending",
        "created_at": now,
        "updated_at": now,
        "moderation_data": {},
    }


@fraiseql.mutation
async def create_tag(info, input: dict):
    """Create tag in database."""
    db = info.context["db"]

    tag_id = uuid.uuid4()
    now = datetime.now()
    slug = re.sub(r"[^a-zA-Z0-9]+", "-", input["name"].lower()).strip("-")

    await db.execute(
        """
        INSERT INTO tb_tag (pk_tag, identifier, fk_parent, name, description, color,
                           sort_order, is_active, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        """,
        tag_id,
        slug,
        input.get("parent_id"),
        input["name"],
        input.get("description"),
        input.get("color", "#6B7280"),
        input.get("sort_order", 0),
        True,
        now,
    )

    return {
        "id": tag_id,
        "name": input["name"],
        "slug": slug,
        "description": input.get("description"),
        "color": input.get("color", "#6B7280"),
        "parent_id": input.get("parent_id"),
        "sort_order": input.get("sort_order", 0),
        "is_active": True,
        "created_at": now,
    }


@fraiseql.mutation
async def update_post(info, id: UUID, input: dict):
    """Update post in database."""
    db = info.context["db"]
    now = datetime.now()

    # Get existing post
    result = await db.execute("SELECT * FROM tb_post WHERE pk_post = $1", id)
    existing_post = await result.fetchone()
    if not existing_post:
        raise Exception("Post not found")

    # Build update fields
    update_fields = []
    update_values = []
    param_count = 0

    if "title" in input:
        param_count += 1
        update_fields.append(f"title = ${param_count}")
        update_values.append(input["title"])

    if "content" in input:
        param_count += 1
        update_fields.append(f"content = ${param_count}")
        update_values.append(input["content"])

    if "excerpt" in input:
        param_count += 1
        update_fields.append(f"excerpt = ${param_count}")
        update_values.append(input["excerpt"])

    if "status" in input:
        param_count += 1
        update_fields.append(f"status = ${param_count}")
        update_values.append(input["status"])

        # Set published_at if publishing
        if input["status"] == "published" and not existing_post["published_at"]:
            param_count += 1
            update_fields.append(f"published_at = ${param_count}")
            update_values.append(now)

    if "featured" in input:
        param_count += 1
        update_fields.append(f"featured = ${param_count}")
        update_values.append(input["featured"])

    if "seo_metadata" in input:
        param_count += 1
        update_fields.append(f"seo_metadata = ${param_count}")
        update_values.append(input["seo_metadata"])

    if "custom_fields" in input:
        param_count += 1
        update_fields.append(f"custom_fields = ${param_count}")
        update_values.append(input["custom_fields"])

    # Always update updated_at
    param_count += 1
    update_fields.append(f"updated_at = ${param_count}")
    update_values.append(now)

    # Add WHERE clause
    param_count += 1
    update_values.append(id)

    if update_fields:
        await db.execute(
            f"UPDATE tb_post SET {', '.join(update_fields)} WHERE pk_post = ${param_count}",
            *update_values,
        )

    # Return updated post data
    result = await db.execute(
        """
        SELECT id, title, slug, content, excerpt, author_id, status, featured,
               created_at, updated_at, published_at, seo_metadata, custom_fields
        FROM v_post WHERE id = $1
        """,
        id,
    )
    updated_post = await result.fetchone()
    return dict(updated_post)


@fraiseql.mutation
async def publish_post(info, id: UUID):
    """Publish post in database."""
    return await update_post(info, id, {"status": "published"})


@fraiseql.mutation
async def update_comment(info, id: UUID, input: dict):
    """Update comment in database."""
    db = info.context["db"]
    now = datetime.now()

    # Build update fields
    update_fields = []
    update_values = []
    param_count = 0

    if "content" in input:
        param_count += 1
        update_fields.append(f"content = ${param_count}")
        update_values.append(input["content"])

    if "status" in input:
        param_count += 1
        update_fields.append(f"status = ${param_count}")
        update_values.append(input["status"])

        # Add moderation data if approving/rejecting
        if input["status"] in ["approved", "rejected"]:
            param_count += 1
            update_fields.append(f"moderation_data = ${param_count}")
            moderation_data = {
                "moderated_at": now.isoformat(),
                "moderated_by": info.context.get("user_id", "system"),
                "reason": f"Marked as {input['status']}",
            }
            update_values.append(moderation_data)

    # Always update updated_at
    param_count += 1
    update_fields.append(f"updated_at = ${param_count}")
    update_values.append(now)

    # Add WHERE clause
    param_count += 1
    update_values.append(id)

    if update_fields:
        await db.execute(
            f"UPDATE tb_comment SET {', '.join(update_fields)} WHERE pk_comment = ${param_count}",
            *update_values,
        )

    # Return updated comment data
    result = await db.execute(
        """
        SELECT id, post_id, author_id, parent_id, content, status,
               created_at, updated_at, moderation_data
        FROM v_comment WHERE id = $1
        """,
        id,
    )
    updated_comment = await result.fetchone()
    return dict(updated_comment)
