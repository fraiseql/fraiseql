# Phase 16: Documentation QA & Validation

**Objective**: Comprehensive quality assurance across all 70,000+ lines of documentation

**Duration**: 3-4 days

**Estimated Changes**: 500-1,000 line edits across multiple files (link fixes, code corrections, clarity improvements)

**Dependencies**: Phase 15 complete, all documentation written (37 files)

---

## Success Criteria

- [ ] All markdown files pass linting (no syntax errors)
- [ ] All cross-references are valid (no broken links)
- [ ] All code examples are syntactically correct per language
- [ ] All SQL examples run without errors
- [ ] GraphQL queries parse correctly
- [ ] All file paths exist and are correct
- [ ] Consistent formatting across all documents
- [ ] Consistent terminology throughout
- [ ] All example code follows stated best practices
- [ ] No orphaned sections or incomplete TODOs
- [ ] All tables of contents match actual sections
- [ ] Front matter (Status, Audience, etc.) complete on all docs

---

## TDD Cycles

### Cycle 1: Markdown Linting & Syntax Validation

**RED**: Write test to validate markdown
```bash
# Test: All markdown files valid
for file in docs/**/*.md; do
  python3 tools/validate-markdown.py "$file" || exit 1
done
```

**GREEN**: Fix markdown syntax errors
```bash
# Run markdownlint
markdownlint-cli2 docs/**/*.md --fix

# Check for common issues
grep -r "^##" docs --include="*.md" | grep -v " " && exit 1  # H2 must have space
grep -r "\[.*\]" docs --include="*.md" | grep -v "\[.*\](.*)" && echo "Unlinked brackets found"
```

**REFACTOR**: Ensure consistency
```bash
# Verify consistent heading structure
# H1 at top, H2 for sections, H3 for subsections
python3 tools/validate-heading-structure.py docs/
```

**CLEANUP**: Fix any warnings
```bash
# Trailing whitespace
find docs -name "*.md" -exec sed -i 's/[[:space:]]*$//' {} \;

# Windows line endings
dos2unix docs/**/*.md
```

---

### Cycle 2: Cross-Reference & Link Validation

**RED**: Test all links
```bash
# Test: All links work
python3 tools/validate-docs-links.py docs/

# Expected output: 0 broken links
```

**GREEN**: Fix broken links
```bash
# Find all links
grep -r "\[.*\](.*)" docs --include="*.md" | \
  python3 tools/extract-links.py | \
  python3 tools/validate-links.py --fix
```

**REFACTOR**: Verify relative paths
```bash
# All internal links should use relative paths
grep -r "http.*fraiseql" docs --include="*.md" && \
  echo "Found absolute URLs (should be relative)"

# Convert absolute to relative
python3 tools/convert-to-relative-links.py docs/
```

**CLEANUP**: Document link structure
```bash
# Generate link map for reference
python3 tools/generate-link-map.py docs/ > docs/LINKS.md
```

---

### Cycle 3: Code Example Validation

**RED**: Test all code examples
```bash
# Test: All code examples are valid
python3 tools/validate-code-examples.py docs/

# Expected: 0 syntax errors across all languages
```

**GREEN**: Fix syntax errors
- **Python**: `python3 -m py_compile example.py`
- **TypeScript**: `npx tsc --noEmit example.ts`
- **Go**: `go fmt example.go && go vet example.go`
- **Java**: `javac Example.java`
- **SQL**: `postgres -c "SELECT 1" < example.sql`
- **GraphQL**: `graphql-validate schema.graphql`

**REFACTOR**: Extract examples to separate files
```bash
# Store runnable examples in tests/examples/
# Link from docs for easy verification
docs/examples/react-apollo-guide.md -> tests/examples/react-apollo/
```

**CLEANUP**: Remove incomplete examples
```bash
# Flag any example with "..." or "FIXME" or "TODO"
grep -r "\.\.\." docs --include="*.md" && \
  echo "Incomplete examples found"
```

---

### Cycle 4: SQL Query Validation

