---
← [Multi-tenancy](multi-tenancy.md) | [Advanced Topics](index.md) | [Next: Performance](performance.md) →
---

# Bounded Contexts

> **In this section:** Implement Domain-Driven Design bounded contexts with FraiseQL
> **Prerequisites:** Understanding of [DDD patterns](database-api-patterns.md) and [CQRS](cqrs.md)
> **Time to complete:** 25 minutes

Bounded contexts help organize large FraiseQL applications by creating clear boundaries between different business domains.

## Context Definition

### User Management Context
```python
# contexts/user_management/types.py
from fraiseql import type as fraise_type, ID
from datetime import datetime

@fraise_type
class User:
    id: ID
    email: str
    name: str
    created_at: datetime
    is_active: bool

@fraise_type
class UserProfile:
    user_id: ID
    avatar_url: str | None
    bio: str | None
    preferences: dict
```

### Content Context
```python
# contexts/content/types.py
from fraiseql import type as fraise_type, ID
from datetime import datetime

@fraise_type
class Post:
    id: ID
    title: str
    content: str
    author_id: ID  # Reference to User context
    published_at: datetime | None
    status: str

@fraise_type
class Comment:
    id: ID
    content: str
    post_id: ID
    author_id: ID  # Reference to User context
    created_at: datetime
```

### Analytics Context
```python
# contexts/analytics/types.py
from fraiseql import type as fraise_type, ID
from datetime import datetime

@fraise_type
class PostAnalytics:
    post_id: ID
    view_count: int
    engagement_score: float
    last_viewed: datetime

@fraise_type
class UserEngagement:
    user_id: ID
    total_posts: int
    total_comments: int
    avg_engagement: float
```

## Schema Organization

### Context-Specific Schemas
```sql
-- User Management Context
CREATE SCHEMA user_mgmt;

CREATE TABLE user_mgmt.tb_user (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    is_active BOOLEAN DEFAULT TRUE
);

CREATE TABLE user_mgmt.tb_user_profile (
    user_id UUID PRIMARY KEY REFERENCES user_mgmt.tb_user(id),
    avatar_url TEXT,
    bio TEXT,
    preferences JSONB DEFAULT '{}'
);

-- Content Context
CREATE SCHEMA content;

CREATE TABLE content.tb_post (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    author_id UUID NOT NULL, -- References user_mgmt.tb_user
    status TEXT DEFAULT 'draft',
    created_at TIMESTAMP DEFAULT NOW(),
    published_at TIMESTAMP
);

CREATE TABLE content.tb_comment (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content TEXT NOT NULL,
    post_id UUID NOT NULL REFERENCES content.tb_post(id),
    author_id UUID NOT NULL, -- References user_mgmt.tb_user
    created_at TIMESTAMP DEFAULT NOW()
);

-- Analytics Context
CREATE SCHEMA analytics;

CREATE TABLE analytics.tb_post_stats (
    post_id UUID PRIMARY KEY, -- References content.tb_post
    view_count INTEGER DEFAULT 0,
    like_count INTEGER DEFAULT 0,
    comment_count INTEGER DEFAULT 0,
    engagement_score NUMERIC(5,2) DEFAULT 0.0,
    last_updated TIMESTAMP DEFAULT NOW()
);
```

### Context Views
```sql
-- User Management Views
CREATE VIEW user_mgmt.v_user AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'created_at', created_at,
        'is_active', is_active
    ) AS data
FROM user_mgmt.tb_user;

CREATE VIEW user_mgmt.v_user_with_profile AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'name', u.name,
        'profile', COALESCE(
            jsonb_build_object(
                'avatar_url', p.avatar_url,
                'bio', p.bio,
                'preferences', p.preferences
            ),
            '{}'::jsonb
        )
    ) AS data
FROM user_mgmt.tb_user u
LEFT JOIN user_mgmt.tb_user_profile p ON u.id = p.user_id;

-- Content Views
CREATE VIEW content.v_post AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'title', title,
        'content', content,
        'author_id', author_id,
        'status', status,
        'created_at', created_at,
        'published_at', published_at
    ) AS data
FROM content.tb_post;

-- Cross-context view (User + Content)
CREATE VIEW content.v_post_with_author AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', jsonb_build_object(
            'id', u.id,
            'name', u.name
        ),
        'created_at', p.created_at
    ) AS data
FROM content.tb_post p
JOIN user_mgmt.tb_user u ON p.author_id = u.id;
```

