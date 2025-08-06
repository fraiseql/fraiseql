"""Type checking and validation command."""

import importlib.util
import sys
from pathlib import Path

import click


@click.command()
def check() -> None:
    """Validate your FraiseQL project structure and types.

    This checks that:
    - Project structure is valid
    - FraiseQL types can be imported
    - GraphQL schema can be built
    """
    click.echo("🔍 Checking FraiseQL project...")

    # Check if we're in a FraiseQL project
    if not Path("pyproject.toml").exists():
        click.echo("Error: Not in a FraiseQL project directory", err=True)
        sys.exit(1)

    # Check project structure
    click.echo("\n📁 Checking project structure...")
    required_dirs = ["src", "tests", "migrations"]
    missing_dirs = []

    for dir_name in required_dirs:
        if Path(dir_name).exists():
            click.echo(f"  ✅ {dir_name}/")
        else:
            click.echo(f"  ❌ {dir_name}/ (missing)")
            missing_dirs.append(dir_name)

    if missing_dirs:
        click.echo(f"\nWarning: Missing directories: {', '.join(missing_dirs)}")

    # Check main.py exists
    main_path = Path("src/main.py")
    if not main_path.exists():
        click.echo("\n❌ src/main.py not found")
        click.echo("Create it with your FraiseQL app definition")
        sys.exit(1)

    click.echo("\n🐍 Validating FraiseQL types...")

    try:
        # Import the app to validate types
        spec = importlib.util.spec_from_file_location("main", "src/main.py")
        main_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(main_module)

        if hasattr(main_module, "app"):
            click.echo("  ✅ Found FraiseQL app")

            # Try to access the schema registry
            from fraiseql.gql.schema_builder import SchemaRegistry

            registry = SchemaRegistry.get_instance()

            # Get registered types
            types_count = len(registry._types) if hasattr(registry, "_types") else 0
            inputs_count = len(registry._input_types) if hasattr(registry, "_input_types") else 0

            click.echo(f"  📊 Registered types: {types_count}")
            click.echo(f"  📊 Input types: {inputs_count}")

            # Try to build schema to validate
            try:
                # Building schema validates all types are properly defined
                schema = registry.build_schema()
                click.echo("  ✅ GraphQL schema builds successfully!")

                # Show some schema info
                if hasattr(schema, "type_map"):
                    custom_types = [name for name in schema.type_map if not name.startswith("__")]
                    click.echo(f"  📊 Schema contains {len(custom_types)} custom types")

            except Exception as e:
                click.echo(f"  ❌ Schema validation failed: {e}", err=True)
                sys.exit(1)
        else:
            click.echo("  ⚠️  No 'app' found in src/main.py")
            click.echo("  Make sure you have: app = fraiseql.create_fraiseql_app(...)")

    except ImportError as e:
        click.echo(f"  ❌ Import error: {e}", err=True)
        click.echo("  Make sure all dependencies are installed")
        sys.exit(1)
    except Exception as e:
        click.echo(f"  ❌ Error validating types: {e}", err=True)
        sys.exit(1)

    click.echo("\n✨ All checks passed!")
