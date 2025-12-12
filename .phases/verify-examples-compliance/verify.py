#!/usr/bin/env python3
"""Main verification script for FraiseQL examples compliance.

Checks examples against Trinity pattern rules and generates compliance reports.
"""

import json
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import List, Optional

import yaml

# Import sql_analyzer (will be available at runtime)

sys.path.insert(0, str(Path(__file__).parent))

from sql_analyzer import FunctionDefinition, SQLAnalyzer, TableDefinition, ViewDefinition


@dataclass
class ViolationReport:
    """Pattern violation report."""

    rule_id: str
    rule_name: str
    severity: str  # ERROR, WARNING, INFO
    file_path: str
    line_number: Optional[int]
    violation_type: str  # table, view, function, python_type
    entity_name: str
    description: str
    example_fix: Optional[str] = None


@dataclass
class ComplianceReport:
    """Overall compliance report for an example."""

    example_name: str
    total_files: int
    files_checked: int
    violations: List[ViolationReport]
    compliance_score: float  # 0.0 to 1.0

    @property
    def errors(self) -> List[ViolationReport]:
        return [v for v in self.violations if v.severity == "ERROR"]

    @property
    def warnings(self) -> List[ViolationReport]:
        return [v for v in self.violations if v.severity == "WARNING"]

    @property
    def infos(self) -> List[ViolationReport]:
        return [v for v in self.violations if v.severity == "INFO"]


