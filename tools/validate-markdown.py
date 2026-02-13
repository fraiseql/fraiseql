#!/usr/bin/env python3

"""
FraiseQL Documentation Markdown Validation

Validates markdown files for syntax errors and formatting issues:
- Unclosed code blocks (```...```)
- Improper heading hierarchy (H2 must follow H1)
- Missing spaces after markdown syntax
- Trailing whitespace
- Mixed list formatting

Usage:
    python3 validate-markdown.py [--fix] [--verbose] [docs_dir]

Exit codes:
    0 = All files valid
    1 = Validation errors found
    2 = No markdown files found
    3 = Script error
"""

import os
import sys
import re
from pathlib import Path
from typing import List, Tuple, Dict
import argparse

# ANSI color codes
GREEN = '\033[0;32m'
RED = '\033[0;31m'
YELLOW = '\033[1;33m'
BLUE = '\033[0;34m'
NC = '\033[0m'  # No Color


class MarkdownValidator:
    def __init__(self, docs_dir: str, verbose: bool = False, fix: bool = False):
        self.docs_dir = Path(docs_dir).resolve()
        self.verbose = verbose
        self.fix = fix
        self.errors: Dict[str, List[str]] = {}
        self.warnings: Dict[str, List[str]] = {}
        self.checked_files = 0
        self.fixed_files = 0

    def log_error(self, message: str) -> None:
        """Log an error message"""
        print(f"{RED}✗{NC} {message}", file=sys.stderr)

    def log_warning(self, message: str) -> None:
        """Log a warning message"""
        print(f"{YELLOW}⚠{NC} {message}", file=sys.stderr)

    def log_success(self, message: str) -> None:
        """Log a success message"""
        print(f"{GREEN}✓{NC} {message}")

    def log_info(self, message: str) -> None:
        """Log an info message"""
        print(f"{BLUE}ℹ{NC} {message}")

    def check_code_blocks(self, lines: List[str], file_path: str) -> Tuple[bool, List[str]]:
        """Check for unclosed code blocks"""
        issues = []
        in_code_block = False
        code_fence_line = 0

        for i, line in enumerate(lines, 1):
            # Count code fences
            code_fences = line.count('```')

            if code_fences % 2 == 1:  # Odd number of fences toggles state
                if in_code_block:
                    in_code_block = False
                else:
                    in_code_block = True
                    code_fence_line = i

        if in_code_block:
            issues.append(f"Line {code_fence_line}: Unclosed code block")

        return len(issues) == 0, issues

    def check_heading_hierarchy(self, lines: List[str], file_path: str) -> Tuple[bool, List[str]]:
        """Check for proper heading hierarchy"""
        issues = []
        last_heading_level = 0
        in_code_block = False

        for i, line in enumerate(lines, 1):
            # Track code blocks
            if line.strip().startswith('```'):
                in_code_block = not in_code_block

            if in_code_block:
                continue

            # Extract heading level
            if line.startswith('#'):
                level = len(line) - len(line.lstrip('#'))

                # Check if there's a space after the hash
                if len(line) > level and line[level] != ' ':
                    issues.append(f"Line {i}: Missing space after heading marker (##...)")

                # Check hierarchy (can skip levels when going down, but not up)
                if last_heading_level > 0 and level > last_heading_level + 1:
                    issues.append(
                        f"Line {i}: Improper heading hierarchy (jumped from H{last_heading_level} to H{level})"
                    )

                last_heading_level = level

        return len(issues) == 0, issues

    def check_list_formatting(self, lines: List[str], file_path: str) -> Tuple[bool, List[str]]:
        """Check for consistent list formatting"""
        issues = []
        in_code_block = False

        for i, line in enumerate(lines, 1):
            # Track code blocks
            if line.strip().startswith('```'):
                in_code_block = not in_code_block

            if in_code_block:
                continue

            # Check for numbered lists without space after period
            if re.match(r'^\d+\.', line):
                if not re.match(r'^\d+\. ', line):
                    issues.append(f"Line {i}: Numbered list item missing space after period")

        return len(issues) == 0, issues

    def check_trailing_whitespace(self, lines: List[str], file_path: str) -> Tuple[bool, List[str]]:
        """Check for trailing whitespace"""
        issues = []

        for i, line in enumerate(lines, 1):
            # Don't count the newline, check actual trailing spaces/tabs
            if line.rstrip('\n') != line.rstrip():
                issues.append(f"Line {i}: Trailing whitespace")

        return len(issues) == 0, issues

    def check_link_syntax(self, lines: List[str], file_path: str) -> Tuple[bool, List[str]]:
        """Check for malformed links"""
        issues = []
        in_code_block = False

        for i, line in enumerate(lines, 1):
            # Track code blocks
            if line.strip().startswith('```'):
                in_code_block = not in_code_block

            if in_code_block:
                continue

            # Check for mismatched brackets
            open_brackets = line.count('[')
            close_brackets = line.count(']')

            # Simple check - should be equal (ignoring escaped brackets)
            if open_brackets != close_brackets and '[' in line and ']' in line:
                # Check if it looks like markdown link pattern
                if re.search(r'\[([^\]]*)\]\(', line):
                    # Verify closing paren exists
                    if ')' not in line:
                        issues.append(f"Line {i}: Malformed link - missing closing parenthesis")

        return len(issues) == 0, issues

    def fix_trailing_whitespace(self, content: str) -> str:
        """Fix trailing whitespace"""
        lines = content.split('\n')
        fixed_lines = [line.rstrip() for line in lines]
        return '\n'.join(fixed_lines)

    def validate_file(self, markdown_file: Path) -> None:
        """Validate a single markdown file"""
        self.checked_files += 1
        relative_path = markdown_file.relative_to(self.docs_dir)

        try:
            with open(markdown_file, 'r', encoding='utf-8') as f:
                content = f.read()
        except (IOError, OSError) as e:
            self.log_error(f"Cannot read {relative_path}: {e}")
            return

        lines = content.split('\n')
        file_errors = []
        file_warnings = []

        # Run all checks
        checks = [
            ("Code blocks", self.check_code_blocks),
            ("Heading hierarchy", self.check_heading_hierarchy),
            ("List formatting", self.check_list_formatting),
            ("Link syntax", self.check_link_syntax),
            ("Trailing whitespace", self.check_trailing_whitespace),
        ]

        for check_name, check_func in checks:
            valid, issues = check_func(lines, str(relative_path))
            if not valid:
                file_errors.extend(issues)

        if file_errors:
            self.errors[str(relative_path)] = file_errors
            for error in file_errors:
                self.log_error(f"{relative_path}: {error}")

        # Try to fix if requested
        if self.fix and file_errors:
            try:
                fixed_content = self.fix_trailing_whitespace(content)
                with open(markdown_file, 'w', encoding='utf-8') as f:
                    f.write(fixed_content)
                self.fixed_files += 1
                self.log_info(f"Fixed {relative_path} (trailing whitespace)")
            except (IOError, OSError) as e:
                self.log_error(f"Cannot write {relative_path}: {e}")

        if self.verbose and not file_errors:
            self.log_success(f"Valid: {relative_path}")

    def validate_docs(self) -> int:
        """Validate all markdown files"""
        if not self.docs_dir.exists():
            self.log_error(f"Documentation directory not found: {self.docs_dir}")
            return 3

        # Find all markdown files
        markdown_files = sorted(self.docs_dir.glob('**/*.md'))

        if not markdown_files:
            self.log_error(f"No markdown files found in {self.docs_dir}")
            return 2

        self.log_info(f"FraiseQL Documentation Markdown Validator")
        self.log_info(f"Checking documentation in: {self.docs_dir}")
        self.log_info(f"Found {len(markdown_files)} markdown files")
        if self.fix:
            self.log_info("Running in FIX mode - will repair fixable issues")
        print()

        # Validate each file
        for markdown_file in markdown_files:
            self.validate_file(markdown_file)

        # Print summary
        print()
        print("━" * 50)

        if not self.errors:
            self.log_success(f"All {self.checked_files} files passed validation!")
            return 0
        else:
            self.log_error(
                f"Found {len(self.errors)} file(s) with {sum(len(v) for v in self.errors.values())} issue(s)"
            )
            if self.fixed_files > 0:
                self.log_success(f"Fixed {self.fixed_files} file(s)")
            return 1


def main():
    """Main entry point"""
    parser = argparse.ArgumentParser(
        description="Validate markdown syntax in FraiseQL documentation"
    )

    parser.add_argument(
        'docs_dir',
        nargs='?',
        default='.',
        help='Documentation directory (default: current directory)'
    )

    parser.add_argument(
        '-v', '--verbose',
        action='store_true',
        help='Print verbose output'
    )

    parser.add_argument(
        '-f', '--fix',
        action='store_true',
        help='Automatically fix fixable issues (like trailing whitespace)'
    )

    args = parser.parse_args()

    validator = MarkdownValidator(
        args.docs_dir,
        verbose=args.verbose,
        fix=args.fix
    )

    return validator.validate_docs()


if __name__ == '__main__':
    sys.exit(main())
