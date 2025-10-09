# Unit Testing

Unit tests verify individual components in isolation, using mocks and stubs to eliminate external dependencies like databases or APIs.

## Testing FraiseQL Types

### Basic Type Testing

```python
# test_types.py
import pytest
from datetime import datetime
from fraiseql import type as fraise_type
from fraiseql.types import ID, EmailAddress
from pydantic import ValidationError

@fraise_type
class User:
    id: ID
    email: EmailAddress
    name: str
    created_at: datetime

class TestUserType:
    def test_user_creation_success(self):
        """Test successful User type creation"""
        user = User(
            id="123e4567-e89b-12d3-a456-426614174000",
            email="test@example.com",
            name="Test User",
            created_at=datetime(2024, 1, 15, 10, 30, 0)
        )

        assert user.id == "123e4567-e89b-12d3-a456-426614174000"
        assert user.email == "test@example.com"
        assert user.name == "Test User"
        assert isinstance(user.created_at, datetime)

    def test_email_validation_failure(self):
        """Test email field validation fails for invalid email"""
        with pytest.raises(ValidationError) as exc_info:
            User(
                id="123e4567-e89b-12d3-a456-426614174000",
                email="not-an-email",  # Invalid email
                name="Test User",
                created_at=datetime.now()
            )

        assert "email" in str(exc_info.value)

    def test_required_field_validation(self):
        """Test validation fails when required fields are missing"""
        with pytest.raises(ValidationError) as exc_info:
            User(
                id="123e4567-e89b-12d3-a456-426614174000",
                email="test@example.com"
                # Missing name and created_at
            )

        errors = exc_info.value.errors()
        field_names = {error["loc"][0] for error in errors}
        assert "name" in field_names
        assert "created_at" in field_names

    def test_id_format_validation(self):
        """Test ID field accepts valid UUID format"""
        # Valid UUID formats should work
        valid_ids = [
            "123e4567-e89b-12d3-a456-426614174000",  # Standard UUID
            "550e8400e29b41d4a716446655440000",        # Without hyphens
        ]

        for valid_id in valid_ids:
            user = User(
                id=valid_id,
                email="test@example.com",
                name="Test User",
                created_at=datetime.now()
            )
            assert user.id == valid_id
```

### Custom Type Validation

```python
# test_custom_types.py
import pytest
from fraiseql import type as fraise_type
from pydantic import ValidationError, validator

@fraise_type
class BlogPost:
    title: str
    content: str
    status: str = "draft"

    @validator("title")
    def title_must_not_be_empty(cls, v):
        if not v.strip():
            raise ValueError("Title cannot be empty")
        return v.strip()

    @validator("status")
    def status_must_be_valid(cls, v):
        valid_statuses = ["draft", "published", "archived"]
        if v not in valid_statuses:
            raise ValueError(f"Status must be one of: {valid_statuses}")
        return v

class TestBlogPostType:
    def test_valid_blog_post_creation(self):
        """Test creating a valid blog post"""
        post = BlogPost(
            title="My Great Post",
            content="This is some great content!",
            status="published"
        )

        assert post.title == "My Great Post"
        assert post.content == "This is some great content!"
        assert post.status == "published"

    def test_empty_title_validation(self):
        """Test that empty titles are rejected"""
        with pytest.raises(ValidationError) as exc_info:
            BlogPost(
                title="   ",  # Empty/whitespace title
                content="Content here"
            )

        assert "Title cannot be empty" in str(exc_info.value)

    def test_invalid_status_validation(self):
        """Test that invalid status values are rejected"""
        with pytest.raises(ValidationError) as exc_info:
            BlogPost(
                title="Valid Title",
                content="Valid content",
                status="invalid_status"
            )

        assert "Status must be one of" in str(exc_info.value)

    def test_default_status(self):
        """Test that status defaults to 'draft'"""
        post = BlogPost(
            title="Default Status Post",
            content="Some content"
        )

        assert post.status == "draft"
```

## Testing Query Resolvers

### Basic Query Testing

