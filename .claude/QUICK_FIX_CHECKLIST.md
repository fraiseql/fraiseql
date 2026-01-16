# Quick Fix Checklist - Return List Field Renaming

## The Change
Replace all `"return_list"` with `"returns_list"` in 11 files.

## Canonical Schema

### File 1: tests/e2e/velocitybench_schemas.py
```bash
# Command to find all occurrences
grep -n "return_list" tests/e2e/velocitybench_schemas.py

# Expected output: ~8 lines
```

**Occurrences to fix**:
- [ ] Line ~88: `"return_list": False` (ping query)
- [ ] Line ~109: `"return_list": True` (users query)
- [ ] Line ~131: `"return_list": True` (posts query)
- [ ] Line ~151: `"return_list": True` (comments query)
- [ ] Line ~212: `"return_list": False` (other queries)
- [ ] Plus any others found by grep

**Verification after fix**:
```bash
python3 -c "
from tests.e2e.velocitybench_schemas import get_velocitybench_schema
import json
schema = get_velocitybench_schema()
has_return_list = 'return_list' in json.dumps(schema)
has_returns_list = 'returns_list' in json.dumps(schema)
print(f'return_list present: {has_return_list}')
print(f'returns_list present: {has_returns_list}')
assert not has_return_list, 'Still has return_list!'
assert has_returns_list, 'Missing returns_list!'
"
```

---

## Language Generators

### File 2: Python Generator
```bash
# Find Python generator
find . -path ./target -prune -o -name "*.py" -type f -exec grep -l "return_list" {} \; | grep -v test

# Expected: Generator file or schema output function
```

**Task**: Replace all `"return_list"` with `"returns_list"`

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with:
  ```bash
  grep "return_list" [generator_file]  # Should return nothing
  ```

---

### File 3: TypeScript Generator
```bash
# Find TypeScript generator
find . -path ./target -prune -o -name "*.ts" -type f -exec grep -l "return_list" {} \;
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

### File 4: Go Generator
```bash
# Find Go generator
find . -path ./target -prune -o -name "*.go" -type f -exec grep -l "return_list" {} \;
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

### File 5: Java Generator
```bash
# Find Java generator
find . -path ./target -prune -o -name "*.java" -type f -exec grep -l "return_list" {} \;
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

### File 6: PHP Generator
```bash
# Find PHP generator
find . -path ./target -prune -o -name "*.php" -type f -exec grep -l "return_list" {} \;
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

### File 7: Kotlin Generator
```bash
# Find Kotlin generator
find . -path ./target -prune -o -name "*.kt" -type f -exec grep -l "return_list" {} \;
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

### File 8: C# Generator
```bash
# Find C# generator
find . -path ./target -prune -o -name "*.cs" -type f -exec grep -l "return_list" {} \;
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

### File 9: Rust Generator
```bash
# Find Rust generator (in fraisier, not fraiseql-core)
find . -path ./target -prune -o -name "*.rs" -type f -exec grep -l "return_list" {} \; | grep fraisier
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

### File 10: JavaScript Generator
```bash
# Find JavaScript generator
find . -path ./target -prune -o -name "*.js" -type f -exec grep -l "return_list" {} \; | grep -v node_modules
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

### File 11: Ruby Generator
```bash
# Find Ruby generator
find . -path ./target -prune -o -name "*.rb" -type f -exec grep -l "return_list" {} \;
```

- [ ] Located generator file: `________________`
- [ ] Fixed all `return_list` occurrences
- [ ] Verified fix with: `grep "return_list" [generator_file]`

---

## Final Verification

### Step 1: Check for remaining issues
```bash
grep -r "return_list" . --include="*.py" --include="*.ts" --include="*.go" \
  --include="*.java" --include="*.php" --include="*.kt" --include="*.cs" \
  --include="*.rs" --include="*.js" --include="*.rb" 2>/dev/null | grep -v target | grep -v ".git"

# Expected output: NOTHING (empty result)
```

- [ ] No remaining `return_list` found

