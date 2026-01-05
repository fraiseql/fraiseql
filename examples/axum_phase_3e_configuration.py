#!/usr/bin/env python3
"""FraiseQL Axum Phase 3E: Advanced Configuration Examples.

Demonstrates various advanced configuration options for request handling,
logging, and security in the Axum HTTP server.
"""

import logging

from fraiseql import create_axum_fraiseql_app, fraise_type
from fraiseql.axum.config import AxumFraiseQLConfig

# Enable logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)


# ===== GraphQL Types =====


@fraise_type
class Post:
    """Example post type."""

    id: str
    title: str
    content: str
    author: str


# ===== Example 1: Default Advanced Configuration =====


def example_default_configuration() -> None:
    """Show default Phase 3E configuration."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 1: Default Advanced Configuration")
    logger.info(sep)

    config = AxumFraiseQLConfig(
        database_url="postgresql://localhost/test"
    )

    logger.info(
        "Max Request Body Size: %.1fMB",
        config.max_request_body_size / 1024 / 1024,
    )
    logger.info("Request Timeout: %ss", config.request_timeout)
    logger.info("Log Requests: %s", config.log_requests)
    logger.info("Log Level: %s", config.log_level)
    logger.info(
        "Introspection in Production: %s",
        config.enable_introspection_in_production,
    )
    logger.info("Require HTTPS: %s", config.require_https)


# ===== Example 2: Small Request Size (IoT Devices) =====


def example_small_request_size() -> None:
    """Configuration for IoT devices with small payloads."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 2: Small Request Size (IoT Devices)")
    logger.info(sep)

    config = AxumFraiseQLConfig(
        database_url="postgresql://localhost/test",
        max_request_body_size=10000,  # 10KB
        request_timeout=5,  # Fast timeout for IoT
        log_requests=False,  # Reduce overhead
    )

    logger.info("Max Request Body Size: %.1fKB", config.max_request_body_size / 1024)
    logger.info("Request Timeout: %ss", config.request_timeout)
    logger.info("Log Requests: %s", config.log_requests)
    logger.info("✓ Optimized for IoT devices with small payloads")


# ===== Example 3: Large File Uploads =====


def example_large_file_uploads() -> None:
    """Configuration for APIs that handle large file uploads."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 3: Large File Uploads")
    logger.info(sep)

    config = AxumFraiseQLConfig(
        database_url="postgresql://localhost/test",
        max_request_body_size=104857600,  # 100MB
        request_timeout=300,  # 5 minutes for uploads
        log_requests=True,
    )

    logger.info(
        "Max Request Body Size: %.0fMB",
        config.max_request_body_size / 1024 / 1024,
    )
    logger.info(
        "Request Timeout: %ss (%.0f min)",
        config.request_timeout,
        config.request_timeout / 60,
    )
    logger.info("✓ Supports large file uploads")


# ===== Example 4: Development Environment =====


def example_development_environment() -> None:
    """Configuration optimized for development."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 4: Development Environment")
    logger.info(sep)

    config = AxumFraiseQLConfig(
        database_url="postgresql://localhost/dev_db",
        environment="development",
        max_request_body_size=10000000,  # 10MB
        request_timeout=60,
        log_requests=True,
        log_level="DEBUG",  # Verbose logging
        enable_introspection_in_production=False,
        require_https=False,
    )

    logger.info("Environment: %s", config.environment)
    logger.info("Log Level: %s", config.log_level)
    logger.info("Request Logging: %s", config.log_requests)
    logger.info(
        "Introspection Allowed: %s",
        not config.enable_introspection_in_production,
    )
    logger.info("HTTPS Required: %s", config.require_https)
    logger.info("✓ Full debugging and introspection enabled")


# ===== Example 5: Production Environment =====


