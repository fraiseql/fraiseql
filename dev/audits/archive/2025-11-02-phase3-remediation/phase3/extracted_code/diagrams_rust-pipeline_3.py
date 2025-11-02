# Extracted from: docs/diagrams/rust-pipeline.md
# Block number: 3
async def safe_execute_with_pipeline(self, query, data):
    try:
        return await self.rust_pipeline.process(data)
    except RustPipelineError as e:
        # Log error for monitoring
        logger.warning(f"Rust pipeline failed: {e}, falling back to Python")

        # Fallback to Python processing
        return await self.python_fallback.process(data)
