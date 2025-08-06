"""FraiseQL Quick Start Example - pgGit Demo

A Git-like version control system for PostgreSQL using FraiseQL

This example shows how to create a GraphQL API with FraiseQL that provides
Git-like functionality for PostgreSQL database version control.
"""

from datetime import UTC, datetime
from typing import List, Optional
from uuid import uuid4

import fraiseql
from fraiseql import fraise_field

# First, let's define our GraphQL types using FraiseQL decorators


@fraiseql.type
class Commit:
    """A database commit representing a point-in-time state"""

    hash: str = fraise_field(description="Unique commit hash")
    message: str = fraise_field(description="Commit message")
    author: str = fraise_field(description="Author email")
    timestamp: datetime = fraise_field(description="When the commit was created")
    parent_hash: Optional[str] = fraise_field(description="Parent commit hash")


@fraiseql.type
class Branch:
    """A branch pointing to a specific commit"""

    name: str = fraise_field(description="Branch name")
    commit_hash: str = fraise_field(description="Current commit hash")
    created_at: datetime
    updated_at: datetime


@fraiseql.type
class Tag:
    """A tag marking a specific commit"""

    name: str = fraise_field(description="Tag name")
    commit_hash: str = fraise_field(description="Tagged commit hash")
    message: Optional[str] = fraise_field(description="Tag annotation")
    created_at: datetime


# Input types for mutations
@fraiseql.input
class CreateCommitInput:
    message: str
    author: str
    parent_hash: Optional[str] = None


@fraiseql.input
class CreateBranchInput:
    name: str
    commit_hash: str


@fraiseql.input
class CreateTagInput:
    name: str
    commit_hash: str
    message: Optional[str] = None


# Query functions
@fraiseql.query
async def commits(info, limit: int = 100) -> List[Commit]:
    """Get recent commits"""
    # In a real app, this would query your database
    # For demo, returning mock data
    return [
        Commit(
            hash="abc123",
            message="Initial commit",
            author="dev@example.com",
            timestamp=datetime.now(tz=UTC),
            parent_hash=None,
        ),
        Commit(
            hash="def456",
            message="Add user authentication",
            author="dev@example.com",
            timestamp=datetime.now(tz=UTC),
            parent_hash="abc123",
        ),
    ]


@fraiseql.query
async def commit(info, hash: str) -> Optional[Commit]:
    """Get a specific commit by hash"""
    # Mock implementation
    if hash == "abc123":
        return Commit(
            hash="abc123",
            message="Initial commit",
            author="dev@example.com",
            timestamp=datetime.now(tz=UTC),
            parent_hash=None,
        )
    return None


@fraiseql.query
async def branches(info) -> List[Branch]:
    """Get all branches"""
    return [
        Branch(
            name="main",
            commit_hash="def456",
            created_at=datetime.now(tz=UTC),
            updated_at=datetime.now(tz=UTC),
        ),
        Branch(
            name="develop",
            commit_hash="abc123",
            created_at=datetime.now(tz=UTC),
            updated_at=datetime.now(tz=UTC),
        ),
    ]


@fraiseql.query
async def branch(info, name: str) -> Optional[Branch]:
    """Get a specific branch"""
    if name == "main":
        return Branch(
            name="main",
            commit_hash="def456",
            created_at=datetime.now(tz=UTC),
            updated_at=datetime.now(tz=UTC),
        )
    return None


@fraiseql.query
async def tags(info) -> List[Tag]:
    """Get all tags"""
    return [
        Tag(
            name="v1.0.0",
            commit_hash="abc123",
            message="First stable release",
            created_at=datetime.now(tz=UTC),
        ),
    ]


# Mutations
@fraiseql.mutation
async def create_commit(info, input: CreateCommitInput) -> Commit:
    """Create a new commit"""
    # Generate a simple hash for demo
    commit_hash = str(uuid4())[:8]

    return Commit(
        hash=commit_hash,
        message=input.message,
        author=input.author,
        timestamp=datetime.now(tz=UTC),
        parent_hash=input.parent_hash,
    )


@fraiseql.mutation
async def create_branch(info, input: CreateBranchInput) -> Branch:
    """Create a new branch"""
    return Branch(
        name=input.name,
        commit_hash=input.commit_hash,
        created_at=datetime.now(tz=UTC),
        updated_at=datetime.now(tz=UTC),
    )


@fraiseql.mutation
async def create_tag(info, input: CreateTagInput) -> Tag:
    """Create a new tag"""
    return Tag(
        name=input.name,
        commit_hash=input.commit_hash,
        message=input.message,
        created_at=datetime.now(tz=UTC),
    )


# Create the FraiseQL app
if __name__ == "__main__":
    import uvicorn

    # This is the correct way to create a FraiseQL app
    # NOT fraiseql.build_schema() which doesn't exist
    app = fraiseql.create_fraiseql_app(
        # Database URL is optional for this demo
        # In production, use: database_url="postgresql://user:pass@localhost/dbname"
        database_url=None,
        # Register your types
        types=[Commit, Branch, Tag],
        # Configuration
        title="pgGit GraphQL API",
        description="Git-like version control for PostgreSQL",
        version="0.1.0",
        # Enable GraphQL Playground (default in development)
        production=False,
        # Optional: Choose between "graphiql" (default) or "apollo-sandbox"
        # config=FraiseQLConfig(playground_tool="apollo-sandbox")
    )

    # Run the server
    uvicorn.run(app, host="0.0.0.0", port=8000)  # noqa: S104
