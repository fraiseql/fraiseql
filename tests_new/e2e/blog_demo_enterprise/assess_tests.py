#!/usr/bin/env python3
"""
Test Assessment Script for FraiseQL Blog Demo Enterprise

This script provides a comprehensive assessment of test status across the project.
Run with: python assess_tests.py
"""

import json
import logging
import os
import re
import subprocess

try:
    from rich.console import Console
    from rich.logging import RichHandler
    from rich.panel import Panel
    from rich.text import Text

    console = Console()
    logging.basicConfig(
        level=logging.INFO,
        format="%(message)s",
        handlers=[RichHandler(console=console, show_time=False)],
    )
except ImportError:
    console = None
    logging.basicConfig(level=logging.INFO)

logger = logging.getLogger(__name__)


def smart_print(message: str, style: str = "white") -> None:
    """Print with rich styling if available, fallback to regular print."""
    if console:
        console.print(message, style=style)
    else:
        print(message)


def smart_panel(content: str, title: str = "", border_style: str = "blue") -> None:
    """Print a panel with rich if available, fallback to formatted text."""
    if console:
        console.print(Panel.fit(Text.from_markup(content), title=title, border_style=border_style))
    else:
        print(f"\n--- {title} ---")
        print(content.replace("[bold]", "").replace("[/bold]", ""))
        print("-" * 40)


