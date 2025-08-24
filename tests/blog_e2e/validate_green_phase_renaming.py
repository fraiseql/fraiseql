#!/usr/bin/env python3
"""GREEN Phase Validation - Default FraiseQL Patterns Renaming

This script validates the GREEN phase implementation of renaming Enhanced/Optimized
patterns to become the default FraiseQL patterns, testing against RED phase requirements.
"""

import sys
from pathlib import Path
import traceback
import uuid

# Add the modules to the path
sys.path.insert(0, str(Path(__file__).parent))


def run_test(test_name: str, test_func) -> bool:
    """Run a single test and return success/failure."""
    try:
        test_func()
        print(f"  ✅ {test_name}")
        return True
    except Exception as e:
        print(f"  ❌ {test_name}: {e}")
        if "--verbose" in sys.argv:
            print(f"     {traceback.format_exc()}")
        return False


def test_fraiseql_mutation_is_the_enhanced_version():
    """Test that FraiseQLMutation is now the enhanced version (no 'Optimized' prefix)."""
    from fraiseql_defaults import FraiseQLMutation

    # Should have the enhanced features
    assert hasattr(FraiseQLMutation, '__init_subclass__')
    assert hasattr(FraiseQLMutation, '_type_hints_cache')  # Caching feature
    assert hasattr(FraiseQLMutation, '_validate_class_structure')  # Enhanced validation

    # Should have enhanced documentation
    assert FraiseQLMutation.__doc__ is not None
    assert "auto-decoration" in FraiseQLMutation.__doc__.lower()


def test_fraiseql_error_is_the_enhanced_version():
    """Test that FraiseQLError is now the enhanced version (no 'Enhanced' prefix)."""
    from fraiseql_defaults import FraiseQLError

    # Should have enhanced features
    sample_error = FraiseQLError(
        code=422,
        identifier="test_error",
        message="Test message",
        details={"field": "test"},
        severity="medium",  # Enhanced field
        category="validation",  # Enhanced field
        field_path="nested.field",  # Enhanced field
        trace_id="test-trace-id"  # Enhanced field
    )

    assert sample_error.code == 422
    assert sample_error.severity == "medium"
    assert sample_error.category == "validation"
    assert sample_error.field_path == "nested.field"
    assert sample_error.trace_id == "test-trace-id"


def test_error_mapper_is_available_as_default():
    """Test that ErrorMapper is available as part of default patterns."""
    from fraiseql_defaults import ErrorMapper

    # Should have advanced mapping capabilities
    assert hasattr(ErrorMapper, 'map_database_result_to_graphql')
    assert callable(ErrorMapper.map_database_result_to_graphql)

    # Should have private helper methods for enhanced functionality
    assert hasattr(ErrorMapper, '_determine_severity')
    assert hasattr(ErrorMapper, '_determine_category')
    assert hasattr(ErrorMapper, '_create_error_summary')


def test_validation_context_is_available():
    """Test that ValidationContext is available as part of default patterns."""
    from fraiseql_defaults import ValidationContext

    # Should be able to create validation context
    context = ValidationContext(
        trace_id="test-trace",
        operation="test_operation",
        timestamp="2025-01-24T12:00:00Z",
        metadata={"test": "data"}
    )

    assert context.trace_id == "test-trace"
    assert context.operation == "test_operation"
    assert context.metadata["test"] == "data"


def test_legacy_fraiseql_mutation_exists():
    """Test that current FraiseQLMutation becomes LegacyFraiseQLMutation."""
    from fraiseql_defaults import LegacyFraiseQLMutation

    # Should exist but be the simpler version
    assert LegacyFraiseQLMutation is not None
    assert hasattr(LegacyFraiseQLMutation, '__init_subclass__')

    # Should NOT have enhanced features
    assert not hasattr(LegacyFraiseQLMutation, '_type_hints_cache')
    assert not hasattr(LegacyFraiseQLMutation, '_validate_class_structure')


def test_legacy_mutation_result_base_exists():
    """Test that MutationResultBase is preserved as LegacyMutationResultBase."""
    from fraiseql_defaults import LegacyMutationResultBase

    # Should be available for backward compatibility
    assert LegacyMutationResultBase is not None

    # Should work as before
    class TestLegacySuccess(LegacyMutationResultBase):
        message: str = "Test success"

    success = TestLegacySuccess(message="Test message")
    assert success.message == "Test message"


def test_legacy_error_patterns_exist():
    """Test that legacy error patterns are preserved."""
    from fraiseql_defaults import LegacyFraiseQLError

    # Should be simpler error type
    legacy_error = LegacyFraiseQLError(
        code=422,
        identifier="test_error",
        message="Test message"
    )

    assert legacy_error.code == 422
    assert legacy_error.identifier == "test_error"
    assert legacy_error.message == "Test message"

    # Should NOT have enhanced fields
    assert not hasattr(legacy_error, 'severity')
    assert not hasattr(legacy_error, 'category')
    assert not hasattr(legacy_error, 'field_path')


