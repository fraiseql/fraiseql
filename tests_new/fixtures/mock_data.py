"""Mock data generators for FraiseQL testing.

This module provides comprehensive test data generation utilities including:
- Factories for common domain objects (users, posts, comments, etc.)
- Realistic fake data generation using Faker
- Relationship builders for complex object graphs
- Database seed data generators
- JSON/JSONB test data structures
- Time-series and sequence generators

These utilities enable consistent, realistic test data across
all test scenarios while reducing boilerplate and improving maintainability.
"""

import random
from datetime import UTC, datetime, timedelta
from typing import Any, Dict, List, Optional
from uuid import uuid4

import pytest
from faker import Faker

# Initialize Faker with consistent seed for reproducible tests
fake = Faker()
Faker.seed(42)


class DataFactory:
    """Base factory class for generating test data."""

    def __init__(self, locale: str = "en_US"):
        """Initialize factory with locale.

        Args:
            locale: Faker locale for data generation
        """
        self.fake = Faker(locale)
        self.fake.seed_instance(42)

    def reset_seed(self, seed: int = 42):
        """Reset the random seed for consistent data.

        Args:
            seed: Random seed value
        """
        self.fake.seed_instance(seed)
        random.seed(seed)


class UserFactory(DataFactory):
    """Factory for generating user test data."""

    def create(
        self, id: Optional[str] = None, role: str = "user", is_active: bool = True, **overrides
    ) -> Dict[str, Any]:
        """Create a user object.

        Args:
            id: User ID (auto-generated if not provided)
            role: User role (admin, user, guest)
            is_active: Whether user is active
            **overrides: Field overrides

        Returns:
            Dict: User data
        """
        user_id = id or str(uuid4())

        base_data = {
            "id": user_id,
            "username": self.fake.user_name(),
            "email": self.fake.email(),
            "first_name": self.fake.first_name(),
            "last_name": self.fake.last_name(),
            "role": role,
            "is_active": is_active,
            "is_admin": role == "admin",
            "created_at": self.fake.date_time_between(start_date="-1y", end_date="now", tzinfo=UTC),
            "updated_at": datetime.now(UTC),
            "profile": {
                "bio": self.fake.text(max_nb_chars=200),
                "avatar_url": self.fake.image_url(),
                "website": self.fake.url(),
                "location": self.fake.city(),
                "timezone": self.fake.timezone(),
            },
            "preferences": {
                "theme": random.choice(["light", "dark", "auto"]),
                "language": "en",
                "notifications": {
                    "email": True,
                    "push": random.choice([True, False]),
                    "marketing": random.choice([True, False]),
                },
            },
            "metadata": {
                "last_login": self.fake.date_time_between(
                    start_date="-30d", end_date="now", tzinfo=UTC
                ),
                "login_count": random.randint(1, 100),
                "ip_address": self.fake.ipv4(),
                "user_agent": self.fake.user_agent(),
            },
        }

        # Apply overrides
        base_data.update(overrides)

        return base_data

    def create_batch(self, count: int, **kwargs) -> List[Dict[str, Any]]:
        """Create multiple users.

        Args:
            count: Number of users to create
            **kwargs: Common user attributes

        Returns:
            List[Dict]: List of user objects
        """
        return [self.create(**kwargs) for _ in range(count)]

    def create_admin(self, **overrides) -> Dict[str, Any]:
        """Create an admin user."""
        return self.create(role="admin", **overrides)

    def create_guest(self, **overrides) -> Dict[str, Any]:
        """Create a guest user."""
        return self.create(role="guest", **overrides)


class PostFactory(DataFactory):
    """Factory for generating blog post test data."""

    def create(
        self,
        id: Optional[str] = None,
        author_id: Optional[str] = None,
        status: str = "published",
        **overrides,
    ) -> Dict[str, Any]:
        """Create a blog post object.

        Args:
            id: Post ID (auto-generated if not provided)
            author_id: Author user ID (auto-generated if not provided)
            status: Post status (draft, published, archived)
            **overrides: Field overrides

        Returns:
            Dict: Post data
        """
        post_id = id or str(uuid4())
        author_id = author_id or str(uuid4())

        created_at = self.fake.date_time_between(start_date="-6m", end_date="now", tzinfo=UTC)

        base_data = {
            "id": post_id,
            "title": self.fake.sentence(nb_words=6).rstrip("."),
            "slug": self.fake.slug(),
            "content": self.fake.text(max_nb_chars=2000),
            "excerpt": self.fake.text(max_nb_chars=200),
            "author_id": author_id,
            "status": status,
            "published": status == "published",
            "featured": random.choice([True, False]),
            "created_at": created_at,
            "updated_at": self.fake.date_time_between(
                start_date=created_at, end_date="now", tzinfo=UTC
            ),
            "published_at": created_at if status == "published" else None,
            "view_count": random.randint(0, 10000),
            "like_count": random.randint(0, 500),
            "comment_count": random.randint(0, 50),
            "reading_time": random.randint(2, 15),
            "tags": [self.fake.word() for _ in range(random.randint(1, 5))],
            "metadata": {
                "seo_title": self.fake.sentence(nb_words=8),
                "seo_description": self.fake.text(max_nb_chars=160),
                "featured_image": self.fake.image_url(),
                "social_image": self.fake.image_url(),
            },
        }

        # Apply overrides
        base_data.update(overrides)

        return base_data

    def create_batch(self, count: int, **kwargs) -> List[Dict[str, Any]]:
        """Create multiple posts.

        Args:
            count: Number of posts to create
            **kwargs: Common post attributes

        Returns:
            List[Dict]: List of post objects
        """
        return [self.create(**kwargs) for _ in range(count)]

    def create_draft(self, **overrides) -> Dict[str, Any]:
        """Create a draft post."""
        return self.create(status="draft", published=False, published_at=None, **overrides)

    def create_published(self, **overrides) -> Dict[str, Any]:
        """Create a published post."""
        return self.create(status="published", **overrides)


