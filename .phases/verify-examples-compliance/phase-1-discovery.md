# Phase 1: Discovery - Inventory Examples and Patterns

## Objective

Systematically discover and catalog all examples, SQL files, and pattern documentation to understand the full scope of verification work.

## Context

FraiseQL has multiple example applications demonstrating different patterns:
- `examples/blog_api/` - Enterprise patterns with Trinity identifiers
- `examples/ecommerce_api/` - Complex validation patterns
- `examples/enterprise_patterns/` - Complete enterprise patterns
- Plus 30+ other examples

Each example should follow the Trinity pattern and CQRS architecture consistently.

## Files to Modify/Create

### Read-Only (Analysis)
- `examples/*/README.md` - All example documentation
- `examples/*/db/**/*.sql` - All SQL files
- `examples/*/*.py` - All Python type definitions
- `docs/core/concepts-glossary.md` - Core pattern documentation
- `~/.claude/skills/printoptim-database-patterns.md` - Trinity pattern reference

### Create
- `.phases/verify-examples-compliance/inventory.json` - Structured inventory
- `.phases/verify-examples-compliance/discovery-report.md` - Human-readable findings

## Implementation Steps

### Step 1: Inventory All Examples

Find and catalog all example directories:

```bash
# List all examples
find examples/ -maxdepth 1 -type d -not -path examples/ | sort

# For each example, check for:
# - README.md (description, patterns documented)
# - db/ directory (SQL files)
# - *.py files (GraphQL types)
# - tests/ directory (test coverage)
```

Expected output structure:
```json
{
  "examples/blog_api": {
    "readme": "examples/blog_api/README.md",
    "sql_files": [
      "examples/blog_api/db/0_schema/01_write/011_tb_user.sql",
      "examples/blog_api/db/0_schema/02_read/021_user/0211_v_user.sql",
      ...
    ],
    "python_files": ["app.py", "types.py", ...],
    "has_tests": true,
    "patterns_claimed": ["Trinity", "CQRS", "Enterprise mutations"]
  },
  ...
}
```

### Step 2: Inventory SQL Pattern Examples in Documentation

Extract all SQL examples from documentation:

```bash
# Find all SQL code blocks in markdown files
grep -r "```sql" docs/ examples/*/README.md ~/.claude/skills/printoptim-database-patterns.md

# For each example, extract:
# - File location
# - Line numbers
# - Pattern being demonstrated
# - Expected behavior described
```

Expected output:
```json
{
  "docs/core/concepts-glossary.md": {
    "examples": [
      {
        "line_range": "75-97",
        "pattern": "Trinity Identifiers - Base Table",
        "code": "CREATE TABLE tb_user (...)",
        "description": "Every table has pk_*, id, identifier"
      },
      {
        "line_range": "176-214",
        "pattern": "JSONB View with Trinity",
        "code": "CREATE VIEW v_user AS SELECT id, jsonb_build_object(...)",
        "description": "Views expose id, include pk_* only if referenced"
      }
    ]
  }
}
```

### Step 3: Extract Pattern Rules

From documentation and code, extract verifiable rules:

**Trinity Pattern Rules:**
```yaml
tables:
  - rule: "Must have pk_<entity> INTEGER GENERATED ... PRIMARY KEY"
    severity: ERROR
    check_method: regex
    pattern: "pk_\\w+\\s+INTEGER\\s+GENERATED"

  - rule: "Must have id UUID DEFAULT gen_random_uuid() ... UNIQUE"
    severity: ERROR
    check_method: regex
    pattern: "id\\s+UUID\\s+DEFAULT\\s+gen_random_uuid"

  - rule: "May have identifier TEXT ... UNIQUE (optional)"
    severity: WARNING
    check_method: regex
    pattern: "identifier\\s+TEXT.*UNIQUE"

views:
  - rule: "Must have 'id' column (not in JSONB)"
    severity: ERROR
    check_method: sql_parse

  - rule: "Include pk_* only if view is referenced by other views"
    severity: WARNING
    check_method: dependency_analysis

  - rule: "JSONB must NOT contain pk_* field"
    severity: ERROR
    check_method: jsonb_analysis
    pattern: "'pk_\\w+'"

