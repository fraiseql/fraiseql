# WP-032: Standardize Documentation File Naming to Kebab-Case

**Assignee:** TW-CORE
**Priority:** P2 (Nice to Have - Technical Debt)
**Estimated Hours:** 10
**Week:** 5
**Dependencies:** WP-023 (Validate All Links - should run after to verify)

---

## Objective

Standardize ALL documentation file naming to use **kebab-case** (lowercase with dashes), which is GitHub best practice and improves:
- URL readability (`/docs/table-naming-conventions` vs `/docs/TABLE_NAMING_CONVENTIONS`)
- SEO and discoverability
- Consistency across the repository
- Cross-platform compatibility (case-sensitive filesystems)

## Current State Analysis

### Naming Pattern Breakdown

| Pattern | Count | Examples |
|---------|-------|----------|
| **UPPERCASE** | 48 files | `table-naming-conventions.md`, `rbac-postgresql-refactored.md` |
| **snake_case** | 36 files | `loki-integration.md`, `mutation-pipeline.md`, `trinity-identifiers.md` |
| **kebab-case** ✅ | 103 files | `getting-started.md`, `connection-pooling.md` |
| **lowercase-simple** | 24 files | `README.md` (keep as-is) |

**Total files needing rename:** ~84 files (48 UPPERCASE + 36 snake_case)

### GitHub Best Practice

According to GitHub documentation and community standards:
- ✅ **kebab-case** (`my-document-name.md`) - Recommended
  - URL-friendly without encoding
  - Easier to read than snake_case in URLs
  - Case-insensitive filesystem safe
  - SEO-friendly
- ⚠️ **snake_case** (`my_document_name.md`) - Acceptable but not preferred
- ❌ **UPPERCASE** (`MY_DOCUMENT_NAME.md`) - Poor practice
  - Screams "old convention"
  - URL unfriendly
  - Accessibility issues

### Special Cases (Keep As-Is)

- `README.md` - GitHub convention
- `CHANGELOG.md` - Conventional uppercase
- `CONTRIBUTING.md` - Conventional uppercase
- `LICENSE` - Conventional uppercase

---

## Files to Rename

### Priority 1: UPPERCASE Files (48 files)

#### Database Documentation
```bash
docs/database/database-level-caching.md       → database-level-caching.md
docs/database/table-naming-conventions.md     → table-naming-conventions.md
docs/database/view-strategies.md              → view-strategies.md
```

#### Enterprise Documentation
```bash
docs/enterprise/rbac-postgresql-refactored.md → rbac-postgresql-refactored.md
docs/enterprise/rbac-postgresql-assessment.md → rbac-postgresql-assessment.md
docs/enterprise/enterprise.md                  → enterprise.md
```

#### Performance Documentation
```bash
docs/performance/apq-assessment.md            → apq-assessment.md
docs/performance/performance-guide.md         → performance-guide.md
```

#### Architecture Documentation
```bash
docs/architecture/direct-path-implementation.md → direct-path-implementation.md
```

#### Testing Documentation
```bash
docs/testing/enabling-external-tests.md       → enabling-external-tests.md
docs/testing/skipped-tests.md                 → skipped-tests.md
```

#### Development Documentation
```bash
docs/development/philosophy.md                     → philosophy.md
docs/development/framework-submission-guide.md     → framework-submission-guide.md
docs/development/new-user-confusions.md            → new-user-confusions.md
```

#### Strategic Documentation
```bash
docs/strategic/tier-1-implementation-plans.md                        → tier-1-implementation-plans.md
docs/strategic/improvement-analysis-prompt.md                        → improvement-analysis-prompt.md
docs/strategic/v1-advanced-patterns.md                               → v1-advanced-patterns.md
docs/strategic/version-status.md                                     → version-status.md
docs/strategic/v1-vision.md                                          → v1-vision.md
docs/strategic/audiences.md                                          → audiences.md
docs/strategic/project-structure.md                                 → project-structure.md
docs/strategic/enterprise-roadmap.md                                 → enterprise-roadmap.md
docs/strategic/fraiseql-industrial-readiness-assessment-2025-10-20.md → fraiseql-industrial-readiness-assessment-2025-10-20.md
```

#### Rust Documentation
```bash
docs/rust/rust-pipeline-implementation-guide.md → rust-pipeline-implementation-guide.md
docs/rust/rust-field-projection.md              → rust-field-projection.md
docs/rust/rust-first-pipeline.md                → rust-first-pipeline.md
```

