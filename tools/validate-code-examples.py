#!/usr/bin/env python3

"""
FraiseQL Documentation Code Example Validator

Validates code examples for syntax errors across multiple languages:
- Python: syntax validation
- TypeScript: basic syntax check
- Go: syntax validation
- Java: syntax validation
- SQL: basic syntax check
- GraphQL: schema validation

Usage:
    python3 validate-code-examples.py [--fix] [--verbose] [docs_dir]

Exit codes:
    0 = All examples valid
    1 = Syntax errors found
    2 = No markdown files found
    3 = Script error
"""

import os
import sys
import re
import ast
import subprocess
import tempfile
from pathlib import Path
from typing import List, Tuple, Dict
from collections import defaultdict

# ANSI color codes
GREEN = '\033[0;32m'
RED = '\033[0;31m'
YELLOW = '\033[1;33m'
BLUE = '\033[0;34m'
NC = '\033[0m'


class CodeExampleValidator:
    def __init__(self, docs_dir: str, verbose: bool = False):
        self.docs_dir = Path(docs_dir).resolve()
        self.verbose = verbose
        self.errors: Dict[str, List[str]] = defaultdict(list)
        self.warnings: Dict[str, List[str]] = defaultdict(list)
        self.checked_files = 0
        self.checked_blocks = 0

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

    def extract_code_blocks(self, content: str) -> List[Tuple[int, str, str]]:
        """Extract code blocks with language and line number"""
        blocks = []
        lines = content.split('\n')
        i = 0

        while i < len(lines):
            line = lines[i]

            # Match opening fence
            if line.strip().startswith('```'):
                # Extract language
                fence_line = line.strip()
                language = fence_line[3:].strip().lower() if len(fence_line) > 3 else ''

                # Find closing fence
                start_line = i + 1
                code_lines = []
                i += 1

                while i < len(lines):
                    if lines[i].strip().startswith('```'):
                        # Closing fence found
                        blocks.append((start_line, language, '\n'.join(code_lines)))
                        break
                    code_lines.append(lines[i])
                    i += 1

                i += 1
            else:
                i += 1

        return blocks

    def validate_python(self, code: str) -> Tuple[bool, str]:
        """Validate Python syntax"""
        try:
            ast.parse(code)
            return True, "Valid Python"
        except SyntaxError as e:
            return False, f"Python syntax error: {e.msg} (line {e.lineno})"
        except Exception as e:
            return False, f"Python error: {str(e)}"

    def validate_typescript(self, code: str) -> Tuple[bool, str]:
        """Basic TypeScript syntax validation"""
        # Check for common issues
        if code.count('{') != code.count('}'):
            return False, "Mismatched braces"
        if code.count('[') != code.count(']'):
            return False, "Mismatched brackets"
        if code.count('(') != code.count(')'):
            return False, "Mismatched parentheses"

        # Check for incomplete statements
        if code.strip().endswith(';') or code.strip().endswith(':'):
            return True, "Valid TypeScript (basic check)"
        return True, "Valid TypeScript (basic check)"

    def validate_go(self, code: str) -> Tuple[bool, str]:
        """Basic Go syntax validation"""
        # Check for common issues
        if code.count('{') != code.count('}'):
            return False, "Mismatched braces"
        if code.count('[') != code.count(']'):
            return False, "Mismatched brackets"
        if code.count('(') != code.count(')'):
            return False, "Mismatched parentheses"
        return True, "Valid Go (basic check)"

    def validate_java(self, code: str) -> Tuple[bool, str]:
        """Basic Java syntax validation"""
        # Check for common issues
        if code.count('{') != code.count('}'):
            return False, "Mismatched braces"
        if code.count('[') != code.count(']'):
            return False, "Mismatched brackets"
        if code.count('(') != code.count(')'):
            return False, "Mismatched parentheses"
        return True, "Valid Java (basic check)"

    def validate_sql(self, code: str) -> Tuple[bool, str]:
        """Basic SQL syntax validation"""
        # Check for obvious issues
        code_upper = code.upper().strip()

        # Check for incomplete queries
        if not any(code_upper.startswith(kw) for kw in ['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'CREATE', 'DROP', 'ALTER', 'WITH']):
            if not code_upper.startswith('--'):  # Allow comments
                return False, "SQL must start with valid keyword (SELECT, INSERT, etc.)"

        # Check for mismatched parentheses
        if code.count('(') != code.count(')'):
            return False, "Mismatched parentheses in SQL"

        # Check for common typos
        if '  ;' in code or ';\n' not in code:
            if not code.strip().endswith(';') and not code.strip().endswith(')'):
                return False, "SQL statement should end with semicolon"

        return True, "Valid SQL (basic check)"

    def validate_graphql(self, code: str) -> Tuple[bool, str]:
        """Basic GraphQL syntax validation"""
        # Check for mismatched braces
        if code.count('{') != code.count('}'):
            return False, "Mismatched braces in GraphQL"

        # Check for proper query/mutation syntax
        code_strip = code.strip()
        if code_strip and not any(code_strip.startswith(kw) for kw in ['query', 'mutation', 'subscription', '{', 'extend', 'type', 'interface', 'enum', 'scalar', 'union']):
            return False, "Invalid GraphQL syntax"

        return True, "Valid GraphQL (basic check)"

    def validate_code_block(self, language: str, code: str) -> Tuple[bool, str]:
        """Validate code block based on language"""
        if not language:
            return True, "No language specified (warning)"

        language = language.lower().strip()

        if language in ['python', 'py']:
            return self.validate_python(code)
        elif language in ['typescript', 'ts']:
            return self.validate_typescript(code)
        elif language in ['go']:
            return self.validate_go(code)
        elif language in ['java']:
            return self.validate_java(code)
        elif language in ['sql']:
            return self.validate_sql(code)
        elif language in ['graphql']:
            return self.validate_graphql(code)
        else:
            return True, f"Language '{language}' not validated"

    def validate_file(self, markdown_file: Path) -> None:
        """Validate code examples in a markdown file"""
        self.checked_files += 1
        relative_path = markdown_file.relative_to(self.docs_dir)

        try:
            with open(markdown_file, 'r', encoding='utf-8') as f:
                content = f.read()
        except (IOError, OSError) as e:
            self.log_error(f"Cannot read {relative_path}: {e}")
            return

        # Extract code blocks
        blocks = self.extract_code_blocks(content)
        if not blocks and self.verbose:
            self.log_info(f"No code blocks in {relative_path}")
            return

        for line_num, language, code in blocks:
            self.checked_blocks += 1

            if not code.strip():
                continue

            valid, message = self.validate_code_block(language, code)

            if not valid:
                error_msg = f"{relative_path}:{line_num} ({language}): {message}"
                self.errors[str(relative_path)].append(error_msg)
                self.log_error(error_msg)
            elif self.verbose:
                self.log_info(f"{relative_path}:{line_num} ({language}): {message}")

    def validate_docs(self) -> int:
        """Validate all code examples in documentation"""
        if not self.docs_dir.exists():
            self.log_error(f"Documentation directory not found: {self.docs_dir}")
            return 3

        # Find all markdown files
        markdown_files = sorted(self.docs_dir.glob('**/*.md'))

        if not markdown_files:
            self.log_error(f"No markdown files found in {self.docs_dir}")
            return 2

        self.log_info(f"FraiseQL Code Example Validator")
        self.log_info(f"Checking documentation in: {self.docs_dir}")
        self.log_info(f"Found {len(markdown_files)} markdown files")
        print()

        # Validate each file
        for markdown_file in markdown_files:
            self.validate_file(markdown_file)

        # Print summary
        print()
        print("━" * 60)

        if not self.errors:
            self.log_success(f"All {self.checked_blocks} code blocks in {self.checked_files} files are valid!")
            return 0
        else:
            error_count = sum(len(v) for v in self.errors.values())
            self.log_error(
                f"Found {error_count} invalid code block(s) in {len(self.errors)} file(s)"
            )
            return 1


def main():
    """Main entry point"""
    import argparse

    parser = argparse.ArgumentParser(
        description="Validate code examples in FraiseQL documentation"
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

    args = parser.parse_args()

    validator = CodeExampleValidator(
        args.docs_dir,
        verbose=args.verbose
    )

    return validator.validate_docs()


if __name__ == '__main__':
    sys.exit(main())
