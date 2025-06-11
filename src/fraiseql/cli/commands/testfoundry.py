"""TestFoundry integration commands."""

import asyncio
import os
from pathlib import Path

import click


@click.group()
def testfoundry():
    """Manage TestFoundry automated test generation."""
    pass


@testfoundry.command()
@click.option(
    "--schema",
    default="testfoundry",
    help="PostgreSQL schema for TestFoundry",
)
@click.pass_context
def install(ctx, schema: str):
    """Install TestFoundry in your database.

    This creates the necessary schema, tables, and functions
    for automated test generation.
    """
    click.echo("📦 Installing TestFoundry...")

    async def _install():
        from fraiseql.cqrs import CQRSRepository
        from fraiseql.extensions.testfoundry import FoundrySetup

        db_url = os.getenv("DATABASE_URL")
        if not db_url:
            click.echo("Error: DATABASE_URL not set", err=True)
            raise click.ClickException("DATABASE_URL not set")

        repository = CQRSRepository(db_url)
        setup = FoundrySetup(repository, schema_name=schema)

        try:
            await setup.install()
            click.echo("✅ TestFoundry installed successfully!")
        except Exception as e:
            click.echo(f"❌ Installation failed: {e}", err=True)
            raise click.ClickException(str(e))
        finally:
            await repository.close()

    asyncio.run(_install())


@testfoundry.command()
@click.argument("entity_name")
@click.option(
    "--input-type",
    help="Input type name (defaults to Create{Entity}Input)",
)
@click.option(
    "--output",
    "-o",
    default="tests/generated",
    help="Output directory for test files",
)
@click.pass_context
def generate(ctx, entity_name: str, input_type: str | None, output: str):
    """Generate tests for a FraiseQL entity.

    This analyzes your type definitions and generates comprehensive
    pgTAP tests including happy path, constraint violations, and
    authorization tests.
    """
    if not input_type:
        input_type = f"Create{entity_name}Input"

    click.echo(f"🧪 Generating tests for {entity_name}...")

    async def _generate():
        from fraiseql.cqrs import CQRSRepository
        from fraiseql.extensions.testfoundry import FoundryGenerator

        db_url = os.getenv("DATABASE_URL")
        if not db_url:
            click.echo("Error: DATABASE_URL not set", err=True)
            raise click.ClickException("DATABASE_URL not set")

        repository = CQRSRepository(db_url)
        generator = FoundryGenerator(repository)

        try:
            # Generate tests
            tests = await generator.generate_tests_for_entity(
                entity_name,
                f"{entity_name.lower()}s",  # table name
            )

            # Write tests to files
            output_dir = Path(output)
            await generator.write_tests_to_files(tests, output_dir)

            click.echo(f"✅ Tests generated in {output_dir}")
            click.echo("\nGenerated tests:")
            for test_type in tests:
                click.echo(f"  - {test_type}")

        except Exception as e:
            click.echo(f"❌ Test generation failed: {e}", err=True)
        finally:
            await repository.close()

    asyncio.run(_generate())


@testfoundry.command()
@click.argument("entity_name")
@click.argument("input_type")
@click.pass_context
def analyze(ctx, entity_name: str, input_type: str):
    """Analyze a FraiseQL input type and show metadata.

    This is useful for debugging and understanding how TestFoundry
    will generate tests for your types.
    """
    click.echo(f"🔍 Analyzing {input_type} for {entity_name}...")

    async def _analyze():
        from fraiseql.extensions.testfoundry import FoundryAnalyzer

        # Import the input type
        try:
            import importlib.util

            spec = importlib.util.spec_from_file_location(
                "types", "src/types/__init__.py"
            )
            types_module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(types_module)

            input_class = getattr(types_module, input_type, None)
            if not input_class:
                click.echo(f"Error: Type '{input_type}' not found", err=True)
                raise click.ClickException(f"Type '{input_type}' not found")

        except Exception as e:
            click.echo(f"Error importing types: {e}", err=True)
            raise click.ClickException(f"Import error: {e}")

        analyzer = FoundryAnalyzer()

        try:
            # Analyze the input type
            field_mappings = analyzer.analyze_input_type(input_class)

            click.echo(f"\n📊 Field Mappings for {input_type}:")
            click.echo("-" * 60)

            for mapping in field_mappings:
                click.echo(f"\nField: {mapping.field_name}")
                click.echo(f"  Type: {mapping.generator_type}")
                if mapping.random_function:
                    click.echo(f"  Random Function: {mapping.random_function}")
                if mapping.fk_mapping_key:
                    click.echo(f"  FK Mapping: {mapping.fk_mapping_key}")
                click.echo(f"  Required: {mapping.required}")

            # Generate SQL preview
            analyzer.field_mappings = field_mappings
            sql = analyzer.generate_sql_statements()

            click.echo("\n📝 Generated SQL:")
            click.echo("-" * 60)
            click.echo(sql)

        except Exception as e:
            click.echo(f"❌ Analysis failed: {e}", err=True)

    asyncio.run(_analyze())


@testfoundry.command()
@click.option(
    "--schema",
    default="testfoundry",
    help="PostgreSQL schema for TestFoundry",
)
@click.pass_context
def uninstall(ctx, schema: str):
    """Remove TestFoundry from your database.

    WARNING: This will drop the TestFoundry schema and all
    generated tests.
    """
    if not click.confirm(
        "⚠️  This will remove TestFoundry and all generated tests. Continue?"
    ):
        return

    click.echo("🗑️  Uninstalling TestFoundry...")

    async def _uninstall():
        from fraiseql.cqrs import CQRSRepository
        from fraiseql.extensions.testfoundry import FoundrySetup

        db_url = os.getenv("DATABASE_URL")
        if not db_url:
            click.echo("Error: DATABASE_URL not set", err=True)
            raise click.ClickException("DATABASE_URL not set")

        repository = CQRSRepository(db_url)
        setup = FoundrySetup(repository, schema_name=schema)

        try:
            await setup.uninstall()
            click.echo("✅ TestFoundry uninstalled")
        except Exception as e:
            click.echo(f"❌ Uninstall failed: {e}", err=True)
        finally:
            await repository.close()

    asyncio.run(_uninstall())
