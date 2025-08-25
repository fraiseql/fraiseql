"""Test GraphQL queries for blog_simple example."""

import pytest

# Mark all tests in this file with blog_simple marker
pytestmark = [pytest.mark.blog_simple, pytest.mark.integration, pytest.mark.database]


@pytest.mark.asyncio
async def test_query_posts(blog_simple_graphql_client):
    """Test querying posts with basic filtering."""
    query = """
        query GetPosts($limit: Int, $where: PostWhereInput) {
            posts(limit: $limit, where: $where) {
                id
                title
                slug
                excerpt
                status
                publishedAt
                createdAt
                author {
                    id
                    username
                    fullName
                }
                tags {
                    id
                    name
                    color
                }
                commentCount
            }
        }
    """

    result = await blog_simple_graphql_client.execute(
        query,
        variables={"limit": 10, "where": {"status": "published"}}
    )

    # Should not have GraphQL errors
    assert "errors" not in result or not result["errors"]

    # Should have posts data
    assert "data" in result
    assert "posts" in result["data"]

    posts = result["data"]["posts"]
    assert isinstance(posts, list)

    # Check structure of posts if any exist
    if posts:
        post = posts[0]
        assert "id" in post
        assert "title" in post
        assert "slug" in post
        assert "status" in post
        assert post["status"] == "published"  # Due to filter

        # Check author structure
        assert "author" in post
        if post["author"]:
            assert "id" in post["author"]
            assert "username" in post["author"]

        # Check tags structure
        assert "tags" in post
        assert isinstance(post["tags"], list)


@pytest.mark.asyncio
async def test_query_single_post_by_id(blog_simple_graphql_client):
    """Test querying a single post by ID."""
    # First get a post ID
    posts_query = """
        query {
            posts(limit: 1) {
                id
            }
        }
    """

    posts_result = await blog_simple_graphql_client.execute(posts_query)

    if not posts_result.get("data", {}).get("posts"):
        pytest.skip("No posts available for testing")

    post_id = posts_result["data"]["posts"][0]["id"]

    # Now query single post
    query = """
        query GetPost($id: UUID) {
            post(id: $id) {
                id
                title
                content
                author {
                    id
                    username
                }
                comments {
                    id
                    content
                    author {
                        username
                    }
                }
            }
        }
    """

    result = await blog_simple_graphql_client.execute(query, variables={"id": post_id})

    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "post" in result["data"]

    post = result["data"]["post"]
    if post:  # Post might not exist
        assert post["id"] == post_id
        assert "title" in post
        assert "content" in post


@pytest.mark.asyncio
async def test_query_single_post_by_slug(blog_simple_graphql_client):
    """Test querying a single post by slug."""
    query = """
        query GetPost($slug: String) {
            post(slug: $slug) {
                id
                title
                slug
                content
            }
        }
    """

    result = await blog_simple_graphql_client.execute(
        query,
        variables={"slug": "getting-started-with-fraiseql"}
    )

    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "post" in result["data"]

    post = result["data"]["post"]
    if post:  # Post might not exist
        assert post["slug"] == "getting-started-with-fraiseql"


@pytest.mark.asyncio
async def test_query_tags(blog_simple_graphql_client):
    """Test querying tags."""
    query = """
        query GetTags($limit: Int) {
            tags(limit: $limit) {
                id
                name
                slug
                color
                description
                postCount
            }
        }
    """

    result = await blog_simple_graphql_client.execute(query, variables={"limit": 10})

    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "tags" in result["data"]

    tags = result["data"]["tags"]
    assert isinstance(tags, list)

    if tags:
        tag = tags[0]
        assert "id" in tag
        assert "name" in tag
        assert "slug" in tag
        assert "postCount" in tag


@pytest.mark.asyncio
async def test_query_users(blog_simple_graphql_client):
    """Test querying users."""
    query = """
        query GetUsers($limit: Int) {
            users(limit: $limit) {
                id
                username
                email
                role
                createdAt
                fullName
            }
        }
    """

    result = await blog_simple_graphql_client.execute(query, variables={"limit": 10})

    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "users" in result["data"]

    users = result["data"]["users"]
    assert isinstance(users, list)

    if users:
        user = users[0]
        assert "id" in user
        assert "username" in user
        assert "email" in user
        assert "role" in user


@pytest.mark.asyncio
async def test_query_posts_with_filtering(blog_simple_graphql_client):
    """Test querying posts with various filters."""
    query = """
        query GetPosts($where: PostWhereInput) {
            posts(where: $where) {
                id
                title
                status
                author {
                    username
                }
            }
        }
    """

    # Test filtering by status
    result = await blog_simple_graphql_client.execute(
        query,
        variables={"where": {"status": "published"}}
    )

    assert "errors" not in result or not result["errors"]
    posts = result["data"]["posts"]

    for post in posts:
        assert post["status"] == "published"


@pytest.mark.asyncio
async def test_query_posts_with_ordering(blog_simple_graphql_client):
    """Test querying posts with custom ordering."""
    query = """
        query GetPosts($orderBy: [PostOrderByInput!]) {
            posts(orderBy: $orderBy) {
                id
                title
                createdAt
            }
        }
    """

    result = await blog_simple_graphql_client.execute(
        query,
        variables={"orderBy": [{"field": "createdAt", "direction": "ASC"}]}
    )

    assert "errors" not in result or not result["errors"]
    posts = result["data"]["posts"]

    # Verify ordering (if multiple posts exist)
    if len(posts) > 1:
        for i in range(len(posts) - 1):
            # Earlier posts should have earlier or equal created dates
            assert posts[i]["createdAt"] <= posts[i + 1]["createdAt"]


@pytest.mark.asyncio
async def test_query_pagination(blog_simple_graphql_client):
    """Test query pagination."""
    query = """
        query GetPosts($limit: Int, $offset: Int) {
            posts(limit: $limit, offset: $offset) {
                id
                title
            }
        }
    """

    # First page
    result1 = await graphql_client.execute(
        query,
        variables={"limit": 2, "offset": 0}
    )

    # Second page
    result2 = await graphql_client.execute(
        query,
        variables={"limit": 2, "offset": 2}
    )

    assert "errors" not in result1 or not result1["errors"]
    assert "errors" not in result2 or not result2["errors"]

    posts1 = result1["data"]["posts"]
    posts2 = result2["data"]["posts"]

    # Ensure we get different posts (if enough posts exist)
    if len(posts1) > 0 and len(posts2) > 0:
        post_ids_1 = {post["id"] for post in posts1}
        post_ids_2 = {post["id"] for post in posts2}
        assert post_ids_1.isdisjoint(post_ids_2), "Pagination should return different posts"


@pytest.mark.asyncio
async def test_nested_field_resolution(blog_simple_graphql_client):
    """Test that nested fields are properly resolved."""
    query = """
        query GetPostWithNested {
            posts(limit: 1) {
                id
                title
                author {
                    id
                    username
                    posts {
                        id
                        title
                    }
                }
                tags {
                    id
                    name
                    posts {
                        id
                        title
                    }
                }
            }
        }
    """

    result = await blog_simple_graphql_client.execute(query)

    assert "errors" not in result or not result["errors"]
    posts = result["data"]["posts"]

    if posts:
        post = posts[0]

        # Check author nested resolution
        if post.get("author"):
            author = post["author"]
            assert "posts" in author
            assert isinstance(author["posts"], list)

        # Check tags nested resolution
        if post.get("tags"):
            for tag in post["tags"]:
                assert "posts" in tag
                assert isinstance(tag["posts"], list)