**RED**: Test all SQL examples
```bash
# Test: All SQL runs without error
python3 tools/validate-sql-examples.py docs/

# Against real PostgreSQL instance
```

**GREEN**: Create test database, run all SQL
```bash
# Setup test database
psql -c "CREATE DATABASE doc_test"

# Run all SQL examples
for file in $(grep -r "^```sql$" docs --include="*.md" -l); do
  python3 tools/extract-sql.py "$file" | psql doc_test || echo "Error in $file"
done
```

**REFACTOR**: Ensure transactions are explicit
```sql
-- All schema-modifying SQL should be explicit
BEGIN;
  -- SQL here
COMMIT;
-- or ROLLBACK; for error handling
```

**CLEANUP**: Remove test database
```bash
psql -c "DROP DATABASE doc_test"
```

---

### Cycle 5: GraphQL Query Validation

**RED**: Test all GraphQL examples
```bash
# Test: All GraphQL parses correctly
python3 tools/validate-graphql-examples.py docs/

# Using graphql-core-3
```

**GREEN**: Validate against schema
```python
from graphql import build_schema, parse, validate

schema = build_schema(FRAISEQL_SCHEMA)
query = parse(QUERY_FROM_DOCS)
errors = validate(schema, query)
assert len(errors) == 0, f"GraphQL validation errors: {errors}"
```

**REFACTOR**: Ensure all queries are named
```graphql
# ✅ Good: Named query
query GetUsers {
  users { id name }
}

# ❌ Bad: Anonymous
{
  users { id name }
}
```

**CLEANUP**: Verify all queries are executable
```bash
# Run against actual FraiseQL server
fraiseql-query --endpoint http://localhost:5000/graphql \
  --file docs/examples/query.graphql
```

---

### Cycle 6: Terminology & Consistency

**RED**: Test for inconsistent terminology
```bash
# Test: Consistent terminology
python3 tools/validate-terminology.py docs/

# Check common variations
grep -r "FraiseQL GraphQL" docs --include="*.md" && echo "Inconsistent"
grep -r "Fraiseql" docs --include="*.md" && echo "Wrong capitalization"
grep -r "SDK\|sdk\|Sdk" docs --include="*.md" | wc -l
```

**GREEN**: Create terminology glossary
```yaml
# tools/glossary.yaml
FraiseQL: GraphQL backend
SDK: Software Development Kit
RLS: Row-Level Security
OLAP: Online Analytical Processing
OT: Operational Transformation
```

**REFACTOR**: Standardize throughout
```bash
# Replace inconsistencies
sed -i 's/Fraiseql/FraiseQL/g' docs/**/*.md
sed -i 's/\bSDK\b/SDK/g' docs/**/*.md  # Only whole word
```

**CLEANUP**: Verify consistency
```bash
python3 tools/check-terminology.py docs/ --strict
```

---

### Cycle 7: Document Metadata & Structure

**RED**: Test front matter completeness
```bash
# Test: All docs have required front matter
python3 tools/validate-front-matter.py docs/

# Required fields:
# - Status (✅ Production Ready, 🚧 Work in Progress, etc.)
# - Audience
# - Last Updated
# - Version (v2.0.0-alpha.1)
```

**GREEN**: Add missing front matter
```bash
# For each file missing metadata:
python3 tools/add-front-matter.py docs/file.md \
  --status "✅ Production Ready" \
  --audience "Developers" \
  --last-updated "2026-02-05"
```

**REFACTOR**: Ensure TOC matches sections
```bash
# Generate TOC from headers
python3 tools/generate-toc.py docs/file.md > TOC.md

# Verify it matches existing TOC
diff -u TOC.md docs/file.md | grep "^+\|^-"
```

**CLEANUP**: Remove development markers
```bash
# Remove any Phase/TODO/FIXME markers
grep -r "Phase\|TODO\|FIXME" docs --include="*.md" && \
  echo "Development markers found" && exit 1
```

---

### Cycle 8: File Organization & Completeness

**RED**: Test file structure
```bash
# Test: All referenced files exist
python3 tools/validate-file-structure.py docs/

