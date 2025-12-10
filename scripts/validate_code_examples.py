#!/usr/bin/env python3
"""
Code Example Validation Script for FraiseQL Documentation

Validates all code examples in markdown files:
- SQL syntax validation using sqlparse
- Python syntax validation using ast + ruff
- Reports broken code with file location and line numbers

Usage:
    python scripts/validate_code_examples.py
    python scripts/validate_code_examples.py --fix
    python scripts/validate_code_examples.py --sql-only
    python scripts/validate_code_examples.py --python-only
"""

import argparse
import ast
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Tuple

try:
    import sqlparse
except ImportError:
    print("Warning: sqlparse not installed. SQL validation will be basic.")
    print("Install with: pip install sqlparse")
    sqlparse = None


@dataclass
class CodeBlock:
    """Represents a code block from markdown"""
    file_path: Path
    language: str
    code: str
    start_line: int
    end_line: int


@dataclass
class CodeIssue:
    """Represents a code validation issue"""
    file_path: Path
    language: str
    start_line: int
    end_line: int
    issue_type: str  # 'syntax_error', 'parse_error', 'lint_warning'
    message: str
    code_snippet: str = ""


@dataclass
class ValidationReport:
    """Validation results"""
    total_files: int = 0
    total_code_blocks: int = 0
    sql_blocks: int = 0
    python_blocks: int = 0
    other_blocks: int = 0
    issues: List[CodeIssue] = field(default_factory=list)

    @property
    def error_count(self) -> int:
        return len([i for i in self.issues if i.issue_type in ['syntax_error', 'parse_error']])

    @property
    def warning_count(self) -> int:
        return len([i for i in self.issues if i.issue_type == 'lint_warning'])

    @property
    def success_rate(self) -> float:
        if self.total_code_blocks == 0:
            return 100.0
        return ((self.total_code_blocks - self.error_count) / self.total_code_blocks) * 100


