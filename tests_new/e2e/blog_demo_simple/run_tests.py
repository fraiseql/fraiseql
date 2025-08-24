#!/usr/bin/env python3
# ruff: noqa: T201
"""Real Database E2E Test Runner for FraiseQL Blog Demo

This script runs the complete E2E test suite using real database operations,
replacing the previous mock-based approach with actual PostgreSQL integration.

Features:
- Automatic Docker container management for PostgreSQL
- Transaction-based test isolation (automatic rollback)
- Real database schema setup for each test
- Comprehensive logging and error reporting
- Performance metrics and timing

Usage:
    python run_tests.py                    # Run all E2E tests
    python run_tests.py --fast             # Skip slow tests
    python run_tests.py --performance      # Run only performance tests
    python run_tests.py --verbose          # Detailed output
    python run_tests.py --help             # Show all options
"""

import argparse
import os
import sys
import time
from pathlib import Path

# Add the tests_new directory to Python path
tests_new_dir = Path(__file__).parent.parent.parent
sys.path.insert(0, str(tests_new_dir))


def run_pytest(args):
    """Run pytest with appropriate arguments."""
    import subprocess

    # Base pytest command
    cmd = ["python", "-m", "pytest"]

    # Add test file
    test_file = "test_blog_real_database.py"
    if Path(test_file).exists():
        cmd.append(test_file)
    else:
        print(f"❌ Test file {test_file} not found")
        return False

    # Add pytest arguments based on options
    if args.fast:
        cmd.extend(["-m", "not slow"])
        print("🏃 Running fast tests only (skipping slow tests)")

    if args.performance:
        cmd.extend(["-m", "performance"])
        print("📊 Running performance tests only")

    if args.verbose:
        cmd.extend(["-v", "-s", "--tb=long"])
        print("🔍 Running with verbose output")
    else:
        cmd.extend(["--tb=short"])

    # Add real database markers
    cmd.extend(["-m", "e2e and blog_demo"])

    # Environment variables for database testing
    env = os.environ.copy()
    env.update(
        {
            "PYTEST_CURRENT_TEST": "real_database_e2e",
            "PYTHONPATH": str(tests_new_dir),
        }
    )

    print(f"🚀 Running command: {' '.join(cmd)}")
    print("📋 Test Environment:")
    print(f"   - Tests directory: {Path.cwd()}")
    print(f"   - Python path: {tests_new_dir}")
    print("   - Database: PostgreSQL (Docker container)")
    print("   - Isolation: Transaction-based (automatic rollback)")
    print("=" * 60)

    # Run the tests
    start_time = time.time()
    try:
        result = subprocess.run(cmd, env=env, check=False)
        duration = time.time() - start_time

        print("=" * 60)
        if result.returncode == 0:
            print(f"✅ All tests passed! Duration: {duration:.2f}s")
            return True
        print(f"❌ Tests failed with exit code {result.returncode}. Duration: {duration:.2f}s")
        return False

    except KeyboardInterrupt:
        duration = time.time() - start_time
        print(f"\n⏹️  Tests interrupted by user after {duration:.2f}s")
        return False
    except Exception as e:
        duration = time.time() - start_time
        print(f"💥 Error running tests after {duration:.2f}s: {e}")
        return False


def check_dependencies():
    """Check if required dependencies are available."""
    missing = []

    # Check for pytest
    try:
        import pytest

        print(f"✅ pytest {pytest.__version__}")
    except ImportError:
        missing.append("pytest")

    # Check for psycopg
    try:
        import psycopg

        print(f"✅ psycopg {psycopg.__version__}")
    except ImportError:
        missing.append("psycopg")

    # Check for fraiseql
    try:
        import fraiseql

        print(f"✅ fraiseql {getattr(fraiseql, '__version__', 'dev')}")
    except ImportError:
        missing.append("fraiseql")

    # Check for testcontainers (optional)
    try:
        import testcontainers

        print(f"✅ testcontainers {testcontainers.__version__}")
    except ImportError:
        print("⚠️  testcontainers not available (will try external database)")

    if missing:
        print("❌ Missing required dependencies:")
        for dep in missing:
            print(f"   - {dep}")
        print("\nInstall missing dependencies with:")
        print(f"   pip install {' '.join(missing)}")
        return False

    return True


def show_test_info():
    """Show information about the test suite."""
    print("📚 FraiseQL Blog Demo - Real Database E2E Tests")
    print("=" * 60)
    print("🎯 What these tests validate:")
    print("   ✓ Real PostgreSQL database operations")
    print("   ✓ Complete GraphQL schema functionality")
    print("   ✓ Foreign key relationships and constraints")
    print("   ✓ JSONB field storage and retrieval")
    print("   ✓ Transaction isolation between tests")
    print("   ✓ User registration → post creation → publishing workflow")
    print("   ✓ Comment threading with moderation")
    print("   ✓ Data consistency across mutations and queries")
    print("   ✓ Performance characteristics")
    print()
    print("🔧 Test Infrastructure:")
    print("   • PostgreSQL via Docker container (automatic)")
    print("   • Transaction-based isolation (automatic rollback)")
    print("   • Real database schema setup per test")
    print("   • Seed data loading")
    print("   • No manual cleanup required")
    print()


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Run FraiseQL Blog Demo E2E tests with real database",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )

    parser.add_argument("--fast", action="store_true", help="Skip slow tests (faster execution)")
    parser.add_argument("--performance", action="store_true", help="Run only performance tests")
    parser.add_argument(
        "--verbose", "-v", action="store_true", help="Verbose output with detailed logging"
    )
    parser.add_argument("--info", action="store_true", help="Show test information and exit")

    args = parser.parse_args()

    if args.info:
        show_test_info()
        return True

    print("🧪 FraiseQL Blog Demo - Real Database E2E Test Runner")
    print("=" * 60)

    # Check dependencies
    print("🔍 Checking dependencies...")
    if not check_dependencies():
        return False

    print("\n📋 Test Configuration:")
    print(f"   - Working directory: {Path.cwd()}")
    print("   - Test isolation: Transaction-based (automatic rollback)")
    print("   - Database: PostgreSQL (Docker container)")
    print("   - Schema: Real database tables and views")
    print()

    # Run tests
    success = run_pytest(args)

    if success:
        print("\n🎉 E2E Tests completed successfully!")
        print("   All real database operations validated ✅")
        return True
    print("\n💥 E2E Tests failed!")
    print("   Check the output above for details ❌")
    return False


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
