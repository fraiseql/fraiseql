"""FraiseQL Vector Store Integration Performance Benchmarks

Comprehensive performance analysis of LangChain and LlamaIndex integrations
including memory usage, throughput, and concurrent load testing.

This benchmark suite validates production readiness and provides comparative
analysis between the two integration approaches.
"""

import asyncio
import json
import time
import tracemalloc
import uuid
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Tuple

# Optional imports for integrations
try:
    from langchain_core.documents import Document as LangChainDocument

    LANGCHAIN_AVAILABLE = True
except ImportError:
    LANGCHAIN_AVAILABLE = False
    LangChainDocument = None

try:
    from llama_index.core.schema import Document as LlamaDocument

    LLAMAINDEX_AVAILABLE = True
except ImportError:
    LLAMAINDEX_AVAILABLE = False
    LlamaDocument = None

from fraiseql.integrations.langchain import FraiseQLVectorStore as LangChainVectorStore
from fraiseql.integrations.llamaindex import (
    FraiseQLVectorStore as LlamaIndexVectorStore,
)


@dataclass
class BenchmarkResult:
    """Container for benchmark results."""

    operation: str
    framework: str
    duration: float
    memory_usage: int
    throughput: float
    error_rate: float
    metadata: Dict[str, Any]


@dataclass
class BenchmarkConfig:
    """Configuration for benchmark runs."""

    document_count: int = 1000
    embedding_dimension: int = 384
    batch_sizes: Optional[List[int]] = None
    concurrent_users: int = 10
    query_count: int = 100
    warmup_iterations: int = 5

    def __post_init__(self):
        if self.batch_sizes is None:
            self.batch_sizes = [1, 10, 50, 100]


@dataclass
class MockDoc:
    """Mock document for testing."""

    page_content: str
    text: str
    metadata: Dict[str, Any]


class MemoryProfiler:
    """Memory usage profiler for benchmark operations."""

    def __init__(self):
        self.snapshots = []

    def start(self) -> None:
        """Start memory profiling."""
        tracemalloc.start()
        self.snapshots = []

    def snapshot(self, label: str) -> None:
        """Take a memory snapshot."""
        current, peak = tracemalloc.get_traced_memory()
        self.snapshots.append(
            {"label": label, "current": current, "peak": peak, "timestamp": time.time()}
        )

    def stop(self) -> Dict[str, Any]:
        """Stop profiling and return results."""
        current, peak = tracemalloc.get_traced_memory()
        tracemalloc.stop()

        return {
            "peak_memory": peak,
            "current_memory": current,
            "snapshots": self.snapshots,
            "memory_efficiency": self._calculate_efficiency(),
        }

    def _calculate_efficiency(self) -> float:
        """Calculate memory efficiency score."""
        if not self.snapshots:
            return 0.0

        peaks = [s["peak"] for s in self.snapshots]
        if not peaks:
            return 0.0

        avg_peak = sum(peaks) / len(peaks)
        max_peak = max(peaks)
        return 1.0 - (max_peak - avg_peak) / max_peak if max_peak > 0 else 1.0


