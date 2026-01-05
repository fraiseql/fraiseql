#!/usr/bin/env python3
"""FraiseQL Axum Middleware Examples.

Demonstrates various middleware configurations for request/response processing,
authentication, logging, rate limiting, and compression.
"""

import logging
from fraiseql import create_axum_fraiseql_app, fraise_type
from fraiseql.axum.middleware import (
    AuthenticationMiddleware,
    CompressionMiddleware,
    MiddlewarePipeline,
    RateLimitMiddleware,
    RequestLoggingMiddleware,
)

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


# ===== Example 1: Basic Request Logging =====


def example_basic_logging() -> None:
    """Basic example with request logging middleware."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 1: Basic Request Logging")
    logger.info("=" * 60)

    middleware = RequestLoggingMiddleware(log_body=False, log_response=False)
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Logs: GET /graphql requests")
    logger.info("Logs: Response status codes")


# ===== Example 2: Request Logging with Body =====


def example_logging_with_body() -> None:
    """Logging middleware that includes request body."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 2: Request Logging with Body")
    logger.info("=" * 60)

    middleware = RequestLoggingMiddleware(log_body=True, log_response=True)
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Logs: Full request including query body")
    logger.info("Logs: Full response including result body")
    logger.info("⚠️ Note: May impact performance with large queries")


# ===== Example 3: Authentication Required =====


def example_authentication() -> None:
    """Authentication middleware requiring Authorization header."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 3: Authentication Required")
    logger.info("=" * 60)

    middleware = AuthenticationMiddleware(
        header_name="Authorization",
        optional_paths=[],
    )
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Requires: Authorization header on all requests")
    logger.info("Blocks: Requests without Authorization")


# ===== Example 4: Authentication with Optional Paths =====


def example_authentication_optional_paths() -> None:
    """Authentication with some paths not requiring auth."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 4: Authentication with Optional Paths")
    logger.info("=" * 60)

    middleware = AuthenticationMiddleware(
        header_name="Authorization",
        optional_paths=["/health", "/status", "/metrics"],
    )
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Requires: Authorization for /graphql requests")
    logger.info("Allows: /health, /status, /metrics without auth")
    logger.info("Use case: Health checks and monitoring don't need auth")


# ===== Example 5: Custom API Key Authentication =====


def example_custom_api_key() -> None:
    """Authentication using custom X-API-Key header."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 5: Custom API Key Authentication")
    logger.info("=" * 60)

    middleware = AuthenticationMiddleware(
        header_name="X-API-Key",
        optional_paths=["/health"],
    )
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Requires: X-API-Key header instead of Authorization")
    logger.info("Pattern: Use for internal API key auth (not Bearer tokens)")


# ===== Example 6: Rate Limiting =====


def example_rate_limiting() -> None:
    """Rate limiting middleware with per-IP tracking."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 6: Rate Limiting")
    logger.info("=" * 60)

    middleware = RateLimitMiddleware(
        requests_per_minute=100,
        requests_per_hour=5000,
    )
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Limits: 100 requests per minute per IP")
    logger.info("Limits: 5000 requests per hour per IP")
    logger.info("Tracking: Per-IP request counts")


# ===== Example 7: Strict Rate Limiting =====


def example_strict_rate_limiting() -> None:
    """Strict rate limiting for production."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 7: Strict Rate Limiting (Production)")
    logger.info("=" * 60)

    middleware = RateLimitMiddleware(
        requests_per_minute=1000,
        requests_per_hour=50000,
    )
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Limits: 1000 requests per minute")
    logger.info("Limits: 50000 requests per hour")
    logger.info("Use case: Shared/public APIs")


# ===== Example 8: Response Compression =====


def example_compression() -> None:
    """Response compression middleware."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 8: Response Compression")
    logger.info("=" * 60)

    middleware = CompressionMiddleware(algorithm="gzip", min_bytes=256)
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Algorithm: gzip")
    logger.info("Min Size: 256 bytes (responses < 256 bytes not compressed)")


# ===== Example 9: Brotli Compression =====


def example_brotli_compression() -> None:
    """High-compression brotli middleware."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 9: Brotli Compression (High Compression)")
    logger.info("=" * 60)

    middleware = CompressionMiddleware(algorithm="brotli", min_bytes=1024)
    pipeline = MiddlewarePipeline([middleware])

    logger.info(f"Middleware: {[m.__class__.__name__ for m in pipeline.middleware]}")
    logger.info("Algorithm: brotli (better compression, slower)")
    logger.info("Min Size: 1024 bytes (only compress large responses)")
    logger.info("Use case: Good bandwidth savings for large GraphQL responses")


# ===== Example 10: Logging + Authentication =====


def example_logging_auth_pipeline() -> None:
    """Combined logging and authentication pipeline."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 10: Logging + Authentication Pipeline")
    logger.info("=" * 60)

    pipeline = MiddlewarePipeline(
        [
            RequestLoggingMiddleware(log_body=True),
            AuthenticationMiddleware(),
        ]
    )

    logger.info("Middleware order:")
    for i, mw in enumerate(pipeline.middleware, 1):
        logger.info(f"  {i}. {mw.__class__.__name__}")

    logger.info("\nRequest flow:")
    logger.info("  1. Log request (with body)")
    logger.info("  2. Verify Authorization header")
    logger.info("  3. Block if no auth")

    logger.info("\nResponse flow (reverse order):")
    logger.info("  1. Check response")
    logger.info("  2. Log response (not modified)")


