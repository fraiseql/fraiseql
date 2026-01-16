# CLI Schema Format Fix - Implementation Plan

## Overview

Fix the schema format mismatch that prevents all 10 language generators from compiling with fraiseql-cli.

**Root Issue**: `"return_list"` → must be `"returns_list"`

**Scope**: 11 files total
- 1 canonical schema file (velocitybench_schemas.py)
- 10 language generators (Python, TypeScript, Go, Java, PHP, Kotlin, C#, Rust, JavaScript, Ruby)

**Time Estimate**: 2-3 hours for full diagnosis and fix

---

## Phase 1: Diagnosis (30 minutes)

### Task 1.1: Locate all generator files

```bash
# Find all language generators
find . -type f -name "*.py" -o -name "*.ts" -o -name "*.go" -o -name "*.java" \
  -o -name "*.php" -o -name "*.kt" -o -name "*.cs" -o -name "*.rs" \
  -o -name "*.js" -o -name "*.rb" | grep -E "(gen|build|schema)" | head -20
```

**Expected locations:**
- `fraiseql-python/fraiseql/schema.py` or `tests/e2e/velocitybench_schemas.py`
- `fraiseql-typescript/src/schema.ts` or similar
- `fraisier/langgen/*/generator.*`

### Task 1.2: Search for `return_list` occurrences

```bash
# Find all "return_list" in codebase
grep -r "return_list" --include="*.py" --include="*.ts" --include="*.go" \
  --include="*.java" --include="*.php" --include="*.kt" --include="*.cs" \
  --include="*.rs" --include="*.js" --include="*.rb" .
```

**Expected**: ~50-100 occurrences across all files

### Task 1.3: Verify this is the ONLY issue

After fixing `return_list`, test one language:
```bash
python3 -c "
from tests.e2e.velocitybench_schemas import get_velocitybench_schema
import json, tempfile, subprocess
from pathlib import Path

schema = get_velocitybench_schema()
with tempfile.TemporaryDirectory() as tmpdir:
    path = Path(tmpdir) / 'test.json'
    with open(path, 'w') as f:
        json.dump(schema, f)
    result = subprocess.run(['./target/release/fraiseql-cli', 'compile', str(path), '-o', '/tmp/out.json'], capture_output=True, text=True)
    print('STDERR:', result.stderr)
    print('Return code:', result.returncode)
"
```

If still failing after fix, investigate further. Document any additional issues.

---

## Phase 2: Fix Canonical Schema (15 minutes)

### Task 2.1: Update velocitybench_schemas.py

**File**: `tests/e2e/velocitybench_schemas.py`

**Change**: All 8 occurrences of `"return_list"` → `"returns_list"`

Search pattern: `"return_list":`

Locations:
- Line ~88 (ping query)
- Line ~109 (users query)
- Line ~131 (posts query)
- Line ~151 (comments query)
- Line ~212 (other queries)

**Verification**:
```python
# After fix, check schema is still valid
from tests.e2e.velocitybench_schemas import get_velocitybench_schema
schema = get_velocitybench_schema()
assert "returns_list" in str(schema.get("queries", [{}])[0])
print("✅ Schema format corrected")
```

---

## Phase 3: Fix All 10 Language Generators (1.5-2 hours)

### Task 3.1: Python Generator

**Location**: Find where Python generates schema JSON
- Likely: `fraiseql-python/fraiseql/schema.py` or similar
- Look for: Function that returns query/mutation dicts

**Change**: All occurrences of `"return_list"` → `"returns_list"`

**Verification**:
```bash
python3 -c "from tests.e2e.velocitybench_schemas import get_python_schema_code; \
  code = get_python_schema_code(); print('return_list' in code)"
# Should print: False (after fix)
```

### Task 3.2: TypeScript Generator

**Location**: Find where TypeScript generates schema JSON
- Likely: `fraiseql-typescript/src/schema.ts` or in test examples
- Look for: Functions returning query/mutation objects

**Change**: All occurrences of `"return_list"` → `"returns_list"`

### Task 3.3: Go Generator

**Location**: `fraisier/langgen/golang/generator.go` or similar
- Look for: String literals with `"return_list"`
- May use struct field mapping

**Change**: Update all string literals

### Task 3.4: Java Generator

**Location**: `fraisier/langgen/java/generator.java` or similar
- Look for: String constants or JSON building code

**Change**: Update all string references

### Task 3.5: PHP Generator

**Location**: `fraisier/langgen/php/generator.php` or similar
- Look for: Array keys or string literals

**Change**: Update all keys

### Task 3.6: Kotlin Generator

**Location**: `fraisier/langgen/kotlin/generator.kt` or similar
- Look for: JSON building or string literals

**Change**: Update all occurrences

### Task 3.7: C# Generator

**Location**: `fraisier/langgen/csharp/generator.cs` or similar
- Look for: String literals or property names

**Change**: Update all occurrences

### Task 3.8: Rust Generator

**Location**: `fraisier/langgen/rust/generator.rs` or similar
- Look for: JSON or map building code

**Change**: Update all occurrences

### Task 3.9: JavaScript Generator

**Location**: `fraisier/langgen/javascript/generator.js` or similar
- Look for: Object property keys or JSON strings

**Change**: Update all occurrences

### Task 3.10: Ruby Generator

**Location**: `fraisier/langgen/ruby/generator.rb` or similar
- Look for: Symbol keys or string literals

**Change**: Update all occurrences

---

## Phase 4: Verification (30 minutes)

### Task 4.1: Quick Single-Language Test

```bash
# Test with Python only
python3 -c "
from tests.e2e.velocitybench_schemas import get_velocitybench_schema
import json, tempfile, subprocess
from pathlib import Path

schema = get_velocitybench_schema()
with tempfile.TemporaryDirectory() as tmpdir:
    path = Path(tmpdir) / 'test.json'
    with open(path, 'w') as f:
        json.dump(schema, f, indent=2)
    result = subprocess.run(['./target/release/fraiseql-cli', 'compile', str(path), '-o', str(Path(tmpdir) / 'out.json')], capture_output=True, text=True)
    if result.returncode == 0:
        print('✅ CLI compilation successful')
        with open(Path(tmpdir) / 'out.json') as f:
            compiled = json.load(f)
            print(f'✅ Output schema valid: {len(compiled.get(\"types\", []))} types')
    else:
        print('❌ CLI compilation failed')
        print('STDERR:', result.stderr)
"
```

**Expected**: `✅ CLI compilation successful`

### Task 4.2: Full Multi-Language E2E Test

```bash
python3 tests/e2e/velocitybench_compilation_test.py
```

**Expected Output**:
```
======================================================================
Phase 2: CLI Compilation E2E Test
======================================================================

Compiling Python       (Python decorators             )... ✅ schema.compiled.json
Compiling TypeScript   (TypeScript decorators         )... ✅ schema.compiled.json
Compiling Go           (Go struct tags                )... ✅ schema.compiled.json
Compiling Java         (Java annotations              )... ✅ schema.compiled.json
Compiling PHP          (PHP attributes                )... ✅ schema.compiled.json
Compiling Kotlin       (Kotlin data classes           )... ✅ schema.compiled.json
Compiling CSharp       (C# records                    )... ✅ schema.compiled.json
Compiling Rust         (Rust macros                   )... ✅ schema.compiled.json
Compiling JavaScript   (JavaScript decorators         )... ✅ schema.compiled.json
Compiling Ruby         (Ruby DSL                      )... ✅ schema.compiled.json

======================================================================
✅ ALL TIER 1A COMPILATION E2E TESTS PASSED!
✅ All 10 languages compile to identical canonical schema.compiled.json
======================================================================
```

### Task 4.3: Semantic Equivalence Check

```python
# Verify all 10 compiled schemas are IDENTICAL
from tests.e2e.velocitybench_compilation_test import compile_schema, get_velocitybench_schema
import json

schema = get_velocitybench_schema()
languages = ["Python", "TypeScript", "Go", "Java", "PHP", "Kotlin", "CSharp", "Rust", "JavaScript", "Ruby"]

compiled_schemas = {}
for lang in languages:
    compiled = compile_schema(schema, lang)
    if compiled:
        # Serialize to string for comparison
        compiled_schemas[lang] = json.dumps(compiled, sort_keys=True)

# Check all are identical
first_compiled = list(compiled_schemas.values())[0]
all_identical = all(s == first_compiled for s in compiled_schemas.values())

if all_identical:
    print("✅ ALL 10 LANGUAGES PRODUCE IDENTICAL COMPILED SCHEMA")
else:
    print("❌ Languages produce different compiled schemas")
    for lang, compiled in compiled_schemas.items():
        print(f"  {lang}: {len(compiled)} chars")
```

---

## Troubleshooting

### If CLI still fails after fixing `return_list`

1. **Check other field names**:
   - Ensure arguments have `"nullable"` field (not `"optional"`)
   - Ensure fields use `"type"` not `"field_type"`
   - Ensure mutations have `"operation"` field if specified

2. **Debug with verbose output**:
   ```bash
   ./target/release/fraiseql-cli compile schema.json -o out.json --debug
   ```

3. **Run CLI tests**:
   ```bash
   cargo test -p fraiseql-cli schema::tests
   ```

4. **Check error messages**:
   - Look at `crates/fraiseql-cli/src/schema/validator.rs` for specific validation rules
   - May reveal additional format requirements

### If one language produces different output

1. **Compare JSON structures**:
   ```bash
   diff -u <(jq -S . compiled_python.json) <(jq -S . compiled_typescript.json)
   ```

2. **Check if generator has language-specific behavior**
   - Each generator should produce IDENTICAL schema JSON
   - Any differences indicate generator bug

3. **Regenerate that language's code**
   - May need to re-run its code generator
   - Check for any conditional logic based on language

---

## Success Criteria

All of the following must pass:

- [ ] Phase 1: Diagnosis complete, root cause confirmed as `return_list` mismatch
- [ ] Phase 2: velocitybench_schemas.py fixed (all 8 occurrences)
- [ ] Phase 3: All 10 language generators fixed
- [ ] Phase 4.1: Single-language test passes
- [ ] Phase 4.2: Full E2E test shows all 10 languages compile ✅
- [ ] Phase 4.3: All 10 compiled schemas are bit-identical
- [ ] No other format issues discovered
- [ ] Test output shows: `✅ ALL TIER 1A COMPILATION E2E TESTS PASSED!`
- [ ] Commit created with descriptive message

---

## Next Steps After Fix

1. **Commit Changes**:
   ```bash
   git add -A
   git commit -m "fix(schema): Normalize schema field names for CLI compatibility"
   ```

2. **Update Documentation**:
   - Update `.claude/INTERMEDIATE_SCHEMA_FORMAT.md` if it exists
   - Document the correct schema format
   - Add examples for each language

3. **Run Full Test Suite**:
   ```bash
   cargo test
   python3 -m pytest tests/
   ```

4. **Move to Next Phase**:
   - Now that Phase 2 E2E tests pass, can proceed with:
   - Phase 3: Actual SQL compilation (if implementing)
   - Phase 4: Query execution testing
   - Or continue with other implementation phases

---

## Estimated Timeline

| Phase | Task | Time |
|-------|------|------|
| 1 | Diagnosis | 30 min |
| 2 | Fix canonical schema | 15 min |
| 3 | Fix 10 generators | 90 min (9 min each) |
| 4 | Verification & testing | 30 min |
| **Total** | | **165 min (2.75 hours)** |

**Parallel Work**: Phases 3 tasks can be done in parallel by different people/local models
