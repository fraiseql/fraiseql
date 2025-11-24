# LangChain Integration Guide

This guide shows you how to integrate LangChain with FraiseQL to build Retrieval-Augmented Generation (RAG) applications. You'll learn how to create a GraphQL API that can search documents and generate answers using LangChain's powerful AI capabilities.

## Overview

FraiseQL + LangChain provides a powerful combination for building AI-powered GraphQL APIs:

- **FraiseQL**: Handles GraphQL schema, database operations, and API serving
- **LangChain**: Provides AI models, embeddings, and vector search capabilities
- **PostgreSQL with pgvector**: Stores documents and their vector embeddings

## Quick Start with Template

The fastest way to get started is using the `fastapi-rag` template:

```bash
# Create a new RAG project
fraiseql init my-rag-app --template fastapi-rag

# Navigate to the project
cd my-rag-app

# Set up environment
python -m venv .venv
source .venv/bin/activate  # On Windows: .venv\Scripts\activate
pip install -e .

# Set up the database
python scripts/setup_database.py

# Configure environment variables
# Edit .env file with your OpenAI API key and database URL

# Run the application
python src/main.py
```

The template includes:
- Complete GraphQL schema with RAG queries
- Document upload mutations
- LangChain integration with OpenAI embeddings
- pgvector setup for vector storage
- Docker configuration for easy deployment

## Manual Setup

If you prefer to set up manually or integrate into an existing project:

### 1. Install Dependencies

```bash
pip install langchain langchain-openai langchain-community pgvector
```

### 2. Database Setup

Create a table for storing documents with vector embeddings:

```sql
-- Enable pgvector extension
CREATE EXTENSION IF NOT EXISTS vector;

-- Create documents table
CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    embedding vector(1536), -- OpenAI ada-002 dimension
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create vector index for fast similarity search
CREATE INDEX ON documents USING ivfflat (embedding vector_cosine_ops)
WITH (lists = 100);
```

### 3. GraphQL Schema

Define your GraphQL types and queries:

```python
import fraiseql
import fraiseql
from fraiseql import fraise_field
from fraiseql.types.scalars import UUID
from typing import List, Optional

@fraiseql.type
class Document:
    """A document in the RAG system."""
    id: UUID = fraise_field(description="Document ID")
    content: str = fraise_field(description="Document content")
    metadata: dict = fraise_field(description="Document metadata")
    created_at: str = fraise_field(description="Creation timestamp")

@fraiseql.type
class SearchResult:
    """Search result with similarity score."""
    document: Document = fraise_field(description="Matching document")
    score: float = fraise_field(description="Similarity score")

@fraiseql.type
class QueryRoot:
    """Root query type."""
    search_documents: List[SearchResult] = fraise_field(
        description="Search documents by semantic similarity"
    )
    ask_question: str = fraise_field(
        description="Ask a question and get an AI-generated answer"
    )

    async def resolve_search_documents(self, info, query: str, limit: int = 5):
        # Implementation below
        pass

    async def resolve_ask_question(self, info, question: str):
        # Implementation below
        pass

@fraiseql.type
class MutationRoot:
    """Root mutation type."""
    upload_document: Document = fraise_field(
        description="Upload a new document to the knowledge base"
    )

    async def resolve_upload_document(self, info, content: str, metadata: Optional[dict] = None):
        # Implementation below
        pass
```

### 4. LangChain Integration

Set up LangChain components:

```python
import os
from langchain_openai import OpenAIEmbeddings, ChatOpenAI
from langchain_community.vectorstores import PGVector
from langchain_core.documents import Document as LangChainDocument
from langchain.text_splitter import RecursiveCharacterTextSplitter

# Initialize embeddings and LLM
embeddings = OpenAIEmbeddings(
    model="text-embedding-ada-002",
    openai_api_key=os.getenv("OPENAI_API_KEY")
)

llm = ChatOpenAI(
    model="gpt-3.5-turbo",
    openai_api_key=os.getenv("OPENAI_API_KEY")
)

# Initialize vector store
vector_store = PGVector(
    connection_string=os.getenv("DATABASE_URL"),
    embedding_function=embeddings,
    collection_name="documents"
)

# Text splitter for document chunking
text_splitter = RecursiveCharacterTextSplitter(
    chunk_size=1000,
    chunk_overlap=200
)
```

### 5. Implement Resolvers

Complete the GraphQL resolvers:

