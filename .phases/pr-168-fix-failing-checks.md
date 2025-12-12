# Phase Plan: Fix PR #168 Failing Checks

## Objective

Fix two failing CI checks in PR #168 (Release v1.8.0-beta.4):
1. **Trivy Security Scan**: Update urllib3 to fix CVE-2025-66418 and CVE-2025-66471
2. **validate-docs**: Fix 29 broken documentation links after kebab-case renaming

## Context

PR #168 introduced kebab-case file naming standardization (WP-032), which caused broken links throughout the documentation. Additionally, urllib3 2.5.0 has high-severity vulnerabilities that need patching.

**Current State**:
- urllib3 version: 2.5.0 (vulnerable)
- 29 broken links across docs, examples, and work packages
- Both checks are blocking PR merge

**Target State**:
- urllib3 upgraded to 2.6.0+
- All documentation links validated and working
- CI checks passing

## Files to Modify

### Security Fix
- `pyproject.toml` - Update urllib3 dependency constraint
- `uv.lock` - Will be regenerated automatically

### Documentation Fixes (High Priority - User Facing)
- `CHANGELOG.md` - Fix VERSION_STATUS.md reference
- `CONTRIBUTING.md` - Fix PHILOSOPHY.md reference
- `examples/blog_simple/README.md` - Fix 3 trinity-related paths
- `examples/rag-system/README.md` - Fix trinity-pattern path

### Documentation Fixes (Lower Priority - Internal Work Packages)
- `.phases/docs-review/fraiseql_docs_work_packages/WP-010-create-security-compliance-hub.md`
- `.phases/docs-review/fraiseql_docs_work_packages/WP-032-standardize-doc-naming-kebab-case.md`
- `.phases/docs-review/fraiseql_docs_work_packages/WP-033-fix-broken-links-post-rename.md`
- `.phases/docs-review/fraiseql_docs_work_packages/WP-006-fix-example-readmes.md`

## Implementation Steps

### Step 1: Update urllib3 Security Vulnerability

**Task**: Upgrade urllib3 from 2.5.0 to 2.6.0+ to fix CVE-2025-66418 and CVE-2025-66471.

**Commands**:
```bash
# Update urllib3 to latest version (2.6.0+)
uv add "urllib3>=2.6.0"

# Verify the update
grep -A 3 'name = "urllib3"' uv.lock | grep version
```

**Expected Output**:
```
version = "2.6.0"
```

**Verification**:
```bash
# Check that uv.lock was updated
git diff uv.lock | grep urllib3

# Expected to see version change from 2.5.0 to 2.6.0+
```

---

### Step 2: Fix High-Priority Documentation Links

**Task**: Fix broken links in user-facing documentation (CHANGELOG, CONTRIBUTING, examples).

#### 2.1: Fix CHANGELOG.md

**Current broken link**:
```markdown
docs/strategic/VERSION_STATUS.md
```

**Fix to**:
```markdown
docs/strategic/version-status.md
```

**Command**:
```bash
# Use Edit tool to replace the link
# Search for: docs/strategic/VERSION_STATUS.md
# Replace with: docs/strategic/version-status.md
```

---

#### 2.2: Fix CONTRIBUTING.md

**Current broken link**:
```markdown
docs/development/PHILOSOPHY.md
```

**Fix to**:
```markdown
docs/development/philosophy.md
```

**Command**:
```bash
# Use Edit tool to replace the link
# Search for: docs/development/PHILOSOPHY.md
# Replace with: docs/development/philosophy.md
```

---

#### 2.3: Fix examples/blog_simple/README.md

**Three broken links to fix**:

