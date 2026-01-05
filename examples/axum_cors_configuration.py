#!/usr/bin/env python3
"""FraiseQL Axum CORS Configuration Examples.

Demonstrates various CORS setup options for different environments.
"""

import logging
from fraiseql import create_axum_fraiseql_app, fraise_type
from fraiseql.axum.cors import CORSConfig

# Enable logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)


# ===== GraphQL Types =====

@fraise_type
class User:
    """Example user type."""

    id: str
    name: str
    email: str


# ===== Example 1: Development (Permissive CORS) =====

def example_development_permissive() -> None:
    """Development setup with permissive CORS (allow all origins).

    ⚠️ Use only in development! Not suitable for production.
    """
    logger.info("\n" + "=" * 60)
    logger.info("Example 1: Development (Permissive CORS)")
    logger.info("=" * 60)

    cors_config = CORSConfig.permissive()
    logger.info(f"CORS Config: {cors_config}")

    app = create_axum_fraiseql_app(
        database_url="postgresql://localhost/fraiseql_test",
        types=[User],
        cors_config=cors_config.to_dict(),
    )

    logger.info(f"App created: {app}")
    logger.info(f"Allows: all origins (wildcard)")
    logger.info(f"Credentials: not allowed (required with wildcard)")


# ===== Example 2: Localhost Development =====

def example_localhost_development() -> None:
    """Development setup for localhost on common dev ports."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 2: Localhost Development")
    logger.info("=" * 60)

    cors_config = CORSConfig.localhost([3000, 3001, 4200, 5173])
    logger.info(f"CORS Config: {cors_config}")

    app = create_axum_fraiseql_app(
        database_url="postgresql://localhost/fraiseql_test",
        types=[User],
        cors_config=cors_config.to_dict(),
    )

    logger.info(f"Allows: {cors_config.allow_origins}")
    logger.info(f"Credentials: allowed")
    logger.info(f"Preflight cache: disabled (max_age=0)")


# ===== Example 3: Production (Single Domain) =====

def example_production_single_domain() -> None:
    """Production setup for a single domain (HTTPS only)."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 3: Production (Single Domain)")
    logger.info("=" * 60)

    cors_config = CORSConfig.production("example.com")
    logger.info(f"CORS Config: {cors_config}")

    app = create_axum_fraiseql_app(
        database_url="postgresql://prod-host/db",
        types=[User],
        cors_config=cors_config.to_dict(),
    )

    logger.info(f"Allows: {cors_config.allow_origins}")
    logger.info(f"Credentials: allowed")
    logger.info(f"Protocol: HTTPS only")


# ===== Example 4: Production (Single Domain + Subdomains) =====

def example_production_with_subdomains() -> None:
    """Production setup for domain and all subdomains."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 4: Production (Domain + Subdomains)")
    logger.info("=" * 60)

    cors_config = CORSConfig.production("example.com", allow_subdomains=True)
    logger.info(f"CORS Config: {cors_config}")

    app = create_axum_fraiseql_app(
        database_url="postgresql://prod-host/db",
        types=[User],
        cors_config=cors_config.to_dict(),
    )

    logger.info(f"Allows: {cors_config.allow_origins}")
    logger.info(f"Includes: example.com, *.example.com")


# ===== Example 5: Production (Multi-Tenant) =====

def example_production_multi_tenant() -> None:
    """Production setup for multiple domains (e.g., different customers)."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 5: Production (Multi-Tenant)")
    logger.info("=" * 60)

    domains = [
        "customer1.example.com",
        "customer2.example.com",
        "customer3.example.com",
    ]

    cors_config = CORSConfig.multi_tenant(domains)
    logger.info(f"CORS Config: {cors_config}")

    app = create_axum_fraiseql_app(
        database_url="postgresql://prod-host/db",
        types=[User],
        cors_config=cors_config.to_dict(),
    )

    logger.info(f"Allows: {cors_config.allow_origins}")
    logger.info(f"Tenant count: {len(domains)}")


# ===== Example 6: Custom CORS Configuration =====

def example_custom_configuration() -> None:
    """Custom CORS configuration with fine-grained control."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 6: Custom Configuration")
    logger.info("=" * 60)

    cors_config = CORSConfig.custom(
        allow_origins=[
            "https://app.example.com",
            "https://admin.example.com",
        ],
        allow_credentials=True,
        allow_methods=["GET", "POST", "PUT", "DELETE"],
        allow_headers=["Authorization", "Content-Type", "X-API-Key"],
        expose_headers=["X-Total-Count", "X-Page-Number"],
        max_age=7200,  # 2 hours
    )

    logger.info(f"CORS Config: {cors_config}")

    app = create_axum_fraiseql_app(
        database_url="postgresql://prod-host/db",
        types=[User],
        cors_config=cors_config.to_dict(),
    )

    logger.info(f"Allowed origins: {cors_config.allow_origins}")
    logger.info(f"Allowed methods: {cors_config.allow_methods}")
    logger.info(f"Allowed headers: {cors_config.allow_headers}")
    logger.info(f"Exposed headers: {cors_config.expose_headers}")
    logger.info(f"Preflight cache: {cors_config.max_age} seconds")


# ===== Example 7: Staging Environment =====

def example_staging_environment() -> None:
    """Staging environment setup (HTTPS with relaxed restrictions)."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 7: Staging Environment")
    logger.info("=" * 60)

    cors_config = CORSConfig.production(
        "staging.example.com",
        allow_subdomains=True,
        https_only=True,
    )

    logger.info(f"CORS Config: {cors_config}")

    app = create_axum_fraiseql_app(
        database_url="postgresql://staging-host/db",
        types=[User],
        cors_config=cors_config.to_dict(),
    )

    logger.info(f"Allows: {cors_config.allow_origins}")
    logger.info(f"Staging domain with all subdomains")


# ===== Example 8: Mixed Environment (Dev + Prod) =====

def example_mixed_environment() -> None:
    """Configuration that works for both dev and prod."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 8: Mixed Environment")
    logger.info("=" * 60)

    import os

    # Determine environment
    env = os.getenv("ENVIRONMENT", "development")

    if env == "development":
        cors_config = CORSConfig.localhost([3000, 4200])
        db_url = "postgresql://localhost/fraiseql_test"
    else:
        cors_config = CORSConfig.production(
            os.getenv("DOMAIN", "example.com"),
            allow_subdomains=True,
        )
        db_url = os.getenv("DATABASE_URL")

    logger.info(f"Environment: {env}")
    logger.info(f"CORS Config: {cors_config}")

    app = create_axum_fraiseql_app(
        database_url=db_url,
        types=[User],
        cors_config=cors_config.to_dict(),
    )

    logger.info(f"Allows: {cors_config.allow_origins}")


# ===== Main Entry Point =====

if __name__ == "__main__":
    import sys

    examples = {
        "permissive": example_development_permissive,
        "localhost": example_localhost_development,
        "production": example_production_single_domain,
        "subdomains": example_production_with_subdomains,
        "multi_tenant": example_production_multi_tenant,
        "custom": example_custom_configuration,
        "staging": example_staging_environment,
        "mixed": example_mixed_environment,
    }

    if len(sys.argv) > 1:
        example_name = sys.argv[1]
        if example_name in examples:
            examples[example_name]()
        else:
            print(f"Unknown example: {example_name}")
            print(f"Available: {', '.join(examples.keys())}")
            sys.exit(1)
    else:
        # Run all examples
        for name, example in examples.items():
            example()
        print("\n" + "=" * 60)
        print("All CORS examples completed!")
        print("=" * 60)
