#!/usr/bin/env python3
"""FraiseQL Examples Compliance Report Generator

Generates human-readable reports from compliance JSON data.
"""

import argparse
import json
from pathlib import Path


def generate_markdown_report(data: dict) -> str:
    """Generate a markdown report from compliance data."""
    metadata = data["metadata"]
    reports = data["reports"]

    lines = []

    # Header
    lines.append("# FraiseQL Examples Compliance Report")
    lines.append("")
    lines.append(f"**Generated:** {metadata['generated_at']}")
    lines.append("")

    # Summary
    lines.append("## 📊 Summary")
    lines.append("")
    lines.append(f"- **Total Examples:** {metadata['total_examples']}")
    lines.append(f"- **Fully Compliant:** {metadata['fully_compliant']}")
    lines.append(".1f")
    lines.append("")

    # Compliance status
    compliant_count = sum(1 for r in reports if r["fully_compliant"])
    if compliant_count == metadata["total_examples"]:
        lines.append("✅ **All examples are compliant!**")
    else:
        lines.append(f"⚠️ **{compliant_count}/{metadata['total_examples']} examples are compliant**")
    lines.append("")

    # Detailed results
    lines.append("## 📋 Detailed Results")
    lines.append("")

    for report in sorted(reports, key=lambda x: (not x["fully_compliant"], x["name"])):
        status = "✅" if report["fully_compliant"] else "❌"
        lines.append(f"### {status} {report['name']}")
        lines.append("")
        lines.append(".1f")
        lines.append("")

        if report["violations"]:
            lines.append("**Violations:**")
            lines.append("")
            for violation in report["violations"]:
                marker = {"ERROR": "🔴", "WARNING": "🟡", "INFO": "i"}.get(
                    violation["severity"], "?"
                )
                lines.append(f"- {marker} **{violation['category']}**: {violation['message']}")
                if violation.get("file_path"):
                    loc = violation["file_path"]
                    if violation.get("line_number"):
                        loc += f":{violation['line_number']}"
                    lines.append(f"  - *{loc}*")
            lines.append("")

    return "\n".join(lines)


def main() -> None:
    """Main entry point for the report generator."""
    parser = argparse.ArgumentParser(description="Generate compliance reports")
    parser.add_argument("--input", required=True, help="Input JSON file")
    parser.add_argument("--output", required=True, help="Output file")
    parser.add_argument("--format", choices=["markdown"], default="markdown", help="Output format")

    args = parser.parse_args()

    # Read input JSON
    with Path(args.input).open() as f:
        data = json.load(f)

    # Generate report
    if args.format == "markdown":
        report = generate_markdown_report(data)
    else:
        raise ValueError(f"Unsupported format: {args.format}")

    # Write output
    with Path(args.output).open("w") as f:
        f.write(report)

    print(f"Report generated: {args.output}")  # noqa: T201


if __name__ == "__main__":
    main()