foreign_keys:
  - rule: "Must reference pk_* column, not id"
    severity: ERROR
    check_method: sql_parse
    pattern: "REFERENCES.*\\(pk_\\w+\\)"

  - rule: "FK column must be INTEGER type"
    severity: ERROR
    check_method: sql_parse

helper_functions:
  - rule: "Name pattern: core.get_pk_<entity>(tenant_id?, <entity>_id)"
    severity: ERROR
    check_method: function_signature

  - rule: "Variables: v_<entity>_pk (INTEGER), v_<entity>_id (UUID)"
    severity: ERROR
    check_method: variable_naming
```

### Step 4: Catalog Actual Patterns in Examples

For each example, analyze actual implementation:

```bash
# For blog_api example:
# - Tables: Check tb_user, tb_post, tb_comment for Trinity pattern
# - Views: Check v_user, v_post, v_comment for correct structure
# - Functions: Check variable naming, helper usage
# - Python types: Check no pk_* exposure
```

Example findings:
```json
{
  "examples/blog_api/db/0_schema/01_write/011_tb_user.sql": {
    "has_pk_user": true,
    "has_id_uuid": true,
    "has_identifier": true,
    "foreign_keys": [],
    "compliance": "FULL"
  },
  "examples/blog_api/db/0_schema/02_read/021_user/0211_v_user.sql": {
    "has_id_column": true,
    "has_pk_column": false,
    "jsonb_contains_pk": false,
    "jsonb_fields": ["id", "identifier", "email", "name", "bio", ...],
    "compliance": "FULL"
  }
}
```

## Verification Commands

### Check All Examples Have Basic Structure
```bash
# Verify each example has required files
for dir in examples/*/; do
  echo "Checking $dir"
  [ -f "$dir/README.md" ] && echo "  ✅ README.md" || echo "  ❌ README.md"
  [ -d "$dir/db" ] && echo "  ✅ db/" || echo "  ⚠️  db/"
  [ -f "$dir/app.py" ] || [ -f "$dir/main.py" ] && echo "  ✅ app file" || echo "  ⚠️  app file"
done
```

### Find All SQL Files
```bash
find examples/ -name "*.sql" -type f | wc -l
# Expected: 100-200 SQL files across all examples
```

### Extract SQL Examples from Docs
```bash
# Extract SQL code blocks from concepts-glossary.md
awk '/```sql/,/```/' docs/core/concepts-glossary.md | grep -v '```' > /tmp/doc_sql_examples.sql

# Count examples
grep -c "CREATE TABLE\|CREATE VIEW\|CREATE FUNCTION" /tmp/doc_sql_examples.sql
# Expected: 15-30 SQL examples in docs
```

## Expected Output

### inventory.json
Comprehensive JSON file with:
- All example directories and their files
- All SQL examples in documentation
- Pattern rules extracted from docs
- Compliance status for each file

### discovery-report.md
Human-readable report with:
- Total number of examples
- Total SQL files to verify
- Total documentation examples
- High-level compliance summary
- List of examples that need review

## Acceptance Criteria

- [ ] All examples inventoried (30+ examples)
- [ ] All SQL files cataloged (100+ files)
- [ ] All documentation SQL examples extracted (15+ examples)
- [ ] Pattern rules defined in structured format (20+ rules)
- [ ] Initial compliance scan completed
- [ ] Discovery report generated
- [ ] Ready for Phase 2 (Pattern Extraction)

## DO NOT

- ❌ Do NOT modify any example files yet (read-only phase)
- ❌ Do NOT run SQL migrations (analysis only)
- ❌ Do NOT fix issues found (document for Phase 5)
- ❌ Do NOT skip examples (inventory ALL examples)

## Notes

- Focus on **discovery**, not remediation
- Document **what exists**, not what should exist
- Be **comprehensive** - this inventory guides all future phases
- **Structured data** (JSON) enables automation in later phases
