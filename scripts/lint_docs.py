#!/usr/bin/env python3
"""
Documentation Linter for FraiseQL

Scans all .md files in the docs/ directory and checks for:
- Consistent import patterns
- Consistent decorator usage
- No outdated patterns
- Missing type hints
- Using str for UUIDs instead of UUID
- Using Dict instead of dict

Usage:
    python scripts/lint_docs.py
    # Exit code 1 if violations found (for CI)
"""

import os
import re
import sys
from pathlib import Path


class DocLinter:
    """Lints FraiseQL documentation for consistency."""

    def __init__(self, docs_dir: str = "docs"):
        self.docs_dir = Path(docs_dir)
        self.violations: list[str] = []

    def find_markdown_files(self) -> list[Path]:
        """Find all .md files in docs directory."""
        return list(self.docs_dir.rglob("*.md"))

    def extract_code_blocks(self, content: str) -> list[str]:
        """Extract Python code blocks from markdown content."""
        # Match ```python ... ``` blocks
        pattern = r"```python\s*\n(.*?)\n```"
        matches = re.findall(pattern, content, re.DOTALL)
        return matches

    def check_import_patterns(self, code: str, file_path: str) -> None:
        """Check for consistent import patterns."""
        lines = code.split("\n")

        # Check for old import patterns
        old_patterns = [
            r"from fraiseql\.decorators import",
            r"\bimport fraiseql\b",
            r"@fraiseql\.",
        ]

        for i, line in enumerate(lines, 1):
            # Skip commented lines (lines starting with #) as they may show bad examples
            if line.strip().startswith("#"):
                continue

            for pattern in old_patterns:
                if re.search(pattern, line):
                    self.violations.append(
                        f"{file_path}: Line {i}: Found old import pattern: {line.strip()}"
                    )

        # Check for proper standard imports
        has_standard_import = any(
            "from fraiseql import" in line
            and any(
                decorator in line for decorator in ["type", "query", "mutation", "input", "field"]
            )
            for line in lines
        )

        if "@type" in code or "@query" in code or "@mutation" in code:
            if not has_standard_import:
                self.violations.append(
                    f"{file_path}: Missing standard fraiseql import (from fraiseql import type, query, mutation, ...)"
                )

    def check_decorator_usage(self, code: str, file_path: str) -> None:
        """Check for consistent decorator usage."""
        lines = code.split("\n")

        # Check for old decorators
        old_decorators = [
            "@fraiseql.type",
            "@fraiseql.query",
            "@fraiseql.mutation",
            "@fraiseql.input",
            "@fraiseql.field",
        ]

        for i, line in enumerate(lines, 1):
            for decorator in old_decorators:
                if decorator in line:
                    self.violations.append(
                        f"{file_path}: Line {i}: Use standard decorator {decorator.replace('@fraiseql.', '@')} instead of {decorator}"
                    )

    def check_type_hints(self, code: str, file_path: str) -> None:
        """Check for proper type hints."""
        lines = code.split("\n")

        # Check for str used where UUID should be used
        for i, line in enumerate(lines, 1):
            # Look for id: str patterns that might be UUIDs
            if re.search(r"\bid\s*:\s*str\b", line) and not re.search(r"#.*\bid.*str", line):
                # Check if this is likely a UUID field by looking at context
                context_lines = lines[max(0, i - 3) : min(len(lines), i + 3)]
                context = "\n".join(context_lines)
                if any(
                    keyword in context.lower()
                    for keyword in ["uuid", "primary", "foreign", "pk_", "id"]
                ):
                    self.violations.append(
                        f"{file_path}: Line {i}: Use UUID type for ID fields instead of str: {line.strip()}"
                    )

        # Check for Dict instead of dict
        for i, line in enumerate(lines, 1):
            if "[" in line and "from typing import" in code:
                self.violations.append(
                    f"{file_path}: Line {i}: Use dict instead of Dict for Python 3.9+: {line.strip()}"
                )

    def check_naming_conventions(self, code: str, file_path: str) -> None:
        """Check for consistent naming conventions."""
        lines = code.split("\n")

        # Check GraphQL field names (should be camelCase in comments/examples)
        for i, line in enumerate(lines, 1):
            # Look for GraphQL query examples
            if "query {" in line.lower() or "mutation {" in line.lower():
                # Check subsequent lines for field names
                for j in range(i, min(len(lines), i + 10)):
                    field_match = re.search(r"(\w+):", lines[j])
                    if field_match:
                        field_name = field_match.group(1)
                        # Check if it looks like snake_case when it should be camelCase
                        if "_" in field_name and not field_name.startswith("_"):
                            self.violations.append(
                                f"{file_path}: Line {j + 1}: GraphQL field '{field_name}' should be camelCase, not snake_case"
                            )

    def lint_file(self, file_path: Path) -> None:
        """Lint a single markdown file."""
        try:
            # Skip STYLE_GUIDE.md as it intentionally shows bad patterns as examples
            if file_path.name == "STYLE_GUIDE.md":
                return

            content = file_path.read_text(encoding="utf-8")
            code_blocks = self.extract_code_blocks(content)

            for code in code_blocks:
                self.check_import_patterns(code, str(file_path))
                self.check_decorator_usage(code, str(file_path))
                self.check_type_hints(code, str(file_path))
                self.check_naming_conventions(code, str(file_path))

        except Exception as e:
            self.violations.append(f"Error reading {file_path}: {e}")

    def lint_all(self) -> bool:
        """Lint all documentation files. Returns True if no violations."""
        markdown_files = self.find_markdown_files()

        for file_path in markdown_files:
            self.lint_file(file_path)

        return len(self.violations) == 0

    def print_report(self) -> None:
        """Print linting report."""
        if not self.violations:
            print("✅ All documentation passes linting checks!")
            return

        print(f"❌ Found {len(self.violations)} violations:")
        print()

        for violation in self.violations:
            print(f"  {violation}")

        print()
        print("💡 Fix these issues to maintain documentation consistency.")
        print("   See docs/STYLE_GUIDE.md for the official patterns.")


def main() -> int:
    """Main entry point."""
    linter = DocLinter()

    if not linter.docs_dir.exists():
        print(f"Error: docs directory not found at {linter.docs_dir}")
        return 1

    success = linter.lint_all()
    linter.print_report()

    return 0 if success else 1


if __name__ == "__main__":
    sys.exit(main())
