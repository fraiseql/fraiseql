# Depth Protection

FraiseQL provides structural protection against GraphQL depth attacks by defining maximum recursion depth at the database level. Unlike traditional GraphQL frameworks that require runtime query analysis and complexity limits, FraiseQL's view-based architecture makes deep queries structurally impossible.

## View-Defined Depth Limits

### Structural Protection, Not Runtime Checks

Traditional GraphQL depth protection requires middleware to analyze queries at runtime:

```python
# Traditional approach: Runtime analysis
def depth_limit_middleware(query, max_depth=5):
    if calculate_depth(query) > max_depth:
        raise GraphQLDepthError("Query too deep")

# Still allows crafting deep queries that get rejected
query {
  users {
    posts {
      comments {
        author {
          posts {  # This gets blocked at runtime
            comments {
              author {
                # ... more nesting
              }
            }
          }
        }
      }
    }
  }
}
```

**FraiseQL Approach:** Depth limits are defined in the database view structure itself:

```sql
-- View defines maximum depth structurally
CREATE VIEW user_posts AS
SELECT
    u.id,
    u.name,
    -- Posts with limited comment depth
    jsonb_agg(
        jsonb_build_object(
            'id', p.id,
            'title', p.title,
            -- Comments limited to 2 levels deep
            'comments', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', c.id,
                        'text', c.text,
                        -- No deeper nesting allowed
                        'author', jsonb_build_object('name', cu.name)
                    )
                )
                FROM comments c
                JOIN users cu ON c.user_id = cu.id
                WHERE c.post_id = p.id
                LIMIT 10  -- Also limit comment count
            )
        )
    ) as posts
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
GROUP BY u.id, u.name;
```

### Attackers Cannot Exceed Defined Limits

The view structure makes it impossible to query deeper than what's defined:

```graphql
# This query works - within view limits
query {
  users {
    posts {
      comments {
        author {
          name  # Limited to this depth
        }
      }
    }
  }
}

# This query fails at schema level - field doesn't exist
query {
  users {
    posts {
      comments {
        author {
          posts {  # ❌ Field not in view
            comments {
              # Cannot go deeper
            }
          }
        }
      }
    }
  }
}
```

## No Query Complexity Middleware Needed

### Traditional Complexity Analysis

Most GraphQL frameworks require complex middleware to prevent abuse:

```python
# Complexity calculation middleware
def complexity_middleware(query):
    complexity = calculate_complexity(query)
    if complexity > MAX_COMPLEXITY:
        raise GraphQLComplexityError()

# Field cost calculation
FIELD_COSTS = {
    'User': 1,
    'Post': 2,
    'Comment': 3,
}

def calculate_complexity(node, multipliers=1):
    cost = FIELD_COSTS.get(node.name, 1)
    for child in node.children:
        cost += calculate_complexity(child, multipliers)
    return cost * multipliers
```

### FraiseQL: Structural Limits

No middleware needed - the database view enforces limits:

```sql
-- View with built-in limits
CREATE VIEW limited_user_data AS
SELECT
    id,
    name,
    -- Limited posts per user
    (SELECT jsonb_agg(
        jsonb_build_object(
            'id', p.id,
            'title', p.title,
            -- Limited comments per post
            'recent_comments', (
                SELECT jsonb_agg(
                    jsonb_build_object('id', c.id, 'text', c.text)
                )
                FROM comments c
                WHERE c.post_id = p.id
                ORDER BY c.created_at DESC
                LIMIT 5  -- Max 5 comments per post
            )
        )
    )
    FROM posts p
    WHERE p.user_id = users.id
    ORDER BY p.created_at DESC
    LIMIT 10  -- Max 10 posts per user
    ) as posts
FROM users;
```

## GraphQL Schema Enforces View Limits

### Automatic Schema Generation

FraiseQL generates GraphQL schemas that match view structure exactly:

```python
# Schema reflects view limitations
class User(BaseModel):
    id: int
    name: str
    posts: List[Post]  # Limited to 10 posts

class Post(BaseModel):
    id: int
    title: str
    recent_comments: List[Comment]  # Limited to 5 comments

class Comment(BaseModel):
    id: int
    text: str
    # No deeper relationships possible
```

### Compile-Time Safety

TypeScript/Python type systems prevent deep queries:

```typescript
// Type-safe client prevents deep queries
const query = gql`
  query GetUsers {
    users {
      posts {
        recentComments {
          text
          // Cannot access author.posts - not in schema
        }
      }
    }
  }
`;
```

## Advanced Depth Control Patterns

### Contextual Depth Limits

