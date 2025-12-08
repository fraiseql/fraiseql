#!/usr/bin/env python3
"""
Rename documentation files to kebab-case and update all references.
Part of WP-032: Standardize Documentation File Naming to Kebab-Case
"""
import os
import re
import subprocess
from pathlib import Path
from typing import Dict, List

def to_kebab_case(filename: str) -> str:
    """Convert filename to kebab-case."""
    # Keep extension
    name, ext = os.path.splitext(filename)

    # Skip README and other conventional uppercase files
    if name in ('README', 'CHANGELOG', 'CONTRIBUTING', 'LICENSE'):
        return filename

    # Convert UPPERCASE or snake_case to kebab-case
    # UPPERCASE → lowercase
    name = name.lower()

    # snake_case → kebab-case
    name = name.replace('_', '-')

    return f"{name}{ext}"

def find_rename_candidates(docs_dir: Path) -> Dict[Path, Path]:
    """Find all files that need renaming."""
    renames = {}

    for md_file in docs_dir.rglob("*.md"):
        old_name = md_file.name
        new_name = to_kebab_case(old_name)

        if old_name != new_name:
            new_path = md_file.parent / new_name
            renames[md_file] = new_path

    return renames

def update_references(file_path: Path, old_name: str, new_name: str) -> bool:
    """Update all references to old filename in a file."""
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
    except (UnicodeDecodeError, PermissionError):
        return False

    original_content = content

    # Replace in markdown links and references
    content = content.replace(old_name, new_name)

    if content != original_content:
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(content)
        return True

    return False

def main():
    docs_dir = Path("docs")

    print("=== FraiseQL Documentation Kebab-Case Renaming ===\n")

    # Find all files to rename
    renames = find_rename_candidates(docs_dir)

    if not renames:
        print("✅ No files need renaming - all docs already use kebab-case!")
        return

    print(f"Found {len(renames)} files to rename:\n")
    for old_path, new_path in sorted(renames.items()):
        print(f"  {old_path.relative_to(docs_dir)} → {new_path.name}")

    print("\n" + "="*60)

    # Step 1: Update all references in .md files (docs + .phases + examples)
    print("\n[1/3] Updating references in markdown files...")
    all_md_files = []
    for pattern in ["docs/**/*.md", ".phases/**/*.md", "examples/**/*.md"]:
        all_md_files.extend(Path(".").glob(pattern))

    total_updated = 0
    for old_path, new_path in renames.items():
        old_name = old_path.name
        new_name = new_path.name

        updated_count = 0
        for md_file in all_md_files:
            if update_references(md_file, old_name, new_name):
                updated_count += 1

        if updated_count > 0:
            print(f"  Updated {updated_count} files for {old_name} → {new_name}")
            total_updated += updated_count

    print(f"\n  Total: Updated {total_updated} file references")

    # Step 2: Rename files (use git mv for proper tracking)
    print("\n[2/3] Renaming files with git mv...")
    for old_path, new_path in sorted(renames.items()):
        try:
            subprocess.run(['git', 'mv', str(old_path), str(new_path)],
                          check=True, capture_output=True)
            print(f"  ✓ {old_path.name} → {new_path.name}")
        except subprocess.CalledProcessError as e:
            print(f"  ✗ Failed to rename {old_path.name}: {e.stderr.decode()}")

    # Step 3: Summary
    print(f"\n[3/3] Summary")
    print(f"  ✅ Renamed {len(renames)} files to kebab-case")
    print(f"  ✅ Updated {total_updated} references in markdown files")

    print("\n" + "="*60)
    print("\nNext steps:")
    print("  1. Review changes: git diff --staged")
    print("  2. Run link validation: ./scripts/validate-docs.sh links")
    print("  3. Commit: git commit -m 'docs: Standardize file naming to kebab-case [WP-032]'")

if __name__ == "__main__":
    main()
