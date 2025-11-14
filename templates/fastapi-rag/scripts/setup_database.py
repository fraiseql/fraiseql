#!/usr/bin/env python3
"""Database setup script for RAG application."""

import os
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

import asyncpg
from dotenv import load_dotenv

# Load environment variables
load_dotenv()

DATABASE_URL = os.getenv("DATABASE_URL")
if not DATABASE_URL:
    print("❌ DATABASE_URL environment variable not set")
    sys.exit(1)


async def setup_database():
    """Set up database schema and initial data."""

    print("🔧 Setting up RAG database...")

    # Connect to database
    conn = await asyncpg.connect(DATABASE_URL)

    try:
        # Enable pgvector extension
        print("📦 Enabling pgvector extension...")
        await conn.execute("CREATE EXTENSION IF NOT EXISTS vector;")

        # Create tables
        print("🗄️ Creating tables...")

        # Documents table
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS documents (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            );
        """)

        # Document chunks table
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS document_chunks (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                content TEXT NOT NULL,
                embedding vector(1536), -- OpenAI text-embedding-ada-002 dimension
                chunk_index INTEGER NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                UNIQUE(document_id, chunk_index)
            );
        """)

        # Create indexes
        print("🔍 Creating indexes...")

        # Vector similarity search index (HNSW)
        await conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_document_chunks_embedding
            ON document_chunks USING hnsw (embedding vector_cosine_ops)
            WITH (m = 16, ef_construction = 64);
        """)

        # Regular indexes
        await conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_documents_created_at
            ON documents(created_at DESC);
        """)

        await conn.execute("""
            CREATE INDEX IF NOT EXISTS idx_document_chunks_document_id
            ON document_chunks(document_id);
        """)

        # Create views
        print("👁️ Creating views...")

        await conn.execute("""
            CREATE OR REPLACE VIEW v_documents AS
            SELECT
                id,
                title,
                content,
                created_at
            FROM documents
            ORDER BY created_at DESC;
        """)

        # Create functions
        print("⚙️ Creating functions...")

        # Function to create document
        await conn.execute("""
            CREATE OR REPLACE FUNCTION fn_create_document(
                p_title TEXT,
                p_content TEXT
            ) RETURNS UUID AS $$
            DECLARE
                v_document_id UUID;
            BEGIN
                INSERT INTO documents (title, content)
                VALUES (p_title, p_content)
                RETURNING id INTO v_document_id;

                RETURN v_document_id;
            END;
            $$ LANGUAGE plpgsql;
        """)

        # Function to create document chunk
        await conn.execute("""
            CREATE OR REPLACE FUNCTION fn_create_document_chunk(
                p_document_id UUID,
                p_content TEXT,
                p_chunk_index INTEGER
            ) RETURNS UUID AS $$
            DECLARE
                v_chunk_id UUID;
            BEGIN
                INSERT INTO document_chunks (document_id, content, chunk_index)
                VALUES (p_document_id, p_content, p_chunk_index)
                RETURNING id INTO v_chunk_id;

                RETURN v_chunk_id;
            END;
            $$ LANGUAGE plpgsql;
        """)

        # Update embedding function
        await conn.execute("""
            CREATE OR REPLACE FUNCTION fn_update_chunk_embedding(
                p_chunk_id UUID,
                p_embedding vector
            ) RETURNS VOID AS $$
            BEGIN
                UPDATE document_chunks
                SET embedding = p_embedding
                WHERE id = p_chunk_id;
            END;
            $$ LANGUAGE plpgsql;
        """)

        print("✅ Database setup complete!")

        # Show setup summary
        result = await conn.fetchrow("""
            SELECT
                (SELECT COUNT(*) FROM documents) as doc_count,
                (SELECT COUNT(*) FROM document_chunks) as chunk_count
        """)

        print(f"📊 Current state: {result['doc_count']} documents, {result['chunk_count']} chunks")

    finally:
        await conn.close()


async def seed_sample_data():
    """Add some sample documents for testing."""

    print("🌱 Seeding sample data...")

    conn = await asyncpg.connect(DATABASE_URL)

    try:
        # Sample documents
        sample_docs = [
            {
                "title": "Introduction to RAG Systems",
                "content": """
                Retrieval-Augmented Generation (RAG) is a technique that combines the power of large language models
                with external knowledge sources. By retrieving relevant information from a knowledge base before
                generating responses, RAG systems can provide more accurate and up-to-date answers.

                Key components of a RAG system:
                1. Document ingestion and processing
                2. Vector embeddings for semantic search
                3. Efficient retrieval mechanisms
                4. Answer generation using retrieved context

                This approach is particularly effective for question-answering tasks where currency and accuracy
                of information are important.
                """,
            },
            {
                "title": "Vector Databases and Embeddings",
                "content": """
                Vector databases are specialized databases designed to store and query high-dimensional vectors,
                which are mathematical representations of data in vector space. These vectors, often called embeddings,
                capture semantic meaning and relationships between pieces of content.

                Popular vector databases include:
                - Pinecone: Cloud-native vector database
                - Weaviate: Open-source vector search engine
                - Qdrant: Vector similarity search engine
                - pgvector: PostgreSQL extension for vector operations

                Embeddings are typically generated using transformer models like BERT, RoBERTa, or specialized
                embedding models like text-embedding-ada-002 from OpenAI.
                """,
            },
            {
                "title": "Building Production RAG Applications",
                "content": """
                When building production RAG applications, several considerations are crucial:

                Data Pipeline:
                - Document preprocessing and cleaning
                - Chunking strategies (fixed size, semantic, hierarchical)
                - Embedding generation and storage
                - Index optimization and maintenance

                Retrieval Strategies:
                - Similarity search (cosine, dot product, Euclidean)
                - Hybrid search combining vector and keyword search
                - Re-ranking and filtering results
                - Handling long contexts

                Generation:
                - Prompt engineering for context integration
                - Handling token limits and truncation
                - Answer quality evaluation
                - Fallback strategies for low-confidence answers

                Monitoring and Maintenance:
                - Performance metrics (latency, accuracy)
                - Data freshness and update strategies
                - User feedback integration
                - Continuous improvement pipelines
                """,
            },
        ]

        for doc in sample_docs:
            # Create document
            _ = await conn.fetchval(
                "SELECT fn_create_document($1, $2)", doc["title"], doc["content"]
            )

            print(f"✅ Added document: {doc['title']}")

        print("🎉 Sample data seeded successfully!")

    finally:
        await conn.close()


async def main():
    """Main setup function."""

    import argparse

    parser = argparse.ArgumentParser(description="Set up RAG database")
    parser.add_argument("--seed", action="store_true", help="Also seed sample data")
    args = parser.parse_args()

    try:
        await setup_database()

        if args.seed:
            await seed_sample_data()

        print("\n🚀 Database is ready! You can now run the application.")

    except Exception as e:
        print(f"❌ Setup failed: {e}")
        sys.exit(1)


if __name__ == "__main__":
    import asyncio

    asyncio.run(main())
