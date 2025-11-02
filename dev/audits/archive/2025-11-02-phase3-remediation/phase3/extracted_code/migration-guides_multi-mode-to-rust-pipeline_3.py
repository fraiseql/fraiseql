# Extracted from: docs/migration-guides/multi-mode-to-rust-pipeline.md
# Block number: 3
# ✅ Works unchanged
result = await repo.find("v_user", where={"name": {"eq": "John"}})
# Returns RustResponseBytes - FastAPI handles this automatically
