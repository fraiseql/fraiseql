# Language Generators - Quick Fixes Checklist

## Objective

Get all 5 language generators to production-ready status (runnable tests, working examples).

**Timeline**: Today (5-6 hours total)

---

## TASK 1: Python - Fix Import System (5 minutes)

**Current Status**: 0/3 tests passing (import errors)

### Command

```bash
cd /home/lionel/code/fraiseql
pip install -e fraiseql-python/
```

### Verify

```bash
cd fraiseql-python
python -m pytest tests/ -v
```

### Expected Output

```
test_types.py::test_int_conversion PASSED
test_types.py::test_list_conversion PASSED
test_types.py::test_union_conversion PASSED
test_decorators.py::test_type_decorator PASSED
test_decorators.py::test_query_decorator PASSED
test_analytics.py::test_fact_table PASSED
test_analytics.py::test_aggregate_query PASSED

========================== 7 passed in 0.23s ==========================
```

### Success Criteria

- ✅ All 7 tests pass
- ✅ No ModuleNotFoundError

---

## TASK 2: TypeScript - Fix Decorator Configuration (15 minutes)

**Current Status**: 10/10 registry tests passing, but examples broken

### Step 1: Check Current tsconfig.json

```bash
cat /home/lionel/code/fraiseql/fraiseql-typescript/tsconfig.json
```

### Step 2: Fix tsconfig.json

Add to compilerOptions:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "experimentalDecorators": true,
    "emitDecoratorMetadata": true,
    "moduleResolution": "node",
    "strict": true,
    "outDir": "./dist",
    "rootDir": "./src"
  }
}
```

### Step 3: Update build script in package.json

```json
{
  "scripts": {
    "build": "tsc && node dist/index.js",
    "example:basic": "tsx --experimental-decorators src/examples/basic_schema.ts",
    "example:analytics": "tsx --experimental-decorators src/examples/analytics_schema.ts",
    "test": "jest"
  }
}
```

### Verify

```bash
cd fraiseql-typescript
npm test
npm run example:basic
npm run example:analytics
```

### Expected Output

```
npm test:
  PASS  tests/registry.test.ts
    10 tests PASSED

npm run example:basic:
  {
    "types": [...],
    "queries": [...],
    "mutations": [...]
  }
```

### Success Criteria

- ✅ 10/10 tests still pass
- ✅ Both examples execute without errors
- ✅ JSON schema printed to stdout

---

## TASK 3: Go - Verify Working (5 minutes)

**Current Status**: 45/45 tests passing ✅, examples working ✅

### Verify Tests

```bash
cd /home/lionel/code/fraiseql/fraiseql-go
go test ./fraiseql/... -v
```

### Verify Examples

```bash
go run examples/basic_schema.go 2>/dev/null | head -20
```

### Expected Output

```
go test:
  ok      fraiseql/fraiseql  0.234s
  ✅ All 45 tests passing

go run examples/basic_schema.go:
  {
    "types": [...],
    ...
  }
```

### Success Criteria

- ✅ 45/45 tests still passing
- ✅ Examples generate valid JSON

---

## TASK 4: Java - Install Maven & Run Tests (20 minutes)

**Current Status**: 95% complete, 82 tests designed but can't run

### Step 1: Check if Maven is installed

```bash
which mvn
mvn --version
```

**If not installed:**

```bash
# On Arch Linux:
sudo pacman -S maven

# On other systems:
# - Ubuntu/Debian: sudo apt-get install maven
# - macOS: brew install maven
```

### Step 2: Run Tests

```bash
cd /home/lionel/code/fraiseql/fraiseql-java
mvn clean test
```

### Expected Output

```
[INFO] Running com.fraiseql.core.Phase2Test
[INFO] Tests run: 21, Failures: 0, Errors: 0, Skipped: 0
[INFO] Running com.fraiseql.core.Phase3Test
[INFO] Tests run: 16, Failures: 0, Errors: 0, Skipped: 0
[INFO] Running com.fraiseql.core.Phase4IntegrationTest
[INFO] Tests run: 9, Failures: 0, Errors: 0, Skipped: 0
[INFO] Running com.fraiseql.core.Phase5AdvancedTest
[INFO] Tests run: 17, Failures: 0, Errors: 0, Skipped: 0
[INFO] Running com.fraiseql.core.Phase6OptimizationTest
[INFO] Tests run: 19, Failures: 0, Errors: 0, Skipped: 0