def example_production_environment() -> None:
    """Configuration optimized for production."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 5: Production Environment")
    logger.info(sep)

    config = AxumFraiseQLConfig(
        database_url="postgresql://prod-host/prod_db",
        environment="production",
        max_request_body_size=5000000,  # 5MB
        request_timeout=30,
        log_requests=True,
        log_level="WARNING",  # Less verbose
        enable_introspection_in_production=False,  # Security
        require_https=True,  # Force HTTPS
    )

    logger.info("Environment: %s", config.environment)
    logger.info("Log Level: %s", config.log_level)
    logger.info("Request Logging: %s", config.log_requests)
    logger.info(
        "Introspection Allowed: %s",
        not config.enable_introspection_in_production,
    )
    logger.info("HTTPS Required: %s", config.require_https)
    logger.info("✓ Security-hardened production setup")


# ===== Example 6: Logging Configuration =====


def example_logging_configuration() -> None:
    """Demonstrate different logging levels."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 6: Logging Configuration")
    logger.info(sep)

    levels = ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]

    for level in levels:
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            log_level=level,
        )

        logger.info("  %s: {'log_level': '%s'}", config.log_level, config.log_level)

    logger.info("\nLogging levels:")
    logger.info("  DEBUG:    Detailed information for debugging")
    logger.info("  INFO:     General informational messages")
    logger.info("  WARNING:  Warning messages for issues")
    logger.info("  ERROR:    Error messages for failures")
    logger.info("  CRITICAL: Critical errors requiring attention")


# ===== Example 7: Request Timeout Scenarios =====


def example_request_timeout_scenarios() -> None:
    """Show different timeout configurations for various scenarios."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 7: Request Timeout Scenarios")
    logger.info(sep)

    scenarios = [
        ("API Gateway (aggressive)", 5),
        ("Real-time API", 10),
        ("Standard API", 30),
        ("Batch Processing", 120),
        ("Heavy Analytics", 300),
    ]

    for name, timeout in scenarios:
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            request_timeout=timeout,
        )

        logger.info("  %s: %ss", name, config.request_timeout)


# ===== Example 8: Request Size Scenarios =====


def example_request_size_scenarios() -> None:
    """Show different request size configurations."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 8: Request Size Scenarios")
    logger.info(sep)

    scenarios = [
        ("IoT Devices", 10000),
        ("Mobile Apps", 256000),
        ("Web Applications", 1000000),
        ("API with Medium Files", 5000000),
        ("File Upload Service", 104857600),
    ]

    for name, size in scenarios:
        config = AxumFraiseQLConfig(
            database_url="postgresql://localhost/test",
            max_request_body_size=size,
        )

        size_mb = config.max_request_body_size / 1024 / 1024
        size_kb = config.max_request_body_size / 1024
        size_str = f"{size_mb:.0f}MB" if size_mb >= 1 else f"{size_kb:.0f}KB"

        logger.info("  %s: %s", name, size_str)


# ===== Example 9: Security Configuration =====


def example_security_configuration() -> None:
    """Demonstrate security-related configurations."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 9: Security Configuration")
    logger.info(sep)

    # Strict security
    strict_config = AxumFraiseQLConfig(
        database_url="postgresql://localhost/test",
        environment="production",
        enable_introspection_in_production=False,
        require_https=True,
    )

    # Relaxed for development
    dev_config = AxumFraiseQLConfig(
        database_url="postgresql://localhost/test",
        environment="development",
        enable_introspection_in_production=True,
        require_https=False,
    )

    logger.info("\nProduction (Strict):")
    logger.info(
        "  Introspection: %s",
        strict_config.enable_introspection_in_production,
    )
    logger.info("  HTTPS Required: %s", strict_config.require_https)

    logger.info("\nDevelopment (Relaxed):")
    logger.info(
        "  Introspection: %s",
        dev_config.enable_introspection_in_production,
    )
    logger.info("  HTTPS Required: %s", dev_config.require_https)


# ===== Example 10: Compliance Configuration =====


def example_compliance_configuration() -> None:
    """Configuration for compliance requirements (HIPAA, GDPR, etc)."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 10: Compliance Configuration")
    logger.info(sep)

    config = AxumFraiseQLConfig(
        database_url="postgresql://secure-host/secure_db",
        environment="production",
        # Compliance requirements
        require_https=True,  # HIPAA, GDPR
        enable_introspection_in_production=False,  # HIPAA
        log_requests=True,  # Audit trail
        log_level="INFO",  # Sufficient logging for compliance
        max_request_body_size=5000000,  # Reasonable limits
        request_timeout=30,  # Prevent hanging connections
    )

    logger.info("HIPAA/GDPR Compliance Features:")
    logger.info("  ✓ HTTPS Enforcement: %s", config.require_https)
    logger.info(
        "  ✓ Schema Introspection Disabled: %s",
        not config.enable_introspection_in_production,
    )
    logger.info("  ✓ Request Logging (Audit): %s", config.log_requests)
    logger.info("  ✓ Logging Level: %s", config.log_level)
    logger.info(
        "  ✓ Request Size Limited: %.1fMB",
        config.max_request_body_size / 1024 / 1024,
    )