class PatternVerifier:
    """Verify examples against Trinity pattern rules."""

    def __init__(self, rules_path: Path):
        with open(rules_path) as f:
            self.rules = yaml.safe_load(f)

    def verify_table(self, table: TableDefinition, file_path: Path) -> List[ViolationReport]:
        """Verify table against Trinity pattern rules."""
        violations = []
        trinity = table.has_trinity_pattern()

        # Exception: tv_* tables don't need Trinity pattern
        if table.name.startswith("tv_"):
            return violations

        # Rule TR-001: Must have INTEGER pk_*
        if not trinity["has_pk"] or not trinity["pk_is_integer"]:
            violations.append(
                ViolationReport(
                    rule_id="TR-001",
                    rule_name="Trinity: INTEGER Primary Key",
                    severity="ERROR",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="table",
                    entity_name=table.name,
                    description=f"Table {table.name} missing INTEGER pk_* primary key",
                    example_fix="pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY",
                )
            )

        # Rule TR-002: Must have UUID id
        if not trinity["has_id_uuid"]:
            violations.append(
                ViolationReport(
                    rule_id="TR-002",
                    rule_name="Trinity: UUID Public Identifier",
                    severity="ERROR",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="table",
                    entity_name=table.name,
                    description=f"Table {table.name} missing 'id UUID DEFAULT gen_random_uuid() UNIQUE'",
                    example_fix="id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE",
                )
            )

        # Rule TR-003: May have identifier (info only)
        if not trinity["has_identifier"]:
            violations.append(
                ViolationReport(
                    rule_id="TR-003",
                    rule_name="Trinity: TEXT Identifier (Optional)",
                    severity="INFO",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="table",
                    entity_name=table.name,
                    description=f"Table {table.name} could benefit from 'identifier TEXT UNIQUE' for SEO-friendly slugs",
                    example_fix="identifier TEXT UNIQUE  -- Human-readable slug",
                )
            )

        # Rule FK-001/FK-002: Foreign keys must reference pk_* (INTEGER)
        for fk in table.foreign_keys:
            if not fk["references_column"].startswith("pk_"):
                violations.append(
                    ViolationReport(
                        rule_id="FK-001",
                        rule_name="FK: Must Reference INTEGER pk_*",
                        severity="ERROR",
                        file_path=str(file_path),
                        line_number=None,
                        violation_type="table",
                        entity_name=table.name,
                        description=f"Foreign key {fk['column']} references {fk['references_column']} (should reference pk_*)",
                        example_fix=f"{fk['column']} INTEGER REFERENCES {fk['references_table']}(pk_{fk['references_table'][3:]})",
                    )
                )

        return violations

    def verify_view(self, view: ViewDefinition, file_path: Path) -> List[ViolationReport]:
        """Verify view against JSONB pattern rules."""
        violations = []

        # Rule VW-001: Must have direct 'id' column
        if not view.has_id_column():
            violations.append(
                ViolationReport(
                    rule_id="VW-001",
                    rule_name="View: Must Expose id Column",
                    severity="ERROR",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="view",
                    entity_name=view.name,
                    description=f"View {view.name} missing direct 'id' column (needed for WHERE filtering)",
                    example_fix="SELECT id, jsonb_build_object(...) as data FROM ...",
                )
            )

        # Rule VW-003: JSONB must NOT contain pk_*
        if view.jsonb_exposes_pk():
            pk_fields = [f for f in view.jsonb_fields if f.startswith("pk_")]
            violations.append(
                ViolationReport(
                    rule_id="VW-003",
                    rule_name="JSONB: Never Expose pk_* Fields",
                    severity="ERROR",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="view",
                    entity_name=view.name,
                    description=f"View {view.name} exposes pk_* in JSONB: {pk_fields} (security violation!)",
                    example_fix="Remove pk_* from jsonb_build_object() - keep it only as direct column if needed for JOINs",
                )
            )

        # Rule VW-002: Include pk_* only if referenced (warning only)
        # Exception: Hierarchical/recursive views need pk_* for path construction
        is_hierarchical = any(
            keyword in view.name.lower()
            or any(keyword in join.get("condition", "") for join in view.joins)
            for keyword in ["recursive", "hierarchical", "tree", "path", "ltree", "comment"]
        )

        if view.has_pk_column() and not is_hierarchical:
            violations.append(
                ViolationReport(
                    rule_id="VW-002",
                    rule_name="View: Include pk_* Only If Referenced",
                    severity="WARNING",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="view",
                    entity_name=view.name,
                    description=f"View {view.name} includes pk_* column - verify other views JOIN to it",
                    example_fix="Only include pk_* if other views use it in JOIN conditions",
                )
            )

        # Rule VW-004: Must have data column
        if not view.jsonb_column:
            violations.append(
                ViolationReport(
                    rule_id="VW-004",
                    rule_name="View: Must Have data Column",
                    severity="ERROR",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="view",
                    entity_name=view.name,
                    description=f"View {view.name} missing JSONB 'data' column",
                    example_fix="SELECT ..., jsonb_build_object(...) AS data FROM ...",
                )
            )

        # Rule VW-005: JSONB must include id field
        if "id" not in view.jsonb_fields:
            violations.append(
                ViolationReport(
                    rule_id="VW-005",
                    rule_name="View: JSONB Must Include id Field",
                    severity="ERROR",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="view",
                    entity_name=view.name,
                    description=f"View {view.name} JSONB missing 'id' field",
                    example_fix="jsonb_build_object('id', id, ...)",
                )
            )

        return violations

    def verify_function(self, func: FunctionDefinition, file_path: Path) -> List[ViolationReport]:
        """Verify function against mutation pattern rules."""
        violations = []

        # Exception: core.* functions can return simple types
        is_core_function = func.schema == "core"

        # Rule MF-001: Simple vs Advanced return types
        if func.name.startswith("fn_") and func.return_type != "JSONB" and not is_core_function:
            violations.append(
                ViolationReport(
                    rule_id="MF-001",
                    rule_name="Mutation: Simple vs Advanced Return Types",
                    severity="ERROR",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="function",
                    entity_name=func.name,
                    description=f"Mutation function {func.name} should return JSONB, got {func.return_type}",
                    example_fix="RETURNS JSONB AS $$ ... RETURN jsonb_build_object('success', true, ...); $$",
                )
            )

        # Rule MF-002: Explicit sync calls for mutations
        # Exceptions: DELETE operations (CASCADE handles cleanup), tenant.* tables
        has_data_modification = (
            "INSERT" in func.body.upper()
            or "UPDATE" in func.body.upper()
            or "DELETE" in func.body.upper()
        )

        is_delete_operation = "DELETE" in func.body.upper()
        is_tenant_operation = "tenant." in func.body

        if has_data_modification and not is_delete_operation and not is_tenant_operation:
            sync_calls = func.has_explicit_sync_calls()
            if not sync_calls:
                violations.append(
                    ViolationReport(
                        rule_id="MF-002",
                        rule_name="Mutation: Explicit Sync for tv_* Tables",
                        severity="ERROR",
                        file_path=str(file_path),
                        line_number=None,
                        violation_type="function",
                        entity_name=func.name,
                        description=f"Function {func.name} modifies data but missing sync calls",
                        example_fix="PERFORM app.sync_tv_entity(); after data modifications",
                    )
                )

        # Rule MF-003: Advanced functions should use build_mutation_response
        if func.schema == "app" and "build_mutation_response" not in func.body:
            violations.append(
                ViolationReport(
                    rule_id="MF-003",
                    rule_name="Mutation: Advanced Response Format",
                    severity="INFO",
                    file_path=str(file_path),
                    line_number=None,
                    violation_type="function",
                    entity_name=func.name,
                    description=f"Advanced function {func.name} should use app.build_mutation_response()",
                    example_fix="RETURN app.build_mutation_response(true, 'SUCCESS', 'Message', jsonb_build_object(...));",
                )
            )

        # Rule HF-002: Variable naming conventions
        import re

        if "DECLARE" in func.body:
            # Check for camelCase or wrong patterns
            bad_vars = re.findall(r"\b(\w*[A-Z]\w*[a-z]\w*Id|\w+Id|\w+_ID)\b", func.body)
            bad_vars = [v for v in bad_vars if not v.startswith("v_") and not v.startswith("p_")]
            if bad_vars:
                violations.append(
                    ViolationReport(
                        rule_id="HF-002",
                        rule_name="Variables: Follow Naming Convention",
                        severity="WARNING",
                        file_path=str(file_path),
                        line_number=None,
                        violation_type="function",
                        entity_name=func.name,
                        description=f"Function {func.name} has non-standard variable names: {set(bad_vars)}",
                        example_fix="Use v_<entity>_id (UUID), v_<entity>_pk (INTEGER), p_<entity>_id (parameter)",
                    )
                )

        return violations

    def verify_sql_file(self, sql_file: Path) -> List[ViolationReport]:
        """Verify a single SQL file."""
        try:
            analyzer = SQLAnalyzer(sql_file)
            violations = []

            # Check tables
            for table in analyzer.extract_tables():
                violations.extend(self.verify_table(table, sql_file))

            # Check views
            for view in analyzer.extract_views():
                violations.extend(self.verify_view(view, sql_file))

            # Check functions
            for func in analyzer.extract_functions():
                violations.extend(self.verify_function(func, sql_file))

            return violations
        except Exception as e:
            # Return a parsing error violation
            return [
                ViolationReport(
                    rule_id="PARSE-ERROR",
                    rule_name="SQL Parsing Error",
                    severity="ERROR",
                    file_path=str(sql_file),
                    line_number=None,
                    violation_type="file",
                    entity_name=str(sql_file),
                    description=f"Failed to parse SQL file: {e!s}",
                    example_fix="Check SQL syntax and file structure",
                )
            ]

    def verify_example(self, example_dir: Path) -> ComplianceReport:
        """Verify entire example directory."""
        sql_files = list(example_dir.rglob("*.sql"))
        all_violations = []

        for sql_file in sql_files:
            violations = self.verify_sql_file(sql_file)
            all_violations.extend(violations)

        # Calculate compliance score
        total_checks = len(sql_files) * 10  # Rough estimate per file
        error_penalty = len([v for v in all_violations if v.severity == "ERROR"]) * 5
        warning_penalty = len([v for v in all_violations if v.severity == "WARNING"]) * 1

        score = max(0.0, 1.0 - (error_penalty + warning_penalty) / max(total_checks, 1))

        return ComplianceReport(
            example_name=example_dir.name,
            total_files=len(sql_files),
            files_checked=len(sql_files),
            violations=all_violations,
            compliance_score=score,
        )


