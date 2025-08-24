"""RED Phase - Failing E2E Tests for Error Arrays

This test suite implements the RED phase of micro TDD for error arrays,
demonstrating the INTENDED architecture where multiple validation errors
are returned as structured arrays following PrintOptim Backend patterns.

ALL TESTS IN THIS FILE SHOULD FAIL INITIALLY because:
- The GraphQL mutations don't exist yet
- The PostgreSQL functions don't exist yet
- The error array structure isn't implemented yet
- The comprehensive validation logic isn't built yet

This defines the EXPECTED behavior for error arrays before implementation.
"""

import pytest
import pytest_asyncio


class TestRedPhaseMultipleValidationErrorArrays:
    """RED: Test comprehensive validation error arrays (will fail initially)."""

    @pytest_asyncio.fixture(autouse=True)
    async def setup(self, clean_database):
        """Ensure clean state for each test."""
        pass

    async def test_create_author_multiple_missing_fields_returns_error_array(self, graphql_client):
        """RED: Test that multiple missing fields return as structured error array."""
        mutation = """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) {
                    __typename
                    ... on CreateAuthorEnhancedSuccess {
                        author { id identifier name email }
                        message
                        errors
                    }
                    ... on CreateAuthorEnhancedError {
                        message
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        validationSummary {
                            totalErrors
                            hasValidationErrors
                            hasConflicts
                            fieldErrors
                            constraintViolations
                        }
                    }
                }
            }
        """

        # This will FAIL because CreateAuthorEnhanced mutation doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    # Missing ALL required fields: identifier, name, email
                    "bio": "This author is missing required fields"
                }
            }
        )

        # Expected behavior (will fail until implementation):
        assert result["data"]["createAuthorEnhanced"]["__typename"] == "CreateAuthorEnhancedError"
        error_response = result["data"]["createAuthorEnhanced"]

        # Should return ARRAY of errors, not single error
        errors = error_response["errors"]
        assert isinstance(errors, list)
        assert len(errors) == 3  # identifier, name, email

        # Each error should follow PrintOptim Backend structure
        for error in errors:
            assert error["code"] == 422  # Unprocessable Entity
            assert error["identifier"] == "missing_required_field"
            assert "Missing required field:" in error["message"]
            assert error["details"]["constraint"] == "required"
            assert error["details"]["field"] in ["identifier", "name", "email"]

        # Should include validation summary
        summary = error_response["validationSummary"]
        assert summary["totalErrors"] == 3
        assert summary["hasValidationErrors"] is True
        assert summary["hasConflicts"] is False
        assert len(summary["fieldErrors"]) == 3
        assert summary["constraintViolations"]["required"] == 3

    async def test_create_author_mixed_validation_types_returns_structured_array(self, graphql_client):
        """RED: Test different validation error types in structured array."""
        mutation = """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) {
                    __typename
                    ... on CreateAuthorEnhancedError {
                        message
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        validationSummary {
                            totalErrors
                            constraintViolations
                        }
                    }
                }
            }
        """

        # This will FAIL because enhanced validation doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "INVALID-CAPS-AND-TOO-LONG-OVER-FIFTY-CHARACTERS-LIMIT",  # Format + length errors
                    "name": "A" * 150,  # Too long (max 100)
                    "email": "not-a-valid-email-format"  # Invalid format
                }
            }
        )

        # Expected: Multiple different error types
        assert result["data"]["createAuthorEnhanced"]["__typename"] == "CreateAuthorEnhancedError"
        error_response = result["data"]["createAuthorEnhanced"]

        errors = error_response["errors"]
        assert len(errors) >= 4  # identifier format, identifier length, name length, email format

        # Should have different error identifiers for different validation types
        error_identifiers = [e["identifier"] for e in errors]
        expected_identifiers = {
            "invalid_identifier_format",  # CAPS and special chars
            "identifier_too_long",        # Over 50 chars
            "name_too_long",             # Over 100 chars
            "invalid_email_format"       # Invalid email
        }

        found_identifiers = set(error_identifiers)
        assert len(found_identifiers.intersection(expected_identifiers)) >= 3

        # Length errors should include length details
        length_errors = [e for e in errors if "too_long" in e["identifier"]]
        for length_error in length_errors:
            assert "max_length" in length_error["details"]
            assert "current_length" in length_error["details"]
            assert length_error["details"]["current_length"] > length_error["details"]["max_length"]

        # Summary should categorize error types
        summary = error_response["validationSummary"]
        assert summary["totalErrors"] >= 4
        assert "max_length" in summary["constraintViolations"]
        assert "format" in summary["constraintViolations"]

    async def test_create_post_comprehensive_validation_array_with_security_errors(self, graphql_client):
        """RED: Test comprehensive post validation with security errors in array."""
        # First need to create author (this will also fail initially)
        await graphql_client.execute(
            """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) { __typename }
            }
            """,
            variables={
                "input": {
                    "identifier": "security-author",
                    "name": "Security Author",
                    "email": "security@example.com"
                }
            }
        )

        mutation = """
            mutation CreatePostEnhanced($input: CreatePostEnhancedInput!) {
                createPostEnhanced(input: $input) {
                    __typename
                    ... on CreatePostEnhancedError {
                        message
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        securityViolations
                        invalidTags
                        validationSummary {
                            totalErrors
                            securityIssues
                            hasValidationErrors
                        }
                    }
                }
            }
        """

        # Content with multiple issues
        dangerous_content = '<script>alert("xss")</script>Content with javascript:void(0) and ../../../etc/passwd'
        very_long_content = "x" * 10001

        # This will FAIL because CreatePostEnhanced doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    # Multiple validation errors:
                    # Missing: identifier (required)
                    "title": "A" * 250,  # Too long (max 200)
                    "content": very_long_content + dangerous_content,  # Length + security
                    "authorIdentifier": "non-existent-author",  # Missing reference
                    "tagIdentifiers": ["missing-tag-1", "missing-tag-2"],  # Missing references
                    "status": "invalid-status"  # Invalid enum
                }
            }
        )

        # Expected: Comprehensive error array with different error types
        assert result["data"]["createPostEnhanced"]["__typename"] == "CreatePostEnhancedError"
        error_response = result["data"]["createPostEnhanced"]

        errors = error_response["errors"]
        assert len(errors) >= 8  # identifier, title, content length, security x3, author, tags x2, status

        # Should have different error categories
        validation_422 = [e for e in errors if e["code"] == 422]
        assert len(validation_422) >= 8

        # Security errors should have security constraint
        security_errors = [e for e in errors if e["details"]["constraint"] == "security"]
        assert len(security_errors) >= 3  # script, javascript, path_traversal

        security_violations = [e["details"]["violation"] for e in security_errors]
        expected_violations = {"script_tag", "javascript_uri", "path_traversal"}
        assert len(set(security_violations).intersection(expected_violations)) >= 2

        # Tag errors should be individual (one per missing tag)
        tag_errors = [e for e in errors if e["identifier"] == "invalid_tag"]
        assert len(tag_errors) == 2

        missing_tags = [e["details"]["missing_identifier"] for e in tag_errors]
        assert "missing-tag-1" in missing_tags
        assert "missing-tag-2" in missing_tags

        # Should include security violations summary
        assert error_response["securityViolations"] is not None
        assert len(error_response["securityViolations"]) >= 2

        # Should include invalid tags list
        assert error_response["invalidTags"] == ["missing-tag-1", "missing-tag-2"]

        # Validation summary should categorize everything
        summary = error_response["validationSummary"]
        assert summary["totalErrors"] >= 8
        assert summary["hasValidationErrors"] is True
        assert summary["securityIssues"] is not None
        assert len(summary["securityIssues"]) >= 2

    async def test_create_author_conflicts_with_validation_errors_mixed_array(self, graphql_client):
        """RED: Test mix of conflict (409) and validation (422) errors in array."""
        # Create existing author to set up conflict scenario (will fail initially)
        await graphql_client.execute(
            """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) { __typename }
            }
            """,
            variables={
                "input": {
                    "identifier": "existing-author",
                    "name": "Existing Author",
                    "email": "existing@example.com"
                }
            }
        )

        mutation = """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) {
                    __typename
                    ... on CreateAuthorEnhancedError {
                        message
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        conflictAuthor {
                            id
                            identifier
                            name
                        }
                        validationSummary {
                            totalErrors
                            hasConflicts
                            hasValidationErrors
                            constraintViolations
                        }
                    }
                }
            }
        """

        # This will FAIL because conflict detection with validation isn't implemented yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "existing-author",  # Conflict
                    "name": "B" * 150,              # Validation error (too long)
                    "email": "existing@example.com" # Conflict
                }
            }
        )

        # Expected: Mix of 409 (conflict) and 422 (validation) errors
        assert result["data"]["createAuthorEnhanced"]["__typename"] == "CreateAuthorEnhancedError"
        error_response = result["data"]["createAuthorEnhanced"]

        errors = error_response["errors"]
        assert len(errors) == 3  # identifier conflict, email conflict, name too long

        # Should have both error codes
        error_codes = [e["code"] for e in errors]
        assert 409 in error_codes  # Conflicts
        assert 422 in error_codes  # Validation

        conflict_errors = [e for e in errors if e["code"] == 409]
        validation_errors = [e for e in errors if e["code"] == 422]

        assert len(conflict_errors) == 2  # identifier + email
        assert len(validation_errors) == 1  # name length

        # Conflict errors should have conflict details
        for conflict_error in conflict_errors:
            assert conflict_error["details"]["constraint"] == "unique"
            assert "conflict_id" in conflict_error["details"]

        # Should include conflict author info
        conflict_author = error_response["conflictAuthor"]
        assert conflict_author["identifier"] == "existing-author"
        assert conflict_author["name"] == "Existing Author"

        # Validation summary should show both types
        summary = error_response["validationSummary"]
        assert summary["totalErrors"] == 3
        assert summary["hasConflicts"] is True
        assert summary["hasValidationErrors"] is True
        assert summary["constraintViolations"]["unique"] == 2
        assert summary["constraintViolations"]["max_length"] == 1


