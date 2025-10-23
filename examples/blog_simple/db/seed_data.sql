-- FraiseQL Blog Simple - Sample Data
-- Seed data for development and testing

-- Disable RLS for seeding
SET row_security = off;

-- Clean existing seed data to allow re-running
DELETE FROM tb_comment WHERE id IN ('71111111-1111-1111-1111-111111111111', '72222222-2222-2222-2222-222222222222', '73333333-3333-3333-3333-333333333333', '74444444-4444-4444-4444-444444444444', '75555555-5555-5555-5555-555555555555', '76666666-6666-6666-6666-666666666666');
DELETE FROM post_tags WHERE fk_post IN (SELECT pk_post FROM tb_post WHERE id IN ('61111111-1111-1111-1111-111111111111', '62222222-2222-2222-2222-222222222222', '63333333-3333-3333-3333-333333333333', '64444444-4444-4444-4444-444444444444'));
DELETE FROM tb_post WHERE id IN ('61111111-1111-1111-1111-111111111111', '62222222-2222-2222-2222-222222222222', '63333333-3333-3333-3333-333333333333', '64444444-4444-4444-4444-444444444444');
DELETE FROM tb_tag WHERE id IN ('51111111-1111-1111-1111-111111111111', '52222222-2222-2222-2222-222222222222', '53333333-3333-3333-3333-333333333333', '54444444-4444-4444-4444-444444444444', '55555555-5555-5555-5555-555555555555', '56666666-6666-6666-6666-666666666666');
DELETE FROM tb_user WHERE id IN ('11111111-1111-1111-1111-111111111111', '22222222-2222-2222-2222-222222222222', '33333333-3333-3333-3333-333333333333', '44444444-4444-4444-4444-444444444444');

-- Insert sample users
INSERT INTO tb_user (id, identifier, email, password_hash, role, profile_data) VALUES
(
    '11111111-1111-1111-1111-111111111111'::uuid,
    'admin',
    'admin@example.com',
    '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewKynicDLWvJz.lG', -- "password"
    'admin',
    '{"first_name": "Admin", "last_name": "User", "bio": "System administrator"}'::jsonb
),
(
    '22222222-2222-2222-2222-222222222222'::uuid,
    'johndoe',
    'john@example.com',
    '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewKynicDLWvJz.lG', -- "password"
    'author',
    '{"first_name": "John", "last_name": "Doe", "bio": "Tech writer and developer", "website": "https://johndoe.com"}'::jsonb
),
(
    '33333333-3333-3333-3333-333333333333'::uuid,
    'janedoe',
    'jane@example.com',
    '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewKynicDLWvJz.lG', -- "password"
    'author',
    '{"first_name": "Jane", "last_name": "Doe", "bio": "Frontend developer and UI/UX enthusiast"}'::jsonb
),
(
    '44444444-4444-4444-4444-444444444444'::uuid,
    'reader',
    'reader@example.com',
    '$2b$12$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewKynicDLWvJz.lG', -- "password"
    'user',
    '{"first_name": "Regular", "last_name": "Reader", "bio": "Avid blog reader"}'::jsonb
);

-- Insert sample tags
INSERT INTO tb_tag (id, name, identifier, color, description) VALUES
(
    '51111111-1111-1111-1111-111111111111'::uuid,
    'GraphQL',
    'graphql',
    '#E10098',
    'GraphQL API development and best practices'
),
(
    '52222222-2222-2222-2222-222222222222'::uuid,
    'PostgreSQL',
    'postgresql',
    '#336791',
    'PostgreSQL database tips and techniques'
),
(
    '53333333-3333-3333-3333-333333333333'::uuid,
    'FraiseQL',
    'fraiseql',
    '#6366f1',
    'FraiseQL framework tutorials and examples'
),
(
    '54444444-4444-4444-4444-444444444444'::uuid,
    'Web Development',
    'web-development',
    '#f59e0b',
    'General web development topics'
),
(
    '55555555-5555-5555-5555-555555555555'::uuid,
    'Python',
    'python',
    '#3776ab',
    'Python programming language'
),
(
    '56666666-6666-6666-6666-666666666666'::uuid,
    'FastAPI',
    'fastapi',
    '#009688',
    'FastAPI framework for building APIs'
);