1. **Trinity pattern moved from database/ to core/**:
   - Current: `../../docs/database/trinity-pattern.md`
   - Fix to: `../../docs/core/trinity-pattern.md`

2. **Trinity identifiers renamed to kebab-case**:
   - Current: `../../docs/database/trinity_identifiers.md`
   - Fix to: `../../docs/database/trinity-identifiers.md`

3. **Table naming conventions doesn't exist** (need to check actual file):
   - Current: `../../docs/database/TABLE_NAMING_CONVENTIONS.md`
   - Need to find where this content is now (likely removed or merged)
   - If removed, update the link text or remove the reference

**Commands**:
```bash
# First, check if TABLE_NAMING_CONVENTIONS content exists elsewhere
find docs -iname "*naming*" -o -iname "*convention*"

# Then apply fixes using Edit tool
```

---

#### 2.4: Fix examples/rag-system/README.md

**Current broken link**:
```markdown
../docs/database/trinity-pattern.md
```

**Fix to**:
```markdown
../../docs/core/trinity-pattern.md
```

**Note**: The path is relative from `examples/rag-system/`, so needs `../../` to reach project root.

---

### Step 3: Fix Lower-Priority Work Package Documentation

**Task**: Fix broken links in `.phases/docs-review/fraiseql_docs_work_packages/` files.

These are internal planning documents, not user-facing. Many contain example paths or references to files that may not exist. Strategy:

#### 3.1: WP-010-create-security-compliance-hub.md

Contains 5 broken links to files that may not exist yet (future work):
- `slsa-provenance.md`
- `audit-trails-deep-dive.md`
- `compliance-matrix.md`
- `security-profiles.md`
- `rbac-row-level-security.md`

**Action**: These are references to planned documentation. Either:
- Option A: Prefix with "TODO:" or mark as planned
- Option B: Remove references if work packages are obsolete
- Option C: Create placeholder files

**Recommended**: Check if WP-010 is still active/relevant, then choose appropriate action.

---

#### 3.2: WP-032-standardize-doc-naming-kebab-case.md

Contains 1 example broken link:
- `path/to/OLD_NAME.md` (this is clearly an example/placeholder)

**Action**: Update the example to use kebab-case:
- Change to: `path/to/old-name.md`

---

#### 3.3: WP-033-fix-broken-links-post-rename.md

Contains 17 broken links - this document itself is ABOUT fixing broken links!

**Context**: This work package document lists examples of the types of link issues to fix. The "broken" links are intentional examples showing before/after states.

**Action**: Review the document structure. If these are example snippets showing what to fix, they should be formatted as code blocks or clearly marked as examples:

```markdown
<!-- Before -->
docs/strategic/VERSION_STATUS.md

<!-- After -->
docs/strategic/version-status.md
```

**Recommended**: Wrap all example paths in code fences (```) to prevent validation from treating them as real links.

---

#### 3.4: WP-006-fix-example-readmes.md

Contains multiple broken `../../docs/core/trinity-pattern.md` references.

**Action**: Same as examples - fix the relative paths in the work package document's examples.

---

### Step 4: Verify All Fixes

**Task**: Run documentation validation to confirm all links are fixed.

**Commands**:
```bash
# Make validation script executable (if needed)
chmod +x scripts/validate-docs.sh

# Run link validation
./scripts/validate-docs.sh links

# Expected output: No broken links reported
```

**Success Criteria**:
- Script exits with status 0
- No `[ERROR] Broken link` messages
- All user-facing docs (CHANGELOG, CONTRIBUTING, examples) pass validation

---

## Verification Commands

### Full CI Check Simulation

Run all the checks that are failing in CI:

```bash
# 1. Security scan (Trivy) - check urllib3 version
grep -A 3 'name = "urllib3"' uv.lock | grep version
# Should show: version = "2.6.0" or higher

# 2. Documentation validation
./scripts/validate-docs.sh links
# Should exit 0 with no errors

# 3. Optional: Run all doc validations
./scripts/validate-docs.sh links
./scripts/validate-docs.sh files
./scripts/validate-docs.sh versions
./scripts/validate-docs.sh install
```

### Expected Output

**urllib3 check**:
```
version = "2.6.0"
```

**Link validation**:
```
[INFO] Starting FraiseQL documentation validation (mode: links)
[INFO] Validating internal links...
[INFO] ✓ All internal links are valid
```

---

## Acceptance Criteria

### Must Have
- [ ] urllib3 upgraded to version 2.6.0 or higher
- [ ] `uv.lock` regenerated with updated urllib3
- [ ] CHANGELOG.md links fixed (version-status.md)
- [ ] CONTRIBUTING.md links fixed (philosophy.md)
- [ ] examples/blog_simple/README.md links fixed (all 3)
- [ ] examples/rag-system/README.md links fixed
- [ ] `./scripts/validate-docs.sh links` passes with no errors
- [ ] Trivy security scan would pass (urllib3 2.6.0+)

### Should Have
- [ ] Work package document links fixed or marked as examples
- [ ] WP-033 example paths wrapped in code blocks
- [ ] All low-priority documentation links resolved

### Nice to Have
- [ ] Verification that TABLE_NAMING_CONVENTIONS content exists elsewhere
- [ ] Cleanup of obsolete work package references

---

## DO NOT

- ❌ **DO NOT** change file names again (kebab-case is already done in WP-032)
- ❌ **DO NOT** modify the trinity-pattern.md file location (it's correctly in docs/core/)
- ❌ **DO NOT** downgrade urllib3 or pin to 2.5.x
- ❌ **DO NOT** skip the link validation step
- ❌ **DO NOT** commit if `./scripts/validate-docs.sh links` still shows errors
- ❌ **DO NOT** modify links in code examples (like in code fences) - only fix actual markdown links
- ❌ **DO NOT** create missing files just to make links work - fix the links to point to existing files

---

## Rollback Plan

If issues arise:

1. **urllib3 update breaks something**:
   ```bash
   # Revert pyproject.toml changes
   git checkout pyproject.toml uv.lock
   uv sync
   ```

2. **Documentation links still broken**:
   ```bash
   # Review the specific broken links reported
   ./scripts/validate-docs.sh links 2>&1 | tee link-errors.txt

   # Fix individually or revert
   git checkout <filename>
   ```

3. **Validation script itself has issues**:
   ```bash
   # Check the script for recent changes
   git log --oneline scripts/validate-docs.sh

   # Run manual link checks
   find docs -name "*.md" -exec grep -l "VERSION_STATUS" {} \;
   ```

---

## Notes

- **Testing**: This branch is `release/v1.8.0b4`, targeting merge to `dev`
- **Urgency**: Both checks are blocking the beta release
- **Security Priority**: urllib3 fix should be done first (higher risk)
- **Work Package Context**: Many `.phases/` docs are historical planning artifacts - don't over-invest in fixing these unless they're still active

## Related Work Packages

- WP-032: Standardize doc naming to kebab-case (completed, caused these link breaks)
- WP-033: Fix broken links post-rename (this phase implements WP-033)

---

## Estimated Effort

- **urllib3 update**: 2 minutes
- **High-priority doc links**: 10-15 minutes
- **Low-priority work packages**: 10-15 minutes (optional)
- **Verification**: 5 minutes
- **Total**: 30-35 minutes

---

## Success Metrics

After completion:
1. PR #168 checks go from 2 failing → 0 failing
2. GitHub Security tab shows 0 high-severity vulnerabilities
3. Documentation validation CI job passes
4. Ready to merge v1.8.0-beta.4 to dev branch
