"""LangChain integration for FraiseQL.

Provides a toolkit that exposes FraiseQL queries and mutations as LangChain tools,
and a retriever that converts query results into LangChain Documents.

Requires the ``langchain`` extra: ``pip install fraiseql[langchain]``

Example:
    ```python
    from fraiseql.integrations.langchain import FraiseQLToolkit

    toolkit = await FraiseQLToolkit.from_url("http://localhost:8080/graphql")
    tools = toolkit.get_tools()
    # Pass tools to an LLM agent
    ```
"""

from __future__ import annotations

import json
from typing import Any

from pydantic import ConfigDict

from fraiseql.client import FraiseQLClient

try:
    from langchain_core.callbacks import (
        AsyncCallbackManagerForRetrieverRun,
        CallbackManagerForRetrieverRun,
        CallbackManagerForToolRun,
    )
    from langchain_core.documents import Document
    from langchain_core.retrievers import BaseRetriever
    from langchain_core.tools import BaseTool
except ImportError as exc:
    raise ImportError(
        "langchain-core is required for FraiseQL LangChain integration. "
        "Install it with: pip install fraiseql[langchain]"
    ) from exc


class FraiseQLTool(BaseTool):
    """A LangChain tool wrapping a single FraiseQL query or mutation."""

    model_config = ConfigDict(arbitrary_types_allowed=True)

    name: str
    description: str
    client: Any  # FraiseQLClient (not serializable by pydantic)
    query_template: str
    is_mutation: bool = False

    def _run(
        self,
        tool_input: str = "",
        run_manager: CallbackManagerForToolRun | None = None,
    ) -> str:
        raise NotImplementedError("Use async version via ainvoke()")

    async def _arun(
        self,
        tool_input: str = "",
        run_manager: CallbackManagerForToolRun | None = None,
    ) -> str:
        variables: dict[str, Any] | None = None
        if tool_input:
            try:
                variables = json.loads(tool_input)
            except json.JSONDecodeError:
                return json.dumps({"error": f"Invalid JSON input: {tool_input}"})

        try:
            result = await self.client.execute(self.query_template, variables=variables)
            return json.dumps(result.get("data", {}))
        except Exception as e:
            return json.dumps({"error": str(e)})


class FraiseQLToolkit:
    """Generates LangChain tools from a FraiseQL schema via introspection."""

    def __init__(
        self,
        client: FraiseQLClient,
        schema_data: dict[str, Any],
    ) -> None:
        self._client = client
        self._schema_data = schema_data

    @classmethod
    async def from_url(
        cls,
        url: str,
        *,
        auth_token: str | None = None,
        api_key: str | None = None,
    ) -> FraiseQLToolkit:
        """Create a toolkit by introspecting a FraiseQL server.

        Args:
            url: GraphQL endpoint URL.
            auth_token: Optional bearer token.
            api_key: Optional API key.
        """
        client = FraiseQLClient(url, auth_token=auth_token, api_key=api_key)
        result = await client.introspect()
        return cls(client=client, schema_data=result)

    def get_tools(
        self,
        *,
        include: list[str] | None = None,
        exclude: list[str] | None = None,
    ) -> list[BaseTool]:
        """Generate LangChain tools from the introspected schema.

        Args:
            include: Whitelist of operation names (None = all).
            exclude: Blacklist of operation names (None = none).
        """
        tools: list[BaseTool] = []
        schema_info = self._schema_data.get("data", {}).get("__schema", {})

        for type_info in schema_info.get("types", []):
            type_name = type_info.get("name", "")
            is_mutation = type_name == "Mutation"
            is_query = type_name == "Query"

            if not (is_query or is_mutation):
                continue

            for field in type_info.get("fields", []):
                name = field["name"]
                if name.startswith("__"):
                    continue
                if include and name not in include:
                    continue
                if exclude and name in exclude:
                    continue

                args = field.get("args", [])
                arg_names = [a["name"] for a in args]
                arg_str = ", ".join(f"${a}: String" for a in arg_names)
                param_str = ", ".join(f"{a}: ${a}" for a in arg_names)

                op_type = "mutation" if is_mutation else "query"
                if arg_names:
                    query_template = f"{op_type} ({arg_str}) {{ {name}({param_str}) }}"
                else:
                    query_template = f"{{ {name} }}"

                description = field.get("description") or f"Execute {name} {op_type}"
                if args:
                    arg_desc = ", ".join(
                        f"{a['name']}: {a.get('type', {}).get('name', 'String')}" for a in args
                    )
                    description += f". Arguments (JSON): {arg_desc}"

                tools.append(
                    FraiseQLTool(
                        name=name,
                        description=description,
                        client=self._client,
                        query_template=query_template,
                        is_mutation=is_mutation,
                    )
                )

        return tools


class FraiseQLRetriever(BaseRetriever):
    """A LangChain retriever that executes a FraiseQL query and returns Documents."""

    model_config = ConfigDict(arbitrary_types_allowed=True)

    client: Any  # FraiseQLClient
    query: str
    variables: dict[str, Any] | None = None
    text_key: str = "name"
    metadata_keys: list[str] | None = None

    def _get_relevant_documents(
        self,
        query: str,
        *,
        run_manager: CallbackManagerForRetrieverRun | None = None,
    ) -> list[Document]:
        raise NotImplementedError("Use async version via ainvoke()")

    async def _aget_relevant_documents(
        self,
        query: str,
        *,
        run_manager: AsyncCallbackManagerForRetrieverRun,
    ) -> list[Document]:
        variables = dict(self.variables or {})
        if query:
            variables["search"] = query

        result = await self.client.execute(self.query, variables=variables)
        data = result.get("data", {})

        documents: list[Document] = []
        for key, value in data.items():
            items = value if isinstance(value, list) else [value]
            for item in items:
                if not isinstance(item, dict):
                    continue
                text = str(item.get(self.text_key, json.dumps(item)))
                metadata: dict[str, Any] = {"source": key}
                if self.metadata_keys:
                    for mk in self.metadata_keys:
                        if mk in item:
                            metadata[mk] = item[mk]
                else:
                    metadata.update(item)
                documents.append(Document(page_content=text, metadata=metadata))

        return documents
