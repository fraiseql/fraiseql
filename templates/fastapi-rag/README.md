# FastAPI RAG Application with FraiseQL

A complete Retrieval-Augmented Generation (RAG) application built with FastAPI, FraiseQL, LangChain, and OpenAI. This template demonstrates how to build a production-ready RAG system for document Q&A.

## Features

- **Document Ingestion**: Upload and process documents with automatic chunking
- **Vector Search**: Semantic search using pgvector and OpenAI embeddings
- **RAG Pipeline**: LangChain-powered question answering
- **GraphQL API**: FraiseQL-powered GraphQL interface
- **Admin Interface**: Simple web UI for document management
- **Docker Support**: Complete containerized deployment

## Quick Start

### Prerequisites
- Python 3.10+
- PostgreSQL with pgvector extension
- OpenAI API key

### 1. Clone and Setup

```bash
# Using fraiseql CLI (when available)
fraiseql init my-rag-app --template=fastapi-rag
cd my-rag-app

# Or manually copy this template
cp -r templates/fastapi-rag/* my-rag-app/
cd my-rag-app
```

### 2. Environment Setup

```bash
# Copy environment template
cp .env.example .env

# Edit .env with your settings
# Required: DATABASE_URL, OPENAI_API_KEY
```

### 3. Database Setup

```bash
# Install dependencies
pip install -e ".[dev]"

# Run database migrations
python scripts/setup_database.py
```

### 4. Start the Application

```bash
# Development mode
python -m src.main

# Or with Docker
docker-compose up --build
```

Visit http://localhost:8000/graphql for the GraphQL playground.

## API Usage

### Upload Documents

```graphql
mutation UploadDocument($input: UploadDocumentInput!) {
  uploadDocument(input: $input) {
    id
    title
    content
    chunks {
      id
      content
      embedding
    }
  }
}
```

### Ask Questions

```graphql
query AskQuestion($question: String!) {
  askQuestion(question: $question) {
    answer
    sources {
      documentId
      content
      similarity
    }
  }
}
```

### Search Documents

```graphql
query SearchDocuments($query: String!, $limit: Int) {
  searchDocuments(query: $query, limit: $limit) {
    id
    title
    content
    similarity
  }
}
```

## Project Structure

```
src/
├── main.py              # FastAPI application and GraphQL schema
├── types/               # GraphQL type definitions
├── queries/             # Query resolvers
├── mutations/           # Mutation resolvers
└── utils/               # Helper functions and RAG logic

scripts/
├── setup_database.py    # Database initialization
└── seed_data.py         # Sample data loading

tests/                   # Test suite
docker-compose.yml       # Docker deployment
```

## Configuration

### Environment Variables

```bash
# Database
DATABASE_URL=postgresql://user:pass@localhost:5432/rag_db

# OpenAI
OPENAI_API_KEY=your-api-key-here

# Application
APP_ENV=development
DEBUG=true
```

### Database Schema

The application uses these main tables:
- `documents` - Document metadata
- `document_chunks` - Text chunks with embeddings
- `conversations` - Chat history (optional)

## Development

### Running Tests

```bash
pytest tests/
```

### Code Quality

```bash
# Linting
ruff check

# Type checking
mypy src/
```

## Deployment

### Docker Production

```bash
docker-compose -f docker-compose.prod.yml up --build
```

### Manual Deployment

```bash
# Install production dependencies
pip install -e .

# Set production environment
export APP_ENV=production

# Run with gunicorn
gunicorn src.main:app -w 4 -k uvicorn.workers.UvicornWorker
```

## Learn More

- [FraiseQL Documentation](https://fraiseql.readthedocs.io)
- [LangChain RAG Guide](https://python.langchain.com/docs/guides/rag)
- [pgvector Documentation](https://github.com/pgvector/pgvector)

## Troubleshooting

### Common Issues

1. **pgvector extension not installed**
   ```sql
   CREATE EXTENSION IF NOT EXISTS vector;
   ```

2. **OpenAI API rate limits**
   - Reduce chunk size in configuration
   - Implement caching for embeddings

3. **Memory issues with large documents**
   - Adjust chunk size and overlap settings
   - Use streaming for large file uploads