# ===== Example 11: Custom Request Size and Timeout =====


def example_custom_request_response() -> None:
    """Configuration with custom request/response settings."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 11: Custom Request/Response Configuration")
    logger.info(sep)

    config = AxumFraiseQLConfig(
        database_url="postgresql://localhost/test",
        max_request_body_size=50000000,  # 50MB
        request_timeout=120,  # 2 minutes
    )

    logger.info("Custom Request/Response Settings:")
    logger.info(
        "  Max Request Body: %.0fMB",
        config.max_request_body_size / 1024 / 1024,
    )
    logger.info(
        "  Request Timeout: %ss (%.1f min)",
        config.request_timeout,
        config.request_timeout / 60,
    )
    logger.info("  Log Requests: %s", config.log_requests)
    logger.info("  Log Level: %s", config.log_level)


# ===== Example 12: Environment Variables =====


def example_environment_variables() -> None:
    """Show how to configure Phase 3E via environment variables."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 12: Configuration via Environment Variables")
    logger.info(sep)

    logger.info("\nEnvironment variables for Phase 3E:")
    logger.info("  FRAISEQL_MAX_REQUEST_SIZE: Maximum request body size in bytes")
    logger.info("  FRAISEQL_REQUEST_TIMEOUT: Request timeout in seconds")
    logger.info(
        "  FRAISEQL_LOG_REQUESTS: 'true' or 'false' for request logging"
    )
    logger.info("  FRAISEQL_LOG_LEVEL: DEBUG|INFO|WARNING|ERROR|CRITICAL")
    logger.info("  FRAISEQL_INTROSPECTION_PROD: 'true' or 'false'")
    logger.info("  FRAISEQL_REQUIRE_HTTPS: 'true' or 'false'")

    logger.info("\nExample usage:")
    logger.info("  export FRAISEQL_DATABASE_URL=postgresql://localhost/test")
    logger.info("  export FRAISEQL_MAX_REQUEST_SIZE=5000000")
    logger.info("  export FRAISEQL_REQUEST_TIMEOUT=30")
    logger.info("  export FRAISEQL_LOG_LEVEL=INFO")
    logger.info("  export FRAISEQL_REQUIRE_HTTPS=true")


# ===== Example 13: Development vs Production =====