# ===== Example 11: Full Production Pipeline =====


def example_production_pipeline() -> None:
    """Complete production-ready middleware pipeline."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 11: Full Production Pipeline")
    logger.info("=" * 60)

    pipeline = MiddlewarePipeline(
        [
            RequestLoggingMiddleware(log_body=False, log_response=True),
            AuthenticationMiddleware(
                optional_paths=["/health", "/metrics"]
            ),
            RateLimitMiddleware(requests_per_minute=1000),
            CompressionMiddleware(algorithm="brotli", min_bytes=1024),
        ]
    )

    logger.info("Middleware order (request → response):")
    for i, mw in enumerate(pipeline.middleware, 1):
        logger.info(f"  {i}. {mw.__class__.__name__}")

    logger.info("\nRequest security layers:")
    logger.info("  1. Log all requests")
    logger.info("  2. Require authentication (except health)")
    logger.info("  3. Rate limit to 1000 req/min")
    logger.info("  4. Prepare compression")

    logger.info("\nResponse processing (reverse order):")
    logger.info("  1. Compress if > 1KB")
    logger.info("  2. Rate limit tracking")
    logger.info("  3. Log response status")


# ===== Example 12: Development vs Production =====


def example_dev_vs_prod() -> None:
    """Comparison of dev and production pipelines."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 12: Development vs Production Pipelines")
    logger.info("=" * 60)

    import os

    env = os.getenv("ENVIRONMENT", "development")

    if env == "development":
        pipeline = MiddlewarePipeline(
            [
                RequestLoggingMiddleware(log_body=True, log_response=True),
                # No auth in development
            ]
        )
        logger.info("Environment: DEVELOPMENT")
        logger.info("Middleware: Logging only (with body)")
        logger.info("Auth: Disabled for easier testing")
    else:
        pipeline = MiddlewarePipeline(
            [
                RequestLoggingMiddleware(log_body=False, log_response=True),
                AuthenticationMiddleware(),
                RateLimitMiddleware(requests_per_minute=1000),
                CompressionMiddleware(algorithm="brotli", min_bytes=1024),
            ]
        )
        logger.info("Environment: PRODUCTION")
        logger.info("Middleware: Full security and compression")
        logger.info("Auth: Required on all requests")

    logger.info(f"Middleware count: {len(pipeline.middleware)}")


# ===== Example 13: Custom Middleware Extension =====


def example_custom_middleware() -> None:
    """Example of extending AxumMiddleware for custom behavior."""
    logger.info("\n" + "=" * 60)
    logger.info("Example 13: Custom Middleware Extension")
    logger.info("=" * 60)

    logger.info("You can extend AxumMiddleware for custom logic:")
    logger.info("\nExample custom middleware code:")
    logger.info("""
    from fraiseql.axum.middleware import AxumMiddleware

    class CustomMetricsMiddleware(AxumMiddleware):
        async def process_request(self, request_data):
            # Track request start time
            self.start_time = time.time()
            return request_data

        async def process_response(self, response_data):
            # Calculate duration and record metric
            duration = time.time() - self.start_time
            metrics.record("request_duration", duration)
            return response_data
    """)

    logger.info("\nThen add to pipeline:")
    logger.info("  pipeline.add(CustomMetricsMiddleware())")


# ===== Main Entry Point =====


if __name__ == "__main__":
    import sys

    examples = {
        "basic_logging": example_basic_logging,
        "logging_with_body": example_logging_with_body,
        "authentication": example_authentication,
        "auth_optional_paths": example_authentication_optional_paths,
        "custom_api_key": example_custom_api_key,
        "rate_limiting": example_rate_limiting,
        "strict_rate_limiting": example_strict_rate_limiting,
        "compression": example_compression,
        "brotli_compression": example_brotli_compression,
        "logging_auth_pipeline": example_logging_auth_pipeline,
        "production_pipeline": example_production_pipeline,
        "dev_vs_prod": example_dev_vs_prod,
        "custom_middleware": example_custom_middleware,
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
        print("All middleware examples completed!")
        print("=" * 60)
