#!/usr/bin/env python3
"""FraiseQL Cursor-Based Pagination Demo

This example demonstrates the new @connection decorator for implementing
cursor-based pagination following the Relay specification.

🚀 Features Demonstrated:
- Basic cursor-based pagination with @fraiseql.connection
- Automatic view name inference
- Custom pagination configuration
- Integration with existing CQRS patterns
- Type-safe Connection[T] responses

Run with: uv run python examples/cursor_pagination_demo.py
"""

from typing import Any
from uuid import UUID

import fraiseql
from fraiseql import Connection


# Define your entity types
@fraiseql.type
class User:
    """User entity for pagination demo."""
    id: str
    name: str
    email: str
    created_at: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "User":
        return cls(
            id=data["id"],
            name=data["name"],
            email=data["email"],
            created_at=data["createdAt"]
        )


@fraiseql.type
class Post:
    """Post entity for pagination demo."""
    id: str
    title: str
    content: str
    user_id: str
    created_at: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Post":
        return cls(
            id=data["id"],
            title=data["title"],
            content=data["content"],
            user_id=data["userId"],
            created_at=data["createdAt"]
        )


# ================================
# Basic Connection Queries
# ================================

@fraiseql.connection(node_type=User)
@fraiseql.query
async def users_connection(
    info,
    first: int | None = None,
    after: str | None = None,
    where: dict[str, Any] | None = None,
) -> Connection[User]:
    """Basic user connection with automatic configuration.

    - View name inferred as "v_users" from function name
    - Default page size: 20
    - Max page size: 100
    - Includes total count
    - Orders by "id" field
    """
    # Implementation handled by @connection decorator


@fraiseql.connection(
    node_type=User,
    cursor_field="created_at",
    default_page_size=10,
)
@fraiseql.query
async def recent_users_connection(
    info,
    first: int | None = None,
    after: str | None = None,
    where: dict[str, Any] | None = None,
) -> Connection[User]:
    """Recent users ordered by creation date.

    - Uses created_at field for cursor ordering
    - Smaller default page size (10 items)
    - Perfect for "recently joined" sections
    """


# ================================
# Advanced Connection Queries
# ================================

@fraiseql.connection(
    node_type=Post,
    view_name="v_published_posts",  # Custom view name
    default_page_size=25,
    max_page_size=50,
    cursor_field="created_at"
)
@fraiseql.query
async def published_posts_connection(
    info,
    first: int | None = None,
    after: str | None = None,
    last: int | None = None,
    before: str | None = None,
    where: dict[str, Any] | None = None,
) -> Connection[Post]:
    """Published posts with custom configuration.

    - Custom view name for published posts only
    - Larger page sizes for content feeds
    - Supports both forward and backward pagination
    - Time-based ordering for chronological feeds
    """


@fraiseql.connection(
    node_type=Post,
    view_name="v_user_posts",
    cursor_field="created_at",
    include_total_count=False,  # Optimize performance
    max_page_size=20
)
@fraiseql.query
async def user_posts_connection(
    info,
    user_id: UUID,
    first: int | None = None,
    after: str | None = None,
    where: dict[str, Any] | None = None,
) -> Connection[Post]:
    """Posts by a specific user with performance optimizations.

    - Excludes total count for better performance
    - Custom filtering by user_id parameter
    - Smaller max page size for mobile-friendly loading
    """
    # The where filter will automatically include user_id filtering
    # You can add custom business logic here if needed


# ================================
# GraphQL Query Examples
# ================================

EXAMPLE_QUERIES = """
# Basic pagination - first page
query GetUsers {
  usersConnection(first: 20) {
    edges {
      node {
        id
        name
        email
        createdAt
      }
      cursor
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
    totalCount
  }
}

# Pagination with filters
query GetActiveUsers {
  usersConnection(
    first: 10,
    where: {status: {equals: "active"}}
  ) {
    edges {
      node {
        id
        name
        email
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
    totalCount
  }
}

# Forward pagination (next page)
query GetNextUsers($after: String!) {
  usersConnection(first: 20, after: $after) {
    edges {
      node {
        id
        name
        email
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}

# Backward pagination (previous page)
query GetPreviousUsers($before: String!) {
  usersConnection(last: 20, before: $before) {
    edges {
      node {
        id
        name
        email
      }
      cursor
    }
    pageInfo {
      hasPreviousPage
      startCursor
    }
  }
}

# Recent posts with time-based cursors
query GetRecentPosts {
  publishedPostsConnection(
    first: 25,
    orderBy: [{createdAt: DESC}]
  ) {
    edges {
      node {
        id
        title
        content
        createdAt
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
    totalCount
  }
}

# User-specific posts
query GetUserPosts($userId: UUID!, $after: String) {
  userPostsConnection(
    userId: $userId,
    first: 10,
    after: $after
  ) {
    edges {
      node {
        id
        title
        content
        createdAt
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
    # Note: no totalCount for performance
  }
}
"""


# ================================
# Frontend Integration Examples
# ================================

VUE_EXAMPLE = """
<!-- Vue.js + Apollo Client Integration -->
<template>
  <div>
    <!-- User list with infinite scroll -->
    <div v-for="edge in users.edges" :key="edge.node.id">
      <UserCard :user="edge.node" />
    </div>

    <!-- Load more button -->
    <button
      v-if="users.pageInfo.hasNextPage"
      @click="loadMore"
      :disabled="loading"
    >
      Load More
    </button>

    <!-- Total count display -->
    <p v-if="users.totalCount">
      Showing {{ users.edges.length }} of {{ users.totalCount }} users
    </p>
  </div>
</template>

<script>
import { useQuery } from '@vue/apollo-composable'
import { gql } from '@apollo/client/core'

const USERS_CONNECTION = gql`
  query GetUsers($first: Int!, $after: String) {
    usersConnection(first: $first, after: $after) {
      edges {
        node {
          id
          name
          email
          createdAt
        }
        cursor
      }
      pageInfo {
        hasNextPage
        endCursor
      }
      totalCount
    }
  }
`

export default {
  setup() {
    const { result, loading, fetchMore } = useQuery(USERS_CONNECTION, {
      first: 20
    })

    const users = computed(() => result.value?.usersConnection || { edges: [] })

    const loadMore = () => {
      if (users.value.pageInfo.hasNextPage) {
        fetchMore({
          variables: {
            after: users.value.pageInfo.endCursor
          },
          updateQuery: (prev, { fetchMoreResult }) => {
            if (!fetchMoreResult) return prev

            return {
              usersConnection: {
                ...fetchMoreResult.usersConnection,
                edges: [
                  ...prev.usersConnection.edges,
                  ...fetchMoreResult.usersConnection.edges
                ]
              }
            }
          }
        })
      }
    }

    return { users, loading, loadMore }
  }
}
</script>
"""


if __name__ == "__main__":
    print("🚀 FraiseQL Cursor-Based Pagination Demo")
    print("=" * 50)
    print(f"GraphQL Query Examples:\\n{EXAMPLE_QUERIES}")
    print(f"Vue.js Integration Example:\\n{VUE_EXAMPLE}")
    print("\\n✅ @connection decorator provides:")
    print("  • Automatic cursor-based pagination")
    print("  • Relay specification compliance")
    print("  • Type-safe Connection[T] responses")
    print("  • Performance optimizations")
    print("  • Easy GraphQL integration")
    print("\\n🎯 Perfect for modern web applications with infinite scroll!")