[INFO] BUILD SUCCESS
[INFO] Total tests run: 82, Failures: 0, Errors: 0
```

### Success Criteria

- ✅ 82/82 tests pass
- ✅ BUILD SUCCESS

---

## TASK 5: PHP - Install Composer & Run Tests (15 minutes)

**Current Status**: 90% complete, 12 test classes designed but can't run

### Step 1: Check if Composer is installed

```bash
which composer
composer --version
```

**If not installed:**

```bash
# On Arch Linux:
sudo pacman -S php composer

# On other systems:
# - Ubuntu/Debian: sudo apt-get install composer
# - macOS: brew install composer
# - Or download from: https://getcomposer.org/download/
```

### Step 2: Install Dependencies

```bash
cd /home/lionel/code/fraiseql/fraiseql-php
composer install
```

### Step 3: Run Tests

```bash
vendor/bin/phpunit tests/
```

### Expected Output

```
PHPUnit 11.0.4 by Sebastian Bergmann and contributors.

Runtime: PHP 8.2.x
Configuration: phpunit.xml

..........................................                    40 tests, 0 failures
```

### Success Criteria

- ✅ All 40+ tests pass
- ✅ No failures
- ✅ ~0.5s execution time

---

## TASK 6: CLI Integration Investigation (1-2 hours)

**Current Status**: All generators produce schema.json, but CLI rejects it

### Step 1: Check Schema Format

Go example (most complete):

```bash
cd /home/lionel/code/fraiseql/fraiseql-go
go run examples/basic_schema.go > /tmp/go_schema.json
cat /tmp/go_schema.json | jq . | head -40
```

### Step 2: Try CLI Compilation

```bash
fraiseql-cli compile /tmp/go_schema.json
```

### Step 3: Diagnose Error

If compilation fails:

- [ ] Check fraiseql-cli schema parser code
- [ ] Compare generated format vs expected format
- [ ] Review compiler error message carefully

### Step 4: Fix Generation or CLI

Options:

- A) Adjust schema generators to match CLI expectations
- B) Fix CLI compiler to accept generator output
- C) Add schema transformation layer

---

## EXECUTION PLAN

### Time Block 1: Python & TypeScript (20 minutes)

```bash
# Terminal 1: Python
cd /home/lionel/code/fraiseql
pip install -e fraiseql-python/
cd fraiseql-python
python -m pytest tests/ -v

# Terminal 2: TypeScript (parallel)
cd /home/lionel/code/fraiseql/fraiseql-typescript
# Edit tsconfig.json (add experimentalDecorators, emitDecoratorMetadata)
npm test
npm run example:basic
```

### Time Block 2: Go & Java (25 minutes)

```bash
# Terminal 1: Go
cd /home/lionel/code/fraiseql/fraiseql-go
go test ./fraiseql/... -v

# Terminal 2: Java (parallel)
# Install Maven if needed
sudo pacman -S maven  # Arch Linux
cd /home/lionel/code/fraiseql/fraiseql-java
mvn clean test
```

### Time Block 3: PHP (15 minutes)

```bash
cd /home/lionel/code/fraiseql/fraiseql-php
composer install
vendor/bin/phpunit tests/
```

### Time Block 4: CLI Investigation (1-2 hours)

```bash
# After all languages verified:
fraiseql-cli compile /tmp/go_schema.json
# Debug based on error message
```

---

## Verification Checklist

### After All Tasks Complete

- [ ] **Python**
  - [ ] 7/7 tests passing
  - [ ] No import errors

- [ ] **TypeScript**
  - [ ] 10/10 registry tests passing
  - [ ] `npm run example:basic` runs successfully
  - [ ] `npm run example:analytics` runs successfully

- [ ] **Go**
  - [ ] 45/45 tests passing (still)
  - [ ] Examples generate valid JSON

- [ ] **Java**
  - [ ] 82/82 tests passing
  - [ ] Maven build successful

- [ ] **PHP**
  - [ ] All 40+ tests passing
  - [ ] No errors in output

- [ ] **CLI Integration**
  - [ ] At minimum: Identified schema format issue
  - [ ] At maximum: All languages compile successfully

---

## Quick Command Reference

```bash
# Python
pip install -e fraiseql-python/
cd fraiseql-python && python -m pytest tests/ -v

# TypeScript
cd fraiseql-typescript && npm test && npm run example:basic

# Go
cd fraiseql-go && go test ./fraiseql/... -v

# Java
cd fraiseql-java && mvn clean test

# PHP
cd fraiseql-php && composer install && vendor/bin/phpunit tests/

# CLI Test
fraiseql-cli compile /tmp/schema.json
```

---

**Estimated Total Time**: 5-6 hours
**Expected Success Rate**: 95%+ (all tasks are environmental/config fixes, not code fixes)

---

Last Updated: January 16, 2026
