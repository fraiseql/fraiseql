# Extracted from: docs/tutorials/production-deployment.md
# Block number: 3
config = FraiseQLConfig(
    # Layer 0: Rust (10-80x faster)
    rust_enabled=True,
    # Layer 1: APQ (5-10x faster)
    apq_enabled=True,
    apq_storage_backend="postgresql",
    # Layer 2: TurboRouter (3-5x faster)
    enable_turbo_router=True,
    turbo_router_cache_size=500,
    # Layer 3: JSON Passthrough (2-3x faster)
    json_passthrough_enabled=True,
    # Combined: 0.5-2ms cached responses
)
