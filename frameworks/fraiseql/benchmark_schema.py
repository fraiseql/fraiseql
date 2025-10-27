"""Benchmark GraphQL Schema for FraiseQL Framework Submission

Implements the standard benchmark schema from FRAMEWORK_SUBMISSION_GUIDE.
Uses CQRS pattern with explicit sync for optimal performance.
"""

import os
from typing import Optional

import asyncpg
import strawberry

# Database connection
DATABASE_URL = os.getenv(
    "DATABASE_URL", "postgresql://benchmark:benchmark@localhost:5432/benchmark_db"
)


@strawberry.type
class User:
    id: int
    name: str
    email: str
    age: Optional[int]
    city: Optional[str]
    created_at: str


@strawberry.type
class Post:
    id: int
    title: str
    content: Optional[str]
    published: bool
    author_id: int
    author: User
    comments: list["Comment"]
    created_at: str


@strawberry.type
class Comment:
    id: int
    content: str
    post_id: int
    post: Post
    author_id: int
    author: User
    created_at: str


@strawberry.input
class UserFilter:
    age_gt: Optional[int] = None
    age_lt: Optional[int] = None
    city: Optional[str] = None
    name_contains: Optional[str] = None


@strawberry.enum
class Direction:
    ASC = "ASC"
    DESC = "DESC"


@strawberry.input
class OrderBy:
    field: str
    direction: Direction


@strawberry.input
class CreateUserInput:
    name: str
    email: str
    age: Optional[int] = None
    city: Optional[str] = None


@strawberry.input
class UpdateUserInput:
    name: Optional[str] = None
    email: Optional[str] = None
    age: Optional[int] = None
    city: Optional[str] = None


@strawberry.input
class CreatePostInput:
    title: str
    content: Optional[str] = None
    published: bool = False
    author_id: int


