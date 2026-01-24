# Quick Reference - Sequential Plan

**Use this as your checklist while executing the plan**

---

## PHASE 1: QUICK FIXES (2.5 hours total)

### Task 1.1: Python (5 min) ⏱️

```bash
cd /home/lionel/code/fraiseql
pip install -e fraiseql-python/
cd fraiseql-python && python -m pytest tests/ -v
```

✅ **Success**: 7 tests passing
➡️ **Next**: Task 1.2

### Task 1.2: TypeScript (15 min) ⏱️

```bash
cd /home/lionel/code/fraiseql/fraiseql-typescript
# Edit tsconfig.json - add experimentalDecorators + emitDecoratorMetadata
npm test
npm run example:basic
npm run example:analytics
```

✅ **Success**: 10 tests passing, 2 examples work
➡️ **Next**: Task 1.3

### Task 1.3: Java (10 min) ⏱️

```bash
sudo pacman -S maven
cd /home/lionel/code/fraiseql/fraiseql-java
mvn test
```

✅ **Success**: 82 tests passing
➡️ **Next**: Task 1.4

### Task 1.4: PHP (5 min) ⏱️

```bash
sudo pacman -S composer
cd /home/lionel/code/fraiseql/fraiseql-php
composer install
vendor/bin/phpunit tests/ -v
```

✅ **Success**: 40+ tests passing
➡️ **Next**: Task 1.5

### Task 1.5: Go (5 min) ⏱️

```bash
cd /home/lionel/code/fraiseql/fraiseql-go
go test ./fraiseql/... -v
go run examples/basic_schema.go > /tmp/go_schema.json
```

✅ **Success**: 45 tests passing
➡️ **Next**: Task 1.6

### Task 1.6: CLI Investigation (2 hours max) ⏱️

```bash
# Generate schemas
python -c "import fraiseql; fraiseql.export_schema('/tmp/python_schema.json')"
cd fraiseql-go && go run examples/basic_schema.go > /tmp/go_schema.json
cd fraiseql-typescript && npm run example:basic > /tmp/typescript_schema.json

# Try CLI
fraiseql-cli compile /tmp/go_schema.json

# Document findings
cat > /tmp/cli_investigation_findings.md << 'EOF'
# Findings
...
EOF
```

✅ **Success**: Findings documented (even if not fully resolved)
➡️ **Next**: PHASE 2

---

## PHASE 2: E2E INFRASTRUCTURE (9 hours total)

### Task 2.1a: Python E2E (1 hour) ⏱️

```bash
mkdir -p /home/lionel/code/fraiseql/tests/e2e
# Copy Python E2E test code from E2E_TESTING_STRATEGY.md
# Create: tests/e2e/python_e2e_test.py
python -m pytest tests/e2e/python_e2e_test.py -v --collect-only
```

✅ **Success**: File created, no syntax errors
➡️ **Next**: Task 2.1b

### Task 2.1b: TypeScript E2E (1 hour) ⏱️

```bash
mkdir -p /home/lionel/code/fraiseql/fraiseql-typescript/tests/e2e
# Copy TypeScript E2E test code from E2E_TESTING_STRATEGY.md
# Create: fraiseql-typescript/tests/e2e/e2e.test.ts
npm test -- --listTests
```

✅ **Success**: File created, no syntax errors
➡️ **Next**: Task 2.1c

### Task 2.1c: Java E2E (1 hour) ⏱️

```bash
mkdir -p /home/lionel/code/fraiseql/fraiseql-java/src/test/java/com/fraiseql
# Copy Java E2E test code from E2E_TESTING_STRATEGY.md
# Create: fraiseql-java/src/test/java/com/fraiseql/E2ETest.java
mvn test -Dtest="E2ETest" --collect-only
```

✅ **Success**: File created, no syntax errors
➡️ **Next**: Task 2.1d

### Task 2.1d: Go E2E (1 hour) ⏱️

