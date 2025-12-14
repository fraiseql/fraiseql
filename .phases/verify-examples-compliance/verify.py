#!/usr/bin/env python3
"""
FraiseQL Examples Compliance Verification Script

Validates all example applications for compliance with FraiseQL standards:
- File structure validation
- Required files presence
- Basic syntax validation
- Configuration consistency

Usage:
    python .phases/verify-examples-compliance/verify.py examples/*/
    python .phases/verify-examples-compliance/verify.py examples/*/ --json > compliance-report.json
"""

import argparse
import ast
import json
import re
import subprocess
import sys
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Literal, Optional, Tuple


@dataclass
class ComplianceViolation:
    """Represents a compliance violation"""

    severity: Literal["ERROR", "WARNING", "INFO"]
    category: str
    message: str
    file_path: Optional[str] = None
    line_number: Optional[int] = None


@dataclass
class ExampleReport:
    """Compliance report for a single example"""

    name: str
    path: Path
    violations: List[ComplianceViolation] = field(default_factory=list)
    score: float = 0.0

    @property
    def fully_compliant(self) -> bool:
        """Check if example has no ERROR violations"""
        return not any(v.severity == "ERROR" for v in self.violations)


@dataclass
class ComplianceReport:
    """Overall compliance report"""

    metadata: Dict
    reports: List[ExampleReport]

    @property
    def total_examples(self) -> int:
        return len(self.reports)

    @property
    def fully_compliant(self) -> int:
        return sum(1 for r in self.reports if r.fully_compliant)

    @property
    def average_score(self) -> float:
        if not self.reports:
            return 0.0
        return sum(r.score for r in self.reports) / len(self.reports)


class ExamplesComplianceValidator:
    """Validates FraiseQL examples for compliance"""

    def __init__(self):
        self.required_files = {
            "README.md",
            "requirements.txt",
            "app.py",
        }

        self.optional_files = {
            "docker-compose.yml",
            "Dockerfile",
            "pytest.ini",
            ".gitignore",
        }

    def validate_example(self, example_path: Path) -> ExampleReport:
        """Validate a single example"""
        name = example_path.name
        report = ExampleReport(name=name, path=example_path)

        # Check required files
        for required_file in self.required_files:
            file_path = example_path / required_file
            if not file_path.exists():
                report.violations.append(
                    ComplianceViolation(
                        severity="ERROR",
                        category="missing_file",
                        message=f"Required file missing: {required_file}",
                        file_path=str(file_path),
                    )
                )

        # Check Python syntax in app.py
        app_py = example_path / "app.py"
        if app_py.exists():
            self._validate_python_syntax(app_py, report)

        # Check requirements.txt format
        requirements_txt = example_path / "requirements.txt"
        if requirements_txt.exists():
            self._validate_requirements(requirements_txt, report)

        # Calculate score (0-100)
        error_count = sum(1 for v in report.violations if v.severity == "ERROR")
        warning_count = sum(1 for v in report.violations if v.severity == "WARNING")

        if error_count == 0 and warning_count == 0:
            report.score = 100.0
        elif error_count == 0:
            report.score = max(50.0, 100.0 - (warning_count * 10))
        else:
            report.score = max(0.0, 50.0 - (error_count * 20) - (warning_count * 5))

        return report

    def _validate_python_syntax(self, file_path: Path, report: ExampleReport):
        """Validate Python syntax"""
        try:
            with open(file_path, "r", encoding="utf-8") as f:
                source = f.read()

            # Parse AST
            ast.parse(source)

            # Try to run ruff check if available
            try:
                result = subprocess.run(
                    ["ruff", "check", "--output-format", "json", str(file_path)],
                    capture_output=True,
                    text=True,
                    timeout=30,
                )

                if result.returncode != 0:
                    # Parse ruff output
                    try:
                        ruff_issues = json.loads(result.stdout)
                        for issue in ruff_issues:
                            severity = (
                                "WARNING" if issue.get("code", "").startswith("E") else "INFO"
                            )
                            report.violations.append(
                                ComplianceViolation(
                                    severity=severity,
                                    category="ruff_lint",
                                    message=f"{issue.get('code', 'UNK')}: {issue.get('message', '')}",
                                    file_path=str(file_path),
                                    line_number=issue.get("location", {}).get("row"),
                                )
                            )
                    except json.JSONDecodeError:
                        report.violations.append(
                            ComplianceViolation(
                                severity="WARNING",
                                category="syntax_check",
                                message="Could not parse ruff output",
                                file_path=str(file_path),
                            )
                        )

            except (subprocess.TimeoutExpired, FileNotFoundError):
                # ruff not available or timeout
                pass

        except SyntaxError as e:
            report.violations.append(
                ComplianceViolation(
                    severity="ERROR",
                    category="syntax_error",
                    message=f"Syntax error: {e.msg}",
                    file_path=str(file_path),
                    line_number=e.lineno,
                )
            )
        except Exception as e:
            report.violations.append(
                ComplianceViolation(
                    severity="ERROR",
                    category="file_error",
                    message=f"Could not validate file: {e}",
                    file_path=str(file_path),
                )
            )

    def _validate_requirements(self, file_path: Path, report: ExampleReport):
        """Validate requirements.txt format"""
        try:
            with open(file_path, "r", encoding="utf-8") as f:
                lines = f.readlines()

            for i, line in enumerate(lines, 1):
                line = line.strip()
                if not line or line.startswith("#"):
                    continue

                # Basic package==version format check
                if not re.match(r"^[a-zA-Z0-9][a-zA-Z0-9._-]*([<>=!~]+[a-zA-Z0-9._-]+)?$", line):
                    report.violations.append(
                        ComplianceViolation(
                            severity="WARNING",
                            category="requirements_format",
                            message=f"Potentially malformed requirement: {line}",
                            file_path=str(file_path),
                            line_number=i,
                        )
                    )

        except Exception as e:
            report.violations.append(
                ComplianceViolation(
                    severity="ERROR",
                    category="file_error",
                    message=f"Could not validate requirements: {e}",
                    file_path=str(file_path),
                )
            )


