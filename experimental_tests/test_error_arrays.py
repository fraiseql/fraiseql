"""Error Arrays E2E Tests - Demonstrating PrintOptim Backend Error Patterns

This test suite demonstrates the intended architecture for error arrays in FraiseQL,
following the patterns found in PrintOptim Backend where multiple validation errors
are returned as structured arrays with comprehensive error information.

Key Demonstrations:
- Multiple validation errors returned as arrays
- Structured error objects with code, identifier, message, details
- Field-level validation error grouping
- Security validation error patterns
- Business rule violation error patterns
- Comprehensive error metadata and debugging information
"""

import pytest
import pytest_asyncio



@pytest.mark.blog_demo
class TestMultipleValidationErrorArrays:
    """Test comprehensive validation error arrays following PrintOptim patterns."""

    @pytest_asyncio.fixture(autouse=True)
    async def setup(self, clean_database):
        """Ensure clean state for each test."""
        pass

    async def test_create_author_multiple_missing_fields_error_array(self, graphql_client):
        """Test that multiple missing required fields return as error array."""
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
                        validationSummary
                    }
                }
            }
        """

        # Try to create author missing ALL required fields
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    # Missing: identifier, name, email
                    "bio": "This author is missing required fields"
                }
            }
        )

        # Should return error with multiple validation errors in array
        assert result["data"]["createAuthorEnhanced"]["__typename"] == "CreateAuthorEnhancedError"
        error_response = result["data"]["createAuthorEnhanced"]

        # Should have multiple errors in array
        errors = error_response["errors"]
        assert isinstance(errors, list)
        assert len(errors) >= 3  # identifier, name, email

        # Check that we have errors for each missing field
        error_identifiers = [e["identifier"] for e in errors]
        assert "missing_required_field" in error_identifiers

        # Check error structure follows PrintOptim patterns
        required_field_errors = [e for e in errors if e["identifier"] == "missing_required_field"]
        assert len(required_field_errors) >= 3

        # Verify each error has proper structure
        for error in required_field_errors:
            assert error["code"] == 422  # Unprocessable Entity
            assert error["identifier"] == "missing_required_field"
            assert "Missing required field:" in error["message"]
            assert error["details"] is not None
            assert "field" in error["details"]
            assert error["details"]["constraint"] == "required"

        # Check specific fields are mentioned
        field_errors = {e["details"]["field"]: e["message"] for e in required_field_errors}
        assert "identifier" in field_errors
        assert "name" in field_errors
        assert "email" in field_errors

        # Verify validation summary provides useful aggregation
        validation_summary = error_response.get("validationSummary")
        if validation_summary:
            assert validation_summary["total_errors"] >= 3
            assert validation_summary["has_validation_errors"] is True
            assert "field_errors" in validation_summary

    async def test_create_author_mixed_validation_error_types_array(self, graphql_client):
        """Test multiple different types of validation errors in one array."""
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
                        validationSummary
                    }
                }
            }
        """

        # Create author with multiple different validation problems
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "INVALID-IDENTIFIER-WITH-CAPS-AND-TOO-LONG-OVER-FIFTY-CHARS",  # Multiple issues
                    "name": "A" * 150,  # Too long (>100 chars)
                    "email": "not-a-valid-email-format",  # Invalid format
                    "bio": "Valid bio"
                }
            }
        )

        assert result["data"]["createAuthorEnhanced"]["__typename"] == "CreateAuthorEnhancedError"
        error_response = result["data"]["createAuthorEnhanced"]

        errors = error_response["errors"]
        assert isinstance(errors, list)
        assert len(errors) >= 3  # Should have multiple different error types

        # Check we have different error types
        error_identifiers = set(e["identifier"] for e in errors)
        expected_identifiers = {
            "invalid_identifier_format",  # CAPS and special chars not allowed
            "identifier_too_long",        # Over 50 characters
            "name_too_long",             # Over 100 characters
            "invalid_email_format"       # Invalid email
        }

        # Should have at least some of these error types
        assert len(error_identifiers.intersection(expected_identifiers)) >= 2

        # Verify different error codes for different types
        validation_errors = [e for e in errors if e["code"] == 422]
        assert len(validation_errors) >= 3

        # Check details provide specific constraint information
        for error in errors:
            assert error["details"] is not None
            assert "field" in error["details"]
            assert "constraint" in error["details"]

            # Length errors should include length information
            if "too_long" in error["identifier"]:
                assert "max_length" in error["details"]
                assert "current_length" in error["details"]
                assert error["details"]["current_length"] > error["details"]["max_length"]

            # Format errors should include format information
            if "format" in error["identifier"]:
                assert error["details"]["constraint"] == "format"

    async def test_create_post_comprehensive_validation_error_array(self, graphql_client):
        """Test comprehensive post validation with multiple error types."""
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
                        validationSummary
                    }
                }
            }
        """

        # Create post with MULTIPLE validation problems
        very_long_content = "x" * 10001  # Too long
        security_content = '<script>alert("xss")</script>Some content with javascript:void(0) and ../../../etc/passwd'

        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    # Missing: identifier (required)
                    "title": "A" * 250,  # Too long (>200 chars)
                    "content": very_long_content + security_content,  # Too long + security issues
                    "authorIdentifier": "non-existent-author",  # Missing author reference
                    "tagIdentifiers": ["missing-tag-1", "missing-tag-2", "missing-tag-3"],  # Missing tags
                    "status": "invalid-status-value"  # Invalid enum value
                }
            }
        )

        assert result["data"]["createPostEnhanced"]["__typename"] == "CreatePostEnhancedError"
        error_response = result["data"]["createPostEnhanced"]

        errors = error_response["errors"]
        assert isinstance(errors, list)
        assert len(errors) >= 7  # Multiple validation errors expected

        # Check we have different categories of errors
        error_identifiers = [e["identifier"] for e in errors]

        # Should include various types
        expected_types = [
            "missing_required_field",  # identifier missing
            "title_too_long",         # title length
            "content_too_long",       # content length
            "invalid_status",         # enum validation
            "unsafe_html",           # security - script tag
            "unsafe_javascript",     # security - javascript: URI
            "path_traversal",        # security - path traversal
            "missing_author",        # reference validation
            "invalid_tag"            # reference validation (multiple)
        ]

        # Should have multiple error types from list
        found_types = set(error_identifiers)
        expected_set = set(expected_types)
        assert len(found_types.intersection(expected_set)) >= 5

        # Check error codes are appropriate
        validation_errors_422 = [e for e in errors if e["code"] == 422]
        assert len(validation_errors_422) >= 5

        # Security errors should have security constraint
        security_errors = [e for e in errors if e["details"] and e["details"].get("constraint") == "security"]
        assert len(security_errors) >= 1  # Should have at least one security error

        # Tag validation errors should be individual (one per missing tag)
        tag_errors = [e for e in errors if e["identifier"] == "invalid_tag"]
        assert len(tag_errors) == 3  # One for each missing tag

        # Each tag error should specify which tag is missing
        missing_tag_ids = [e["details"]["missing_identifier"] for e in tag_errors]
        assert "missing-tag-1" in missing_tag_ids
        assert "missing-tag-2" in missing_tag_ids
        assert "missing-tag-3" in missing_tag_ids

        # Validation summary should aggregate information
        validation_summary = error_response.get("validationSummary")
        if validation_summary:
            assert validation_summary["total_errors"] >= 7
            assert validation_summary["has_validation_errors"] is True
            assert validation_summary["has_conflicts"] is False  # No conflicts, just validation
            assert "security_issues" in validation_summary

    async def test_create_author_conflict_and_validation_error_array(self, graphql_client):
        """Test combination of validation errors and business rule conflicts."""
        # First create an author to create conflict scenario
        await graphql_client.execute(
            """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) {
                    __typename
                    ... on CreateAuthorEnhancedSuccess {
                        author { id }
                    }
                }
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

        # Now try to create another author with conflicts AND validation errors
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
                        conflictAuthor { id identifier name }
                        validationSummary
                    }
                }
            }
        """

        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "existing-author",  # Conflict - duplicate identifier
                    "name": "B" * 150,              # Validation error - too long
                    "email": "existing@example.com" # Conflict - duplicate email
                    # Missing bio is fine (optional)
                }
            }
        )

        assert result["data"]["createAuthorEnhanced"]["__typename"] == "CreateAuthorEnhancedError"
        error_response = result["data"]["createAuthorEnhanced"]

        errors = error_response["errors"]
        assert isinstance(errors, list)
        assert len(errors) >= 3  # duplicate identifier, duplicate email, name too long

        # Check mix of error codes
        error_codes = [e["code"] for e in errors]
        assert 409 in error_codes  # Conflict errors
        assert 422 in error_codes  # Validation errors

        # Check we have both conflict and validation error types
        conflict_errors = [e for e in errors if e["code"] == 409]
        validation_errors = [e for e in errors if e["code"] == 422]

        assert len(conflict_errors) >= 2  # identifier and email conflicts
        assert len(validation_errors) >= 1  # name too long

        # Conflict errors should have conflict context
        for conflict_error in conflict_errors:
            assert conflict_error["details"]["constraint"] == "unique"
            assert "conflict_id" in conflict_error["details"]

        # Validation summary should show both types
        validation_summary = error_response.get("validationSummary")
        if validation_summary:
            assert validation_summary["has_conflicts"] is True
            assert validation_summary["has_validation_errors"] is True

    async def test_error_array_structure_consistency(self, graphql_client):
        """Test that all errors in array follow consistent structure."""
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

        # Create invalid input that will generate multiple errors
        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "",  # Empty - invalid
                    "name": "",       # Empty - invalid
                    "email": "bad-email"  # Invalid format
                }
            }
        )

        error_response = result["data"]["createAuthorEnhanced"]
        errors = error_response["errors"]

        assert len(errors) >= 3

        # Every error must follow consistent structure
        for i, error in enumerate(errors):
            # Required fields
            assert "code" in error, f"Error {i} missing 'code' field"
            assert "identifier" in error, f"Error {i} missing 'identifier' field"
            assert "message" in error, f"Error {i} missing 'message' field"

            # Field types
            assert isinstance(error["code"], int), f"Error {i} 'code' should be integer"
            assert isinstance(error["identifier"], str), f"Error {i} 'identifier' should be string"
            assert isinstance(error["message"], str), f"Error {i} 'message' should be string"

            # Code should be valid HTTP status code
            assert error["code"] in [400, 401, 403, 404, 409, 422, 500], f"Error {i} has invalid code: {error['code']}"

            # Identifier should be machine-readable (snake_case)
            assert "_" in error["identifier"] or error["identifier"].islower(), f"Error {i} identifier not snake_case: {error['identifier']}"

            # Message should be human-readable (non-empty)
            assert len(error["message"]) > 0, f"Error {i} has empty message"

            # Details should be dict if present
            if error.get("details"):
                assert isinstance(error["details"], dict), f"Error {i} 'details' should be dict"

    async def test_success_response_empty_errors_array(self, graphql_client):
        """Test that successful operations return empty errors array."""
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

        assert result["data"]["createAuthorEnhanced"]["__typename"] == "CreateAuthorEnhancedSuccess"
        success_response = result["data"]["createAuthorEnhanced"]

        # Success should have empty errors array (not null)
        errors = success_response["errors"]
        assert isinstance(errors, list)
        assert len(errors) == 0

        # Should have actual data
        assert success_response["author"] is not None
        assert success_response["author"]["identifier"] == "success-author"
        assert success_response["message"] == "Author created successfully"


class TestFieldLevelErrorGrouping:
    """Test error grouping and field-level validation patterns."""

    async def test_field_level_error_aggregation(self, graphql_client, clean_database):
        """Test that errors can be grouped by field for client convenience."""
        # This would typically be done on the client side or in a GraphQL extension
        # but demonstrates how the error array structure supports field grouping

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
                        validationSummary
                    }
                }
            }
        """

        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "INVALID_IDENTIFIER_WITH_UNDERSCORES_AND_TOO_LONG_OVER_FIFTY",
                    "name": "X" * 150,  # Too long
                    "email": "invalid-email-format"
                }
            }
        )

        error_response = result["data"]["createAuthorEnhanced"]
        errors = error_response["errors"]

        # Demonstrate client-side field grouping
        field_errors = {}
        for error in errors:
            field = error.get("details", {}).get("field")
            if field:
                if field not in field_errors:
                    field_errors[field] = []
                field_errors[field].append({
                    "identifier": error["identifier"],
                    "message": error["message"],
                    "constraint": error.get("details", {}).get("constraint")
                })

        # Should have errors grouped by field
        assert "identifier" in field_errors
        assert "name" in field_errors
        assert "email" in field_errors

        # Each field should have appropriate errors
        identifier_errors = field_errors["identifier"]
        assert len(identifier_errors) >= 1  # At least format or length error

        name_errors = field_errors["name"]
        assert len(name_errors) >= 1  # Length error
        assert any("too_long" in e["identifier"] for e in name_errors)

        email_errors = field_errors["email"]
        assert len(email_errors) >= 1  # Format error
        assert any("format" in e["identifier"] for e in email_errors)


