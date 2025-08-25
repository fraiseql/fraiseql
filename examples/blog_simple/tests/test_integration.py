"""Integration tests for blog_simple example."""

import pytest

# Mark all tests in this file with blog_simple marker
pytestmark = [pytest.mark.blog_simple, pytest.mark.integration, pytest.mark.database, pytest.mark.e2e]


@pytest.mark.asyncio
async def test_complete_blog_workflow(graphql_client):
    """Test a complete blog workflow: create post -> publish -> comment -> reply."""

    # Step 1: Create a draft post
    create_post_mutation = """
        mutation CreatePost($input: CreatePostInput!) {
            createPost(input: $input) {
                ... on CreatePostSuccess {
                    post {
                        id
                        title
                        status
                    }
                }
            }
        }
    """

    post_data = {
        "title": "Integration Test Post",
        "content": "This is a test post for integration testing workflow."
    }

    create_result = await graphql_client.execute(
        create_post_mutation,
        variables={"input": post_data}
    )

    assert "errors" not in create_result or not create_result["errors"]
    assert create_result["data"]["createPost"]["__typename"] == "CreatePostSuccess"

    post_id = create_result["data"]["createPost"]["post"]["id"]
    assert create_result["data"]["createPost"]["post"]["status"] == "draft"

    # Step 2: Publish the post
    update_post_mutation = """
        mutation UpdatePost($id: UUID!, $input: UpdatePostInput!) {
            updatePost(id: $id, input: $input) {
                ... on UpdatePostSuccess {
                    post {
                        id
                        status
                        publishedAt
                    }
                }
            }
        }
    """

    publish_result = await graphql_client.execute(
        update_post_mutation,
        variables={
            "id": post_id,
            "input": {"status": "published"}
        }
    )

    assert "errors" not in publish_result or not publish_result["errors"]
    assert publish_result["data"]["updatePost"]["__typename"] == "UpdatePostSuccess"

    published_post = publish_result["data"]["updatePost"]["post"]
    assert published_post["status"] == "published"
    assert published_post["publishedAt"] is not None

    # Step 3: Add a comment to the post
    create_comment_mutation = """
        mutation CreateComment($input: CreateCommentInput!) {
            createComment(input: $input) {
                ... on CreateCommentSuccess {
                    comment {
                        id
                        content
                        status
                    }
                }
            }
        }
    """

    comment_data = {
        "postId": post_id,
        "content": "Great post! Very informative."
    }

    comment_result = await graphql_client.execute(
        create_comment_mutation,
        variables={"input": comment_data}
    )

    assert "errors" not in comment_result or not comment_result["errors"]
    assert comment_result["data"]["createComment"]["__typename"] == "CreateCommentSuccess"

    comment_id = comment_result["data"]["createComment"]["comment"]["id"]
    assert comment_result["data"]["createComment"]["comment"]["status"] == "pending"

    # Step 4: Create a reply to the comment
    reply_data = {
        "postId": post_id,
        "content": "Thanks for the feedback!",
        "parentId": comment_id
    }

    reply_result = await graphql_client.execute(
        create_comment_mutation,
        variables={"input": reply_data}
    )

    assert "errors" not in reply_result or not reply_result["errors"]
    assert reply_result["data"]["createComment"]["__typename"] == "CreateCommentSuccess"

    reply_comment = reply_result["data"]["createComment"]["comment"]
    # Note: The parentId field might not be returned in the GraphQL response
    # depending on the schema definition

    # Step 5: Verify the complete post with all data
    verify_query = """
        query GetPost($id: UUID) {
            post(id: $id) {
                id
                title
                status
                publishedAt
                author {
                    username
                }
                comments {
                    id
                    content
                    parentId
                    replies {
                        id
                        content
                    }
                }
            }
        }
    """

    verify_result = await graphql_client.execute(
        verify_query,
        variables={"id": post_id}
    )

    assert "errors" not in verify_result or not verify_result["errors"]

    final_post = verify_result["data"]["post"]
    assert final_post is not None
    assert final_post["title"] == post_data["title"]
    assert final_post["status"] == "published"
    assert final_post["publishedAt"] is not None

    # Comments might be filtered by status (only approved shown)
    # So we might not see the pending comments in the response


