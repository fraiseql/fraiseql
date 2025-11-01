# Phase 1: Fix Broken Documentation Links

**PR**: #105 - v1.1.1: CI Modernization + Documentation Consolidation
**Objective**: Update all internal documentation links to match the new consolidated structure
**Total Broken Links**: 60+
**Estimated Time**: 45-60 minutes
**Complexity**: Medium (systematic but tedious)

---

## Overview

The documentation reorganization moved many files but left broken links throughout the codebase. This phase systematically fixes all broken links by updating references to match the new structure.

### File Movement Summary

**Key Changes:**
- `docs/quickstart.md` → `docs/getting-started/quickstart.md`
- `docs/FIRST_HOUR.md` → `docs/getting-started/first-hour.md`
- `docs/INSTALLATION.md` → `docs/getting-started/installation.md`
- `docs/TROUBLESHOOTING.md` → `docs/guides/troubleshooting.md`
- `docs/TROUBLESHOOTING_DECISION_TREE.md` → `docs/guides/troubleshooting-decision-tree.md`
- `docs/UNDERSTANDING.md` → `docs/guides/understanding-fraiseql.md`
- `docs/CONTRIBUTING.md` → `CONTRIBUTING.md` (moved to root)

---

## Phase 1A: Root-Level Documentation Files

### Task 1.1: Fix `CHANGELOG.md` (8 broken links)

**Priority**: HIGH (release documentation)

**Broken Links → Correct Paths:**
- `docs/quickstart.md` → `docs/getting-started/quickstart.md`
- `docs/FIRST_HOUR.md` → `docs/getting-started/first-hour.md`
- `docs/TROUBLESHOOTING_DECISION_TREE.md` → `docs/guides/troubleshooting-decision-tree.md`
- `docs/TROUBLESHOOTING.md` → `docs/guides/troubleshooting.md`
- `docs/INSTALLATION.md` → `docs/getting-started/installation.md`
- `docs/CONTRIBUTING.md` → `CONTRIBUTING.md`

**Steps:**
1. Read `CHANGELOG.md`
2. Use Edit tool to replace each broken link with correct path
3. For links that appear multiple times, use `replace_all: true`
4. Verify all 8 links are fixed

**Validation:**
```bash
grep -n "docs/quickstart.md" CHANGELOG.md  # Should find 0 results
grep -n "docs/getting-started/quickstart.md" CHANGELOG.md  # Should find results
```

---

### Task 1.2: Fix `RELEASE_NOTES.md` (11 broken links)

**Priority**: HIGH (release documentation)

**Broken Links → Correct Paths:**
- `docs/FIRST_HOUR.md` → `docs/getting-started/first-hour.md` (appears 3x)
- `docs/quickstart.md` → `docs/getting-started/quickstart.md` (appears 3x)
- `docs/UNDERSTANDING.md` → `docs/guides/understanding-fraiseql.md` (appears 3x)
- `docs/INSTALLATION.md` → `docs/getting-started/installation.md` (1x)
- `docs/TROUBLESHOOTING.md` → `docs/guides/troubleshooting.md` (appears 2x)
- `docs/CONTRIBUTING.md` → `CONTRIBUTING.md` (1x)

**Steps:**
1. Read `RELEASE_NOTES.md`
2. Use Edit tool with `replace_all: true` for repeated links
3. Handle each unique broken link
4. Verify all 11 links are fixed

**Note**: Multiple occurrences make this file perfect for `replace_all: true` flag

---

### Task 1.3: Fix `examples/README.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../docs/quickstart.md` → `../docs/getting-started/quickstart.md`

**Steps:**
1. Read `examples/README.md`
2. Use Edit tool to fix the relative path
3. Verify fix

---

### Task 1.4: Fix `examples/INDEX.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../docs/quickstart.md` → `../docs/getting-started/quickstart.md`

**Steps:**
1. Read `examples/INDEX.md`
2. Use Edit tool to fix the relative path
3. Verify fix

---

## Phase 1B: GitHub Actions Documentation

### Task 1.5: Fix `.github/docs/README.md` (2 broken links)

**Priority**: LOW (internal docs)

