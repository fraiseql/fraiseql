import pytest

"""Test database URL conversion utilities."""

from fraiseql.utils.db_url import normalize_database_url, psycopg2_to_url, url_to_psycopg2


@pytest.mark.unit
class TestPsycopg2ToUrl:
    """Test psycopg2 to URL conversion."""

    def test_basic_conversion(self) -> None:
        """Test basic connection string conversion."""
        conn_str = "dbname='mydb' user='myuser' host='localhost' port='5432'"
        result = psycopg2_to_url(conn_str)
        assert result == "postgresql://myuser@localhost:5432/mydb"

    def test_with_password(self) -> None:
        """Test conversion with password."""
        conn_str = "dbname='mydb' user='myuser' password='mypass' host='localhost'"
        result = psycopg2_to_url(conn_str)
        assert result == "postgresql://myuser:mypass@localhost:5432/mydb"

    def test_password_special_chars(self) -> None:
        """Test password with special characters."""
        conn_str = "dbname='mydb' user='myuser' password='my@pass#word!' host='localhost'"
        result = psycopg2_to_url(conn_str)
        # Password should be URL-encoded
        assert result == "postgresql://myuser:my%40pass%23word%21@localhost:5432/mydb"

    def test_unquoted_values(self) -> None:
        """Test connection string with unquoted values."""
        conn_str = "dbname=mydb user=myuser host=localhost port=5432"
        result = psycopg2_to_url(conn_str)
        assert result == "postgresql://myuser@localhost:5432/mydb"

    def test_mixed_quoted_unquoted(self) -> None:
        """Test mixed quoted and unquoted values."""
        conn_str = "dbname='my db' user=myuser password='my pass' host=localhost"
        result = psycopg2_to_url(conn_str)
        assert result == "postgresql://myuser:my+pass@localhost:5432/my db"

    def test_extra_parameters(self) -> None:
        """Test with extra parameters like sslmode."""
        conn_str = (
            "dbname='mydb' user='myuser' host='localhost' sslmode='require' connect_timeout='10'"
        )
        result = psycopg2_to_url(conn_str)
        assert (
            result == "postgresql://myuser@localhost:5432/mydb?sslmode=require&connect_timeout=10"
        )

    def test_real_world_example(self) -> None:
        """Test real-world example from user."""
        conn_str = "dbname='fraiseql_db_local' user='lionel' host='localhost'"
        result = psycopg2_to_url(conn_str)
        assert result == "postgresql://lionel@localhost:5432/fraiseql_db_local"

    def test_defaults(self) -> None:
        """Test with minimal connection string."""
        conn_str = "dbname='mydb'"
        result = psycopg2_to_url(conn_str)
        assert result == "postgresql://postgres@localhost:5432/mydb"


class TestUrlToPsycopg2:
    """Test URL to psycopg2 conversion."""

    def test_basic_conversion(self) -> None:
        """Test basic URL conversion."""
        url = "postgresql://myuser@localhost:5432/mydb"
        result = url_to_psycopg2(url)
        assert result == "dbname='mydb' user='myuser' host='localhost' port='5432'"

    def test_with_password(self) -> None:
        """Test URL with password."""
        url = "postgresql://myuser:mypass@localhost:5432/mydb"
        result = url_to_psycopg2(url)
        assert (
            result == "dbname='mydb' user='myuser' password='mypass' host='localhost' port='5432'"
        )

    def test_with_query_params(self) -> None:
        """Test URL with query parameters."""
        url = "postgresql://myuser@localhost:5432/mydb?sslmode=require&connect_timeout=10"
        result = url_to_psycopg2(url)
        expected = (
            """dbname='mydb' user='myuser' host='localhost' """
            """port='5432' sslmode='require' connect_timeout='10'"""
        )
        assert result == expected

    def test_postgres_scheme(self) -> None:
        """Test with postgres:// scheme."""
        url = "postgres://myuser@localhost/mydb"
        result = url_to_psycopg2(url)
        assert "dbname='mydb'" in result
        assert "user='myuser'" in result
        assert "host='localhost'" in result


class TestNormalizeDatabaseUrl:
    """Test database URL normalization."""

    def test_already_url(self) -> None:
        """Test that URLs are returned unchanged."""
        urls = [
            """postgresql://user@localhost/db"""
            """postgres://user:pass@host:5432/db"""
            """postgis://user@localhost/db"""
        ]
        for url in urls:
            assert normalize_database_url(url) == url

    def test_psycopg2_format(self) -> None:
        """Test that psycopg2 format is converted."""
        conn_str = "dbname='mydb' user='myuser' host='localhost'"
        result = normalize_database_url(conn_str)
        assert result == "postgresql://myuser@localhost:5432/mydb"

    def test_mixed_input(self) -> None:
        """Test various input formats."""
        # URL format - unchanged
        url = "postgresql://user@localhost/db"
        assert normalize_database_url(url) == url

        # psycopg2 format - converted
        conn_str = "dbname='db' user='user' host='localhost'"
        result = normalize_database_url(conn_str)
        assert result.startswith("postgresql://")
        assert "user@localhost" in result
        assert "/db" in result


class TestRoundTrip:
    """Test round-trip conversions."""

    def test_url_roundtrip(self) -> None:
        """Test URL -> psycopg2 -> URL conversion."""
        original = "postgresql://myuser:mypass@localhost:5432/mydb"
        psycopg2 = url_to_psycopg2(original)
        back_to_url = psycopg2_to_url(psycopg2)
        assert back_to_url == original

    def test_psycopg2_roundtrip_basic(self) -> None:
        """Test psycopg2 -> URL -> psycopg2 conversion (basic)."""
        # Note: Order might change, so we parse and compare components
        original = "dbname='mydb' user='myuser' host='localhost' port='5432'"
        url = psycopg2_to_url(original)
        back_to_psycopg2 = url_to_psycopg2(url)

        # Parse both to compare
        def parse_psycopg2(s) -> None:
            params = {}
            import re

            for match in re.finditer(r"(\w+)='([^']*)'", s):
                params[match.group(1)] = match.group(2)
            return params

        original_params = parse_psycopg2(original)
        result_params = parse_psycopg2(back_to_psycopg2)

        assert original_params == result_params