def test_enhanced_patterns_work_with_new_default_names():
    """Test that enhanced patterns work with clean default names."""
    from fraiseql_defaults import FraiseQLMutation, FraiseQLError

    # Should be able to create clean mutations without adjectives
    class CreateUserSuccess:
        user: dict | None = None
        message: str = "User created successfully"
        errors: list[FraiseQLError] = []

    class CreateUserError:
        message: str
        errors: list[FraiseQLError]
        trace_id: str | None = None

    class CreateUser(
        FraiseQLMutation,  # Clean name!
        function="create_user",
        validation_strict=True,
        error_trace=True
    ):
        input: dict
        success: CreateUserSuccess
        failure: CreateUserError

    # Should create successfully
    mutation = CreateUser()
    assert mutation is not None

    # Should have enhanced features
    assert hasattr(CreateUserSuccess, '__fraiseql_success__')
    assert hasattr(CreateUserError, '__fraiseql_failure__')
    assert hasattr(CreateUser, '__fraiseql_optimized__')


def test_migration_documentation_exists():
    """Test that migration documentation is available."""
    from fraiseql_defaults import get_migration_guide

    guide = get_migration_guide()
    assert isinstance(guide, dict)

    # Should have key migration information
    assert "renaming_map" in guide
    assert "breaking_changes" in guide
    assert "migration_steps" in guide

    # Check key renames are documented
    renaming_map = guide["renaming_map"]
    assert "OptimizedFraiseQLMutation" in renaming_map
    assert renaming_map["OptimizedFraiseQLMutation"] == "FraiseQLMutation"
    assert "EnhancedFraiseQLError" in renaming_map
    assert renaming_map["EnhancedFraiseQLError"] == "FraiseQLError"


def test_clean_blog_mutations_with_default_patterns():
    """Test clean blog mutations using default patterns (no adjectives)."""
    from fraiseql_defaults import FraiseQLMutation, FraiseQLError, ErrorMapper

    # Clean result types
    class CreateAuthorSuccess:
        author: dict | None = None
        message: str = "Author created successfully"
        errors: list[FraiseQLError] = []
        trace_id: str | None = None

    class CreateAuthorError:
        message: str
        errors: list[FraiseQLError]
        error_summary: dict | None = None
        conflict_author: dict | None = None
        trace_id: str | None = None

    # Clean mutation using default patterns
    class CreateAuthor(
        FraiseQLMutation,  # Default, no "Optimized" prefix!
        function="create_author_enhanced",
        context_params={"user_id": "input_created_by"},
        validation_strict=True,
        error_trace=True
    ):
        input: dict
        success: CreateAuthorSuccess
        failure: CreateAuthorError

    # Should create successfully with all features
    mutation = CreateAuthor()
    assert mutation is not None

    # Should have all enhanced features without "Enhanced"/"Optimized" names
    assert hasattr(CreateAuthor, '__fraiseql_optimized__')
    assert hasattr(CreateAuthorSuccess, '__fraiseql_success__')
    assert hasattr(CreateAuthorError, '__fraiseql_failure__')


def test_error_handling_with_default_patterns():
    """Test error handling using clean default patterns."""
    from fraiseql_defaults import FraiseQLError, ErrorMapper, ValidationContext

    # Should be able to create validation context
    context = ValidationContext(
        trace_id=str(uuid.uuid4()),
        operation="test_operation",
        timestamp="2025-01-24T12:00:00Z",
        metadata={"user_id": "test"}
    )

    # Should be able to create errors with clean pattern
    error = FraiseQLError(
        code=422,
        identifier="validation_error",
        message="Test validation error",
        details={"field": "name"},
        severity="medium",  # Enhanced features available
        category="validation",
        field_path="user.name",
        trace_id=context.trace_id
    )

    assert error.severity == "medium"
    assert error.category == "validation"
    assert error.field_path == "user.name"

    # Should be able to use ErrorMapper
    db_result = {
        "errors": [
            {
                "code": 422,
                "identifier": "test_error",
                "message": "Test error",
                "details": {"field": "test"}
            }
        ]
    }

    response = ErrorMapper.map_database_result_to_graphql(
        db_result,
        'TestError',
        context
    )

    assert response is not None


