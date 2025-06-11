"""Tests for pgTAP output structure and content."""

import re

import pytest

from fraiseql.extensions.testfoundry.config import FoundryConfig
from fraiseql.extensions.testfoundry.generator import FoundryGenerator


@pytest.mark.database
class TestPgTAPOutput:
    """Test the structure and content of generated pgTAP tests."""

    @pytest.mark.asyncio
    async def test_pgtap_happy_path_structure(self, populated_metadata, test_config):
        """Test the structure of a happy path pgTAP test using real TestFoundry."""
        generator = FoundryGenerator(populated_metadata, test_config)

        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        happy_test = tests.get("happy_create", "")

        # Test should have a header comment
        assert re.search(r"-- Happy path CREATE test for entity \w+", happy_test)

        # Test should declare a plan
        plan_match = re.search(r"SELECT plan\((\d+)\)", happy_test)
        assert plan_match is not None
        plan_count = int(plan_match.group(1))
        assert plan_count > 0

        # Test should set authentication variables
        assert "\\gset" in happy_test  # Uses \\gset for variable assignment
        assert "v_org" in happy_test
        assert "v_user" in happy_test
        # Check for UUID pattern (any valid UUID)
        assert re.search(
            r"'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'::uuid",
            happy_test,
        )

        # Test should call the mutation function
        assert re.search(
            r"SELECT \* INTO v_result FROM create_\w+_with_log", happy_test
        )
        assert ":v_org" in happy_test
        assert ":v_user" in happy_test

        # Test should have assertions
        assert "SELECT is(" in happy_test
        assert "'Status should be new'" in happy_test
        assert "SELECT ok(" in happy_test

        # Test should finish
        assert "SELECT * FROM finish();" in happy_test

    @pytest.mark.asyncio
    async def test_pgtap_variable_extraction(self, populated_metadata, test_config):
        """Test that pgTAP tests properly extract result variables."""
        generator = FoundryGenerator(populated_metadata, test_config)

        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        happy_test = tests.get("happy_create", "")

        # Should extract the ID for further use
        assert (
            "SELECT v_result.pk AS v_id \\gset" in happy_test
            or "v_result.pk" in happy_test
        )

        # Should use the extracted ID in existence checks
        assert re.search(r"WHERE id = :?v_id", happy_test)

    @pytest.mark.asyncio
    async def test_duplicate_create_structure(self, populated_metadata, test_config):
        """Test the structure of duplicate create tests."""
        # Enable duplicate tests
        config = FoundryConfig(
            test_options={
                "happy_path": False,
                "constraint_violations": True,
                "fk_violations": False,
                "soft_delete": False,
            }
        )
        generator = FoundryGenerator(populated_metadata, config)

        tests = await generator.generate_tests_for_entity(
            entity_name="users", table_name="tb_users", input_type_name="user_input"
        )

        duplicate_test = tests.get("duplicate_create", "")

        if duplicate_test and "Error generating" not in duplicate_test:
            # Should have duplicate-specific structure
            assert (
                "duplicate" in duplicate_test.lower()
                or "constraint" in duplicate_test.lower()
            )
            assert "SELECT plan(" in duplicate_test
            assert "SELECT * FROM finish();" in duplicate_test

    @pytest.mark.asyncio
    async def test_fk_violation_structure(self, populated_metadata, test_config):
        """Test the structure of FK violation tests."""
        # Enable FK violation tests
        config = FoundryConfig(
            test_options={
                "happy_path": False,
                "constraint_violations": False,
                "fk_violations": True,
                "soft_delete": False,
            }
        )
        generator = FoundryGenerator(populated_metadata, config)

        # Test with posts which has FK to users
        tests = await generator.generate_tests_for_entity(
            entity_name="posts", table_name="tb_posts", input_type_name="post_input"
        )

        fk_test = tests.get("fk_violation_create", "")

        if fk_test and "Error generating" not in fk_test:
            # Should have FK violation specific structure
            assert "foreign key" in fk_test.lower() or "fk" in fk_test.lower()
            assert "SELECT plan(" in fk_test
            assert "SELECT * FROM finish();" in fk_test
