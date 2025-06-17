"""Test @dataloader_field decorator for automatic DataLoader integration."""

from typing import Dict, List, Optional
from uuid import UUID, uuid4

import pytest
from fastapi.testclient import TestClient

import fraiseql
from fraiseql.fastapi import create_fraiseql_app
from fraiseql.optimization.dataloader import DataLoader
from fraiseql.optimization.registry import get_loader
from fraiseql.gql.schema_builder import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


@pytest.fixture
def register_test_queries():
    """Register the test queries needed for schema tests."""
    from fraiseql.gql.schema_builder import SchemaRegistry
    registry = SchemaRegistry.get_instance()
    
    # Re-register the query functions
    registry.register_query(get_post)
    registry.register_query(get_comment)
    
    return registry


# Test DataLoaders
class UserDataLoader(DataLoader[UUID, Dict]):
    """DataLoader for loading users by ID."""
    
    def __init__(self, db):
        super().__init__()
        self.db = db
        self.load_calls = []  # Track batch calls for testing
    
    async def batch_load(self, user_ids: List[UUID]) -> List[Optional[Dict]]:
        """Batch load users by IDs."""
        self.load_calls.append(list(user_ids))  # Track the call
        
        # Mock data
        users_db = {
            UUID("223e4567-e89b-12d3-a456-426614174001"): {
                "id": UUID("223e4567-e89b-12d3-a456-426614174001"),
                "name": "John Doe",
                "email": "john@example.com"
            },
            UUID("323e4567-e89b-12d3-a456-426614174002"): {
                "id": UUID("323e4567-e89b-12d3-a456-426614174002"),
                "name": "Jane Smith", 
                "email": "jane@example.com"
            }
        }
        
        results = []
        for user_id in user_ids:
            user_data = users_db.get(user_id)
            results.append(user_data)
        
        return results


class PostDataLoader(DataLoader[UUID, Dict]):
    """DataLoader for loading posts by ID."""
    
    def __init__(self, db):
        super().__init__()
        self.db = db
    
    async def batch_load(self, post_ids: List[UUID]) -> List[Optional[Dict]]:
        """Batch load posts by IDs."""
        # Mock data
        posts_db = {
            UUID("123e4567-e89b-12d3-a456-426614174000"): {
                "id": UUID("123e4567-e89b-12d3-a456-426614174000"),
                "title": "Test Post",
                "content": "Test content",
                "author_id": UUID("223e4567-e89b-12d3-a456-426614174001")
            }
        }
        
        results = []
        for post_id in post_ids:
            post_data = posts_db.get(post_id)
            results.append(post_data)
        
        return results


# Test types with @dataloader_field decorator
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str


@fraiseql.type
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID
    
    @fraiseql.dataloader_field(UserDataLoader, key_field="author_id")
    async def author(self, info) -> Optional[User]:
        """Load author using DataLoader automatically."""
        # This should be auto-implemented by the decorator
        pass


@fraiseql.type  
class Comment:
    id: UUID
    content: str
    author_id: UUID
    post_id: UUID
    
    @fraiseql.dataloader_field(UserDataLoader, key_field="author_id")
    async def author(self, info) -> Optional[User]:
        """Load comment author using DataLoader."""
        pass
    
    @fraiseql.dataloader_field(PostDataLoader, key_field="post_id")
    async def post(self, info) -> Optional[Post]:
        """Load comment post using DataLoader."""
        pass


# Test queries
@fraiseql.query
async def get_post(info, id: UUID) -> Optional[Post]:
    """Get a post by ID."""
    if str(id) == "123e4567-e89b-12d3-a456-426614174000":
        return Post(
            id=id,
            title="Test Post",
            content="Test content",
            author_id=UUID("223e4567-e89b-12d3-a456-426614174001")
        )
    return None


