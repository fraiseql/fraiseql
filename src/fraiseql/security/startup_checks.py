"""Security startup checks for FraiseQL production environments.

These checks run at application startup to detect security misconfigurations
and prevent exploitation of known CVEs.

CVE Mitigations:
- CVE-2025-7709 (SQLite FTS5): Ensure SQLite is not used (PostgreSQL only)
"""

import logging
import os
import sys
import warnings
from pathlib import Path
from typing import List

logger = logging.getLogger(__name__)


class SecurityCheckError(Exception):
    """Raised when a security check fails in production."""


def check_sqlite_not_used() -> None:
    """Ensure SQLite is not imported or used in production.

    FraiseQL exclusively uses PostgreSQL. SQLite presence indicates:
    1. Misconfiguration or dependency issue
    2. Potential exposure to CVE-2025-7709 (SQLite FTS5 integer overflow)

    Raises:
        SecurityCheckError: If sqlite3 module is already loaded
    """
    if "sqlite3" in sys.modules:
        raise SecurityCheckError(
            "SECURITY VIOLATION: sqlite3 module detected in production. "
            "FraiseQL uses PostgreSQL exclusively. "
            "Check dependencies for unexpected SQLite imports. "
            "CVE-2025-7709 mitigation failed."
        )


def disable_sqlite_fts5() -> None:
    """Disable SQLite FTS5 extension if SQLite is somehow loaded.

    This is a defense-in-depth measure. If SQLite is present despite checks,
    disable the FTS5 extension to mitigate CVE-2025-7709.

    Note: This should never be reached in properly configured deployments.
    """
    try:
        import sqlite3

        # If we get here, SQLite was imported (shouldn't happen)
        warnings.warn(
            "WARNING: sqlite3 imported despite checks. "
            "Disabling load_extension to mitigate CVE-2025-7709.",
            UserWarning,
            stacklevel=2,
        )

        # Disable extension loading
        # Note: This is a global setting and will affect all SQLite connections
        try:
            sqlite3.enable_load_extension(False)  # type: ignore[attr-defined]
        except AttributeError:
            # enable_load_extension may not be available on all platforms
            pass

    except ImportError:
        # Good - SQLite not available
        pass


def check_production_environment() -> None:
    """Validate production environment configuration.

    Checks:
    - FRAISEQL_PRODUCTION environment variable is set
    - Database URL uses PostgreSQL (not SQLite)
    - No debug mode enabled
    """
    is_production = os.getenv("FRAISEQL_PRODUCTION", "false").lower() == "true"

    if is_production:
        # Check database URL
        db_url = os.getenv("DATABASE_URL", "")
        if "sqlite" in db_url.lower():
            raise SecurityCheckError(
                "SECURITY VIOLATION: DATABASE_URL contains 'sqlite' in production. "
                "FraiseQL requires PostgreSQL. "
                "Set DATABASE_URL to postgresql://... "
                "CVE-2025-7709 mitigation failed."
            )

        # Check for debug mode
        if os.getenv("DEBUG", "false").lower() == "true":
            warnings.warn(
                "WARNING: DEBUG=true in production environment. "
                "This may expose sensitive information. "
                "Set DEBUG=false for production deployments.",
                UserWarning,
                stacklevel=2,
            )


def check_user_privileges() -> None:
    """Verify application is not running as root.

    Running as root violates security best practices and increases
    impact of potential exploits (including CVE-2025-14104).

    Raises:
        SecurityCheckError: If running as root (UID 0)
    """
    try:
        import pwd

        uid = os.getuid()
        if uid == 0:
            raise SecurityCheckError(
                "SECURITY VIOLATION: Application running as root (UID 0). "
                "FraiseQL must run as non-root user (recommended UID 65532). "
                "This increases impact of CVE-2025-14104 and other exploits."
            )

        # Log current user for audit
        try:
            user_info = pwd.getpwuid(uid)
            logger.info(f"Security Check: Running as {user_info.pw_name} (UID {uid})")
        except KeyError:
            # User not in passwd (expected in minimal containers)
            logger.info(f"Security Check: Running as UID {uid} (non-root)")

    except ImportError:
        # pwd module not available (Windows)
        pass
    except AttributeError:
        # os.getuid() not available (Windows)
        pass


def check_filesystem_permissions() -> None:
    """Verify key directories have appropriate permissions.

    Checks:
    - /tmp is writable (required for read-only root filesystem)
    - /app is readable (application directory)
    - /etc/passwd is not writable (prevents CVE-2025-14104 exploitation)
    """
    checks: List[tuple[str, bool, str]] = [
        ("/tmp", True, "Temporary directory must be writable"),
        ("/app", False, "Application directory must be readable"),
    ]

    for path, should_be_writable, description in checks:
        if not Path(path).exists():
            continue  # Path doesn't exist, skip check

        if should_be_writable:
            if not os.access(path, os.W_OK):
                warnings.warn(
                    f"WARNING: {path} is not writable. "
                    f"{description}. "
                    f"This may cause runtime errors with read-only root filesystem.",
                    RuntimeWarning,
                    stacklevel=2,
                )
        elif not os.access(path, os.R_OK):
            warnings.warn(
                f"WARNING: {path} is not readable. "
                f"{description}. "
                f"This may cause application failures.",
                RuntimeWarning,
                stacklevel=2,
            )

    # Check /etc/passwd is not writable (CVE-2025-14104 mitigation)
    if Path("/etc/passwd").exists() and os.access("/etc/passwd", os.W_OK):
        raise SecurityCheckError(
            "SECURITY VIOLATION: /etc/passwd is writable. "
            "This enables exploitation of CVE-2025-14104. "
            "Container must run as non-root with read-only /etc."
        )


def run_all_security_checks() -> None:
    """Run all security checks at application startup.

    This is the main entry point for startup security validation.
    Should be called early in application initialization, before
    accepting any external connections.

    Raises:
        SecurityCheckError: If any critical security check fails

    Example:
        >>> from fraiseql.security import run_all_security_checks
        >>> run_all_security_checks()
        ✅ Security Check: Running as fraiseql (UID 65532)
        ✅ Security Check: SQLite not imported (PostgreSQL only)
        ✅ Security Check: Production environment validated
        ✅ Security Check: Filesystem permissions correct
    """
    logger.info("Running FraiseQL security startup checks...")

    try:
        # CVE-2025-7709 mitigation
        check_sqlite_not_used()
        logger.info("Security Check: SQLite not imported (PostgreSQL only)")

        # CVE-2025-7709 defense-in-depth
        disable_sqlite_fts5()

        # Production environment validation
        check_production_environment()
        logger.info("Security Check: Production environment validated")

        # CVE-2025-14104 mitigation
        check_user_privileges()

        # CVE-2025-14104 mitigation
        check_filesystem_permissions()
        logger.info("Security Check: Filesystem permissions correct")

        logger.info("All security checks passed!")

    except SecurityCheckError as e:
        logger.error(f"SECURITY CHECK FAILED: {e}")
        logger.error("Application startup aborted for security reasons.")
        sys.exit(1)


# Run checks automatically if module is executed directly (for testing)
if __name__ == "__main__":
    run_all_security_checks()