**Missing Files:**
- `workflows.md` → File doesn't exist
- `trusted-publishing.md` → File doesn't exist

**Decision Required**: Either create these files or remove the links

**Steps:**
1. Read `.github/docs/README.md` to understand context
2. Check `.github/docs/` directory contents with `ls .github/docs/`
3. **Option A** - Create placeholder files:
   - Create `workflows.md` with basic workflow documentation
   - Create `trusted-publishing.md` with PyPI publishing info
4. **Option B** - Remove broken links:
   - Edit README to remove or comment out these references
5. Commit decision with explanation

**Recommendation**: Option B (remove links) - these can be added later when needed

---

## Phase 1C: Documentation Directory Files

### Task 1.6: Fix `docs/reference/cli.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../quickstart.md` → `../getting-started/quickstart.md`

**Steps:**
1. Read `docs/reference/cli.md`
2. Use Edit tool to fix relative path
3. Verify fix

---

### Task 1.7: Fix `docs/reference/quick-reference.md` (3 broken links)

**Priority**: HIGH (frequently accessed reference)

**Broken Links → Correct Paths:**
- `../FIRST_HOUR.md` → `../getting-started/first-hour.md`
- `../TROUBLESHOOTING.md` → `../guides/troubleshooting.md`
- `../UNDERSTANDING.md` → `../guides/understanding-fraiseql.md`

**Steps:**
1. Read `docs/reference/quick-reference.md`
2. Use Edit tool to fix each relative path
3. Verify all 3 links are fixed

---

### Task 1.8: Fix `docs/diagrams/README.md` (1 broken link)

**Priority**: LOW

**Broken Links → Correct Paths:**
- `../UNDERSTANDING.md` → `../guides/understanding-fraiseql.md`

**Steps:**
1. Read `docs/diagrams/README.md`
2. Use Edit tool to fix relative path
3. Verify fix

---

### Task 1.9: Fix `docs/production/README.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../TROUBLESHOOTING.md` → `../guides/troubleshooting.md`

**Steps:**
1. Read `docs/production/README.md`
2. Use Edit tool to fix relative path
3. Verify fix

---

### Task 1.10: Fix `docs/deployment/README.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../TROUBLESHOOTING.md` → `../guides/troubleshooting.md`

**Steps:**
1. Read `docs/deployment/README.md`
2. Use Edit tool to fix relative path
3. Verify fix

---

### Task 1.11: Fix `docs/strategic/VERSION_STATUS.md` (2 broken links)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../INSTALLATION.md` → `../getting-started/installation.md`
- `../quickstart.md` → `../getting-started/quickstart.md`

**Steps:**
1. Read `docs/strategic/VERSION_STATUS.md`
2. Use Edit tool to fix both paths
3. Verify both links are fixed

---

### Task 1.12: Fix `docs/core/configuration.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../INSTALLATION.md` → `../getting-started/installation.md`

**Steps:**
1. Read `docs/core/configuration.md`
2. Use Edit tool to fix path
3. Verify fix

---

### Task 1.13: Fix `docs/core/project-structure.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../quickstart.md` → `../getting-started/quickstart.md`

**Steps:**
1. Read `docs/core/project-structure.md`
2. Use Edit tool to fix path
3. Verify fix

---

### Task 1.14: Fix `docs/core/rust-pipeline-integration.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../CONTRIBUTING.md` → `../../CONTRIBUTING.md`

**Note**: CONTRIBUTING.md is now at repository root, need to go up two levels

**Steps:**
1. Read `docs/core/rust-pipeline-integration.md`
2. Use Edit tool to fix path (two levels up)
3. Verify fix

---

### Task 1.15: Fix `docs/features/index.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../quickstart.md` → `../getting-started/quickstart.md`

**Steps:**
1. Read `docs/features/index.md`
2. Use Edit tool to fix path
3. Verify fix

---

## Phase 1D: Getting Started Directory

### Task 1.16: Fix `docs/getting-started/quickstart.md` (5 broken links)

**Priority**: CRITICAL (main entry point for users)

