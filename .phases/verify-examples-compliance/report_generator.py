#!/usr/bin/env python3
"""Generate human-readable compliance reports from verification results."""

import json

# Import verify module
import sys
from dataclasses import asdict
from datetime import datetime
from pathlib import Path
from typing import List

sys.path.insert(0, str(Path(__file__).parent))

from verify import ComplianceReport, ViolationReport


class ReportGenerator:
    """Generate compliance reports in various formats."""

    @staticmethod
    def generate_markdown(reports: List[ComplianceReport], output_path: Path):
        """Generate markdown compliance report."""
        with open(output_path, "w") as f:
            f.write("# FraiseQL Examples Compliance Report\n\n")
            f.write(f"**Generated:** {datetime.now().isoformat()}\n\n")

            # Summary
            total_examples = len(reports)
            fully_compliant = len([r for r in reports if len(r.errors) == 0])
            avg_score = sum(r.compliance_score for r in reports) / len(reports) if reports else 0

            f.write("## 📊 Summary\n\n")
            f.write(f"- **Total Examples**: {total_examples}\n")
            f.write(
                f"- **Fully Compliant**: {fully_compliant} ({fully_compliant / total_examples:.1%} if total_examples else 0)\n"
            )
            f.write(f"- **Average Score**: {avg_score:.1%}\n\n")

            # Compliance distribution
            score_ranges = {
                "90-100%": len([r for r in reports if r.compliance_score >= 0.9]),
                "70-89%": len([r for r in reports if 0.7 <= r.compliance_score < 0.9]),
                "50-69%": len([r for r in reports if 0.5 <= r.compliance_score < 0.7]),
                "<50%": len([r for r in reports if r.compliance_score < 0.5]),
            }

            f.write("### Compliance Distribution\n\n")
            f.write("| Score Range | Count | Percentage |\n")
            f.write("|-------------|-------|------------|\n")
            for range_name, count in score_ranges.items():
                percentage = count / total_examples * 100 if total_examples else 0
                f.write(f"| {range_name} | {count} | {percentage:.1f}% |\n")
            f.write("\n")

            # Per-example reports (sorted by compliance score)
            f.write("## 📋 Example Reports\n\n")
            for report in sorted(reports, key=lambda r: r.compliance_score, reverse=True):
                status_emoji = (
                    "✅" if len(report.errors) == 0 else "⚠️" if len(report.errors) < 5 else "❌"
                )

                f.write(f"### {status_emoji} {report.example_name}\n\n")
                f.write(f"**Score**: {report.compliance_score:.1%} | ")
                f.write(f"**Files**: {report.files_checked}/{report.total_files} | ")
                f.write(f"**Errors**: {len(report.errors)} | ")
                f.write(f"**Warnings**: {len(report.warnings)} | ")
                f.write(f"**Info**: {len(report.infos)}\n\n")

                # Show critical issues
                if report.errors:
                    f.write("**🚨 Critical Issues:**\n")
                    for error in report.errors[:10]:  # Show first 10
                        f.write(
                            f"- `[{error.rule_id}]` **{error.entity_name}**: {error.description}\n"
                        )
                        if error.example_fix:
                            f.write(f"  - 💡 *{error.example_fix}*\n")
                    f.write("\n")

                    if len(report.errors) > 10:
                        f.write(f"*... and {len(report.errors) - 10} more errors*\n\n")

                # Show warnings
                if report.warnings:
                    f.write("**⚠️ Warnings:**\n")
                    for warning in report.warnings[:5]:  # Show first 5
                        f.write(
                            f"- `[{warning.rule_id}]` **{warning.entity_name}**: {warning.description}\n"
                        )
                        if warning.example_fix:
                            f.write(f"  - 💡 *{warning.example_fix}*\n")
                    f.write("\n")

                    if len(report.warnings) > 5:
                        f.write(f"*... and {len(report.warnings) - 5} more warnings*\n\n")

                # Show improvement suggestions
                if report.infos:
                    f.write("**💡 Improvement Suggestions:**\n")
                    for info in report.infos[:3]:  # Show first 3
                        f.write(
                            f"- `[{info.rule_id}]` **{info.entity_name}**: {info.description}\n"
                        )
                        if info.example_fix:
                            f.write(f"  - 💡 *{info.example_fix}*\n")
                    f.write("\n")

                    if len(report.infos) > 3:
                        f.write(f"*... and {len(report.infos) - 3} more suggestions*\n\n")

            # Rule violation summary
            f.write("## 📈 Rule Violation Summary\n\n")

            all_violations = []
            for report in reports:
                all_violations.extend(report.violations)

            rule_counts = {}
            for violation in all_violations:
                rule_id = violation.rule_id
                if rule_id not in rule_counts:
                    rule_counts[rule_id] = {
                        "ERROR": 0,
                        "WARNING": 0,
                        "INFO": 0,
                        "name": violation.rule_name,
                    }
                rule_counts[rule_id][violation.severity] += 1

            f.write("| Rule ID | Rule Name | Errors | Warnings | Info | Total |\n")
            f.write("|---------|-----------|--------|----------|------|-------|\n")

            for rule_id, counts in sorted(rule_counts.items()):
                total = counts["ERROR"] + counts["WARNING"] + counts["INFO"]
                f.write(
                    f"| {rule_id} | {counts['name']} | {counts['ERROR']} | {counts['WARNING']} | {counts['INFO']} | {total} |\n"
                )

            f.write("\n")

            # Footer
            f.write("---\n\n")
            f.write("*Report generated by FraiseQL compliance verifier*\n")
            f.write(
                "*Rules: https://github.com/fraiseql/fraiseql/tree/main/.phases/verify-examples-compliance/rules.yaml*\n"
            )

    @staticmethod
    def generate_json(reports: List[ComplianceReport], output_path: Path):
        """Generate JSON compliance report."""
        data = {
            "metadata": {
                "generated_at": datetime.now().isoformat(),
                "total_examples": len(reports),
                "fully_compliant": len([r for r in reports if len(r.errors) == 0]),
                "average_score": sum(r.compliance_score for r in reports) / len(reports)
                if reports
                else 0,
            },
            "reports": [asdict(report) for report in reports],
        }

        with open(output_path, "w") as f:
            json.dump(data, f, indent=2, default=str)

    @staticmethod
    def generate_summary_text(reports: List[ComplianceReport]) -> str:
        """Generate a concise text summary."""
        total_examples = len(reports)
        fully_compliant = len([r for r in reports if len(r.errors) == 0])
        avg_score = sum(r.compliance_score for r in reports) / len(reports) if reports else 0

        lines = [
            "FraiseQL Compliance Summary",
            f"Generated: {datetime.now().isoformat()}",
            "",
            f"Total Examples: {total_examples}",
            f"Fully Compliant: {fully_compliant} ({fully_compliant / total_examples:.1%} if total_examples else 0)",
            f"Average Score: {avg_score:.1%}",
            "",
            "Top Issues:",
        ]

        # Count violations by rule
        rule_violations = {}
        for report in reports:
            for violation in report.violations:
                rule_id = violation.rule_id
                if rule_id not in rule_violations:
                    rule_violations[rule_id] = 0
                rule_violations[rule_id] += 1

        # Show top 5 violated rules
        top_rules = sorted(rule_violations.items(), key=lambda x: x[1], reverse=True)[:5]
        for rule_id, count in top_rules:
            lines.append(f"  {rule_id}: {count} violations")

        return "\n".join(lines)