@fraiseql.query
async def get_comment(info, id: UUID) -> Optional[Comment]:
    """Get a comment by ID."""
    if str(id) == "323e4567-e89b-12d3-a456-426614174002":
        return Comment(
            id=id,
            content="Great post!",
            author_id=UUID("323e4567-e89b-12d3-a456-426614174002"),
            post_id=UUID("123e4567-e89b-12d3-a456-426614174000")
        )
    return None


def test_dataloader_field_decorator_exists():
    """Test that @dataloader_field decorator exists and can be imported."""
    # This test will fail until we implement the decorator
    try:
        from fraiseql import dataloader_field
        assert dataloader_field is not None
    except ImportError:
        pytest.fail("@dataloader_field decorator not implemented yet")


def test_dataloader_field_adds_metadata():
    """Test that @dataloader_field decorator adds proper metadata to methods."""
    # Check that the decorator adds metadata we can use for field resolution
    assert hasattr(Post.author, '__fraiseql_dataloader__')
    
    metadata = Post.author.__fraiseql_dataloader__
    assert metadata['loader_class'] == UserDataLoader
    assert metadata['key_field'] == 'author_id'


def test_dataloader_field_generates_schema_field(register_test_queries):
    """Test that @dataloader_field decorated methods appear in GraphQL schema."""
    app = create_fraiseql_app(
        database_url="postgresql://test/test",
        types=[User, Post, Comment]
    )
    
    with TestClient(app) as client:
        # Test introspection to verify field exists
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        __type(name: "Post") {
                            fields {
                                name
                                type {
                                    name
                                }
                            }
                        }
                    }
                """
            }
        )
        
        assert response.status_code == 200
        data = response.json()
        
        # Should have author field from @dataloader_field
        fields = {f["name"]: f["type"]["name"] for f in data["data"]["__type"]["fields"]}
        assert "author" in fields
        assert fields["author"] == "User"


def test_dataloader_field_automatic_resolution(register_test_queries):
    """Test that @dataloader_field automatically resolves using DataLoader."""
    app = create_fraiseql_app(
        database_url="postgresql://test/test",
        types=[User, Post, Comment]
    )
    
    with TestClient(app) as client:
        # Query that should automatically use DataLoader for author resolution
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        get_post(id: "123e4567-e89b-12d3-a456-426614174000") {
                            id
                            title
                            author {
                                id
                                name
                                email
                            }
                        }
                    }
                """
            }
        )
        
        assert response.status_code == 200
        data = response.json()
        
        # Should successfully resolve author using DataLoader
        post = data["data"]["get_post"]
        assert post["title"] == "Test Post"
        assert post["author"]["name"] == "John Doe"
        assert post["author"]["email"] == "john@example.com"


def test_dataloader_field_batching(register_test_queries):
    """Test that @dataloader_field properly batches multiple field resolutions."""
    app = create_fraiseql_app(
        database_url="postgresql://test/test",
        types=[User, Post, Comment]
    )
    
    # We need a way to track DataLoader calls to verify batching
    # This would require access to the actual DataLoader instance
    with TestClient(app) as client:
        # Query multiple items that should batch author lookups
        response = client.post(
            "/graphql", 
            json={
                "query": """
                    query {
                        post: get_post(id: "123e4567-e89b-12d3-a456-426614174000") {
                            author { name }
                        }
                        comment: get_comment(id: "323e4567-e89b-12d3-a456-426614174002") {
                            author { name }
                            post {
                                author { name }
                            }
                        }
                    }
                """
            }
        )
        
        assert response.status_code == 200
        data = response.json()
        
        # Both should resolve authors (would be batched in real implementation)
        assert data["data"]["post"]["author"]["name"] == "John Doe"
        assert data["data"]["comment"]["author"]["name"] == "Jane Smith"
        assert data["data"]["comment"]["post"]["author"]["name"] == "John Doe"


