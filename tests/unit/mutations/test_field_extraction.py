"""Test field extraction from GraphQL info object."""

from unittest.mock import Mock

from fraiseql.mutations.mutation_decorator import _extract_mutation_selected_fields


def test_extract_mutation_selected_fields_from_fragment():
    """Test extracting fields from inline fragment."""
    # Mock GraphQL info object with inline fragment
    info = Mock()
    field_node = Mock()

    # Create mock selection for "... on CreateMachineSuccess { status machine { id } }"
    fragment_selection = Mock()
    fragment_selection.type_condition = Mock()
    fragment_selection.type_condition.name = Mock()
    fragment_selection.type_condition.name.value = "CreateMachineSuccess"

    # Mock the fields within the fragment
    status_field = Mock()
    status_field.name = Mock()
    status_field.name.value = "status"

    machine_field = Mock()
    machine_field.name = Mock()
    machine_field.name.value = "machine"

    typename_field = Mock()
    typename_field.name = Mock()
    typename_field.name.value = "__typename"

    fragment_selection.selection_set = Mock()
    fragment_selection.selection_set.selections = [typename_field, status_field, machine_field]

    field_node.selection_set = Mock()
    field_node.selection_set.selections = [fragment_selection]

    info.field_nodes = [field_node]

    # Extract fields
    result = _extract_mutation_selected_fields(info, "CreateMachineSuccess")

    # Should extract status and machine (not __typename)
    assert result is not None
    assert set(result) == {"status", "machine"}
    print(f"✅ Extracted fields: {result}")


def test_extract_mutation_selected_fields_no_matching_fragment():
    """Test when fragment type doesn't match."""
    info = Mock()
    field_node = Mock()

    fragment_selection = Mock()
    fragment_selection.type_condition = Mock()
    fragment_selection.type_condition.name = Mock()
    fragment_selection.type_condition.name.value = "OtherType"
    fragment_selection.selection_set = Mock()
    fragment_selection.selection_set.selections = []

    field_node.selection_set = Mock()
    field_node.selection_set.selections = [fragment_selection]

    info.field_nodes = [field_node]

    # Extract fields for different type
    result = _extract_mutation_selected_fields(info, "CreateMachineSuccess")

    # Should return None (no matching fragment)
    assert result is None
    print("✅ No matching fragment returns None")


def test_extract_mutation_selected_fields_none_info():
    """Test with None info (backward compat)."""
    result = _extract_mutation_selected_fields(None, "CreateMachineSuccess")
    assert result is None
    print("✅ None info returns None")
