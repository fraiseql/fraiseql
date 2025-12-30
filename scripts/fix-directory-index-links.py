#!/usr/bin/env python3
"""
Fix links to missing .md files that should point to directory README.md files.

Converts:
  [Link](docs/performance.md) → [Link](docs/performance/README.md)
  [Link](../core.md) → [Link](../core/README.md)
"""

import re
from pathlib import Path
from typing import Tuple


# Known directory mappings - directories that exist with README.md files
DIRECTORY_MAPPINGS = {
    'performance': 'performance/README.md',
    'core': 'core/README.md',
    'production': 'production/README.md',
    'advanced': 'advanced/README.md',
    'reference': 'reference/README.md',
    'examples': 'examples/README.md',
    'enterprise': 'advanced/README.md',  # enterprise → advanced
}


def fix_directory_index_links(content: str, file_path: Path) -> Tuple[str, int]:
    """
    Fix links to directory index files.

    Returns:
        (fixed_content, num_fixes)
    """
    num_fixes = 0

    for dir_name, target in DIRECTORY_MAPPINGS.items():
        # Pattern variations to match
        patterns = [
            (f']({dir_name}.md)', f']({target})'),
            (f'](../{dir_name}.md)', f'](../{target})'),
            (f'](../../{dir_name}.md)', f'](../../{target})'),
            (f'](docs/{dir_name}.md)', f'](docs/{target})'),
            (f'](../docs/{dir_name}.md)', f'](../docs/{target})'),
        ]

        for old, new in patterns:
            if old in content:
                content = content.replace(old, new)
                num_fixes += 1

    return content, num_fixes


def process_file(file_path: Path) -> int:
    """
    Process a single markdown file.

    Returns:
        Number of links fixed
    """
    content = file_path.read_text(encoding='utf-8')
    fixed_content, num_fixes = fix_directory_index_links(content, file_path)

    if num_fixes > 0:
        file_path.write_text(fixed_content, encoding='utf-8')
        print(f"✓ {file_path}: {num_fixes} directory index links fixed")

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
    print(f"Total directory index links fixed: {total_fixes}")
    print(f"{'='*60}")


if __name__ == '__main__':
    main()