```python
# test_queries.py
import pytest
from unittest.mock import AsyncMock, MagicMock
from app.queries import get_users, get_user_by_id, search_users

@pytest.mark.asyncio
class TestUserQueries:
    async def test_get_users_success(self):
        """Test successful retrieval of all users"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.find.return_value = [
            {"id": "1", "name": "Alice", "email": "alice@example.com"},
            {"id": "2", "name": "Bob", "email": "bob@example.com"}
        ]

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act
        users = await get_users(info)

        # Assert
        assert len(users) == 2
        assert users[0].name == "Alice"
        assert users[1].name == "Bob"
        mock_repo.find.assert_called_once_with("v_user")

    async def test_get_users_empty_result(self):
        """Test get_users when no users exist"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.find.return_value = []

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act
        users = await get_users(info)

        # Assert
        assert users == []
        mock_repo.find.assert_called_once_with("v_user")

    async def test_get_user_by_id_found(self):
        """Test get_user_by_id when user exists"""
        # Arrange
        expected_user_data = {
            "id": "test-id-123",
            "name": "Found User",
            "email": "found@example.com"
        }

        mock_repo = AsyncMock()
        mock_repo.find_one.return_value = expected_user_data

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act
        user = await get_user_by_id(info, id="test-id-123")

        # Assert
        assert user is not None
        assert user.id == "test-id-123"
        assert user.name == "Found User"
        mock_repo.find_one.assert_called_once_with(
            "v_user",
            where={"id": "test-id-123"}
        )

    async def test_get_user_by_id_not_found(self):
        """Test get_user_by_id when user doesn't exist"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.find_one.return_value = None

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act
        user = await get_user_by_id(info, id="nonexistent-id")

        # Assert
        assert user is None
        mock_repo.find_one.assert_called_once_with(
            "v_user",
            where={"id": "nonexistent-id"}
        )

    async def test_search_users_with_filters(self):
        """Test search_users with name filter"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.find.return_value = [
            {"id": "1", "name": "Alice Smith", "email": "alice@example.com"}
        ]

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act
        users = await search_users(info, name_contains="Alice")

        # Assert
        assert len(users) == 1
        assert users[0].name == "Alice Smith"
        mock_repo.find.assert_called_once_with(
            "v_user",
            where={"name": {"ilike": "%Alice%"}}
        )
```

### Testing Query Error Handling

```python
# test_query_errors.py
import pytest
from unittest.mock import AsyncMock, MagicMock
from fraiseql import GraphQLError
from app.queries import get_user_posts
import asyncpg

@pytest.mark.asyncio
class TestQueryErrorHandling:
    async def test_database_connection_error(self):
        """Test handling of database connection errors"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.find.side_effect = asyncpg.ConnectionDoesNotExistError()

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act & Assert
        with pytest.raises(GraphQLError) as exc_info:
            await get_user_posts(info, user_id="test-id")

        assert "database connection" in str(exc_info.value).lower()

    async def test_invalid_uuid_format(self):
        """Test handling of invalid UUID formats"""
        # Arrange
        info = MagicMock()
        info.context = {"repo": AsyncMock()}

        # Act & Assert
        with pytest.raises(GraphQLError) as exc_info:
            await get_user_posts(info, user_id="not-a-uuid")

        assert "invalid id format" in str(exc_info.value).lower()

    async def test_permission_denied_error(self):
        """Test handling of permission errors"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.find.side_effect = asyncpg.InsufficientPrivilegeError()

        info = MagicMock()
        info.context = {"repo": mock_repo, "user": None}  # No authenticated user

        # Act & Assert
        with pytest.raises(GraphQLError) as exc_info:
            await get_user_posts(info, user_id="test-id")

        assert "permission denied" in str(exc_info.value).lower()
```

## Testing Mutations

### Basic Mutation Testing

