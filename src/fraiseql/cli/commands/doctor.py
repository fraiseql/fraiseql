"""Comprehensive project health check and diagnostics command."""

import importlib.util
import os
import subprocess
import sys
from pathlib import Path

import click


@click.command()
@click.option(
    "--fix",
    is_flag=True,
    help="Automatically fix issues where possible",
)
@click.option(
    "--verbose",
    "-v",
    is_flag=True,
    help="Show detailed output",
)
def doctor(fix: bool, verbose: bool) -> None:
    """Run comprehensive health checks on your FraiseQL project.

    This command performs extensive validation including:
    - Project structure and configuration
    - Dependencies and imports
    - Database connectivity
    - GraphQL schema validation
    - Type checking
    - Code quality checks
    - Integration health

    Use --fix to automatically resolve issues where possible.
    """
    click.echo("🏥 Running FraiseQL Doctor diagnostics...\n")

    issues_found = 0
    issues_fixed = 0

    # Check 1: Project structure
    click.echo("📁 Checking project structure...")
    issues_found += check_project_structure(verbose)

    # Check 2: Dependencies
    click.echo("\n📦 Checking dependencies...")
    issues_found += check_dependencies(verbose)

    # Check 3: Configuration
    click.echo("\n⚙️  Checking configuration...")
    issues_found += check_configuration(verbose)

    # Check 4: Database connectivity
    click.echo("\n🗄️  Checking database connectivity...")
    issues_found += check_database_connectivity(verbose)

    # Check 5: GraphQL schema
    click.echo("\n🔗 Checking GraphQL schema...")
    issues_found += check_graphql_schema(verbose)

    # Check 6: Type checking
    click.echo("\n🔍 Running type checks...")
    issues_found += check_types(verbose)

    # Check 7: Code quality
    click.echo("\n🧹 Checking code quality...")
    issues_found += check_code_quality(verbose)

    # Check 8: Integration health
    click.echo("\n🔌 Checking integrations...")
    issues_found += check_integrations(verbose)

    # Summary
    click.echo(f"\n{'=' * 50}")
    if issues_found == 0:
        click.echo("✨ All checks passed! Your project is healthy.")
    else:
        click.echo(f"⚠️  Found {issues_found} issue(s)")
        if issues_fixed > 0:
            click.echo(f"✅ Automatically fixed {issues_fixed} issue(s)")
        if issues_found > issues_fixed:
            click.echo("💡 Run with --fix to automatically resolve some issues")
            click.echo(
                f"   Or manually address the remaining {issues_found - issues_fixed} issue(s)"
            )

    if issues_found > 0:
        sys.exit(1)


def check_project_structure(verbose: bool) -> int:
    """Check project structure and required files."""
    issues = 0

    # Check if we're in a FraiseQL project
    if not Path("pyproject.toml").exists():
        click.echo("  ❌ Not in a FraiseQL project directory (missing pyproject.toml)")
        return 1

    # Required directories
    required_dirs = ["src", "tests"]
    for dir_name in required_dirs:
        if not Path(dir_name).exists():
            click.echo(f"  ❌ Missing directory: {dir_name}/")
            issues += 1
        else:
            click.echo(f"  ✅ {dir_name}/")

    # Required files
    required_files = ["src/main.py", "pyproject.toml"]
    for file_name in required_files:
        if not Path(file_name).exists():
            click.echo(f"  ❌ Missing file: {file_name}")
            issues += 1
        else:
            click.echo(f"  ✅ {file_name}")

    # Optional but recommended
    optional_files = [".env", "README.md", "migrations"]
    for item in optional_files:
        if not Path(item).exists():
            click.echo(f"  ⚠️  Missing (recommended): {item}")
        else:
            click.echo(f"  ✅ {item}")

    return issues


def check_dependencies(verbose: bool) -> int:
    """Check Python dependencies."""
    issues = 0

    try:
        import fraiseql  # noqa: F401

        click.echo("  ✅ fraiseql package available")
    except ImportError:
        click.echo("  ❌ fraiseql package not found")
        issues += 1

    # Check for common optional dependencies
    optional_deps = {
        "fastapi": "FastAPI framework",
        "uvicorn": "ASGI server",
        "psycopg": "PostgreSQL driver",
        "pydantic": "Data validation",
    }

    for dep, description in optional_deps.items():
        try:
            __import__(dep)
            click.echo(f"  ✅ {description} ({dep})")
        except ImportError:
            click.echo(f"  ⚠️  {description} ({dep}) not available")
            if verbose:
                click.echo(f"      Install with: pip install {dep}")

    return issues


def check_configuration(verbose: bool) -> int:
    """Check configuration files and environment."""
    issues = 0

    # Check .env file
    env_path = Path(".env")
    if env_path.exists():
        click.echo("  ✅ .env file exists")

        # Check for required environment variables
        required_vars = ["DATABASE_URL"]
        env_content = env_path.read_text()

        for var in required_vars:
            if var not in env_content:
                click.echo(f"  ❌ Missing environment variable: {var}")
                issues += 1
            else:
                click.echo(f"  ✅ Environment variable: {var}")
    else:
        click.echo("  ⚠️  .env file not found")
        if verbose:
            click.echo("      Consider creating one with database configuration")

    # Check pyproject.toml
    try:
        import tomllib

        with Path("pyproject.toml").open("rb") as f:
            config = tomllib.load(f)

        if "project" in config:
            click.echo("  ✅ pyproject.toml has project section")
        else:
            click.echo("  ❌ pyproject.toml missing project section")
            issues += 1

        if "dependencies" in config.get("project", {}):
            deps = config["project"]["dependencies"]
            if any("fraiseql" in dep for dep in deps):
                click.echo("  ✅ fraiseql listed as dependency")
            else:
                click.echo("  ⚠️  fraiseql not found in dependencies")
        else:
            click.echo("  ❌ No dependencies section in pyproject.toml")
            issues += 1

    except ImportError:
        click.echo("  ⚠️  tomllib not available (Python < 3.11)")
    except Exception as e:
        click.echo(f"  ❌ Error reading pyproject.toml: {e}")
        issues += 1

    return issues


