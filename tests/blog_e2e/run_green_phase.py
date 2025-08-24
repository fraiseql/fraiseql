#!/usr/bin/env python3
"""Runner for GREEN phase tests - demonstrates working implementation.

This script runs the comprehensive E2E test suite to demonstrate the GREEN phase
of micro TDD. All tests should now pass with the minimal PostgreSQL functions
and GraphQL mutations implemented.

Expected behavior:
- All tests pass with minimal implementation
- Error scenarios are properly handled
- Database-first architecture works correctly
- FraiseQL error handling provides rich error responses
"""

import asyncio
import subprocess
import sys
from pathlib import Path


async def main():
    """Run GREEN phase tests and show expected success."""
    print("🟢 GREEN PHASE - Running E2E tests with minimal implementation")
    print("=" * 60)
    print()
    print("Expected behavior:")
    print("- All tests should PASS with minimal implementation")
    print("- PostgreSQL functions handle business logic and validation")
    print("- FraiseQL mutations provide rich error responses")
    print("- Database-first architecture demonstrates error handling patterns")
    print()
    print("Implementation includes:")
    print("- app.* wrapper functions accepting JSONB from GraphQL")
    print("- core.* business logic functions with comprehensive validation")
    print("- PrintOptim-style mutation_result type with rich metadata")
    print("- FraiseQL mutations using BlogMutationBase pattern")
    print("- Comprehensive error scenarios (NOOP patterns)")
    print()
    print("Starting test run...")
    print("-" * 40)
    
    # Get the test directory
    test_dir = Path(__file__).parent
    
    # Run pytest with detailed output
    cmd = [
        sys.executable, "-m", "pytest", 
        str(test_dir / "test_red_phase.py"),
        "-v",                    # Verbose output
        "--tb=short",           # Short traceback on failures
        "--no-header",          # No pytest header
        "--disable-warnings",   # Clean output
        "-s"                    # Don't capture output (show prints)
    ]
    
    try:
        result = subprocess.run(cmd, capture_output=True, text=True)
        
        print("TEST OUTPUT:")
        print(result.stdout)
        if result.stderr:
            print("\nERRORS/WARNINGS:")
            print(result.stderr)
        
        print()
        print("=" * 60)
        if result.returncode == 0:
            print("✅ GREEN PHASE SUCCESSFUL - All tests passed!")
            print()
            print("What was demonstrated:")
            print("1. ✅ Database-first architecture with rich error handling")
            print("2. ✅ Two-function pattern (app.* → core.*) following PrintOptim")
            print("3. ✅ Comprehensive validation with NOOP status codes")
            print("4. ✅ FraiseQL mutations with DEFAULT_ERROR_CONFIG")
            print("5. ✅ Rich error metadata for debugging and client handling")
            print("6. ✅ PostgreSQL functions as single source of truth")
            print()
            print("Next steps:")
            print("- REFACTOR phase: Add more error scenarios and optimizations")
            print("- Cache invalidation patterns")
            print("- Performance optimizations")
            print("- Additional validation patterns")
        else:
            print("❌ Some tests failed. Check the output above for details.")
            print()
            print("Common issues:")
            print("- Database connection problems")
            print("- Missing dependencies (pytest, pytest-asyncio, asyncpg)")
            print("- PostgreSQL not running or accessible")
            print("- FraiseQL version compatibility")
            return 1
            
    except FileNotFoundError:
        print("❌ Error: pytest not found. Please install dependencies:")
        print("pip install pytest pytest-asyncio asyncpg")
        return 1
    except Exception as e:
        print(f"❌ Error running tests: {e}")
        return 1
    
    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)