```python
# test_mutations.py
import pytest
from unittest.mock import AsyncMock, MagicMock
from fraiseql import GraphQLError
from app.mutations import create_user, update_user, delete_user
import asyncpg

@pytest.mark.asyncio
class TestUserMutations:
    async def test_create_user_success(self):
        """Test successful user creation"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.call_function.return_value = "new-user-id-123"
        mock_repo.find_one.return_value = {
            "id": "new-user-id-123",
            "name": "New User",
            "email": "new@example.com",
            "created_at": "2024-01-15T10:30:00"
        }

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act
        user = await create_user(
            info,
            name="New User",
            email="new@example.com"
        )

        # Assert
        assert user.id == "new-user-id-123"
        assert user.name == "New User"
        assert user.email == "new@example.com"

        # Verify function was called correctly
        mock_repo.call_function.assert_called_once_with(
            "fn_create_user",
            p_name="New User",
            p_email="new@example.com"
        )

        # Verify user was fetched after creation
        mock_repo.find_one.assert_called_once_with(
            "v_user",
            where={"id": "new-user-id-123"}
        )

    async def test_create_user_duplicate_email(self):
        """Test create_user handles duplicate email error"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.call_function.side_effect = asyncpg.UniqueViolationError(
            "duplicate key value violates unique constraint"
        )

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act & Assert
        with pytest.raises(GraphQLError) as exc_info:
            await create_user(
                info,
                name="Test User",
                email="existing@example.com"
            )

        error = exc_info.value
        assert "email already exists" in str(error).lower()
        assert error.extensions["code"] == "DUPLICATE_EMAIL"

    async def test_update_user_success(self):
        """Test successful user update"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.call_function.return_value = True  # Update successful
        mock_repo.find_one.return_value = {
            "id": "user-123",
            "name": "Updated Name",
            "email": "updated@example.com",
            "created_at": "2024-01-15T10:30:00"
        }

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act
        user = await update_user(
            info,
            id="user-123",
            name="Updated Name"
        )

        # Assert
        assert user.id == "user-123"
        assert user.name == "Updated Name"

        mock_repo.call_function.assert_called_once_with(
            "fn_update_user",
            p_user_id="user-123",
            p_name="Updated Name"
        )

    async def test_update_user_not_found(self):
        """Test update_user when user doesn't exist"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.call_function.return_value = False  # No rows affected

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act & Assert
        with pytest.raises(GraphQLError) as exc_info:
            await update_user(
                info,
                id="nonexistent-id",
                name="New Name"
            )

        error = exc_info.value
        assert "user not found" in str(error).lower()
        assert error.extensions["code"] == "USER_NOT_FOUND"

    async def test_delete_user_success(self):
        """Test successful user deletion"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.call_function.return_value = True  # Deletion successful

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act
        success = await delete_user(info, id="user-to-delete")

        # Assert
        assert success is True
        mock_repo.call_function.assert_called_once_with(
            "fn_delete_user",
            p_user_id="user-to-delete"
        )

    async def test_delete_user_with_dependencies(self):
        """Test delete_user when user has dependent records"""
        # Arrange
        mock_repo = AsyncMock()
        mock_repo.call_function.side_effect = asyncpg.ForeignKeyViolationError(
            "cannot delete user with existing posts"
        )

        info = MagicMock()
        info.context = {"repo": mock_repo}

        # Act & Assert
        with pytest.raises(GraphQLError) as exc_info:
            await delete_user(info, id="user-with-posts")

        error = exc_info.value
        assert "cannot delete" in str(error).lower()
        assert error.extensions["code"] == "DEPENDENCY_ERROR"
```

### Testing Input Validation

```python
# test_mutation_validation.py
import pytest
from unittest.mock import MagicMock, AsyncMock
from fraiseql import GraphQLError
from app.mutations import create_user

@pytest.mark.asyncio
class TestMutationValidation:
    async def test_create_user_empty_name(self):
        """Test create_user rejects empty name"""
        info = MagicMock()
        info.context = {"repo": AsyncMock()}

        with pytest.raises(GraphQLError) as exc_info:
            await create_user(info, name="", email="test@example.com")

        assert "name cannot be empty" in str(exc_info.value).lower()

    async def test_create_user_invalid_email_format(self):
        """Test create_user rejects invalid email formats"""
        info = MagicMock()
        info.context = {"repo": AsyncMock()}

        invalid_emails = [
            "notanemail",
            "@example.com",
            "test@",
            "test@.com",
            "test space@example.com"
        ]

        for invalid_email in invalid_emails:
            with pytest.raises(GraphQLError) as exc_info:
                await create_user(
                    info,
                    name="Test User",
                    email=invalid_email
                )

            assert "invalid email" in str(exc_info.value).lower()

    async def test_create_user_name_too_long(self):
        """Test create_user rejects names that are too long"""
        info = MagicMock()
        info.context = {"repo": AsyncMock()}

        very_long_name = "x" * 256  # Assume 255 char limit

        with pytest.raises(GraphQLError) as exc_info:
            await create_user(
                info,
                name=very_long_name,
                email="test@example.com"
            )

        assert "name too long" in str(exc_info.value).lower()
```

## Testing Utilities and Helpers

### Testing Pure Functions

```python
# test_utils.py
import pytest
from datetime import datetime, timezone
from app.utils import (
    format_date_for_api,
    validate_uuid,
    generate_slug,
    sanitize_input
)

class TestDateUtils:
    def test_format_date_for_api(self):
        """Test date formatting for API responses"""
        # Test with timezone-aware datetime
        dt = datetime(2024, 1, 15, 10, 30, 45, tzinfo=timezone.utc)
        formatted = format_date_for_api(dt)

        assert formatted == "2024-01-15T10:30:45Z"

    def test_format_date_for_api_none(self):
        """Test date formatting with None input"""
        result = format_date_for_api(None)
        assert result is None

class TestValidationUtils:
    def test_validate_uuid_valid(self):
        """Test UUID validation with valid UUIDs"""
        valid_uuids = [
            "123e4567-e89b-12d3-a456-426614174000",
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
        ]

        for uuid_str in valid_uuids:
            assert validate_uuid(uuid_str) is True

    def test_validate_uuid_invalid(self):
        """Test UUID validation with invalid UUIDs"""
        invalid_uuids = [
            "not-a-uuid",
            "123",
            "123e4567-e89b-12d3-a456",  # Too short
            "123e4567-e89b-12d3-a456-426614174000-extra"  # Too long
        ]

        for invalid_uuid in invalid_uuids:
            assert validate_uuid(invalid_uuid) is False

class TestStringUtils:
    def test_generate_slug(self):
        """Test slug generation from titles"""
        test_cases = [
            ("My Great Blog Post", "my-great-blog-post"),
            ("Title With Special Characters!@#", "title-with-special-characters"),
            ("  Extra   Spaces  ", "extra-spaces"),
            ("UPPERCASE TITLE", "uppercase-title"),
            ("Title-with-hyphens", "title-with-hyphens")
        ]

        for title, expected_slug in test_cases:
            assert generate_slug(title) == expected_slug

    def test_sanitize_input(self):
        """Test input sanitization"""
        test_cases = [
            ("<script>alert('xss')</script>", "alert('xss')"),
            ("Normal text", "Normal text"),
            ("<b>Bold</b> text", "Bold text"),
            ("", ""),
            ("   whitespace   ", "whitespace")
        ]

        for input_str, expected in test_cases:
            assert sanitize_input(input_str) == expected
```

