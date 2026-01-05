"""AxumServer wrapper class for Axum HTTP server.

Provides Python-friendly lifecycle management and request execution interface
for the Rust-based Axum GraphQL server.
"""

import asyncio
import json
import logging
from contextlib import asynccontextmanager, contextmanager
from typing import Any, Generator, Type

try:
    from fraiseql._fraiseql_rs import PyAxumServer
except ImportError:
    PyAxumServer = None  # type: ignore[assignment]


from fraiseql.axum.config import AxumFraiseQLConfig

logger = logging.getLogger(__name__)


class AxumServer:
    """Axum-based GraphQL server wrapper.

    Wraps the Rust PyAxumServer FFI binding and provides:
    - Type registration for GraphQL schema
    - Server lifecycle management (start/shutdown)
    - Direct query execution (for tests/jobs)
    - Configuration management
    - Metrics access

    Attributes:
        _config: Server configuration
        _py_server: Rust FFI server instance
        _is_running: Current running state
        _types: Registered GraphQL types
        _mutations: Registered mutations
        _queries: Registered queries
        _subscriptions: Registered subscriptions
    """

    def __init__(self, config: AxumFraiseQLConfig):
        """Initialize Axum server.

        Args:
            config: AxumFraiseQLConfig instance

        Raises:
            ImportError: If PyAxumServer FFI binding not available (raised on start)
        """
        self._config = config
        self._py_server: PyAxumServer | None = None
        self._is_running = False
        self._types: dict[str, Type[Any]] = {}
        self._mutations: dict[str, Type[Any]] = {}
        self._queries: dict[str, Type[Any]] = {}
        self._subscriptions: dict[str, Type[Any]] = {}

        logger.debug(f"Initialized AxumServer: {config}")

    # ===== Type Registration =====

    def register_types(self, types: list[Type[Any]]) -> None:
        """Register GraphQL types.

        Args:
            types: List of @fraiseql.type decorated classes
        """
        for type_ in types:
            type_name = getattr(type_, "__name__", str(type_))
            self._types[type_name] = type_
        logger.debug(f"Registered {len(types)} GraphQL types")

    def register_mutations(self, mutations: list[Type[Any]]) -> None:
        """Register GraphQL mutations.

        Args:
            mutations: List of mutation classes
        """
        for mut in mutations:
            mut_name = getattr(mut, "__name__", str(mut))
            self._mutations[mut_name] = mut
        logger.debug(f"Registered {len(mutations)} mutations")

    def register_queries(self, queries: list[Type[Any]]) -> None:
        """Register GraphQL queries.

        Args:
            queries: List of query classes
        """
        for query in queries:
            query_name = getattr(query, "__name__", str(query))
            self._queries[query_name] = query
        logger.debug(f"Registered {len(queries)} queries")

    def register_subscriptions(self, subscriptions: list[Type[Any]]) -> None:
        """Register GraphQL subscriptions.

        Args:
            subscriptions: List of subscription classes
        """
        for sub in subscriptions:
            sub_name = getattr(sub, "__name__", str(sub))
            self._subscriptions[sub_name] = sub
        logger.debug(f"Registered {len(subscriptions)} subscriptions")

    def add_middleware(self, middleware: Any) -> None:
        """Add Axum middleware.

        Note: Full middleware support is deferred to Phase 16.
        For now, this is a no-op placeholder.

        Args:
            middleware: Middleware instance
        """
        logger.warning("Middleware support is planned for Phase 16")

    # ===== Lifecycle Management =====

    def start(
        self,
        host: str | None = None,
        port: int | None = None,
        workers: int | None = None,
    ) -> None:
        """Start the HTTP server (blocking).

        This call blocks the main thread until the server is shut down.
        For non-blocking startup, use start_async() instead.

        Args:
            host: Bind address (default: from config)
            port: Bind port (default: from config)
            workers: Number of worker threads (default: from config)

        Raises:
            RuntimeError: If server is already running
            ValueError: If database connection fails

        Example:
            ```python
            app = create_axum_fraiseql_app(database_url="...")
            app.start(host="0.0.0.0", port=8000)  # Blocking
            ```
        """
        if self._is_running:
            raise RuntimeError("Server is already running")

        if PyAxumServer is None:
            raise ImportError(
                "PyAxumServer FFI binding not available. "
                "Ensure fraiseql_rs is compiled and installed."
            )

        host = host or self._config.axum_host
        port = port or self._config.axum_port

        try:
            # Initialize Rust FFI server
            self._py_server = PyAxumServer.new(
                database_url=self._config.database_url,
                metrics_admin_token=self._config.axum_metrics_token,
            )

            # Start HTTP server
            self._py_server.start(host=host, port=port)
            self._is_running = True

            logger.info(
                f"FraiseQL Axum server running at http://{host}:{port}",
                extra={
                    "performance": "7-10x faster than FastAPI",
                    "graphql": "/graphql",
                    "metrics": "/metrics",
                },
            )

        except Exception as e:
            self._is_running = False
            logger.error(f"Failed to start Axum server: {e}")
            raise

    async def start_async(
        self,
        host: str | None = None,
        port: int | None = None,
    ) -> None:
        """Start server asynchronously (non-blocking).

        This starts the server in a background task, allowing the caller
        to continue with other async operations.

        Args:
            host: Bind address (default: from config)
            port: Bind port (default: from config)

        Raises:
            RuntimeError: If server already running

        Example:
            ```python
            app = create_axum_fraiseql_app(database_url="...")
            await app.start_async(host="0.0.0.0", port=8000)
            # Server runs in background
            await asyncio.sleep(10)
            await app.shutdown()
            ```
        """
        if self._is_running:
            raise RuntimeError("Server is already running")

        if PyAxumServer is None:
            raise ImportError(
                "PyAxumServer FFI binding not available. "
                "Ensure fraiseql_rs is compiled and installed."
            )

        host = host or self._config.axum_host
        port = port or self._config.axum_port

        try:
            # Initialize Rust FFI server
            self._py_server = PyAxumServer.new(
                database_url=self._config.database_url,
                metrics_admin_token=self._config.axum_metrics_token,
            )

            # Start HTTP server (async)
            self._py_server.start(host=host, port=port)
            self._is_running = True

            logger.info(f"FraiseQL Axum server started at http://{host}:{port}")

        except Exception as e:
            self._is_running = False
            logger.error(f"Failed to start async Axum server: {e}")
            raise

    async def shutdown(self) -> None:
        """Gracefully shutdown the server.

        Closes all connections and cleans up resources.

        Example:
            ```python
            await app.shutdown()
            ```
        """
        if not self._is_running:
            logger.warning("Server is not running")
            return

        try:
            if self._py_server:
                self._py_server.shutdown()
            self._is_running = False
            logger.info("FraiseQL Axum server stopped")
        except Exception as e:
            logger.error(f"Error during shutdown: {e}")
            raise

    def is_running(self) -> bool:
        """Check if server is running.

        Returns:
            True if server is running, False otherwise
        """
        return self._is_running and (self._py_server is not None and self._py_server.is_running())

    # ===== Request Execution (Direct API) =====

    def execute_query(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
        context: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Execute GraphQL query directly (synchronous).

        Executes query without HTTP, useful for:
        - Testing
        - Background jobs
        - Internal operations

        Args:
            query: GraphQL query string
            variables: Query variables dict
            operation_name: Name of operation (if multiple)
            context: Execution context (reserved for future use)

        Returns:
            GraphQL response: {"data": {...}} or {"errors": [...]}

        Raises:
            RuntimeError: If server not initialized

        Example:
            ```python
            result = app.execute_query(
                'query GetUser($id: ID!) { user(id: $id) { id name } }',
                variables={"id": "123"}
            )
            assert result["data"]["user"]["id"] == "123"
            ```
        """
        if not self._py_server:
            raise RuntimeError("Server not initialized. Call start() or start_async() first.")

        try:
            variables_json = json.dumps(variables) if variables else None
            result_json = self._py_server.execute_query(
                query=query,
                variables=variables_json,
                operation_name=operation_name,
            )
            return json.loads(result_json)
        except json.JSONDecodeError as e:
            logger.error(f"Failed to parse GraphQL response: {e}")
            return {"errors": [{"message": f"Failed to parse response: {e}"}]}
        except Exception as e:
            logger.error(f"Query execution failed: {e}")
            return {"errors": [{"message": str(e)}]}

    async def execute_query_async(
        self,
        query: str,
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
    ) -> dict[str, Any]:
        """Execute GraphQL query asynchronously.

        Wrapper around execute_query that runs in event loop.

        Args:
            query: GraphQL query string
            variables: Query variables dict
            operation_name: Name of operation (if multiple)

        Returns:
            GraphQL response: {"data": {...}} or {"errors": [...]}
        """
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(
            None, self.execute_query, query, variables, operation_name
        )

    # ===== Configuration & Introspection =====

    def get_config(self) -> AxumFraiseQLConfig:
        """Get server configuration.

        Returns:
            AxumFraiseQLConfig instance
        """
        return self._config

    def get_schema(self) -> dict[str, Any]:
        """Get GraphQL schema introspection.

        Executes the standard GraphQL introspection query to retrieve
        schema information. This includes types, mutations, subscriptions, etc.

        Returns:
            Introspection result: __schema with types, queryType, mutationType, etc.

        Raises:
            RuntimeError: If server not initialized
        """
        introspection_query = """
        query IntrospectionQuery {
            __schema {
                types { name description }
                queryType { name }
                mutationType { name }
                subscriptionType { name }
            }
        }
        """
        return self.execute_query(introspection_query)

    def get_metrics(self) -> str:
        """Get server metrics in Prometheus format.

        Returns:
            Prometheus format metrics string

        Raises:
            RuntimeError: If server not initialized
            ValueError: If metrics_admin_token not set or invalid
        """
        if not self._py_server:
            raise RuntimeError("Server not initialized")

        if not self._config.axum_metrics_token:
            logger.warning("No metrics token configured, metrics may not be accessible")

        metrics_str = self._py_server.get_metrics()
        return metrics_str

    # ===== Type Introspection =====

    def registered_types(self) -> list[str]:
        """Get list of registered GraphQL types.

        Returns:
            List of type names
        """
        return list(self._types.keys())

    def registered_mutations(self) -> list[str]:
        """Get list of registered mutations.

        Returns:
            List of mutation names
        """
        return list(self._mutations.keys())

    def registered_queries(self) -> list[str]:
        """Get list of registered queries.

        Returns:
            List of query names
        """
        return list(self._queries.keys())

    def registered_subscriptions(self) -> list[str]:
        """Get list of registered subscriptions.

        Returns:
            List of subscription names
        """
        return list(self._subscriptions.keys())

    # ===== Context Managers =====

    @contextmanager
    def running(self, host: str = "127.0.0.1", port: int = 8000) -> Generator["AxumServer"]:
        """Context manager for server lifecycle (blocking).

        Starts the server on entry and stops on exit. Useful for tests.

        Note: This blocks the main thread. For async context, use running_async().

        Args:
            host: Bind address
            port: Bind port

        Yields:
            AxumServer instance (self)

        Example:
            ```python
            app = create_axum_fraiseql_app(database_url="...")

            with app.running(host="0.0.0.0", port=8000):
                # Server is running here
                response = requests.post(
                    "http://0.0.0.0:8000/graphql",
                    json={"query": "{ users { id } }"}
                )
                assert response.status_code == 200
            # Server is stopped here
            ```
        """
        self.start(host=host, port=port)
        try:
            yield self
        finally:
            if self._is_running:
                # Stop server synchronously
                if self._py_server:
                    self._py_server.shutdown()
                self._is_running = False

    @asynccontextmanager
    async def running_async(
        self, host: str = "127.0.0.1", port: int = 8000
    ) -> Generator["AxumServer"]:  # type: ignore[type-arg]
        """Context manager for server lifecycle (async).

        Starts the server on entry and stops on exit. Non-blocking.

        Args:
            host: Bind address
            port: Bind port

        Yields:
            AxumServer instance (self)

        Example:
            ```python
            app = create_axum_fraiseql_app(database_url="...")

            async with app.running_async(host="0.0.0.0", port=8000):
                # Server is running here
                response = await asyncio.sleep(1)
            # Server is stopped here
            ```
        """
        await self.start_async(host=host, port=port)
        try:
            yield self
        finally:
            if self._is_running:
                await self.shutdown()

    # ===== Utility Methods =====

    def __repr__(self) -> str:
        """String representation."""
        return (
            f"AxumServer("
            f"host={self._config.axum_host}, "
            f"port={self._config.axum_port}, "
            f"running={self._is_running})"
        )

    def __str__(self) -> str:
        """User-friendly string."""
        status = "running" if self._is_running else "stopped"
        return f"FraiseQL Axum Server [{status}] at {self._config.server_url}"
