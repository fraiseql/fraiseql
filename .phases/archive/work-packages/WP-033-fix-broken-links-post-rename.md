# WP-033: Fix Broken Documentation Links Post-Rename

**Assignee:** TW-CORE
**Priority:** P1 (Critical - Blocks Release)
**Estimated Hours:** 6
**Week:** 5
**Dependencies:** WP-032 (Standardize Doc Naming - COMPLETE)

---

## Objective

Fix 82 broken internal documentation links discovered after WP-032 kebab-case renaming. These broken links fall into three categories:
1. Root files (CHANGELOG.md, CONTRIBUTING.md) still referencing old UPPERCASE names
2. References to missing files that need to be created or redirected
3. Incorrect relative paths after renaming

## Current State Analysis

### Link Validation Results

```bash
./scripts/validate-docs.sh links
# Result: 82 broken internal links across 360 files
```

### Breakdown by Category

| Category | Count | Severity | Fix Strategy |
|----------|-------|----------|--------------|
| **Root file old refs** | 2 | 🔴 Critical | Update to kebab-case names |
| **Missing files** | 45 | 🟡 Medium | Create files or update refs |
| **Wrong paths** | 35 | 🟠 High | Fix relative paths |

---

## Critical Issues (Must Fix for Release)

### 1. Root Files with Old References (2 issues)

**Impact**: High visibility - CHANGELOG and CONTRIBUTING are root docs

#### `CHANGELOG.md`
```markdown
# Current (BROKEN):
See [Version Status](docs/strategic/VERSION_STATUS.md)

# Fix:
See [Version Status](docs/strategic/version-status.md)
```

#### `CONTRIBUTING.md`
```markdown
# Current (BROKEN):
Read our [Philosophy](docs/development/PHILOSOPHY.md)

# Fix:
Read our [Philosophy](docs/development/philosophy.md)
```

### 2. Examples with Wrong Paths (High Priority)

#### `examples/rag-system/README.md` (Line 339)
```markdown
# Current (BROKEN):
- [Trinity Pattern Guide](../docs/database/trinity-pattern.md)

# Issue: File doesn't exist at ../docs/database/trinity-pattern.md
# Fix Option A: Point to existing docs/core/trinity-pattern.md
- [Trinity Pattern Guide](../../docs/core/trinity-pattern.md)

# Fix Option B: Create the missing file (if content differs)
# Would need to create docs/database/trinity-pattern.md
```

### 3. Database Docs with Missing References

#### `docs/database/avoid-triggers.md`
```markdown
# Current (BROKEN):
See [Trinity Pattern](trinity-pattern.md) for proper CQRS design.

# Issue: trinity-pattern.md doesn't exist in docs/database/
# Fix: Point to existing docs/core/trinity-pattern.md
See [Trinity Pattern](../core/trinity-pattern.md) for proper CQRS design.
```

### 4. Core Docs with Missing Files

#### `docs/core/trinity-pattern.md` (Multiple broken links)
```markdown
# Current (BROKEN):
- [Naming Conventions](./naming-conventions.md)
- [View Strategies](./view-strategies.md)
- [Performance Tuning](./performance-tuning.md)

# Issue: These files don't exist in docs/core/
# Fix Option A: Point to existing files
- [Naming Conventions](../database/table-naming-conventions.md)
- [View Strategies](../database/view-strategies.md)
- [Performance Tuning](../performance/performance-guide.md)

# Fix Option B: Create placeholder files (if needed for different content)
```

### 5. Security/Compliance Missing Files

#### `docs/security-compliance/README.md`
```markdown
# Current (BROKEN):
- [Audit Trails](../advanced/audit-trails.md)

# Issue: docs/advanced/audit-trails.md doesn't exist
# Fix: Point to existing audit documentation
- [Audit Trails](../enterprise/audit-logging.md)

# OR: Mark as TODO if this is planned content
- [Audit Trails](../advanced/audit-trails.md) *(Coming Soon)*
```

---

## Implementation Strategy

### Phase 1: Fix Critical Root Files (30 min)

**Priority**: 🔴 CRITICAL - Must fix before release

