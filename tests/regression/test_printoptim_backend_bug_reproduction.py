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
from typing import Any
from unittest.mock import patch

import pytest

import fraiseql
from fraiseql.types import EmailAddress, IpAddress
from fraiseql.types.coercion import coerce_input
from fraiseql.types.definitions import UNSET

pytestmark = pytest.mark.integration

logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger(__name__)

from fraiseql.gql.builders.registry import SchemaRegistry


@pytest.fixture(autouse=True)
def clear_schema_registry():
    """Clear the schema registry before and after each test."""
    registry = SchemaRegistry.get_instance()
    registry.clear()
    yield
    registry.clear()


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


@fraiseql.error
class CreateNetworkConfigurationError:
    message: str
    conflict_network_configuration: dict[str, Any] | None = None


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
    error: CreateNetworkConfigurationError


@patch("fraiseql.config.schema_config.SchemaConfig.get_instance")
@pytest.mark.asyncio
async def test_printoptim_backend_exact_bug_reproduction(mock_config) -> None:
    """Reproduce the exact bug from PrintOptim Backend.

    This test validates that the _to_dict function correctly preserves field names
    like dns_1_id and dns_2_id, rather than incorrectly stripping the _id suffix.
    """
    # Enable camel_case_fields as in PrintOptim Backend
    mock_config.return_value.camel_case_fields = True

    logger.info("=== REPRODUCING PRINTOPTIM BACKEND BUG ===")

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

        # The key assertion: input_dict contains dns_1_id and dns_2_id, not dns_1 and dns_2
        # This is the actual bug fix validation - _to_dict no longer strips _id suffix
        assert "dns_1_id" in input_dict, (
            f"BUG: dns_1_id missing from input_dict: {list(input_dict.keys())}"
        )
        assert "dns_2_id" in input_dict, (
            f"BUG: dns_2_id missing from input_dict: {list(input_dict.keys())}"
        )
        assert "dns_1" not in input_dict, (
            f"BUG: dns_1 found (should be dns_1_id): {list(input_dict.keys())}"
        )
        assert "dns_2" not in input_dict, (
            f"BUG: dns_2 found (should be dns_2_id): {list(input_dict.keys())}"
        )

        logger.info("✅ All assertions passed - bug is fixed!")

    except Exception as e:
        logger.error(f"❌ Unexpected error: {e}")
        raise


if __name__ == "__main__":
    import asyncio

    asyncio.run(test_printoptim_backend_exact_bug_reproduction())
