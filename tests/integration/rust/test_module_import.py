"""Test fraiseql_rs module import.

Test basic module import functionality.
"""

import pytest

pytestmark = pytest.mark.integration


def test_fraiseql_rs_module_exists() -> None:
    """Test that fraiseql_rs module can be imported.

    RED: This should fail with ModuleNotFoundError
    GREEN: After creating the Rust module, this should pass
    """
    try:
        from fraiseql import fraiseql_rs

        assert fraiseql_rs is not None
    except ModuleNotFoundError as e:
        pytest.fail(f"fraiseql_rs module not found: {e}")


def test_fraiseql_rs_has_version() -> None:
    """Test that fraiseql_rs module has __version__ attribute.

    RED: This should fail because module doesn't exist
    GREEN: After creating the module with version, this should pass
    """
    from fraiseql import fraiseql_rs

    assert hasattr(fraiseql_rs, "__version__")
    assert isinstance(fraiseql_rs.__version__, str)
    assert len(fraiseql_rs.__version__) > 0


def test_fraiseql_rs_version_format() -> None:
    """Test that version follows semantic versioning.

    Expected format: X.Y.Z or X.Y.Z-suffix
    """
    from fraiseql import fraiseql_rs

    version = fraiseql_rs.__version__
    # Basic semver check: should have at least X.Y.Z
    parts = version.split("-")[0].split(".")
    assert len(parts) >= 3, f"Version {version} doesn't follow semver format"

    # Check that major, minor, patch are numbers
    major, minor, patch = parts[0], parts[1], parts[2]
    assert major.isdigit(), f"Major version '{major}' is not a number"
    assert minor.isdigit(), f"Minor version '{minor}' is not a number"
    assert patch.isdigit(), f"Patch version '{patch}' is not a number"


if __name__ == "__main__":
    # Run tests manually for quick testing during development
    pytest.main([__file__, "-v"])
