"""Canary tests - will break if field auto-injection changes unexpectedly."""

from fraiseql.mutations.decorators import error, success


def test_success_type_fields_canary():
    """Canary: Success type fields should not change unexpectedly."""

    @success
    class TestSuccess:
        entity: dict

    # Expected auto-injected fields (v1.8.1)
    expected = {"status", "message", "updated_fields", "id", "entity"}
    actual = set(TestSuccess.__annotations__.keys())

    assert actual == expected, (
        f"❌ Success type fields changed!\n"
        f"   Expected: {expected}\n"
        f"   Got:      {actual}\n"
        f"   Missing:  {expected - actual}\n"
        f"   Extra:    {actual - expected}"
    )


def test_error_type_fields_canary():
    """Canary: Error type fields should not change unexpectedly."""

    @error
    class TestError:
        pass

    # Expected auto-injected fields (v1.8.1)
    expected = {"status", "message", "code", "errors"}
    actual = set(TestError.__annotations__.keys())

    assert actual == expected, (
        f"❌ Error type fields changed!\n"
        f"   Expected: {expected}\n"
        f"   Got:      {actual}\n"
        f"   Missing:  {expected - actual}\n"
        f"   Extra:    {actual - expected}"
    )


def test_error_type_no_update_fields_canary():
    """Canary: Error types should NOT have updatedFields or id."""

    @error
    class TestError:
        pass

    forbidden = {"updated_fields", "id"}
    actual = set(TestError.__annotations__.keys())

    unexpected = forbidden & actual
    assert not unexpected, (
        f"❌ Error type has forbidden fields: {unexpected}\n"
        f"   Error types should NOT have: {forbidden}\n"
        f"   Actual fields: {actual}"
    )


def test_success_type_no_error_fields_canary():
    """Canary: Success types should NOT have code or errors."""

    @success
    class TestSuccess:
        entity: dict

    forbidden = {"code", "errors"}
    actual = set(TestSuccess.__annotations__.keys())

    unexpected = forbidden & actual
    assert not unexpected, (
        f"❌ Success type has forbidden fields: {unexpected}\n"
        f"   Success types should NOT have: {forbidden}\n"
        f"   Actual fields: {actual}"
    )


def test_success_type_graphql_fields_canary():
    """Canary: Success type GraphQL fields should not include errors."""

    @success
    class TestSuccess:
        entity: dict

    gql_fields = set(TestSuccess.__gql_fields__.keys())

    # errors should NOT be in GraphQL schema
    assert "errors" not in gql_fields, (
        f"❌ Success type GraphQL schema has 'errors' field!\n"
        f"   This should have been removed in v1.9.0\n"
        f"   GraphQL fields: {gql_fields}"
    )


def test_error_type_graphql_fields_canary():
    """Canary: Error type GraphQL fields should include code but not updatedFields/id."""

    @error
    class TestError:
        pass

    gql_fields = set(TestError.__gql_fields__.keys())

    # code should be present
    assert "code" in gql_fields, (
        f"❌ Error type GraphQL schema missing 'code' field!\n"
        f"   This should have been auto-injected in v1.8.1\n"
        f"   GraphQL fields: {gql_fields}"
    )

    # updatedFields and id should NOT be present
    forbidden = {"updatedFields", "id"}
    unexpected = forbidden & gql_fields

    assert not unexpected, (
        f"❌ Error type GraphQL schema has forbidden fields: {unexpected}\n"
        f"   Error types should NOT have: {forbidden}\n"
        f"   GraphQL fields: {gql_fields}"
    )
