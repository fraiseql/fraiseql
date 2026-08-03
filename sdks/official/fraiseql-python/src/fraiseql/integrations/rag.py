"""Framework-agnostic RAG source over a FraiseQL view.

Wraps a list query (typically backed by a ``v_embedded_documents``-style view)
as a retrieval source any Python RAG stack can consume: ``retrieve`` returns
plain ``{"text": ..., "metadata": {...}}`` dicts, so adapting to a specific
framework's document type is one comprehension. The LangChain and LlamaIndex
integrations ship their own native document types; this module is for
everything else — custom agent loops included.

No extra dependency is required.

Example:
    ```python
    from fraiseql.client import FraiseQLClient
    from fraiseql.integrations.rag import as_source

    client = FraiseQLClient("http://localhost:8080/graphql", auth_token=token)
    source = as_source(
        client,
        "documents",
        fields=["id", "title", "content"],
        text_key="content",
    )
    hits = await source.retrieve("how do I rotate credentials?", top_k=5)
    ```
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from fraiseql.client import FraiseQLClient


class FraiseQLRAGSource:
    """A retrieval source backed by one FraiseQL list query.

    The query is executed through the typed client, so authentication and
    per-tenant context apply — a tenant's retrieval can only see rows its RLS
    lets it see.

    Args:
        client: The connected ``FraiseQLClient``.
        query_field: Root query field to retrieve from (e.g. ``"documents"``).
        fields: Fields to select on each row.
        text_key: The selected field used as the document text; defaults to the
            last entry of ``fields``.
        metadata_keys: Selected fields copied into metadata (None = all others).
        search_arg: Argument name carrying the query text (``"search"``).
        limit_arg: Argument name carrying ``top_k`` (``"limit"``).
        search_type: GraphQL type of the search argument (``"String"``).
    """

    def __init__(
        self,
        client: FraiseQLClient,
        query_field: str,
        *,
        fields: list[str],
        text_key: str | None = None,
        metadata_keys: list[str] | None = None,
        search_arg: str = "search",
        limit_arg: str = "limit",
        search_type: str = "String",
    ) -> None:
        if not fields:
            msg = "fields must name at least one selected field"
            raise ValueError(msg)
        self._client = client
        self._query_field = query_field
        self._fields = list(fields)
        self._text_key = text_key or fields[-1]
        self._metadata_keys = metadata_keys
        self._search_arg = search_arg
        self._limit_arg = limit_arg
        selection = " ".join(self._fields)
        self._document = (
            f"query (${search_arg}: {search_type}, ${limit_arg}: Int) "
            f"{{ {query_field}({search_arg}: ${search_arg}, {limit_arg}: ${limit_arg}) "
            f"{{ {selection} }} }}"
        )

    @property
    def document(self) -> str:
        """The GraphQL document this source executes."""
        return self._document

    async def retrieve(
        self,
        query: str,
        *,
        top_k: int = 10,
    ) -> list[dict[str, Any]]:
        """Retrieve rows matching ``query``.

        Returns:
            ``{"text": str, "metadata": dict}`` per row. ``text`` is the
            ``text_key`` field (full-row JSON when the field is absent);
            metadata carries the other selected fields plus ``source`` (the
            query field name).
        """
        result = await self._client.execute(
            self._document,
            variables={self._search_arg: query, self._limit_arg: top_k},
        )
        rows = result.get("data", {}).get(self._query_field) or []
        if not isinstance(rows, list):
            rows = [rows]

        documents: list[dict[str, Any]] = []
        for row in rows:
            if not isinstance(row, dict):
                continue
            text = row.get(self._text_key)
            metadata: dict[str, Any] = {"source": self._query_field}
            keys = self._metadata_keys
            if keys is None:
                metadata.update({k: v for k, v in row.items() if k != self._text_key})
            else:
                metadata.update({k: row[k] for k in keys if k in row})
            documents.append(
                {
                    "text": str(text) if text is not None else json.dumps(row),
                    "metadata": metadata,
                }
            )
        return documents


def as_source(
    client: FraiseQLClient,
    query_field: str,
    *,
    fields: list[str],
    text_key: str | None = None,
    metadata_keys: list[str] | None = None,
    search_arg: str = "search",
    limit_arg: str = "limit",
    search_type: str = "String",
) -> FraiseQLRAGSource:
    """Wrap a FraiseQL list query as a retrieval source (see the module docs)."""
    return FraiseQLRAGSource(
        client,
        query_field,
        fields=fields,
        text_key=text_key,
        metadata_keys=metadata_keys,
        search_arg=search_arg,
        limit_arg=limit_arg,
        search_type=search_type,
    )
