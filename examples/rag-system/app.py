"""RAG System Example - Retrieval-Augmented Generation with FraiseQL + LangChain"""

import asyncio
import os
from typing import List, Optional
from uuid import UUID

import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

# FraiseQL imports - CORRECTED
import fraiseql
from fraiseql import fraise_field, fraise_type
from fraiseql.types.scalars import UUID as FraiseUUID

# LangChain imports - UPDATED to langchain_openai
try:
    from langchain.chains import RetrievalQA
    from langchain.docstore.document import Document
    from langchain_openai import ChatOpenAI, OpenAIEmbeddings
except ImportError:
    print("⚠️  LangChain not installed. Install with: pip install langchain langchain-openai")
    OpenAIEmbeddings = None
    ChatOpenAI = None
    RetrievalQA = None
    Document = None

# Direct database access for vector operations
import psycopg


# Pydantic models for API
class DocumentCreate(BaseModel):
    title: str
    content: str
    source: Optional[str] = None
    metadata: Optional[dict] = {}


class DocumentResponse(BaseModel):
    id: UUID
    title: str
    content: str
    source: Optional[str]
    metadata: dict
    created_at: str


class EmbeddingUpdate(BaseModel):
    embedding: List[float]
    model: str = "text-embedding-ada-002"


class SearchQuery(BaseModel):
    query: str
    limit: int = 5
    similarity_threshold: float = 0.7


class RAGQuery(BaseModel):
    question: str
    context_limit: int = 3


# FraiseQL types
@fraise_type
class TBDocument:
    """Document table type following trinity pattern."""

    id: UUID
    title: str
    content: str
    source: Optional[str]
    metadata: dict
    created_at: str
    updated_at: str


@fraise_type
class TVDocumentEmbedding:
    """Document embedding table view type."""

    id: UUID
    document_id: UUID
    embedding: List[float]
    embedding_model: str
    created_at: str


# ✅ CORRECT PATTERN - Class-based resolvers
@fraise_type
class QueryRoot:
    """Root query type for RAG system."""

    documents: List[TBDocument] = fraise_field(
        description="Get documents with optional source filtering"
    )

    async def resolve_documents(
        self, info, limit: int = 50, source: Optional[str] = None
    ) -> List[TBDocument]:
        """Get documents with optional source filtering."""
        repo = info.context["repo"]

        where = {}
        if source:
            where["source"] = {"eq": source}

        results = await repo.find(
            "v_document",  # Use VIEW not table
            where=where,
            orderBy={"created_at": "DESC"},
            limit=limit,
        )

        return [TBDocument(**doc) for doc in results]


@fraise_type
class MutationRoot:
    """Root mutation type for RAG system."""

    create_document: TBDocument = fraise_field(description="Create a new document")

    update_document_embedding: bool = fraise_field(description="Update document embedding")

    async def resolve_create_document(
        self,
        info,
        title: str,
        content: str,
        source: Optional[str] = None,
        metadata: Optional[dict] = None,
    ) -> TBDocument:
        """Create a new document."""
        repo = info.context["repo"]

        if metadata is None:
            metadata = {}

        # Call the PostgreSQL function from schema.sql
        doc_id = await repo.call_function(
            "create_document_with_embedding",
            p_title=title,
            p_content=content,
            p_source=source,
            p_metadata=metadata,
            p_embedding=None,  # Will be added separately via REST endpoint
        )

        # Fetch the created document
        result = await repo.find_one("v_document", where={"id": doc_id})
        return TBDocument(**result)

    async def resolve_update_document_embedding(
        self,
        info,
        document_id: UUID,
        embedding: List[float],
        embedding_model: str = "text-embedding-ada-002",
    ) -> bool:
        """Update document embedding."""
        repo = info.context["repo"]

        # Call the PostgreSQL function from schema.sql
        result = await repo.call_function(
            "update_document_embedding",
            p_document_id=document_id,
            p_embedding=embedding,
            p_embedding_model=embedding_model,
        )

        return bool(result)


