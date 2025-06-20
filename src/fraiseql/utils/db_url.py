"""Database URL conversion utilities."""

import re
from urllib.parse import quote_plus, urlparse


def psycopg2_to_url(conn_string: str) -> str:
    """Convert psycopg2 connection string to PostgreSQL URL format.

    Converts from:
        "dbname='mydb' user='myuser' host='localhost' port='5432' password='mypass'"

    To:
        "postgresql://myuser:mypass@localhost:5432/mydb"

    Args:
        conn_string: psycopg2-style connection string

    Returns:
        PostgreSQL URL format string
    """
    # Parse key-value pairs from connection string
    params: dict[str, str] = {}

    # Handle both quoted and unquoted values
    # Pattern matches: key='value' or key=value
    pattern = r"(\w+)=(?:'([^']*)'|([^\s]*))"

    for match in re.finditer(pattern, conn_string):
        key = match.group(1)
        # Use quoted value if present, otherwise unquoted
        value = match.group(2) if match.group(2) is not None else match.group(3)
        params[key] = value

    # Extract components
    dbname = params.get("dbname", "postgres")
    user = params.get("user", "postgres")
    password = params.get("password", "")
    host = params.get("host", "localhost")
    port = params.get("port", "5432")

    # Handle other parameters
    extra_params = []
    skip_keys = {"dbname", "user", "password", "host", "port"}
    for key, value in params.items():
        if key not in skip_keys:
            extra_params.append(f"{key}={value}")

    # Build URL
    # URL-encode password to handle special characters
    auth = f"{user}:{quote_plus(password)}" if password else user

    url = f"postgresql://{auth}@{host}:{port}/{dbname}"

    # Add extra parameters if any
    if extra_params:
        url += "?" + "&".join(extra_params)

    return url


def url_to_psycopg2(url: str) -> str:
    """Convert PostgreSQL URL to psycopg2 connection string format.

    Converts from:
        "postgresql://myuser:mypass@localhost:5432/mydb?sslmode=require"

    To:
        "dbname='mydb' user='myuser' password='mypass' host='localhost' port='5432' \
sslmode='require'"

    Args:
        url: PostgreSQL URL format string

    Returns:
        psycopg2-style connection string
    """
    parsed = urlparse(url)

    parts = []

    # Database name
    if parsed.path:
        dbname = parsed.path.lstrip("/")
        if dbname:
            parts.append(f"dbname='{dbname}'")

    # User
    if parsed.username:
        parts.append(f"user='{parsed.username}'")

    # Password
    if parsed.password:
        parts.append(f"password='{parsed.password}'")

    # Host
    if parsed.hostname:
        parts.append(f"host='{parsed.hostname}'")

    # Port
    if parsed.port:
        parts.append(f"port='{parsed.port}'")

    # Query parameters
    if parsed.query:
        for param in parsed.query.split("&"):
            if "=" in param:
                key, value = param.split("=", 1)
                parts.append(f"{key}='{value}'")

    return " ".join(parts)


def normalize_database_url(url_or_conn_string: str) -> str:
    """Normalize database connection string to URL format.

    Accepts either PostgreSQL URL format or psycopg2 connection string
    and returns normalized URL format.

    Args:
        url_or_conn_string: Database connection in either format

    Returns:
        PostgreSQL URL format string
    """
    # Quick check if it's already a URL
    if url_or_conn_string.startswith(("postgresql://", "postgres://", "postgis://")):
        return url_or_conn_string

    # Otherwise assume it's psycopg2 format
    return psycopg2_to_url(url_or_conn_string)
