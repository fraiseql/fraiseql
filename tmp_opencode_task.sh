#!/bin/bash

opencode run -m xai/grok-code-fast-1 "
TASK: Scan all test files and create comprehensive inventory

CONTEXT:
- Project: FraiseQL (GraphQL framework with PostgreSQL)
- Language: Python 3.10+
- Testing: pytest
- Working directory: /home/lionel/code/fraiseql
- Test directories: tests/integration/, tests/regression/, tests/unit/, tests/storage/, tests/utils/

OBJECTIVE:
Create a comprehensive JSON inventory of all test files, including:
- All test files and their locations
- Total count of tests per file
- Skipped tests with their skip reasons
- Test metadata (classes, fixtures used, imports)

FILES TO CREATE:
- /tmp/audit_phase1_inventory.json - Complete test inventory

IMPLEMENTATION STEPS:
1. Find all test files: find tests -name 'test_*.py' -type f | sort
2. For each test file, extract:
   - File path
   - Test functions (def test_*) and test methods (class Test*: def test_*)
   - Skipped tests (@pytest.mark.skip, @pytest.mark.skipif, pytest.skip())
   - Skip reasons from decorators and docstrings
   - Test classes and their methods
   - Fixture usage (@pytest.fixture, fixture parameters)
   - Key imports (pytest, fastapi, graphql, etc.)
3. Parse Python AST to extract test information accurately
4. Count totals: total tests, skipped tests, test files
5. Output structured JSON with all findings

VERIFICATION:
- /tmp/audit_phase1_inventory.json should exist
- JSON should be valid and parseable
- Should include counts: total_files, total_tests, total_skipped
- Each test entry should have: file, name, type (function/method), skipped (bool), skip_reason

ACCEPTANCE CRITERIA:
- [ ] All test files in tests/ directory analyzed
- [ ] Skipped tests identified with reasons
- [ ] JSON output is well-structured and complete
- [ ] Statistics are accurate

DO NOT:
- Modify any test files
- Skip any test directories
- Make assumptions - parse actual code

COMPLETION SIGNAL:
When done, write your status to: /tmp/opencode-228391c0-7584-4959-968c-fd5697aa52d2.marker
- On success: echo 'SUCCESS' > /tmp/opencode-228391c0-7584-4959-968c-fd5697aa52d2.marker
- On failure: echo 'FAILURE:<reason>' > /tmp/opencode-228391c0-7584-4959-968c-fd5697aa52d2.marker
This marker file is REQUIRED - do not skip this step.
" &

while true; do
  if [ -f /tmp/opencode-228391c0-7584-4959-968c-fd5697aa52d2.marker ]; then
    cat /tmp/opencode-228391c0-7584-4959-968c-fd5697aa52d2.marker
    rm -f /tmp/opencode-228391c0-7584-4959-968c-fd5697aa52d2.marker
    break
  fi
  sleep 5
done