## Context Repositories

### Base Context Repository
```python
from abc import ABC, abstractmethod
from fraiseql.repository import FraiseQLRepository

class ContextRepository(ABC):
    def __init__(self, base_repo: FraiseQLRepository, schema: str):
        self.repo = base_repo
        self.schema = schema

    def _qualified_name(self, name: str) -> str:
        """Get schema-qualified name"""
        return f"{self.schema}.{name}"

    async def find(self, view_name: str, **kwargs):
        """Find records in context schema"""
        qualified_view = self._qualified_name(view_name)
        return await self.repo.find(qualified_view, **kwargs)

    async def find_one(self, view_name: str, **kwargs):
        """Find single record in context schema"""
        qualified_view = self._qualified_name(view_name)
        return await self.repo.find_one(qualified_view, **kwargs)

    async def call_function(self, function_name: str, **kwargs):
        """Call function in context schema"""
        qualified_function = self._qualified_name(function_name)
        return await self.repo.call_function(qualified_function, **kwargs)
```

### User Management Repository
```python
class UserManagementRepository(ContextRepository):
    def __init__(self, base_repo: FraiseQLRepository):
        super().__init__(base_repo, "user_mgmt")

    async def get_user(self, user_id: str) -> dict | None:
        """Get user by ID"""
        return await self.find_one("v_user", where={"id": user_id})

    async def get_user_with_profile(self, user_id: str) -> dict | None:
        """Get user with profile data"""
        return await self.find_one("v_user_with_profile", where={"id": user_id})

    async def create_user(self, email: str, name: str, password_hash: str) -> str:
        """Create new user"""
        return await self.call_function(
            "fn_create_user",
            p_email=email,
            p_name=name,
            p_password_hash=password_hash
        )

    async def update_profile(self, user_id: str, profile_data: dict) -> bool:
        """Update user profile"""
        return await self.call_function(
            "fn_update_user_profile",
            p_user_id=user_id,
            p_profile_data=profile_data
        )
```

### Content Repository
```python
class ContentRepository(ContextRepository):
    def __init__(self, base_repo: FraiseQLRepository):
        super().__init__(base_repo, "content")

    async def get_post(self, post_id: str) -> dict | None:
        """Get post by ID"""
        return await self.find_one("v_post", where={"id": post_id})

    async def get_posts_by_author(self, author_id: str) -> list[dict]:
        """Get posts by author"""
        return await self.find("v_post", where={"author_id": author_id})

    async def get_post_with_author(self, post_id: str) -> dict | None:
        """Get post with author information (cross-context)"""
        return await self.find_one("v_post_with_author", where={"id": post_id})

    async def create_post(self, title: str, content: str, author_id: str) -> str:
        """Create new post"""
        return await self.call_function(
            "fn_create_post",
            p_title=title,
            p_content=content,
            p_author_id=author_id
        )
```

### Analytics Repository
```python
class AnalyticsRepository(ContextRepository):
    def __init__(self, base_repo: FraiseQLRepository):
        super().__init__(base_repo, "analytics")

    async def get_post_analytics(self, post_id: str) -> dict | None:
        """Get analytics for specific post"""
        return await self.find_one("v_post_analytics", where={"post_id": post_id})

    async def increment_view_count(self, post_id: str) -> bool:
        """Increment view count for post"""
        return await self.call_function("fn_increment_view_count", p_post_id=post_id)

    async def get_user_engagement(self, user_id: str) -> dict | None:
        """Get user engagement metrics"""
        return await self.find_one("v_user_engagement", where={"user_id": user_id})
```

## Context Integration

### Context Manager
```python
from typing import Dict
from fraiseql.repository import FraiseQLRepository

class BoundedContextManager:
    def __init__(self, base_repo: FraiseQLRepository):
        self.base_repo = base_repo
        self._contexts: Dict[str, ContextRepository] = {}

        # Initialize contexts
        self._contexts["user_mgmt"] = UserManagementRepository(base_repo)
        self._contexts["content"] = ContentRepository(base_repo)
        self._contexts["analytics"] = AnalyticsRepository(base_repo)

    def get_context(self, context_name: str) -> ContextRepository:
        """Get specific bounded context"""
        if context_name not in self._contexts:
            raise ValueError(f"Unknown context: {context_name}")
        return self._contexts[context_name]

    @property
    def user_mgmt(self) -> UserManagementRepository:
        return self._contexts["user_mgmt"]

    @property
    def content(self) -> ContentRepository:
        return self._contexts["content"]

    @property
    def analytics(self) -> AnalyticsRepository:
        return self._contexts["analytics"]
```

