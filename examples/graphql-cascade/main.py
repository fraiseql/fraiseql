#!/usr/bin/env python3
"""
GraphQL Cascade Example

This example demonstrates GraphQL Cascade functionality in FraiseQL.
Run with: python main.py
"""

import uuid
from typing import List, Optional

import uvicorn
from fastapi import FastAPI

import fraiseql
from fraiseql import gql
from fraiseql.mutations import mutation


# Input/Output Types
@fraiseql.input
class CreatePostInput:
    title: str
    content: Optional[str] = None
    author_id: str


@fraiseql.type
class Post:
    id: str
    title: str
    content: Optional[str]
    author_id: str
    created_at: str


@fraiseql.type
class User:
    id: str
    name: str
    post_count: int
    created_at: str


@fraiseql.type
class PostWithAuthor:
    id: str
    title: str
    content: Optional[str]
    author: User
    created_at: str


@fraiseql.type
class CreatePostSuccess:
    id: str
    message: str


@fraiseql.type
class CreatePostError:
    code: str
    message: str
    field: Optional[str]


# Queries
@gql.query
class GetPosts:
    result: List[PostWithAuthor]


@gql.query
class GetUser:
    input: str  # user ID
    result: User


# Mutations
@mutation(enable_cascade=True)
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError


# FastAPI app
app = FastAPI(title="GraphQL Cascade Example")

# Add GraphQL endpoint
app.add_route("/graphql", gql.graphql_app, methods=["GET", "POST"])

# Add GraphiQL
app.add_route("/graphiql", gql.graphiql_app, methods=["GET"])


@app.get("/")
async def root():
    return {
        "message": "GraphQL Cascade Example",
        "graphql_endpoint": "/graphql",
        "graphiql": "/graphiql",
        "docs": "/docs",
    }


if __name__ == "__main__":
    print("🚀 GraphQL Cascade Example")
    print("📊 GraphQL endpoint: http://localhost:8000/graphql")
    print("🎛️  GraphiQL: http://localhost:8000/graphiql")
    print("📚 API docs: http://localhost:8000/docs")
    print()
    print("Example mutation:")
    print("""
    mutation CreatePost($input: CreatePostInput!) {
      createPost(input: $input) {
        id
        message
        cascade {
          updated {
            __typename
            id
            operation
            entity
          }
          invalidations {
            queryName
            strategy
            scope
          }
          metadata {
            timestamp
            affectedCount
          }
        }
      }
    }
    """)
    print('Variables: { "input": { "title": "Hello World", "author_id": "<user-id>" } }')

    uvicorn.run(app, host="0.0.0.0", port=8000)