-- Insert sample posts
INSERT INTO tb_post (id, title, identifier, content, excerpt, fk_author, status, published_at) VALUES
(
    '61111111-1111-1111-1111-111111111111'::uuid,
    'Getting Started with FraiseQL',
    'getting-started-with-fraiseql',
    'FraiseQL is a powerful framework for building GraphQL APIs with PostgreSQL. In this comprehensive guide, we''ll explore how to create your first FraiseQL application.

## What is FraiseQL?

FraiseQL combines the best of GraphQL and PostgreSQL to create a seamless development experience. It provides:

- **Database-first architecture**: Your PostgreSQL schema drives your GraphQL API
- **Type safety**: Full type safety from database to GraphQL
- **Performance**: Optimized queries with built-in N+1 prevention
- **Developer experience**: Clean, declarative API definitions

## Installation

Getting started with FraiseQL is easy:

```bash
pip install fraiseql[fastapi]
```

## Your First Application

Here''s a simple example of a FraiseQL application:

```python
import fraiseql
from fraiseql.fastapi import create_fraiseql_app

@fraiseql.type(sql_source="users")
class User:
    id: str
    name: str
    email: str

app = create_fraiseql_app(
    database_url="postgresql://user:pass@localhost/db",
    types=[User]
)
```

## Next Steps

Now that you have a basic understanding of FraiseQL, you can:

1. Explore the documentation
2. Try the interactive examples
3. Build your first real application

Happy coding!',
    'Learn how to get started with FraiseQL, a powerful framework for building GraphQL APIs with PostgreSQL.',
    2, -- fk_author (johndoe pk_user = 2)
    'published',
    NOW() - INTERVAL '2 days'
),
(
    '62222222-2222-2222-2222-222222222222'::uuid,
    'Advanced PostgreSQL Patterns in FraiseQL',
    'advanced-postgresql-patterns-in-fraiseql',
    'PostgreSQL is more than just a database - it''s a powerful platform for building robust applications. In this article, we''ll explore advanced patterns that make FraiseQL applications shine.

## JSONB and Flexible Schemas

One of PostgreSQL''s greatest strengths is JSONB support:

```sql
CREATE TABLE posts (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    metadata JSONB DEFAULT ''''{}''''::jsonb
);
```

This allows for flexible, schema-less data alongside structured columns.

## Views for Query Optimization

FraiseQL leverages PostgreSQL views for clean separation between command and query sides:

```sql
CREATE VIEW v_post_with_author AS
SELECT
    p.*,
    jsonb_build_object(
        ''''id'''', u.id,
        ''''username'''', u.username,
        ''''profile'''', u.profile_data
    ) as author
FROM posts p
JOIN users u ON p.author_id = u.id;
```

## Triggers and Functions

Automate business logic with triggers:

```sql
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```

## Row Level Security

Implement authorization at the database level:

```sql
ALTER TABLE posts ENABLE ROW LEVEL SECURITY;

CREATE POLICY posts_visibility ON posts
    FOR SELECT USING (
        status = ''''published''''
        OR author_id = current_setting(''''app.current_user_id'''')::uuid
    );
```

These patterns form the foundation of scalable, secure FraiseQL applications.',
    'Explore advanced PostgreSQL patterns that power robust FraiseQL applications, from JSONB to Row Level Security.',
    2, -- fk_author (johndoe pk_user = 2)
    'published',
    NOW() - INTERVAL '1 day'
),
(
    '63333333-3333-3333-3333-333333333333'::uuid,
    'Building Reactive UIs with GraphQL Subscriptions',
    'building-reactive-uis-with-graphql-subscriptions',
    'Real-time updates are essential for modern web applications. FraiseQL makes it easy to implement GraphQL subscriptions for reactive user interfaces.

## What are GraphQL Subscriptions?

GraphQL subscriptions allow clients to receive real-time updates when data changes:

```graphql
subscription PostUpdates {
    postUpdates {
        mutation
        node {
            id
            title
            status
        }
    }
}
```

## Implementation with FraiseQL

FraiseQL provides decorators for easy subscription implementation:

```python
@fraiseql.subscription
async def post_updates(info: GraphQLResolveInfo):
    """Subscribe to post updates"""
    async for update in post_stream():
        yield update
```

## WebSocket Integration

FraiseQL integrates seamlessly with WebSocket for real-time communication:

```python
from fraiseql.subscriptions import WebSocketSubscriptionServer

app.add_websocket_route("/ws", WebSocketSubscriptionServer())
```

## Frontend Integration

Connect your React application:

```typescript
import { createClient } from ''''graphql-ws'''';

const client = createClient({
    url: ''''ws://localhost:8000/ws'''',
});

const subscription = client.iterate({
    query: ''''subscription { postUpdates { id title } }''''
});

for await (const result of subscription) {
    console.log(result);
}
```

## Use Cases

Subscriptions are perfect for:

- Live comments and messaging
- Real-time analytics dashboards
- Collaborative editing
- Live notifications
- Activity feeds

Start building reactive applications today with FraiseQL subscriptions!',
    'Learn how to build real-time, reactive user interfaces using GraphQL subscriptions with FraiseQL.',
    3, -- fk_author (janedoe pk_user = 3)
    'published',
    NOW() - INTERVAL '6 hours'
),
(
    '64444444-4444-4444-4444-444444444444'::uuid,
    'FraiseQL vs Other GraphQL Frameworks',
    'fraiseql-vs-other-graphql-frameworks',
    'Choosing the right GraphQL framework is crucial for project success. Let''s compare FraiseQL with other popular options and understand when to choose each.

## FraiseQL - Database-First Approach

FraiseQL''s unique selling point is its database-first approach:

**Strengths:**
- Schema driven by database structure
- Excellent PostgreSQL integration
- Built-in performance optimizations
- Type safety from database to GraphQL
- CQRS patterns built-in

**Best for:**
- PostgreSQL-based applications
- Complex business domains
- High-performance requirements
- Teams comfortable with SQL

## GraphQL-Core Python - Pure GraphQL

The reference implementation for Python:

**Strengths:**
- Maximum flexibility
- Direct GraphQL spec compliance
- Minimal overhead
- Complete control

**Best for:**
- Custom implementations
- Learning GraphQL internals
- Specific performance requirements

## Strawberry - Modern Python GraphQL

A modern, decorator-based framework:

**Strengths:**
- Clean, Pythonic syntax
- Great developer experience
- Strong typing with dataclasses
- Active development

**Best for:**
- New Python projects
- Teams preferring modern Python patterns
- Rapid prototyping

## Graphene - Mature Ecosystem

A mature framework with broad adoption:

**Strengths:**
- Large ecosystem
- Django/Flask integrations
- Proven in production
- Extensive documentation

**Best for:**
- Django applications
- Established codebases
- Teams needing ecosystem maturity

## Making the Right Choice

Choose FraiseQL when:
- You''re building on PostgreSQL
- Performance is critical
- You need CQRS patterns
- Your team thinks in database terms

Consider alternatives when:
- Using different databases
- Need maximum flexibility
- Working with existing GraphQL codebases
- Team prefers ORM-style thinking

Each framework has its place - choose based on your specific needs and constraints.',
    'A comprehensive comparison of FraiseQL with other GraphQL frameworks to help you make the right choice.',
    2, -- fk_author (johndoe pk_user = 2)
    'draft',
    NULL
);