**Broken Links → Correct Paths:**
- `UNDERSTANDING.md` → `../guides/understanding-fraiseql.md`
- `FIRST_HOUR.md` → `first-hour.md` (same directory)
- `TROUBLESHOOTING.md` → `../guides/troubleshooting.md` (appears 2x)
- `development/style-guide.md` → Check if exists, likely should be removed or updated

**Steps:**
1. Read `docs/getting-started/quickstart.md`
2. Check if `dev/development/style-guide.md` exists with `ls dev/development/`
3. Use Edit tool to fix all paths
4. Use `replace_all: true` for TROUBLESHOOTING.md (appears 2x)
5. For style-guide.md: either fix path or remove reference
6. Verify all 5 links are fixed

**Investigation needed**: Determine correct path for style-guide.md

---

### Task 1.17: Fix `docs/getting-started/installation.md` (2 broken links)

**Priority**: HIGH

**Broken Links → Correct Paths:**
- `core/concepts-glossary.md` → `../core/concepts-glossary.md`
- `core/configuration.md` → `../core/configuration.md`

**Steps:**
1. Read `docs/getting-started/installation.md`
2. Use Edit tool to fix relative paths (add `../` prefix)
3. Verify both links are fixed

---

### Task 1.18: Fix `docs/getting-started/first-hour.md` (10 broken links)

**Priority**: HIGH (key tutorial document)

**Broken Links → Correct Paths:**
- `UNDERSTANDING.md` → `../guides/understanding-fraiseql.md`
- `advanced/filter-operators.md` → `../advanced/filter-operators.md`
- `tutorials/beginner-path.md` → `../tutorials/beginner-path.md` (appears 2x)
- `tutorials/blog-api.md` → `../tutorials/blog-api.md`
- `performance/PERFORMANCE_GUIDE.md` → `../guides/performance-guide.md`
- `advanced/multi-tenancy.md` → `../advanced/multi-tenancy.md`
- `migration/v0-to-v1.md` → `../migration/v0-to-v1.md`
- `TROUBLESHOOTING.md` → `../guides/troubleshooting.md`
- `reference/quick-reference.md` → `../reference/quick-reference.md`

**Steps:**
1. Read `docs/getting-started/first-hour.md`
2. Use Edit tool to fix each path
3. Use `replace_all: true` for beginner-path.md (appears 2x)
4. Verify all 10 links are fixed

**Note**: This file has the most broken links in a single document

---

## Phase 1E: Guides Directory

### Task 1.19: Fix `docs/guides/troubleshooting-decision-tree.md` (3 broken links)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `core/ddl-organization.md` → `../core/ddl-organization.md`
- `README.md` → Check if `docs/guides/README.md` exists
- `TROUBLESHOOTING.md` → `troubleshooting.md` (same directory)

**Steps:**
1. Read `docs/guides/troubleshooting-decision-tree.md`
2. Check if `docs/guides/README.md` exists with `ls docs/guides/`
3. If README.md doesn't exist, either create it or remove the link
4. Use Edit tool to fix paths
5. Verify all 3 links are fixed

**Decision Required**: Handle missing README.md

---

### Task 1.20: Fix `docs/guides/performance-guide.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `benchmarks/` → Check if directory exists

**Steps:**
1. Read `docs/guides/performance-guide.md`
2. Check if `docs/benchmarks/` or `benchmarks/` directory exists
3. **Option A**: Fix path if directory exists
4. **Option B**: Remove link if directory doesn't exist
5. Verify fix

**Investigation needed**: Locate benchmarks directory or remove reference

---

### Task 1.21: Fix `docs/guides/troubleshooting.md` (5 broken links)

**Priority**: HIGH (frequently accessed)

**Broken Links → Correct Paths:**
- `TROUBLESHOOTING_DECISION_TREE.md` → `troubleshooting-decision-tree.md`
- `development/style-guide.md` → Investigate correct path
- `FIRST_HOUR.md` → `../getting-started/first-hour.md`
- `reference/quick-reference.md` → `../reference/quick-reference.md`
- `tutorials/beginner-path.md` → `../tutorials/beginner-path.md`

**Steps:**
1. Read `docs/guides/troubleshooting.md`
2. Check if `dev/development/style-guide.md` exists
3. Use Edit tool to fix all paths
4. For style-guide.md: fix path or remove if doesn't exist
5. Verify all 5 links are fixed

