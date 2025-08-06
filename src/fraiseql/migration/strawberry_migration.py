"""Strawberry GraphQL migration utilities.

Tools to help migrate from Strawberry GraphQL to FraiseQL.
"""

import re
from dataclasses import dataclass
from pathlib import Path


@dataclass
class MigrationIssue:
    """Represents a migration issue found in the codebase."""

    file_path: str
    line_number: int
    issue_type: str
    description: str
    suggestion: str | None = None


def check_strawberry_compatibility(project_path: str) -> list[MigrationIssue]:
    """Check a project for Strawberry GraphQL patterns that need migration.

    Args:
        project_path: Path to the project to analyze

    Returns:
        List of migration issues found
    """
    issues = []
    project_root = Path(project_path)

    if not project_root.exists():
        return issues

    # Find all Python files
    python_files = list(project_root.rglob("*.py"))

    for file_path in python_files:
        issues.extend(_analyze_file(file_path))

    return issues


def _analyze_file(file_path: Path) -> list[MigrationIssue]:
    """Analyze a single Python file for Strawberry patterns."""
    issues = []

    try:
        content = file_path.read_text(encoding="utf-8")
        content.splitlines()

        # Check for Strawberry imports
        strawberry_imports = _find_strawberry_imports(content, str(file_path))
        issues.extend(strawberry_imports)

        # Check for Strawberry decorators
        strawberry_decorators = _find_strawberry_decorators(content, str(file_path))
        issues.extend(strawberry_decorators)

        # Check for Strawberry-specific patterns
        strawberry_patterns = _find_strawberry_patterns(content, str(file_path))
        issues.extend(strawberry_patterns)

    except Exception as e:
        issues.append(
            MigrationIssue(
                file_path=str(file_path),
                line_number=0,
                issue_type="parse_error",
                description=f"Could not parse file: {e}",
                suggestion="Check file syntax",
            ),
        )

    return issues


def _find_strawberry_imports(content: str, file_path: str) -> list[MigrationIssue]:
    """Find Strawberry import statements."""
    issues = []
    lines = content.splitlines()

    for line_no, line in enumerate(lines, 1):
        if re.search(r"import\s+strawberry", line) or re.search(r"from\s+strawberry", line):
            issues.append(
                MigrationIssue(
                    file_path=file_path,
                    line_number=line_no,
                    issue_type="strawberry_import",
                    description="Strawberry import found",
                    suggestion="Replace with 'import fraiseql' or 'from fraiseql import ...'",
                ),
            )

    return issues


def _find_strawberry_decorators(content: str, file_path: str) -> list[MigrationIssue]:
    """Find Strawberry decorator usage."""
    issues = []
    lines = content.splitlines()

    decorator_patterns = [
        (r"@strawberry\.type", "Replace with @fraiseql.type"),
        (r"@strawberry\.input", "Replace with @fraiseql.input"),
        (r"@strawberry\.enum", "Replace with @fraiseql.enum"),
        (r"@strawberry\.interface", "Replace with @fraiseql.interface"),
        (r"@strawberry\.field", "Replace with @fraiseql.field"),
        (r"@strawberry\.mutation", "Replace with @fraiseql.mutation"),
        (r"@strawberry\.subscription", "Replace with @fraiseql.subscription"),
    ]

    for line_no, line in enumerate(lines, 1):
        for pattern, suggestion in decorator_patterns:
            if re.search(pattern, line):
                issues.append(
                    MigrationIssue(
                        file_path=file_path,
                        line_number=line_no,
                        issue_type="strawberry_decorator",
                        description=f"Strawberry decorator found: {pattern}",
                        suggestion=suggestion,
                    ),
                )

    return issues


def _find_strawberry_patterns(content: str, file_path: str) -> list[MigrationIssue]:
    """Find Strawberry-specific patterns that need attention."""
    issues = []
    lines = content.splitlines()

    patterns = [
        (r"strawberry\.federation", "Consider FraiseQL federation support"),
        (r"strawberry\.dataloader", "Migrate to fraiseql.optimization.DataLoader"),
        (r"strawberry\.exceptions", "Use fraiseql.exceptions instead"),
        (r"strawberry\.extensions", "Migrate to FraiseQL middleware"),
        (r"BaseGraphQLTestClient", "Use FastAPI TestClient with FraiseQL app"),
        (r"strawberry\.Schema", "Use fraiseql.create_fraiseql_app()"),
    ]

    for line_no, line in enumerate(lines, 1):
        for pattern, suggestion in patterns:
            if re.search(pattern, line):
                issues.append(
                    MigrationIssue(
                        file_path=file_path,
                        line_number=line_no,
                        issue_type="strawberry_pattern",
                        description=f"Strawberry pattern found: {pattern}",
                        suggestion=suggestion,
                    ),
                )

    return issues


def generate_migration_report(issues: list[MigrationIssue]) -> str:
    """Generate a human-readable migration report."""
    if not issues:
        return "✅ No Strawberry patterns found. Your codebase appears ready for FraiseQL!"

    report = f"🔍 Found {len(issues)} migration issues:\n\n"

    # Group by file
    files = {}
    for issue in issues:
        if issue.file_path not in files:
            files[issue.file_path] = []
        files[issue.file_path].append(issue)

    for file_path, file_issues in files.items():
        report += f"📄 {file_path}:\n"
        for issue in file_issues:
            report += f"  Line {issue.line_number}: {issue.description}\n"
            if issue.suggestion:
                report += f"    💡 {issue.suggestion}\n"
        report += "\n"

    # Summary by issue type
    issue_types = {}
    for issue in issues:
        issue_types[issue.issue_type] = issue_types.get(issue.issue_type, 0) + 1

    report += "📊 Summary:\n"
    for issue_type, count in issue_types.items():
        report += f"  - {issue_type}: {count} issues\n"

    return report
