"""Blog Mutations - FraiseQL Clean Patterns Showcase

Demonstrates the new clean default patterns without "Enhanced" or "Optimized" prefixes.
Shows enterprise-ready error handling with native error arrays.
"""

import uuid
from typing import List, Optional, TYPE_CHECKING
from datetime import datetime

import fraiseql
from fraiseql.errors import FraiseQLError
from fraiseql import UNSET

# Import the actual types for FraiseQL to resolve them
from .blog_types import Post, Author


# ============================================================================
# INPUT TYPES - Clean and simple
# ============================================================================

@fraiseql.input
class CreatePostInput:
    """Create post input - showcasing clean input patterns."""
    identifier: str
    title: str
    content: str
    author_identifier: str
    excerpt: Optional[str] = UNSET
    tags: List[str] = []
    status: str = "draft"


@fraiseql.input
class UpdatePostInput:
    """Update post input - partial updates with UNSET support."""
    title: Optional[str] = UNSET
    content: Optional[str] = UNSET
    excerpt: Optional[str] = UNSET
    tags: Optional[List[str]] = UNSET


@fraiseql.input
class CreateAuthorInput:
    """Create author input."""
    identifier: str
    name: str
    email: str
    bio: Optional[str] = UNSET


# ============================================================================
# SUCCESS/ERROR TYPES - Strongly Opinionated Error-as-Data Pattern
# ============================================================================

@fraiseql.type
class CreatePostSuccess:
    """Clean success response - no errors needed on success!"""
    post: Post
    message: str = "Post created successfully"  # Always present - strongly opinionated!


@fraiseql.type
class CreatePostError:
    """Error response using FraiseQL's opinionated error-as-data approach."""
    message: str
    errors: List[FraiseQLError] = []  # Always as data, never as GraphQL errors!

    # Rich error context
    duplicate_post: Optional[Post] = None
    missing_author: Optional[dict] = None
    validation_details: Optional[dict] = None


@fraiseql.type
class UpdatePostSuccess:
    """Update success with change tracking."""
    post: Post
    message: str = "Post updated successfully"
    changed_fields: List[str] = []


@fraiseql.type
class UpdatePostError:
    """Update error with detailed context."""
    message: str
    errors: List[FraiseQLError] = []
    post_not_found: Optional[dict] = None


@fraiseql.type
class PublishPostSuccess:
    """Publish success response."""
    post: Post
    message: str = "Post published successfully"
    published_at: datetime


@fraiseql.type
class PublishPostError:
    """Publish error with requirements."""
    message: str
    errors: List[FraiseQLError] = []
    requirements_not_met: Optional[dict] = None


@fraiseql.type
class CreateAuthorSuccess:
    """Author creation success."""
    author: Author
    message: str = "Author created successfully"


@fraiseql.type
class CreateAuthorError:
    """Author creation error."""
    message: str
    errors: List[FraiseQLError] = []
    duplicate_email: Optional[dict] = None


# ============================================================================
# MUTATION CLASSES - Clean Default Patterns ✨
# ============================================================================

@fraiseql.mutation(function="app.create_post")
class CreatePost:
    """Create a blog post using clean FraiseQL patterns.

    🎯 Showcases:
    - Clean @fraiseql.mutation decorator
    - Native error arrays as data (strongly opinionated!)
    - Comprehensive error context
    - Database-first validation
    """

    input: CreatePostInput
    success: CreatePostSuccess
    failure: CreatePostError


@fraiseql.mutation(function="app.update_post")
class UpdatePost:
    """Update a blog post with optimistic locking.

    🎯 Features:
    - Partial updates with UNSET support
    - Change tracking and version management
    - Native error arrays as data
    """

    input: UpdatePostInput
    success: UpdatePostSuccess
    failure: UpdatePostError


@fraiseql.mutation(function="app.publish_post")
class PublishPost:
    """Publish a post with business rule validation.

    🎯 Business Rules:
    - Post must have title and content
    - Author must exist
    - Minimum content length requirements
    - Native error arrays for all validation
    """

    input: dict  # Simple dict input for demo
    success: PublishPostSuccess
    failure: PublishPostError


@fraiseql.mutation(function="app.create_author")
class CreateAuthor:
    """Create an author with email validation.

    🎯 Validation:
    - Email format validation
    - Duplicate email detection
    - Native error arrays as data
    """

    input: CreateAuthorInput
    success: CreateAuthorSuccess
    failure: CreateAuthorError
