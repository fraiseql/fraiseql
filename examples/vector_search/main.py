"""Vector Search Example - Demonstrating all 6 pgvector distance operators
"""
import asyncio
from typing import List
from uuid import UUID

from fraiseql import fraise_query, fraise_type
from fraiseql.fastapi import FraiseQLApp


@fraise_type
class Document:
    """Document with vector embeddings and binary hash for similarity search."""
    id: UUID
    title: str
    content: str
    embedding: List[float]  # Vector embedding (384 dimensions)
    binary_hash: str       # Binary hash for Hamming/Jaccard (64 bits)
    category: str
    created_at: str


@fraise_query
async def search_documents(
    info,
    # Float vector operators
    cosine_distance: List[float] = None,
    l2_distance: List[float] = None,
    inner_product: List[float] = None,
    l1_distance: List[float] = None,
    # Binary vector operators
    hamming_distance: str = None,
    jaccard_distance: str = None,
    # Filters
    category: str = None,
    limit: int = 10
) -> List[Document]:
    """Search documents using vector similarity with all 6 distance operators.

    Examples:
    - Semantic search: cosine_distance=[0.1, 0.2, 0.3, ...]
    - Binary search: hamming_distance="10101010..."
    - Hybrid search: Combine vector + metadata filters
    """
    repo = info.context["db"]

    # Build where clause
    where = {}
    if cosine_distance:
        where["embedding"] = {"cosine_distance": cosine_distance}
    if l2_distance:
        where["embedding"] = {"l2_distance": l2_distance}
    if inner_product:
        where["embedding"] = {"inner_product": inner_product}
    if l1_distance:
        where["embedding"] = {"l1_distance": l1_distance}
    if hamming_distance:
        where["binary_hash"] = {"hamming_distance": hamming_distance}
    if jaccard_distance:
        where["binary_hash"] = {"jaccard_distance": jaccard_distance}
    if category:
        where["category"] = {"eq": category}

    # Build order by (most similar first)
    order_by = {}
    if cosine_distance:
        order_by["embedding"] = {"cosine_distance": cosine_distance}
    elif l2_distance:
        order_by["embedding"] = {"l2_distance": l2_distance}
    elif inner_product:
        order_by["embedding"] = {"inner_product": inner_product}
    elif l1_distance:
        order_by["embedding"] = {"l1_distance": l1_distance}
    elif hamming_distance:
        order_by["binary_hash"] = {"hamming_distance": hamming_distance}
    elif jaccard_distance:
        order_by["binary_hash"] = {"jaccard_distance": jaccard_distance}

    return await repo.find(
        "documents",
        where=where,
        orderBy=order_by,
        limit=limit
    )


# Create FastAPI app with GraphQL
app = FraiseQLApp(
    database_url="postgresql://localhost:5432/vectordb",
    types=[Document],
    queries=[search_documents]
)


if __name__ == "__main__":
    import uvicorn
    print("🚀 Vector Search Example")
    print("📊 Supports all 6 pgvector distance operators:")
    print("   • cosine_distance (<=>) - Semantic similarity")
    print("   • l2_distance (<->) - Euclidean distance")
    print("   • inner_product (<#>) - Learned similarity")
    print("   • l1_distance (<+>) - Manhattan distance")
    print("   • hamming_distance (<~>) - Binary Hamming")
    print("   • jaccard_distance (<%>) - Set similarity")
    print("\n📝 GraphQL endpoint: http://localhost:8000/graphql")
    print("🔍 Try queries in the GraphQL playground!")

    uvicorn.run(app, host="0.0.0.0", port=8000)