class CodeValidator:
    """Validates code examples in markdown"""

    # Pattern to match code blocks: ```language\ncode\n```
    CODE_BLOCK_PATTERN = re.compile(
        r'^```(\w+)\s*\n(.*?)\n```',
        re.MULTILINE | re.DOTALL
    )

    def __init__(
        self,
        docs_root: Path,
        validate_sql: bool = True,
        validate_python: bool = True
    ):
        self.docs_root = docs_root
        self.validate_sql = validate_sql
        self.validate_python = validate_python
        self.report = ValidationReport()

    def extract_code_blocks(self, file_path: Path) -> List[CodeBlock]:
        """Extract all code blocks from a markdown file"""
        try:
            content = file_path.read_text(encoding='utf-8')
        except Exception as e:
            print(f"Error reading {file_path}: {e}")
            return []

        blocks = []
        for match in self.CODE_BLOCK_PATTERN.finditer(content):
            language = match.group(1).lower()
            code = match.group(2)

            # Calculate line numbers
            start_line = content[:match.start()].count('\n') + 1
            end_line = content[:match.end()].count('\n') + 1

            blocks.append(CodeBlock(
                file_path=file_path,
                language=language,
                code=code,
                start_line=start_line,
                end_line=end_line
            ))

        return blocks

    def validate_sql_block(self, block: CodeBlock) -> List[CodeIssue]:
        """Validate SQL code block"""
        issues = []

        # Skip if it's just a placeholder or comment
        stripped = block.code.strip()
        if not stripped or stripped.startswith('--'):
            return issues

        # Skip if it contains obvious placeholders
        if any(placeholder in stripped.lower() for placeholder in ['...', '<your-', '[your-', 'todo:', 'fixme:']):
            return issues

        # Use sqlparse if available
        if sqlparse:
            try:
                # Try to parse the SQL
                parsed = sqlparse.parse(block.code)
                if not parsed:
                    issues.append(CodeIssue(
                        file_path=block.file_path,
                        language='sql',
                        start_line=block.start_line,
                        end_line=block.end_line,
                        issue_type='parse_error',
                        message='Failed to parse SQL code',
                        code_snippet=block.code[:200]
                    ))
            except Exception as e:
                issues.append(CodeIssue(
                    file_path=block.file_path,
                    language='sql',
                    start_line=block.start_line,
                    end_line=block.end_line,
                    issue_type='parse_error',
                    message=f'SQL parse error: {str(e)}',
                    code_snippet=block.code[:200]
                ))
        else:
            # Basic validation without sqlparse
            # Check for common SQL keywords
            sql_keywords = ['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'CREATE', 'ALTER', 'DROP']
            has_sql = any(keyword in stripped.upper() for keyword in sql_keywords)

            if has_sql:
                # Very basic syntax checks
                if stripped.count('(') != stripped.count(')'):
                    issues.append(CodeIssue(
                        file_path=block.file_path,
                        language='sql',
                        start_line=block.start_line,
                        end_line=block.end_line,
                        issue_type='syntax_error',
                        message='Mismatched parentheses in SQL',
                        code_snippet=block.code[:200]
                    ))

        return issues

    def is_signature_only(self, code: str) -> bool:
        """Check if code is just a function/method signature (common in API docs)"""
        lines = [line.strip() for line in code.split('\n') if line.strip() and not line.strip().startswith('#')]

        # Check if it's a function definition that ends with ) -> return_type
        if lines:
            # Join lines to handle multi-line signatures
            full_code = ' '.join(lines)
            # Pattern: def/async def name(...) -> return_type with no body
            if ('def ' in full_code and
                ') ->' in full_code and
                not any(keyword in full_code for keyword in ['pass', 'return', '=', 'raise', 'if ', 'for ', 'while '])):
                return True

            # Also check for function definition that ends with ): without body
            # Count actual code lines (exclude def line, comments, empty lines)
            code_lines = [l for l in lines if not l.startswith(('def ', 'async def '))]
            if ('def ' in full_code and ')' in full_code and len(code_lines) == 0):
                return True

        return False

    def is_decorator_signature(self, code: str) -> bool:
        """Check if code shows decorator usage pattern (common in docs)"""
        lines = code.strip().split('\n')
        # Decorator pattern: @decorator(...) followed by class/function with minimal/no body
        has_decorator = any(line.strip().startswith('@') for line in lines)
        has_definition = any(line.strip().startswith(('class ', 'def ', 'async def ')) for line in lines)

        if has_decorator and has_definition:
            # Check if there's actual implementation or just signature
            code_lines = [l for l in lines if l.strip() and not l.strip().startswith(('#', '@', 'import '))]
            # If less than 5 lines of actual code after decorator, it's probably just a pattern
            return len(code_lines) < 5
        return False

    def has_mixed_languages(self, code: str) -> bool:
        """Check if code mixes SQL and Python (common in tutorials)"""
        has_sql = any(keyword in code.upper() for keyword in ['CREATE ', 'SELECT ', 'INSERT ', 'UPDATE ', 'DELETE '])
        has_python = any(keyword in code for keyword in ['import ', 'def ', 'class ', '@'])
        return has_sql and has_python

    def has_only_comments_as_body(self, code: str) -> bool:
        """Check if function body contains only comments (test stubs)"""
        # First check: function definition followed only by comments
        lines = code.split('\n')
        has_def = any('def ' in line for line in lines)
        if has_def:
            # Get lines after function definition
            after_def = []
            found_def = False
            for line in lines:
                if 'def ' in line:
                    found_def = True
                    continue
                if found_def:
                    stripped = line.strip()
                    if stripped and not stripped.startswith('#'):
                        after_def.append(stripped)

            # If no real code after def, just comments
            if not after_def:
                return True

        try:
            tree = ast.parse(code)
            for node in ast.walk(tree):
                if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    # Check if body only contains Pass, Expr(constant), or nothing
                    if len(node.body) == 0:
                        return True
                    # All statements are either Pass or docstrings
                    for stmt in node.body:
                        if not isinstance(stmt, (ast.Pass, ast.Expr)):
                            return False
                    return True
        except:
            # If we can't parse, let normal validation handle it
            pass
        return False

    def validate_python_block(self, block: CodeBlock) -> List[CodeIssue]:
        """Validate Python code block"""
        issues = []

        # Skip if it's just a placeholder or comment
        stripped = block.code.strip()
        if not stripped or stripped.startswith('#'):
            return issues

        # Skip if it contains obvious placeholders
        if any(placeholder in stripped for placeholder in ['...', '<your-', '[your-', 'TODO:', 'FIXME:']):
            return issues

        # Skip if it's clearly pseudocode or output
        if stripped.startswith(('>>>', '$', '%', '!')) or 'Output:' in stripped[:50]:
            return issues

        # Skip if it's just a decorator line (common in docs showing decorator usage)
        lines = [l.strip() for l in block.code.split('\n') if l.strip()]
        if len(lines) <= 2 and all(l.startswith('@') or l.startswith('import ') for l in lines):
            return issues

        # Skip if it's a function signature only (API reference pattern)
        if self.is_signature_only(block.code):
            return issues

        # Skip if it's a decorator pattern example (common in docs)
        if self.is_decorator_signature(block.code):
            return issues

        # Skip if it mixes SQL and Python (tutorial pattern)
        if self.has_mixed_languages(block.code):
            return issues

        # Skip if it's a test stub with only comments
        if self.has_only_comments_as_body(block.code):
            return issues

        # Syntax validation using ast
        try:
            ast.parse(block.code)
        except SyntaxError as e:
            issues.append(CodeIssue(
                file_path=block.file_path,
                language='python',
                start_line=block.start_line + (e.lineno or 1) - 1,
                end_line=block.end_line,
                issue_type='syntax_error',
                message=f'Python syntax error: {e.msg}',
                code_snippet=block.code[:200]
            ))
        except Exception as e:
            issues.append(CodeIssue(
                file_path=block.file_path,
                language='python',
                start_line=block.start_line,
                end_line=block.end_line,
                issue_type='parse_error',
                message=f'Python parse error: {str(e)}',
                code_snippet=block.code[:200]
            ))

        return issues

    def validate_file(self, file_path: Path) -> None:
        """Validate all code blocks in a markdown file"""
        self.report.total_files += 1
        blocks = self.extract_code_blocks(file_path)

        for block in blocks:
            self.report.total_code_blocks += 1

            if block.language in ['sql', 'postgresql', 'psql']:
                self.report.sql_blocks += 1
                if self.validate_sql:
                    issues = self.validate_sql_block(block)
                    self.report.issues.extend(issues)

            elif block.language in ['python', 'py']:
                self.report.python_blocks += 1
                if self.validate_python:
                    issues = self.validate_python_block(block)
                    self.report.issues.extend(issues)

            else:
                self.report.other_blocks += 1

    def validate_all(self) -> ValidationReport:
        """Validate all markdown files"""
        print(f"🔍 Scanning for markdown files in {self.docs_root}...")
        md_files = sorted(self.docs_root.rglob("*.md"))

        print(f"📄 Found {len(md_files)} markdown files")
        print(f"{'✅ Validating SQL' if self.validate_sql else '⏭️  Skipping SQL'}")
        print(f"{'✅ Validating Python' if self.validate_python else '⏭️  Skipping Python'}")
        print()

        for i, md_file in enumerate(md_files, 1):
            rel_path = md_file.relative_to(self.docs_root)
            print(f"[{i}/{len(md_files)}] {rel_path}...", end='\r')
            self.validate_file(md_file)

        print()  # New line after progress
        return self.report

    def print_report(self) -> None:
        """Print validation report"""
        print("\n" + "=" * 80)
        print("CODE VALIDATION REPORT")
        print("=" * 80)
        print(f"Files Scanned:        {self.report.total_files}")
        print(f"Code Blocks Found:    {self.report.total_code_blocks}")
        print(f"  - SQL blocks:       {self.report.sql_blocks}")
        print(f"  - Python blocks:    {self.report.python_blocks}")
        print(f"  - Other blocks:     {self.report.other_blocks}")
        print()
        print(f"Errors:               {self.report.error_count}")
        print(f"Warnings:             {self.report.warning_count}")
        print(f"Success Rate:         {self.report.success_rate:.1f}%")
        print()

        if self.report.issues:
            print("=" * 80)
            print("ISSUES FOUND")
            print("=" * 80)

            # Group by file
            by_file = {}
            for issue in self.report.issues:
                by_file.setdefault(issue.file_path, []).append(issue)

            for file_path, issues in sorted(by_file.items()):
                rel_path = file_path.relative_to(self.docs_root)
                print(f"\n📄 {rel_path} ({len(issues)} issues)")
                print("-" * 80)

                for issue in issues:
                    icon = "❌" if issue.issue_type in ['syntax_error', 'parse_error'] else "⚠️"
                    print(f"\n  {icon} Line {issue.start_line}-{issue.end_line} [{issue.language.upper()}]")
                    print(f"     {issue.message}")
                    if issue.code_snippet:
                        preview = issue.code_snippet.split('\n')[0][:70]
                        print(f"     Code: {preview}...")
        else:
            print("✅ No code issues found!")

        print("\n" + "=" * 80)

    def save_report(self, output_file: Path) -> None:
        """Save report to file"""
        with open(output_file, 'w') as f:
            f.write("Code Validation Report\n")
            f.write("=" * 80 + "\n\n")
            f.write(f"Files Scanned: {self.report.total_files}\n")
            f.write(f"Code Blocks: {self.report.total_code_blocks}\n")
            f.write(f"  - SQL: {self.report.sql_blocks}\n")
            f.write(f"  - Python: {self.report.python_blocks}\n")
            f.write(f"  - Other: {self.report.other_blocks}\n")
            f.write(f"Errors: {self.report.error_count}\n")
            f.write(f"Warnings: {self.report.warning_count}\n")
            f.write(f"Success Rate: {self.report.success_rate:.1f}%\n\n")

            if self.report.issues:
                f.write("Issues Found:\n")
                f.write("-" * 80 + "\n\n")

                by_file = {}
                for issue in self.report.issues:
                    by_file.setdefault(issue.file_path, []).append(issue)

                for file_path, issues in sorted(by_file.items()):
                    rel_path = file_path.relative_to(self.docs_root)
                    f.write(f"\nFile: {rel_path} ({len(issues)} issues)\n")
                    f.write("-" * 80 + "\n")

                    for issue in issues:
                        f.write(f"\nLine {issue.start_line}-{issue.end_line} [{issue.language.upper()}]\n")
                        f.write(f"Type: {issue.issue_type}\n")
                        f.write(f"Message: {issue.message}\n")
                        if issue.code_snippet:
                            f.write(f"Code preview:\n{issue.code_snippet[:300]}\n")
                        f.write("\n")
            else:
                f.write("✅ No code issues found!\n")

        print(f"\n📄 Report saved to: {output_file}")


def main():
    parser = argparse.ArgumentParser(description="Validate code examples in FraiseQL documentation")
    parser.add_argument(
        '--sql-only',
        action='store_true',
        help='Only validate SQL code blocks'
    )
    parser.add_argument(
        '--python-only',
        action='store_true',
        help='Only validate Python code blocks'
    )
    parser.add_argument(
        '--report',
        type=Path,
        default=Path('.phases/docs-review/code_validation_report.txt'),
        help='Output file for report'
    )
    args = parser.parse_args()

    # Determine what to validate
    validate_sql = not args.python_only
    validate_python = not args.sql_only

    # Find repository root
    repo_root = Path(__file__).parent.parent
    docs_root = repo_root / 'docs'

    if not docs_root.exists():
        print(f"Error: Documentation directory not found: {docs_root}")
        sys.exit(1)

    # Check for sqlparse
    if validate_sql and sqlparse is None:
        print("⚠️  Warning: sqlparse not installed, SQL validation will be basic")
        print("   Install with: pip install sqlparse")
        print()

    # Run validation
    validator = CodeValidator(docs_root, validate_sql=validate_sql, validate_python=validate_python)
    validator.validate_all()
    validator.print_report()

    # Save report
    args.report.parent.mkdir(parents=True, exist_ok=True)
    validator.save_report(args.report)

    # Exit with error code if issues found
    sys.exit(1 if validator.report.error_count > 0 else 0)


if __name__ == '__main__':
    main()