class TestRedPhaseErrorArrayStructure:
    """RED: Test error array structure consistency (will fail initially)."""

    async def test_error_array_structure_follows_printoptim_patterns(self, graphql_client, clean_database):
        """RED: Test that all errors follow consistent PrintOptim Backend structure."""
        mutation = """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) {
                    __typename
                    ... on CreateAuthorEnhancedError {
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                    }
                }
            }
        """

        # This will FAIL because structured validation doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "",      # Empty - invalid
                    "name": "",           # Empty - invalid
                    "email": "bad-email"  # Invalid format
                }
            }
        )

        # Expected: Consistent error structure following PrintOptim patterns
        error_response = result["data"]["createAuthorEnhanced"]
        errors = error_response["errors"]

        assert len(errors) >= 3

        # Every error must follow PrintOptim Backend structure
        for i, error in enumerate(errors):
            # Required fields
            assert "code" in error, f"Error {i} missing 'code' field"
            assert "identifier" in error, f"Error {i} missing 'identifier' field"
            assert "message" in error, f"Error {i} missing 'message' field"
            assert "details" in error, f"Error {i} missing 'details' field"

            # Field types
            assert isinstance(error["code"], int), f"Error {i} 'code' should be integer"
            assert isinstance(error["identifier"], str), f"Error {i} 'identifier' should be string"
            assert isinstance(error["message"], str), f"Error {i} 'message' should be string"
            assert isinstance(error["details"], dict), f"Error {i} 'details' should be dict"

            # Code should be valid HTTP status
            valid_codes = {400, 401, 403, 404, 409, 422, 500}
            assert error["code"] in valid_codes, f"Error {i} invalid code: {error['code']}"

            # Identifier should be snake_case
            assert "_" in error["identifier"], f"Error {i} identifier should be snake_case: {error['identifier']}"

            # Details should have field and constraint
            assert "field" in error["details"], f"Error {i} details missing 'field'"
            assert "constraint" in error["details"], f"Error {i} details missing 'constraint'"

    async def test_success_response_has_empty_errors_array(self, graphql_client, clean_database):
        """RED: Test that successful operations return empty errors array (not null)."""
        mutation = """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) {
                    __typename
                    ... on CreateAuthorEnhancedSuccess {
                        author { id identifier name }
                        message
                        errors
                    }
                }
            }
        """

        # This will FAIL because CreateAuthorEnhanced doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "success-author",
                    "name": "Success Author",
                    "email": "success@example.com"
                }
            }
        )

        # Expected: Success with empty errors array
        assert result["data"]["createAuthorEnhanced"]["__typename"] == "CreateAuthorEnhancedSuccess"
        success_response = result["data"]["createAuthorEnhanced"]

        # Success should have empty errors array (following PrintOptim pattern)
        errors = success_response["errors"]
        assert isinstance(errors, list), "Errors should be array, not null"
        assert len(errors) == 0, "Success should have empty errors array"

        # Should have actual data
        assert success_response["author"] is not None
        assert success_response["author"]["identifier"] == "success-author"
        assert success_response["message"] == "Author created successfully"


