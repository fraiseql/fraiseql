# Documentation Link Best Practices

**Purpose**: Ensure maintainable, resilient documentation links that survive refactoring and reorganization.

## Quick Reference

```markdown
# ✅ Recommended: Absolute from repo root
[Installation Guide](/docs/getting-started/installation.md)
[Examples](/examples/blog_api/)

# ❌ Fragile: Relative paths
[Installation Guide](../getting-started/installation.md)
[Examples](../../examples/blog_api/)

# ✅ External links
[PostgreSQL Docs](https://www.postgresql.org/docs/)

# ✅ Anchor links
[See Configuration](#configuration-options)
```

---

## Link Types

### 1. Absolute Repository Links (RECOMMENDED)

**Pattern**: `/path/from/repo/root/file.md`

```markdown
[Core Concepts](/docs/core/concepts-glossary.md)
[API Reference](/docs/api-reference/database.md)
[Examples Directory](/examples/)
[Contributing Guide](/CONTRIBUTING.md)
```

**Why absolute paths:**

- **Refactor-proof** - Links work from any location in the docs
- **Move-friendly** - Relocating a file doesn't break its outbound links
- **Predictable** - Always starts from repository root
- **IDE-friendly** - Most editors resolve absolute paths correctly
- **CI-validated** - Validation script checks absolute paths reliably

**When to use:**
- Internal documentation cross-references (95% of cases)
- Links to examples or source code
- Links to project root files (README, CONTRIBUTING, LICENSE)

### 2. Relative Links

**Pattern**: `./file.md` or `../sibling/file.md`

```markdown
# Same directory
[Style Guide](./style-guide.md)

# Parent directory
[Core Concepts](../core/concepts-glossary.md)

# Sibling directory
[Getting Started](../getting-started/installation.md)
```

**When to use (RARE):**
- Links within the same directory
- Generated documentation (e.g., API docs that move together)
- Templates where absolute paths don't apply

**Drawbacks:**
- Breaks when source file is moved
- Requires mental calculation of directory depth
- Hard to validate across refactorings

**Example of fragility:**

```markdown
# In: docs/advanced/authentication.md
[Installation](../getting-started/installation.md)  # Works

# After moving to: docs/guides/security/authentication.md
[Installation](../getting-started/installation.md)  # BROKEN! Now needs ../../getting-started/
```

### 3. External Links

**Pattern**: `https://...` or `http://...`

```markdown
[PostgreSQL Documentation](https://www.postgresql.org/docs/)
[GraphQL Spec](https://spec.graphql.org/)
[Python Type Hints](https://docs.python.org/3/library/typing.html)
```

**Best practices:**
- Use HTTPS when available
- Link to specific version docs when relevant
- Avoid linking to own repository on GitHub (use relative/absolute instead)

**Anti-pattern:**

```markdown
# ❌ Don't link to own repo via GitHub URL
[Core Concepts](https://github.com/fraiseql/fraiseql/blob/main/docs/core/concepts-glossary.md)

# ✅ Use absolute path instead
[Core Concepts](/docs/core/concepts-glossary.md)
```

### 4. Anchor Links

**Pattern**: `#section-name`

```markdown
# Link to section in same file
[See Installation Steps](#installation-steps)

# Link to section in different file
[Configuration Options](/docs/core/configuration.md#environment-variables)
```

**Rules:**
- GitHub auto-generates anchors from headers (lowercase, hyphens for spaces)
- Remove special characters (!, ?, etc.)
- Multiple words: use hyphens

**Example mapping:**

```markdown
## Installation Steps          → #installation-steps
## Why Use FraiseQL?           → #why-use-fraiseql
## Core Concepts & Glossary    → #core-concepts--glossary
```

---

## Directory vs File Links

### Files: Include Extension

```markdown
✅ [Configuration](/docs/core/configuration.md)
❌ [Configuration](../core/configuration.md)
```

### Directories: Include Trailing Slash

```markdown
✅ [Examples Directory](/examples/)
✅ [Core Docs](/docs/core/)
❌ [Examples Directory](/examples)
```

**Why this matters:**
- GitHub renders `/examples/` as directory listing
- `/examples` might 404 or redirect
- Trailing slash indicates browsable content

---

## Common Mistakes

### 1. Wrong Relative Path Depth

```markdown
# ❌ Wrong - Missing directory level
# File: docs/guides/performance-guide.md
[Installation](../getting-started/installation.md)

# ✅ Correct calculation (if using relative)
# docs/guides/performance-guide.md → docs/getting-started/installation.md
[Installation](../getting-started/installation.md)

# ✅ Better - Use absolute path
[Installation](/docs/getting-started/installation.md)
```

### 2. Linking to Directories Without Trailing Slash

```markdown
# ❌ May break in GitHub rendering
[Examples](/examples)

# ✅ Clear directory indication
[Examples](/examples/)
```

### 3. Using GitHub URLs for Internal Links

```markdown
# ❌ External link to own repository
[Core](https://github.com/fraiseql/fraiseql/blob/main/docs/core/README.md)

# ✅ Absolute path
[Core](/docs/core/README.md)
```

### 4. Inconsistent Link Styles in Same File

```markdown
# ❌ Mixed styles are confusing
[Installation](/docs/getting-started/installation.md)
[Core Concepts](../core/concepts-glossary.md)
[API Reference](/docs/api-reference/)

# ✅ Consistent absolute paths
[Installation](/docs/getting-started/installation.md)
[Core Concepts](/docs/core/concepts-glossary.md)
[API Reference](/docs/api-reference/)
```

---

## Validation

### Run Validation Locally

