# Documentation Consolidation Plan

**Objective**: Consolidate scattered documentation into a clear, maintainable structure before v1.1.2 release.

**Problem**: Currently 50+ markdown files across 10 directories with overlapping content, outdated files, and unclear navigation.

---

## 📊 Current State Analysis

### Documentation Locations
```
Root (18 files)          - Mix of dev notes, release docs, guides
├── docs/ (10 files)     - User documentation
├── .github/ (6 files)   - CI/CD documentation
├── fraiseql/ (5 files)  - Internal dev docs
├── fraiseql_rs/ (4 files) - Rust extension docs
├── examples/ (5 files)  - Example documentation
├── benchmarks/ (4 files) - Performance docs
├── tests/ (1 file)      - Test documentation
├── scripts/ (1 file)    - Script documentation
├── grafana/ (1 file)    - Monitoring docs
└── archive/ (1 file)    - Archived content
```

### Issues
1. **Duplication**: `INSTALLATION.md` in both root and `docs/`, `CONTRIBUTING.md` in both root and `docs/`
2. **Scattered Release Notes**: 5+ release-related files in root
3. **Outdated Content**: Python 3.11+ badge but requires 3.13+, old release execution docs
4. **Unclear Ownership**: Dev notes mixed with user documentation
5. **Poor Discoverability**: No clear entry point for different audiences

---

## 🎯 Target Structure

```
/ (Root - 6 files)
├── README.md              # Main entry point
├── CHANGELOG.md           # Version history
├── CONTRIBUTING.md        # How to contribute
├── SECURITY.md            # Security policy
├── LICENSE                # MIT license
└── pyproject.toml         # Project config

docs/ (User Documentation)
├── README.md              # Documentation hub
├── getting-started/
│   ├── installation.md
│   ├── quickstart.md
│   └── first-hour.md
├── guides/
│   ├── understanding-fraiseql.md
│   ├── performance-guide.md
│   ├── troubleshooting.md
│   └── nested-array-filtering.md
├── advanced/
│   ├── rust-extension.md
│   └── advanced-patterns.md
├── reference/
│   ├── api-reference.md
│   └── testing-checklist.md
└── examples/
    └── INDEX.md

.github/ (Repository Management)
├── workflows/             # CI/CD workflows
├── ISSUE_TEMPLATE/
├── pull_request_template.md
└── docs/                  # CI/CD documentation
    ├── branch-protection.md
    ├── workflows.md       # Consolidated workflow docs
    └── trusted-publishing.md

dev/ (Developer Internal Docs)
├── README.md              # Dev documentation hub
├── releases/
│   ├── release-process.md
│   ├── v1.0.1-execution.md
│   └── v1.1.1-plan.md
├── architecture/
│   ├── vision.md
│   ├── component-prds.md
│   └── audiences.md
├── audits/
│   ├── python-version-analysis.md
│   ├── type-hinting-audit.md
│   └── version-status.md
└── rust/
    ├── implementation.md
    ├── api.md
    └── benchmarks/

archive/ (Historical/Deprecated)
├── README.md              # What's archived and why
└── [old release notes, outdated guides]
```

---

## 🔄 Phased Execution Plan

### Phase 1: Audit & Categorize (30 minutes)
**Goal**: Understand what we have and make decisions about each file.

#### Tasks:
1. **Create categorization matrix** for all 50+ files:
   - Keep as-is (good location, current content)
   - Move (wrong location, good content)
   - Consolidate (duplicate content)
   - Archive (outdated but historical value)
   - Delete (no value, outdated)

2. **Decision criteria**:
   - User-facing → `docs/`
   - Developer internal → `dev/`
   - CI/CD specific → `.github/docs/`
   - Outdated but valuable → `archive/`
   - Truly obsolete → Delete

3. **Deliverable**: `DOCS_AUDIT.md` with categorization table

---

### Phase 2: Create New Structure (15 minutes)
**Goal**: Set up the target directory structure.

#### Tasks:
1. Create new directories:
   ```bash
   mkdir -p dev/{releases,architecture,audits,rust/benchmarks}
   mkdir -p docs/{getting-started,guides,advanced,reference}
   mkdir -p .github/docs
   ```