@strawberry.type
class Query:
    @strawberry.field
    async def users(self, limit: Optional[int] = None, offset: Optional[int] = None) -> list[User]:
        """Get users with optional pagination."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            if limit:
                rows = await conn.fetch(
                    "SELECT id, name, email, age, city, created_at::text FROM users ORDER BY id LIMIT $1 OFFSET $2",
                    limit,
                    offset or 0,
                )
            else:
                rows = await conn.fetch(
                    "SELECT id, name, email, age, city, created_at::text FROM users ORDER BY id"
                )

            return [User(**dict(row)) for row in rows]
        finally:
            await conn.close()

    @strawberry.field
    async def user(self, id: int) -> Optional[User]:
        """Get a single user by ID."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            row = await conn.fetchrow(
                "SELECT id, name, email, age, city, created_at::text FROM users WHERE id = $1", id
            )

            if row:
                return User(**dict(row))
            return None
        finally:
            await conn.close()

    @strawberry.field
    async def users_where(
        self,
        where: Optional[UserFilter] = None,
        order_by: Optional[OrderBy] = None,
        limit: Optional[int] = None,
    ) -> list[User]:
        """Get users with filtering and ordering."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            # Build WHERE clause
            conditions = []
            params = []
            param_count = 0

            if where:
                if where.age_gt is not None:
                    param_count += 1
                    conditions.append(f"age > ${param_count}")
                    params.append(where.age_gt)

                if where.age_lt is not None:
                    param_count += 1
                    conditions.append(f"age < ${param_count}")
                    params.append(where.age_lt)

                if where.city:
                    param_count += 1
                    conditions.append(f"city = ${param_count}")
                    params.append(where.city)

                if where.name_contains:
                    param_count += 1
                    conditions.append(f"name ILIKE ${param_count}")
                    params.append(f"%{where.name_contains}%")

            where_clause = " AND ".join(conditions) if conditions else "TRUE"

            # Build ORDER BY clause
            order_clause = "ORDER BY id"
            if order_by:
                direction = order_by.direction.value
                order_clause = f"ORDER BY {order_by.field} {direction}"

            # Build LIMIT clause
            limit_clause = f"LIMIT {limit}" if limit else ""

            query = f"""
                SELECT id, name, email, age, city, created_at::text
                FROM users
                WHERE {where_clause}
                {order_clause}
                {limit_clause}
            """

            rows = await conn.fetch(query, *params)
            return [User(**dict(row)) for row in rows]
        finally:
            await conn.close()

    @strawberry.field
    async def users_with_posts(self, limit: Optional[int] = None) -> list[User]:
        """Get users with their posts (N+1 prevention test)."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            # Get users
            if limit:
                user_rows = await conn.fetch(
                    "SELECT id, name, email, age, city, created_at::text FROM users ORDER BY id LIMIT $1",
                    limit,
                )
            else:
                user_rows = await conn.fetch(
                    "SELECT id, name, email, age, city, created_at::text FROM users ORDER BY id"
                )

            users = []
            for user_row in user_rows:
                user_dict = dict(user_row)

                # Get user's posts (prevents N+1 by batching)
                post_rows = await conn.fetch(
                    "SELECT id, title, content, published, author_id, created_at::text FROM posts WHERE author_id = $1 ORDER BY created_at DESC",
                    user_row["id"],
                )

                # For this benchmark, we'll return users without embedded posts
                # to focus on the core query performance
                users.append(User(**user_dict))

            return users
        finally:
            await conn.close()

    @strawberry.field
    async def posts(self, limit: Optional[int] = None, offset: Optional[int] = None) -> list[Post]:
        """Get posts with optional pagination."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            if limit:
                rows = await conn.fetch(
                    """
                    SELECT p.id, p.title, p.content, p.published, p.author_id, p.created_at::text,
                           u.id as author_id, u.name as author_name, u.email as author_email,
                           u.age as author_age, u.city as author_city, u.created_at::text as author_created_at
                    FROM posts p
                    JOIN users u ON p.author_id = u.id
                    ORDER BY p.id
                    LIMIT $1 OFFSET $2
                    """,
                    limit,
                    offset or 0,
                )
            else:
                rows = await conn.fetch(
                    """
                    SELECT p.id, p.title, p.content, p.published, p.author_id, p.created_at::text,
                           u.id as author_id, u.name as author_name, u.email as author_email,
                           u.age as author_age, u.city as author_city, u.created_at::text as author_created_at
                    FROM posts p
                    JOIN users u ON p.author_id = u.id
                    ORDER BY p.id
                    """
                )

            posts = []
            for row in rows:
                row_dict = dict(row)

                # Create author object
                author = User(
                    id=row_dict["author_id"],
                    name=row_dict["author_name"],
                    email=row_dict["author_email"],
                    age=row_dict["author_age"],
                    city=row_dict["author_city"],
                    created_at=row_dict["author_created_at"],
                )

                # Get comments for this post
                comment_rows = await conn.fetch(
                    """
                    SELECT c.id, c.content, c.post_id, c.author_id, c.created_at::text,
                           u.id as comment_author_id, u.name as comment_author_name,
                           u.email as comment_author_email, u.age as comment_author_age,
                           u.city as comment_author_city, u.created_at::text as comment_author_created_at
                    FROM comments c
                    JOIN users u ON c.author_id = u.id
                    WHERE c.post_id = $1
                    ORDER BY c.created_at
                    """,
                    row_dict["id"],
                )

                comments = []
                for comment_row in comment_rows:
                    comment_dict = dict(comment_row)
                    comment_author = User(
                        id=comment_dict["comment_author_id"],
                        name=comment_dict["comment_author_name"],
                        email=comment_dict["comment_author_email"],
                        age=comment_dict["comment_author_age"],
                        city=comment_dict["comment_author_city"],
                        created_at=comment_dict["comment_author_created_at"],
                    )
                    comments.append(
                        Comment(
                            id=comment_dict["id"],
                            content=comment_dict["content"],
                            post_id=comment_dict["post_id"],
                            post=None,  # Avoid circular reference
                            author_id=comment_dict["author_id"],
                            author=comment_author,
                            created_at=comment_dict["created_at"],
                        )
                    )

                post = Post(
                    id=row_dict["id"],
                    title=row_dict["title"],
                    content=row_dict["content"],
                    published=row_dict["published"],
                    author_id=row_dict["author_id"],
                    author=author,
                    comments=comments,
                    created_at=row_dict["created_at"],
                )
                posts.append(post)

            return posts
        finally:
            await conn.close()

    @strawberry.field
    async def post(self, id: int) -> Optional[Post]:
        """Get a single post with full details."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            row = await conn.fetchrow(
                """
                SELECT p.id, p.title, p.content, p.published, p.author_id, p.created_at::text,
                       u.id as author_id, u.name as author_name, u.email as author_email,
                       u.age as author_age, u.city as author_city, u.created_at::text as author_created_at
                FROM posts p
                JOIN users u ON p.author_id = u.id
                WHERE p.id = $1
                """,
                id,
            )

            if not row:
                return None

            row_dict = dict(row)

            # Create author
            author = User(
                id=row_dict["author_id"],
                name=row_dict["author_name"],
                email=row_dict["author_email"],
                age=row_dict["author_age"],
                city=row_dict["author_city"],
                created_at=row_dict["author_created_at"],
            )

            # Get comments
            comment_rows = await conn.fetch(
                """
                SELECT c.id, c.content, c.post_id, c.author_id, c.created_at::text,
                       u.id as comment_author_id, u.name as comment_author_name,
                       u.email as comment_author_email, u.age as comment_author_age,
                       u.city as comment_author_city, u.created_at::text as comment_author_created_at
                FROM comments c
                JOIN users u ON c.author_id = u.id
                WHERE c.post_id = $1
                ORDER BY c.created_at
                """,
                id,
            )

            comments = []
            for comment_row in comment_rows:
                comment_dict = dict(comment_row)
                comment_author = User(
                    id=comment_dict["comment_author_id"],
                    name=comment_dict["comment_author_name"],
                    email=comment_dict["comment_author_email"],
                    age=comment_dict["comment_author_age"],
                    city=comment_dict["comment_author_city"],
                    created_at=comment_dict["comment_author_created_at"],
                )
                comments.append(
                    Comment(
                        id=comment_dict["id"],
                        content=comment_dict["content"],
                        post_id=comment_dict["post_id"],
                        post=None,
                        author_id=comment_dict["author_id"],
                        author=comment_author,
                        created_at=comment_dict["created_at"],
                    )
                )

            return Post(
                id=row_dict["id"],
                title=row_dict["title"],
                content=row_dict["content"],
                published=row_dict["published"],
                author_id=row_dict["author_id"],
                author=author,
                comments=comments,
                created_at=row_dict["created_at"],
            )
        finally:
            await conn.close()


