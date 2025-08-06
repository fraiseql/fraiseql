"""Initialize a new FraiseQL project."""

import subprocess
from pathlib import Path

import click


@click.command()
@click.argument("project_name")
@click.option(
    "--template",
    type=click.Choice(["basic", "blog", "ecommerce"]),
    default="basic",
    help="Project template to use",
)
@click.option(
    "--database-url",
    default="postgresql://localhost/mydb",
    help="PostgreSQL database URL",
)
@click.option(
    "--no-git",
    is_flag=True,
    help="Skip git initialization",
)
def init(project_name: str, template: str, database_url: str, no_git: bool) -> None:
    """Initialize a new FraiseQL project.

    Creates a new directory with the given PROJECT_NAME and sets up
    a basic FraiseQL application structure.
    """
    project_path = Path(project_name)

    # Check if directory already exists
    if project_path.exists():
        click.echo(f"Error: Directory '{project_name}' already exists", err=True)
        msg = f"Directory '{project_name}' already exists"
        raise click.ClickException(msg)

    click.echo(f"🚀 Creating FraiseQL project '{project_name}'...")

    # Create project directory
    project_path.mkdir(parents=True)

    # Create directory structure
    directories = [
        "src",
        "src/types",
        "src/mutations",
        "src/queries",
        "tests",
        "migrations",
    ]

    for directory in directories:
        (project_path / directory).mkdir(parents=True, exist_ok=True)

    # Create .env file
    env_content = f"""# FraiseQL Configuration
FRAISEQL_DATABASE_URL={database_url}
FRAISEQL_AUTO_CAMEL_CASE=true
FRAISEQL_DEV_AUTH_PASSWORD=development-only-password

# Production settings (uncomment for production)
# FRAISEQL_ENVIRONMENT=production
# SECRET_KEY=your-secret-key-here
"""
    (project_path / ".env").write_text(env_content)

    # Create .gitignore
    gitignore_content = """# Python
__pycache__/
*.py[cod]
*$py.class
*.so
.Python
env/
venv/
.venv/
pip-log.txt
pip-delete-this-directory.txt
.tox/
.coverage
.coverage.*
.cache
.pytest_cache/
htmlcov/
*.cover
.hypothesis/

# Environment
.env
.env.*

# IDE
.vscode/
.idea/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db

# Project
/dist/
/build/
*.egg-info/
"""
    (project_path / ".gitignore").write_text(gitignore_content)

    # Create pyproject.toml
    pyproject_content = f"""[project]
name = "{project_name}"
version = "0.1.0"
description = "A FraiseQL GraphQL API"
requires-python = ">=3.10"
dependencies = [
    "fraiseql>=0.1.0",
    "uvicorn>=0.34.3",
    "python-dotenv>=1.0.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.3.5",
    "pytest-asyncio>=0.21.0",
    "ruff>=0.8.4",
]

[tool.ruff]
line-length = 100
target-version = "py310"

[tool.pyright]
pythonVersion = "3.10"
typeCheckingMode = "strict"
"""
    (project_path / "pyproject.toml").write_text(pyproject_content)

    # Create main app file based on template
    if template == "basic":
        create_basic_template(project_path)
    elif template == "blog":
        create_blog_template(project_path)
    elif template == "ecommerce":
        create_ecommerce_template(project_path)

    # Create README
    readme_content = f"""# {project_name}

A FraiseQL GraphQL API project.

## Getting Started

1. Set up a Python virtual environment:
   ```bash
   python -m venv .venv
   source .venv/bin/activate  # On Windows: .venv\\Scripts\\activate
   ```

2. Install dependencies:
   ```bash
   pip install -e ".[dev]"
   ```

3. Set up your PostgreSQL database and update the DATABASE_URL in `.env`

4. Run migrations:
   ```bash
   fraiseql migrate
   ```

5. Start the development server:
   ```bash
   fraiseql dev
   ```

Your GraphQL API will be available at http://localhost:8000/graphql

## Project Structure

- `src/` - Application source code
  - `types/` - FraiseQL type definitions
  - `mutations/` - GraphQL mutations
  - `queries/` - Custom query logic
- `tests/` - Test files
- `migrations/` - Database migrations

## Learn More

- [FraiseQL Documentation](https://fraiseql.readthedocs.io)
- [GraphQL](https://graphql.org)
"""
    (project_path / "README.md").write_text(readme_content)

    # Initialize git repository
    if not no_git:
        try:
            subprocess.run(["git", "init", "-q"], check=True, cwd=str(project_path))
            subprocess.run(["git", "add", "."], check=True, cwd=str(project_path))
            subprocess.run(
                ["git", "commit", "-q", "-m", "Initial commit from FraiseQL CLI"],
                check=True,
                cwd=str(project_path),
            )
            click.echo("✅ Initialized git repository")
        except subprocess.CalledProcessError as e:
            click.echo(f"⚠️ Git initialization failed: {e}", err=True)

    click.echo(
        f"""
✨ Project '{project_name}' created successfully!

Next steps:
1. cd {project_name}
2. python -m venv .venv
3. source .venv/bin/activate  # On Windows: .venv\\Scripts\\activate
4. pip install -e ".[dev]"
5. Set up your PostgreSQL database
6. fraiseql dev

Happy coding! 🎉
""",
    )


