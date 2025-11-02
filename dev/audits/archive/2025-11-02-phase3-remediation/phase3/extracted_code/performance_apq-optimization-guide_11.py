# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 11
from fraiseql.storage.backends.base import APQStorageBackend


class CustomBackend(APQStorageBackend):
    def get_persisted_query(self, hash_value: str) -> str | None:
        # Your implementation
        pass

    def store_persisted_query(self, hash_value: str, query: str) -> None:
        # Your implementation
        pass

    def get_cached_response(self, hash_value: str, context=None) -> dict | None:
        # Your implementation
        pass

    def store_cached_response(self, hash_value: str, response: dict, context=None) -> None:
        # Your implementation
        pass
