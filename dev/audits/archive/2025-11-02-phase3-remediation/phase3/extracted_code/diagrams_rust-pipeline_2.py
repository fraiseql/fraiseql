# Extracted from: docs/diagrams/rust-pipeline.md
# Block number: 2
# Automatic selection based on query complexity
pipeline_config = {
    "enable_rust_pipeline": True,
    "complexity_threshold": 10,  # Use Rust for queries above this score
    "memory_limit": "100MB",  # Max memory per pipeline
    "concurrency_limit": 4,  # Max concurrent pipelines
}
