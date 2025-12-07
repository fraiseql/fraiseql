"""RAG System Example - Retrieval-Augmented Generation with FraiseQL + LangChain"""

import asyncio
import os
from typing import List, Optional
from uuid import UUID

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
import uvicorn

# FraiseQL imports
from fraiseql import fraise_query, fraise_type, fraise_mutation
from fraiseql.fastapi import FraiseQLApp

# LangChain imports
try:
    from langchain.embeddings.openai import OpenAIEmbeddings
    from langchain.vectorstores.pgvector import PGVector
    from langchain.chains import RetrievalQA
    from langchain.chat_models import ChatOpenAI
    from langchain.docstore.document import Document
except ImportError:
    print("⚠️  LangChain not installed. Install with: pip install langchain openai")
    OpenAIEmbeddings = None
    PGVector = None
    RetrievalQA = None
    ChatOpenAI = None
    Document = None


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


# FraiseQL queries
@fraise_query
async def get_documents(info, limit: int = 50, source: Optional[str] = None) -> List[TBDocument]:
    """Get documents with optional source filtering."""
    repo = info.context["db"]

    where = {}
    if source:
        where["source"] = {"eq": source}

    return await repo.find("tb_document", where=where, orderBy={"created_at": "DESC"}, limit=limit)


@fraise_query
async def search_documents(
    info, query_embedding: List[float], limit: int = 10, similarity_threshold: float = 0.7
) -> List[dict]:
    """Search documents by embedding similarity."""
    repo = info.context["db"]

    # Use the SQL function for efficient similarity search
    result = await repo.execute_raw(
        """
        SELECT 
            d.id,
            d.title,
            d.content,
            d.source,
            d.metadata,
            (1 - (e.embedding <=> $1::vector))::REAL as similarity
        FROM tb_document d
        JOIN tv_document_embedding e ON d.id = e.document_id
        WHERE (1 - (e.embedding <=> $1::vector)) >= $2
        ORDER BY (e.embedding <=> $1::vector)
        LIMIT $3
    """,
        query_embedding,
        similarity_threshold,
        limit,
    )

    return result


# FraiseQL mutations
@fraise_mutation
async def create_document(
    info, title: str, content: str, source: Optional[str] = None, metadata: Optional[dict] = {}
) -> TBDocument:
    """Create a new document."""
    repo = info.context["db"]

    return await repo.create(
        "tb_document",
        data={"title": title, "content": content, "source": source, "metadata": metadata},
    )


@fraise_mutation
async def update_document_embedding(
    info, document_id: UUID, embedding: List[float], embedding_model: str = "text-embedding-ada-002"
) -> bool:
    """Update document embedding."""
    repo = info.context["db"]

    # Delete existing embedding
    await repo.execute_raw("DELETE FROM tv_document_embedding WHERE document_id = $1", document_id)

    # Insert new embedding
    await repo.execute_raw(
        """INSERT INTO tv_document_embedding (document_id, embedding, embedding_model) 
           VALUES ($1, $2::vector, $2)""",
        document_id,
        embedding,
        embedding_model,
    )

    return True


# RAG Service class
class RAGService:
    """Service for RAG operations using LangChain."""

    def __init__(self, database_url: str, openai_api_key: str):
        self.database_url = database_url
        self.openai_api_key = openai_api_key

        if OpenAIEmbeddings and PGVector and ChatOpenAI:
            self.embeddings = OpenAIEmbeddings(openai_api_key=openai_api_key)
            self.vectorstore = PGVector(
                connection_string=database_url,
                embedding_function=self.embeddings,
                collection_name="tv_document_embedding",
                embedding_field="embedding",
                text_field="content",
            )
            self.qa_chain = RetrievalQA.from_chain_type(
                llm=ChatOpenAI(openai_api_key=openai_api_key),
                chain_type="stuff",
                retriever=self.vectorstore.as_retriever(),
            )
        else:
            self.embeddings = None
            self.vectorstore = None
            self.qa_chain = None

    async def add_document_with_embedding(self, title: str, content: str, **kwargs) -> UUID:
        """Add document and generate embedding."""
        if not self.embeddings:
            raise HTTPException(status_code=500, detail="LangChain not available")

        # Generate embedding
        embedding = await self.embeddings.aembed_query(content)

        # Create document with embedding
        repo = FraiseQLRepository(self.database_url)
        doc_id = await repo.execute_raw(
            """SELECT create_document_with_embedding($1, $2, $3, $4, $5::vector)""",
            title,
            content,
            kwargs.get("source"),
            kwargs.get("metadata", {}),
            embedding,
        )

        return doc_id[0]["create_document_with_embedding"]

    async def semantic_search(self, query: str, limit: int = 5) -> List[dict]:
        """Perform semantic search."""
        if not self.embeddings:
            raise HTTPException(status_code=500, detail="LangChain not available")

        # Generate query embedding
        query_embedding = await self.embeddings.aembed_query(query)

        # Search using FraiseQL
        repo = FraiseQLRepository(self.database_url)
        results = await search_documents(
            type("Info", (), {"context": {"db": repo}})(),
            query_embedding=query_embedding,
            limit=limit,
        )

        return results

    async def answer_question(self, question: str, context_limit: int = 3) -> dict:
        """Answer question using RAG."""
        if not self.qa_chain:
            raise HTTPException(status_code=500, detail="LangChain not available")

        # Get relevant documents
        search_results = await self.semantic_search(question, limit=context_limit)

        # Format context
        context = "\n\n".join(
            [f"Document: {doc['title']}\n{doc['content']}" for doc in search_results]
        )

        # Generate answer
        response = await self.qa_chain.arun({"query": question, "context": context})

        return {
            "question": question,
            "answer": response,
            "sources": [
                {"id": doc["id"], "title": doc["title"], "similarity": doc["similarity"]}
                for doc in search_results
            ],
        }


# Create FastAPI app with FraiseQL
app = FraiseQLApp(
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost:5432/ragdb"),
    types=[TBDocument, TVDocumentEmbedding],
    queries=[get_documents, search_documents],
    mutations=[create_document, update_document_embedding],
)

# RAG service instance
rag_service = None


@app.on_event("startup")
async def startup_event():
    """Initialize RAG service."""
    global rag_service
    database_url = os.getenv("DATABASE_URL", "postgresql://localhost:5432/ragdb")
    openai_api_key = os.getenv("OPENAI_API_KEY")

    if openai_api_key:
        rag_service = RAGService(database_url, openai_api_key)
    else:
        print("⚠️  OPENAI_API_KEY not set. RAG features will be limited.")


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


if __name__ == "__main__":
    print("🚀 RAG System Example")
    print("📚 Features:")
    print("   • Document storage with trinity pattern")
    print("   • Vector embeddings with pgvector")
    print("   • Semantic search via GraphQL")
    print("   • RAG question answering")
    print("   • LangChain integration")
    print("\n📝 GraphQL endpoint: http://localhost:8000/graphql")
    print("🔍 REST endpoints:")
    print("   • POST /api/documents/search - Semantic search")
    print("   • POST /api/rag/ask - RAG question answering")
    print("   • POST /api/documents/embed - Create with embedding")
    print("\n⚙️  Set environment variables:")
    print("   • DATABASE_URL - PostgreSQL connection")
    print("   • OPENAI_API_KEY - For embeddings and LLM")

    uvicorn.run(app, host="0.0.0.0", port=8000)