```python
async def resolve_search_documents(self, info, query: str, limit: int = 5):
    """Search documents by semantic similarity."""
    # Search for similar documents
    docs = vector_store.similarity_search_with_score(query, k=limit)

    results = []
    for doc, score in docs:
        # Get document from database
        doc_id = doc.metadata.get("id")
        # Query your documents table to get full document info
        # (Implementation depends on your database setup)

        results.append(SearchResult(
            document=Document(id=doc_id, content=doc.page_content, ...),
            score=score
        ))

    return results

async def resolve_ask_question(self, info, question: str):
    """Generate an answer using RAG."""
    # Retrieve relevant documents
    docs = vector_store.similarity_search(question, k=3)

    # Create context from retrieved documents
    context = "\n\n".join([doc.page_content for doc in docs])

    # Generate answer using LLM
    prompt = f"""Use the following context to answer the question.
If you cannot find the answer in the context, say so.

Context:
{context}

Question: {question}

Answer:"""

    response = llm.invoke(prompt)
    return response.content

async def resolve_upload_document(self, info, content: str, metadata: Optional[dict] = None):
    """Upload and index a new document."""
    # Split document into chunks
    chunks = text_splitter.split_text(content)

    # Create LangChain documents
    langchain_docs = [
        LangChainDocument(
            page_content=chunk,
            metadata={"id": str(uuid.uuid4()), **(metadata or {})}
        )
        for chunk in chunks
    ]

    # Add to vector store
    vector_store.add_documents(langchain_docs)

    # Save to database
    # (Implementation depends on your database setup)

    return Document(id=doc_id, content=content, metadata=metadata, ...)
```

## Advanced Features

### Custom Embedding Models

Use different embedding models:

```python
from langchain_huggingface import HuggingFaceEmbeddings

# Use local HuggingFace model
embeddings = HuggingFaceEmbeddings(
    model_name="sentence-transformers/all-MiniLM-L6-v2"
)

# Update vector dimension in database schema
# embedding vector(384) for MiniLM
```

### Conversation History

Add conversation memory:

```python
from langchain.memory import ConversationBufferMemory
from langchain.chains import ConversationalRetrievalChain

# Create conversational chain
qa_chain = ConversationalRetrievalChain.from_llm(
    llm=llm,
    retriever=vector_store.as_retriever(),
    memory=ConversationBufferMemory(
        memory_key="chat_history",
        return_messages=True
    )
)

# Use in resolver
async def resolve_ask_question(self, info, question: str, conversation_id: str):
    response = qa_chain({"question": question})
    return response["answer"]
```

### Document Filtering

Filter documents by metadata:

```python
# Search with metadata filter
docs = vector_store.similarity_search(
    query,
    k=5,
    filter={"category": "technical"}
)
```

### Streaming Responses

For long responses, implement streaming:

```python
from fastapi.responses import StreamingResponse

async def resolve_ask_question_stream(self, info, question: str):
    async def generate():
        # Retrieve context
        docs = vector_store.similarity_search(question, k=3)
        context = "\n\n".join([doc.page_content for doc in docs])

        # Stream the response
        async for chunk in llm.astream(prompt):
            yield chunk.content

    return StreamingResponse(generate(), media_type="text/plain")
```

## Deployment

### Docker Configuration

Use the provided Docker setup for production:

```yaml
# docker-compose.yml
version: '3.8'
services:
  db:
    image: pgvector/pgvector:pg16
    environment:
      POSTGRES_DB: ragdb
      POSTGRES_USER: user
      POSTGRES_PASSWORD: password
    volumes:
      - pgdata:/var/lib/postgresql/data
    ports:
      - "5432:5432"

  app:
    build: .
    environment:
      - DATABASE_URL=postgresql://user:password@db:5432/ragdb
      - OPENAI_API_KEY=${OPENAI_API_KEY}
    ports:
      - "8000:8000"
    depends_on:
      - db
```

### Environment Variables

Configure your `.env` file:

```bash
# Database
DATABASE_URL=postgresql://user:password@localhost:5432/ragdb

# OpenAI
OPENAI_API_KEY=your-api-key-here

# Application
FRAISEQL_DATABASE_URL=${DATABASE_URL}
FRAISEQL_AUTO_CAMEL_CASE=true
```

## Best Practices

1. **Chunk Size**: Experiment with different chunk sizes (500-2000 characters) based on your content
2. **Overlap**: Use 10-20% overlap between chunks for better context
3. **Indexing**: Rebuild vector indexes periodically for better performance
4. **Caching**: Cache frequently accessed embeddings
5. **Validation**: Validate document content before indexing
6. **Monitoring**: Monitor vector search performance and adjust parameters

## Troubleshooting

### Common Issues

**"pgvector extension not found"**
```sql
-- Enable the extension
CREATE EXTENSION vector;
```

**"Dimension mismatch"**
- Ensure your vector column dimension matches your embedding model
- OpenAI ada-002: 1536 dimensions
- MiniLM: 384 dimensions

**"Connection timeout"**
- Check your DATABASE_URL
- Ensure PostgreSQL is running and accessible

**"OpenAI API rate limit"**
- Implement retry logic with exponential backoff
- Consider using a different model or provider

## Next Steps

- Explore [LangChain documentation](https://python.langchain.com/) for advanced features
- Check out [FraiseQL examples](../examples/) for more patterns
- Consider adding authentication and authorization to your API
- Implement document versioning and updates

This integration provides a solid foundation for building AI-powered applications with GraphQL. The combination of FraiseQL's type safety and LangChain's AI capabilities enables rapid development of sophisticated RAG systems.