def main():
    """Command-line interface for report generation."""
    import argparse
    import sys

    # Add current directory to path for imports
    sys.path.insert(0, str(Path(__file__).parent))

    from verify import ComplianceReport

    parser = argparse.ArgumentParser(description="Generate compliance reports")
    parser.add_argument("--input", "-i", required=True, help="JSON file with compliance reports")
    parser.add_argument("--output", "-o", required=True, help="Output file path")
    parser.add_argument(
        "--format", choices=["markdown", "json", "text"], default="markdown", help="Output format"
    )

    args = parser.parse_args()

    # Load reports from JSON
    with open(args.input) as f:
        data = json.load(f)

    # Convert back to ComplianceReport objects
    reports = []
    for report_data in data.get("reports", []):
        # Convert dict back to ComplianceReport
        violations = [ViolationReport(**v) for v in report_data["violations"]]
        report = ComplianceReport(
            example_name=report_data["example_name"],
            total_files=report_data["total_files"],
            files_checked=report_data["files_checked"],
            violations=violations,
            compliance_score=report_data["compliance_score"],
        )
        reports.append(report)

    # Generate report
    output_path = Path(args.output)

    if args.format == "markdown":
        ReportGenerator.generate_markdown(reports, output_path)
        print(f"📄 Markdown report generated: {output_path}")
    elif args.format == "json":
        ReportGenerator.generate_json(reports, output_path)
        print(f"📄 JSON report generated: {output_path}")
    elif args.format == "text":
        summary = ReportGenerator.generate_summary_text(reports)
        with open(output_path, "w") as f:
            f.write(summary)
        print(f"📄 Text summary generated: {output_path}")


if __name__ == "__main__":
    main()
