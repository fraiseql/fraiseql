"""Simplified Blog Demo - FraiseQL Enterprise Showcase

A streamlined blog application demonstrating FraiseQL's core strengths:
- Strongly opinionated error-as-data pattern (no choice between configs)
- Clean mutation decorators (@fraiseql.mutation)
- Native error arrays only on error types
- Database-first architecture
- Smooth developer experience
"""

import uuid
from typing import Dict, Any
from fastapi import FastAPI, Request

import fraiseql
from fraiseql.errors import FraiseQLError


def create_simple_demo_app() -> FastAPI:
    """Create a minimal blog demo showcasing FraiseQL's patterns."""

    app = FastAPI(
        title="FraiseQL Blog Demo",
        description="Strongly opinionated error-as-data patterns",
        version="1.0.0"
    )

    # Import types to register them
    from .types import blog_mutations, blog_types, blog_queries

    @app.get("/")
    async def home():
        return {
            "message": "🎉 FraiseQL Blog Demo - Strongly Opinionated!",
            "features": [
                "Error-as-data (no choice between configs)",
                "Clean @fraiseql.mutation decorators",
                "Native error arrays only on error types",
                "Database-first architecture",
                "Smooth developer experience"
            ],
            "patterns": {
                "success_types": "No errors array - clean success!",
                "error_types": "Native error arrays as data",
                "mutations": "@fraiseql.mutation decorator",
                "strongly_opinionated": "One way to handle errors"
            },
            "endpoints": {
                "demo": "/demo-types",
                "status": "/health"
            }
        }

    @app.get("/demo-types")
    async def demo_types():
        """Show the clean type patterns."""
        return {
            "success_example": {
                "description": "Success types are clean - no errors array",
                "structure": {
                    "post": "Post",
                    "message": "Post created successfully"
                    # No errors array!
                }
            },
            "error_example": {
                "description": "Error types have native error arrays as data",
                "structure": {
                    "message": "Creation failed",
                    "errors": [
                        {
                            "code": "DUPLICATE_IDENTIFIER",
                            "message": "Post with this identifier already exists"
                        }
                    ],
                    "duplicate_post": "Post | null"
                }
            },
            "mutation_example": {
                "description": "Clean mutation decorators",
                "pattern": "@fraiseql.mutation(function='app.create_post')"
            }
        }

    @app.get("/health")
    async def health():
        return {
            "status": "healthy",
            "service": "fraiseql_blog_demo",
            "patterns": "strongly_opinionated"
        }

    return app


# Create the app instance
app = create_simple_demo_app()


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