def create_basic_template(project_path: Path) -> None:
    """Create a basic template with simple User type."""
    # Create main.py
    main_content = '''"""Main application entry point."""

import os
from typing import List

import fraiseql
from fraiseql import fraise_field

# Define your types
@fraiseql.type
class User:
    """A user in the system."""
    id: int = fraise_field(description="User ID")
    name: str = fraise_field(description="User's display name")
    email: str = fraise_field(description="User's email address")
    created_at: str = fraise_field(description="When the user was created")


@fraiseql.type
class QueryRoot:
    """Root query type."""
    users: List[User] = fraise_field(default_factory=list, description="List all users")

    async def resolve_users(self, info):
        # TODO: Implement actual database query
        return []


# Create the FastAPI app
app = fraiseql.create_fraiseql_app(
    queries=[QueryRoot],
    database_url=os.getenv("DATABASE_URL"),
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
'''
    (project_path / "src" / "main.py").write_text(main_content)

    # Create __init__.py files
    (project_path / "src" / "__init__.py").write_text("")
    (project_path / "src" / "types" / "__init__.py").write_text("")


def create_blog_template(project_path: Path) -> None:
    """Create a blog template with User, Post, and Comment types."""
    # Create types
    user_type = '''"""User type definition."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.types.scalars import UUID


@fraiseql.type
class User:
    """A blog author."""
    id: UUID
    username: str = fraise_field(description="Unique username")
    email: str = fraise_field(description="Email address")
    bio: str | None = fraise_field(description="User biography")
    avatar_url: str | None = fraise_field(description="Profile picture URL")
    created_at: str = fraise_field(description="Account creation date")
    posts: list["Post"] = fraise_field(description="Posts written by this user")
'''

    post_type = '''"""Post type definition."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.types.scalars import UUID

from .user import User
from .comment import Comment


@fraiseql.type
class Post:
    """A blog post."""
    id: UUID
    title: str = fraise_field(description="Post title")
    slug: str = fraise_field(description="URL-friendly slug")
    content: str = fraise_field(description="Post content in Markdown")
    excerpt: str | None = fraise_field(description="Short summary")
    author: User = fraise_field(description="Post author")
    published_at: str | None = fraise_field(description="Publication date")
    updated_at: str = fraise_field(description="Last update date")
    tags: list[str] = fraise_field(description="Post tags")
    comments: list[Comment] = fraise_field(description="Post comments")
    is_published: bool = fraise_field(description="Whether post is published")
'''

    comment_type = '''"""Comment type definition."""

import fraiseql
from fraiseql import fraise_field
from fraiseql.types.scalars import UUID

from .user import User


@fraiseql.type
class Comment:
    """A comment on a blog post."""
    id: UUID
    content: str = fraise_field(description="Comment text")
    author: User = fraise_field(description="Comment author")
    created_at: str = fraise_field(description="When comment was posted")
    updated_at: str = fraise_field(description="Last edit time")
    is_approved: bool = fraise_field(description="Whether comment is approved")
'''

    # Write type files
    (project_path / "src" / "types" / "user.py").write_text(user_type)
    (project_path / "src" / "types" / "post.py").write_text(post_type)
    (project_path / "src" / "types" / "comment.py").write_text(comment_type)

    # Create main.py
    main_content = '''"""Blog API main application."""

import os
from typing import List

import fraiseql
from fraiseql import fraise_field

from src.types.user import User
from src.types.post import Post
from src.types.comment import Comment


@fraiseql.type
class QueryRoot:
    """Root query type for blog."""
    users: List[User] = fraise_field(default_factory=list, description="List all users")
    posts: List[Post] = fraise_field(default_factory=list, description="List all posts")
    comments: List[Comment] = fraise_field(default_factory=list, description="List all comments")

    async def resolve_users(self, info):
        # TODO: Implement actual database query
        return []

    async def resolve_posts(self, info):
        # TODO: Implement actual database query
        return []

    async def resolve_comments(self, info):
        # TODO: Implement actual database query
        return []


# Create the FastAPI app
app = fraiseql.create_fraiseql_app(
    queries=[QueryRoot],
    database_url=os.getenv("DATABASE_URL"),
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
'''
    (project_path / "src" / "main.py").write_text(main_content)

    # Create __init__.py files
    (project_path / "src" / "__init__.py").write_text("")
    (project_path / "src" / "types" / "__init__.py").write_text(
        """from .user import User
from .post import Post
from .comment import Comment

__all__ = ["User", "Post", "Comment"]
""",
    )


def create_ecommerce_template(project_path: Path) -> None:
    """Create an e-commerce template."""
    # This would create Product, Order, Customer types
    # For brevity, using basic template for now
    create_basic_template(project_path)
    click.echo("Note: E-commerce template uses basic structure for now")
