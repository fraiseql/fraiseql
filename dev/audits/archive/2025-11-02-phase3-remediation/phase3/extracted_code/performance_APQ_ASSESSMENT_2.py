# Extracted from: docs/performance/APQ_ASSESSMENT.md
# Block number: 2
# src/fraiseql/fastapi/config.py
apq_storage_backend: Literal["memory", "postgresql", "redis", "custom"] = "memory"
apq_cache_responses: bool = False  # ⚠️ DISABLED BY DEFAULT
apq_response_cache_ttl: int = 600  # 10 minutes