class TestRedPhaseFieldLevelErrorGrouping:
    """RED: Test field-level error grouping capabilities (will fail initially)."""

    async def test_validation_summary_groups_errors_by_field(self, graphql_client, clean_database):
        """RED: Test validation summary provides field-level error grouping."""
        mutation = """
            mutation CreatePostEnhanced($input: CreatePostEnhancedInput!) {
                createPostEnhanced(input: $input) {
                    __typename
                    ... on CreatePostEnhancedError {
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        validationSummary {
                            totalErrors
                            fieldErrors
                            constraintViolations
                        }
                    }
                }
            }
        """

        # This will FAIL because validation summary doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "INVALID_IDENTIFIER_TOO_LONG_" + "X" * 50,  # Format + length
                    "title": "Y" * 250,     # Too long
                    "content": "Z" * 15000, # Too long
                    "authorIdentifier": "missing-author",  # Missing reference
                    "status": "invalid-status"             # Invalid enum
                }
            }
        )

        # Expected: Errors grouped by field in validation summary
        error_response = result["data"]["createPostEnhanced"]
        summary = error_response["validationSummary"]

        # Should have field-level error grouping
        field_errors = summary["fieldErrors"]
        assert "identifier" in field_errors
        assert "title" in field_errors
        assert "content" in field_errors
        assert "authorIdentifier" in field_errors
        assert "status" in field_errors

        # Each field should have its errors listed
        identifier_errors = field_errors["identifier"]
        assert len(identifier_errors) >= 1  # Format or length error

        # Should group by constraint type
        constraint_violations = summary["constraintViolations"]
        assert "max_length" in constraint_violations
        assert "format" in constraint_violations
        assert "foreign_key" in constraint_violations
        assert "enum" in constraint_violations


