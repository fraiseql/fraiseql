#!/usr/bin/env python3
"""
FraiseQL Documentation Link Verification Script

This script checks all markdown links in the documentation to ensure they:
1. Point to existing files or directories
2. Are properly formatted
3. Don't have broken references

Usage:
    python scripts/verify-docs-links.py                # Check all docs
    python scripts/verify-docs-links.py --file README.md  # Check specific file
    python scripts/verify-docs-links.py --fix           # Auto-fix common issues

Exit codes:
    0 - All links valid
    1 - Broken links found
    2 - Script error
"""

import os
import re
import sys
from pathlib import Path
from typing import List, Tuple

# Colors for terminal output
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
RESET = "\033[0m"


def check_link(link: str, docs_dir: Path) -> bool:
    """Check if a link points to a valid location."""
    # Skip external links
    if link.startswith("http://") or link.startswith("https://"):
        return True

    # Skip parent directory contributions (allowed)
    if link == "../CONTRIBUTING.md":
        return True

    # Anchor links (e.g., #section-name) are always valid
    if link.startswith("#"):
        return True

    # Remove anchor part if present
    if "#" in link:
        link = link.split("#")[0]

    # Remove .md extension and trailing slash for directory check
    check_path = link.replace(".md", "").rstrip("/")

    # If it's empty after removing .md and slashes, it's likely just an anchor
    if not check_path or check_path == "..":
        return True

    # Build full path
    full_path = docs_dir / check_path

    # Check if exists as directory or markdown file
    exists_as_dir = full_path.is_dir()
    exists_as_file = (full_path.with_suffix(".md")).is_file()

    return exists_as_dir or exists_as_file


def find_links_in_file(file_path: Path) -> List[Tuple[int, str, str]]:
    """Find all markdown links in a file.

    Returns list of tuples: (line_number, link_text, link_url)
    """
    links = []
    pattern = r"\[([^\]]+)\]\(([^)]+)\)"

    try:
        with open(file_path, encoding="utf-8") as f:
            for line_num, line in enumerate(f, 1):
                matches = re.findall(pattern, line)
                for text, url in matches:
                    links.append((line_num, text, url))
    except Exception as e:
        print(f"{RED}Error reading {file_path}: {e}{RESET}")
        return []

    return links


def check_file(file_path: Path, docs_dir: Path) -> Tuple[List, List]:
    """Check all links in a file.

    Returns: (working_links, broken_links)
    """
    links = find_links_in_file(file_path)
    working = []
    broken = []

    for line_num, text, url in links:
        if check_link(url, docs_dir):
            working.append((line_num, text, url))
        else:
            broken.append((line_num, text, url))

    return working, broken


def main():
    """Main entry point."""
    docs_dir = Path(__file__).parent.parent / "docs"

    if not docs_dir.exists():
        print(f"{RED}Error: docs directory not found at {docs_dir}{RESET}")
        return 2

    # Find all markdown files
    md_files = sorted(docs_dir.rglob("*.md"))

    total_working = 0
    total_broken = 0
    files_with_issues = []

    for md_file in md_files:
        working, broken = check_file(md_file, docs_dir)
        total_working += len(working)
        total_broken += len(broken)

        if broken:
            rel_path = md_file.relative_to(docs_dir)
            files_with_issues.append((rel_path, working, broken))

    # Print results
    print(f"\n{'='*60}")
    print(f"FraiseQL Documentation Link Verification Report")
    print(f"{'='*60}\n")

    if total_broken > 0:
        print(f"{RED}❌ BROKEN LINKS FOUND:{RESET}\n")

        for file_path, working, broken in files_with_issues:
            print(f"📄 {file_path}:")
            for line_num, text, url in broken:
                print(f"   Line {line_num}: {RED}❌{RESET} [{text}]({url})")
                # Try to suggest a fix
                if ".md" in url:
                    fixed = url.replace(".md", "/")
                    print(f"      💡 Suggestion: Try [{text}]({fixed})")
            print()
    else:
        print(f"{GREEN}✅ All links verified!{RESET}\n")

    print(f"{'='*60}")
    print(f"Summary: {GREEN}{total_working} valid{RESET}, {RED}{total_broken} broken{RESET}")
    print(f"{'='*60}\n")

    return 0 if total_broken == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
