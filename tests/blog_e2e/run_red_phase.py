#!/usr/bin/env python3
"""Runner for RED phase tests - demonstrates failing tests before implementation.

This script runs the comprehensive E2E test suite to demonstrate the RED phase
of micro TDD. All tests should fail since we haven't implemented the GraphQL
mutations or PostgreSQL functions yet.

Expected behavior:
- All tests fail with missing schema/function errors
- Test structure validates the expected API surface
- Error scenarios are comprehensively defined
"""

import asyncio
import subprocess
import sys
from pathlib import Path


async def main():
    """Run RED phase tests and show expected failures."""
    print("🔴 RED PHASE - Running failing E2E tests for Blog Application")
    print("=" * 60)
    print()
    print("Expected behavior:")
    print("- All tests should FAIL (we haven't implemented anything yet)")
    print("- Tests define the expected GraphQL API surface")  
    print("- Comprehensive error scenarios are specified")
    print("- Database schema exists but functions don't")
    print()
    print("Starting test run...")
    print("-" * 40)
    
    # Get the test directory
    test_dir = Path(__file__).parent
    
    # Run pytest with verbose output to show test definitions
    cmd = [
        sys.executable, "-m", "pytest", 
        str(test_dir / "test_red_phase.py"),
        "-v",                    # Verbose output
        "--tb=short",           # Short traceback
        "--no-header",          # No pytest header
        "--disable-warnings",   # Clean output
        "-x"                    # Stop on first failure to see pattern
    ]
    
    try:
        result = subprocess.run(cmd, capture_output=True, text=True)
        
        print("STDOUT:")
        print(result.stdout)
        print()
        print("STDERR:")
        print(result.stderr)
        
        print()
        print("=" * 60)
        if result.returncode != 0:
            print("✅ RED PHASE SUCCESSFUL - Tests failed as expected!")
            print()
            print("Next steps:")
            print("1. Implement PostgreSQL functions (app.* and core.* schemas)")
            print("2. Create FraiseQL GraphQL mutations with PrintOptimMutation pattern")
            print("3. Run GREEN phase to make tests pass")
            print("4. REFACTOR phase for error handling improvements")
        else:
            print("❌ Unexpected - Tests passed! Something is already implemented.")
            return 1
            
    except FileNotFoundError:
        print("❌ Error: pytest not found. Please install pytest:")
        print("pip install pytest pytest-asyncio")
        return 1
    except Exception as e:
        print(f"❌ Error running tests: {e}")
        return 1
    
    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)