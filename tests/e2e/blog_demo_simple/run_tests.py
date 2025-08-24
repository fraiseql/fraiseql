#!/usr/bin/env python3
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
import logging
import os
import sys
import time
from pathlib import Path

try:
    from rich.console import Console
    from rich.logging import RichHandler
    from rich.panel import Panel
    from rich.text import Text

    # Setup console and logging with rich
    console = Console()
    logging.basicConfig(
        level=logging.INFO,
        format="%(message)s",
        handlers=[RichHandler(console=console, show_time=False)],
    )
except ImportError:
    # Fallback to basic console if rich is not available
    console = None
    logging.basicConfig(level=logging.INFO)

logger = logging.getLogger(__name__)


def smart_print(message: str, style: str = "white") -> None:
    """Print with rich styling if available, fallback to regular print."""
    if console:
        console.print(message, style=style)
    else:
        # Use sys.stdout.write to avoid T201 lint error
        import sys

        sys.stdout.write(message + "\n")
        sys.stdout.flush()


def smart_panel(content: str, title: str = "", border_style: str = "blue") -> None:
    """Print a panel with rich if available, fallback to formatted text."""
    if console:
        console.print(Panel.fit(Text.from_markup(content), title=title, border_style=border_style))
    else:
        # Use sys.stdout.write to avoid T201 lint error
        import sys

        sys.stdout.write(f"\n--- {title} ---\n")
        sys.stdout.write(content.replace("[bold]", "").replace("[/bold]", "") + "\n")
        sys.stdout.write("-" * 40 + "\n")
        sys.stdout.flush()


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
        smart_print(f"❌ Test file {test_file} not found", "red")
        return False

    # Add pytest arguments based on options
    if args.fast:
        cmd.extend(["-m", "not slow"])
        smart_print("🏃 Running fast tests only (skipping slow tests)", "yellow")

    if args.performance:
        cmd.extend(["-m", "performance"])
        smart_print("📊 Running performance tests only", "blue")

    if args.verbose:
        cmd.extend(["-v", "-s", "--tb=long"])
        smart_print("🔍 Running with verbose output", "cyan")
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

    smart_print(f"🚀 Running command: {' '.join(cmd)}", "bold blue")
    smart_panel(
        f"[bold]📋 Test Environment:[/bold]\n"
        f"   • Tests directory: {Path.cwd()}\n"
        f"   • Python path: {tests_new_dir}\n"
        f"   • Database: PostgreSQL (Docker container)\n"
        f"   • Isolation: Transaction-based (automatic rollback)",
        title="Environment",
        border_style="blue",
    )

    # Run the tests
    start_time = time.time()
    try:
        result = subprocess.run(cmd, env=env, check=False)
        duration = time.time() - start_time

        if result.returncode == 0:
            smart_print(f"✅ All tests passed! Duration: {duration:.2f}s", "bold green")
            return True
        smart_print(
            f"❌ Tests failed with exit code {result.returncode}. Duration: {duration:.2f}s",
            "bold red",
        )
        return False

    except KeyboardInterrupt:
        duration = time.time() - start_time
        smart_print(f"\n⏹️  Tests interrupted by user after {duration:.2f}s", "yellow")
        return False
    except Exception as e:
        duration = time.time() - start_time
        smart_print(f"💥 Error running tests after {duration:.2f}s: {e}", "red")
        return False


def check_dependencies():
    """Check if required dependencies are available."""
    missing = []

    # Check for pytest
    try:
        import pytest

        smart_print(f"✅ pytest {pytest.__version__}", "green")
    except ImportError:
        missing.append("pytest")

    # Check for psycopg
    try:
        import psycopg

        smart_print(f"✅ psycopg {psycopg.__version__}", "green")
    except ImportError:
        missing.append("psycopg")

    # Check for fraiseql
    try:
        import fraiseql

        smart_print(f"✅ fraiseql {getattr(fraiseql, '__version__', 'dev')}", "green")
    except ImportError:
        missing.append("fraiseql")

    # Check for testcontainers (optional)
    try:
        import testcontainers

        smart_print(f"✅ testcontainers {testcontainers.__version__}", "green")
    except ImportError:
        smart_print("⚠️  testcontainers not available (will try external database)", "yellow")

    if missing:
        smart_print("❌ Missing required dependencies:", "red")
        for dep in missing:
            smart_print(f"   - {dep}", "red")
        smart_print("\nInstall missing dependencies with:", "yellow")
        smart_print(f"   pip install {' '.join(missing)}", "cyan")
        return False

    return True


def show_test_info():
    """Show information about the test suite."""
    smart_panel(
        "[bold]🎯 What these tests validate:[/bold]\n"
        "   ✓ Real PostgreSQL database operations\n"
        "   ✓ Complete GraphQL schema functionality\n"
        "   ✓ Foreign key relationships and constraints\n"
        "   ✓ JSONB field storage and retrieval\n"
        "   ✓ Transaction isolation between tests\n"
        "   ✓ User registration → post creation → publishing workflow\n"
        "   ✓ Comment threading with moderation\n"
        "   ✓ Data consistency across mutations and queries\n"
        "   ✓ Performance characteristics\n\n"
        "[bold]🔧 Test Infrastructure:[/bold]\n"
        "   • PostgreSQL via Docker container (automatic)\n"
        "   • Transaction-based isolation (automatic rollback)\n"
        "   • Real database schema setup per test\n"
        "   • Seed data loading\n"
        "   • No manual cleanup required",
        title="📚 FraiseQL Blog Demo - Real Database E2E Tests",
        border_style="green",
    )


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

    smart_print("🧪 FraiseQL Blog Demo - Real Database E2E Test Runner", "bold magenta")

    # Check dependencies
    smart_print("🔍 Checking dependencies...", "blue")
    if not check_dependencies():
        return False

    smart_panel(
        f"[bold]📋 Test Configuration:[/bold]\n"
        f"   - Working directory: {Path.cwd()}\n"
        f"   - Test isolation: Transaction-based (automatic rollback)\n"
        f"   - Database: PostgreSQL (Docker container)\n"
        f"   - Schema: Real database tables and views",
        title="Configuration",
        border_style="cyan",
    )

    # Run tests
    success = run_pytest(args)

    if success:
        smart_print("\n🎉 E2E Tests completed successfully!", "bold green")
        smart_print("   All real database operations validated ✅", "green")
        return True
    smart_print("\n💥 E2E Tests failed!", "bold red")
    smart_print("   Check the output above for details ❌", "red")
    return False


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