# RAG Service class - FIXED with local model support
class RAGService:
    """Service for RAG operations with support for OpenAI or local models."""

    def __init__(
        self,
        database_url: str,
        openai_api_key: Optional[str] = None,
        use_local_embeddings: bool = False,
        local_model_url: str = "http://localhost:8000/v1",
    ):
        self.database_url = database_url
        self.openai_api_key = openai_api_key
        self.use_local_embeddings = use_local_embeddings

        # Initialize embeddings and LLM
        if use_local_embeddings or not openai_api_key:
            print("ℹ️  Using local embedding model...")
            try:
                from local_embeddings import get_embedding_provider

                self.embeddings = get_embedding_provider(
                    provider="local" if use_local_embeddings else "auto",
                    openai_api_key=openai_api_key,
                )
                print(f"✓ Embeddings: Local (dimensions: {self.embeddings.dimensions})")

                # For LLM, use local vLLM server if available
                try:
                    from openai import AsyncOpenAI

                    self.llm = AsyncOpenAI(
                        base_url=local_model_url, api_key="not-needed-for-local"
                    )
                    print(f"✓ LLM: Local vLLM ({local_model_url})")
                except ImportError:
                    print("⚠️  openai package not installed for local LLM")
                    self.llm = None

            except Exception as e:
                print(f"⚠️  Failed to initialize local models: {e}")
                self.embeddings = None
                self.llm = None
        else:
            print("ℹ️  Using OpenAI models...")
            try:
                from langchain_openai import ChatOpenAI, OpenAIEmbeddings

                self.embeddings = OpenAIEmbeddings(openai_api_key=openai_api_key)
                self.llm = ChatOpenAI(openai_api_key=openai_api_key)
                print("✓ Embeddings: OpenAI (text-embedding-ada-002)")
                print("✓ LLM: OpenAI (gpt-3.5-turbo)")
            except ImportError:
                print(
                    "⚠️  langchain-openai not installed. Install with: pip install langchain-openai"
                )
                self.embeddings = None
                self.llm = None

    async def semantic_search(self, query: str, limit: int = 5) -> List[dict]:
        """Perform semantic search using raw SQL."""
        if not self.embeddings:
            raise HTTPException(status_code=500, detail="Embeddings not available")

        # Generate query embedding (works with both OpenAI and local)
        if hasattr(self.embeddings, "aembed_query"):
            # Local embeddings with async support
            query_embedding = await self.embeddings.aembed_query(query)
        else:
            # OpenAI embeddings (sync, run in thread)
            query_embedding = await asyncio.to_thread(self.embeddings.embed_query, query)

        # Use psycopg directly for vector search

        conn = await psycopg.AsyncConnection.connect(self.database_url)
        try:
            async with conn.cursor() as cur:
                await cur.execute(
                    """
                    SELECT
                        d.id,
                        d.title,
                        d.content,
                        d.source,
                        d.metadata,
                        (1 - (e.embedding <=> %s::vector))::REAL as similarity
                    FROM tb_document d
                    JOIN tv_document_embedding e ON d.id = e.document_id
                    WHERE (1 - (e.embedding <=> %s::vector)) >= 0.7
                    ORDER BY (e.embedding <=> %s::vector)
                    LIMIT %s
                    """,
                    (query_embedding, query_embedding, query_embedding, limit),
                )
                results = await cur.fetchall()
                return [
                    {
                        "id": row[0],
                        "title": row[1],
                        "content": row[2],
                        "source": row[3],
                        "metadata": row[4],
                        "similarity": row[5],
                    }
                    for row in results
                ]
        finally:
            await conn.close()

    async def add_document_with_embedding(
        self,
        title: str,
        content: str,
        source: Optional[str] = None,
        metadata: Optional[dict] = None,
    ) -> UUID:
        """Add document and generate embedding."""
        if not self.embeddings:
            raise HTTPException(status_code=500, detail="Embeddings not available")

        # Generate embedding (works with both OpenAI and local)
        if hasattr(self.embeddings, "aembed_query"):
            # Local embeddings with async support
            embedding = await self.embeddings.aembed_query(content)
        else:
            # OpenAI embeddings (sync, run in thread)
            embedding = await asyncio.to_thread(self.embeddings.embed_query, content)

        # Use psycopg to call the function

        conn = await psycopg.AsyncConnection.connect(self.database_url)
        try:
            async with conn.cursor() as cur:
                await cur.execute(
                    """
                    SELECT create_document_with_embedding(
                        %s, %s, %s, %s, %s::vector, %s
                    )
                    """,
                    (title, content, source, metadata or {}, embedding, "text-embedding-ada-002"),
                )
                result = await cur.fetchone()
                return result[0]
        finally:
            await conn.close()

    async def answer_question(self, question: str, context_limit: int = 3) -> dict:
        """Answer question using RAG."""
        if not self.llm:
            raise HTTPException(status_code=500, detail="LLM not available")

        # Get relevant documents
        search_results = await self.semantic_search(question, limit=context_limit)

        # Format context
        context = "\n\n".join(
            [f"Document: {doc['title']}\n{doc['content']}" for doc in search_results]
        )

        # Generate answer using LangChain
        from langchain.chains import RetrievalQA
        from langchain.docstore.document import Document

        # Create documents for LangChain
        docs = [
            Document(
                page_content=doc["content"], metadata={"title": doc["title"], "id": str(doc["id"])}
            )
            for doc in search_results
        ]

        # Simple QA chain
        from langchain.chains.question_answering import load_qa_chain

        chain = load_qa_chain(self.llm, chain_type="stuff")

        response = await chain.arun(input_documents=docs, question=question)

        return {
            "question": question,
            "answer": response,
            "sources": [
                {"id": doc["id"], "title": doc["title"], "similarity": doc["similarity"]}
                for doc in search_results
            ],
        }