#### Compliance Documentation
```bash
docs/compliance/global-regulations.md         → global-regulations.md
```

#### Tutorials Documentation
```bash
docs/tutorials/interactive-examples.md        → interactive-examples.md
```

### Priority 2: snake_case Files (36 files)

#### Production Documentation
```bash
docs/production/loki-integration.md           → loki-integration.md
```

#### Architecture Documentation
```bash
docs/architecture/mutation-pipeline.md                       → mutation-pipeline.md
docs/architecture/decisions/002-ultra-direct-mutation-path.md → 002-ultra-direct-mutation-path.md
docs/architecture/decisions/003-unified-audit-table.md        → 003-unified-audit-table.md
docs/architecture/decisions/005-simplified-single-source-cdc.md → 005-simplified-single-source-cdc.md
```

#### Mutations Documentation
```bash
docs/mutations/migration-guide.md            → migration-guide.md
docs/mutations/cascade-architecture.md       → cascade-architecture.md
```

#### Performance Documentation
```bash
docs/performance/coordinate-performance-guide.md → coordinate-performance-guide.md
```

#### Database Documentation
```bash
docs/database/trinity-identifiers.md         → trinity-identifiers.md
```

**Note:** Full list of 36 snake_case files to be generated by automated script during implementation.

---

## Implementation Strategy

### Phase 1: Automated Rename Script (2h)

Create `scripts/rename-docs-kebab-case.py`:

```python
#!/usr/bin/env python3
"""
Rename documentation files to kebab-case and update all references.
"""
import os
import re
from pathlib import Path
from typing import Dict, List, Tuple

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

def find_references(file_path: Path, old_name: str, new_name: str) -> List[str]:
    """Find all references to old filename in a file."""
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()

    # Match markdown links: [text](path/to/old-name.md)
    # Match relative paths: ./OLD_NAME.md, ../OLD_NAME.md
    patterns = [
        rf'\]\([^)]*{re.escape(old_name)}\)',
        rf'`[^`]*{re.escape(old_name)}`',
        rf'\b{re.escape(old_name)}\b',
    ]

    matches = []
    for pattern in patterns:
        matches.extend(re.findall(pattern, content))

    return matches

def update_references(file_path: Path, old_name: str, new_name: str) -> bool:
    """Update all references to old filename in a file."""
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()

    original_content = content

    # Replace in markdown links
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

    print(f"Found {len(renames)} files to rename:\n")
    for old_path, new_path in sorted(renames.items()):
        print(f"  {old_path.relative_to(docs_dir)} → {new_path.name}")

    print("\n" + "="*60)

    # Step 1: Find all references before renaming
    print("\n[1/3] Scanning for references...")
    all_md_files = list(docs_dir.rglob("*.md"))
    reference_map: Dict[str, List[Path]] = {}

    for old_path, new_path in renames.items():
        old_name = old_path.name
        new_name = new_path.name

        files_with_refs = []
        for md_file in all_md_files:
            refs = find_references(md_file, old_name, new_name)
            if refs:
                files_with_refs.append(md_file)

        if files_with_refs:
            reference_map[old_name] = files_with_refs
            print(f"  {old_name}: {len(files_with_refs)} references")

    # Step 2: Update all references
    print("\n[2/3] Updating references...")
    for old_path, new_path in renames.items():
        old_name = old_path.name
        new_name = new_path.name

        updated_count = 0
        for md_file in all_md_files:
            if update_references(md_file, old_name, new_name):
                updated_count += 1

        if updated_count > 0:
            print(f"  Updated {updated_count} files for {old_name} → {new_name}")

    # Step 3: Rename files (use git mv for proper tracking)
    print("\n[3/3] Renaming files with git mv...")
    for old_path, new_path in sorted(renames.items()):
        os.system(f'git mv "{old_path}" "{new_path}"')
        print(f"  Renamed: {old_path.name} → {new_path.name}")

    print(f"\n✅ Complete! Renamed {len(renames)} files to kebab-case.")
    print("\nNext steps:")
    print("  1. Review changes: git diff --staged")
    print("  2. Run link validation: ./scripts/validate-docs.sh links")
    print("  3. Commit: git commit -m 'docs: Standardize file naming to kebab-case'")

if __name__ == "__main__":
    main()