---

### Task 1.22: Fix `docs/guides/understanding-fraiseql.md` (5 broken links)

**Priority**: HIGH (core concept document)

**Broken Links → Correct Paths:**
- `quickstart.md` → `../getting-started/quickstart.md` (appears 2x)
- `FIRST_HOUR.md` → `../getting-started/first-hour.md`
- `core/concepts-glossary.md` → `../core/concepts-glossary.md`
- `reference/quick-reference.md` → `../reference/quick-reference.md`

**Steps:**
1. Read `docs/guides/understanding-fraiseql.md`
2. Use Edit tool with `replace_all: true` for quickstart.md (appears 2x)
3. Fix other unique links
4. Verify all 5 links are fixed

---

### Task 1.23: Fix `docs/guides/nested-array-filtering.md` (3 broken links)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../tests/test_end_to_end_nested_array_where.py` → `../../tests/test_end_to_end_nested_array_where.py`
- `../tests/test_nested_array_logical_operators.py` → `../../tests/test_nested_array_logical_operators.py`
- `../src/fraiseql/core/graphql_type.py` → `../../src/fraiseql/core/graphql_type.py`

**Note**: Paths need one more `../` to escape docs/ directory

**Steps:**
1. Read `docs/guides/nested-array-filtering.md`
2. Use Edit tool to fix relative paths (add one more level)
3. Verify all 3 links are fixed

---

## Phase 1F: Tutorials Directory

### Task 1.24: Fix `docs/tutorials/beginner-path.md` (1 broken link)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../quickstart.md` → `../getting-started/quickstart.md`

**Steps:**
1. Read `docs/tutorials/beginner-path.md`
2. Use Edit tool to fix path
3. Verify fix

---

### Task 1.25: Fix `docs/tutorials/blog-api.md` (2 broken links)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../quickstart.md` → `../getting-started/quickstart.md` (appears 2x)

**Steps:**
1. Read `docs/tutorials/blog-api.md`
2. Use Edit tool with `replace_all: true`
3. Verify both links are fixed

---

### Task 1.26: Fix `docs/tutorials/INTERACTIVE_EXAMPLES.md` (2 broken links)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `../quickstart.md` → `../getting-started/quickstart.md`
- `../UNDERSTANDING.md` → `../guides/understanding-fraiseql.md`

**Steps:**
1. Read `docs/tutorials/INTERACTIVE_EXAMPLES.md`
2. Use Edit tool to fix both paths
3. Verify both links are fixed

---

## Phase 1G: Dev Directory

### Task 1.27: Fix `dev/rust/api.md` (2 broken links)

**Priority**: LOW (internal dev docs)

**Broken Links → Correct Paths:**
- `../CHANGELOG.md` → `../../CHANGELOG.md`
- `README.md#contributing` → Check if section exists in `dev/rust/README.md`

**Steps:**
1. Read `dev/rust/api.md`
2. Check if `dev/rust/README.md` exists and has contributing section
3. Use Edit tool to fix CHANGELOG path
4. For README link: fix if exists, remove if not
5. Verify fixes

---

### Task 1.28: Fix `dev/README.md` (1 broken link)

**Priority**: LOW

**Broken Links → Correct Paths:**
- `releases/release-process.md` → File doesn't exist

**Decision Required**: Create file or remove link

**Steps:**
1. Read `dev/README.md` to understand context
2. **Option A**: Create placeholder `dev/releases/release-process.md`
3. **Option B**: Remove or comment out the link
4. Commit decision with explanation

**Recommendation**: Option B - remove link, can be added when process is documented

---

### Task 1.29: Fix `dev/audits/version-status.md` (6 broken links)

**Priority**: MEDIUM

**Broken Links → Correct Paths:**
- `CHANGELOG.md#110---2025-10-29` → `../../CHANGELOG.md#110---2025-10-29`
- `CHANGELOG.md#103---2025-10-27` → `../../CHANGELOG.md#103---2025-10-27`
- `CHANGELOG.md#102---2025-10-25` → `../../CHANGELOG.md#102---2025-10-25`
- `CHANGELOG.md#101---2025-10-24` → `../../CHANGELOG.md#101---2025-10-24`
- `docs/migration/v0-to-v1.md` → `../../docs/migration/v0-to-v1.md`
- `CHANGELOG.md` → `../../CHANGELOG.md`