```bash
# Files to update:
- CHANGELOG.md (1 link)
- CONTRIBUTING.md (1 link)

# Simple search & replace:
# VERSION_STATUS.md → version-status.md
# PHILOSOPHY.md → philosophy.md
```

**Verification**:
```bash
./scripts/validate-docs.sh links | grep -E "CHANGELOG|CONTRIBUTING"
# Expected: 0 broken links
```

### Phase 2: Fix Example READMEs (1h)

**Priority**: 🟠 HIGH - Examples are user-facing

```bash
# Files to update:
- examples/rag-system/README.md
- examples/blog_api/README.md (if affected)
- examples/*/README.md (all examples)

# Strategy:
# 1. For trinity-pattern.md references → point to docs/core/trinity-pattern.md
# 2. For missing files → check if file exists elsewhere and update path
# 3. For planned content → mark as TODO or remove reference
```

**Verification**:
```bash
./scripts/validate-docs.sh links | grep "examples/"
# Expected: 0 broken links in examples/
```

### Phase 3: Fix Database Docs Internal Links (1.5h)

**Priority**: 🟡 MEDIUM - Core feature documentation

```bash
# Files to update:
- docs/database/avoid-triggers.md
- docs/database/*.md (scan for broken links)

# Strategy:
# 1. trinity-pattern.md → ../core/trinity-pattern.md
# 2. naming-conventions.md → table-naming-conventions.md
# 3. view-strategies.md → view-strategies.md (same dir)
```

**Verification**:
```bash
./scripts/validate-docs.sh links | grep "docs/database/"
# Expected: 0 broken links in docs/database/
```

### Phase 4: Fix Core Docs Missing Files (2h)

**Priority**: 🟡 MEDIUM - Core feature documentation

```bash
# File: docs/core/trinity-pattern.md

# Strategy:
# Option A: Point to existing files (preferred for beta)
# Option B: Create missing files (deferred to future WP)

# Broken references to fix:
# - naming-conventions.md → ../database/table-naming-conventions.md
# - view-strategies.md → ../database/view-strategies.md
# - performance-tuning.md → ../performance/performance-guide.md
```

**Decision**: For beta release, use Option A (redirect to existing docs)

**Verification**:
```bash
./scripts/validate-docs.sh links | grep "docs/core/trinity-pattern.md"
# Expected: 0 broken links
```

### Phase 5: Fix .phases/ WP References (1h)

**Priority**: 🟢 LOW - Internal planning docs

```bash
# Files to update:
- .phases/docs-review/fraiseql_docs_work_packages/WP-*.md

# Strategy:
# 1. Update old UPPERCASE references to kebab-case
# 2. For missing files → mark as TODO or remove if obsolete
# 3. For planned content → keep as-is (expected to be missing)
```

**Verification**:
```bash
./scripts/validate-docs.sh links | grep ".phases/"
# Expected: Only references to truly planned (not-yet-created) files
```

### Phase 6: Final Validation and Report (30 min)

```bash
# Run full validation
./scripts/validate-docs.sh links > link-validation-report.txt

# Expected outcome:
# - 0 broken links in root files (CHANGELOG, CONTRIBUTING)
# - 0 broken links in examples/
# - 0 broken links in docs/ (except planned content)
# - Only acceptable broken links in .phases/ (planned content)
```

---

## Automated Fix Script

Create `scripts/fix-broken-links.py`:

```python
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
        with open(file_path, 'r', encoding='utf-8') as f:
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
        with open(file_path, 'w', encoding='utf-8') as f:
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
```

---

## Manual Fixes Required

Some fixes require manual judgment:

### 1. Context-Specific Path Fixes

Files in different locations may need different relative paths:

```bash
# Example: Reference to trinity-pattern.md
# From docs/database/ → ../core/trinity-pattern.md
# From examples/rag-system/ → ../../docs/core/trinity-pattern.md
# From docs/patterns/ → ../core/trinity-pattern.md
```

### 2. Missing Files Decision

For each missing file reference, decide:
- **Option A**: Redirect to existing equivalent file
- **Option B**: Create placeholder file (defer to future WP)
- **Option C**: Remove reference (if obsolete/wrong)