```bash
# Check all links
./scripts/validate-docs.sh links

# Run full validation suite
./scripts/validate-docs.sh all
```

**What the validator checks:**

```
1. Absolute links (/docs/file.md)
   - Resolves to: $PROJECT_ROOT/docs/file.md
   - Checks file/directory exists

2. Relative links (../file.md)
   - Resolves from current file's directory
   - Checks target exists after path resolution

3. External links (https://...)
   - Skipped (not validated locally)

4. Anchor links (#section)
   - Skipped (requires runtime rendering)
```

### CI Validation

**Runs on every PR:**

```yaml
# .github/workflows/docs.yml
- name: Validate Documentation
  run: ./scripts/validate-docs.sh all
```

**Checks:**
- Broken internal links
- Missing files
- Invalid paths
- Code syntax in examples

**Fix broken links:**

```bash
# 1. Run validation to find errors
./scripts/validate-docs.sh links

# 2. Example output:
# [ERROR] Broken link in docs/advanced/authentication.md:
#         ../core/concepts-glossary.md (resolved to: docs/core/concepts-glossary.md)

# 3. Fix the link:
# Old: [Concepts](../core/concepts-glossary.md)
# New: [Concepts](/docs/core/concepts-glossary.md)

# 4. Re-run validation
./scripts/validate-docs.sh links
```

---

## Debugging Broken Links

### Understanding Validation Errors

```bash
# Error message format:
[ERROR] Broken link in <source-file>: <link-text> (resolved to: <target-path>)

# Example:
[ERROR] Broken link in docs/guides/troubleshooting.md:
        ../core/database-api.md (resolved to: docs/core/database-api.md)
```

**Common causes:**

1. **File was renamed/moved**
   ```markdown
   # Link points to old location
   [Database API](../core/database-api.md)

   # File was renamed to: docs/api-reference/database.md
   # Fix: [Database API](/docs/api-reference/database.md)
   ```

2. **Wrong relative path depth**
    ```markdown
    # From: docs/development/link-best-practices.md
    [Core](https://github.com/fraiseql/fraiseql/blob/main/docs/core/concepts-glossary.md)  # Wrong - uses GitHub URL
    [Core](../core/concepts-glossary.md)  # Correct - uses relative path

    # Better: Use absolute
    [Core](/docs/core/concepts-glossary.md)
    ```

3. **Using GitHub URLs for internal links**
   ```markdown
   [Config](https://github.com/fraiseql/fraiseql/blob/main/docs/core/configuration.md)  # External link to own repo
   [Config](/docs/core/configuration.md)  # Correct absolute path
   ```

### Debugging Steps

```bash
# 1. Find the broken link
./scripts/validate-docs.sh links

# 2. Check if target file exists
ls -la docs/core/concepts-glossary.md

# 3. Search for other references to same file
grep -r "concepts-glossary.md" docs/

# 4. Verify your fix
./scripts/validate-docs.sh links
```

---

## Migration Guide

### Converting Relative to Absolute Links

```markdown
# Before: docs/advanced/authentication.md
[Installation Guide](../getting-started/installation.md)
[Core Concepts](../core/concepts-glossary.md)
[Examples](../../examples/)

# After: docs/advanced/authentication.md (same file, different links)
[Installation Guide](/docs/getting-started/installation.md)
[Core Concepts](/docs/core/concepts-glossary.md)
[Examples](/examples/)
```

**Benefits after conversion:**
- File can be moved without updating links
- Links work from any documentation location
- CI validation catches broken links immediately

### Bulk Migration Script

```bash
# Find all relative links in documentation
grep -r "](\.\./" docs/ | wc -l

# Review and convert high-traffic files first:
# - README files
# - Getting started guides
# - Core concepts
# - API reference indexes
```

---

## Best Practices Summary

### DO

✅ Use absolute paths from repo root (`/docs/...`)
✅ Include file extensions (`.md`)
✅ Add trailing slash for directories (`/examples/`)
✅ Run validation before committing
✅ Use descriptive link text
✅ Link to specific sections when relevant

### DON'T

❌ Use relative paths unless necessary
❌ Link to own repo via GitHub URLs
❌ Forget file extensions
❌ Mix link styles in same file
❌ Skip validation checks
❌ Use generic link text ("click here")

---

## Examples from FraiseQL Docs

### Good Examples

```markdown
# Clear, absolute paths
[Installation Guide](/docs/getting-started/installation.md)
[Core Concepts](/docs/core/concepts-glossary.md)
[Performance Optimization](/docs/guides/performance-guide.md)
[Blog API Example](/examples/blog_api/)

# Descriptive link text with context
See the [filter operators reference](/docs/advanced/filter-operators.md)
for a complete list of supported operators.

For production deployment, review the
[deployment guide](/docs/production/deployment.md) and
[security checklist](/docs/production/security.md).
```

### Improved Examples

```markdown
# ❌ Before: Fragile relative path
For more details, see [here](../guides/performance-guide.md).

# ✅ After: Absolute path with descriptive text
For query optimization strategies, see the
[Performance Guide](/docs/guides/performance-guide.md).

# ❌ Before: Multiple relative depths
[Installation](../getting-started/installation.md)
[Core](../core/concepts-glossary.md)

# ✅ After: Consistent absolute paths
[Installation Guide](/docs/getting-started/installation.md)
[Core Concepts](/docs/core/concepts-glossary.md)
```

---

## Related Documentation

- [Style Guide](/docs/development/style-guide.md) - Code and documentation standards
- [Contributing Guide](/CONTRIBUTING.md) - Development workflow
- [Documentation Structure](/docs/README.md) - Organization overview

---

**Questions?** Open an issue or discussion on GitHub.