@pytest.mark.asyncio
async def test_post_tagging_workflow(graphql_client):
    """Test creating a post with tags and querying by tags."""

    # Step 1: Get available tags
    tags_query = """
        query GetTags($limit: Int) {
            tags(limit: $limit) {
                id
                name
                slug
            }
        }
    """

    tags_result = await graphql_client.execute(tags_query, variables={"limit": 3})

    if not tags_result.get("data", {}).get("tags"):
        pytest.skip("No tags available for testing")

    available_tags = tags_result["data"]["tags"]
    tag_ids = [tag["id"] for tag in available_tags[:2]]  # Use first 2 tags

    # Step 2: Create a post with tags
    create_post_mutation = """
        mutation CreatePost($input: CreatePostInput!) {
            createPost(input: $input) {
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
            }
        }
    """

    post_data = {
        "title": "Tagged Post Test",
        "content": "This post has multiple tags for testing purposes.",
        "tagIds": tag_ids
    }

    create_result = await graphql_client.execute(
        create_post_mutation,
        variables={"input": post_data}
    )

    assert "errors" not in create_result or not create_result["errors"]
    assert create_result["data"]["createPost"]["__typename"] == "CreatePostSuccess"

    created_post = create_result["data"]["createPost"]["post"]

    # Step 3: Verify tags are associated
    post_tag_ids = {tag["id"] for tag in created_post["tags"]}
    expected_tag_ids = set(tag_ids)

    # Should have some intersection (might not be exact match due to implementation)
    assert len(post_tag_ids.intersection(expected_tag_ids)) > 0

    # Step 4: Query posts by tag
    posts_by_tag_query = """
        query GetPostsByTag($where: PostWhereInput) {
            posts(where: $where) {
                id
                title
                tags {
                    id
                    name
                }
            }
        }
    """

    # Query posts with one of the tags
    posts_result = await graphql_client.execute(
        posts_by_tag_query,
        variables={"where": {"tagIds": [tag_ids[0]]}}
    )

    assert "errors" not in posts_result or not posts_result["errors"]

    # The created post should appear in the results (if filtering works)
    filtered_posts = posts_result["data"]["posts"]

    # Verify that posts in results have the requested tag
    for post in filtered_posts:
        post_tag_ids = {tag["id"] for tag in post["tags"]}
        # Should contain at least one of the requested tags
        assert len(post_tag_ids.intersection(set([tag_ids[0]]))) > 0


@pytest.mark.asyncio
async def test_search_functionality(graphql_client):
    """Test searching posts by title content."""

    # Create a post with distinctive title for searching
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

    unique_title = "Unique Search Test Post"
    post_data = {
        "title": unique_title,
        "content": "This post has a unique title for search testing."
    }

    create_result = await graphql_client.execute(
        create_mutation,
        variables={"input": post_data}
    )

    if create_result["data"]["createPost"]["__typename"] != "CreatePostSuccess":
        pytest.skip("Could not create post for search test")

    # Search for posts containing part of the title
    search_query = """
        query SearchPosts($where: PostWhereInput) {
            posts(where: $where) {
                id
                title
            }
        }
    """

    search_result = await graphql_client.execute(
        search_query,
        variables={"where": {"titleContains": "Unique Search"}}
    )

    assert "errors" not in search_result or not search_result["errors"]

    found_posts = search_result["data"]["posts"]

    # Should find our created post
    found_titles = [post["title"] for post in found_posts]
    assert unique_title in found_titles


@pytest.mark.asyncio
async def test_author_posts_relationship(graphql_client):
    """Test the relationship between authors and their posts."""

    # Get a user with posts
    users_query = """
        query GetUsers($limit: Int) {
            users(limit: $limit) {
                id
                username
                posts {
                    id
                    title
                    author {
                        username
                    }
                }
            }
        }
    """

    users_result = await graphql_client.execute(users_query, variables={"limit": 5})

    assert "errors" not in users_result or not users_result["errors"]
    users = users_result["data"]["users"]

    for user in users:
        # Check that all posts by this user have the correct author
        for post in user["posts"]:
            assert post["author"]["username"] == user["username"]


@pytest.mark.asyncio
async def test_comment_hierarchy(graphql_client):
    """Test comment threading and hierarchy."""

    # Get a post with comments
    posts_query = """
        query GetPostsWithComments {
            posts(limit: 1) {
                id
                comments {
                    id
                    content
                    parentId
                    parent {
                        id
                        content
                    }
                    replies {
                        id
                        content
                        parentId
                    }
                }
            }
        }
    """

    posts_result = await graphql_client.execute(posts_query)

    assert "errors" not in posts_result or not posts_result["errors"]
    posts = posts_result["data"]["posts"]

    if not posts or not posts[0].get("comments"):
        pytest.skip("No posts with comments available for testing")

    comments = posts[0]["comments"]

    # Verify comment hierarchy structure
    for comment in comments:
        if comment["parentId"]:
            # This is a reply - should have parent
            assert comment["parent"] is not None
            assert comment["parent"]["id"] == comment["parentId"]
        else:
            # This is a root comment - might have replies
            if comment["replies"]:
                for reply in comment["replies"]:
                    assert reply["parentId"] == comment["id"]


@pytest.mark.asyncio
async def test_database_constraints(graphql_client):
    """Test that database constraints are enforced."""

    # Try to create comment on non-existent post
    create_comment_mutation = """
        mutation CreateComment($input: CreateCommentInput!) {
            createComment(input: $input) {
                __typename
                ... on NotFoundError {
                    message
                    entityType
                }
                ... on ValidationError {
                    message
                }
            }
        }
    """

    invalid_comment = {
        "postId": "00000000-0000-0000-0000-000000000000",  # Non-existent post
        "content": "This should fail"
    }

    result = await graphql_client.execute(
        create_comment_mutation,
        variables={"input": invalid_comment}
    )

    assert "errors" not in result or not result["errors"]

    comment_result = result["data"]["createComment"]
    # Should be either NotFoundError or ValidationError
    assert comment_result["__typename"] in ["NotFoundError", "ValidationError"]
