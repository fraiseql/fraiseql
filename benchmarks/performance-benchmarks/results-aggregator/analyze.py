#!/usr/bin/env python3
"""
Performance benchmark results analyzer and report generator
"""

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import matplotlib.pyplot as plt
import numpy as np
from tabulate import tabulate


class BenchmarkAnalyzer:
    def __init__(self, results_dir: str = "/results"):
        self.results_dir = Path(results_dir)
        self.reports_dir = Path("/results/reports")
        self.reports_dir.mkdir(exist_ok=True)

        self.frameworks = ["fraiseql", "strawberry-sqlalchemy", "graphene-sqlalchemy", "ariadne"]
        self.test_types = ["simple-queries", "nested-queries", "load-test"]

    def load_results(self) -> dict[str, dict[str, Any]]:
        """Load all benchmark results from JSON files"""
        results = {}

        for framework in self.frameworks:
            results[framework] = {}

            for test_type in self.test_types:
                # Look for result files
                pattern = f"{framework}-{test_type}*.json"
                files = list(self.results_dir.glob(pattern))

                if files:
                    # Get the most recent file
                    latest_file = max(files, key=lambda f: f.stat().st_mtime)
                    with latest_file.open() as f:
                        results[framework][test_type] = json.load(f)

        return results

    def generate_comparison_table(self, results: dict[str, dict[str, Any]]) -> str:
        """Generate a comparison table of key metrics"""
        table_data = []
        headers = [
            "Framework",
            "Test Type",
            "Avg Response (ms)",
            "P95 (ms)",
            "P99 (ms)",
            "Error Rate",
            "Requests/sec",
        ]

        for framework, tests in results.items():
            for test_type, data in tests.items():
                if "summary" in data:
                    summary = data["summary"]
                    row = [
                        framework,
                        test_type,
                        f"{summary.get('avgResponseTime', 0):.2f}",
                        f"{summary.get('p95ResponseTime', 0):.2f}",
                        f"{summary.get('p99ResponseTime', 0):.2f}",
                        f"{summary.get('errorRate', 0):.2%}",
                        f"{summary.get('totalRequests', 0) / 300:.2f}",  # Assuming 5 min test
                    ]
                    table_data.append(row)

        return tabulate(table_data, headers=headers, tablefmt="pipe")

    def generate_response_time_chart(self, results: dict[str, dict[str, Any]]):
        """Generate response time comparison charts"""
        fig, axes = plt.subplots(1, 3, figsize=(15, 5))

        for idx, test_type in enumerate(self.test_types):
            ax = axes[idx]
            frameworks = []
            avg_times = []
            p95_times = []
            p99_times = []

            for framework, tests in results.items():
                if test_type in tests and "summary" in tests[test_type]:
                    summary = tests[test_type]["summary"]
                    frameworks.append(framework.replace("-", "\n"))
                    avg_times.append(summary.get("avgResponseTime", 0))
                    p95_times.append(summary.get("p95ResponseTime", 0))
                    p99_times.append(summary.get("p99ResponseTime", 0))

            if frameworks:
                x = np.arange(len(frameworks))
                width = 0.25

                ax.bar(x - width, avg_times, width, label="Average", alpha=0.8)
                ax.bar(x, p95_times, width, label="P95", alpha=0.8)
                ax.bar(x + width, p99_times, width, label="P99", alpha=0.8)

                ax.set_xlabel("Framework")
                ax.set_ylabel("Response Time (ms)")
                ax.set_title(f"{test_type.replace('-', ' ').title()}")
                ax.set_xticks(x)
                ax.set_xticklabels(frameworks)
                ax.legend()
                ax.grid(axis="y", alpha=0.3)

        plt.tight_layout()
        plt.savefig(self.reports_dir / "response_time_comparison.png", dpi=300)
        plt.close()

    def generate_throughput_chart(self, results: dict[str, dict[str, Any]]):
        """Generate throughput comparison chart"""
        fig, ax = plt.subplots(figsize=(10, 6))

        frameworks = []
        throughputs = []

        for framework, tests in results.items():
            if "load-test" in tests and "summary" in tests["load-test"]:
                summary = tests["load-test"]["summary"]
                frameworks.append(framework)
                # Calculate requests per second
                total_requests = summary.get("totalRequests", 0)
                throughputs.append(total_requests / 300)  # Assuming 5 min test

        if frameworks:
            bars = ax.bar(frameworks, throughputs, alpha=0.8)

            # Add value labels on bars
            for bar, value in zip(bars, throughputs):
                height = bar.get_height()
                ax.text(
                    bar.get_x() + bar.get_width() / 2.0,
                    height,
                    f"{value:.1f}",
                    ha="center",
                    va="bottom",
                )

            ax.set_xlabel("Framework")
            ax.set_ylabel("Requests per Second")
            ax.set_title("Throughput Comparison - Load Test (500 concurrent users)")
            ax.grid(axis="y", alpha=0.3)

        plt.tight_layout()
        plt.savefig(self.reports_dir / "throughput_comparison.png", dpi=300)
        plt.close()

    def generate_error_rate_chart(self, results: dict[str, dict[str, Any]]):
        """Generate error rate comparison chart"""
        fig, ax = plt.subplots(figsize=(10, 6))

        test_types = []
        fraiseql_errors = []
        strawberry_errors = []

        for test_type in self.test_types:
            test_types.append(test_type.replace("-", "\n"))

            # FraiseQL error rate
            if "fraiseql" in results and test_type in results["fraiseql"]:
                summary = results["fraiseql"][test_type].get("summary", {})
                fraiseql_errors.append(summary.get("errorRate", 0) * 100)
            else:
                fraiseql_errors.append(0)

            # Strawberry error rate
            if "strawberry-sqlalchemy" in results and test_type in results["strawberry-sqlalchemy"]:
                summary = results["strawberry-sqlalchemy"][test_type].get("summary", {})
                strawberry_errors.append(summary.get("errorRate", 0) * 100)
            else:
                strawberry_errors.append(0)

        x = np.arange(len(test_types))
        width = 0.35

        ax.bar(x - width / 2, fraiseql_errors, width, label="FraiseQL", alpha=0.8)
        ax.bar(x + width / 2, strawberry_errors, width, label="Strawberry+SQLAlchemy", alpha=0.8)

        ax.set_xlabel("Test Type")
        ax.set_ylabel("Error Rate (%)")
        ax.set_title("Error Rate Comparison")
        ax.set_xticks(x)
        ax.set_xticklabels(test_types)
        ax.legend()
        ax.grid(axis="y", alpha=0.3)

        plt.tight_layout()
        plt.savefig(self.reports_dir / "error_rate_comparison.png", dpi=300)
        plt.close()

    def generate_markdown_report(self, results: dict[str, dict[str, Any]]) -> str:
        """Generate comprehensive markdown report"""
        report = []
        report.append("# FraiseQL Performance Benchmark Report")
        report.append(
            f"\nGenerated: {datetime.now(tz=timezone.utc).strftime('%Y-%m-%d %H:%M:%S UTC')}"
        )
        report.append("\n## Executive Summary")

        # Find the best performer for each metric
        best_avg = float("inf")
        best_avg_framework = ""
        best_throughput = 0
        best_throughput_framework = ""

        for framework, tests in results.items():
            if "simple-queries" in tests:
                avg = (
                    tests["simple-queries"].get("summary", {}).get("avgResponseTime", float("inf"))
                )
                if avg < best_avg:
                    best_avg = avg
                    best_avg_framework = framework

            if "load-test" in tests:
                throughput = tests["load-test"].get("summary", {}).get("totalRequests", 0) / 300
                if throughput > best_throughput:
                    best_throughput = throughput
                    best_throughput_framework = framework

        report.append(
            f"\n- **Best Average Response Time**: {best_avg_framework} ({best_avg:.2f}ms)"
        )
        report.append(
            f"- **Highest Throughput**: {best_throughput_framework} ({best_throughput:.1f} req/s)"
        )

        report.append("\n## Performance Comparison Table")
        report.append("\n" + self.generate_comparison_table(results))

        report.append("\n## Detailed Results")

        for framework, tests in results.items():
            report.append(f"\n### {framework}")

            for test_type, data in tests.items():
                report.append(f"\n#### {test_type}")

                if "summary" in data:
                    summary = data["summary"]
                    report.append(f"\n- **Total Requests**: {summary.get('totalRequests', 0):,}")
                    report.append(
                        f"- **Average Response Time**: {summary.get('avgResponseTime', 0):.2f}ms"
                    )
                    report.append(
                        f"- **P95 Response Time**: {summary.get('p95ResponseTime', 0):.2f}ms"
                    )
                    report.append(
                        f"- **P99 Response Time**: {summary.get('p99ResponseTime', 0):.2f}ms"
                    )
                    report.append(f"- **Error Rate**: {summary.get('errorRate', 0):.2%}")
                    report.append(f"- **Max Concurrent Users**: {summary.get('maxVUs', 0)}")

                if "queryBreakdown" in data:
                    report.append("\n**Query Breakdown:**")
                    for query_name, metrics in data["queryBreakdown"].items():
                        report.append(f"\n- {query_name}:")
                        report.append(f"  - Count: {metrics.get('count', 0):,}")
                        report.append(f"  - Avg: {metrics.get('avg', 0):.2f}ms")
                        report.append(f"  - P95: {metrics.get('p95', 0):.2f}ms")

        report.append("\n## Performance Graphs")
        report.append("\n### Response Time Comparison")
        report.append("![Response Time Comparison](./response_time_comparison.png)")
        report.append("\n### Throughput Comparison")
        report.append("![Throughput Comparison](./throughput_comparison.png)")
        report.append("\n### Error Rate Comparison")
        report.append("![Error Rate Comparison](./error_rate_comparison.png)")

        report.append("\n## Key Findings")
        report.append("\n1. **FraiseQL Performance**: ")
        report.append("   - Optimized JSONB queries reduce database round trips")
        report.append("   - Single query execution for complex nested data")
        report.append("\n2. **Traditional ORM Challenges**: ")
        report.append("   - N+1 query patterns impact performance under load")
        report.append("   - DataLoader helps but adds complexity")

        report.append("\n## Recommendations")
        report.append("\n- Use FraiseQL for read-heavy workloads with complex nested data")
        report.append("- Consider caching strategies for frequently accessed data")
        report.append("- Monitor database connection pool usage under high load")

        return "\n".join(report)

    def run(self):
        """Run the complete analysis"""
        print("Loading benchmark results...")
        results = self.load_results()

        if not any(results.values()):
            print("No results found. Run benchmarks first.")
            return

        print("Generating comparison charts...")
        self.generate_response_time_chart(results)
        self.generate_throughput_chart(results)
        self.generate_error_rate_chart(results)

        print("Generating markdown report...")
        report = self.generate_markdown_report(results)

        report_path = (
            self.reports_dir
            / f"benchmark_report_{datetime.now(tz=timezone.utc).strftime('%Y%m%d_%H%M%S')}.md"
        )
        with report_path.open("w") as f:
            f.write(report)

        # Also save the latest report
        latest_path = self.reports_dir / "latest_report.md"
        with latest_path.open("w") as f:
            f.write(report)

        print(f"Analysis complete! Reports saved to {self.reports_dir}")
        print(f"- Markdown report: {report_path}")
        print("- Response time chart: response_time_comparison.png")
        print("- Throughput chart: throughput_comparison.png")
        print("- Error rate chart: error_rate_comparison.png")


if __name__ == "__main__":
    analyzer = BenchmarkAnalyzer()
    analyzer.run()
