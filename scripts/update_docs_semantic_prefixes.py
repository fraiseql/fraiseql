#!/usr/bin/env python3
"""
Update FraiseQL documentation to remove legacy error patterns.

Replaces:
- validation_error: → validation:
- failed:validation → validation:
- failed:not_found → not_found:
"""

import re
from pathlib import Path

# Patterns to replace
REPLACEMENTS = [
    # validation_error: → validation:
    (r'`validation_error:', r'`validation:'),
    (r"'validation_error:", r"'validation:"),
    (r'"validation_error:', r'"validation:'),
    (r'\|validation_error:', r'|validation:'),

    # failed:validation → validation:
    (r'`failed:validation([:`])', r'`validation:\1'),
    (r"'failed:validation([:'`])", r"'validation:\1"),
    (r'"failed:validation([:"`])', r'"validation:\1'),
    (r'\|failed:validation([|`])', r'|validation:\1'),

    # failed:not_found → not_found:
    (r'`failed:not_found([:`])', r'`not_found:\1'),
    (r"'failed:not_found([:'`])", r"'not_found:\1"),
    (r'"failed:not_found([:"`])', r'"not_found:\1'),
    (r'\|failed:not_found([|`])', r'|not_found:\1'),
]

def update_file(file_path: Path) -> bool:
    """Update a single file. Returns True if changes were made."""
    content = file_path.read_text()
    original = content

    for pattern, replacement in REPLACEMENTS:
        content = re.sub(pattern, replacement, content)

    if content != original:
        file_path.write_text(content)
        return True
    return False

def main():
    """Update all documentation files."""
    docs_dir = Path("docs")

    if not docs_dir.exists():
        print(f"Error: {docs_dir} not found")
        return 1

    # Find all markdown files
    md_files = list(docs_dir.rglob("*.md"))
    print(f"Found {len(md_files)} markdown files\n")

    files_changed = 0

    for md_file in md_files:
        if update_file(md_file):
            files_changed += 1
            print(f"✓ {md_file.relative_to(docs_dir.parent)}")

    print(f"\n{'='*60}")
    print(f"Documentation Update Complete!")
    print(f"  Files changed: {files_changed}/{len(md_files)}")
    print(f"{'='*60}\n")

    if files_changed > 0:
        print("Removed legacy patterns:")
        print("  - validation_error: → validation:")
        print("  - failed:validation → validation:")
        print("  - failed:not_found → not_found:")

    return 0

if __name__ == "__main__":
    exit(main())
