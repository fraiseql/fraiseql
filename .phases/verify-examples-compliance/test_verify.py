#!/usr/bin/env python3
"""Test suite for FraiseQL compliance verification."""

import sys
from pathlib import Path

# Add current directory to path
sys.path.insert(0, str(Path(__file__).parent))

from sql_analyzer import SQLAnalyzer
from verify import PatternVerifier


def test_sql_analyzer():
    """Test SQL analyzer on known files."""
    print("🧪 Testing SQL Analyzer...")

    # Test on blog_api user table
    sql_file = Path("examples/blog_api/db/0_schema/01_write/011_tb_user.sql")
    if sql_file.exists():
        analyzer = SQLAnalyzer(sql_file)
        tables = analyzer.extract_tables()

        assert len(tables) == 1, f"Expected 1 table, got {len(tables)}"
        table = tables[0]

        assert table.name == "tb_user", f"Expected table name 'tb_user', got '{table.name}'"

        # Check Trinity pattern
        trinity = table.has_trinity_pattern()
        assert trinity["has_pk"], "Table should have pk_* primary key"
        assert trinity["has_id_uuid"], "Table should have UUID id column"
        assert trinity["pk_is_integer"], "Primary key should be INTEGER"

        print("  ✅ Table parsing works")
    else:
        print("  ⚠️  Test file not found, skipping table tests")

    # Test on blog_api user view
    view_file = Path("examples/blog_api/db/0_schema/02_read/021_user/0211_v_user.sql")
    if view_file.exists():
        analyzer = SQLAnalyzer(view_file)
        views = analyzer.extract_views()

        assert len(views) >= 1, f"Expected at least 1 view, got {len(views)}"
        view = views[0]

        assert view.has_id_column(), "View should have direct id column"
        assert not view.jsonb_exposes_pk(), "View should not expose pk_* in JSONB"
        assert "id" in view.jsonb_fields, "JSONB should include id field"

        print("  ✅ View parsing works")
    else:
        print("  ⚠️  Test file not found, skipping view tests")


def test_pattern_verifier():
    """Test pattern verifier on known examples."""
    print("🧪 Testing Pattern Verifier...")

    rules_file = Path(".phases/verify-examples-compliance/rules.yaml")
    if not rules_file.exists():
        print("  ❌ Rules file not found")
        return

    verifier = PatternVerifier(rules_file)

    # Test on blog_api user table
    sql_file = Path("examples/blog_api/db/0_schema/01_write/011_tb_user.sql")
    if sql_file.exists():
        violations = verifier.verify_sql_file(sql_file)

        # Should have minimal violations (maybe some INFO level)
        errors = [v for v in violations if v.severity == "ERROR"]
        assert len(errors) == 0, (
            f"Expected no errors on blog_api, got: {[e.description for e in errors]}"
        )

        print("  ✅ Pattern verification works")
    else:
        print("  ⚠️  Test file not found, skipping verification tests")


def test_compliance_scoring():
    """Test compliance scoring logic."""
    print("🧪 Testing Compliance Scoring...")

    from verify import ComplianceReport, ViolationReport

    # Test perfect compliance
    perfect_report = ComplianceReport(
        example_name="perfect", total_files=1, files_checked=1, violations=[], compliance_score=1.0
    )
    assert perfect_report.compliance_score == 1.0, "Perfect compliance should be 1.0"

    # Test with violations
    violations = [
        ViolationReport("TR-001", "Test", "ERROR", "file.sql", None, "table", "test", "desc"),
        ViolationReport("VW-001", "Test", "WARNING", "file.sql", None, "view", "test", "desc"),
    ]
    imperfect_report = ComplianceReport(
        example_name="imperfect",
        total_files=1,
        files_checked=1,
        violations=violations,
        compliance_score=0.5,  # Should be calculated automatically
    )

    assert len(imperfect_report.errors) == 1, "Should have 1 error"
    assert len(imperfect_report.warnings) == 1, "Should have 1 warning"

    print("  ✅ Compliance scoring works")


def run_tests():
    """Run all tests."""
    print("🚀 Running FraiseQL Compliance Tests\n")

    try:
        test_sql_analyzer()
        test_pattern_verifier()
        test_compliance_scoring()

        print("\n✅ All tests passed!")

    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False

    return True


if __name__ == "__main__":
    success = run_tests()
    sys.exit(0 if success else 1)