-- Insert post-tag relationships
INSERT INTO post_tags (fk_post, fk_tag) VALUES
-- Getting Started with FraiseQL (pk_post = 1)
(1, 1), -- GraphQL (pk_tag = 1)
(1, 3), -- FraiseQL (pk_tag = 3)
(1, 4), -- Web Development (pk_tag = 4)

-- Advanced PostgreSQL Patterns (pk_post = 2)
(2, 2), -- PostgreSQL (pk_tag = 2)
(2, 3), -- FraiseQL (pk_tag = 3)
(2, 5), -- Python (pk_tag = 5)

-- Building Reactive UIs (pk_post = 3)
(3, 1), -- GraphQL (pk_tag = 1)
(3, 3), -- FraiseQL (pk_tag = 3)
(3, 4), -- Web Development (pk_tag = 4)

-- FraiseQL vs Other Frameworks (pk_post = 4)
(4, 1), -- GraphQL (pk_tag = 1)
(4, 3), -- FraiseQL (pk_tag = 3)
(4, 5); -- Python (pk_tag = 5)

-- Insert sample comments
INSERT INTO tb_comment (id, fk_post, fk_author, content, status) VALUES
(
    '71111111-1111-1111-1111-111111111111'::uuid,
    1, -- fk_post (Getting Started post pk_post = 1)
    4, -- fk_author (reader pk_user = 4)
    'Great introduction to FraiseQL! I''ve been looking for a GraphQL framework that works well with PostgreSQL. The database-first approach really appeals to me.',
    'approved'
),
(
    '72222222-2222-2222-2222-222222222222'::uuid,
    1, -- fk_post (Getting Started post pk_post = 1)
    3, -- fk_author (janedoe pk_user = 3)
    'Thanks for this tutorial! One question - how does FraiseQL handle complex joins and relationships? Looking forward to more advanced examples.',
    'approved'
),
(
    '73333333-3333-3333-3333-333333333333'::uuid,
    1, -- fk_post (Getting Started post pk_post = 1)
    2, -- fk_author (johndoe pk_user = 2)
    '@janedoe Great question! FraiseQL handles relationships through field resolvers and can optimize joins automatically. I''ll cover this in detail in an upcoming post.',
    'approved'
),
(
    '74444444-4444-4444-4444-444444444444'::uuid,
    2, -- fk_post (Advanced PostgreSQL post pk_post = 2)
    4, -- fk_author (reader pk_user = 4)
    'The JSONB examples are really helpful. I didn''t realize PostgreSQL could be so flexible while maintaining relational integrity.',
    'approved'
),
(
    '75555555-5555-5555-5555-555555555555'::uuid,
    3, -- fk_post (Reactive UIs post pk_post = 3)
    2, -- fk_author (johndoe pk_user = 2)
    'Subscriptions are such a powerful feature. The WebSocket integration looks seamless. Can''t wait to try this in my next project!',
    'approved'
),
-- Nested comment (reply)
(
    '76666666-6666-6666-6666-666666666666'::uuid,
    3, -- fk_post (Reactive UIs post pk_post = 3)
    3, -- fk_author (janedoe pk_user = 3)
    '@johndoe Definitely give it a try! The real-time updates make such a difference for user experience. Let me know if you run into any issues.',
    'approved'
);

-- Update the reply to have the correct fk_parent
UPDATE tb_comment
SET fk_parent = (SELECT pk_comment FROM tb_comment WHERE id = '75555555-5555-5555-5555-555555555555'::uuid)
WHERE id = '76666666-6666-6666-6666-666666666666'::uuid;

-- Re-enable RLS
SET row_security = on;
