"""Test mutation result pattern implementation."""

import pytest
from uuid import UUID, uuid4
from datetime import datetime
from decimal import Decimal

from ..models import (
    CreateOrganizationInput,
    CreateUserInput,
    CreateProjectInput,
    CreateTaskInput,
)


class TestMutationResultPattern:
    """Test the standardized mutation result pattern."""

    async def test_create_organization_success(self, graphql_client):
        """Test successful organization creation with audit metadata."""
        mutation = """
            mutation CreateOrganization($input: CreateOrganizationInput!) {
                createOrganization(input: $input) {
                    __typename
                    ... on CreateOrganizationSuccess {
                        organization {
                            id
                            name
                            identifier
                            auditTrail {
                                createdAt
                                createdByName
                                version
                            }
                        }
                        message
                        generatedIdentifier
                        auditMetadata
                    }
                    ... on CreateOrganizationError {
                        message
                        errorCode
                        validationFailures
                    }
                }
            }
        """

        input_data = {
            "name": "Acme Corporation",
            "legalName": "Acme Corporation Ltd.",
            "industry": "Technology",
            "employeeCount": 150,
            "annualRevenue": "5000000.00"
        }

        result = await graphql_client.execute(
            mutation,
            variables={"input": input_data}
        )

        # Verify success response structure
        assert "errors" not in result
        response = result["data"]["createOrganization"]

        assert response["__typename"] == "CreateOrganizationSuccess"
        assert response["message"] == "Organization created successfully"

        # Verify organization data
        org = response["organization"]
        assert org["name"] == "Acme Corporation"
        assert org["identifier"].startswith("ORG-")

        # Verify audit trail
        audit = org["auditTrail"]
        assert audit["createdAt"] is not None
        assert audit["version"] == 1

        # Verify enterprise metadata
        assert response["generatedIdentifier"] == org["identifier"]
        assert response["auditMetadata"] is not None

    async def test_create_user_with_validation_error(self, graphql_client):
        """Test user creation with validation errors."""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    __typename
                    ... on CreateUserSuccess {
                        user {
                            id
                            email
                        }
                    }
                    ... on CreateUserError {
                        message
                        errorCode
                        fieldErrors
                        validationFailures
                        emailValidationFailed
                        dataPrivacyViolations
                    }
                }
            }
        """

        # Invalid email format
        input_data = {
            "email": "invalid-email",
            "firstName": "J",  # Too short
            "lastName": "Doe",
            "organizationId": str(uuid4())
        }

        result = await graphql_client.execute(
            mutation,
            variables={"input": input_data}
        )

        response = result["data"]["createUser"]

        assert response["__typename"] == "CreateUserError"
        assert response["errorCode"] == "VALIDATION_FAILED"
        assert response["emailValidationFailed"] is True

        # Verify structured field errors
        field_errors = response["fieldErrors"]
        assert "email" in field_errors
        assert "firstName" in field_errors

        # Verify detailed validation context
        validation_failures = response["validationFailures"]
        assert len(validation_failures) > 0
        assert any("email" in failure["field"] for failure in validation_failures)

    async def test_create_project_noop_scenario(self, graphql_client, existing_organization, existing_user):
        """Test project creation NOOP when name conflicts."""
        mutation = """
            mutation CreateProject($input: CreateProjectInput!) {
                createProject(input: $input) {
                    __typename
                    ... on CreateProjectSuccess {
                        project {
                            id
                            name
                        }
                    }
                    ... on CreateProjectNoop {
                        existingProject {
                            id
                            name
                        }
                        message
                        noopReason
                        nameConflictInOrganization
                        suggestedAlternativeName
                    }
                }
            }
        """

        # Create a project first
        project_name = "Test Project Alpha"
        first_input = {
            "name": project_name,
            "organizationId": str(existing_organization["id"]),
            "ownerId": str(existing_user["id"])
        }

        first_result = await graphql_client.execute(
            mutation,
            variables={"input": first_input}
        )

        assert first_result["data"]["createProject"]["__typename"] == "CreateProjectSuccess"

        # Try to create project with same name (should NOOP)
        second_result = await graphql_client.execute(
            mutation,
            variables={"input": first_input}
        )

        response = second_result["data"]["createProject"]

        assert response["__typename"] == "CreateProjectNoop"
        assert response["noopReason"] == "name_conflict"
        assert response["nameConflictInOrganization"] is True
        assert response["existingProject"]["name"] == project_name
        assert "already exists" in response["message"]

    async def test_update_project_with_optimistic_locking(self, graphql_client, existing_project):
        """Test project update with version conflict handling."""
        mutation = """
            mutation UpdateProject($id: UUID!, $input: UpdateProjectInput!) {
                updateProject(id: $id, input: $input) {
                    __typename
                    ... on UpdateProjectSuccess {
                        project {
                            id
                            name
                            auditTrail {
                                version
                                updatedAt
                            }
                        }
                        updatedFields
                        previousVersion
                        newVersion
                        auditMetadata
                    }
                    ... on UpdateProjectNoop {
                        project {
                            auditTrail {
                                version
                            }
                        }
                        noopReason
                        versionConflict
                        currentVersion
                        expectedVersion
                    }
                }
            }
        """

        project_id = existing_project["id"]

        # Update with correct version
        input_data = {
            "name": "Updated Project Name",
            "_expectedVersion": 1
        }

        result = await graphql_client.execute(
            mutation,
            variables={"id": project_id, "input": input_data}
        )

        response = result["data"]["updateProject"]

        assert response["__typename"] == "UpdateProjectSuccess"
        assert "name" in response["updatedFields"]
        assert response["previousVersion"] == 1
        assert response["newVersion"] == 2

        # Try to update with old version (should NOOP due to version conflict)
        stale_input = {
            "name": "Another Update",
            "_expectedVersion": 1  # Stale version
        }

        stale_result = await graphql_client.execute(
            mutation,
            variables={"id": project_id, "input": stale_input}
        )

        stale_response = stale_result["data"]["updateProject"]

        assert stale_response["__typename"] == "UpdateProjectNoop"
        assert stale_response["noopReason"] == "version_conflict"
        assert stale_response["versionConflict"] is True
        assert stale_response["currentVersion"] == 2
        assert stale_response["expectedVersion"] == 1

    async def test_create_task_with_cross_entity_validation(self, graphql_client, existing_project):
        """Test task creation with complex cross-entity validation."""
        mutation = """
            mutation CreateTask($input: CreateTaskInput!) {
                createTask(input: $input) {
                    __typename
                    ... on CreateTaskSuccess {
                        task {
                            id
                            title
                            identifier
                        }
                        generatedIdentifier
                        projectStatsUpdated
                        auditMetadata
                    }
                    ... on CreateTaskNoop {
                        message
                        noopReason
                        projectNotAcceptingTasks
                        duplicateTitleInProject
                    }
                    ... on CreateTaskError {
                        message
                        errorCode
                        projectValidationFailed
                        assigneeValidationFailed
                        timelineConflict
                    }
                }
            }
        """

        # Valid task creation
        valid_input = {
            "title": "Implement user authentication",
            "projectId": str(existing_project["id"]),
            "estimatedHours": 8.0,
            "priority": "high"
        }

        result = await graphql_client.execute(
            mutation,
            variables={"input": valid_input}
        )

        response = result["data"]["createTask"]

        assert response["__typename"] == "CreateTaskSuccess"
        assert response["task"]["title"] == "Implement user authentication"
        assert response["task"]["identifier"].startswith("TASK-")
        assert response["projectStatsUpdated"] is True

        # Try to create task with same title (should NOOP)
        duplicate_result = await graphql_client.execute(
            mutation,
            variables={"input": valid_input}
        )

        duplicate_response = duplicate_result["data"]["createTask"]

        assert duplicate_response["__typename"] == "CreateTaskNoop"
        assert duplicate_response["noopReason"] == "duplicate_title"
        assert duplicate_response["duplicateTitleInProject"] is True

    async def test_mutation_audit_metadata_structure(self, graphql_client):
        """Test that all mutation results include proper audit metadata."""
        mutation = """
            mutation CreateOrganization($input: CreateOrganizationInput!) {
                createOrganization(input: $input) {
                    ... on CreateOrganizationSuccess {
                        auditMetadata
                    }
                }
            }
        """

        input_data = {
            "name": "Audit Test Corp",
            "legalName": "Audit Test Corporation"
        }

        result = await graphql_client.execute(
            mutation,
            variables={"input": input_data}
        )

        metadata = result["data"]["createOrganization"]["auditMetadata"]

        # Verify audit metadata structure
        assert "operation_id" in metadata
        assert "timestamp" in metadata
        assert "source_system" in metadata
        assert "validation_layers" in metadata
        assert "business_rules_applied" in metadata
        assert "performance_metrics" in metadata

        # Verify performance tracking
        perf_metrics = metadata["performance_metrics"]
        assert "execution_time_ms" in perf_metrics
        assert "database_queries" in perf_metrics
        assert "cache_hits" in perf_metrics

    async def test_error_response_completeness(self, graphql_client):
        """Test that error responses include comprehensive debugging information."""
        mutation = """
            mutation CreateUser($input: CreateUserInput!) {
                createUser(input: $input) {
                    ... on CreateUserError {
                        message
                        errorCode
                        fieldErrors
                        validationFailures
                        businessRuleViolations
                        systemConstraints
                        suggestedFixes
                        complianceContext
                    }
                }
            }
        """

        # Invalid input that should trigger multiple validation layers
        input_data = {
            "email": "invalid",
            "firstName": "",
            "lastName": "",
            "organizationId": "not-a-uuid"
        }

        result = await graphql_client.execute(
            mutation,
            variables={"input": input_data}
        )

        error_response = result["data"]["createUser"]

        # Verify comprehensive error information
        assert error_response["message"] is not None
        assert error_response["errorCode"] is not None

        # Field-level errors
        field_errors = error_response["fieldErrors"]
        assert "email" in field_errors
        assert "firstName" in field_errors
        assert "organizationId" in field_errors

        # Validation layer details
        validation_failures = error_response["validationFailures"]
        assert len(validation_failures) > 0

        for failure in validation_failures:
            assert "layer" in failure  # graphql, app, core, database
            assert "field" in failure
            assert "message" in failure
            assert "rule" in failure

        # Business rule violations
        business_violations = error_response["businessRuleViolations"]
        assert isinstance(business_violations, list)

        # System constraints
        system_constraints = error_response["systemConstraints"]
        assert isinstance(system_constraints, list)

        # Actionable suggestions
        suggested_fixes = error_response["suggestedFixes"]
        assert len(suggested_fixes) > 0

        for fix in suggested_fixes:
            assert "field" in fix
            assert "suggestion" in fix
            assert "example" in fix