Different views for different contexts with appropriate depths:

```sql
-- Shallow view for lists
CREATE VIEW user_list AS
SELECT id, name FROM users;

-- Medium depth for profiles
CREATE VIEW user_profile AS
SELECT
    u.id, u.name, u.bio,
    jsonb_agg(jsonb_build_object('id', p.id, 'title', p.title)) as posts
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
GROUP BY u.id, u.name, u.bio;

-- Deep view for detailed analysis (admin only)
CREATE VIEW user_detailed AS
SELECT
    u.*,
    jsonb_agg(
        jsonb_build_object(
            'id', p.id,
            'title', p.title,
            'comments', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', c.id,
                        'text', c.text,
                        'author', jsonb_build_object(
                            'id', cu.id,
                            'name', cu.name,
                            'posts', (
                                SELECT jsonb_agg(jsonb_build_object('title', cp.title))
                                FROM posts cp
                                WHERE cp.user_id = cu.id
                                LIMIT 3
                            )
                        )
                    )
                )
                FROM comments c
                JOIN users cu ON c.user_id = cu.id
                WHERE c.post_id = p.id
            )
        )
    ) as posts
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
GROUP BY u.id;
```

### Pagination-Based Depth Control

Use pagination to limit depth indirectly:

```sql
-- Paginated relationships prevent deep traversal
CREATE VIEW posts_paginated AS
SELECT
    p.id,
    p.title,
    p.content,
    -- Limited comments with pagination info
    jsonb_build_object(
        'items', (
            SELECT jsonb_agg(
                jsonb_build_object('id', c.id, 'text', c.text)
            )
            FROM comments c
            WHERE c.post_id = p.id
            ORDER BY c.created_at DESC
            LIMIT 10
            OFFSET 0
        ),
        'has_more', (
            SELECT count(*) > 10
            FROM comments c
            WHERE c.post_id = p.id
        ),
        'total_count', (
            SELECT count(*)
            FROM comments c
            WHERE c.post_id = p.id
        )
    ) as comments
FROM posts p;
```

## Migration from Runtime Protection

### Step 1: Analyze Current Query Patterns

Identify your most complex queries and their depth requirements:

```graphql
# Analyze this query's depth requirements
query UserDashboard {
  users(limit: 10) {
    posts(limit: 5) {
      comments(limit: 3) {
        author {
          name  # What's the maximum depth needed?
        }
      }
    }
  }
}
```

### Step 2: Design Views with Appropriate Limits

Create views that match your business requirements:

```sql
-- View designed for dashboard use case
CREATE VIEW user_dashboard AS
SELECT
    u.id, u.name,
    -- Exactly 5 posts per user
    (SELECT jsonb_agg(
        jsonb_build_object(
            'id', p.id,
            'title', p.title,
            'content', p.content,
            -- Exactly 3 comments per post
            'comments', (
                SELECT jsonb_agg(
                    jsonb_build_object(
                        'id', c.id,
                        'text', c.text,
                        'author', jsonb_build_object('name', cu.name)
                    )
                )
                FROM comments c
                JOIN users cu ON c.user_id = cu.id
                WHERE c.post_id = p.id
                ORDER BY c.created_at DESC
                LIMIT 3
            )
        )
    )
    FROM posts p
    WHERE p.user_id = u.id
    ORDER BY p.created_at DESC
    LIMIT 5
    ) as posts
FROM users u
ORDER BY u.created_at DESC
LIMIT 10;
```

### Step 3: Remove Runtime Middleware

Once views enforce limits, remove complexity middleware:

```python
# Before: Runtime protection
app.add_middleware(GraphQLComplexityMiddleware, max_complexity=100)

# After: Structural protection via views
# No middleware needed - database enforces limits
```

## Security Benefits

### DDoS Protection

Structural limits prevent attackers from crafting expensive queries:

**❌ Traditional GraphQL:**
```graphql
# Expensive query possible (gets blocked by middleware)
query Attack {
  users {
    posts {
      comments {
        author {
          posts {
            comments {
              author {
                # Very expensive traversal
              }
            }
          }
        }
      }
    }
  }
}
```

**✅ FraiseQL:**
```graphql
# Impossible to craft - schema doesn't allow it
query {
  users {
    posts {
      comments {
        author {
          name  # Limited to this depth
        }
      }
    }
  }
}
```

### Predictable Performance

View-defined limits ensure consistent query performance:

- No expensive queries possible
- Resource usage is bounded
- Performance testing covers all possible query shapes
- Scaling calculations are deterministic

This approach provides superior protection compared to runtime analysis while maintaining excellent developer experience.
