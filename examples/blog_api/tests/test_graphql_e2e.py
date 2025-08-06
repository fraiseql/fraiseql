"""End-to-end GraphQL API tests."""

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
class TestGraphQLEndToEnd:
    """Test the GraphQL API end-to-end with real HTTP requests."""

    async def _graphql_request(
        self,
        client: AsyncClient,
        query: str,
        variables: dict | None = None,
        headers: dict | None = None,
    ):
        """Make a GraphQL request and return the result."""
        response = await client.post(
            "/graphql",
            json={"query": query, "variables": variables or {}},
            headers=headers or {"Content-Type": "application/json"},
        )
        assert response.status_code == 200
        return response.json()

    async def test_create_user_mutation(self, async_client: AsyncClient, clean_db):
        """Test creating a user via GraphQL."""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    ... on CreateUserSuccess {
                        user {
                            id
                            email
                            name
                            bio
                            createdAt
                            isActive
                            roles
                        }
                        message
                    }
                    ... on CreateUserError {
                        message
                        code
                        fieldErrors
                    }
                }
            }
        """

        variables = {
            "input": {
                "email": "graphql@example.com",
                "name": "GraphQL User",
                "password": "secure123",
                "bio": "Created via GraphQL",
            },
        }

        result = await self._graphql_request(async_client, mutation, variables)

        assert "errors" not in result
        data = result["data"]["createUser"]
        assert "user" in data
        assert data["user"]["email"] == "graphql@example.com"
        assert data["user"]["name"] == "GraphQL User"
        assert data["user"]["bio"] == "Created via GraphQL"
        assert data["user"]["isActive"] is True
        assert data["user"]["roles"] == ["user"]
        assert data["message"] == "User created successfully"

    async def test_create_user_duplicate_email(
        self, async_client: AsyncClient, test_user, clean_db,
    ):
        """Test creating user with duplicate email."""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    ... on CreateUserSuccess {
                        user { id }
                    }
                    ... on CreateUserError {
                        message
                        code
                        fieldErrors
                    }
                }
            }
        """

        variables = {
            "input": {
                "email": test_user.email,
                "name": "Duplicate",
                "password": "password",
            },
        }

        result = await self._graphql_request(async_client, mutation, variables)

        assert "errors" not in result
        data = result["data"]["createUser"]
        assert "code" in data
        assert data["code"] == "EMAIL_EXISTS"
        assert "email" in data["fieldErrors"]

    async def test_query_user(self, async_client: AsyncClient, test_user, clean_db):
        """Test querying a user by ID."""
        query = """
            query GetUser($id: UUID!) {
                getUser(id: $id) {
                    id
                    email
                    name
                    bio
                    avatarUrl
                    createdAt
                }
            }
        """

        variables = {"id": str(test_user.id)}

        result = await self._graphql_request(async_client, query, variables)

        assert "errors" not in result
        user = result["data"]["getUser"]
        assert user["id"] == str(test_user.id)
        assert user["email"] == test_user.email
        assert user["name"] == test_user.name

    async def test_create_and_query_post(
        self, async_client: AsyncClient, auth_headers, clean_db,
    ):
        """Test creating a post and then querying it."""
        # First create a post
        create_mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    ... on CreatePostSuccess {
                        post {
                            id
                            title
                            slug
                            content
                            excerpt
                            tags
                            isPublished
                            publishedAt
                            author {
                                name
                                email
                            }
                        }
                    }
                    ... on CreatePostError {
                        message
                        code
                    }
                }
            }
        """

        create_variables = {
            "input": {
                "title": "GraphQL Test Post",
                "content": "This post was created via GraphQL",
                "excerpt": "GraphQL test",
                "tags": ["graphql", "test"],
                "isPublished": True,
            },
        }

        create_result = await self._graphql_request(
            async_client, create_mutation, create_variables, auth_headers,
        )

        assert "errors" not in create_result
        post_data = create_result["data"]["createPost"]["post"]
        post_id = post_data["id"]
        assert post_data["title"] == "GraphQL Test Post"
        assert post_data["slug"] == "graphql-test-post"
        assert post_data["isPublished"] is True
        assert post_data["publishedAt"] is not None

        # Now query the post
        query = """
            query GetPost($id: UUID!) {
                getPost(id: $id) {
                    id
                    title
                    content
                    viewCount
                    comments {
                        id
                        content
                        author {
                            name
                        }
                    }
                }
            }
        """

        query_variables = {"id": post_id}

        query_result = await self._graphql_request(async_client, query, query_variables)

        assert "errors" not in query_result
        queried_post = query_result["data"]["getPost"]
        assert queried_post["id"] == post_id
        assert queried_post["title"] == "GraphQL Test Post"
        assert queried_post["viewCount"] == 1  # Incremented by query
        assert queried_post["comments"] == []

    async def test_query_posts_with_filters(
        self, async_client: AsyncClient, test_user, create_test_post, clean_db,
    ):
        """Test querying posts with filters and pagination."""
        # Create test posts
        await create_test_post(
            title="Published Python Post", is_published=True, tags=["python"],
        )
        await create_test_post(
            title="Draft JavaScript Post", is_published=False, tags=["javascript"],
        )
        await create_test_post(
            title="Published Tutorial", is_published=True, tags=["tutorial"],
        )

        query = """
            query GetPosts($filters: PostFilters, $orderBy: PostOrderBy, $limit: Int, $offset: Int) {
                getPosts(filters: $filters, orderBy: $orderBy, limit: $limit, offset: $offset) {
                    id
                    title
                    tags
                    isPublished
                    author {
                        name
                    }
                }
            }
        """

        # Test filtering by published status
        variables = {
            "filters": {"isPublished": True},
            "orderBy": "CREATED_AT_DESC",
            "limit": 10,
        }

        result = await self._graphql_request(async_client, query, variables)

        assert "errors" not in result
        posts = result["data"]["getPosts"]
        assert len(posts) == 2
        assert all(p["isPublished"] for p in posts)

        # Test filtering by tags
        variables = {"filters": {"tagsContain": ["python"]}, "limit": 10}

        result = await self._graphql_request(async_client, query, variables)

        posts = result["data"]["getPosts"]
        assert len(posts) == 1
        assert "python" in posts[0]["tags"]

    async def test_update_post_mutation(
        self, async_client: AsyncClient, auth_headers, create_test_post, clean_db,
    ):
        """Test updating a post via GraphQL."""
        # Create a post first
        post = await create_test_post(
            title="Original Title", content="Original content",
        )

        mutation = """
            mutation UpdatePost($id: UUID!, $input: UpdatePostInput!) {
                updatePost(id: $id, input: $input) {
                    ... on UpdatePostSuccess {
                        post {
                            id
                            title
                            content
                            tags
                        }
                        updatedFields
                    }
                    ... on UpdatePostError {
                        message
                        code
                    }
                }
            }
        """

        variables = {
            "id": str(post.id),
            "input": {
                "title": "Updated Title",
                "content": "Updated content",
                "tags": ["updated", "graphql"],
            },
        }

        result = await self._graphql_request(
            async_client, mutation, variables, auth_headers,
        )

        assert "errors" not in result
        data = result["data"]["updatePost"]
        assert "post" in data
        assert data["post"]["title"] == "Updated Title"
        assert data["post"]["content"] == "Updated content"
        assert data["post"]["tags"] == ["updated", "graphql"]
        assert set(data["updatedFields"]) == {"title", "content", "tags"}

    async def test_create_comment_and_replies(
        self, async_client: AsyncClient, auth_headers, create_test_post, clean_db,
    ):
        """Test creating comments and nested replies."""
        # Create a post
        post = await create_test_post(title="Post for Comments")

        # Create a comment
        create_comment_mutation = """
            mutation CreateComment($input: CreateCommentInput!) {
                createComment(input: $input) {
                    id
                    content
                    author {
                        name
                    }
                    createdAt
                }
            }
        """

        comment_variables = {
            "input": {"postId": str(post.id), "content": "This is a top-level comment"},
        }

        comment_result = await self._graphql_request(
            async_client, create_comment_mutation, comment_variables, auth_headers,
        )

        assert "errors" not in comment_result
        comment = comment_result["data"]["createComment"]
        comment_id = comment["id"]
        assert comment["content"] == "This is a top-level comment"

        # Create a reply
        reply_variables = {
            "input": {
                "postId": str(post.id),
                "content": "This is a reply",
                "parentCommentId": comment_id,
            },
        }

        reply_result = await self._graphql_request(
            async_client, create_comment_mutation, reply_variables, auth_headers,
        )

        assert "errors" not in reply_result
        reply = reply_result["data"]["createComment"]
        assert reply["content"] == "This is a reply"

        # Query the post with comments
        query = """
            query GetPostWithComments($id: UUID!) {
                getPost(id: $id) {
                    title
                    comments {
                        id
                        content
                        replies {
                            id
                            content
                        }
                    }
                }
            }
        """

        post_result = await self._graphql_request(
            async_client, query, {"id": str(post.id)},
        )

        post_data = post_result["data"]["getPost"]
        assert len(post_data["comments"]) == 2

        # Find the parent comment
        parent_comment = next(
            c
            for c in post_data["comments"]
            if c["content"] == "This is a top-level comment"
        )
        assert len(parent_comment["replies"]) == 1
        assert parent_comment["replies"][0]["content"] == "This is a reply"

    async def test_me_query_authenticated(
        self, async_client: AsyncClient, auth_headers, test_user, clean_db,
    ):
        """Test the me query with authentication."""
        query = """
            query Me {
                me {
                    id
                    email
                    name
                    posts {
                        title
                    }
                }
            }
        """

        result = await self._graphql_request(async_client, query, headers=auth_headers)

        assert "errors" not in result
        me_data = result["data"]["me"]
        assert me_data["id"] == str(test_user.id)
        assert me_data["email"] == test_user.email
        assert me_data["name"] == test_user.name

    async def test_delete_post_as_admin(
        self, async_client: AsyncClient, admin_user, create_test_post, clean_db,
    ):
        """Test deleting a post as admin."""
        # Create a post
        post = await create_test_post(title="Post to Delete")

        # Create admin headers
        admin_headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer test-token-{admin_user.id}",
        }

        mutation = """
            mutation DeletePost($id: UUID!) {
                deletePost(id: $id)
            }
        """

        variables = {"id": str(post.id)}

        result = await self._graphql_request(
            async_client, mutation, variables, admin_headers,
        )

        assert "errors" not in result
        assert result["data"]["deletePost"] is True

        # Verify post is deleted
        query = """
            query GetPost($id: UUID!) {
                getPost(id: $id) {
                    id
                }
            }
        """

        verify_result = await self._graphql_request(async_client, query, variables)
        assert verify_result["data"]["getPost"] is None

    async def test_complex_nested_query(
        self,
        async_client: AsyncClient,
        test_user,
        create_test_post,
        create_test_comment,
        clean_db,
    ):
        """Test a complex nested query with multiple levels."""
        # Create test data
        post1 = await create_test_post(title="Post 1", tags=["graphql", "test"])
        await create_test_post(title="Post 2", tags=["tutorial"])

        comment1_id = await create_test_comment(str(post1.id), "Comment on post 1")
        await create_test_comment(
            str(post1.id), "Reply to comment", parent_id=comment1_id,
        )

        query = """
            query ComplexQuery($userId: UUID!) {
                getUser(id: $userId) {
                    name
                    email
                    posts {
                        id
                        title
                        tags
                        comments {
                            content
                            author {
                                name
                            }
                            replies {
                                content
                                author {
                                    email
                                }
                            }
                        }
                    }
                }
            }
        """

        variables = {"userId": str(test_user.id)}

        result = await self._graphql_request(async_client, query, variables)

        assert "errors" not in result
        user_data = result["data"]["getUser"]
        assert user_data["name"] == test_user.name
        assert len(user_data["posts"]) == 2

        # Find post with comments
        post_with_comments = next(
            p for p in user_data["posts"] if p["title"] == "Post 1"
        )
        assert len(post_with_comments["comments"]) == 2

        # Find comment with reply
        comment_with_reply = next(
            c
            for c in post_with_comments["comments"]
            if c["content"] == "Comment on post 1"
        )
        assert len(comment_with_reply["replies"]) == 1
        assert comment_with_reply["replies"][0]["content"] == "Reply to comment"
