"""Reproduce the exact bug from PrintOptim Backend.

The issue:
1. PostgreSQL function expects: dns_1_id, dns_2_id
2. GraphQL client sends: dns1Id, dns2Id
3. FraiseQL converts: dns1Id -> dns1_id, dns2Id -> dns2_id
4. Field mismatch: dns1_id != dns_1_id, dns2_id != dns_2_id
5. FraiseQL fallback somehow strips _id: dns1_id -> dns_1
6. Error: "got an unexpected keyword argument 'dns_1'"

This is the EXACT scenario from PrintOptim Backend.
"""

import logging
import uuid
from unittest.mock import patch
from typing import Any

import pytest

import fraiseql
from fraiseql.types import IpAddress, EmailAddress
from fraiseql.types.definitions import UNSET
from fraiseql.types.coercion import coerce_input

logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)


@fraiseql.input
class CreateNetworkConfigurationInput:
    """EXACT input class from PrintOptim Backend."""

    ip_address: IpAddress
    subnet_mask: IpAddress
    gateway_id: uuid.UUID
    dns_1_id: uuid.UUID | None = UNSET  # PostgreSQL expects dns_1_id
    dns_2_id: uuid.UUID | None = UNSET  # PostgreSQL expects dns_2_id
    print_server_ids: list[uuid.UUID] | None = UNSET
    router_id: uuid.UUID | None = UNSET
    smtp_server_id: uuid.UUID | None = UNSET
    email_address: EmailAddress | None = UNSET
    is_dhcp: bool | None = UNSET


@fraiseql.success
class CreateNetworkConfigurationSuccess:
    message: str = "Network configuration created successfully"
    network_configuration: dict[str, Any]


@fraiseql.failure
class CreateNetworkConfigurationError:
    message: str
    conflict_network_configuration: dict[str, Any] | None = None


class PrintOptimBackendMockDB:
    """Mock that simulates the PrintOptim Backend PostgreSQL function call."""

    def __init__(self):
        self.calls = []

    async def execute_function_with_context(self, function_name, context_args, input_data):
        """Mock the PostgreSQL function that expects dns_1_id, dns_2_id."""

        self.calls.append(
            {
                "function": function_name,
                "context_args": context_args,
                "input_data": input_data.copy(),
            }
        )

        logger.info("=" * 80)
        logger.info(f"PRINTOPTIM BACKEND POSTGRESQL FUNCTION: {function_name}")
        logger.info(f"Expected parameters: dns_1_id, dns_2_id, gateway_id, etc.")
        logger.info(f"Received parameters: {list(input_data.keys())}")
        logger.info(f"Full input data: {input_data}")
        logger.info("=" * 80)

        # Check if we got the problematic parameters
        expected_params = ["dns_1_id", "dns_2_id", "gateway_id", "ip_address", "subnet_mask"]
        problematic_params = []

        for param in input_data.keys():
            if param not in expected_params and param not in [
                "print_server_ids",
                "router_id",
                "smtp_server_id",
                "email_address",
                "is_dhcp",
            ]:
                problematic_params.append(param)

        logger.info("PARAMETER ANALYSIS:")
        for expected in ["dns_1_id", "dns_2_id"]:
            if expected in input_data:
                logger.info(f"  ✅ {expected}: FOUND")
            else:
                logger.info(f"  ❌ {expected}: MISSING")

        for problematic in problematic_params:
            logger.info(f"  🐛 UNEXPECTED: {problematic}")

        # This is where the bug manifests - if we get dns_1 instead of dns_1_id
        if "dns_1" in input_data:
            error_msg = "got an unexpected keyword argument 'dns_1'"
            logger.error(f"🐛 BUG REPRODUCED: {error_msg}")
            raise TypeError(error_msg)

        if "dns_2" in input_data:
            error_msg = "got an unexpected keyword argument 'dns_2'"
            logger.error(f"🐛 BUG REPRODUCED: {error_msg}")
            raise TypeError(error_msg)

        return {
            "status": "success",
            "object_data": {
                "id": str(uuid.uuid4()),
                "ip_address": str(input_data.get("ip_address", "10.0.0.1")),
                "dns_1_id": input_data.get("dns_1_id"),
                "dns_2_id": input_data.get("dns_2_id"),
            },
            "message": "Network configuration created successfully",
            "extra_metadata": {"entity": "network_configuration"},
        }


@fraiseql.mutation(
    function="create_network_configuration",
    context_params={
        "tenant_id": "input_pk_organization",
        "user_id": "input_created_by",
    },
    error_config=fraiseql.DEFAULT_ERROR_CONFIG,
)
class CreateNetworkConfiguration:
    """EXACT mutation class from PrintOptim Backend."""

    input: CreateNetworkConfigurationInput
    success: CreateNetworkConfigurationSuccess
    failure: CreateNetworkConfigurationError


