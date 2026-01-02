"""Configuration module for FraiseQL."""

import os

from .schema_config import SchemaConfig
from .websocket_config import WebSocketConfig, WebSocketPresets, resolve_websocket_config

__all__ = [
    "LOG_QUERY_BUILDER_MODE",
    "RUST_QB_FALLBACK_ON_ERROR",
    "RUST_QUERY_BUILDER_PERCENTAGE",
    "USE_RUST_QUERY_BUILDER",
    "SchemaConfig",
    "WebSocketConfig",
    "WebSocketPresets",
    "resolve_websocket_config",
]

# Phase 7: Rust Query Builder Configuration
# Enable/disable Rust query builder (default: False for safety)
USE_RUST_QUERY_BUILDER = os.getenv("FRAISEQL_USE_RUST_QUERY_BUILDER", "false").lower() in (
    "true",
    "1",
    "yes",
)

# Gradual rollout percentage (0-100)
# If USE_RUST_QUERY_BUILDER is False, this percentage determines random sampling
RUST_QUERY_BUILDER_PERCENTAGE = int(os.getenv("FRAISEQL_RUST_QB_PERCENTAGE", "0"))

# Log which query builder is used for each query
LOG_QUERY_BUILDER_MODE = os.getenv("FRAISEQL_LOG_QUERY_BUILDER_MODE", "false").lower() in (
    "true",
    "1",
    "yes",
)

# Fallback to Python on Rust errors (default: True for safety)
RUST_QB_FALLBACK_ON_ERROR = os.getenv("FRAISEQL_RUST_QB_FALLBACK", "true").lower() in (
    "true",
    "1",
    "yes",
)