class TestRedPhaseSecurityValidationArrays:
    """RED: Test security validation error arrays (will fail initially)."""

    async def test_multiple_security_violations_in_structured_array(self, graphql_client, clean_database):
        """RED: Test multiple security violations returned as structured array."""
        # Create author first (will fail initially)
        await graphql_client.execute(
            """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) { __typename }
            }
            """,
            variables={
                "input": {
                    "identifier": "security-author",
                    "name": "Security Author",
                    "email": "security@example.com"
                }
            }
        )

        mutation = """
            mutation CreatePostEnhanced($input: CreatePostEnhancedInput!) {
                createPostEnhanced(input: $input) {
                    __typename
                    ... on CreatePostEnhancedError {
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        securityViolations
                    }
                }
            }
        """

        # Content with multiple security issues
        dangerous_content = '''
        <script>alert("XSS");</script>
        <a href="javascript:void(0)">Link</a>
        <img src="../../../etc/passwd" />
        '''

        # This will FAIL because security validation doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "security-test",
                    "title": "Security Test",
                    "content": dangerous_content,
                    "authorIdentifier": "security-author"
                }
            }
        )

        # Expected: Multiple security errors in structured array
        error_response = result["data"]["createPostEnhanced"]
        errors = error_response["errors"]

        # Should have multiple security errors
        security_errors = [e for e in errors if e["details"]["constraint"] == "security"]
        assert len(security_errors) >= 3  # script, javascript, path_traversal

        # Should detect specific violation types
        violations = [e["details"]["violation"] for e in security_errors]
        expected = {"script_tag", "javascript_uri", "path_traversal"}
        assert len(set(violations).intersection(expected)) >= 2

        # Should include security violations summary
        assert "securityViolations" in error_response
        security_violations = error_response["securityViolations"]
        assert isinstance(security_violations, list)
        assert len(security_violations) >= 2


class TestRedPhasePerformanceWithErrorArrays:
    """RED: Test performance characteristics with large error arrays (will fail initially)."""

    async def test_many_validation_errors_handled_efficiently(self, graphql_client, clean_database):
        """RED: Test efficient handling of large numbers of validation errors."""
        import time

        mutation = """
            mutation CreatePostEnhanced($input: CreatePostEnhancedInput!) {
                createPostEnhanced(input: $input) {
                    __typename
                    ... on CreatePostEnhancedError {
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        validationSummary {
                            totalErrors
                        }
                    }
                }
            }
        """

        # Create input that will generate many errors
        many_invalid_tags = [f"missing-tag-{i}" for i in range(100)]  # 100 missing tags

        start_time = time.time()

        # This will FAIL because bulk error handling doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "X" * 100,        # Too long
                    "title": "Y" * 300,            # Too long
                    "content": "Z" * 15000,        # Too long
                    "authorIdentifier": "missing", # Missing reference
                    "tagIdentifiers": many_invalid_tags,  # 100 missing references
                    "status": "invalid"            # Invalid enum
                }
            }
        )

        end_time = time.time()
        execution_time = end_time - start_time

        # Expected: Efficient handling of many errors
        assert execution_time < 5.0, f"Should handle many errors efficiently, took {execution_time:.2f}s"

        error_response = result["data"]["createPostEnhanced"]
        errors = error_response["errors"]

        # Should have many errors (100+ from tags plus other validation errors)
        assert len(errors) >= 105  # 100 tags + 5 other validation errors

        validation_summary = error_response["validationSummary"]
        assert validation_summary["totalErrors"] >= 105

        print(f"RED: Expected to handle {len(errors)} errors in {execution_time:.3f}s")


