# Enabling SQL Logging in FraiseQL

FraiseQL uses `psycopg` (via `psycopg_pool`) for PostgreSQL database interactions. To enable SQL query logging, you can use the built-in `database_echo` configuration parameter or manually configure Python's `logging` module.

## Quick Start: Using `database_echo`

**NEW in v0.11.1**: FraiseQL now has integrated SQL logging support via the `database_echo` configuration parameter.

### Enable via Configuration

```python
from fraiseql import create_fraiseql_app, FraiseQLConfig

# Method 1: Direct parameter
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    config=FraiseQLConfig(
        database_url="postgresql://localhost/mydb",
        database_echo=True  # Enable SQL logging
    )
)
```

### Enable via Environment Variable

```bash
export FRAISEQL_DATABASE_ECHO=true
python -m uvicorn main:app --reload
```

When `database_echo=True`, FraiseQL automatically configures the `psycopg` logger to DEBUG level, showing all SQL queries in your application logs.

## Manual Configuration (Advanced)

For more control over SQL logging (custom log levels, filters, etc.), you can manually configure the logging module:

1.  **Create a Logging Configuration File:**

    Create a new Python file, for example, `logging_config.py`, with the following content:

    ```python
    import logging
    import os

    def setup_logging():
        """Configures logging for the FraiseQL application, including SQL logging."""
        log_level = os.getenv("FRAISEQL_LOG_LEVEL", "INFO").upper()

        # Set up basic logging for the application
        logging.basicConfig(level=log_level)

        # Enable SQL logging for psycopg
        # Set to DEBUG for full query logging, INFO for connection/transaction info
        psycopg_log_level = os.getenv("FRAISEQL_SQL_LOG_LEVEL", "INFO").upper()
        logging.getLogger("psycopg.sql").setLevel(psycopg_log_level)
        logging.getLogger("psycopg.pool").setLevel(psycopg_log_level)
        logging.getLogger("psycopg").setLevel(psycopg_log_level)

        logger = logging.getLogger(__name__)
        logger.info(f"Logging configured with general level: {log_level}, SQL level: {psycopg_log_level}")
    ```

2.  **Integrate into FastAPI Application Startup:**

    Modify your `src/fraiseql/fastapi/app.py` file to import and call the `setup_logging` function. The best place for this is at the very beginning of the `create_fraiseql_app` function, before any other FraiseQL or FastAPI initialization.

    ```python
    # src/fraiseql/fastapi/app.py

    import logging
    # ... other imports
    from fraiseql.logging_config import setup_logging # Add this import

    logger = logging.getLogger(__name__)

    # Global to store turbo registry for lifespan access
    _global_turbo_registry = None


    async def create_db_pool(database_url: str, **pool_kwargs: Any) -> psycopg_pool.AsyncConnectionPool:
        # ... existing code ...


def create_fraiseql_app(
    *,
    # Required configuration
    database_url: str | None = None,
    # ... other parameters ...
) -> FastAPI:
    """Create a FastAPI application with FraiseQL GraphQL endpoint."""
    setup_logging() # Call the logging setup function here

    # ... rest of the function ...
    ```

3.  **Control Logging Level with Environment Variables:**

    You can control the verbosity of the logs using environment variables:

    *   `FRAISEQL_LOG_LEVEL`: Sets the general logging level for the application (e.g., `INFO`, `DEBUG`, `WARNING`, `ERROR`). Defaults to `INFO`.
    *   `FRAISEQL_SQL_LOG_LEVEL`: Sets the specific logging level for `psycopg` to control SQL query output. Defaults to `INFO`.

    To see full SQL queries, set `FRAISEQL_SQL_LOG_LEVEL` to `DEBUG`:

    ```bash
    export FRAISEQL_SQL_LOG_LEVEL=DEBUG
    export FRAISEQL_LOG_LEVEL=DEBUG # Optional: for more general application logs
    python -m uvicorn main:app --reload
    ```

## Important Considerations

*   **Performance:** Enabling `DEBUG` level SQL logging can generate a large volume of output and may impact application performance, especially in high-traffic environments. Use it judiciously for development and debugging.
*   **Security:** Be cautious about logging sensitive data in production environments. Ensure your logging configuration and practices comply with your organization's security policies.
*   **Log Management:** If enabling detailed logging, ensure you have proper log rotation and management in place to prevent disk space exhaustion.

By following these steps, you can effectively enable and manage SQL logging within your FraiseQL application.
