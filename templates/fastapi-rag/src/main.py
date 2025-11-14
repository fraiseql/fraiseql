"""FastAPI RAG Application with FraiseQL and LangChain."""

import os
from datetime import datetime
from typing import List

import fraiseql
from fraiseql import fraise_field
from fraiseql.types.scalars import UUID
from langchain_openai import OpenAIEmbeddings
from langchain_community.vectorstores import PGVector
from langchain_core.documents import Document
from langchain.text_splitter import RecursiveCharacterTextSplitter


# GraphQL Types
@fraiseql.type
class DocumentType:
    """A document in the RAG system."""

    id: UUID = fraise_field(description="Document ID")
    title: str = fraise_field(description="Document title")
    content: str = fraise_field(description="Document content")
    created_at: datetime = fraise_field(description="When document was created")


@fraiseql.type
class DocumentChunk:
    """A chunk of a document with embedding."""

    id: UUID = fraise_field(description="Chunk ID")
    document_id: UUID = fraise_field(description="Parent document ID")
    content: str = fraise_field(description="Chunk content")
    embedding: List[float] = fraise_field(description="Vector embedding")
    chunk_index: int = fraise_field(description="Chunk index in document")


@fraiseql.type
class SearchResult:
    """Result from document search."""

    document_id: UUID = fraise_field(description="Document ID")
    content: str = fraise_field(description="Relevant content")
    similarity: float = fraise_field(description="Similarity score")


@fraiseql.type
class RAGResponse:
    """Response from RAG question answering."""

    answer: str = fraise_field(description="Generated answer")
    sources: List[SearchResult] = fraise_field(description="Source documents")


# Input Types
@fraiseql.input
class UploadDocumentInput:
    """Input for uploading a document."""

    title: str = fraise_field(description="Document title")
    content: str = fraise_field(description="Document content")


@fraiseql.input
class AskQuestionInput:
    """Input for asking a question."""

    question: str = fraise_field(description="Question to ask")


# Query Resolvers
@fraiseql.type
class QueryRoot:
    """Root query type."""

    documents: List[DocumentType] = fraise_field(description="List all documents")
    document: DocumentType | None = fraise_field(description="Get single document by ID")
    search_documents: List[SearchResult] = fraise_field(description="Search documents by query")
    ask_question: RAGResponse = fraise_field(description="Ask a question using RAG")

    async def resolve_documents(self, info):
        """Get all documents."""
        repo = info.context["repo"]
        results = await repo.find("v_documents", order_by=[("created_at", "desc")])
        return [DocumentType(**result) for result in results]

    async def resolve_document(self, info, id: UUID):
        """Get single document by ID."""
        repo = info.context["repo"]
        result = await repo.find_one("v_documents", where={"id": id})
        return DocumentType(**result) if result else None

    async def resolve_search_documents(self, info, query: str, limit: int = 5):
        """Search documents using vector similarity."""
        # Get vector store from context
        vector_store = info.context["vector_store"]

        # Perform similarity search
        docs = vector_store.similarity_search_with_score(query, k=limit)

        results = []
        for doc, score in docs:
            # Extract document ID from metadata
            doc_id = doc.metadata.get("document_id")
            if doc_id:
                results.append(
                    SearchResult(
                        document_id=doc_id, content=doc.page_content, similarity=float(score)
                    )
                )

        return results

    async def resolve_ask_question(self, info, question: str):
        """Answer question using RAG pipeline."""
        from langchain.chains import RetrievalQA
        from langchain_openai import ChatOpenAI

        # Get components from context
        vector_store = info.context["vector_store"]
        llm = ChatOpenAI(temperature=0, model="gpt-3.5-turbo")

        # Create RAG chain
        qa_chain = RetrievalQA.from_chain_type(
            llm=llm,
            chain_type="stuff",
            retriever=vector_store.as_retriever(search_kwargs={"k": 3}),
            return_source_documents=True,
        )

        # Get answer
        result = qa_chain({"query": question})

        # Format sources
        sources = []
        for doc in result["source_documents"]:
            doc_id = doc.metadata.get("document_id")
            if doc_id:
                sources.append(
                    SearchResult(
                        document_id=doc_id,
                        content=doc.page_content,
                        similarity=0.0,  # Could calculate if needed
                    )
                )

        return RAGResponse(answer=result["result"], sources=sources)


# Mutation Resolvers
@fraiseql.type
class MutationRoot:
    """Root mutation type."""

    upload_document: DocumentType = fraise_field(description="Upload a new document")

    async def resolve_upload_document(self, info, input: UploadDocumentInput):
        """Upload and process a document."""
        repo = info.context["repo"]
        vector_store = info.context["vector_store"]

        # Create document record
        doc_id = await repo.call_function(
            "fn_create_document", p_title=input.title, p_content=input.content
        )

        # Get the created document
        doc_result = await repo.find_one("v_documents", where={"id": doc_id})
        document = DocumentType(**doc_result)

        # Split document into chunks
        text_splitter = RecursiveCharacterTextSplitter(
            chunk_size=1000, chunk_overlap=200, length_function=len
        )
        chunks = text_splitter.split_text(input.content)

        # Create embeddings and store chunks

        # Prepare documents for vector store
        langchain_docs = []
        for i, chunk in enumerate(chunks):
            # Store chunk in database
            await repo.call_function(
                "fn_create_document_chunk", p_document_id=doc_id, p_content=chunk, p_chunk_index=i
            )

            # Prepare for vector store
            langchain_docs.append(
                Document(
                    page_content=chunk, metadata={"document_id": str(doc_id), "chunk_index": i}
                )
            )

        # Add to vector store
        if langchain_docs:
            vector_store.add_documents(langchain_docs)

        return document


# Application setup
def create_app():
    """Create FastAPI application with RAG components."""

    # Initialize embeddings and vector store
    embeddings = OpenAIEmbeddings()
    connection_string = os.getenv("DATABASE_URL")

    # Initialize PGVector store
    vector_store = PGVector(
        connection_string=connection_string,
        collection_name="document_chunks",
        embedding_function=embeddings,
    )

    # Create FraiseQL app
    app = fraiseql.create_fraiseql_app(
        queries=[QueryRoot],
        mutations=[MutationRoot],
        database_url=connection_string,
        context_value={
            "vector_store": vector_store,
        },
    )

    return app


# Create the app
app = create_app()

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000, reload=True)