class CommentFactory(DataFactory):
    """Factory for generating comment test data."""

    def create(
        self,
        id: Optional[str] = None,
        post_id: Optional[str] = None,
        author_id: Optional[str] = None,
        parent_id: Optional[str] = None,
        **overrides,
    ) -> Dict[str, Any]:
        """Create a comment object.

        Args:
            id: Comment ID (auto-generated if not provided)
            post_id: Post ID (auto-generated if not provided)
            author_id: Author user ID (auto-generated if not provided)
            parent_id: Parent comment ID for nested comments
            **overrides: Field overrides

        Returns:
            Dict: Comment data
        """
        comment_id = id or str(uuid4())
        post_id = post_id or str(uuid4())
        author_id = author_id or str(uuid4())

        base_data = {
            "id": comment_id,
            "post_id": post_id,
            "author_id": author_id,
            "parent_id": parent_id,
            "content": self.fake.text(max_nb_chars=500),
            "status": "approved",
            "created_at": self.fake.date_time_between(start_date="-3m", end_date="now", tzinfo=UTC),
            "updated_at": datetime.now(UTC),
            "like_count": random.randint(0, 50),
            "reply_count": random.randint(0, 10) if not parent_id else 0,
            "metadata": {
                "ip_address": self.fake.ipv4(),
                "user_agent": self.fake.user_agent(),
                "edited": random.choice([True, False]),
            },
        }

        # Apply overrides
        base_data.update(overrides)

        return base_data

    def create_thread(self, post_id: str, depth: int = 3, width: int = 2) -> List[Dict[str, Any]]:
        """Create a comment thread with nested replies.

        Args:
            post_id: ID of the post
            depth: Maximum nesting depth
            width: Number of comments per level

        Returns:
            List[Dict]: Nested comment structure
        """
        comments = []

        def create_level(parent_id: Optional[str], current_depth: int):
            if current_depth >= depth:
                return

            for _ in range(width):
                comment = self.create(post_id=post_id, parent_id=parent_id)
                comments.append(comment)

                # Create nested replies
                if random.choice([True, False]) and current_depth < depth - 1:
                    create_level(comment["id"], current_depth + 1)

        create_level(None, 0)
        return comments


class CategoryFactory(DataFactory):
    """Factory for generating category/tag test data."""

    def create(
        self, id: Optional[str] = None, parent_id: Optional[str] = None, **overrides
    ) -> Dict[str, Any]:
        """Create a category object.

        Args:
            id: Category ID (auto-generated if not provided)
            parent_id: Parent category ID for hierarchical categories
            **overrides: Field overrides

        Returns:
            Dict: Category data
        """
        category_id = id or str(uuid4())

        base_data = {
            "id": category_id,
            "name": self.fake.word().title(),
            "slug": self.fake.slug(),
            "description": self.fake.text(max_nb_chars=200),
            "parent_id": parent_id,
            "color": self.fake.hex_color(),
            "icon": f"icon-{self.fake.word()}",
            "sort_order": random.randint(1, 100),
            "is_active": True,
            "post_count": random.randint(0, 50),
            "created_at": self.fake.date_time_between(start_date="-1y", end_date="now", tzinfo=UTC),
            "updated_at": datetime.now(UTC),
        }

        # Apply overrides
        base_data.update(overrides)

        return base_data