# Expected directory structure:
# docs/
# ├── guides/
# ├── patterns/
# ├── examples/
# ├── tutorials/
# ├── integrations/sdk/
# └── integrations/framework-guides/
```

**GREEN**: Create missing files/directories
```bash
# Create any missing structure
mkdir -p docs/{guides/clients,patterns,examples,tutorials,integrations/{sdk,framework-guides}}

# Verify all expected files exist
ls docs/guides/clients/*.md | wc -l  # Should be 6
ls docs/patterns/*.md | wc -l         # Should be 7
ls docs/examples/*.md | wc -l         # Should be 4
```

**REFACTOR**: Ensure consistent naming
```bash
# Files should be lowercase with hyphens
for file in docs/**/*[A-Z]*.md; do
  mv "$file" "$(echo $file | tr '[:upper:]' '[:lower:]')"
done

# Verify naming pattern
find docs -name "*.md" ! -name "*-*.md" | head
```

**CLEANUP**: Remove duplicate files
```bash
# Check for duplicate content
python3 tools/find-duplicate-content.py docs/

# Remove duplicates, keep canonical version
```

---

### Cycle 9: Image & Asset Validation

**RED**: Test all image references
```bash
# Test: All referenced images exist
python3 tools/validate-images.py docs/

# Expected: All [alt text](path/to/image.png) files exist
```

**GREEN**: Create missing assets
```bash
# For any missing images, create placeholders or add to TODO list
python3 tools/find-missing-images.py docs/ | tee missing-images.txt

# If images needed: create them
# If not needed: remove references
```

**REFACTOR**: Optimize image sizes
```bash
# Ensure images are reasonably sized
python3 tools/optimize-images.py docs/

# Expected: .png < 500KB, .jpg < 300KB
```

**CLEANUP**: Verify all images referenced
```bash
# Find unused images
find docs -name "*.{png,jpg,svg}" | while read img; do
  grep -r "$(basename $img)" docs --include="*.md" || echo "Unused: $img"
done
```

---

## Verification Checklist

Run all validations:

```bash
#!/bin/bash
set -e

echo "Running documentation QA..."

echo "1. Markdown linting..."
markdownlint-cli2 docs/**/*.md || exit 1

echo "2. Link validation..."
python3 tools/validate-docs-links.py docs/ || exit 1

echo "3. Code example validation..."
python3 tools/validate-code-examples.py docs/ || exit 1

echo "4. SQL validation..."
python3 tools/validate-sql-examples.py docs/ || exit 1

echo "5. GraphQL validation..."
python3 tools/validate-graphql-examples.py docs/ || exit 1

echo "6. Terminology validation..."
python3 tools/check-terminology.py docs/ --strict || exit 1

echo "7. Front matter validation..."
python3 tools/validate-front-matter.py docs/ || exit 1

echo "8. File structure validation..."
python3 tools/validate-file-structure.py docs/ || exit 1

echo "9. Image validation..."
python3 tools/validate-images.py docs/ || exit 1

echo "✅ All QA checks passed!"
```

---

## Quality Metrics

Track these metrics:

| Metric | Target | Current |
|--------|--------|---------|
| Broken links | 0 | ? |
| Code examples with errors | 0 | ? |
| Missing front matter | 0 | ? |
| Documentation coverage | 100% | ? |
| Average time to find info | < 2 min | ? |
| Code correctness | 100% | ? |

---

## Status

- [ ] Not Started
- [ ] In Progress
- [ ] Complete

---

## Notes

- All tools referenced should be created in `tools/` directory
- Tests should be automated in CI/CD pipeline
- Documentation should be reviewed by at least one other person per major section
- Any changes should be committed as: `docs(qa): Fix [issue type] in [file]`

---

**Phase Dependencies**:
- Requires: Phase 15 complete
- Blocks: Phase 17

**Estimated Effort**: 10-15 developer-hours for automated tooling + 5-10 hours for manual review