```bash
# Copy Go E2E test code from E2E_TESTING_STRATEGY.md
# Create: fraiseql-go/fraiseql/e2e_test.go
go test ./fraiseql/... -run TestE2E -v --collect-only
```

✅ **Success**: File created, no syntax errors
➡️ **Next**: Task 2.1e

### Task 2.1e: PHP E2E (1 hour) ⏱️

```bash
mkdir -p /home/lionel/code/fraiseql/fraiseql-php/tests/e2e
# Copy PHP E2E test code from E2E_TESTING_STRATEGY.md
# Create: fraiseql-php/tests/e2e/E2ETest.php
vendor/bin/phpunit tests/e2e/ --list-tests
```

✅ **Success**: File created, no syntax errors
➡️ **Next**: Task 2.2

### Task 2.2: Makefile (2 hours) ⏱️

```bash
cd /home/lionel/code/fraiseql
# Append E2E targets to Makefile (copy from IMPLEMENTATION_PLAN_SEQUENTIAL.md)
# Should have: e2e-setup, e2e-all, e2e-python, e2e-typescript, e2e-java, e2e-go, e2e-php, e2e-clean, e2e-status
make -n e2e-setup
grep "e2e-all" Makefile
```

✅ **Success**: Makefile has all E2E targets
➡️ **Next**: Task 2.3

### Task 2.3: GitHub Actions (3 hours) ⏱️

```bash
mkdir -p /home/lionel/code/fraiseql/.github/workflows
# Copy GitHub Actions workflow from E2E_TESTING_STRATEGY.md
# Create: .github/workflows/e2e-tests.yml
git add .github/workflows/e2e-tests.yml
git commit -m "ci: Add E2E testing workflow"
git push origin feature/phase-1-foundation
```

✅ **Success**: Workflow committed and pushed to GitHub
➡️ **Next**: PHASE 3

---

## PHASE 3: CLI INTEGRATION FIX (2.25 hours total)

### Task 3.1: Analyze (30 min) ⏱️

```bash
cat /tmp/cli_investigation_findings.md
# Review findings and decide: Option A/B/C
cat > /tmp/fix_strategy.md << 'EOF'
# Fix Strategy
- Problem: [...]
- Solution: [...]
- Files to modify: [...]
EOF
```

✅ **Success**: Fix strategy documented
➡️ **Next**: Task 3.2

### Task 3.2: Implement (45 min) ⏱️

```bash
# Edit files based on your fix strategy
# Run affected tests to verify
cd fraiseql-python && python -m pytest tests/ -q
cd fraiseql-typescript && npm test -q
# etc.
```

✅ **Success**: Changes made and unit tests passing
➡️ **Next**: Task 3.3

### Task 3.3: Verify (30 min) ⏱️

```bash
# Generate fresh schemas
python -c "from fraiseql import schema; schema.export_schema('/tmp/py_fix_test.json')"
# Compile with CLI
for schema in /tmp/*_fix_test.json; do
  fraiseql-cli compile "$schema" > /dev/null 2>&1 && echo "✅ $(basename $schema)" || echo "❌ $(basename $schema)"
done
ls -lh schema.compiled.json
```

✅ **Success**: All schemas compile
➡️ **Next**: Task 3.4

### Task 3.4: Update Tests (30 min) ⏱️

```bash
# Create CLI schema format documentation
cat > /home/lionel/code/fraiseql/docs/cli-schema-format.md << 'EOF'
# CLI Schema Format
...
EOF
```

✅ **Success**: CLI documentation created
➡️ **Next**: PHASE 4

---

## PHASE 4: DOCUMENTATION (2.5 hours total)

### Task 4.1: README (30 min) ⏱️

```bash
# Edit README.md
# Add Language Generators section with table
# Add Quick Example (Python)
grep "Language Generators" README.md
```

✅ **Success**: README updated with language info
➡️ **Next**: Task 4.2

### Task 4.2: Language Generators Docs (45 min) ⏱️

```bash
# Create docs/language-generators.md
# Include: All 5 languages, how it works, testing, contributing
[ -f docs/language-generators.md ] && echo "✅ Created"
```

