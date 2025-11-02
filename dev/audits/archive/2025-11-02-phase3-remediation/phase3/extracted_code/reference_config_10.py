# Extracted from: docs/reference/config.md
# Block number: 10
# v0.11.4 and earlier (OLD - remove these)
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    execution_mode_priority=["turbo", "passthrough", "normal"],  # ❌ Remove
    enable_python_fallback=True,  # ❌ Remove
    passthrough_detection_enabled=True,  # ❌ Remove
)

# v1.0.0+ - Exclusive Rust pipeline
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    # ✅ Rust pipeline always active, minimal config needed
)
