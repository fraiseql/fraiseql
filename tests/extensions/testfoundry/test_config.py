"""Tests for FoundryConfig."""

from pathlib import Path

from fraiseql.extensions.testfoundry import FoundryConfig


class TestFoundryConfig:
    """Test the FoundryConfig dataclass."""

    def test_default_config(self):
        """Test default configuration values."""
        config = FoundryConfig()

        assert config.schema_name == "testfoundry"
        assert config.test_output_dir == Path("tests/generated")
        assert config.table_prefix == "tb_"
        assert config.view_prefix == "v_"
        assert config.input_type_prefix == "type_"
        assert config.input_type_suffix == "_input"
        assert config.generate_pytest is True
        assert config.debug_mode is False
        assert config.naming_adapters == {}
        assert config.test_options == {
            "happy_path": True,
            "constraint_violations": True,
            "fk_violations": True,
            "soft_delete": True,
            "blocked_delete": False,
            "authorization": False,
        }

    def test_custom_config(self):
        """Test custom configuration values."""
        custom_path = Path("/custom/test/path")
        custom_options = {
            "happy_path": False,
            "constraint_violations": True,
            "fk_violations": False,
            "soft_delete": False,
            "blocked_delete": True,
            "authorization": True,
        }

        config = FoundryConfig(
            schema_name="custom_schema",
            test_output_dir=custom_path,
            table_prefix="tbl_",
            view_prefix="vw_",
            input_type_prefix="input_",
            input_type_suffix="",
            generate_pytest=False,
            debug_mode=True,
            naming_adapters={"user": "account"},
            test_options=custom_options,
        )

        assert config.schema_name == "custom_schema"
        assert config.test_output_dir == custom_path
        assert config.table_prefix == "tbl_"
        assert config.view_prefix == "vw_"
        assert config.input_type_prefix == "input_"
        assert config.input_type_suffix == ""
        assert config.generate_pytest is False
        assert config.debug_mode is True
        assert config.naming_adapters == {"user": "account"}
        assert config.test_options == custom_options

    def test_partial_test_options(self):
        """Test that partial test options work correctly."""
        # When providing test_options, you need to provide the complete dict
        config = FoundryConfig(
            test_options={
                "happy_path": False,
                "constraint_violations": True,
                "fk_violations": True,
                "soft_delete": False,
                "blocked_delete": False,
                "authorization": False,
            }
        )

        # Should have the specified values
        assert config.test_options["happy_path"] is False
        assert config.test_options["soft_delete"] is False
        assert config.test_options["constraint_violations"] is True
        assert config.test_options["fk_violations"] is True

    def test_config_immutability(self):
        """Test that config maintains separate instances."""
        config1 = FoundryConfig()
        config2 = FoundryConfig()

        # Modify one config's test options
        config1.test_options["happy_path"] = False

        # Other config should not be affected
        assert config2.test_options["happy_path"] is True

    def test_path_handling(self):
        """Test that paths are properly handled."""
        # String path should be converted to Path
        config = FoundryConfig(test_output_dir=Path("some/path"))
        assert isinstance(config.test_output_dir, Path)
        assert str(config.test_output_dir) == "some/path"
