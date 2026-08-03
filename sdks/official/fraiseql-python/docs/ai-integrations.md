# AI framework integrations

`fraiseql.integrations` wraps a FraiseQL API as AI-agent tools and retrieval
sources, in-process — no separate MCP server hop when both ends are Python.
Every adapter routes calls through the typed `FraiseQLClient`, so
authentication, per-tenant context, and audit logging reuse the client's
existing path: the agent acts as the authenticated user, and RLS applies.

All adapters share one normalised operation model (built from a standard
introspection query), so tool names, argument types, and generated GraphQL
documents are identical across frameworks.

| Module | Needs extra | What you get |
|---|---|---|
| `integrations.openai` | – | OpenAI tool/function definitions + a call dispatcher |
| `integrations.mcp` | – | Raw MCP tool descriptors + a `tools/call` dispatcher |
| `integrations.rag` | – | A framework-agnostic retrieval source over a list query |
| `integrations.langchain` | `fraiseql[langchain]` | LangChain `BaseTool`s + a retriever |
| `integrations.llamaindex` | `fraiseql[llamaindex]` | A LlamaIndex reader |

## OpenAI function calling

```python
from fraiseql.client import FraiseQLClient
from fraiseql.integrations.openai import FraiseQLOpenAIFunctions

client = FraiseQLClient("https://api.example.com/graphql", auth_token=token)
functions = await FraiseQLOpenAIFunctions.from_client(client, exclude=["deleteUser"])

response = openai_client.chat.completions.create(
    model="...", messages=messages, tools=functions.definitions(),
)
for tool_call in response.choices[0].message.tool_calls or []:
    data = await functions.call(tool_call.function.name, tool_call.function.arguments)
```

An unknown function name raises `KeyError` before any server round-trip — a
hallucinated tool never reaches the API.

## Raw MCP (in-process)

```python
from fraiseql.integrations.mcp import FraiseQLMcpTools

tools = await FraiseQLMcpTools.from_client(client, include=["users", "documents"])
tools.list_tools()                              # MCP tools/list entries
await tools.call_tool("users", {"limit": 5})    # MCP tools/call result
```

Descriptors and results are plain dicts in the MCP wire shape — embed them in
any Python MCP server, or use them directly in an agent loop. Errors come back
in-band (`isError: true`), and names outside the include/exclude set are
refused at the dispatch boundary.

## Views as RAG sources

```python
from fraiseql.integrations.rag import as_source

source = as_source(
    client,
    "documents",                       # the list query (e.g. over v_embedded_documents)
    fields=["id", "title", "content"],
    text_key="content",
)
hits = await source.retrieve("how do I rotate credentials?", top_k=5)
# → [{"text": "...", "metadata": {"source": "documents", "id": ..., "title": ...}}, ...]
```

Results are plain dicts, so adapting to any RAG framework's document type is a
one-line comprehension. For native LangChain/LlamaIndex documents use those
adapters' retriever/reader instead.

## Exposure control

Every adapter takes `include` (allowlist) and `exclude` (blocklist) of
operation names at construction; the dispatchers enforce the same set at call
time, so a filtered-out operation is neither advertised nor callable.