## Testing Decorators and Middleware

```python
# test_decorators.py
import pytest
from unittest.mock import MagicMock, AsyncMock
from fraiseql import GraphQLError
from app.decorators import require_auth, rate_limit, validate_input

@pytest.mark.asyncio
class TestAuthDecorator:
    async def test_require_auth_with_valid_token(self):
        """Test require_auth allows access with valid token"""
        # Arrange
        @require_auth
        async def protected_resolver(info):
            return "secret data"

        info = MagicMock()
        info.context = {
            "user": {"id": "user-123", "email": "test@example.com"}
        }

        # Act
        result = await protected_resolver(info)

        # Assert
        assert result == "secret data"

    async def test_require_auth_without_token(self):
        """Test require_auth blocks access without token"""
        # Arrange
        @require_auth
        async def protected_resolver(info):
            return "secret data"

        info = MagicMock()
        info.context = {"user": None}

        # Act & Assert
        with pytest.raises(GraphQLError) as exc_info:
            await protected_resolver(info)

        error = exc_info.value
        assert "authentication required" in str(error).lower()
        assert error.extensions["code"] == "UNAUTHENTICATED"

@pytest.mark.asyncio
class TestValidationDecorator:
    async def test_validate_input_success(self):
        """Test input validation passes with valid data"""
        # Arrange
        @validate_input(name=str, email=str)
        async def resolver_with_validation(info, **kwargs):
            return f"Hello {kwargs['name']}"

        # Act
        result = await resolver_with_validation(
            MagicMock(),
            name="John",
            email="john@example.com"
        )

        # Assert
        assert result == "Hello John"

    async def test_validate_input_failure(self):
        """Test input validation fails with invalid data"""
        # Arrange
        @validate_input(name=str, age=int)
        async def resolver_with_validation(info, **kwargs):
            return "success"

        # Act & Assert
        with pytest.raises(GraphQLError) as exc_info:
            await resolver_with_validation(
                MagicMock(),
                name="John",
                age="not-a-number"  # Invalid type
            )

        assert "validation error" in str(exc_info.value).lower()
```

## Test Configuration and Fixtures

### Shared Test Configuration

```python
# conftest.py
import pytest
from unittest.mock import MagicMock, AsyncMock

@pytest.fixture
def mock_info():
    """Standard GraphQL info object for testing"""
    info = MagicMock()
    info.context = {}
    return info

@pytest.fixture
def mock_repo():
    """Mock repository for unit tests"""
    return AsyncMock()

@pytest.fixture
def mock_authenticated_context(mock_repo):
    """Mock context with authenticated user"""
    return {
        "repo": mock_repo,
        "user": {
            "id": "test-user-123",
            "email": "test@example.com",
            "name": "Test User"
        }
    }

@pytest.fixture
def mock_unauthenticated_context(mock_repo):
    """Mock context without authenticated user"""
    return {
        "repo": mock_repo,
        "user": None
    }
```

## Running Unit Tests

### Command Line Examples

```bash
# Run all unit tests
pytest tests/unit/ -v

# Run specific test file
pytest tests/unit/test_queries.py -v

# Run with coverage
pytest tests/unit/ --cov=app --cov-report=html

# Run tests matching pattern
pytest -k "test_create_user" -v

# Run failed tests from last run
pytest --lf

# Run in parallel (faster execution)
pytest tests/unit/ -n auto
```

### IDE Integration

Most IDEs support running pytest tests directly:

- **VS Code**: Install Python extension, tests appear in Test Explorer
- **PyCharm**: Built-in pytest support with run/debug capabilities
- **Vim/Neovim**: Use vim-test plugin with pytest integration

Unit tests should run quickly (< 1 second each) since they don't use real databases or external services. They're perfect for TDD workflows and rapid feedback during development.
