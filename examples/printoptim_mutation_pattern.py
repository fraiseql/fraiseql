"""Example: PrintOptim-compatible mutation pattern with errors as data.

This example shows how to configure FraiseQL mutations to work with
PrintOptim's PostgreSQL status conventions where:
- Only 'failed:*' statuses trigger GraphQL errors
- 'noop:*' statuses are returned as data (not errors)
- All other statuses follow the tb_entity_change_log constraint
"""

from uuid import UUID

from fraiseql import (
    PRINTOPTIM_ERROR_CONFIG,
    failure,
    fraise_input,
    fraise_type,
    mutation,
    success,
)


# Input type
@fraise_input
class CreateMachineInput:
    name: str
    contract_id: UUID
    organization_id: UUID


# Success type - for actual successful operations
@success
class CreateMachineSuccess:
    message: str
    machine: "Machine"
    status: str  # Will contain: 'new', 'existing', 'updated', etc.


# Error type - for business logic "errors" returned as data
@failure
class CreateMachineError:
    message: str
    code: str  # Will contain the full status like 'noop:invalid_contract_id'
    reason: str | None = None
    details: dict | None = None
    status: str  # The raw status from PostgreSQL


# The machine type
@fraise_type
class Machine:
    id: UUID
    name: str
    contract_id: UUID
    organization_id: UUID


# Mutation with PrintOptim error configuration
@mutation(
    function="create_machine",
    schema="app",
    context_params={
        "tenant_id": "input_pk_organization",
        "user": "input_created_by",
    },
    error_config=PRINTOPTIM_ERROR_CONFIG,  # Use PrintOptim's error detection rules
)
class CreateMachine:
    input: CreateMachineInput
    success: CreateMachineSuccess
    failure: CreateMachineError  # Note: using 'failure' instead of 'error'


# The PostgreSQL function following PrintOptim's patterns:
POSTGRESQL_FUNCTION = """
CREATE OR REPLACE FUNCTION app.create_machine(
    input_pk_organization UUID,
    input_created_by UUID,
    input_json jsonb
)
RETURNS app.mutation_result AS $$
DECLARE
    v_input jsonb := input_json->'input';
    v_contract_id uuid := (v_input->>'contract_id')::uuid;
    v_machine_id uuid;
    v_existing_machine record;
BEGIN
    -- Validation: Check if contract exists
    IF NOT EXISTS (
        SELECT 1 FROM contracts
        WHERE id = v_contract_id
        AND organization_id = input_pk_organization
    ) THEN
        -- Return with 'noop:' prefix - will be returned as data, not error
        RETURN log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'machine',
            NULL,
            'NOOP',
            'noop:invalid_contract_id',  -- This will NOT be a GraphQL error
            ARRAY[]::TEXT[],
            'Contract not found or access denied',
            NULL,
            NULL,
            jsonb_build_object(
                'reason', 'contract_does_not_exist',
                'code', 'noop:invalid_contract_id'
            )
        );
    END IF;

    -- Check if machine already exists
    SELECT * INTO v_existing_machine
    FROM machines
    WHERE name = v_input->>'name'
    AND organization_id = input_pk_organization;

    IF v_existing_machine.id IS NOT NULL THEN
        -- Return existing machine with 'noop:existing' status
        RETURN log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'machine',
            v_existing_machine.id,
            'NOOP',
            'noop:existing',  -- This will NOT be a GraphQL error
            ARRAY[]::TEXT[],
            'Machine already exists',
            jsonb_build_object(
                'id', v_existing_machine.id,
                'name', v_existing_machine.name,
                'contract_id', v_existing_machine.contract_id,
                'organization_id', v_existing_machine.organization_id
            ),
            NULL,
            jsonb_build_object(
                'reason', 'duplicate_name',
                'code', 'noop:existing'
            )
        );
    END IF;

    -- Try to create the machine
    BEGIN
        INSERT INTO machines (name, contract_id, organization_id)
        VALUES (v_input->>'name', v_contract_id, input_pk_organization)
        RETURNING id INTO v_machine_id;

        -- Success case
        RETURN log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'machine',
            v_machine_id,
            'INSERT',
            'new',  -- Success status
            ARRAY['name', 'contract_id']::TEXT[],
            'Machine created successfully',
            NULL,
            jsonb_build_object(
                'id', v_machine_id,
                'name', v_input->>'name',
                'contract_id', v_contract_id,
                'organization_id', input_pk_organization
            ),
            NULL
        );

    EXCEPTION WHEN OTHERS THEN
        -- Only use 'failed:' prefix for actual errors
        RETURN log_and_return_mutation(
            input_pk_organization,
            input_created_by,
            'machine',
            NULL,
            'NOOP',
            'failed:database_error',  -- This WILL be a GraphQL error
            ARRAY[]::TEXT[],
            'Database error occurred',
            NULL,
            NULL,
            jsonb_build_object(
                'error', SQLERRM,
                'code', 'failed:database_error'
            )
        );
    END;
END;
$$ LANGUAGE plpgsql;
"""