class TestAssessor:
    """Assess test status across the FraiseQL project."""

    def __init__(self):
        self.results = {
            "blog_demo_enterprise": {"unit": {}, "integration": {}},
            "blog_demo_simple": {},
            "core_fraiseql": {},
            "summary": {},
        }

    def run_command(self, cmd: list[str], cwd: str = None) -> tuple[int, str, str]:
        """Run a command and return exit code, stdout, stderr."""
        try:
            result = subprocess.run(
                cmd,
                cwd=cwd,
                capture_output=True,
                text=True,
                timeout=300,  # 5 minute timeout
            )
            return result.returncode, result.stdout, result.stderr
        except subprocess.TimeoutExpired:
            return -1, "", "Command timed out"
        except Exception as e:
            return -1, "", str(e)

    def parse_pytest_output(self, output: str) -> dict:
        """Parse pytest output to extract test statistics."""
        stats = {"passed": 0, "failed": 0, "errors": 0, "skipped": 0, "total": 0}

        # Look for pytest summary line like "=== 71 passed, 87 warnings in 0.16s ==="
        summary_pattern = r"=+ ([\d\w\s,]+) in [\d\.]+s =+"
        match = re.search(summary_pattern, output)

        if match:
            summary = match.group(1)
            # Extract numbers for different statuses
            for status in ["passed", "failed", "error", "skipped"]:
                pattern = rf"(\d+) {status}"
                status_match = re.search(pattern, summary)
                if status_match:
                    stats[status] = int(status_match.group(1))

        # Calculate total
        stats["total"] = sum([stats[key] for key in ["passed", "failed", "errors", "skipped"]])

        # Extract failed test names
        failed_pattern = r"FAILED (.+?) - "
        failed_matches = re.findall(failed_pattern, output)
        stats["failed_tests"] = failed_matches

        return stats

    def assess_blog_demo_enterprise_unit_tests(self):
        """Assess blog demo enterprise unit tests."""
        smart_print("🔍 Assessing Blog Demo Enterprise Unit Tests...", "blue")

        cwd = "/home/lionel/code/fraiseql/tests_new/e2e/blog_demo_enterprise"
        cmd = ["uv", "run", "pytest", "tests/unit/domain/", "--tb=short", "-q"]
        env = os.environ.copy()
        env["PYTHONPATH"] = "src"

        exit_code, stdout, stderr = self.run_command(cmd, cwd)

        stats = self.parse_pytest_output(stdout + stderr)
        stats["exit_code"] = exit_code
        stats["command"] = " ".join(cmd)
        stats["working_directory"] = cwd

        self.results["blog_demo_enterprise"]["unit"] = stats

        if exit_code == 0:
            smart_print(f"  ✅ Unit tests: {stats['passed']} passed", "green")
        else:
            smart_print(
                f"  ❌ Unit tests: {stats['failed']} failed, {stats['passed']} passed", "red"
            )

    def assess_blog_demo_enterprise_integration_tests(self):
        """Assess blog demo enterprise integration tests."""
        smart_print("🔍 Assessing Blog Demo Enterprise Integration Tests...", "blue")

        cwd = "/home/lionel/code/fraiseql/tests_new/e2e/blog_demo_enterprise"
        cmd = ["uv", "run", "pytest", "tests/integration/", "--tb=short", "-q"]
        env = os.environ.copy()
        env["PYTHONPATH"] = "src"

        exit_code, stdout, stderr = self.run_command(cmd, cwd)

        stats = self.parse_pytest_output(stdout + stderr)
        stats["exit_code"] = exit_code
        stats["command"] = " ".join(cmd)
        stats["working_directory"] = cwd

        self.results["blog_demo_enterprise"]["integration"] = stats

        if exit_code == 0:
            smart_print(f"  ✅ Integration tests: {stats['passed']} passed", "green")
        else:
            smart_print(
                f"  ❌ Integration tests: {stats['failed']} failed, {stats['passed']} passed", "red"
            )

    def assess_core_fraiseql_tests(self):
        """Assess core FraiseQL framework tests."""
        smart_print("🔍 Assessing Core FraiseQL Framework Tests...", "blue")

        cwd = "/home/lionel/code/fraiseql"
        cmd = [".venv/bin/pytest", "tests/", "--tb=short", "-q", "--ignore=tests_new/"]

        exit_code, stdout, stderr = self.run_command(cmd, cwd)

        stats = self.parse_pytest_output(stdout + stderr)
        stats["exit_code"] = exit_code
        stats["command"] = " ".join(cmd)
        stats["working_directory"] = cwd

        self.results["core_fraiseql"] = stats

        if exit_code == 0:
            smart_print(f"  ✅ Core tests: {stats['passed']} passed", "green")
        else:
            smart_print(
                f"  ❌ Core tests: {stats['failed']} failed, {stats['passed']} passed", "red"
            )

    def assess_blog_demo_simple_tests(self):
        """Assess blog demo simple tests."""
        smart_print("🔍 Assessing Blog Demo Simple Tests...", "blue")

        cwd = "/home/lionel/code/fraiseql/tests_new/e2e/blog_demo_simple"
        cmd = ["python", "-m", "pytest", "--tb=short", "-q"]

        exit_code, stdout, stderr = self.run_command(cmd, cwd)

        stats = self.parse_pytest_output(stdout + stderr)
        stats["exit_code"] = exit_code
        stats["command"] = " ".join(cmd)
        stats["working_directory"] = cwd

        self.results["blog_demo_simple"] = stats

        if exit_code == 0:
            smart_print(f"  ✅ Simple blog tests: {stats['passed']} passed", "green")
        else:
            smart_print(
                f"  ❌ Simple blog tests: {stats['failed']} failed, {stats['passed']} passed", "red"
            )

    def generate_summary(self):
        """Generate overall summary statistics."""
        total_passed = 0
        total_failed = 0
        total_errors = 0
        total_skipped = 0

        # Sum up all test results
        for category, data in self.results.items():
            if category == "summary":
                continue

            if isinstance(data, dict):
                if "unit" in data and "integration" in data:
                    # Blog demo enterprise has subcategories
                    for subcat in ["unit", "integration"]:
                        if data[subcat]:
                            total_passed += data[subcat].get("passed", 0)
                            total_failed += data[subcat].get("failed", 0)
                            total_errors += data[subcat].get("errors", 0)
                            total_skipped += data[subcat].get("skipped", 0)
                else:
                    # Other categories
                    total_passed += data.get("passed", 0)
                    total_failed += data.get("failed", 0)
                    total_errors += data.get("errors", 0)
                    total_skipped += data.get("skipped", 0)

        total_tests = total_passed + total_failed + total_errors + total_skipped

        self.results["summary"] = {
            "total_tests": total_tests,
            "passed": total_passed,
            "failed": total_failed,
            "errors": total_errors,
            "skipped": total_skipped,
            "pass_percentage": round(
                (total_passed / total_tests * 100) if total_tests > 0 else 0, 1
            ),
        }

    def print_detailed_report(self):
        """Print a detailed assessment report."""
        summary = self.results["summary"]

        # Main summary panel
        summary_content = (
            f"[bold]🎯 OVERALL SUMMARY[/bold]\n"
            f"   Total Tests: {summary['total_tests']}\n"
            f"   Passed: {summary['passed']} ({summary['pass_percentage']}%)\n"
            f"   Failed: {summary['failed']}\n"
            f"   Errors: {summary['errors']}\n"
            f"   Skipped: {summary['skipped']}"
        )
        smart_panel(summary_content, "📊 DETAILED TEST ASSESSMENT REPORT", "magenta")

        # Category details
        category_content = ""

        # Blog Demo Enterprise
        unit = self.results["blog_demo_enterprise"]["unit"]
        integration = self.results["blog_demo_enterprise"]["integration"]
        category_content += "[bold]📦 Blog Demo Enterprise:[/bold]\n"
        if unit:
            category_content += (
                f"    Unit Tests: {unit['passed']} passed, {unit['failed']} failed\n"
            )
        if integration:
            category_content += f"    Integration Tests: {integration['passed']} passed, {integration['failed']} failed\n"

        # Core FraiseQL
        category_content += "\n[bold]⚙️ Core FraiseQL Framework:[/bold]\n"
        core = self.results["core_fraiseql"]
        if core:
            category_content += f"    Tests: {core['passed']} passed, {core['failed']} failed\n"
            if core.get("failed_tests"):
                category_content += f"    Failed tests: {', '.join(core['failed_tests'][:5])}...\n"

        # Blog Demo Simple
        category_content += "\n[bold]📝 Blog Demo Simple:[/bold]\n"
        simple = self.results["blog_demo_simple"]
        if simple:
            category_content += f"    Tests: {simple['passed']} passed, {simple['failed']} failed\n"

        smart_panel(category_content, "📋 BY CATEGORY", "blue")

        smart_print("\n🎯 RECOMMENDED ACTIONS", "bold yellow")

        if summary["failed"] > 0 or summary["errors"] > 0:
            smart_print("   🔥 HIGH PRIORITY:", "red")
            if self.results["core_fraiseql"].get("failed", 0) > 0:
                smart_print(
                    f"      • Fix {self.results['core_fraiseql']['failed']} failing core framework tests",
                    "red",
                )
            if self.results["blog_demo_enterprise"]["integration"].get("failed", 0) > 0:
                smart_print(
                    f"      • Fix {self.results['blog_demo_enterprise']['integration']['failed']} failing integration tests",
                    "red",
                )

        if summary["skipped"] > 0:
            smart_print("   📋 MEDIUM PRIORITY:", "yellow")
            smart_print(f"      • Review {summary['skipped']} skipped tests", "yellow")

        if summary["pass_percentage"] < 90:
            smart_print("   📈 IMPROVEMENT NEEDED:", "yellow")
            smart_print(
                f"      • Overall pass rate is {summary['pass_percentage']}% - aim for >90%",
                "yellow",
            )

        smart_print("\n💾 Raw results saved to: test_assessment_results.json", "cyan")

    def save_results(self):
        """Save results to JSON file."""
        with open(
            "/home/lionel/code/fraiseql/tests_new/e2e/blog_demo_enterprise/test_assessment_results.json",
            "w",
        ) as f:
            json.dump(self.results, f, indent=2)

    def run_full_assessment(self):
        """Run complete test assessment."""
        smart_panel(
            "[bold]🚀 Starting Comprehensive Test Assessment...[/bold]",
            title="Test Assessment",
            border_style="green",
        )

        # Run all assessments
        self.assess_blog_demo_enterprise_unit_tests()
        self.assess_blog_demo_enterprise_integration_tests()
        self.assess_core_fraiseql_tests()
        self.assess_blog_demo_simple_tests()

        # Generate summary and report
        self.generate_summary()
        self.print_detailed_report()
        self.save_results()

        smart_print("\n✅ Assessment Complete!", "bold green")


def main():
    """Main entry point."""
    assessor = TestAssessor()
    assessor.run_full_assessment()


if __name__ == "__main__":
    main()