✅ **Success**: Language generators docs created
➡️ **Next**: Task 4.3

### Task 4.3: E2E Testing Docs (45 min) ⏱️

```bash
# Create docs/e2e-testing.md
# Include: Quick start, individual tests, infrastructure, troubleshooting
[ -f docs/e2e-testing.md ] && echo "✅ Created"
```

✅ **Success**: E2E testing docs created
➡️ **Next**: Task 4.4

### Task 4.4: Final Commit (30 min) ⏱️

```bash
cd /home/lionel/code/fraiseql
git add -A
git status
git commit -m "feat: Complete language generators E2E testing infrastructure..."
git push origin feature/phase-1-foundation
```

✅ **Success**: All changes committed and pushed
➡️ **Next**: VERIFY

---

## FINAL VERIFICATION

```bash
cd /home/lionel/code/fraiseql

# Phase 1
echo "PHASE 1:"
cd fraiseql-python && python -m pytest tests/ -q 2>&1 | tail -1 && cd ..
cd fraiseql-typescript && npm test 2>&1 | grep -E "passed" && cd ..
cd fraiseql-java && mvn test -q 2>&1 | tail -1 && cd ..
cd fraiseql-php && vendor/bin/phpunit tests/ -q 2>&1 | tail -1 && cd ..
cd fraiseql-go && go test ./fraiseql/... -q 2>&1 | tail -1 && cd ..

# Phase 2
echo "PHASE 2:"
[ -f tests/e2e/python_e2e_test.py ] && echo "✅ Python E2E"
[ -f fraiseql-typescript/tests/e2e/e2e.test.ts ] && echo "✅ TypeScript E2E"
[ -f fraiseql-java/src/test/java/com/fraiseql/E2ETest.java ] && echo "✅ Java E2E"
[ -f fraiseql-go/fraiseql/e2e_test.go ] && echo "✅ Go E2E"
[ -f fraiseql-php/tests/e2e/E2ETest.php ] && echo "✅ PHP E2E"
grep -q "e2e-all" Makefile && echo "✅ Makefile"
[ -f .github/workflows/e2e-tests.yml ] && echo "✅ GitHub Actions"

# Phase 3
echo "PHASE 3:"
fraiseql-cli compile /tmp/go_fix_test.json > /dev/null 2>&1 && echo "✅ CLI compiles"
[ -f docs/cli-schema-format.md ] && echo "✅ CLI docs"

# Phase 4
echo "PHASE 4:"
grep -q "Language Generators" README.md && echo "✅ README"
[ -f docs/language-generators.md ] && echo "✅ Language generators docs"
[ -f docs/e2e-testing.md ] && echo "✅ E2E docs"

echo ""
echo "=== ALL PHASES VERIFIED ==="
```

---

## Timeline Reference

| Phase | Duration | Start | End |
|-------|----------|-------|-----|
| **Phase 1** | 2.5 hrs | Day 1 AM | Day 1 PM |
| **Phase 2.1** | 5 hrs | Day 2 AM | Day 2 PM |
| **Phase 2.2-2.3** | 5 hrs | Day 2 PM | Day 3 AM |
| **Phase 3** | 2.25 hrs | Day 3 PM | Day 3 Evening |
| **Phase 4** | 2.5 hrs | Day 4 AM | Day 4 PM |
| **TOTAL** | **16.25 hrs** | Day 1 | Day 4 |

---

## Troubleshooting Quick Links

| Issue | Solution |
|-------|----------|
| Python import error | `pip install -e fraiseql-python/` |
| TypeScript decorator error | Add `experimentalDecorators: true` to tsconfig.json |
| Maven not installed | `sudo pacman -S maven` |
| Composer not installed | `sudo pacman -S composer` |
| CLI not found | Install fraiseql-cli from crates/ |
| Tests timeout | Check that databases are running |
| Git push fails | Verify you're on `feature/phase-1-foundation` branch |

---

**Print this page or keep it open while executing the plan**

Good luck! 🚀