# Test example showing how errors are returned as data:
def test_create_machine_with_invalid_contract():
    """Test that noop statuses are returned as data, not errors."""
    mutation = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    message
                    status
                    machine {
                        id
                        name
                    }
                }
                ... on CreateMachineError {
                    message
                    code
                    status
                    reason
                    details
                }
            }
        }
    """

    response = client.post(
        "/graphql",
        json={
            "query": mutation,
            "variables": {
                "input": {
                    "name": "Test Machine",
                    "contractId": "00000000-0000-0000-0000-000000000000",
                    "organizationId": org_id,
                },
            },
        },
    )

    result = response.json()

    # With PRINTOPTIM_ERROR_CONFIG, noop: statuses are in data, not errors
    assert "data" in result
    assert "errors" not in result  # No GraphQL errors!

    # The result is in the data field as CreateMachineError type
    machine_result = result["data"]["createMachine"]
    assert machine_result["__typename"] == "CreateMachineError"
    assert machine_result["code"] == "noop:invalid_contract_id"
    assert machine_result["status"] == "noop:invalid_contract_id"
    assert machine_result["reason"] == "contract_does_not_exist"
    assert machine_result["message"] == "Contract not found or access denied"


def test_create_machine_with_database_error():
    """Test that failed: statuses still trigger GraphQL errors."""
    # Simulate a database error that returns 'failed:database_error'

    response = client.post(
        "/graphql",
        json={
            "query": mutation,
            "variables": {
                "input": {
                    # Invalid input that causes database error
                },
            },
        },
    )

    result = response.json()

    # With PRINTOPTIM_ERROR_CONFIG, failed: statuses are GraphQL errors
    assert "errors" in result
    assert result["errors"][0]["extensions"]["code"] == "failed:database_error"


# Alternative: Use ALWAYS_DATA_CONFIG to never raise GraphQL errors
from fraiseql import ALWAYS_DATA_CONFIG


@mutation(
    function="create_machine_v2",
    schema="app",
    error_config=ALWAYS_DATA_CONFIG,  # ALL errors returned as data
)
class CreateMachineV2:
    input: CreateMachineInput
    success: CreateMachineSuccess
    failure: CreateMachineError


# Custom configuration example
from fraiseql import MutationErrorConfig

# Create custom error detection for your project
CUSTOM_ERROR_CONFIG = MutationErrorConfig(
    success_keywords={
        "success",
        "completed",
        "ok",
        "done",
        "new",
        "existing",
        "updated",
        "deleted",
        "synced",
    },
    error_prefixes={
        "failed:",  # Database/system failures
        "error:",  # Application errors
        "critical:",  # Critical failures
    },
    error_as_data_prefixes={
        "noop:",  # No operation performed
        "blocked:",  # Business rule blockage
        "skipped:",  # Skipped processing
        "warning:",  # Warnings but operation succeeded
    },
    error_keywords=set(),  # Don't use generic keywords
)


# Benefits of this approach:
# 1. Tests can always access result["data"]["mutationName"]
# 2. Business logic "errors" are typed and structured
# 3. Only true system failures trigger GraphQL errors
# 4. Follows PrintOptim's PostgreSQL conventions
# 5. Compatible with tb_entity_change_log constraints