class VectorStoreBenchmark:
    """Comprehensive benchmark suite for vector store integrations."""

    def __init__(self, db_pool: Any, config: Optional[BenchmarkConfig] = None):
        self.db_pool = db_pool
        self.config = config or BenchmarkConfig()
        self.results = []
        self.profiler = MemoryProfiler()

    async def setup_test_tables(self) -> Tuple[str, str]:
        """Create test tables for both frameworks."""
        langchain_table = f"bench_langchain_{uuid.uuid4().hex[:8]}"
        llamaindex_table = f"bench_llamaindex_{uuid.uuid4().hex[:8]}"

        async with self.db_pool.connection() as conn:
            # Enable pgvector
            try:
                await conn.execute("CREATE EXTENSION IF NOT EXISTS vector;")
            except Exception:
                pass  # Extension might already exist

            # Create tables
            for table in [langchain_table, llamaindex_table]:
                await conn.execute(f"""
                    CREATE TABLE {table} (
                        id TEXT PRIMARY KEY,
                        content TEXT NOT NULL,
                        embedding vector({self.config.embedding_dimension}),
                        metadata JSONB
                    );
                """)

                # Create vector index
                try:
                    await conn.execute(f"""
                        CREATE INDEX {table}_embedding_idx
                        ON {table} USING ivfflat (embedding vector_cosine_ops)
                        WITH (lists = 100);
                    """)
                except Exception:
                    pass  # Index creation might fail in some environments

        return langchain_table, llamaindex_table

    async def generate_test_documents(self, count: int) -> Tuple[List[Any], List[Any]]:
        """Generate test documents for both frameworks."""
        langchain_docs = []
        llamaindex_docs = []

        for i in range(count):
            content = (
                f"This is test document number {i} with some content for benchmarking purposes. "
                * 5
            )
            metadata = {
                "index": i,
                "category": f"category_{i % 10}",
                "tags": [f"tag_{j}" for j in range(i % 5)],
                "created_at": "2025-11-13T10:00:00Z",
            }

            # Mock embedding (normally would be generated by embedding model)
            embedding = [0.1 + (i * 0.001) % 0.8 for _ in range(self.config.embedding_dimension)]

            if LANGCHAIN_AVAILABLE:
                langchain_docs.append(LangChainDocument(page_content=content, metadata=metadata))

            if LLAMAINDEX_AVAILABLE:
                llamaindex_docs.append(LlamaDocument(text=content, metadata=metadata))

        return langchain_docs, llamaindex_docs

    async def benchmark_insertion(
        self, framework: str, vectorstore: Any, documents: List[Any], batch_size: int
    ) -> BenchmarkResult:
        """Benchmark document insertion performance."""
        self.profiler.start()

        start_time = time.time()
        errors = 0
        total_processed = 0

        try:
            # Process in batches
            for i in range(0, len(documents), batch_size):
                batch = documents[i : i + batch_size]
                self.profiler.snapshot(f"batch_{i // batch_size}_start")

                try:
                    if framework == "langchain":
                        # LangChain uses add_documents
                        await vectorstore.aadd_documents(batch)
                    else:  # llamaindex
                        # Convert documents to nodes (simplified for benchmark)
                        nodes = []
                        for doc in batch:
                            node = type(
                                "MockNode",
                                (),
                                {
                                    "id_": str(uuid.uuid4()),
                                    "text": doc.text,
                                    "embedding": [0.1] * self.config.embedding_dimension,
                                    "metadata": doc.metadata,
                                },
                            )()
                            nodes.append(node)
                        await vectorstore.aadd(nodes)

                    total_processed += len(batch)
                    self.profiler.snapshot(f"batch_{i // batch_size}_end")

                except Exception as e:
                    errors += 1
                    print(f"Batch error: {e}")

        except Exception as e:
            errors += 1
            print(f"Insertion benchmark error: {e}")

        duration = time.time() - start_time
        memory_stats = self.profiler.stop()

        return BenchmarkResult(
            operation="insertion",
            framework=framework,
            duration=duration,
            memory_usage=memory_stats["peak_memory"],
            throughput=total_processed / duration if duration > 0 else 0,
            error_rate=errors / max(1, len(documents) // batch_size),
            metadata={
                "batch_size": batch_size,
                "total_documents": len(documents),
                "processed_documents": total_processed,
                "memory_efficiency": memory_stats["memory_efficiency"],
            },
        )

    async def benchmark_search(
        self, framework: str, vectorstore: Any, query_count: int = 100
    ) -> BenchmarkResult:
        """Benchmark vector search performance."""
        self.profiler.start()

        # Generate query embeddings (mock)
        query_embeddings = [
            [0.1 + (i * 0.01) % 0.8 for _ in range(self.config.embedding_dimension)]
            for i in range(query_count)
        ]

        start_time = time.time()
        successful_queries = 0
        total_results = 0

        try:
            for i, query_embedding in enumerate(query_embeddings):
                self.profiler.snapshot(f"query_{i}_start")

                try:
                    if framework == "langchain":
                        results = await vectorstore.asimilarity_search_by_vector(
                            query_embedding, k=10
                        )
                    else:  # llamaindex
                        query = type(
                            "MockQuery",
                            (),
                            {
                                "query_embedding": query_embedding,
                                "similarity_top_k": 10,
                                "filters": None,
                            },
                        )()
                        results = await vectorstore.aquery(query)

                    if results:
                        successful_queries += 1
                        total_results += len(results) if hasattr(results, "__len__") else 1

                    self.profiler.snapshot(f"query_{i}_end")

                except Exception as e:
                    print(f"Query error: {e}")

        except Exception as e:
            print(f"Search benchmark error: {e}")

        duration = time.time() - start_time
        memory_stats = self.profiler.stop()

        return BenchmarkResult(
            operation="search",
            framework=framework,
            duration=duration,
            memory_usage=memory_stats["peak_memory"],
            throughput=successful_queries / duration if duration > 0 else 0,
            error_rate=(query_count - successful_queries) / query_count,
            metadata={
                "query_count": query_count,
                "successful_queries": successful_queries,
                "avg_results_per_query": total_results / max(1, successful_queries),
                "memory_efficiency": memory_stats["memory_efficiency"],
            },
        )

    async def benchmark_concurrent_load(
        self,
        framework: str,
        vectorstore: Any,
        concurrent_users: int = 10,
        operations_per_user: int = 50,
    ) -> BenchmarkResult:
        """Benchmark concurrent load performance."""
        self.profiler.start()

        async def user_workload(user_id: int):
            """Simulate user workload."""
            results = []
            for i in range(operations_per_user):
                try:
                    if i % 2 == 0:  # Mix of search and insertion
                        # Search operation
                        query_embedding = [
                            0.1 + (user_id * 0.01 + i * 0.001) % 0.8
                            for _ in range(self.config.embedding_dimension)
                        ]

                        if framework == "langchain":
                            result = await vectorstore.asimilarity_search_by_vector(
                                query_embedding, k=5
                            )
                        else:
                            query = type(
                                "MockQuery",
                                (),
                                {
                                    "query_embedding": query_embedding,
                                    "similarity_top_k": 5,
                                    "filters": None,
                                },
                            )()
                            result = await vectorstore.aquery(query)

                        results.append(len(result) if hasattr(result, "__len__") else 1)
                    else:
                        # Insertion operation
                        doc = MockDoc(
                            page_content=f"Concurrent doc {user_id}-{i}",
                            text=f"Concurrent doc {user_id}-{i}",
                            metadata={"user": user_id, "op": i},
                        )

                        if framework == "langchain":
                            await vectorstore.aadd_documents([doc])
                        else:
                            node = type(
                                "MockNode",
                                (),
                                {
                                    "id_": str(uuid.uuid4()),
                                    "text": doc.text,
                                    "embedding": [0.1] * self.config.embedding_dimension,
                                    "metadata": doc.metadata,
                                },
                            )()
                            await vectorstore.aadd([node])

                        results.append(1)

                except Exception as e:
                    results.append(0)  # Failed operation

            return results

        start_time = time.time()

        # Run concurrent users
        tasks = [user_workload(i) for i in range(concurrent_users)]
        user_results = await asyncio.gather(*tasks, return_exceptions=True)

        duration = time.time() - start_time
        memory_stats = self.profiler.stop()

        # Analyze results
        total_operations = concurrent_users * operations_per_user
        successful_operations = sum(
            len([r for r in results if r > 0])
            for results in user_results
            if not isinstance(results, BaseException) and isinstance(results, list)
        )

        return BenchmarkResult(
            operation="concurrent_load",
            framework=framework,
            duration=duration,
            memory_usage=memory_stats["peak_memory"],
            throughput=successful_operations / duration if duration > 0 else 0,
            error_rate=(total_operations - successful_operations) / total_operations,
            metadata={
                "concurrent_users": concurrent_users,
                "operations_per_user": operations_per_user,
                "total_operations": total_operations,
                "successful_operations": successful_operations,
                "memory_efficiency": memory_stats["memory_efficiency"],
            },
        )

    async def run_full_benchmark(self) -> Dict[str, Any]:
        """Run complete benchmark suite."""
        print("🚀 Starting FraiseQL Vector Store Integration Benchmarks")
        print(
            f"📊 Configuration: {self.config.document_count} documents, "
            f"{self.config.embedding_dimension}D embeddings"
        )

        # Setup
        langchain_table, llamaindex_table = await self.setup_test_tables()
        langchain_docs, llamaindex_docs = await self.generate_test_documents(
            self.config.document_count
        )

        results = []

        # Benchmark each framework
        frameworks = []
        if LANGCHAIN_AVAILABLE:
            frameworks.append(
                (
                    "langchain",
                    LangChainVectorStore(
                        db_pool=self.db_pool,
                        table_name=langchain_table,
                        embedding_function=None,  # We'll provide embeddings manually
                    ),
                    langchain_docs,
                )
            )

        if LLAMAINDEX_AVAILABLE:
            frameworks.append(
                (
                    "llamaindex",
                    LlamaIndexVectorStore(
                        db_pool=self.db_pool,
                        table_name=llamaindex_table,
                        embedding_dimension=self.config.embedding_dimension,
                    ),
                    llamaindex_docs,
                )
            )

        for framework_name, vectorstore, docs in frameworks:
            print(f"\n🔬 Benchmarking {framework_name.upper()}")

            # Warmup
            print("  📈 Running warmup...")
            for _ in range(self.config.warmup_iterations):
                await self.benchmark_insertion(framework_name, vectorstore, docs[:10], 10)

            # Insertion benchmarks
            print("  📥 Testing insertion performance...")
            for batch_size in self.config.batch_sizes:
                result = await self.benchmark_insertion(
                    framework_name, vectorstore, docs, batch_size
                )
                results.append(result)
                print(f".2fthroughput: {result.throughput:.1f} docs/sec")

            # Search benchmarks
            print("  🔍 Testing search performance...")
            search_result = await self.benchmark_search(
                framework_name, vectorstore, self.config.query_count
            )
            results.append(search_result)
            print(f".2fthroughput: {search_result.throughput:.1f} queries/sec")

            # Concurrent load benchmarks
            print("  ⚡ Testing concurrent load...")
            concurrent_result = await self.benchmark_concurrent_load(
                framework_name,
                vectorstore,
                self.config.concurrent_users,
                self.config.query_count // self.config.concurrent_users,
            )
            results.append(concurrent_result)
            print(f".2fthroughput: {concurrent_result.throughput:.1f} ops/sec")

        # Cleanup
        await self.cleanup_tables(langchain_table, llamaindex_table)

        # Generate report
        report = self.generate_report(results)
        print("\n📋 Benchmark Report Generated")
        print(json.dumps(report, indent=2, default=str))

        return report

    async def cleanup_tables(self, *table_names: str) -> None:
        """Clean up test tables."""
        async with self.db_pool.connection() as conn:
            for table in table_names:
                try:
                    await conn.execute(f"DROP TABLE IF EXISTS {table} CASCADE;")
                except Exception:
                    pass

    def generate_report(self, results: List[BenchmarkResult]) -> Dict[str, Any]:
        """Generate comprehensive benchmark report."""
        report = {
            "timestamp": time.time(),
            "config": {
                "document_count": self.config.document_count,
                "embedding_dimension": self.config.embedding_dimension,
                "batch_sizes": self.config.batch_sizes,
                "concurrent_users": self.config.concurrent_users,
                "query_count": self.config.query_count,
            },
            "frameworks": {},
            "comparisons": {},
        }

        # Group results by framework
        framework_results = {}
        for result in results:
            if result.framework not in framework_results:
                framework_results[result.framework] = []
            framework_results[result.framework].append(result)

        # Analyze each framework
        for framework, f_results in framework_results.items():
            report["frameworks"][framework] = {
                "operations": {},
                "summary": {
                    "avg_throughput": sum(r.throughput for r in f_results) / len(f_results),
                    "avg_memory_usage": sum(r.memory_usage for r in f_results) / len(f_results),
                    "total_errors": sum(
                        r.error_rate * r.metadata.get("total_operations", 1) for r in f_results
                    ),
                    "memory_efficiency": sum(
                        r.metadata.get("memory_efficiency", 0) for r in f_results
                    )
                    / len(f_results),
                },
            }

            # Group by operation
            operations = {}
            for result in f_results:
                if result.operation not in operations:
                    operations[result.operation] = []
                operations[result.operation].append(result)

            for op_name, op_results in operations.items():
                report["frameworks"][framework]["operations"][op_name] = {
                    "count": len(op_results),
                    "avg_duration": sum(r.duration for r in op_results) / len(op_results),
                    "avg_throughput": sum(r.throughput for r in op_results) / len(op_results),
                    "avg_memory": sum(r.memory_usage for r in op_results) / len(op_results),
                    "error_rate": sum(r.error_rate for r in op_results) / len(op_results),
                    "results": [r.metadata for r in op_results],
                }

        # Generate comparisons
        if len(framework_results) > 1:
            frameworks = list(framework_results.keys())
            report["comparisons"] = {
                "performance_ratio": {},
                "memory_efficiency": {},
                "error_rates": {},
            }

            for op in ["insertion", "search", "concurrent_load"]:
                op_results = {}
                for framework in frameworks:
                    op_data = report["frameworks"][framework]["operations"].get(op, {})
                    if op_data:
                        op_results[framework] = op_data["avg_throughput"]

                if len(op_results) > 1:
                    # Calculate relative performance
                    max_throughput = max(op_results.values())
                    for framework, throughput in op_results.items():
                        ratio = throughput / max_throughput if max_throughput > 0 else 0
                        report["comparisons"]["performance_ratio"][f"{framework}_{op}"] = ratio

        return report


async def main() -> None:
    """Run benchmark suite."""
    # This would normally get a database pool from your application
    # For now, we'll create a mock to show the structure
    print("Note: This benchmark requires a PostgreSQL database pool.")
    print("Run with: python benchmarks/vectorstore_integration_benchmark.py")

    # Example usage:
    """
    import psycopg_pool
    import asyncio

    async def run_benchmarks():
        # Setup database connection
        conninfo = "postgresql://user:password@localhost:5432/fraiseql_test"
        db_pool = psycopg_pool.AsyncConnectionPool(conninfo)

        # Configure benchmark
        config = BenchmarkConfig(
            document_count=5000,
            embedding_dimension=1536,  # OpenAI ada-002
            concurrent_users=20
        )

        # Run benchmarks
        benchmark = VectorStoreBenchmark(db_pool, config)
        results = await benchmark.run_full_benchmark()

        # Save results
        with open("benchmark_results.json", "w") as f:
            json.dump(results, f, indent=2, default=str)

    asyncio.run(run_benchmarks())
    """


if __name__ == "__main__":
    asyncio.run(main())