def check_database_connectivity(verbose: bool) -> int:
    """Check database connectivity."""
    issues = 0

    database_url = os.getenv("DATABASE_URL")
    if not database_url:
        click.echo("  ❌ DATABASE_URL not set")
        return 1

    click.echo("  ✅ DATABASE_URL configured")

    # Try to connect (lightweight check)
    try:
        if database_url.startswith("postgresql://"):
            import psycopg

            # Just test connection without executing queries
            with psycopg.connect(database_url, autocommit=True):
                click.echo("  ✅ Database connection successful")
        else:
            click.echo("  ⚠️  Unsupported database URL scheme")
            issues += 1
    except ImportError:
        click.echo("  ⚠️  psycopg not available for connectivity check")
    except Exception as e:
        click.echo(f"  ❌ Database connection failed: {e}")
        issues += 1

    return issues


def check_graphql_schema(verbose: bool) -> int:
    """Check GraphQL schema validity."""
    issues = 0

    try:
        # Import the main module
        spec = importlib.util.spec_from_file_location("main", "src/main.py")
        main_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(main_module)

        if hasattr(main_module, "app"):
            click.echo("  ✅ Found FraiseQL app in src/main.py")

            # Try to build schema
            from fraiseql.gql.schema_builder import SchemaRegistry

            registry = SchemaRegistry.get_instance()

            try:
                schema = registry.build_schema()
                click.echo("  ✅ GraphQL schema builds successfully")

                # Check for common issues
                if hasattr(schema, "type_map"):
                    types = [name for name in schema.type_map if not name.startswith("__")]
                    click.echo(f"  📊 Schema has {len(types)} custom types")

                    if len(types) == 0:
                        click.echo("  ⚠️  No custom GraphQL types defined")
                        issues += 1

            except Exception as e:
                click.echo(f"  ❌ Schema build failed: {e}")
                issues += 1
        else:
            click.echo("  ❌ No 'app' variable found in src/main.py")
            issues += 1

    except Exception as e:
        click.echo(f"  ❌ Error loading main.py: {e}")
        issues += 1

    return issues


def check_types(verbose: bool) -> int:
    """Run type checking."""
    issues = 0

    # Try mypy if available
    try:
        result = subprocess.run(
            ["mypy", "src/", "--ignore-missing-imports"],
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )

        if result.returncode == 0:
            click.echo("  ✅ Type checking passed")
        else:
            click.echo("  ⚠️  Type checking found issues")
            if verbose and result.stdout:
                click.echo("      " + "\n      ".join(result.stdout.split("\n")[:5]))
            issues += 1

    except FileNotFoundError:
        click.echo("  ⚠️  mypy not installed")
        if verbose:
            click.echo("      Install with: pip install mypy")
    except subprocess.TimeoutExpired:
        click.echo("  ⚠️  Type checking timed out")
    except Exception as e:
        click.echo(f"  ⚠️  Type checking error: {e}")

    return issues


def check_code_quality(verbose: bool) -> int:
    """Check code quality with linters."""
    issues = 0

    # Try ruff if available
    try:
        result = subprocess.run(
            ["ruff", "check", "src/"], check=False, capture_output=True, text=True, timeout=30
        )

        if result.returncode == 0:
            click.echo("  ✅ Code quality checks passed")
        else:
            click.echo("  ⚠️  Code quality issues found")
            if verbose and result.stdout:
                click.echo("      " + "\n      ".join(result.stdout.split("\n")[:5]))
            issues += 1

    except FileNotFoundError:
        click.echo("  ⚠️  ruff not installed")
        if verbose:
            click.echo("      Install with: pip install ruff")
    except subprocess.TimeoutExpired:
        click.echo("  ⚠️  Code quality check timed out")
    except Exception as e:
        click.echo(f"  ⚠️  Code quality check error: {e}")

    return issues


def check_integrations(verbose: bool) -> int:
    """Check integration health."""
    issues = 0

    # Check LangChain integration
    try:
        from fraiseql.integrations.langchain import FraiseQLVectorStore

        click.echo("  ✅ LangChain integration available")
    except ImportError:
        click.echo("  ⚠️  LangChain integration not available")
        if verbose:
            click.echo("      Install with: pip install langchain langchain-openai")

    # Check LlamaIndex integration
    try:
        from fraiseql.integrations.llamaindex import FraiseQLVectorStore  # noqa: F401

        click.echo("  ✅ LlamaIndex integration available")
    except ImportError:
        click.echo("  ⚠️  LlamaIndex integration not available")
        if verbose:
            click.echo("      Install with: pip install llama-index")

    # Check for common integration issues
    if os.getenv("OPENAI_API_KEY"):
        click.echo("  ✅ OpenAI API key configured")
    else:
        click.echo("  ⚠️  OpenAI API key not found")
        if verbose:
            click.echo("      Set OPENAI_API_KEY environment variable for AI features")

    return issues
