#!/usr/bin/env python3
"""
Fix links to directories that should point to README.md files.

Converts:
  [Link](../examples/fastapi.md) → [Link](../examples/fastapi/README.md)
  [Link](docs/core.md) → [Link](docs/core/README.md)
"""

import re
from pathlib import Path
from typing import Tuple, Set


def find_directories_with_readme(base_path: Path) -> Set[str]:
    """Find all directories that contain README.md files."""
    readme_dirs = set()

    for readme in base_path.rglob("README.md"):
        # Get the directory containing the README
        dir_path = readme.parent
        # Get relative path from base
        rel_path = dir_path.relative_to(base_path)
        readme_dirs.add(str(rel_path))

    return readme_dirs


def fix_readme_links(content: str, readme_dirs: Set[str], file_path: Path, base_path: Path) -> Tuple[str, int]:
    """
    Fix links that should point to README.md files.

    Returns:
        (fixed_content, num_fixes)
    """
    num_fixes = 0
    lines = content.split('\n')

    for i, line in enumerate(lines):
        # Find all markdown links in this line
        # Pattern: [text](path.md)
        for match in re.finditer(r'\[([^\]]+)\]\(([^)]+\.md)\)', line):
            link_path = match.group(2)

            # Skip external URLs
            if link_path.startswith('http://') or link_path.startswith('https://'):
                continue

            # Resolve the link relative to the current file
            current_dir = file_path.parent

            # Handle the link path
            if link_path.startswith('../') or link_path.startswith('./'):
                # Relative link
                resolved = (current_dir / link_path).resolve()
            else:
                # Assume relative to current directory
                resolved = (current_dir / link_path).resolve()

            # Check if this points to a non-existent .md file
            if not resolved.exists():
                # Try to see if it's a directory with README
                # Remove the .md extension and check if directory exists
                potential_dir = resolved.parent / resolved.stem

                if potential_dir.is_dir() and (potential_dir / "README.md").exists():
                    # This should be a link to directory/README.md
                    # Calculate the new link path
                    if link_path.endswith('.md'):
                        new_link = link_path[:-3] + '/README.md'
                        old_pattern = f']({link_path})'
                        new_pattern = f']({new_link})'

                        if old_pattern in line:
                            lines[i] = line.replace(old_pattern, new_pattern)
                            num_fixes += 1

    return '\n'.join(lines), num_fixes


def process_file(file_path: Path, readme_dirs: Set[str], base_path: Path) -> int:
    """
    Process a single markdown file.

    Returns:
        Number of links fixed
    """
    content = file_path.read_text(encoding='utf-8')
    fixed_content, num_fixes = fix_readme_links(content, readme_dirs, file_path, base_path)

    if num_fixes > 0:
        file_path.write_text(fixed_content, encoding='utf-8')
        print(f"✓ {file_path}: {num_fixes} README links fixed")

    return num_fixes


def main():
    """Fix all markdown files in the repository."""
    base_path = Path(__file__).parent.parent

    # Find all directories with README.md
    print("Scanning for directories with README.md files...")
    readme_dirs = find_directories_with_readme(base_path)
    print(f"Found {len(readme_dirs)} directories with README.md")

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

        fixes = process_file(md_file, readme_dirs, base_path)
        total_fixes += fixes

    print(f"\n{'='*60}")
    print(f"Total files processed: {len(md_files)}")
    print(f"Total README links fixed: {total_fixes}")
    print(f"{'='*60}")


if __name__ == '__main__':
    main()
