"""Local embedding providers for RAG system.

Supports:
1. vLLM server (localhost:8000)
2. sentence-transformers (local GPU)
3. OpenAI API (fallback)
"""

import asyncio
import os
from typing import List, Optional

import httpx


class LocalVLLMEmbeddings:
    """Embedding provider using local vLLM server."""

    def __init__(
        self,
        base_url: str = "http://localhost:8000/v1",
        model: str = "sentence-transformers/all-MiniLM-L6-v2",
        dimensions: int = 384,
    ):
        self.base_url = base_url
        self.model = model
        self.dimensions = dimensions
        self.client = httpx.AsyncClient(timeout=30.0)

    async def embed_query(self, text: str) -> List[float]:
        """Generate embedding for a single text."""
        # vLLM doesn't natively support embeddings API yet
        # So we use sentence-transformers directly
        raise NotImplementedError(
            "vLLM doesn't support embeddings API. Use SentenceTransformerEmbeddings instead."
        )

    async def embed_documents(self, texts: List[str]) -> List[List[float]]:
        """Generate embeddings for multiple texts."""
        return [await self.embed_query(text) for text in texts]

    async def aclose(self):
        """Close the HTTP client."""
        await self.client.aclose()


class SentenceTransformerEmbeddings:
    """Embedding provider using sentence-transformers (local GPU)."""

    def __init__(
        self,
        model_name: str = "sentence-transformers/all-MiniLM-L6-v2",
        device: str = "cuda",  # or 'cpu'
        normalize_embeddings: bool = True,
    ):
        try:
            from sentence_transformers import SentenceTransformer
        except ImportError:
            raise ImportError(
                "sentence-transformers not installed. "
                "Install with: pip install sentence-transformers"
            )

        self.model = SentenceTransformer(model_name, device=device)
        self.normalize_embeddings = normalize_embeddings
        self.dimensions = self.model.get_sentence_embedding_dimension()
        print(f"✓ Loaded local embedding model: {model_name}")
        print(f"  Device: {device}")
        print(f"  Dimensions: {self.dimensions}")

    def embed_query(self, text: str) -> List[float]:
        """Generate embedding for a single text (sync)."""
        embedding = self.model.encode(
            text,
            normalize_embeddings=self.normalize_embeddings,
            convert_to_numpy=True,
        )
        return embedding.tolist()

    async def aembed_query(self, text: str) -> List[float]:
        """Generate embedding for a single text (async)."""
        # Run in thread pool since sentence-transformers is sync
        return await asyncio.to_thread(self.embed_query, text)

    def embed_documents(self, texts: List[str]) -> List[List[float]]:
        """Generate embeddings for multiple texts (sync)."""
        embeddings = self.model.encode(
            texts,
            normalize_embeddings=self.normalize_embeddings,
            convert_to_numpy=True,
            batch_size=32,
            show_progress_bar=False,
        )
        return embeddings.tolist()

    async def aembed_documents(self, texts: List[str]) -> List[List[float]]:
        """Generate embeddings for multiple texts (async)."""
        return await asyncio.to_thread(self.embed_documents, texts)


class OpenAICompatibleEmbeddings:
    """Embedding provider using OpenAI-compatible API (works with vLLM, LocalAI, etc.)."""

    def __init__(
        self,
        base_url: str = "http://localhost:8000/v1",
        api_key: str = "not-needed-for-local",
        model: str = "sentence-transformers/all-MiniLM-L6-v2",
    ):
        try:
            from openai import AsyncOpenAI
        except ImportError:
            raise ImportError("openai not installed. Install with: pip install openai")

        self.client = AsyncOpenAI(base_url=base_url, api_key=api_key)
        self.model = model

    async def embed_query(self, text: str) -> List[float]:
        """Generate embedding for a single text."""
        response = await self.client.embeddings.create(input=text, model=self.model)
        return response.data[0].embedding

    async def embed_documents(self, texts: List[str]) -> List[List[float]]:
        """Generate embeddings for multiple texts."""
        response = await self.client.embeddings.create(input=texts, model=self.model)
        return [item.embedding for item in response.data]


def get_embedding_provider(
    provider: str = "auto",
    openai_api_key: Optional[str] = None,
    vllm_base_url: str = "http://localhost:8000/v1",
):
    """Get the best available embedding provider.

    Args:
        provider: One of 'auto', 'openai', 'local', 'sentence-transformers'
        openai_api_key: OpenAI API key (if using OpenAI)
        vllm_base_url: vLLM server URL (if using vLLM)

    Returns:
        Embedding provider instance
    """
    if provider == "openai":
        if not openai_api_key:
            raise ValueError("OpenAI API key required for 'openai' provider")
        from langchain_openai import OpenAIEmbeddings

        return OpenAIEmbeddings(openai_api_key=openai_api_key)

    if provider == "sentence-transformers" or provider == "local":
        # Use sentence-transformers with local GPU
        return SentenceTransformerEmbeddings(
            model_name=os.getenv(
                "LOCAL_EMBEDDING_MODEL", "sentence-transformers/all-MiniLM-L6-v2"
            ),
            device=os.getenv("EMBEDDING_DEVICE", "cuda"),
        )

    if provider == "auto":
        # Try providers in order of preference
        if openai_api_key:
            print("✓ Using OpenAI embeddings")
            from langchain_openai import OpenAIEmbeddings

            return OpenAIEmbeddings(openai_api_key=openai_api_key)

        # Try sentence-transformers (local GPU)
        try:
            print("ℹ OpenAI key not found, trying local embeddings...")
            return SentenceTransformerEmbeddings()
        except ImportError:
            raise RuntimeError(
                "No embedding provider available. Please either:\n"
                "  1. Set OPENAI_API_KEY environment variable, or\n"
                "  2. Install sentence-transformers: pip install sentence-transformers"
            )

    else:
        raise ValueError(f"Unknown provider: {provider}")


# Example usage
if __name__ == "__main__":
    import asyncio

    async def test_embeddings():
        # Test sentence-transformers
        print("\n=== Testing Sentence Transformers ===")
        embedder = SentenceTransformerEmbeddings()
        text = "This is a test document"
        embedding = await embedder.aembed_query(text)
        print(f"Text: {text}")
        print(f"Embedding dimensions: {len(embedding)}")
        print(f"First 5 values: {embedding[:5]}")

        # Test batch
        texts = ["Document 1", "Document 2", "Document 3"]
        embeddings = await embedder.aembed_documents(texts)
        print(f"\nBatch embeddings: {len(embeddings)} documents")
        for i, emb in enumerate(embeddings):
            print(f"  Doc {i+1}: {len(emb)} dimensions, first 3 values: {emb[:3]}")

    asyncio.run(test_embeddings())
