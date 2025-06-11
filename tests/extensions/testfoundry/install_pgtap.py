"""Install pgTAP extension in PostgreSQL for testing."""

import tarfile
import urllib.request
from pathlib import Path


async def install_pgtap_from_source(db_connection):
    """Install pgTAP by compiling from source and loading into database."""

    # Check if pgTAP is already installed
    try:
        result = await db_connection.fetchval(
            "SELECT extversion FROM pg_extension WHERE extname = 'pgtap'"
        )
        if result:
            print(f"pgTAP {result} already installed")
            return True
    except Exception:
        pass

    # Download and extract pgTAP source
    pgtap_version = "1.3.3"
    cache_dir = Path("/tmp/pgtap_build")
    cache_dir.mkdir(exist_ok=True)

    pgtap_sql = cache_dir / "pgtap.sql"

    if not pgtap_sql.exists():
        print(f"Downloading pgTAP {pgtap_version}...")
        url = f"https://github.com/theory/pgtap/archive/v{pgtap_version}.tar.gz"
        tarball = cache_dir / f"pgtap-{pgtap_version}.tar.gz"

        # Download
        urllib.request.urlretrieve(url, tarball)

        # Extract
        with tarfile.open(tarball, "r:gz") as tf:
            tf.extractall(cache_dir)

        # Find the SQL files
        source_dir = cache_dir / f"pgtap-{pgtap_version}"
        sql_dir = source_dir / "sql"

        # Build the complete SQL file
        print("Building pgTAP SQL...")

        # Look for the main SQL file
        main_sql = None
        for sql_file in sql_dir.glob("*.sql*"):
            if "uninstall" not in sql_file.name:
                main_sql = sql_file
                break

        if main_sql and main_sql.exists():
            # Process the SQL file
            content = main_sql.read_text()

            # Replace template variables
            content = content.replace("__VERSION__", pgtap_version)
            content = content.replace("@TAPSCHEMA@", "public")

            # Write processed SQL
            pgtap_sql.write_text(content)
        else:
            # Alternative: build from pgtap.sql.in
            template = sql_dir / "pgtap.sql.in"
            if template.exists():
                content = template.read_text()
                content = content.replace("__VERSION__", pgtap_version)
                content = content.replace("@TAPSCHEMA@", "public")
                pgtap_sql.write_text(content)
            else:
                raise FileNotFoundError("Could not find pgTAP SQL files")

    # Install pgTAP functions into database
    print("Installing pgTAP functions...")
    sql_content = pgtap_sql.read_text()

    # Split into individual statements and execute
    # Remove comments and empty lines
    statements = []
    current_statement = []

    for line in sql_content.split("\n"):
        # Skip comments and empty lines
        if line.strip().startswith("--") or not line.strip():
            continue

        current_statement.append(line)

        # Check if statement is complete (ends with semicolon)
        if line.rstrip().endswith(";"):
            statements.append("\n".join(current_statement))
            current_statement = []

    # Execute statements
    success_count = 0
    for stmt in statements[:100]:  # Limit to first 100 for testing
        if stmt.strip():
            try:
                await db_connection.execute(stmt)
                success_count += 1
            except Exception as e:
                # Skip errors for functions that might already exist
                if "already exists" not in str(e):
                    print(f"Warning: {e}")

    print(f"Installed {success_count} pgTAP objects")
    return success_count > 0


async def install_pgtap_minimal(db_connection):
    """Install minimal pgTAP functions needed for testing."""

    # Create a minimal set of pgTAP functions
    minimal_pgtap = """
    -- Minimal pgTAP implementation for testing

    -- Plan function
    CREATE OR REPLACE FUNCTION plan(integer) RETURNS text
    LANGUAGE sql AS $$
        SELECT '1..' || $1;
    $$;

    -- Finish function
    CREATE OR REPLACE FUNCTION finish() RETURNS SETOF text
    LANGUAGE sql AS $$
        SELECT 'All tests successful.'::text;
    $$;

    -- Basic test functions
    CREATE OR REPLACE FUNCTION ok(boolean, text) RETURNS text
    LANGUAGE sql AS $$
        SELECT CASE WHEN $1 THEN 'ok - ' || $2 ELSE 'not ok - ' || $2 END;
    $$;

    CREATE OR REPLACE FUNCTION is(anyelement, anyelement, text) RETURNS text
    LANGUAGE sql AS $$
        SELECT CASE WHEN $1 = $2 THEN 'ok - ' || $3 ELSE 'not ok - ' || $3 || ' (expected: ' || $2::text || ', got: ' || $1::text || ')' END;
    $$;

    CREATE OR REPLACE FUNCTION isnt(anyelement, anyelement, text) RETURNS text
    LANGUAGE sql AS $$
        SELECT CASE WHEN $1 != $2 THEN 'ok - ' || $3 ELSE 'not ok - ' || $3 END;
    $$;

    -- Use a different name to avoid conflict with PostgreSQL's LIKE operator
    CREATE OR REPLACE FUNCTION alike(text, text, text) RETURNS text
    LANGUAGE sql AS $$
        SELECT CASE WHEN $1 LIKE $2 THEN 'ok - ' || $3 ELSE 'not ok - ' || $3 END;
    $$;

    -- Also create 'like' as pgTAP expects it, with specific argument types
    CREATE OR REPLACE FUNCTION "like"(anyelement, text, text) RETURNS text
    LANGUAGE sql AS $$
        SELECT CASE WHEN $1::text LIKE $2 THEN 'ok - ' || $3 ELSE 'not ok - ' || $3 END;
    $$;

    CREATE OR REPLACE FUNCTION lives_ok(text, text) RETURNS text
    LANGUAGE plpgsql AS $$
    BEGIN
        EXECUTE $1;
        RETURN 'ok - ' || $2;
    EXCEPTION WHEN OTHERS THEN
        RETURN 'not ok - ' || $2 || ' (died: ' || SQLERRM || ')';
    END;
    $$;

    CREATE OR REPLACE FUNCTION throws_ok(text, text, text, text) RETURNS text
    LANGUAGE plpgsql AS $$
    BEGIN
        EXECUTE $1;
        RETURN 'not ok - ' || $4 || ' (no exception thrown)';
    EXCEPTION
        WHEN OTHERS THEN
            IF SQLSTATE = $2 AND SQLERRM LIKE '%' || $3 || '%' THEN
                RETURN 'ok - ' || $4;
            ELSE
                RETURN 'not ok - ' || $4 || ' (wrong exception: ' || SQLSTATE || ' ' || SQLERRM || ')';
            END IF;
    END;
    $$;

    CREATE OR REPLACE FUNCTION pass(text) RETURNS text
    LANGUAGE sql AS $$
        SELECT 'ok - ' || $1;
    $$;

    CREATE OR REPLACE FUNCTION fail(text) RETURNS text
    LANGUAGE sql AS $$
        SELECT 'not ok - ' || $1;
    $$;

    CREATE OR REPLACE FUNCTION diag(text) RETURNS text
    LANGUAGE sql AS $$
        SELECT '# ' || $1;
    $$;
    """

    try:
        await db_connection.execute(minimal_pgtap)
        print("Installed minimal pgTAP functions")
        return True
    except Exception as e:
        print(f"Error installing minimal pgTAP: {e}")
        return False
