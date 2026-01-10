#!/usr/bin/env python3
"""
Analyze chaos test failures and categorize them.

This script parses pytest output and creates a categorized failure inventory.
"""

import re
import sys
from collections import defaultdict
from pathlib import Path


class FailureAnalyzer:
    """Analyze and categorize chaos test failures."""

    CATEGORIES = {
        'ENVIRONMENT': 'Environment-specific (hardware, timing)',
        'CONFIGURATION': 'Configuration issues (pools, timeouts)',
        'BUG': 'Potential bug (requires investigation)',
        'TEST_DESIGN': 'Test design issue (unrealistic expectations)',
        'UNKNOWN': 'Needs analysis',
    }

    def __init__(self, test_output_file):
        """Initialize analyzer with test output file."""
        self.test_output_file = Path(test_output_file)
        self.failures = []
        self.passes = []
        self.summary = {
            'total': 0,
            'passed': 0,
            'failed': 0,
            'by_category': defaultdict(list),
            'by_test_file': defaultdict(lambda: {'pass': 0, 'fail': 0}),
        }

    def parse_test_output(self):
        """Parse pytest output to extract test results."""
        if not self.test_output_file.exists():
            print(f"Error: File not found: {self.test_output_file}")
            return False

        content = self.test_output_file.read_text()

        # Pattern to match test results
        # Example: tests/chaos/auth/test_auth_chaos.py::TestClass::test_name FAILED
        pattern = r'(tests/chaos/[^\s]+)::([\w:]+)\s+(PASSED|FAILED)'

        for match in re.finditer(pattern, content):
            file_path, test_name, status = match.groups()

            test_info = {
                'file': file_path,
                'name': test_name,
                'status': status,
                'category': 'UNKNOWN',
                'priority': 'MEDIUM',
            }

            if status == 'PASSED':
                self.passes.append(test_info)
                file_key = file_path.split('/')[-1]
                self.summary['by_test_file'][file_key]['pass'] += 1
            else:
                self.failures.append(test_info)
                file_key = file_path.split('/')[-1]
                self.summary['by_test_file'][file_key]['fail'] += 1

        self.summary['total'] = len(self.passes) + len(self.failures)
        self.summary['passed'] = len(self.passes)
        self.summary['failed'] = len(self.failures)

        return True

    def categorize_failures(self):
        """Categorize failures based on test name and patterns."""
        for failure in self.failures:
            name = failure['name'].lower()

            # Authentication-related patterns
            if 'auth' in name or 'jwt' in name or 'rbac' in name:
                if 'concurrent' in name or 'load' in name:
                    failure['category'] = 'ENVIRONMENT'
                    failure['priority'] = 'LOW'
                    failure['reason'] = 'Contention not detected - system too fast'
                elif 'service_outage' in name:
                    failure['category'] = 'BUG'
                    failure['priority'] = 'HIGH'
                    failure['reason'] = 'Auth service failure not handled correctly'
                elif 'expiration' in name or 'validation' in name:
                    failure['category'] = 'BUG'
                    failure['priority'] = 'HIGH'
                    failure['reason'] = 'JWT validation logic may have issues'
                else:
                    failure['category'] = 'UNKNOWN'
                    failure['priority'] = 'MEDIUM'

            # Cache-related patterns
            elif 'cache' in name:
                if 'memory_pressure' in name or 'stampede' in name:
                    failure['category'] = 'CONFIGURATION'
                    failure['priority'] = 'MEDIUM'
                    failure['reason'] = 'Cache pool size or eviction policy needs tuning'
                else:
                    failure['category'] = 'UNKNOWN'
                    failure['priority'] = 'MEDIUM'

            # Concurrency-related patterns
            elif 'concurrency' in name or 'race' in name or 'deadlock' in name:
                if 'concurrent' in name:
                    failure['category'] = 'ENVIRONMENT'
                    failure['priority'] = 'LOW'
                    failure['reason'] = 'Hardware too fast to trigger race condition'
                else:
                    failure['category'] = 'BUG'
                    failure['priority'] = 'HIGH'
                    failure['reason'] = 'Potential concurrency bug'

            # Database-related patterns
            elif 'database' in name or 'consistency' in name or 'transaction' in name:
                if 'load' in name or 'concurrent' in name:
                    failure['category'] = 'CONFIGURATION'
                    failure['priority'] = 'MEDIUM'
                    failure['reason'] = 'Pool size or connection limits need adjustment'
                else:
                    failure['category'] = 'BUG'
                    failure['priority'] = 'HIGH'
                    failure['reason'] = 'Data consistency issue - needs investigation'

            # Network-related patterns
            elif 'network' in name or 'connection' in name or 'latency' in name:
                failure['category'] = 'TEST_DESIGN'
                failure['priority'] = 'LOW'
                failure['reason'] = 'Network simulation not available in environment'

            # Default
            else:
                failure['category'] = 'UNKNOWN'
                failure['priority'] = 'MEDIUM'

            # Add to category summary
            self.summary['by_category'][failure['category']].append(failure)

    def generate_report(self):
        """Generate a detailed failure report."""
        report = []
        report.append("=" * 80)
        report.append("CHAOS TEST FAILURE ANALYSIS REPORT")
        report.append("=" * 80)
        report.append("")

        # Summary section
        report.append("SUMMARY")
        report.append("-" * 80)
        report.append(f"Total Tests:    {self.summary['total']}")
        report.append(f"Passed:         {self.summary['passed']} ({self.summary['passed']/self.summary['total']*100:.1f}%)")
        report.append(f"Failed:         {self.summary['failed']} ({self.summary['failed']/self.summary['total']*100:.1f}%)")
        report.append("")

        # By category
        report.append("FAILURES BY CATEGORY")
        report.append("-" * 80)
        for category in ['BUG', 'CONFIGURATION', 'ENVIRONMENT', 'TEST_DESIGN', 'UNKNOWN']:
            failures = self.summary['by_category'].get(category, [])
            if failures:
                report.append(f"{category:15} {len(failures):3} tests")
                report.append(f"  Description: {self.CATEGORIES[category]}")
                report.append("")

        # By test file
        report.append("FAILURES BY TEST FILE")
        report.append("-" * 80)
        for file_name, stats in sorted(self.summary['by_test_file'].items()):
            total = stats['pass'] + stats['fail']
            pass_pct = stats['pass'] / total * 100 if total > 0 else 0
            if stats['fail'] > 0:
                report.append(f"{file_name:40} {stats['fail']:2} failures  ({pass_pct:.0f}% pass rate)")

        report.append("")

        # Detailed failures
        report.append("DETAILED FAILURE ANALYSIS")
        report.append("=" * 80)

        for category in ['BUG', 'CONFIGURATION', 'ENVIRONMENT', 'TEST_DESIGN', 'UNKNOWN']:
            failures = self.summary['by_category'].get(category, [])
            if not failures:
                continue

            report.append("")
            report.append(f"CATEGORY: {category} ({len(failures)} failures)")
            report.append("-" * 80)

            for i, failure in enumerate(failures, 1):
                report.append(f"\n{i}. {failure['name']}")
                report.append(f"   File:     {failure['file']}")
                report.append(f"   Priority: {failure['priority']}")
                if 'reason' in failure:
                    report.append(f"   Reason:   {failure['reason']}")

        return "\n".join(report)

    def generate_csv(self):
        """Generate CSV format for spreadsheet import."""
        csv = []
        csv.append("File,Test Name,Status,Category,Priority,Reason")

        for failure in self.failures:
            reason = failure.get('reason', '').replace(',', ';')
            csv.append(f"{failure['file']},{failure['name']},FAILED,{failure['category']},{failure['priority']},{reason}")

        return "\n".join(csv)


def main():
    """Main entry point."""
    if len(sys.argv) < 2:
        print("Usage: analyze_chaos_failures.py <test_output_file>")
        print("\nExample:")
        print("  pytest tests/chaos -v --tb=short > output.txt 2>&1")
        print("  ./scripts/analyze_chaos_failures.py output.txt")
        sys.exit(1)

    analyzer = FailureAnalyzer(sys.argv[1])

    if not analyzer.parse_test_output():
        sys.exit(1)

    analyzer.categorize_failures()

    # Generate report
    report = analyzer.generate_report()
    print(report)

    # Save to files
    output_dir = Path("tests/chaos/analysis")
    output_dir.mkdir(exist_ok=True)

    report_file = output_dir / "failure_report.txt"
    report_file.write_text(report)
    print(f"\n\nReport saved to: {report_file}")

    csv_file = output_dir / "failure_inventory.csv"
    csv_file.write_text(analyzer.generate_csv())
    print(f"CSV saved to: {csv_file}")


if __name__ == "__main__":
    main()