2. Create hub files:
   - `dev/README.md` - Developer documentation hub
   - `.github/docs/README.md` - CI/CD documentation hub
   - Update `docs/README.md` - User documentation hub

3. Create `archive/README.md` explaining archive purpose

---

### Phase 3: Move & Consolidate (45 minutes)
**Goal**: Relocate files to correct locations and merge duplicates.

#### Priority 1: Root Cleanup (High Impact)
Move out of root:
- `AUDIENCES.md` → `dev/architecture/audiences.md`
- `PYTHON_*.md` (3 files) → `dev/audits/`
- `RELEASE_*.md` (5 files) → `dev/releases/`
- `VERSION_STATUS.md` → `dev/audits/version-status.md`
- `PERFORMANCE_GUIDE.md` → `docs/guides/performance-guide.md`
- `GITHUB_ACTIONS_SETUP.md` → `.github/docs/setup.md`
- `PUSH_INSTRUCTIONS.md` → `dev/releases/push-instructions.md`

Consolidate duplicates:
- Keep `INSTALLATION.md` in root (PyPI standard), move detailed version to `docs/getting-started/installation.md`
- Keep `CONTRIBUTING.md` in root (GitHub standard), remove `docs/CONTRIBUTING.md`

#### Priority 2: fraiseql/ Directory
Move to dev/:
- `fraiseql/VISION.md` → `dev/architecture/vision.md`
- `fraiseql/COMPONENT_PRDS.md` → `dev/architecture/component-prds.md`
- `fraiseql/ADVANCED_PATTERNS.md` → `docs/advanced/advanced-patterns.md`
- `fraiseql/GETTING_STARTED.md` → Consolidate into `docs/getting-started/quickstart.md`

#### Priority 3: fraiseql_rs/ Directory
Move to dev/rust/:
- `fraiseql_rs/API.md` → `dev/rust/api.md`
- `fraiseql_rs/IMPLEMENTATION_COMPLETE.md` → `dev/rust/implementation.md`
- `fraiseql_rs/PHASE_6_BASELINE_RESULTS.md` → `dev/rust/benchmarks/phase-6-baseline.md`
- `fraiseql_rs/README.md` → Keep (technical README), also create `docs/advanced/rust-extension.md` (user guide)

#### Priority 4: .github/ Consolidation
Consolidate CI/CD docs:
- `.github/CICD_REVIEW_REQUEST.md` → Archive (one-time review)
- `.github/PUBLISH_WORKFLOW_CHANGES.md` → `.github/docs/publish-workflow.md`
- `.github/branch-protection.md` → `.github/docs/branch-protection.md`

#### Priority 5: docs/ Reorganization
Organize existing docs:
- `docs/INSTALLATION.md` → `docs/getting-started/installation.md`
- `docs/quickstart.md` → `docs/getting-started/quickstart.md`
- `docs/FIRST_HOUR.md` → `docs/getting-started/first-hour.md`
- `docs/UNDERSTANDING.md` → `docs/guides/understanding-fraiseql.md`
- `docs/TROUBLESHOOTING*.md` → `docs/guides/troubleshooting.md`
- `docs/nested-array-filtering.md` → `docs/guides/nested-array-filtering.md`
- `docs/TESTING_CHECKLIST.md` → `docs/reference/testing-checklist.md`

---

### Phase 4: Update Cross-References (30 minutes)
**Goal**: Fix all broken links after moves.

#### Tasks:
1. **Update root README.md**:
   - Fix documentation links
   - Update badges (Python 3.13+)
   - Add clear navigation to `docs/`, `dev/`, `.github/docs/`

2. **Update hub files**:
   - `docs/README.md` - Update all paths
   - `dev/README.md` - Link to all dev docs
   - `.github/docs/README.md` - Link to workflow docs

3. **Search and replace common patterns**:
   ```bash
   # Find all markdown links
   grep -r "](.*\.md)" --include="*.md" .

   # Update common patterns
   sed -i 's|docs/INSTALLATION.md|docs/getting-started/installation.md|g' **/*.md
   ```