class TestRedPhaseComprehensiveErrorScenarios:
    """RED: Test comprehensive error scenarios that should be supported (will fail initially)."""

    async def test_nested_validation_with_cascading_errors(self, graphql_client, clean_database):
        """RED: Test complex nested validation scenarios."""
        mutation = """
            mutation CreatePostEnhanced($input: CreatePostEnhancedInput!) {
                createPostEnhanced(input: $input) {
                    __typename
                    ... on CreatePostEnhancedError {
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        validationSummary {
                            totalErrors
                            fieldErrors
                            securityIssues
                            hasConflicts
                            hasValidationErrors
                        }
                    }
                }
            }
        """

        # This will FAIL because comprehensive nested validation doesn't exist yet
        complex_input = {
            # Field validation errors
            "identifier": "",  # Missing
            "title": "",      # Missing
            "content": "<script>alert('xss')</script>" + ("x" * 10000),  # Security + length

            # Reference validation errors
            "authorIdentifier": "non-existent",
            "tagIdentifiers": ["missing-1", "missing-2", "missing-3"],

            # Business rule errors
            "status": "invalid-status",
            "publishAt": "2020-01-01T00:00:00Z",  # Past date for published

            # Additional validation
            "featuredImageUrl": "not-a-valid-url",
            "excerpt": "A" * 1000  # Too long
        }

        result = await graphql_client.execute(mutation, {"input": complex_input})

        # Expected: Comprehensive error array covering all validation types
        error_response = result["data"]["createPostEnhanced"]
        errors = error_response["errors"]

        # Should have errors for all validation failures
        assert len(errors) >= 10  # Multiple categories of errors

        # Should categorize different error types
        error_types = set(e["identifier"] for e in errors)
        expected_types = {
            "missing_required_field",
            "content_too_long",
            "unsafe_html",
            "missing_author",
            "invalid_tag",
            "invalid_status",
            "invalid_publish_date",
            "invalid_url_format",
            "excerpt_too_long"
        }

        # Should find multiple error types
        assert len(error_types.intersection(expected_types)) >= 5

        # Validation summary should provide comprehensive overview
        summary = error_response["validationSummary"]
        assert summary["totalErrors"] >= 10
        assert summary["hasValidationErrors"] is True
        assert summary["securityIssues"] is not None
        assert len(summary["fieldErrors"]) >= 7  # Multiple fields with errors

    async def test_error_recovery_and_partial_validation(self, graphql_client, clean_database):
        """RED: Test that validation continues after encountering errors."""
        mutation = """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) {
                    __typename
                    ... on CreateAuthorEnhancedError {
                        errors {
                            code
                            identifier
                            message
                            details
                        }
                        validationSummary {
                            totalErrors
                            constraintViolations
                        }
                    }
                }
            }
        """

        # This will FAIL because comprehensive validation doesn't exist yet
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": None,        # Null (should be caught)
                    "name": "",               # Empty string (should be caught)
                    "email": "invalid-email", # Format error (should be caught)
                    "bio": "A" * 2000        # Too long (should be caught)
                }
            }
        )

        # Expected: All validation errors caught, not just first one
        error_response = result["data"]["createAuthorEnhanced"]
        errors = error_response["errors"]

        # Should catch ALL validation issues, not stop at first
        assert len(errors) == 4  # identifier null, name empty, email format, bio length

        # Should have different constraint types
        constraints = [e["details"]["constraint"] for e in errors]
        expected_constraints = {"required", "format", "max_length"}
        assert len(set(constraints).intersection(expected_constraints)) >= 2

        # Summary should show comprehensive validation was performed
        summary = error_response["validationSummary"]
        assert summary["totalErrors"] == 4
        constraint_violations = summary["constraintViolations"]
        assert constraint_violations["required"] >= 2  # identifier + name
        assert constraint_violations["format"] >= 1   # email
        assert constraint_violations["max_length"] >= 1  # bio
