"""Test Unix domain socket support in database URLs."""

import pytest

from fraiseql.fastapi.config import FraiseQLConfig, validate_postgres_url

pytestmark = pytest.mark.integration


@pytest.mark.unit
class TestUnixSocketURLs:
    """Test validation of Unix domain socket PostgreSQL URLs."""

    def test_validate_postgres_url_with_socket(self) -> None:
        """Test that Unix socket URLs are accepted."""
        # Test various valid socket URL formats
        valid_urls = [
            "postgresql://user@/var/run/postgresql:5432/database",
            "postgresql://user:password@/var/run/postgresql:5432/database",
            "postgres://user@/tmp:5432/mydb",
            "postgresql://fraiseql_app@/var/run/postgresql:5432/fraiseql_test",
        ]

        for url in valid_urls:
            result = validate_postgres_url(url)
            assert result == url

    def test_validate_postgres_url_with_regular_url(self) -> None:
        """Test that regular PostgreSQL URLs still work."""
        valid_urls = [
            "postgresql://user:password@localhost:5432/database",
            "postgresql://user@localhost/database",
            "postgres://user:pass@host.com:5432/db",
        ]

        for url in valid_urls:
            result = validate_postgres_url(url)
            assert result == url

    def test_validate_postgres_url_invalid(self) -> None:
        """Test that invalid URLs are rejected."""
        invalid_urls = [
            "mysql://user@localhost/db",  # Wrong protocol
            "postgresql://",  # Empty URL
            "postgresql://user@/",  # Socket without database
            "",  # Empty string
            123,  # Not a string
        ]

        for url in invalid_urls:
            with pytest.raises((ValueError, TypeError)):
                validate_postgres_url(url)

    def test_fraiseql_config_with_socket_url(self) -> None:
        """Test that FraiseQLConfig accepts Unix socket URLs."""
        # The exact URL from the bug report
        config = FraiseQLConfig(
            database_url="postgresql://fraiseql_app@/var/run/postgresql:5432/fraiseql_test"
        )
        assert (
            config.database_url
            == "postgresql://fraiseql_app@/var/run/postgresql:5432/fraiseql_test"
        )

    def test_fraiseql_config_with_regular_url(self) -> None:
        """Test that FraiseQLConfig still accepts regular URLs."""
        config = FraiseQLConfig(database_url="postgresql://user:pass@localhost:5432/testdb")
        assert config.database_url == "postgresql://user:pass@localhost:5432/testdb"

    def test_fraiseql_config_env_var_socket_url(self, monkeypatch) -> None:
        """Test loading socket URL from environment variable."""
        monkeypatch.setenv(
            "FRAISEQL_DATABASE_URL",
            "postgresql://user@/var/run/postgresql:5432/mydb",
        )
        config = FraiseQLConfig()
        assert config.database_url == "postgresql://user@/var/run/postgresql:5432/mydb"
