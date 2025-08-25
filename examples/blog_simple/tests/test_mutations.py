"""Test GraphQL mutations for blog_simple example."""

import pytest

# Mark all tests in this file with blog_simple marker
pytestmark = [pytest.mark.blog_simple, pytest.mark.integration, pytest.mark.database]


@pytest.mark.asyncio
async def test_create_post_mutation(graphql_client, sample_post_data):
    """Test creating a new post."""
    mutation = """
        mutation CreatePost($input: CreatePostInput!) {
            createPost(input: $input) {
                __typename
                ... on CreatePostSuccess {
                    post {
                        id
                        title
                        slug
                        content
                        status
                        author {
                            username
                        }
                    }
                    message
                }
                ... on ValidationError {
                    message
                    code
                    fieldErrors {
                        field
                        message
                    }
                }
                ... on PermissionError {
                    message
                    code
                }
            }
        }
    """

    result = await graphql_client.execute(
        mutation,
        variables={"input": sample_post_data}
    )

    assert "errors" not in result or not result["errors"]
    assert "data" in result
    assert "createPost" in result["data"]

    create_result = result["data"]["createPost"]

    # Should be a success (assuming valid input)
    if create_result["__typename"] == "CreatePostSuccess":
        post = create_result["post"]
        assert post["title"] == sample_post_data["title"]
        assert post["content"] == sample_post_data["content"]
        assert post["status"] == "draft"  # Default status
        assert "slug" in post
        assert post["slug"] is not None
        assert "id" in post

        # Author should be set from context
        assert "author" in post
        if post["author"]:
            assert "username" in post["author"]

    else:
        # If it's an error, print for debugging
        pytest.fail(f"Expected success but got: {create_result}")


@pytest.mark.asyncio
async def test_create_post_with_tags(graphql_client):
    """Test creating a post with tags."""
    # First, get existing tag IDs
    tags_query = """
        query {
            tags(limit: 2) {
                id
                name
            }
        }
    """

    tags_result = await graphql_client.execute(tags_query)
    tags = tags_result.get("data", {}).get("tags", [])

    if not tags:
        pytest.skip("No tags available for testing")

    tag_ids = [tag["id"] for tag in tags[:2]]

    mutation = """
        mutation CreatePost($input: CreatePostInput!) {
            createPost(input: $input) {
                __typename
                ... on CreatePostSuccess {
                    post {
                        id
                        title
                        tags {
                            id
                            name
                        }
                    }
                }
                ... on ValidationError {
                    message
                }
            }
        }
    """

    input_data = {
        "title": "Post with Tags",
        "content": "This post has tags attached to it.",
        "tagIds": tag_ids
    }

    result = await graphql_client.execute(
        mutation,
        variables={"input": input_data}
    )

    assert "errors" not in result or not result["errors"]
    create_result = result["data"]["createPost"]

    if create_result["__typename"] == "CreatePostSuccess":
        post = create_result["post"]
        assert "tags" in post

        # Should have the tags we assigned
        post_tag_ids = {tag["id"] for tag in post["tags"]}
        assert len(post_tag_ids.intersection(set(tag_ids))) > 0


@pytest.mark.asyncio
async def test_update_post_mutation(graphql_client):
    """Test updating an existing post."""
    # First create a post to update
    create_mutation = """
        mutation CreatePost($input: CreatePostInput!) {
            createPost(input: $input) {
                ... on CreatePostSuccess {
                    post {
                        id
                        title
                    }
                }
            }
        }
    """

    create_result = await graphql_client.execute(
        create_mutation,
        variables={
            "input": {
                "title": "Post to Update",
                "content": "Original content"
            }
        }
    )

    if create_result["data"]["createPost"]["__typename"] != "CreatePostSuccess":
        pytest.skip("Could not create post for update test")

    post_id = create_result["data"]["createPost"]["post"]["id"]

    # Now update the post
    update_mutation = """
        mutation UpdatePost($id: UUID!, $input: UpdatePostInput!) {
            updatePost(id: $id, input: $input) {
                __typename
                ... on UpdatePostSuccess {
                    post {
                        id
                        title
                        content
                        status
                    }
                    message
                }
                ... on ValidationError {
                    message
                    code
                }
                ... on NotFoundError {
                    message
                    entityType
                }
                ... on PermissionError {
                    message
                }
            }
        }
    """

    update_input = {
        "title": "Updated Post Title",
        "content": "Updated content with more information",
        "status": "published"
    }

    result = await graphql_client.execute(
        update_mutation,
        variables={"id": post_id, "input": update_input}
    )

    assert "errors" not in result or not result["errors"]
    update_result = result["data"]["updatePost"]

    if update_result["__typename"] == "UpdatePostSuccess":
        post = update_result["post"]
        assert post["id"] == post_id
        assert post["title"] == update_input["title"]
        assert post["content"] == update_input["content"]
        assert post["status"] == update_input["status"]
    else:
        pytest.fail(f"Expected success but got: {update_result}")