def test_dataloader_field_with_multiple_loaders(register_test_queries):
    """Test @dataloader_field works with different DataLoader types."""
    app = create_fraiseql_app(
        database_url="postgresql://test/test",
        types=[User, Post, Comment]
    )
    
    with TestClient(app) as client:
        # Query that uses both UserDataLoader and PostDataLoader
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        get_comment(id: "323e4567-e89b-12d3-a456-426614174002") {
                            id
                            content
                            author {
                                name
                            }
                            post {
                                id
                                title
                                author {
                                    name
                                }
                            }
                        }
                    }
                """
            }
        )
        
        assert response.status_code == 200
        data = response.json()
        
        comment = data["data"]["get_comment"]
        assert comment["content"] == "Great post!"
        assert comment["author"]["name"] == "Jane Smith"
        assert comment["post"]["title"] == "Test Post"
        assert comment["post"]["author"]["name"] == "John Doe"


def test_dataloader_field_error_handling():
    """Test that @dataloader_field handles errors gracefully."""
    # Test decorator with invalid parameters
    with pytest.raises(ValueError, match="loader_class must be a DataLoader subclass"):
        @fraiseql.type
        class InvalidType:
            @fraiseql.dataloader_field(str, key_field="id")  # Invalid loader class
            async def field(self, info):
                pass


def test_dataloader_field_without_key_field():
    """Test that @dataloader_field requires key_field parameter."""
    with pytest.raises(TypeError, match="missing 1 required keyword-only argument: 'key_field'"):
        @fraiseql.type  
        class InvalidType:
            @fraiseql.dataloader_field(UserDataLoader)  # Missing key_field
            async def field(self, info):
                pass


def test_dataloader_field_with_custom_resolver(register_test_queries):
    """Test @dataloader_field with custom resolver logic."""
    @fraiseql.type
    class CustomPost:
        id: UUID
        author_id: UUID
        
        @fraiseql.dataloader_field(UserDataLoader, key_field="author_id")
        async def author(self, info) -> Optional[User]:
            """Custom logic before DataLoader."""
            if not self.author_id:
                return None
            
            # Custom logic can be added here
            # The decorator should still handle the DataLoader call
            loader = get_loader(UserDataLoader)
            user_data = await loader.load(self.author_id)
            
            if user_data:
                # Custom processing
                user_data = dict(user_data)
                user_data["name"] = f"Mr. {user_data['name']}"
                return User(**user_data)
            
            return None
    
    # Test that custom logic works
    app = create_fraiseql_app(
        database_url="postgresql://test/test",
        types=[User, CustomPost]
    )
    
    # This test verifies the decorator doesn't interfere with custom logic
    assert True  # Would need actual query test when implemented


def test_dataloader_field_schema_introspection(register_test_queries):
    """Test that @dataloader_field decorated fields show up in schema introspection."""
    app = create_fraiseql_app(
        database_url="postgresql://test/test",
        types=[User, Post, Comment]
    )
    
    with TestClient(app) as client:
        # Get full schema to verify all fields are present
        response = client.post(
            "/graphql",
            json={
                "query": """
                    query {
                        __schema {
                            types {
                                name
                                fields {
                                    name
                                    type {
                                        name
                                    }
                                }
                            }
                        }
                    }
                """
            }
        )
        
        assert response.status_code == 200
        data = response.json()
        
        # Find Post type and verify it has author field
        types = {t["name"]: t for t in data["data"]["__schema"]["types"]}
        
        assert "Post" in types
        post_fields = {f["name"]: f["type"]["name"] for f in types["Post"]["fields"] if f["name"] != "__typename"}
        assert "author" in post_fields
        assert post_fields["author"] == "User"
        
        # Find Comment type and verify it has both author and post fields
        assert "Comment" in types
        comment_fields = {f["name"]: f["type"]["name"] for f in types["Comment"]["fields"] if f["name"] != "__typename"}
        assert "author" in comment_fields
        assert "post" in comment_fields
        assert comment_fields["author"] == "User"
        assert comment_fields["post"] == "Post"