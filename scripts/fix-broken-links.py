#!/usr/bin/env python3
"""
Fix broken documentation links after kebab-case renaming.
Part of WP-033: Fix Broken Links Post-Rename
"""

import re
from pathlib import Path
from typing import Dict, List, Tuple

# Mapping of old references → new references
LINK_FIXES = {
    # Root files
    "docs/strategic/VERSION_STATUS.md": "docs/strategic/version-status.md",
    "docs/development/PHILOSOPHY.md": "docs/development/philosophy.md",
    # Common patterns
    "trinity-pattern.md": "../core/trinity-pattern.md",  # from docs/database/
    "./naming-conventions.md": "../database/table-naming-conventions.md",  # from docs/core/
    "./view-strategies.md": "../database/view-strategies.md",  # from docs/core/
    "./performance-tuning.md": "../performance/performance-guide.md",  # from docs/core/
    # Security/compliance
    "../advanced/audit-trails.md": "../enterprise/audit-logging.md",
}


def fix_links_in_file(file_path: Path, fixes: Dict[str, str]) -> int:
    """Fix broken links in a markdown file."""
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            content = f.read()
    except (UnicodeDecodeError, PermissionError):
        return 0

    original_content = content
    fixes_applied = 0

    for old_ref, new_ref in fixes.items():
        if old_ref in content:
            content = content.replace(old_ref, new_ref)
            fixes_applied += 1

    if content != original_content:
        with open(file_path, "w", encoding="utf-8") as f:
            f.write(content)

    return fixes_applied


def main():
    print("=== Fixing Broken Documentation Links ===\n")

    # Phase 1: Root files
    print("[1/3] Fixing root files...")
    root_files = [Path("CHANGELOG.md"), Path("CONTRIBUTING.md")]
    for file_path in root_files:
        if file_path.exists():
            fixes = fix_links_in_file(file_path, LINK_FIXES)
            if fixes > 0:
                print(f"  ✓ {file_path}: {fixes} links fixed")

    # Phase 2: Examples
    print("\n[2/3] Fixing example READMEs...")
    for readme in Path("examples").rglob("README.md"):
        fixes = fix_links_in_file(readme, LINK_FIXES)
        if fixes > 0:
            print(f"  ✓ {readme}: {fixes} links fixed")

    # Phase 3: Docs directory
    print("\n[3/3] Fixing docs/ links...")
    for md_file in Path("docs").rglob("*.md"):
        fixes = fix_links_in_file(md_file, LINK_FIXES)
        if fixes > 0:
            print(f"  ✓ {md_file}: {fixes} links fixed")

    print("\n✅ Link fixes complete!")
    print("\nNext steps:")
    print("  1. Review changes: git diff")
    print("  2. Validate: ./scripts/validate-docs.sh links")
    print("  3. Commit: git commit -m 'docs: Fix broken links post-rename [WP-033]'")


if __name__ == "__main__":
    main()