**Note**: All CHANGELOG references need to go up two levels from `dev/audits/`

**Steps:**
1. Read `dev/audits/version-status.md`
2. Use Edit tool with `replace_all: true` for "CHANGELOG.md" → "../../CHANGELOG.md"
3. This should fix multiple references in one edit
4. Fix migration doc path separately
5. Verify all 6 links are fixed

---

### Task 1.30: Fix `dev/audits/python-version-analysis.md` (2 broken links)

**Priority**: LOW (likely false positives)

**Suspected False Positives:**
- `x:` - Not a real link
- `T` - Not a real link

**Steps:**
1. Read `dev/audits/python-version-analysis.md`
2. Search for what's being flagged as `x:` and `T`
3. If they're actual broken links, fix them
4. If false positives, document and skip
5. Consider if validation script needs adjustment

**Note**: These are likely parsing errors in the validation script, not actual broken links

---

## Phase 1 Completion & Validation

### Final Steps

**1. Run Local Validation**
```bash
chmod +x scripts/validate-docs.sh
./scripts/validate-docs.sh links
```

**2. Review Validation Output**
- Should show 0 broken links
- If any remain, investigate and fix

**3. Create Commit**
```bash
git add -A
git commit -m "docs: fix 60+ broken links after documentation consolidation

- Updated all references to match new docs structure
- Fixed paths for moved files (quickstart, FIRST_HOUR, TROUBLESHOOTING, etc.)
- Updated relative paths in guides, tutorials, and reference docs
- Removed or created missing file references in .github/docs and dev/
- Verified all internal links point to correct locations

Related: PR #105 - v1.1.1 documentation consolidation

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

**4. Push to Branch**
```bash
git push origin docs/consolidation
```

**5. Monitor CI**
- Check PR #105 on GitHub
- Wait for `validate-docs` job to complete
- Verify it passes (green checkmark)

**6. If Still Failing**
- Review new CI logs
- Identify remaining issues
- Iterate on fixes

---

## Success Criteria

- [ ] All 30 tasks completed
- [ ] Local validation passes: `./scripts/validate-docs.sh links`
- [ ] CI `validate-docs` job passes
- [ ] No broken internal links in documentation
- [ ] All cross-references work correctly
- [ ] Documentation browsing works seamlessly

---

## Troubleshooting

### If Local Validation Still Shows Errors

1. **Check for typos in paths**
   - Verify capitalization matches actual files
   - Ensure no extra spaces or special characters

2. **Verify files exist**
   - Use `ls -la docs/getting-started/` etc.
   - Confirm file names match exactly

3. **Check for absolute vs relative paths**
   - Ensure using relative paths correctly
   - Count `../` levels carefully

4. **Review anchor links**
   - For links like `#section-name`, verify section exists
   - Check markdown heading format

### If CI Validation Passes but Links Don't Work

1. **Test manually**
   - Navigate through docs following links
   - Open files in GitHub web interface

2. **Check case sensitivity**
   - Linux/CI is case-sensitive
   - Local macOS might not catch case issues

3. **Verify commit includes all files**
   - Use `git status` to check for uncommitted changes
   - Ensure all edits were saved

---

## Time Tracking

**Estimated Task Times:**
- Tasks 1.1-1.5 (Root & GitHub docs): 15 minutes
- Tasks 1.6-1.15 (Docs directory): 15 minutes
- Tasks 1.16-1.18 (Getting started): 10 minutes
- Tasks 1.19-1.23 (Guides): 10 minutes
- Tasks 1.24-1.26 (Tutorials): 5 minutes
- Tasks 1.27-1.30 (Dev directory): 5 minutes
- Validation & commit: 5 minutes

**Total: 65 minutes** (with buffer: 45-75 minutes)

---

## Next Steps

Once Phase 1 is complete and validated:
→ Proceed to **Phase 2: Fix Tox Validation Issue**