### Step 2: Test with CLI
```bash
python3 -c "
from tests.e2e.velocitybench_schemas import get_velocitybench_schema
import json, tempfile, subprocess
from pathlib import Path

schema = get_velocitybench_schema()
with tempfile.TemporaryDirectory() as tmpdir:
    path = Path(tmpdir) / 'test.json'
    with open(path, 'w') as f:
        json.dump(schema, f, indent=2)
    result = subprocess.run(
        ['./target/release/fraiseql-cli', 'compile', str(path), '-o', str(Path(tmpdir) / 'out.json')],
        capture_output=True,
        text=True
    )
    if result.returncode == 0:
        print('✅ CLI compilation successful!')
        import json
        with open(Path(tmpdir) / 'out.json') as f:
            compiled = json.load(f)
            print(f'✅ Types: {len(compiled.get(\"types\", []))}')
            print(f'✅ Queries: {len(compiled.get(\"queries\", []))}')
            print(f'✅ Mutations: {len(compiled.get(\"mutations\", []))}')
    else:
        print('❌ CLI compilation failed')
        print('STDERR:', result.stderr)
"
```

- [ ] CLI successfully compiles schema
- [ ] Output contains expected types, queries, mutations

### Step 3: Run full E2E test
```bash
python3 tests/e2e/velocitybench_compilation_test.py
```

Expected output includes:
```
Compiling Python       ... ✅
Compiling TypeScript   ... ✅
Compiling Go           ... ✅
Compiling Java         ... ✅
Compiling PHP          ... ✅
Compiling Kotlin       ... ✅
Compiling CSharp       ... ✅
Compiling Rust         ... ✅
Compiling JavaScript   ... ✅
Compiling Ruby         ... ✅

✅ ALL TIER 1A COMPILATION E2E TESTS PASSED!
```

- [ ] All 10 languages compile successfully
- [ ] All E2E tests pass

---

## Commit

```bash
git add -A
git commit -m "fix(schema): Normalize schema field names for CLI compatibility

## Changes
- Fix 'return_list' → 'returns_list' in velocitybench_schemas.py
- Fix 'return_list' → 'returns_list' in all 10 language generators:
  - Python, TypeScript, Go, Java, PHP
  - Kotlin, C#, Rust, JavaScript, Ruby

## Verification
✅ All 10 languages now produce CLI-compatible schema format
✅ CLI compilation succeeds for all 10 languages
✅ All compiled schemas are identical (semantic equivalence proven)
✅ velocitybench_compilation_test.py Phase 2 passes completely
"
```

- [ ] Changes committed

---

## Summary

| # | File | Status | Notes |
|---|------|--------|-------|
| 1 | tests/e2e/velocitybench_schemas.py | [ ] | Canonical schema (8 changes) |
| 2 | Python generator | [ ] | |
| 3 | TypeScript generator | [ ] | |
| 4 | Go generator | [ ] | |
| 5 | Java generator | [ ] | |
| 6 | PHP generator | [ ] | |
| 7 | Kotlin generator | [ ] | |
| 8 | C# generator | [ ] | |
| 9 | Rust generator | [ ] | |
| 10 | JavaScript generator | [ ] | |
| 11 | Ruby generator | [ ] | |

**Total fixes needed**: ~50-60 field occurrences across 11 files
**Time estimate**: 1-2 hours (30 min with grep + replace tools, 30 min testing/verification)

---

## Troubleshooting

### If CLI still fails:
1. Check for typos: Must be exactly `"returns_list"` (not `"returns_List"` or `"returnslist"`)
2. Check for quotes: Must be double quotes `"` not single quotes `'`
3. Check for case sensitivity: Field names are case-sensitive
4. Look for any arguments with `"return_list"` (should never happen, but check)
5. Review full error message: `RUST_LOG=debug ./target/release/fraiseql-cli compile schema.json`

### If one language produces different output:
1. Check if it generated from correct schema
2. Verify both use `"returns_list"` not `"return_list"`
3. Compare with working language using: `diff -u compiled1.json compiled2.json`

---

## Reference

- **Analysis**: `.claude/CLI_SCHEMA_FORMAT_ANALYSIS.md`
- **Plan**: `.claude/CLI_SCHEMA_FIX_IMPLEMENTATION_PLAN.md`
- **Summary**: `.claude/CLI_SCHEMA_FIX_SUMMARY.md`
- **This checklist**: `.claude/QUICK_FIX_CHECKLIST.md`

---

Good luck! 🚀