@patch("fraiseql.config.schema_config.SchemaConfig.get_instance")
@pytest.mark.asyncio
async def test_printoptim_backend_exact_bug_reproduction(mock_config):
    """Reproduce the exact bug from PrintOptim Backend."""

    # Enable camel_case_fields as in PrintOptim Backend
    mock_config.return_value.camel_case_fields = True

    logger.info("=== REPRODUCING PRINTOPTIM BACKEND BUG ===")

    # Setup
    mock_db = PrintOptimBackendMockDB()
    mock_info = type(
        "MockInfo",
        (),
        {"context": {"db": mock_db, "tenant_id": uuid.uuid4(), "user_id": uuid.uuid4()}},
    )()

    # Create the EXACT GraphQL input data that PrintOptim Backend sends
    # This comes from their test: test_create_network_configuration.py lines 36-46
    graphql_client_input = {
        "ipAddress": "10.4.50.60",
        "subnetMask": "255.255.255.0",
        "gatewayId": str(uuid.uuid4()),
        "isDhcp": False,
        "dns1Id": str(uuid.uuid4()),  # Client sends dns1Id
        "dns2Id": str(uuid.uuid4()),  # Client sends dns2Id
        "routerId": str(uuid.uuid4()),
        "smtpServerId": str(uuid.uuid4()),
        "emailAddress": "test@example.com",
    }

    logger.info(f"GraphQL client sends: {list(graphql_client_input.keys())}")

    # Test the coercion step that causes the problem
    logger.info("Step 1: Testing input coercion...")

    try:
        coerced_input = coerce_input(CreateNetworkConfigurationInput, graphql_client_input)
        logger.info("✅ Coercion succeeded")

        # Check what fields the coerced object has
        coerced_fields = []
        for attr in ["dns_1_id", "dns_2_id", "gateway_id", "ip_address", "subnet_mask"]:
            if hasattr(coerced_input, attr):
                value = getattr(coerced_input, attr)
                coerced_fields.append(f"{attr}={value}")

        logger.info(f"Coerced fields: {coerced_fields}")

        # Convert to dict (this is what _to_dict does in the mutation)
        from fraiseql.mutations.mutation_decorator import _to_dict

        input_dict = _to_dict(coerced_input)

        logger.info(f"Final input_dict keys: {list(input_dict.keys())}")

        # Check for the bug
        if "dns_1" in input_dict:
            logger.error(f"🐛 BUG FOUND: dns_1 in input_dict: {input_dict}")
        elif "dns_1_id" not in input_dict:
            logger.warning(f"⚠️ Expected dns_1_id missing: {list(input_dict.keys())}")
        else:
            logger.info("✅ dns_1_id correctly present in input_dict")

        # Now test the full mutation
        logger.info("\nStep 2: Testing full mutation execution...")

        # Create the input object manually
        input_obj = CreateNetworkConfigurationInput(
            ip_address=graphql_client_input["ipAddress"],
            subnet_mask=graphql_client_input["subnetMask"],
            gateway_id=uuid.UUID(graphql_client_input["gatewayId"]),
            dns_1_id=uuid.UUID(graphql_client_input["dns1Id"]),
            dns_2_id=uuid.UUID(graphql_client_input["dns2Id"]),
            router_id=uuid.UUID(graphql_client_input["routerId"]),
            smtp_server_id=uuid.UUID(graphql_client_input["smtpServerId"]),
            email_address=graphql_client_input["emailAddress"],
            is_dhcp=graphql_client_input["isDhcp"],
        )

        resolver = CreateNetworkConfiguration.__fraiseql_resolver__
        result = await resolver(mock_info, input_obj)

        logger.info("✅ Mutation execution succeeded")

        # Check what was actually sent to the database
        call = mock_db.calls[-1]
        logger.info(f"Database received: {list(call['input_data'].keys())}")

        # Verify the correct fields were sent
        assert "dns_1_id" in call["input_data"], (
            f"dns_1_id missing: {list(call['input_data'].keys())}"
        )
        assert "dns_2_id" in call["input_data"], (
            f"dns_2_id missing: {list(call['input_data'].keys())}"
        )
        assert "dns_1" not in call["input_data"], (
            f"BUG: dns_1 found: {list(call['input_data'].keys())}"
        )
        assert "dns_2" not in call["input_data"], (
            f"BUG: dns_2 found: {list(call['input_data'].keys())}"
        )

    except TypeError as e:
        if "got an unexpected keyword argument" in str(e):
            logger.error(f"🐛 EXACT BUG REPRODUCED: {e}")
            call = mock_db.calls[-1]
            logger.error(f"Problematic parameters sent: {list(call['input_data'].keys())}")
            pytest.fail(f"PrintOptim Backend bug reproduced: {e}")
        else:
            raise
    except Exception as e:
        logger.error(f"❌ Unexpected error: {e}")
        raise


if __name__ == "__main__":
    import asyncio

    asyncio.run(test_printoptim_backend_exact_bug_reproduction())