4. **Verify links**: Use a markdown link checker

---

### Phase 5: Archive & Delete (15 minutes)
**Goal**: Clean up outdated content.

#### Archive (Keep for history):
- `RELEASE_EXECUTION_v1.0.1.md` → `archive/releases/`
- `RELEASE_NOTES_v1.0.1.md` → `archive/releases/`
- Old benchmark files if superseded

#### Delete (No longer relevant):
- `PUSH_INSTRUCTIONS.md` (after consolidating into dev docs)
- `.github/CICD_REVIEW_REQUEST.md` (one-time review, no longer needed)
- Duplicate files after consolidation

#### Update archive/README.md:
```markdown
# Archive

This directory contains historical documentation that is no longer
current but preserved for reference.

## Contents
- `releases/` - Historical release execution notes
- [List other archived content]

## Why Archive?
We archive rather than delete documentation that:
- Has historical value for understanding decisions
- Documents past implementations
- May be referenced in old issues/PRs
```

---

### Phase 6: Polish & Validate (30 minutes)
**Goal**: Ensure everything works and is discoverable.

#### Tasks:
1. **Update CHANGELOG.md**:
   ```markdown
   ## [Unreleased]
   ### Documentation
   - Reorganized documentation structure for clarity
   - Consolidated scattered docs into docs/, dev/, .github/docs/
   - Archived outdated release notes
   - Fixed all cross-references
   ```

2. **Test navigation**:
   - Can a new user find quickstart?
   - Can a contributor find release process?
   - Can a maintainer find CI/CD docs?

3. **Validate links**:
   ```bash
   # Install markdown link checker
   npm install -g markdown-link-check

   # Check all markdown files
   find . -name "*.md" -not -path "./node_modules/*" -exec markdown-link-check {} \;
   ```

4. **Update badges in README**:
   - Fix Python version badge (3.13+)
   - Verify all badges work
   - Update status badges if needed

5. **Create PR**:
   - Title: "docs: consolidate and reorganize documentation structure"
   - Description: Link to this plan, explain rationale
   - Label: documentation

---

## 📋 File-by-File Action Matrix

### Root Level (Action Required)

| File | Action | Destination | Reason |
|------|--------|-------------|--------|
| README.md | **Keep** | - | Main entry point (standard) |
| CHANGELOG.md | **Keep** | - | Version history (standard) |
| CONTRIBUTING.md | **Keep** | - | GitHub standard location |
| SECURITY.md | **Keep** | - | Security policy (standard) |
| LICENSE | **Keep** | - | Required |
| INSTALLATION.md | **Keep** | - | PyPI/pip standard |
| AUDIENCES.md | **Move** | dev/architecture/ | Internal planning doc |
| PYTHON_VERSION_*.md (3) | **Move** | dev/audits/ | Internal audit docs |
| RELEASE_*.md (5) | **Move** | dev/releases/ | Internal release docs |
| VERSION_STATUS.md | **Move** | dev/audits/ | Internal status tracking |
| PERFORMANCE_GUIDE.md | **Move** | docs/guides/ | User-facing guide |
| GITHUB_ACTIONS_SETUP.md | **Move** | .github/docs/ | CI/CD documentation |
| PUSH_INSTRUCTIONS.md | **Move** | dev/releases/ | Internal process |
| RELEASE_NOTES.md | **Consolidate** | CHANGELOG.md | Duplicate information |

### .github/ (Consolidate CI/CD Docs)