### Context-Aware Resolvers
```python
# User Management Context Resolvers
@fraiseql.query
async def user(info, id: ID) -> User | None:
    """Get user (User Management context)"""
    contexts = info.context["contexts"]

    result = await contexts.user_mgmt.get_user(id)
    return User(**result) if result else None

@fraiseql.query
async def user_with_profile(info, id: ID) -> UserProfile | None:
    """Get user with profile (User Management context)"""
    contexts = info.context["contexts"]

    result = await contexts.user_mgmt.get_user_with_profile(id)
    return UserProfile(**result) if result else None

# Content Context Resolvers
@fraiseql.query
async def post(info, id: ID) -> Post | None:
    """Get post (Content context)"""
    contexts = info.context["contexts"]

    result = await contexts.content.get_post(id)
    return Post(**result) if result else None

@fraiseql.query
async def post_with_author(info, id: ID) -> PostWithAuthor | None:
    """Get post with author (cross-context)"""
    contexts = info.context["contexts"]

    result = await contexts.content.get_post_with_author(id)
    return PostWithAuthor(**result) if result else None

# Analytics Context Resolvers
@fraiseql.query
async def post_analytics(info, post_id: ID) -> PostAnalytics | None:
    """Get post analytics (Analytics context)"""
    contexts = info.context["contexts"]

    result = await contexts.analytics.get_post_analytics(post_id)
    return PostAnalytics(**result) if result else None
```

## Cross-Context Communication

### Domain Events
```sql
-- Domain events table (shared across contexts)
CREATE TABLE public.tb_domain_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,
    source_context TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    event_data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    processed_at TIMESTAMP
);
```

### Event Publishing
```python
class DomainEventPublisher:
    def __init__(self, repo: FraiseQLRepository):
        self.repo = repo

    async def publish_event(
        self,
        event_type: str,
        source_context: str,
        aggregate_id: str,
        event_data: dict
    ) -> str:
        """Publish domain event"""
        return await self.repo.call_function(
            "fn_publish_domain_event",
            p_event_type=event_type,
            p_source_context=source_context,
            p_aggregate_id=aggregate_id,
            p_event_data=event_data
        )

# Usage in mutations
@fraiseql.mutation
async def create_post(info, title: str, content: str) -> Post:
    """Create post and publish event"""
    contexts = info.context["contexts"]
    publisher = info.context["event_publisher"]
    user = info.context["user"]

    # Create post in Content context
    post_id = await contexts.content.create_post(title, content, user.id)

    # Publish domain event
    await publisher.publish_event(
        event_type="POST_CREATED",
        source_context="content",
        aggregate_id=post_id,
        event_data={
            "title": title,
            "author_id": user.id,
            "created_at": datetime.now().isoformat()
        }
    )

    result = await contexts.content.get_post(post_id)
    return Post(**result)
```

### Event Handlers
```python
class AnalyticsEventHandler:
    def __init__(self, analytics_repo: AnalyticsRepository):
        self.analytics = analytics_repo

    async def handle_post_created(self, event_data: dict):
        """Handle POST_CREATED event"""
        post_id = event_data["aggregate_id"]

        # Initialize analytics for new post
        await self.analytics.call_function(
            "fn_initialize_post_analytics",
            p_post_id=post_id
        )

    async def handle_post_viewed(self, event_data: dict):
        """Handle POST_VIEWED event"""
        post_id = event_data["post_id"]

        # Increment view count
        await self.analytics.increment_view_count(post_id)

# Event processor
async def process_domain_events():
    """Background task to process domain events"""
    contexts = get_bounded_contexts()
    event_handler = AnalyticsEventHandler(contexts.analytics)

    # Get unprocessed events
    events = await contexts.base_repo.find(
        "tb_domain_events",
        where={"processed_at": None},
        order_by="created_at"
    )

    for event in events:
        try:
            if event["event_type"] == "POST_CREATED":
                await event_handler.handle_post_created(event)
            elif event["event_type"] == "POST_VIEWED":
                await event_handler.handle_post_viewed(event)

            # Mark as processed
            await contexts.base_repo.execute(
                "UPDATE tb_domain_events SET processed_at = NOW() WHERE id = $1",
                event["id"]
            )

        except Exception as e:
            logger.error(f"Failed to process event {event['id']}: {e}")
```