class TestSecurityValidationErrorArrays:
    """Test security validation error patterns in arrays."""

    async def test_multiple_security_violations_in_error_array(self, graphql_client, clean_database):
        """Test multiple security violations returned as structured error array."""
        # First create author for post
        await graphql_client.execute(
            """
            mutation CreateAuthorEnhanced($input: CreateAuthorEnhancedInput!) {
                createAuthorEnhanced(input: $input) {
                    __typename
                }
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
        <script>alert("XSS Attack");</script>
        <a href="javascript:void(0)">Click me</a>
        <img src="../../../etc/passwd" />
        Some normal content here.
        <iframe src="data:text/html,<script>alert('another xss')</script>"></iframe>
        '''

        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    "identifier": "security-test-post",
                    "title": "Security Test Post",
                    "content": dangerous_content,
                    "authorIdentifier": "security-author"
                }
            }
        )

        error_response = result["data"]["createPostEnhanced"]
        errors = error_response["errors"]

        # Should have multiple security-related errors
        security_errors = [e for e in errors if e.get("details", {}).get("constraint") == "security"]
        assert len(security_errors) >= 2  # Should catch multiple security issues

        # Check specific security violation types
        violation_types = [e["details"]["violation"] for e in security_errors if e.get("details", {}).get("violation")]
        expected_violations = {"script_tag", "javascript_uri", "path_traversal"}

        # Should have detected multiple violation types
        found_violations = set(violation_types)
        assert len(found_violations.intersection(expected_violations)) >= 2

        # All security errors should have proper structure
        for sec_error in security_errors:
            assert sec_error["code"] == 422
            assert sec_error["details"]["field"] == "content"
            assert sec_error["details"]["constraint"] == "security"
            assert "violation" in sec_error["details"]
            assert "unsafe" in sec_error["identifier"] or "traversal" in sec_error["identifier"]


class TestPerformanceWithErrorArrays:
    """Test performance characteristics when handling multiple errors."""

    async def test_large_number_of_validation_errors_performance(self, graphql_client, clean_database):
        """Test handling of many validation errors in single request."""
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
                        validationSummary
                    }
                }
            }
        """

        # Create input that will generate many errors
        many_invalid_tags = [f"missing-tag-{i}" for i in range(50)]  # 50 missing tags

        start_time = time.time()

        result = await graphql_client.execute(
            mutation,
            variables={
                "input": {
                    # Multiple validation errors
                    "identifier": "X" * 100,  # Too long
                    "title": "Y" * 300,       # Too long
                    "content": "Z" * 15000,   # Too long
                    "authorIdentifier": "missing-author",  # Missing reference
                    "tagIdentifiers": many_invalid_tags,   # Many missing references
                    "status": "invalid-status"             # Invalid enum
                }
            }
        )

        end_time = time.time()
        execution_time = end_time - start_time

        # Should handle large number of errors efficiently
        assert execution_time < 5.0  # Should complete in reasonable time

        error_response = result["data"]["createPostEnhanced"]
        errors = error_response["errors"]

        # Should have many errors (50 tags + other validation errors)
        assert len(errors) >= 53  # 50 tags + identifier + title + content + author + status

        # Performance should be reasonable even with many errors
        validation_summary = error_response.get("validationSummary")
        if validation_summary:
            assert validation_summary["total_errors"] >= 53

        print(f"Handled {len(errors)} validation errors in {execution_time:.3f} seconds")