```

### Phase 2: Execution (6h)

1. **Dry Run** (30 min)
   ```bash
   python scripts/rename-docs-kebab-case.py --dry-run
   # Review output for any issues
   ```

2. **Execute Renames** (1h)
   ```bash
   python scripts/rename-docs-kebab-case.py
   git status  # Verify git mv was used
   ```

3. **Update External References** (2h)
   - Update `.phases/` Work Package references
   - Update `examples/` README references
   - Update root `README.md` if it has doc links
   - Update any CI/CD scripts that reference specific doc files

4. **Validation** (1h)
   ```bash
   # Run link validation
   ./scripts/validate-docs.sh links

   # Search for any remaining old references
   grep -r "TABLE_NAMING_CONVENTIONS" docs/
   grep -r "trinity_identifiers" docs/
   grep -r "loki_integration" docs/
   ```

5. **Manual Review** (1h30)
   - Check 10-15 high-traffic docs manually
   - Verify all internal links work
   - Verify all cross-references updated
   - Check that old references don't exist

### Phase 3: Documentation Update (2h)

Update `docs/development/CONTRIBUTING.md` (or create `docs/development/contributing.md`) with file naming convention:

```markdown
## Documentation File Naming Convention

All documentation files MUST use **kebab-case** naming:

✅ **Correct:**
- `getting-started.md`
- `table-naming-conventions.md`
- `rbac-postgresql-guide.md`

❌ **Incorrect:**
- `GETTING_STARTED.md` (UPPERCASE)
- `table_naming_conventions.md` (snake_case)
- `TableNamingConventions.md` (PascalCase)

**Exceptions:**
- `README.md` - GitHub convention
- `CHANGELOG.md` - Conventional uppercase
- `CONTRIBUTING.md` - Conventional uppercase
```

---

## Verification Commands

```bash
# 1. Check no UPPERCASE files remain (except conventional)
find docs -type f -name "*.md" | grep -E "[A-Z]" | \
  grep -v "README.md" | grep -v "CHANGELOG.md" | \
  grep -v "CONTRIBUTING.md"

# Expected: 0 results

# 2. Check no snake_case files remain
find docs -type f -name "*.md" | grep "_"

# Expected: 0 results

# 3. Run link validation
./scripts/validate-docs.sh links

# Expected: 0 broken links

# 4. Search for old references in WP files
grep -r "TABLE_NAMING_CONVENTIONS\|trinity_identifiers\|loki_integration" .phases/

# Expected: Only in WP-032 (this file)

# 5. Verify git history preserved
git log --follow docs/database/table-naming-conventions.md

# Expected: Shows full history from table-naming-conventions.md
```

---

## Acceptance Criteria

- [ ] ALL documentation files (except README.md, CHANGELOG.md, CONTRIBUTING.md) use kebab-case
- [ ] Zero UPPERCASE documentation files remain
- [ ] Zero snake_case documentation files remain
- [ ] All internal markdown links updated to new names
- [ ] All WP references in `.phases/` updated
- [ ] All example README references updated
- [ ] Link validation passes with 0 broken links
- [ ] Git history preserved (used `git mv`)
- [ ] Contributing guide documents the naming convention
- [ ] CI/CD passes after changes

---

## DO NOT

- ❌ DO NOT use `mv` - MUST use `git mv` to preserve history
- ❌ DO NOT rename `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`
- ❌ DO NOT rename files in `src/` or `tests/` directories (not documentation)
- ❌ DO NOT batch commit - commit in logical groups (e.g., "Rename database docs", "Rename enterprise docs")
- ❌ DO NOT forget to update cross-references in other repos that link to these docs

---

## Breaking Changes Warning

⚠️ **External Link Breakage**: This will break any external links to documentation that use the old filenames.

**Mitigation:**
1. Add redirects in GitHub Pages config (if using Pages)
2. Add old→new mapping in `docs/REDIRECTS.md` for reference
3. Update any external references (blog posts, tweets, etc.)
4. Consider this for a major version release (v2.0.0) or add 301 redirects

---

## Implementation Plan Reference

See `.phases/loki_fixes_implementation_plan.md` for examples of systematic file renaming patterns.

**Related WPs:**
- WP-001: Fix Core Docs Naming
- WP-002: Fix Database Docs Naming
- WP-023: Validate All Links (run AFTER this WP)

---

## Estimated Timeline

| Task | Time |
|------|------|
| Create rename script | 2h |
| Dry run and testing | 0.5h |
| Execute renames | 1h |
| Update external references | 2h |
| Validation | 1h |
| Manual review | 1.5h |
| Documentation update | 2h |
| **Total** | **10h** |