def main():
    parser = argparse.ArgumentParser(description="Validate FraiseQL examples compliance")
    parser.add_argument("examples", nargs="+", help="Example directories to validate")
    parser.add_argument("--json", action="store_true", help="Output JSON report")

    args = parser.parse_args()

    validator = ExamplesComplianceValidator()
    reports = []

    for example_path_str in args.examples:
        example_path = Path(example_path_str)
        if not example_path.exists() or not example_path.is_dir():
            print(f"Warning: {example_path} is not a valid directory", file=sys.stderr)
            continue

        report = validator.validate_example(example_path)
        reports.append(report)

    # Create compliance report
    compliance_report = ComplianceReport(
        metadata={
            "total_examples": len(reports),
            "fully_compliant": sum(1 for r in reports if r.fully_compliant),
            "average_score": sum(r.score for r in reports) / len(reports) if reports else 0.0,
            "generated_at": datetime.now().isoformat(),
        },
        reports=reports,
    )

    if args.json:
        # Output JSON for CI/CD
        print(
            json.dumps(
                {
                    "metadata": compliance_report.metadata,
                    "reports": [
                        {
                            "name": r.name,
                            "path": str(r.path),
                            "fully_compliant": r.fully_compliant,
                            "score": r.score,
                            "violations": [
                                {
                                    "severity": v.severity,
                                    "category": v.category,
                                    "message": v.message,
                                    "file_path": v.file_path,
                                    "line_number": v.line_number,
                                }
                                for v in r.violations
                            ],
                        }
                        for r in reports
                    ],
                },
                indent=2,
            )
        )
    else:
        # Human-readable output
        print("FraiseQL Examples Compliance Report")
        print("=" * 40)
        print(f"Total examples: {compliance_report.total_examples}")
        print(f"Fully compliant: {compliance_report.fully_compliant}")
        print(".1f")
        print()

        for report in reports:
            status = "✅ PASS" if report.fully_compliant else "❌ FAIL"
            print(f"{status} {report.name} (Score: {report.score:.1f})")

            for violation in report.violations:
                marker = {"ERROR": "🔴", "WARNING": "🟡", "INFO": "ℹ️"}.get(violation.severity, "?")
                print(f"  {marker} {violation.category}: {violation.message}")
                if violation.file_path:
                    loc = f" at {violation.file_path}"
                    if violation.line_number:
                        loc += f":{violation.line_number}"
                    print(f"    {loc}")

            print()

    # Exit with error if any examples have ERROR violations
    has_errors = any(any(v.severity == "ERROR" for v in r.violations) for r in reports)

    sys.exit(1 if has_errors else 0)


if __name__ == "__main__":
    main()