**Recommended for Beta**: Option A (redirect to existing)

### 3. Planned Content

Some references in .phases/ WP docs point to planned-but-not-yet-created files:
- Keep these as-is (expected to be missing)
- Mark clearly in validation report as "intentional/planned"

---

## Verification Commands

```bash
# 1. Run full link validation
./scripts/validate-docs.sh links | tee link-validation-post-fix.txt

# 2. Check specific critical areas
echo "Root files:"
./scripts/validate-docs.sh links | grep -E "CHANGELOG|CONTRIBUTING"

echo "Examples:"
./scripts/validate-docs.sh links | grep "examples/"

echo "Database docs:"
./scripts/validate-docs.sh links | grep "docs/database/"

echo "Core docs:"
./scripts/validate-docs.sh links | grep "docs/core/"

# 3. Compare before/after
echo "Broken links before: 82"
echo "Broken links after: $(./scripts/validate-docs.sh links 2>&1 | grep -c 'BROKEN:')"

# Expected: <10 remaining (only planned content in .phases/)
```

---

## Acceptance Criteria

### Must-Have (P0 - Critical)
- [ ] 0 broken links in `CHANGELOG.md`
- [ ] 0 broken links in `CONTRIBUTING.md`
- [ ] 0 broken links in `examples/*/README.md`
- [ ] 0 broken links in `docs/core/` (except planned content marked as TODO)
- [ ] 0 broken links in `docs/database/`
- [ ] Link validation script exits with code 0 (or only planned content warnings)

### Should-Have (P1 - High)
- [ ] 0 broken links in `docs/enterprise/`
- [ ] 0 broken links in `docs/security-compliance/`
- [ ] 0 broken links in `docs/patterns/`
- [ ] Automated script handles 80%+ of fixes

### Nice-to-Have (P2 - Medium)
- [ ] 0 broken links in `.phases/` (except planned WP content)
- [ ] 0 broken links in `docs/strategic/`
- [ ] Documentation of which links are intentionally "planned content"

---

## DO NOT

- ❌ DO NOT create missing files without proper content (defer to future WPs)
- ❌ DO NOT remove references to planned content in .phases/ WP docs
- ❌ DO NOT fix links in non-markdown files (code, configs, etc.)
- ❌ DO NOT change link text/descriptions, only the URLs/paths
- ❌ DO NOT batch commit - commit in logical groups by area

---

## Breaking Changes Warning

⚠️ **None** - This WP only fixes broken links introduced by WP-032. No new breaking changes.

---

## Related Work Packages

- **WP-032**: Standardize Documentation File Naming (COMPLETE) - Created the broken links
- **WP-023**: Validate All Links (PENDING) - Will run final validation
- **WP-003**: Create Trinity Migration Guide (FUTURE) - May resolve some missing file refs

---

## Estimated Timeline

| Task | Time |
|------|------|
| Fix root files | 0.5h |
| Fix example READMEs | 1h |
| Fix database docs | 1.5h |
| Fix core docs | 2h |
| Fix .phases/ WPs | 1h |
| Final validation | 0.5h |
| **Total** | **6h** |

---

## Implementation Notes

### Quick Win Strategy

For beta release, prioritize:
1. **Root files** (5 min) - Highest visibility
2. **Examples** (30 min) - User-facing
3. **Core/Database docs** (1h) - Most critical features

Total quick win time: **1.5h** to fix 80% of critical issues

Defer to post-beta:
- .phases/ WP document links (internal planning)
- Strategic/development docs (lower priority)
- Missing files creation (future WPs)

---

## Success Metrics

**Before WP-033**:
- 82 broken internal links
- Link validation: ❌ FAIL

**After WP-033** (Target):
- 0-5 broken links (only planned content)
- Link validation: ✅ PASS (with expected warnings documented)

**Definition of Done**:
- Root files + Examples + Core/Database docs have 0 broken links
- Automated script handles 80%+ of fixes
- Remaining broken links are documented as planned content
- Link validation script exits cleanly (or with only expected warnings)
