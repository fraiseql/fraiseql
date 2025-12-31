#!/usr/bin/env python3
"""
Fix directory-style markdown links to file-style links.

Converts:
  [Link](path/to/doc/) → [Link](path/to/doc.md)
  [Link](./doc/) → [Link](./doc.md)
  [Link](../doc/index/) → [Link](../doc/index.md)
"""

import re
from pathlib import Path
from typing import Tuple


def fix_markdown_links(content: str) -> Tuple[str, int]:
    """
    Fix directory-style links in markdown content.

    Returns:
        (fixed_content, num_fixes)
    """
    # Pattern: [text](path/) where path can contain ../  ./  or plain paths
    # Must end with / and not be a URL (http://, https://)
    pattern = r'\[([^\]]+)\]\((?!https?://)(\.\.?/)?([a-zA-Z0-9_/-]+)/\)'

    def replace_link(match):
        text = match.group(1)  # Link text
        prefix = match.group(2) or ''  # ../ or ./ or empty
        path = match.group(3)  # path/to/doc

        # Convert to .md link
        return f'[{text}]({prefix}{path}.md)'

    fixed_content, num_subs = re.subn(pattern, replace_link, content)
    return fixed_content, num_subs


def process_file(file_path: Path) -> int:
    """
    Process a single markdown file.

    Returns:
        Number of links fixed
    """
    content = file_path.read_text(encoding='utf-8')
    fixed_content, num_fixes = fix_markdown_links(content)

    if num_fixes > 0:
        file_path.write_text(fixed_content, encoding='utf-8')
        print(f"✓ {file_path}: {num_fixes} links fixed")

    return num_fixes


def main():
    """Fix all markdown files in the repository."""
    base_path = Path(__file__).parent.parent

    # Find all markdown files
    md_files = []
    for pattern in ['docs/**/*.md', 'examples/**/*.md', '*.md']:
        md_files.extend(base_path.glob(pattern))

    # Process files
    total_fixes = 0
    for md_file in sorted(md_files):
        # Skip archived files (they're not actively maintained)
        if 'archive' in md_file.parts:
            continue

        fixes = process_file(md_file)
        total_fixes += fixes

    print(f"\n{'='*60}")
    print(f"Total files processed: {len(md_files)}")
    print(f"Total links fixed: {total_fixes}")
    print(f"{'='*60}")


if __name__ == '__main__':
    main()