def example_development_vs_production() -> None:
    """Compare development and production configurations."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 13: Development vs Production")
    logger.info(sep)

    dev_config = AxumFraiseQLConfig(
        database_url="postgresql://localhost/dev",
        environment="development",
        max_request_body_size=10000000,
        request_timeout=60,
        log_requests=True,
        log_level="DEBUG",
    )

    prod_config = AxumFraiseQLConfig(
        database_url="postgresql://prod-host/prod",
        environment="production",
        max_request_body_size=5000000,
        request_timeout=30,
        log_requests=True,
        log_level="WARNING",
        require_https=True,
    )

    logger.info("\nDevelopment Configuration:")
    logger.info("  Environment: %s", dev_config.environment)
    logger.info(
        "  Request Body Size: %.0fMB",
        dev_config.max_request_body_size / 1024 / 1024,
    )
    logger.info("  Request Timeout: %ss", dev_config.request_timeout)
    logger.info("  Log Level: %s", dev_config.log_level)

    logger.info("\nProduction Configuration:")
    logger.info("  Environment: %s", prod_config.environment)
    logger.info(
        "  Request Body Size: %.0fMB",
        prod_config.max_request_body_size / 1024 / 1024,
    )
    logger.info("  Request Timeout: %ss", prod_config.request_timeout)
    logger.info("  Log Level: %s", prod_config.log_level)
    logger.info("  HTTPS Required: %s", prod_config.require_https)


# ===== Example 14: Full Configuration Example =====


def example_full_configuration() -> None:
    """Complete configuration with all Phase 3E options."""
    sep = "=" * 60
    logger.info("\n%s", sep)
    logger.info("Example 14: Full Advanced Configuration")
    logger.info(sep)

    config = AxumFraiseQLConfig(
        # Database
        database_url="postgresql://user:pass@host:5432/graphql_db",
        database_pool_size=20,
        database_pool_timeout=60,
        # Environment
        environment="production",
        production_mode=True,
        # GraphQL
        enable_introspection=True,
        enable_playground=False,
        max_query_depth=20,
        # Security
        auth_enabled=True,
        jwt_secret="super-secret-key",
        # Performance
        enable_query_caching=True,
        cache_ttl=600,
        # Axum Server
        axum_host="0.0.0.0",  # noqa: S104
        axum_port=8000,
        axum_workers=8,
        # Phase 3E: Advanced Configuration
        max_request_body_size=10000000,  # 10MB
        request_timeout=45,
        log_requests=True,
        log_level="INFO",
        enable_introspection_in_production=False,
        require_https=True,
        # Compression
        enable_compression=True,
        compression_algorithm="brotli",
    )

    logger.info("Complete Advanced Configuration:")
    logger.info("  Database URL: %s", config.database_url)
    logger.info("  Environment: %s", config.environment)
    logger.info("  Server: %s:%s", config.axum_host, config.axum_port)
    logger.info("  Workers: %s", config.axum_workers)
    logger.info(
        "  Max Request Size: %.0fMB",
        config.max_request_body_size / 1024 / 1024,
    )
    logger.info("  Request Timeout: %ss", config.request_timeout)
    logger.info("  Log Level: %s", config.log_level)
    logger.info("  HTTPS: %s", config.require_https)
    logger.info(
        "  Compression: %s (%s)",
        config.enable_compression,
        config.compression_algorithm,
    )


# ===== Main Entry Point =====


if __name__ == "__main__":
    import sys

    examples = {
        "default": example_default_configuration,
        "small_requests": example_small_request_size,
        "large_uploads": example_large_file_uploads,
        "development": example_development_environment,
        "production": example_production_environment,
        "logging": example_logging_configuration,
        "timeouts": example_request_timeout_scenarios,
        "sizes": example_request_size_scenarios,
        "security": example_security_configuration,
        "compliance": example_compliance_configuration,
        "custom": example_custom_request_response,
        "env_vars": example_environment_variables,
        "dev_vs_prod": example_development_vs_production,
        "full": example_full_configuration,
    }

    if len(sys.argv) > 1:
        example_name = sys.argv[1]
        if example_name in examples:
            examples[example_name]()
        else:
            print(f"Unknown example: {example_name}")
            print(f"Available: {', '.join(sorted(examples.keys()))}")
            sys.exit(1)
    else:
        # Run all examples
        for name in sorted(examples.keys()):
            examples[name]()

        print("\n" + "=" * 60)
        print("All Phase 3E configuration examples completed!")
        print("=" * 60)
