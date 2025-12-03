import pytest

from fraiseql.core.graphql_type import convert_type_to_graphql_output
from fraiseql.enterprise.audit.types import AuditEvent

pytestmark = pytest.mark.integration


def test_audit_event_graphql_type() -> None:
    """Verify AuditEvent GraphQL type is properly defined."""
    gql_type = convert_type_to_graphql_output(AuditEvent)

    assert gql_type is not None
    assert gql_type.name == "AuditEvent"

    fields = gql_type.fields
    assert "id" in fields
    assert "eventType" in fields
    assert "eventData" in fields
    assert "userId" in fields
    assert "timestamp" in fields
    assert "eventHash" in fields