# ✅ CORRECT - Use create_fraiseql_app
app = fraiseql.create_fraiseql_app(
    queries=[QueryRoot],
    mutations=[MutationRoot],
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost:5432/ragdb"),
)

# RAG service instance
rag_service = None


@app.on_event("startup")
async def startup_event():
    """Initialize RAG service."""
    global rag_service
    database_url = os.getenv("DATABASE_URL", "postgresql://localhost:5432/ragdb")
    openai_api_key = os.getenv("OPENAI_API_KEY")
    use_local = os.getenv("USE_LOCAL_EMBEDDINGS", "false").lower() in ("true", "1", "yes")
    local_model_url = os.getenv("LOCAL_MODEL_URL", "http://localhost:8000/v1")

    print("\n" + "=" * 50)
    print("Initializing RAG Service")
    print("=" * 50)

    try:
        rag_service = RAGService(
            database_url=database_url,
            openai_api_key=openai_api_key,
            use_local_embeddings=use_local,
            local_model_url=local_model_url,
        )
        print("=" * 50 + "\n")
    except Exception as e:
        print(f"⚠️  Failed to initialize RAG service: {e}")
        print(
            "Note: You can still use GraphQL operations, but embedding features won't work."
        )
        print("=" * 50 + "\n")
        rag_service = None


# Additional REST endpoints for RAG operations
@app.post("/api/documents/search")
async def search_endpoint(search_query: SearchQuery):
    """Search documents semantically."""
    if not rag_service:
        raise HTTPException(status_code=500, detail="RAG service not available")

    results = await rag_service.semantic_search(search_query.query, limit=search_query.limit)

    return {"query": search_query.query, "results": results}


@app.post("/api/rag/ask")
async def ask_endpoint(rag_query: RAGQuery):
    """Ask question using RAG."""
    if not rag_service:
        raise HTTPException(status_code=500, detail="RAG service not available")

    response = await rag_service.answer_question(
        rag_query.question, context_limit=rag_query.context_limit
    )

    return response


@app.post("/api/documents/embed")
async def embed_document(doc: DocumentCreate):
    """Create document with embedding."""
    if not rag_service:
        raise HTTPException(status_code=500, detail="RAG service not available")

    doc_id = await rag_service.add_document_with_embedding(
        doc.title, doc.content, source=doc.source, metadata=doc.metadata
    )

    return {"id": doc_id, "message": "Document created with embedding"}


@app.get("/health")
async def health_check():
    """Health check endpoint for Docker."""
    return {
        "status": "healthy",
        "rag_service": "available" if rag_service else "unavailable",
        "openai_configured": rag_service.embeddings is not None if rag_service else False,
    }


if __name__ == "__main__":
    port = int(os.getenv("PORT", "8000"))

    print("🚀 RAG System Example")
    print("📚 Features:")
    print("   • Document storage with trinity pattern")
    print("   • Vector embeddings with pgvector")
    print("   • Semantic search via GraphQL")
    print("   • RAG question answering")
    print("   • Support for OpenAI or local embeddings")
    print(f"\n📝 GraphQL endpoint: http://localhost:{port}/graphql")
    print("🔍 REST endpoints:")
    print("   • POST /api/documents/search - Semantic search")
    print("   • POST /api/rag/ask - RAG question answering")
    print("   • POST /api/documents/embed - Create with embedding")
    print("   • GET /health - Health check")
    print("\n⚙️  Environment variables:")
    print("   • DATABASE_URL - PostgreSQL connection")
    print("   • OPENAI_API_KEY - For OpenAI embeddings (optional)")
    print("   • USE_LOCAL_EMBEDDINGS - Set to 'true' for local models")
    print("   • LOCAL_MODEL_URL - vLLM server URL (default: http://localhost:8000/v1)")

    uvicorn.run(app, host="0.0.0.0", port=port)