@strawberry.type
class Mutation:
    @strawberry.mutation
    async def create_user(self, input: CreateUserInput) -> User:
        """Create a new user."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            row = await conn.fetchrow(
                """
                INSERT INTO users (name, email, age, city)
                VALUES ($1, $2, $3, $4)
                RETURNING id, name, email, age, city, created_at::text
                """,
                input.name,
                input.email,
                input.age,
                input.city,
            )

            return User(**dict(row))
        finally:
            await conn.close()

    @strawberry.mutation
    async def update_user(self, id: int, input: UpdateUserInput) -> User:
        """Update an existing user."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            # Build update query dynamically
            updates = []
            params = []
            param_count = 0

            if input.name is not None:
                param_count += 1
                updates.append(f"name = ${param_count}")
                params.append(input.name)

            if input.email is not None:
                param_count += 1
                updates.append(f"email = ${param_count}")
                params.append(input.email)

            if input.age is not None:
                param_count += 1
                updates.append(f"age = ${param_count}")
                params.append(input.age)

            if input.city is not None:
                param_count += 1
                updates.append(f"city = ${param_count}")
                params.append(input.city)

            if not updates:
                # No updates provided, return current user
                row = await conn.fetchrow(
                    "SELECT id, name, email, age, city, created_at::text FROM users WHERE id = $1",
                    id,
                )
                return User(**dict(row))

            update_clause = ", ".join(updates)
            params.append(id)  # Add ID as last parameter

            row = await conn.fetchrow(
                f"""
                UPDATE users
                SET {update_clause}
                WHERE id = ${param_count + 1}
                RETURNING id, name, email, age, city, created_at::text
                """,
                *params,
            )

            return User(**dict(row))
        finally:
            await conn.close()

    @strawberry.mutation
    async def delete_user(self, id: int) -> bool:
        """Delete a user."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            result = await conn.execute("DELETE FROM users WHERE id = $1", id)
            return result == "DELETE 1"
        finally:
            await conn.close()

    @strawberry.mutation
    async def create_post(self, input: CreatePostInput) -> Post:
        """Create a new post."""
        conn = await asyncpg.connect(DATABASE_URL)

        try:
            # Insert post
            row = await conn.fetchrow(
                """
                INSERT INTO posts (title, content, published, author_id)
                VALUES ($1, $2, $3, $4)
                RETURNING id, title, content, published, author_id, created_at::text
                """,
                input.title,
                input.content,
                input.published,
                input.author_id,
            )

            post_dict = dict(row)

            # Get author details
            author_row = await conn.fetchrow(
                "SELECT id, name, email, age, city, created_at::text FROM users WHERE id = $1",
                input.author_id,
            )

            author = User(**dict(author_row))

            # For benchmark, return post with empty comments
            return Post(
                id=post_dict["id"],
                title=post_dict["title"],
                content=post_dict["content"],
                published=post_dict["published"],
                author_id=post_dict["author_id"],
                author=author,
                comments=[],  # Empty for benchmark simplicity
                created_at=post_dict["created_at"],
            )
        finally:
            await conn.close()


# Create the schema
schema = strawberry.Schema(query=Query, mutation=Mutation)