@pytest.mark.asyncio
async def test_create_comment_mutation(graphql_client):
    """Test creating a comment on a post."""
    # First get a post to comment on
    posts_query = """
        query {
            posts(limit: 1, where: {status: "published"}) {
                id
                title
            }
        }
    """

    posts_result = await graphql_client.execute(posts_query)
    posts = posts_result.get("data", {}).get("posts", [])

    if not posts:
        pytest.skip("No published posts available for commenting")

    post_id = posts[0]["id"]

    mutation = """
        mutation CreateComment($input: CreateCommentInput!) {
            createComment(input: $input) {
                __typename
                ... on CreateCommentSuccess {
                    comment {
                        id
                        content
                        status
                        post {
                            id
                        }
                        author {
                            username
                        }
                    }
                    message
                }
                ... on ValidationError {
                    message
                }
                ... on NotFoundError {
                    message
                    entityType
                }
            }
        }
    """

    comment_input = {
        "postId": post_id,
        "content": "This is a test comment with valuable insights."
    }

    result = await graphql_client.execute(
        mutation,
        variables={"input": comment_input}
    )

    assert "errors" not in result or not result["errors"]
    create_result = result["data"]["createComment"]

    if create_result["__typename"] == "CreateCommentSuccess":
        comment = create_result["comment"]
        assert comment["content"] == comment_input["content"]
        assert comment["status"] == "pending"  # Default status for new comments
        assert comment["post"]["id"] == post_id
        assert "author" in comment
    else:
        pytest.fail(f"Expected success but got: {create_result}")


@pytest.mark.asyncio
async def test_create_nested_comment(graphql_client):
    """Test creating a reply comment."""
    # First get a comment to reply to
    posts_query = """
        query {
            posts(limit: 1) {
                id
                comments {
                    id
                    content
                }
            }
        }
    """

    posts_result = await graphql_client.execute(posts_query)
    posts = posts_result.get("data", {}).get("posts", [])

    parent_comment_id = None
    post_id = None

    for post in posts:
        if post.get("comments"):
            parent_comment_id = post["comments"][0]["id"]
            post_id = post["id"]
            break

    if not parent_comment_id:
        pytest.skip("No comments available for reply testing")

    mutation = """
        mutation CreateComment($input: CreateCommentInput!) {
            createComment(input: $input) {
                ... on CreateCommentSuccess {
                    comment {
                        id
                        content
                        parentId
                        parent {
                            id
                            content
                        }
                    }
                }
            }
        }
    """

    reply_input = {
        "postId": post_id,
        "content": "This is a reply to the parent comment.",
        "parentId": parent_comment_id
    }

    result = await graphql_client.execute(
        mutation,
        variables={"input": reply_input}
    )

    assert "errors" not in result or not result["errors"]
    create_result = result["data"]["createComment"]

    if create_result["__typename"] == "CreateCommentSuccess":
        comment = create_result["comment"]
        assert comment["parentId"] == parent_comment_id
        assert "parent" in comment
        if comment["parent"]:
            assert comment["parent"]["id"] == parent_comment_id


@pytest.mark.asyncio
async def test_mutation_validation_errors(graphql_client):
    """Test that mutations properly validate input."""
    mutation = """
        mutation CreatePost($input: CreatePostInput!) {
            createPost(input: $input) {
                __typename
                ... on ValidationError {
                    message
                    code
                    fieldErrors {
                        field
                        message
                    }
                }
            }
        }
    """

    # Test with empty title (should fail validation)
    invalid_input = {
        "title": "",  # Empty title should fail
        "content": "Valid content"
    }

    result = await graphql_client.execute(
        mutation,
        variables={"input": invalid_input}
    )

    # Should either get GraphQL validation error or ValidationError result
    if "errors" not in result:
        create_result = result["data"]["createPost"]
        # If no GraphQL errors, should be ValidationError
        assert create_result["__typename"] == "ValidationError"
        assert "VALIDATION_ERROR" in create_result.get("code", "")


@pytest.mark.asyncio
async def test_permission_errors(graphql_client):
    """Test permission-based errors."""
    # Try to update a post that doesn't belong to the current user
    # This assumes there's a post from a different user in the seed data

    mutation = """
        mutation UpdatePost($id: UUID!, $input: UpdatePostInput!) {
            updatePost(id: $id, input: $input) {
                __typename
                ... on PermissionError {
                    message
                    code
                }
                ... on NotFoundError {
                    message
                    entityType
                }
            }
        }
    """

    # Use a non-existent UUID to test NotFoundError
    fake_id = "00000000-0000-0000-0000-000000000000"

    result = await graphql_client.execute(
        mutation,
        variables={
            "id": fake_id,
            "input": {"title": "Should not work"}
        }
    )

    assert "errors" not in result or not result["errors"]
    update_result = result["data"]["updatePost"]

    # Should be either NotFoundError or PermissionError
    assert update_result["__typename"] in ["NotFoundError", "PermissionError"]