## Context Boundaries

### Anti-Corruption Layer
```python
class UserManagementAdapter:
    """Adapter for User Management context"""

    def __init__(self, user_repo: UserManagementRepository):
        self.user_repo = user_repo

    async def get_author_info(self, author_id: str) -> dict:
        """Get author information for Content context"""
        user = await self.user_repo.get_user(author_id)
        if not user:
            return {"id": author_id, "name": "Unknown User", "is_active": False}

        # Transform to Content context's author model
        return {
            "id": user["id"],
            "name": user["name"],
            "is_active": user["is_active"]
        }

# Usage in Content context
class ContentService:
    def __init__(self, content_repo: ContentRepository, user_adapter: UserManagementAdapter):
        self.content_repo = content_repo
        self.user_adapter = user_adapter

    async def get_enriched_post(self, post_id: str) -> dict:
        """Get post with author information"""
        post = await self.content_repo.get_post(post_id)
        if not post:
            return None

        # Get author info through adapter
        author = await self.user_adapter.get_author_info(post["author_id"])

        return {
            **post,
            "author": author
        }
```

### Interface Segregation
```python
# Define interfaces for cross-context dependencies
from abc import ABC, abstractmethod

class AuthorProvider(ABC):
    @abstractmethod
    async def get_author_info(self, author_id: str) -> dict:
        pass

class PostProvider(ABC):
    @abstractmethod
    async def get_post_info(self, post_id: str) -> dict:
        pass

# Implementations
class UserManagementAuthorProvider(AuthorProvider):
    def __init__(self, user_repo: UserManagementRepository):
        self.user_repo = user_repo

    async def get_author_info(self, author_id: str) -> dict:
        return await self.user_repo.get_user(author_id)

class ContentPostProvider(PostProvider):
    def __init__(self, content_repo: ContentRepository):
        self.content_repo = content_repo

    async def get_post_info(self, post_id: str) -> dict:
        return await self.content_repo.get_post(post_id)
```

## Testing Bounded Contexts

### Context-Specific Tests
```python
import pytest
from tests.fixtures import get_test_contexts

@pytest.mark.asyncio
class TestUserManagementContext:
    async def test_create_user(self):
        """Test user creation in User Management context"""
        contexts = await get_test_contexts()

        user_id = await contexts.user_mgmt.create_user(
            email="test@example.com",
            name="Test User",
            password_hash="hashed"
        )

        user = await contexts.user_mgmt.get_user(user_id)
        assert user["email"] == "test@example.com"

@pytest.mark.asyncio
class TestCrossContextIntegration:
    async def test_post_with_author(self):
        """Test cross-context data integration"""
        contexts = await get_test_contexts()

        # Create user in User Management context
        user_id = await contexts.user_mgmt.create_user(
            email="author@example.com",
            name="Author",
            password_hash="hashed"
        )

        # Create post in Content context
        post_id = await contexts.content.create_post(
            title="Test Post",
            content="Content",
            author_id=user_id
        )

        # Get enriched post (cross-context)
        post_with_author = await contexts.content.get_post_with_author(post_id)

        assert post_with_author["author"]["name"] == "Author"
```

## Best Practices

### Context Design
- Keep contexts loosely coupled
- Define clear interfaces between contexts
- Use domain events for cross-context communication
- Avoid direct database access across contexts

### Data Consistency
- Use eventual consistency for cross-context operations
- Implement compensating actions for failures
- Monitor cross-context data integrity
- Use sagas for complex multi-context transactions

### Performance
- Optimize cross-context queries with materialized views
- Cache frequently accessed cross-context data
- Consider data duplication for performance-critical paths
- Monitor query patterns across contexts

## See Also

### Related Concepts
- [**Domain-Driven Design**](database-api-patterns.md) - DDD fundamentals
- [**CQRS Implementation**](cqrs.md) - Context separation patterns
- [**Event Sourcing**](event-sourcing.md) - Cross-context events

### Implementation
- [**Architecture Overview**](../core-concepts/architecture.md) - System design
- [**Database Views**](../core-concepts/database-views.md) - View organization
- [**Testing**](../testing/integration-testing.md) - Context testing

### Advanced Topics
- [**Multi-tenancy**](multi-tenancy.md) - Tenant-aware contexts
- [**Performance**](performance.md) - Context optimization
- [**Security**](security.md) - Context-level security
