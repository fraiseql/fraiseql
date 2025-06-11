# TestFoundry Tests

This directory contains tests for the TestFoundry extension, an automated test generation framework for PostgreSQL databases that integrates with FraiseQL.

## Test Structure

- `test_analyzer.py` - Tests for the FraiseQL type analyzer that extracts metadata
- `test_generator.py` - Tests for the test generation functionality
- `test_setup.py` - Tests for TestFoundry installation and setup
- `test_pgtap_structure.py` - Tests that verify the structure of generated pgTAP tests
- `test_pgtap_execution.py` - Tests that execute pgTAP tests with a minimal pgTAP implementation
- `install_pgtap.py` - Minimal pgTAP implementation for testing

## pgTAP Installation

The tests use a minimal pgTAP implementation (`install_pgtap.py`) that provides the essential pgTAP functions needed for testing:

- `plan(n)` - Declare the number of tests
- `finish()` - Complete the test run
- `ok(boolean, text)` - Basic assertion
- `is(actual, expected, text)` - Equality assertion
- `isnt(actual, expected, text)` - Inequality assertion
- `like(text, pattern, text)` - Pattern matching assertion
- `lives_ok(sql, text)` - Test that SQL executes without error
- `throws_ok(sql, errcode, errmsg, text)` - Test that SQL throws expected error
- `pass(text)` - Always pass
- `fail(text)` - Always fail
- `diag(text)` - Diagnostic message

## Running Tests

All tests use Podman for containerized PostgreSQL:

```bash
export TESTCONTAINERS_PODMAN=true
export TESTCONTAINERS_RYUK_DISABLED=true

# Run all TestFoundry tests
pytest tests/extensions/testfoundry/ -xvs

# Run specific test file
pytest tests/extensions/testfoundry/test_pgtap_execution.py -xvs
```

## Notes

- TestFoundry uses a dedicated PostgreSQL schema (`testfoundry`) for its functions and tables
- Functions within the schema reference each other with the `testfoundry_` prefix
- Generated tests use psql-style variables with `\gset` commands
- The minimal pgTAP implementation is sufficient for testing but not for production use
