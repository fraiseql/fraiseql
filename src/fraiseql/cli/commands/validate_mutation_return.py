"""CLI command to validate mutation return types against a GraphQL schema."""

import json
import sys
from pathlib import Path

import click

from fraiseql.mutations.validation import ValidationError as MutationValidationError
from fraiseql.mutations.validation import ValidationResult, validate_mutation_return


@click.command("validate-mutation-return")
@click.option(
    "--schema",
    "schema_path",
    required=True,
    type=click.Path(exists=True, dir_okay=False),
    help="Path to GraphQL schema file (.graphql or .gql).",
)
@click.option(
    "--mutation",
    "mutation_name",
    required=True,
    help="Name of the mutation to validate against.",
)
@click.option(
    "--response-file",
    type=click.Path(exists=True, dir_okay=False),
    help="Path to a JSON response file to validate.",
)
@click.option(
    "--format",
    "output_format",
    type=click.Choice(["human", "json", "junit"]),
    default="human",
    help="Output format (default: human).",
)
@click.argument("files", nargs=-1, type=click.Path(exists=True, dir_okay=False))
def validate_mutation_return_command(
    schema_path: str,
    mutation_name: str,
    response_file: str | None,
    output_format: str,
    files: tuple[str, ...],
) -> None:
    r"""Validate mutation return values against a GraphQL schema.

    Checks that JSON response files match the mutation's expected return type,
    catching missing fields, type mismatches, and structural errors.

    \b
    Examples:
      fraiseql validate-mutation-return \
        --schema schema.graphql --mutation createUser \
        --response-file response.json
    """
    from graphql import build_schema as gql_build_schema

    # Load schema
    schema_text = Path(schema_path).read_text()
    try:
        schema = gql_build_schema(schema_text)
    except Exception as e:
        click.echo(f"Error parsing schema: {e}", err=True)
        sys.exit(1)

    # Collect response files
    response_files: list[str] = []
    if response_file:
        response_files.append(response_file)
    response_files.extend(files)

    if not response_files:
        click.echo(
            "Error: No response files provided. Use --response-file or pass files as arguments.",
            err=True,
        )
        sys.exit(1)

    # Validate each file
    all_results: list[tuple[str, ValidationResult]] = []
    for filepath in response_files:
        try:
            data = json.loads(Path(filepath).read_text())
        except json.JSONDecodeError as e:
            all_results.append(
                (
                    filepath,
                    ValidationResult(
                        is_valid=False,
                        errors=[
                            MutationValidationError(
                                field_path="",
                                message=f"Invalid JSON: {e}",
                                expected_type="",
                            )
                        ],
                    ),
                )
            )
            continue

        result = validate_mutation_return(schema, mutation_name, data)
        all_results.append((filepath, result))

    # Output results
    if output_format == "json":
        _output_json(all_results)
    elif output_format == "junit":
        _output_junit(all_results, mutation_name)
    else:
        _output_human(all_results)

    # Exit with error if any validation failed
    if any(not r.is_valid for _, r in all_results):
        sys.exit(1)


def _output_human(results: list[tuple[str, ValidationResult]]) -> None:
    """Output results in human-readable format."""
    for filepath, result in results:
        if result.is_valid:
            matched = f" (matched: {result.matched_type})" if result.matched_type else ""
            click.echo(f"PASS {filepath}{matched}")
        else:
            click.echo(f"FAIL {filepath}")
            for error in result.errors:
                path_str = f"  {error.field_path}: " if error.field_path else "  "
                click.echo(f"{path_str}{error.message}")
                if error.expected_type:
                    click.echo(f"    expected: {error.expected_type}")

    total = len(results)
    passed = sum(1 for _, r in results if r.is_valid)
    failed = total - passed
    click.echo(f"\n{passed}/{total} passed, {failed} failed")


def _output_json(results: list[tuple[str, ValidationResult]]) -> None:
    """Output results as JSON."""
    output = []
    for filepath, result in results:
        output.append(
            {
                "file": filepath,
                "valid": result.is_valid,
                "matched_type": result.matched_type,
                "errors": [
                    {
                        "field_path": e.field_path,
                        "message": e.message,
                        "expected_type": e.expected_type,
                    }
                    for e in result.errors
                ],
            }
        )
    click.echo(json.dumps(output, indent=2))


def _output_junit(
    results: list[tuple[str, ValidationResult]],
    mutation_name: str,
) -> None:
    """Output results as JUnit XML for CI integration."""
    total = len(results)
    failures = sum(1 for _, r in results if not r.is_valid)

    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        (
            f'<testsuite name="validate-mutation-return:{mutation_name}" '
            f'tests="{total}" failures="{failures}">'
        ),
    ]

    for filepath, result in results:
        classname = Path(filepath).stem
        if result.is_valid:
            lines.append(f'  <testcase classname="{classname}" name="{filepath}" />')
        else:
            error_messages = "; ".join(
                f"{e.field_path}: {e.message}" if e.field_path else e.message for e in result.errors
            )
            # Escape XML special chars
            error_messages = (
                error_messages.replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;")
                .replace('"', "&quot;")
            )
            lines.append(f'  <testcase classname="{classname}" name="{filepath}">')
            lines.append(f'    <failure message="Validation failed">{error_messages}</failure>')
            lines.append("  </testcase>")

    lines.append("</testsuite>")
    click.echo("\n".join(lines))
