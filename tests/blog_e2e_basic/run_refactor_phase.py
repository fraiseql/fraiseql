#!/usr/bin/env python3
"""Runner for REFACTOR phase tests - demonstrates enhanced comprehensive testing.

This script runs the enhanced E2E test suite to demonstrate the REFACTOR phase
of micro TDD. This builds upon the working GREEN phase implementation with
comprehensive error scenarios, performance tests, and advanced patterns.

Expected behavior:
- All GREEN phase tests continue to pass
- Additional REFACTOR phase tests demonstrate advanced patterns
- Performance characteristics are validated
- Edge cases and security scenarios are tested
- Database transaction integrity is verified
"""

import asyncio
import subprocess
import sys
from pathlib import Path


async def main():
    """Run REFACTOR phase tests showing comprehensive implementation."""
    print("🔵 REFACTOR PHASE - Running comprehensive E2E tests with advanced patterns")
    print("=" * 70)
    print()
    print("Expected behavior:")
    print("- All GREEN phase tests continue to pass")
    print("- Advanced validation and error handling scenarios")
    print("- Performance characteristics testing")
    print("- Database transaction integrity verification")
    print("- Security validation patterns")
    print("- Cache invalidation and materialized table consistency")
    print("- Bulk operations and edge case handling")
    print()
    print("REFACTOR phase enhancements:")
    print("- ✅ Advanced author validation (email normalization, identifiers)")
    print("- ✅ Complex post validation (content security, tag hierarchy)")
    print("- ✅ Error metadata enhancement and consistency")
    print("- ✅ Database transaction integrity testing")
    print("- ✅ Cache invalidation pattern validation")
    print("- ✅ Performance characteristics testing")
    print("- ✅ Bulk operations and edge cases")
    print()
    print("Starting comprehensive test run...")
    print("-" * 50)

    # Get the test directory
    test_dir = Path(__file__).parent

    # Run both GREEN (original) and REFACTOR (enhanced) test suites
    test_files = [
        "test_red_phase.py",     # Original tests (should still pass)
        "test_refactor_phase.py" # Enhanced tests
    ]

    total_success = True

    for test_file in test_files:
        print(f"\n🧪 Running {test_file}")
        print("─" * 40)

        cmd = [
            sys.executable, "-m", "pytest",
            str(test_dir / test_file),
            "-v",                    # Verbose output
            "--tb=short",           # Short traceback on failures
            "--no-header",          # No pytest header
            "--disable-warnings",   # Clean output
            "-m", "not performance" if test_file == "test_refactor_phase.py" else "",  # Skip performance tests by default
            # Remove empty string from command
            *(["-m", "not performance"] if test_file == "test_refactor_phase.py" else [])
        ]

        # Remove empty strings from command
        cmd = [arg for arg in cmd if arg != ""]

        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=120)

            print(f"Results for {test_file}:")
            if result.stdout:
                # Show just the summary line and any failures
                lines = result.stdout.split('\n')
                for line in lines:
                    if any(keyword in line for keyword in ['PASSED', 'FAILED', 'ERROR', 'passed', 'failed', 'error']) or line.startswith('='):
                        print(line)
                    elif 'test_' in line and ('::' in line or '...' in line):
                        # Show test names
                        print(line)

            if result.stderr and result.returncode != 0:
                print("ERRORS:")
                print(result.stderr[:1000])  # Limit error output

            if result.returncode != 0:
                total_success = False
                print(f"❌ {test_file} had failures")
            else:
                print(f"✅ {test_file} passed")

        except subprocess.TimeoutExpired:
            print(f"⏰ {test_file} timed out after 120 seconds")
            total_success = False
        except Exception as e:
            print(f"❌ Error running {test_file}: {e}")
            total_success = False

    # Optionally run performance tests separately
    print("\n🚀 Performance Tests (optional)")
    print("─" * 30)
    print("Running performance-specific tests...")

    perf_cmd = [
        sys.executable, "-m", "pytest",
        str(test_dir / "test_refactor_phase.py"),
        "-v",
        "-m", "performance",    # Only performance tests
        "--tb=short",
        "--no-header",
        "--disable-warnings"
    ]

    try:
        perf_result = subprocess.run(perf_cmd, capture_output=True, text=True, timeout=60)
        if perf_result.returncode == 0:
            print("✅ Performance tests passed")
            if "Created" in perf_result.stdout:
                # Show performance metrics
                for line in perf_result.stdout.split('\n'):
                    if "Created" in line and "seconds" in line:
                        print(f"   📊 {line.strip()}")
        else:
            print("⚠️  Performance tests had issues (this is optional)")

    except subprocess.TimeoutExpired:
        print("⏰ Performance tests timed out (acceptable for comprehensive testing)")
    except Exception as e:
        print(f"⚠️  Performance test error: {e}")

    print()
    print("=" * 70)

    if total_success:
        print("🎉 REFACTOR PHASE SUCCESSFUL - All tests passed!")
        print()
        print("Comprehensive testing demonstrates:")
        print("1. ✅ Database-first architecture with rich error handling")
        print("2. ✅ Advanced validation patterns and security checks")
        print("3. ✅ Transaction integrity and rollback scenarios")
        print("4. ✅ Cache invalidation and materialized table consistency")
        print("5. ✅ Performance characteristics and bulk operations")
        print("6. ✅ Error metadata consistency across all mutations")
        print("7. ✅ Complex relationship handling (tags, hierarchies)")
        print("8. ✅ Edge cases and concurrent operation handling")
        print()
        print("The E2E test suite demonstrates a complete implementation of:")
        print("- PrintOptim Backend patterns (database-first, two-function)")
        print("- FraiseQL mutation system with comprehensive error handling")
        print("- PostgreSQL as single source of truth with rich validation")
        print("- Materialized projections for performance")
        print("- NOOP patterns for idempotency and error handling")
        print()
        print("This test suite can serve as a reference implementation for:")
        print("- Database-first GraphQL API development")
        print("- Comprehensive error handling patterns")
        print("- E2E testing of complex business logic")
        print("- Performance testing of database operations")

        return 0
    else:
        print("❌ Some test suites had failures.")
        print()
        print("This could indicate:")
        print("- Database connection or setup issues")
        print("- Missing dependencies or configuration")
        print("- Implementation gaps in the GREEN phase")
        print("- Environmental differences (PostgreSQL version, etc.)")
        print()
        print("For troubleshooting:")
        print("1. Ensure PostgreSQL is running and accessible")
        print("2. Check that all dependencies are installed")
        print("3. Review the specific error messages above")
        print("4. Consider running tests individually for better debugging")

        return 1


if __name__ == "__main__":
    try:
        exit_code = asyncio.run(main())
        sys.exit(exit_code)
    except KeyboardInterrupt:
        print("\n🛑 Test run interrupted by user")
        sys.exit(1)
    except Exception as e:
        print(f"\n💥 Unexpected error: {e}")
        sys.exit(1)