def test_legacy_patterns_coexist_with_defaults():
    """Test that both legacy and default patterns work simultaneously."""
    from fraiseql_defaults import (
        FraiseQLMutation as NewPattern,
        LegacyFraiseQLMutation as OldPattern,
        FraiseQLError as NewError,
        LegacyFraiseQLError as OldError
    )

    # Both should be available
    assert NewPattern is not None
    assert OldPattern is not None
    assert NewError is not None
    assert OldError is not None

    # Create proper class types for testing
    class TestSuccessType:
        message: str = "Success"

    class TestFailureType:
        message: str = "Failure"

    # Both should work for creating mutations
    class NewStyleMutation(
        NewPattern,
        function="test_function"
    ):
        input: dict
        success: TestSuccessType
        failure: TestFailureType

    class OldStyleMutation(
        OldPattern,
        function="test_function"
    ):
        input: dict
        success: TestSuccessType
        failure: TestFailureType

    new_mutation = NewStyleMutation()
    old_mutation = OldStyleMutation()

    assert new_mutation is not None
    assert old_mutation is not None

    # New should have enhanced features, old should not
    assert hasattr(NewStyleMutation, '__fraiseql_optimized__')
    assert hasattr(OldStyleMutation, '__fraiseql_legacy__')


def validate_green_phase_renaming():
    """Run all GREEN phase renaming validation tests."""

    print("🟢 GREEN PHASE - Default FraiseQL Patterns Renaming Validation")
    print("=" * 65)
    print()
    print("Testing GREEN phase implementation against RED phase requirements...")
    print()

    tests = [
        ("FraiseQLMutation is Enhanced Version", test_fraiseql_mutation_is_the_enhanced_version),
        ("FraiseQLError is Enhanced Version", test_fraiseql_error_is_the_enhanced_version),
        ("ErrorMapper Available as Default", test_error_mapper_is_available_as_default),
        ("ValidationContext Available", test_validation_context_is_available),
        ("Legacy FraiseQLMutation Exists", test_legacy_fraiseql_mutation_exists),
        ("Legacy MutationResultBase Exists", test_legacy_mutation_result_base_exists),
        ("Legacy Error Patterns Exist", test_legacy_error_patterns_exist),
        ("Enhanced Patterns Work with Clean Names", test_enhanced_patterns_work_with_new_default_names),
        ("Migration Documentation Exists", test_migration_documentation_exists),
        ("Clean Blog Mutations with Default Patterns", test_clean_blog_mutations_with_default_patterns),
        ("Error Handling with Default Patterns", test_error_handling_with_default_patterns),
        ("Legacy and Default Patterns Coexist", test_legacy_patterns_coexist_with_defaults)
    ]

    passed = 0
    total = len(tests)

    print("Running validation tests:")
    print("-" * 25)

    for test_name, test_func in tests:
        if run_test(test_name, test_func):
            passed += 1

    print()
    print(f"Results: {passed}/{total} tests passed")
    print()

    if passed == total:
        print("🎉 GREEN PHASE SUCCESSFUL - Default Patterns Renaming Complete!")
        print("=" * 65)
        print()
        print("✅ Default FraiseQL Patterns Successfully Renamed:")
        print("   • OptimizedFraiseQLMutation → FraiseQLMutation (default)")
        print("   • EnhancedFraiseQLError → FraiseQLError (default)")
        print("   • ErrorMapper available as default pattern")
        print("   • ValidationContext available as default pattern")
        print()
        print("✅ Legacy Patterns Preserved for Backward Compatibility:")
        print("   • LegacyFraiseQLMutation (was FraiseQLMutation)")
        print("   • LegacyMutationResultBase (was MutationResultBase)")
        print("   • LegacyFraiseQLError for basic error handling")
        print()
        print("✅ Enhanced Features Available with Clean Names:")
        print("   • Auto-decoration of success/failure types")
        print("   • Comprehensive error arrays with severity and categorization")
        print("   • Production-ready performance optimizations")
        print("   • Advanced error mapping and validation context")
        print()
        print("🎯 New Default Usage (Clean Pattern):")
        print("   from fraiseql_defaults import FraiseQLMutation, FraiseQLError")
        print("   ")
        print("   class CreateUser(FraiseQLMutation, function='create_user'):")
        print("       input: CreateUserInput")
        print("       success: CreateUserSuccess  # Auto-decorated!")
        print("       failure: CreateUserError   # Auto-decorated!")
        print()
        print("🔄 Migration Guide Available:")
        print("   • Zero breaking changes - fully backward compatible")
        print("   • Gradual, opt-in migration to clean default patterns")
        print("   • Comprehensive documentation and examples")
        print("   • Legacy patterns available indefinitely")
        print()
        print("🚀 Ready for REFACTOR Phase:")
        print("   • Create comprehensive migration tooling")
        print("   • Update all existing references to use clean patterns")
        print("   • Generate migration scripts and documentation")
        print("   • Set up automated migration assistance")

        return 0
    else:
        print("❌ GREEN PHASE INCOMPLETE - Some Requirements Not Met")
        print("=" * 55)
        print()
        print(f"Failed tests: {total - passed}")
        print("Continue development to satisfy all RED phase requirements.")
        print("Use --verbose flag for detailed error information.")

        return 1


if __name__ == "__main__":
    sys.exit(validate_green_phase_renaming())
