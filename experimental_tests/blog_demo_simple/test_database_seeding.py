"""Test database seeding functionality."""

import pytest


@pytest.mark.asyncio
async def test_seeded_data_creation(seeded_blog_database_simple):
    """Test that seeded data is created properly."""
    seeded_data = seeded_blog_database_simple

    # Check that all data types were seeded
    assert "users" in seeded_data
    assert "posts" in seeded_data
    assert "tags" in seeded_data
    assert "comments" in seeded_data

    # Check counts
    assert len(seeded_data["users"]) == 5
    assert len(seeded_data["posts"]) == 6  # 3 authors * 2 posts each
    assert len(seeded_data["tags"]) == 6
    assert len(seeded_data["comments"]) >= 4  # At least 2 comments per published post

    # Check data structure
    user = seeded_data["users"][0]
    assert "pk_user" in user
    assert "identifier" in user
    assert "email" in user
    assert "name" in user

    post = seeded_data["posts"][0]
    assert "pk_post" in post
    assert "identifier" in post
    assert "title" in post
    assert "content" in post
    assert "author_id" in post


@pytest.mark.asyncio
async def test_seeded_data_in_database(db_manager_simple, seeded_blog_database_simple):
    """Test that seeded data actually exists in the database."""

    # Check users in database
    users = await db_manager_simple.execute_query("SELECT COUNT(*) as count FROM tb_user")
    assert users[0]["count"] >= 5

    # Check posts in database
    posts = await db_manager_simple.execute_query("SELECT COUNT(*) as count FROM tb_post")
    assert posts[0]["count"] >= 6

    # Check tags in database
    tags = await db_manager_simple.execute_query("SELECT COUNT(*) as count FROM tb_tag")
    assert tags[0]["count"] >= 6

    # Check comments in database
    comments = await db_manager_simple.execute_query("SELECT COUNT(*) as count FROM tb_comment")
    assert comments[0]["count"] >= 4

    # Check post-tag associations
    associations = await db_manager_simple.execute_query("SELECT COUNT(*) as count FROM tb_post_tag")
    assert associations[0]["count"] >= 4


@pytest.mark.asyncio
async def test_seeded_data_relationships(db_manager_simple, seeded_blog_database_simple):
    """Test that seeded data has proper relationships."""

    # Check that posts have valid authors
    query = """
        SELECT p.pk_post, p.title,
               u.identifier as author_username
        FROM tb_post p
        JOIN tb_user u ON p.fk_author = u.pk_user
        LIMIT 5
    """

    post_authors = await db_manager_simple.execute_query(query)
    assert len(post_authors) >= 5

    for post_author in post_authors:
        assert post_author["title"] is not None
        assert post_author["author_username"] is not None

    # Check that comments belong to valid posts and users
    query = """
        SELECT c.pk_comment, c.content,
               p.title as post_title,
               u.identifier as commenter_username
        FROM tb_comment c
        JOIN tb_post p ON c.fk_post = p.pk_post
        JOIN tb_user u ON c.fk_author = u.pk_user
        LIMIT 3
    """

    comment_relationships = await db_manager_simple.execute_query(query)
    assert len(comment_relationships) >= 3

    for comment in comment_relationships:
        assert comment["content"] is not None
        assert comment["post_title"] is not None
        assert comment["commenter_username"] is not None


@pytest.mark.asyncio
async def test_cleanup_after_seeding(db_connection_simple):
    """Test that data is properly cleaned up after seeding."""
    from fixtures.database.seeding import BlogDataSeeder

    seeder = BlogDataSeeder(db_connection_simple)

    # Seed some data
    await seeder.seed_users(2)
    await seeder.seed_tags(2)

    # Verify data exists
    async with db_connection_simple.cursor() as cursor:
        await cursor.execute("SELECT COUNT(*) FROM tb_user")
        user_count = (await cursor.fetchone())[0]
        assert user_count >= 2

        await cursor.execute("SELECT COUNT(*) FROM tb_tag")
        tag_count = (await cursor.fetchone())[0]
        assert tag_count >= 2

    # Clean up
    await seeder.cleanup_all_data()

    # Verify data is gone
    async with db_connection_simple.cursor() as cursor:
        await cursor.execute("SELECT COUNT(*) FROM tb_user")
        user_count = (await cursor.fetchone())[0]
        assert user_count == 0

        await cursor.execute("SELECT COUNT(*) FROM tb_tag")
        tag_count = (await cursor.fetchone())[0]
        assert tag_count == 0