# Time series and analytics data
class AnalyticsFactory(DataFactory):
    """Factory for generating analytics and time-series data."""

    def create_metrics(
        self,
        start_date: datetime,
        end_date: datetime,
        granularity: str = "daily",  # hourly, daily, weekly, monthly
    ) -> List[Dict[str, Any]]:
        """Create time-series metrics data.

        Args:
            start_date: Start date for metrics
            end_date: End date for metrics
            granularity: Time granularity

        Returns:
            List[Dict]: Time-series data points
        """
        data_points = []
        current = start_date

        # Determine time delta based on granularity
        deltas = {
            "hourly": timedelta(hours=1),
            "daily": timedelta(days=1),
            "weekly": timedelta(weeks=1),
            "monthly": timedelta(days=30),
        }

        delta = deltas.get(granularity, timedelta(days=1))

        while current <= end_date:
            # Generate realistic metrics with some randomness and trends
            base_views = 1000
            trend_factor = (current - start_date).days / 365  # Yearly growth trend
            seasonal_factor = 1 + 0.2 * random.sin(
                (current.timetuple().tm_yday / 365) * 2 * 3.14159
            )

            data_points.append(
                {
                    "timestamp": current,
                    "date": current.date().isoformat(),
                    "hour": current.hour if granularity == "hourly" else None,
                    "views": int(
                        base_views * (1 + trend_factor) * seasonal_factor * random.uniform(0.7, 1.3)
                    ),
                    "unique_visitors": int(
                        base_views
                        * 0.7
                        * (1 + trend_factor)
                        * seasonal_factor
                        * random.uniform(0.6, 1.2)
                    ),
                    "page_views": int(
                        base_views
                        * 1.5
                        * (1 + trend_factor)
                        * seasonal_factor
                        * random.uniform(0.8, 1.4)
                    ),
                    "bounce_rate": random.uniform(0.3, 0.7),
                    "avg_session_duration": random.randint(60, 300),  # seconds
                    "conversions": random.randint(0, 50),
                }
            )

            current += delta

        return data_points


# Database seed data generators
@pytest.fixture
def user_factory():
    """User data factory fixture."""
    return UserFactory()


@pytest.fixture
def post_factory():
    """Post data factory fixture."""
    return PostFactory()


@pytest.fixture
def comment_factory():
    """Comment data factory fixture."""
    return CommentFactory()


@pytest.fixture
def category_factory():
    """Category data factory fixture."""
    return CategoryFactory()


@pytest.fixture
def analytics_factory():
    """Analytics data factory fixture."""
    return AnalyticsFactory()


@pytest.fixture
def sample_blog_data(user_factory, post_factory, comment_factory, category_factory):
    """Complete blog data set for testing."""
    # Create users
    admin = user_factory.create_admin(username="admin", email="admin@blog.test")
    authors = user_factory.create_batch(3, role="user")
    commenters = user_factory.create_batch(10, role="user")

    # Create categories
    categories = [
        category_factory.create(name="Technology", slug="technology"),
        category_factory.create(name="Programming", slug="programming"),
        category_factory.create(name="Tutorials", slug="tutorials"),
    ]

    # Create posts
    posts = []
    for author in authors:
        author_posts = post_factory.create_batch(random.randint(2, 5), author_id=author["id"])
        posts.extend(author_posts)

    # Create comments
    comments = []
    for post in posts:
        if random.choice([True, False]):  # Not all posts have comments
            post_comments = comment_factory.create_thread(
                post_id=post["id"], depth=random.randint(1, 3), width=random.randint(1, 4)
            )
            comments.extend(post_comments)

    return {
        "admin": admin,
        "authors": authors,
        "commenters": commenters,
        "users": [admin] + authors + commenters,
        "categories": categories,
        "posts": posts,
        "comments": comments,
    }


# JSONB and complex data structures
def generate_jsonb_data(schema: Dict[str, str], randomize: bool = True) -> Dict[str, Any]:
    """Generate JSONB test data based on schema.

    Args:
        schema: Field type mapping (e.g., {"name": "string", "age": "integer"})
        randomize: Whether to randomize values

    Returns:
        Dict: Generated data matching schema
    """
    fake_instance = Faker()
    if not randomize:
        fake_instance.seed_instance(42)

    data = {}

    for field, field_type in schema.items():
        if field_type == "string":
            data[field] = fake_instance.sentence()
        elif field_type == "integer":
            data[field] = fake_instance.random_int(min=1, max=1000)
        elif field_type == "float":
            data[field] = round(fake_instance.random.uniform(0.1, 1000.0), 2)
        elif field_type == "boolean":
            data[field] = fake_instance.boolean()
        elif field_type == "date":
            data[field] = fake_instance.date().isoformat()
        elif field_type == "datetime":
            data[field] = fake_instance.date_time().isoformat()
        elif field_type == "email":
            data[field] = fake_instance.email()
        elif field_type == "url":
            data[field] = fake_instance.url()
        elif field_type == "uuid":
            data[field] = str(uuid4())
        elif field_type == "array":
            data[field] = [fake_instance.word() for _ in range(random.randint(1, 5))]
        elif field_type == "object":
            data[field] = {
                "nested_field": fake_instance.word(),
                "nested_value": fake_instance.random_int(min=1, max=100),
            }

    return data


# Sequence generators for IDs and timestamps
class SequenceGenerator:
    """Generator for sequential IDs and timestamps."""

    def __init__(self, start: int = 1):
        """Initialize sequence generator.

        Args:
            start: Starting number for sequence
        """
        self.current = start

    def next_int(self) -> int:
        """Get next integer in sequence."""
        value = self.current
        self.current += 1
        return value

    def next_str(self, prefix: str = "") -> str:
        """Get next string ID in sequence."""
        return f"{prefix}{self.next_int()}"

    def next_uuid(self) -> str:
        """Get next UUID (deterministic for testing)."""
        return str(uuid4())


@pytest.fixture
def sequence_generator():
    """Sequence generator fixture."""
    return SequenceGenerator()