| File | Action | Destination | Reason |
|------|--------|-------------|--------|
| workflows/*.yml | **Keep** | - | Required workflows |
| ISSUE_TEMPLATE/ | **Keep** | - | GitHub templates |
| pull_request_template.md | **Keep** | - | GitHub template |
| branch-protection.md | **Move** | .github/docs/ | CI/CD documentation |
| CICD_REVIEW_REQUEST.md | **Archive** | archive/ | One-time review |
| PUBLISH_WORKFLOW_CHANGES.md | **Move** | .github/docs/ | Workflow documentation |

### fraiseql/ (Developer Docs)

| File | Action | Destination | Reason |
|------|--------|-------------|--------|
| README.md | **Keep** | - | Package README |
| VISION.md | **Move** | dev/architecture/ | Strategic planning |
| COMPONENT_PRDS.md | **Move** | dev/architecture/ | Internal specs |
| ADVANCED_PATTERNS.md | **Move** | docs/advanced/ | User-facing guide |
| GETTING_STARTED.md | **Consolidate** | docs/getting-started/ | Merge into quickstart |

### fraiseql_rs/ (Rust Docs)

| File | Action | Destination | Reason |
|------|--------|-------------|--------|
| README.md | **Keep** | - | Technical README |
| API.md | **Move** | dev/rust/ | Developer reference |
| IMPLEMENTATION_COMPLETE.md | **Move** | dev/rust/ | Implementation notes |
| PHASE_6_BASELINE_RESULTS.md | **Move** | dev/rust/benchmarks/ | Benchmark results |

### docs/ (User Documentation)

| File | Action | Destination | Reason |
|------|--------|-------------|--------|
| README.md | **Update** | - | Documentation hub |
| CONTRIBUTING.md | **Delete** | - | Duplicate of root |
| INSTALLATION.md | **Move** | getting-started/ | Better organization |
| quickstart.md | **Move** | getting-started/ | Better organization |
| FIRST_HOUR.md | **Move** | getting-started/ | Better organization |
| UNDERSTANDING.md | **Move** | guides/ | Better categorization |
| TROUBLESHOOTING*.md | **Consolidate** | guides/troubleshooting.md | Single guide |
| nested-array-filtering.md | **Move** | guides/ | Feature guide |
| TESTING_CHECKLIST.md | **Move** | reference/ | Reference material |

### examples/, benchmarks/, tests/, scripts/, grafana/
**Keep as-is** - Already well-organized, just update cross-references

---

## 🎯 Success Criteria

✅ **Discoverability**:
- New users can find quickstart in < 30 seconds
- Contributors can find release process in < 30 seconds
- Maintainers can find CI/CD docs in < 30 seconds

✅ **Maintainability**:
- No duplicate documentation
- Clear ownership (user vs developer vs CI/CD)
- All links work (0 broken links)

✅ **Clarity**:
- Root has ≤ 6 files (standards only)
- Each directory has clear purpose
- Hub files guide navigation

✅ **Completeness**:
- All valuable content preserved
- Historical docs archived (not deleted)
- CHANGELOG documents the reorganization

---

## 🚀 Execution Timeline

**Total Time**: ~2.5 hours

1. Phase 1 (Audit): 30 min
2. Phase 2 (Structure): 15 min
3. Phase 3 (Move): 45 min
4. Phase 4 (Links): 30 min
5. Phase 5 (Archive): 15 min
6. Phase 6 (Polish): 30 min

**Recommended**: Execute in one session to maintain consistency and avoid merge conflicts.

---

## 📝 Post-Consolidation Maintenance

### New Content Guidelines:

**User-facing** → `docs/`
- Getting started guides
- Feature documentation
- Troubleshooting
- API references

**Developer internal** → `dev/`
- Architecture decisions
- Release processes
- Internal audits
- Planning documents

**CI/CD** → `.github/docs/`
- Workflow documentation
- Branch protection
- Deployment guides

**Code-specific** → In-tree
- `fraiseql/README.md` - Package README
- `fraiseql_rs/README.md` - Rust extension README
- `examples/README.md` - Examples index

### Review Checklist (Before Merging New Docs):
- [ ] File in correct directory?
- [ ] Links to external docs working?
- [ ] Added to appropriate hub file (README)?
- [ ] No duplicate content?
- [ ] Clear target audience?

---

## 🔗 Related Issues

This consolidation addresses:
- Documentation scattered across too many locations
- Duplicate installation/contributing guides
- Unclear entry points for different audiences
- Root directory cluttered with internal docs
- Broken cross-references after moves

**Next Steps After Consolidation**:
1. Update CI to validate markdown links
2. Add docs preview in PR checks
3. Consider adding docs versioning for major releases
