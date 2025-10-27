#!/usr/bin/env python3
"""FraiseQL MCP Server for Claude Code
Provides stack-specific context and code examples
"""

import json
from pathlib import Path
from typing import Any


class FraiseQLMCPServer:
    """MCP server providing FraiseQL-specific context to Claude Code"""

    def __init__(self, project_root: str):
        self.project_root = Path(project_root)
        self.docs_path = self.project_root / "docs"
        self.examples_path = self.project_root / "examples"

    def get_patterns(self, pattern_type: str) -> dict[str, Any]:
        """Get common FraiseQL patterns"""
        patterns = {
            "type_definition": {
                "description": "FraiseQL type definition pattern",
                "code": """@fraise_type
class User:
    id: UUID
    name: str
    email: str
    created_at: datetime""",
            },
            "query_function": {
                "description": "FraiseQL query function (not resolver)",
                "code": """@fraiseql.query
async def users(info, limit: int = 10) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view", limit=limit)""",
            },
            "database_view": {
                "description": "JSONB pattern for database views",
                "code": """CREATE VIEW user_view AS
SELECT
    id,
    tenant_id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email,
        'created_at', created_at
    ) as data
FROM users;""",
            },
            "mutation": {
                "description": "FraiseQL mutation with union types",
                "code": """@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> CreateUserResult:
    db = info.context["db"]
    try:
        user_id = await db.execute_function(
            "create_user",
            name=input.name,
            email=input.email
        )
        return CreateUserSuccess(user_id=user_id)
    except Exception as e:
        return CreateUserError(message=str(e))""",
            },
        }
        return patterns.get(pattern_type, {})

    def get_architecture_guidance(self) -> dict[str, str]:
        """Get architecture-specific guidance"""
        return {
            "cqrs": "Use database views for queries, PostgreSQL functions for mutations",
            "jsonb": "All data must flow through JSONB 'data' column in views",
            "auth": "Use @requires_auth decorator and check info.context['user']",
            "testing": "Use unified container testing with database_conftest.py",
            "frontend": "Generate TypeScript types from GraphQL schema",
        }

    def suggest_file_structure(self, feature_name: str) -> dict[str, str]:
        """Suggest file structure for a new feature"""
        return {
            f"src/types/{feature_name}.py": "Type definitions with @fraise_type",
            f"src/queries/{feature_name}.py": "Query functions with @fraiseql.query",
            f"src/mutations/{feature_name}.py": "Mutation functions with @fraiseql.mutation",
            f"db/views/{feature_name}_view.sql": "Database view with JSONB data column",
            f"db/functions/{feature_name}_functions.sql": "PostgreSQL functions for mutations",
            f"tests/test_{feature_name}.py": "Unit and integration tests",
        }


if __name__ == "__main__":
    # MCP server initialization would go here
    server = FraiseQLMCPServer("/home/lionel/code/fraiseql")
    print(json.dumps(server.get_patterns("type_definition"), indent=2))