def print_report(report: ComplianceReport, verbose: bool = False):
    """Print a compliance report to console."""
    print(f"\n🔍 Compliance Report: {report.example_name}")
    print(f"📊 Score: {report.compliance_score:.1%}")
    print(f"📁 Files: {report.files_checked}/{report.total_files}")
    print(f"❌ Errors: {len(report.errors)}")
    print(f"⚠️  Warnings: {len(report.warnings)}")
    print(f"ℹ️  Info: {len(report.infos)}")

    if verbose and report.violations:
        print("\n🚨 Violations:")

        # Group by severity
        for severity, emoji in [("ERROR", "❌"), ("WARNING", "⚠️"), ("INFO", "ℹ️")]:
            violations = [v for v in report.violations if v.severity == severity]
            if violations:
                print(f"\n{emoji} {severity} ({len(violations)}):")
                for v in violations[:5]:  # Show first 5
                    print(f"  [{v.rule_id}] {v.entity_name}: {v.description}")
                    if v.example_fix:
                        print(f"    💡 {v.example_fix}")

                if len(violations) > 5:
                    print(f"    ... and {len(violations) - 5} more")


def main():
    """Command-line interface."""
    import argparse

    parser = argparse.ArgumentParser(
        description="Verify FraiseQL examples against Trinity patterns"
    )
    parser.add_argument("example_path", help="Path to example directory or SQL file")
    parser.add_argument(
        "--rules",
        default=".phases/verify-examples-compliance/rules.yaml",
        help="Path to rules YAML file",
    )
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    parser.add_argument("--json", action="store_true", help="Output JSON report")

    args = parser.parse_args()

    example_path = Path(args.example_path)
    rules_path = Path(args.rules)

    if not rules_path.exists():
        print(f"❌ Rules file not found: {rules_path}")
        sys.exit(1)

    verifier = PatternVerifier(rules_path)

    if example_path.is_file() and example_path.suffix == ".sql":
        # Verify single SQL file
        violations = verifier.verify_sql_file(example_path)
        report = ComplianceReport(
            example_name=example_path.name,
            total_files=1,
            files_checked=1,
            violations=violations,
            compliance_score=1.0 if not violations else 0.5,
        )
    elif example_path.is_dir():
        # Verify example directory
        report = verifier.verify_example(example_path)
    else:
        print(f"❌ Invalid path: {example_path}")
        sys.exit(1)

    if args.json:
        print(json.dumps(asdict(report), indent=2, default=str))
    else:
        print_report(report, args.verbose)


if __name__ == "__main__":
    main()
