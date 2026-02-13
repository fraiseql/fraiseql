#!/usr/bin/env python3

"""
FraiseQL Documentation Link Validator

Validates internal and external markdown links in documentation.
- Checks for broken internal links
- Validates relative path references
- Supports both [text](link) and [text]: link markdown formats
- Optional external link validation

Usage:
    python3 validate-docs-links.py [--verbose] [--check-external] [docs_dir]

Exit codes:
    0 = All links valid
    1 = Broken internal links found
    2 = No markdown files found
    3 = Script error
"""

import os
import sys
import re
import argparse
import urllib.request
import urllib.error
from pathlib import Path
from typing import Set, Tuple, List
from collections import defaultdict

# ANSI color codes
GREEN = '\033[0;32m'
RED = '\033[0;31m'
YELLOW = '\033[1;33m'
BLUE = '\033[0;34m'
NC = '\033[0m'  # No Color


class LinkValidator:
    def __init__(self, docs_dir: str, verbose: bool = False, check_external: bool = False):
        self.docs_dir = Path(docs_dir).resolve()
        self.verbose = verbose
        self.check_external = check_external
        self.errors: List[str] = []
        self.warnings: List[str] = []
        self.checked_files = 0

    def log_error(self, message: str) -> None:
        """Log an error message"""
        print(f"{RED}✗{NC} {message}", file=sys.stderr)
        self.errors.append(message)

    def log_warning(self, message: str) -> None:
        """Log a warning message"""
        print(f"{YELLOW}⚠{NC} {message}", file=sys.stderr)
        self.warnings.append(message)

    def log_success(self, message: str) -> None:
        """Log a success message"""
        print(f"{GREEN}✓{NC} {message}")

    def log_info(self, message: str) -> None:
        """Log an info message"""
        print(f"{BLUE}ℹ{NC} {message}")

    def extract_links(self, content: str) -> Set[str]:
        """Extract all links from markdown content"""
        links = set()

        # Pattern 1: [text](url)
        pattern1 = r'\[([^\]]*)\]\(([^)]+)\)'
        for match in re.finditer(pattern1, content):
            link = match.group(2)
            links.add(link)

        # Pattern 2: [text]: url
        pattern2 = r'^\s*\[([^\]]+)\]:\s+(\S+)'
        for match in re.finditer(pattern2, content, re.MULTILINE):
            link = match.group(2)
            links.add(link)

        return links

    def resolve_link(self, link: str, file_dir: Path) -> Path:
        """Resolve a relative link to an absolute path"""
        # Remove fragments and query strings
        link_path = link.split('#')[0].split('?')[0]

        if not link_path:
            return None

        # Skip external URLs
        if link_path.startswith('http://') or link_path.startswith('https://'):
            return None

        # Handle different path formats
        if link_path.startswith('/'):
            # Absolute path from docs root
            resolved = self.docs_dir / link_path[1:]
        else:
            # Relative path
            resolved = (file_dir / link_path).resolve()

        return resolved

    def link_exists(self, resolved_path: Path) -> bool:
        """Check if a link target exists"""
        if not resolved_path:
            return True

        # Check direct file
        if resolved_path.exists() and resolved_path.is_file():
            return True

        # Check with .md extension
        md_path = resolved_path.with_suffix('.md')
        if md_path.exists():
            return True

        # Check as directory with README.md
        readme_path = resolved_path / 'README.md'
        if readme_path.exists():
            return True

        return False

    def check_external_link(self, url: str, file: str) -> bool:
        """Check if an external URL is reachable"""
        try:
            req = urllib.request.Request(url, method='HEAD')
            with urllib.request.urlopen(req, timeout=3) as response:
                return response.status < 400
        except (urllib.error.URLError, urllib.error.HTTPError, Exception):
            return False

    def validate_file(self, markdown_file: Path) -> None:
        """Validate links in a single markdown file"""
        self.checked_files += 1
        relative_path = markdown_file.relative_to(self.docs_dir)

        try:
            with open(markdown_file, 'r', encoding='utf-8') as f:
                content = f.read()
        except (IOError, OSError) as e:
            self.log_error(f"Cannot read {relative_path}: {e}")
            return

        # Extract all links
        links = self.extract_links(content)

        if self.verbose and links:
            self.log_info(f"Found {len(links)} links in {relative_path}")

        for link in links:
            # Skip anchors and empty links
            if not link or link.startswith('#'):
                continue

            # Handle external URLs
            if link.startswith('http://') or link.startswith('https://'):
                if self.check_external:
                    if not self.check_external_link(link, str(relative_path)):
                        self.log_warning(f"Unreachable external link in {relative_path}: {link}")
                continue

            # Check internal link
            resolved = self.resolve_link(link, markdown_file.parent)

            if resolved and not self.link_exists(resolved):
                self.log_error(
                    f"Broken link in {relative_path}: {link} "
                    f"(resolved to: {resolved.relative_to(self.docs_dir) if resolved.is_relative_to(self.docs_dir) else resolved})"
                )
            elif self.verbose and resolved:
                self.log_info(f"Valid link in {relative_path}: {link}")

    def validate_docs(self) -> int:
        """Validate all markdown files in the documentation directory"""
        if not self.docs_dir.exists():
            self.log_error(f"Documentation directory not found: {self.docs_dir}")
            return 3

        # Find all markdown files
        markdown_files = sorted(self.docs_dir.glob('**/*.md'))

        if not markdown_files:
            self.log_error(f"No markdown files found in {self.docs_dir}")
            return 2

        self.log_info(f"FraiseQL Documentation Link Validator")
        self.log_info(f"Checking documentation in: {self.docs_dir}")
        self.log_info(f"Found {len(markdown_files)} markdown files")
        print()

        # Validate each file
        for markdown_file in markdown_files:
            self.validate_file(markdown_file)

        # Print summary
        print()
        print("━" * 50)

        if not self.errors:
            self.log_success(f"All {self.checked_files} files validated successfully!")
            if self.warnings:
                print(f"Warnings: {len(self.warnings)}")
            return 0
        else:
            self.log_error(
                f"Found {len(self.errors)} broken link(s) in {self.checked_files} file(s)"
            )
            if self.warnings:
                print(f"Warnings: {len(self.warnings)}")
            return 1


def main():
    """Main entry point"""
    parser = argparse.ArgumentParser(
        description="Validate internal and external markdown links in FraiseQL documentation"
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
        '-e', '--check-external',
        action='store_true',
        help='Also check external URLs (slower)'
    )

    args = parser.parse_args()

    validator = LinkValidator(
        args.docs_dir,
        verbose=args.verbose,
        check_external=args.check_external
    )

    return validator.validate_docs()


if __name__ == '__main__':
    sys.exit(main())
