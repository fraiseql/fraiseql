"""Logging configuration utilities for FraiseQL."""

import logging


def configure_sql_logging(enabled: bool = False, level: str = "DEBUG") -> None:
    """Configure SQL query logging for psycopg.

    When enabled, this sets up Python's logging module to output SQL queries
    from psycopg at the specified level.

    Args:
        enabled: Whether to enable SQL logging
        level: Log level to use (DEBUG, INFO, WARNING, ERROR)

    Example:
        >>> configure_sql_logging(enabled=True)  # Enable SQL logging
        >>> configure_sql_logging(enabled=False)  # Disable SQL logging
    """
    if not enabled:
        # Disable SQL logging by setting to WARNING (only show errors/warnings)
        logging.getLogger("psycopg").setLevel(logging.WARNING)
        logging.getLogger("psycopg.pool").setLevel(logging.WARNING)
        logging.getLogger("psycopg.sql").setLevel(logging.WARNING)
        return

    # Convert string level to logging constant
    log_level = getattr(logging, level.upper(), logging.DEBUG)

    # Configure psycopg loggers
    logging.getLogger("psycopg").setLevel(log_level)
    logging.getLogger("psycopg.pool").setLevel(log_level)
    logging.getLogger("psycopg.sql").setLevel(log_level)

    # Ensure root logger is configured to at least INFO level
    # so that SQL queries are actually displayed
    if logging.getLogger().level > log_level:
        logging.basicConfig(
            level=log_level,
            format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
        )

    logger = logging.getLogger(__name__)
    logger.info("SQL query logging enabled at %s level", level.upper())
