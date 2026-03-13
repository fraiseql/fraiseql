"""LlamaIndex integration for FraiseQL.

Provides a reader that loads FraiseQL query results as LlamaIndex Documents.

Requires the ``llamaindex`` extra: ``pip install fraiseql[llamaindex]``

Example:
    ```python
    from fraiseql.integrations.llamaindex import FraiseQLReader
    from fraiseql.client import FraiseQLClient

    client = FraiseQLClient("http://localhost:8080/graphql")
    reader = FraiseQLReader(client=client)
    documents = await reader.aload_data(
        query="{ users { id name email } }",
        text_template="{name} ({email})",
    )
    ```
"""

from __future__ import annotations

import json
from typing import Any

from fraiseql.client import FraiseQLClient

try:
    from llama_index.core import Document
    from llama_index.core.readers.base import BaseReader
except ImportError as exc:
    raise ImportError(
        "llama-index-core is required for FraiseQL LlamaIndex integration. "
        "Install it with: pip install fraiseql[llamaindex]"
    ) from exc


class FraiseQLReader(BaseReader):
    """Loads FraiseQL query results as LlamaIndex Documents.

    Args:
        client: A ``FraiseQLClient`` instance.
    """

    def __init__(self, client: FraiseQLClient) -> None:
        super().__init__()
        self._client = client

    def load_data(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        text_template: str | None = None,
        metadata_fields: list[str] | None = None,
    ) -> list[Document]:
        """Synchronous wrapper — raises NotImplementedError; use ``aload_data``."""
        raise NotImplementedError("Use aload_data() for async execution")

    async def aload_data(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        text_template: str | None = None,
        metadata_fields: list[str] | None = None,
    ) -> list[Document]:
        """Execute a query and return results as LlamaIndex Documents.

        Args:
            query: GraphQL query string.
            variables: Optional query variables.
            text_template: Python format string for Document text.
                Uses field names as keys (e.g., ``"{name} ({email})"``)
                If None, the full JSON is used as text.
            metadata_fields: Fields to include in metadata. If None, all fields are included.

        Returns:
            List of LlamaIndex Document objects.
        """
        result = await self._client.execute(query, variables=variables)
        data = result.get("data", {})

        documents: list[Document] = []
        for key, value in data.items():
            items = value if isinstance(value, list) else [value]
            for item in items:
                if not isinstance(item, dict):
                    continue

                text = text_template.format(**item) if text_template else json.dumps(item)

                metadata: dict[str, Any] = {"source_query": key}
                if metadata_fields:
                    for field_name in metadata_fields:
                        if field_name in item:
                            metadata[field_name] = item[field_name]
                else:
                    metadata.update(item)

                documents.append(Document(text=text, metadata=metadata))

        return documents
