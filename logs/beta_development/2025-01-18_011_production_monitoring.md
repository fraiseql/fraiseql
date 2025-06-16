# Beta Development Log: Sprint 2 - Production Monitoring
**Date**: 2025-01-18  
**Time**: 14:00 UTC  
**Session**: 011  
**Author**: DevOps Lead (Viktor wants "total visibility")

## Production Monitoring & Observability Stack

### Comprehensive Metrics System

#### Created: `/src/fraiseql/monitoring/metrics.py`
```python
"""Production metrics for FraiseQL."""

from prometheus_client import (
    Counter, Histogram, Gauge, Summary, Info,
    CollectorRegistry, generate_latest
)
from typing import Dict, Any, Optional
import time
import psutil
import asyncio

# Create custom registry
registry = CollectorRegistry()

# System Information
system_info = Info(
    'fraiseql_system',
    'FraiseQL system information',
    registry=registry
)

# Request Metrics
graphql_requests_total = Counter(
    'fraiseql_graphql_requests_total',
    'Total GraphQL requests',
    ['operation_type', 'operation_name', 'status'],
    registry=registry
)

graphql_request_duration = Histogram(
    'fraiseql_graphql_request_duration_seconds',
    'GraphQL request duration',
    ['operation_type', 'operation_name'],
    buckets=[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
    registry=registry
)

graphql_field_duration = Histogram(
    'fraiseql_graphql_field_duration_seconds',
    'Individual field resolver duration',
    ['type_name', 'field_name'],
    buckets=[0.001, 0.005, 0.01, 0.05, 0.1, 0.5],
    registry=registry
)

# Query Metrics
graphql_query_complexity = Histogram(
    'fraiseql_graphql_query_complexity',
    'Query complexity score',
    ['operation_name'],
    buckets=[1, 10, 50, 100, 500, 1000, 5000],
    registry=registry
)

graphql_query_depth = Histogram(
    'fraiseql_graphql_query_depth',
    'Query depth',
    ['operation_name'],
    buckets=[1, 2, 3, 5, 7, 10, 15],
    registry=registry
)

# Database Metrics
database_queries_total = Counter(
    'fraiseql_database_queries_total',
    'Total database queries',
    ['query_type', 'table'],
    registry=registry
)

database_query_duration = Histogram(
    'fraiseql_database_query_duration_seconds',
    'Database query duration',
    ['query_type'],
    buckets=[0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0],
    registry=registry
)

database_connections_active = Gauge(
    'fraiseql_database_connections_active',
    'Active database connections',
    registry=registry
)

database_connections_idle = Gauge(
    'fraiseql_database_connections_idle',
    'Idle database connections',
    registry=registry
)

# DataLoader Metrics
dataloader_batch_size = Histogram(
    'fraiseql_dataloader_batch_size',
    'DataLoader batch sizes',
    ['loader_type'],
    buckets=[1, 5, 10, 25, 50, 100, 250, 500, 1000],
    registry=registry
)

dataloader_cache_hits = Counter(
    'fraiseql_dataloader_cache_hits_total',
    'DataLoader cache hits',
    ['loader_type'],
    registry=registry
)

dataloader_cache_misses = Counter(
    'fraiseql_dataloader_cache_misses_total',
    'DataLoader cache misses',
    ['loader_type'],
    registry=registry
)

# Subscription Metrics
subscription_active = Gauge(
    'fraiseql_subscription_active',
    'Active subscriptions',
    ['subscription_name'],
    registry=registry
)

subscription_messages_sent = Counter(
    'fraiseql_subscription_messages_sent_total',
    'Subscription messages sent',
    ['subscription_name'],
    registry=registry
)

subscription_errors = Counter(
    'fraiseql_subscription_errors_total',
    'Subscription errors',
    ['subscription_name', 'error_type'],
    registry=registry
)

# Error Metrics
graphql_errors_total = Counter(
    'fraiseql_graphql_errors_total',
    'GraphQL errors',
    ['error_type', 'operation_name'],
    registry=registry
)

unhandled_errors_total = Counter(
    'fraiseql_unhandled_errors_total',
    'Unhandled errors',
    ['error_type'],
    registry=registry
)

# Performance Metrics
n1_queries_detected = Counter(
    'fraiseql_n1_queries_detected_total',
    'N+1 queries detected',
    ['query_pattern'],
    registry=registry
)

slow_queries_total = Counter(
    'fraiseql_slow_queries_total',
    'Queries exceeding duration threshold',
    ['operation_name', 'threshold'],
    registry=registry
)

# Resource Metrics
memory_usage_bytes = Gauge(
    'fraiseql_memory_usage_bytes',
    'Memory usage in bytes',
    ['type'],  # rss, vms, shared
    registry=registry
)

cpu_usage_percent = Gauge(
    'fraiseql_cpu_usage_percent',
    'CPU usage percentage',
    registry=registry
)

# Business Metrics
business_operations_total = Counter(
    'fraiseql_business_operations_total',
    'Business operations',
    ['operation', 'status'],
    registry=registry
)


class MetricsCollector:
    """Collects and exposes metrics."""
    
    def __init__(self):
        self.process = psutil.Process()
        self._collection_task: Optional[asyncio.Task] = None
    
    async def start(self):
        """Start metrics collection."""
        # Set system info
        system_info.info({
            'version': '0.1.0a3',
            'python_version': '3.11',
            'environment': 'production'
        })
        
        # Start collection loop
        self._collection_task = asyncio.create_task(self._collect_loop())
    
    async def stop(self):
        """Stop metrics collection."""
        if self._collection_task:
            self._collection_task.cancel()
            await asyncio.gather(self._collection_task, return_exceptions=True)
    
    async def _collect_loop(self):
        """Periodically collect system metrics."""
        while True:
            try:
                # Memory metrics
                memory = self.process.memory_info()
                memory_usage_bytes.labels(type='rss').set(memory.rss)
                memory_usage_bytes.labels(type='vms').set(memory.vms)
                
                # CPU metrics
                cpu_percent = self.process.cpu_percent(interval=1)
                cpu_usage_percent.set(cpu_percent)
                
                # Database pool metrics (if available)
                from fraiseql.fastapi.dependencies import get_db_pool
                pool = get_db_pool()
                if pool:
                    database_connections_active.set(pool.size - pool.idle)
                    database_connections_idle.set(pool.idle)
                
                await asyncio.sleep(10)  # Collect every 10 seconds
                
            except asyncio.CancelledError:
                break
            except Exception as e:
                print(f"Metrics collection error: {e}")
    
    def get_metrics(self) -> bytes:
        """Get Prometheus metrics."""
        return generate_latest(registry)
```

### OpenTelemetry Tracing

#### Created: `/src/fraiseql/monitoring/tracing.py`
```python
"""OpenTelemetry tracing for FraiseQL."""

from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.sdk.resources import Resource
from opentelemetry.instrumentation.asyncpg import AsyncPGInstrumentor
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor
from opentelemetry.trace import Status, StatusCode

from typing import Dict, Any, Optional, Callable
from functools import wraps
import json

# Configure tracer
resource = Resource(attributes={
    "service.name": "fraiseql",
    "service.version": "0.1.0a3",
})

provider = TracerProvider(resource=resource)
processor = BatchSpanProcessor(
    OTLPSpanExporter(endpoint="localhost:4317", insecure=True)
)
provider.add_span_processor(processor)
trace.set_tracer_provider(provider)

tracer = trace.get_tracer("fraiseql")


class GraphQLTracer:
    """Traces GraphQL operations."""
    
    @staticmethod
    def trace_operation(operation_type: str, operation_name: str):
        """Trace a GraphQL operation."""
        def decorator(func):
            @wraps(func)
            async def wrapper(*args, **kwargs):
                with tracer.start_as_current_span(
                    f"graphql.{operation_type}",
                    attributes={
                        "graphql.operation.type": operation_type,
                        "graphql.operation.name": operation_name,
                    }
                ) as span:
                    try:
                        result = await func(*args, **kwargs)
                        span.set_status(Status(StatusCode.OK))
                        return result
                    except Exception as e:
                        span.set_status(
                            Status(StatusCode.ERROR, str(e))
                        )
                        span.record_exception(e)
                        raise
            
            return wrapper
        return decorator
    
    @staticmethod
    def trace_field(type_name: str, field_name: str):
        """Trace a field resolver."""
        def decorator(func):
            @wraps(func)
            async def wrapper(*args, **kwargs):
                with tracer.start_as_current_span(
                    f"graphql.field.{type_name}.{field_name}",
                    attributes={
                        "graphql.type": type_name,
                        "graphql.field": field_name,
                    }
                ) as span:
                    try:
                        result = await func(*args, **kwargs)
                        
                        # Add result metadata
                        if isinstance(result, list):
                            span.set_attribute("result.count", len(result))
                        
                        return result
                    except Exception as e:
                        span.record_exception(e)
                        raise
            
            return wrapper
        return decorator
    
    @staticmethod
    def trace_dataloader(loader_type: str):
        """Trace DataLoader operations."""
        def decorator(func):
            @wraps(func)
            async def wrapper(self, keys: list):
                with tracer.start_as_current_span(
                    f"dataloader.{loader_type}",
                    attributes={
                        "dataloader.type": loader_type,
                        "dataloader.keys.count": len(keys),
                        "dataloader.keys.sample": str(keys[:3]),
                    }
                ) as span:
                    try:
                        result = await func(self, keys)
                        
                        # Record cache efficiency
                        cache_hits = getattr(self, '_cache_hits', 0)
                        cache_total = getattr(self, '_cache_total', 0)
                        if cache_total > 0:
                            span.set_attribute(
                                "dataloader.cache.hit_rate",
                                cache_hits / cache_total
                            )
                        
                        return result
                    except Exception as e:
                        span.record_exception(e)
                        raise
            
            return wrapper
        return decorator


def instrument_fraiseql():
    """Instrument FraiseQL with OpenTelemetry."""
    # Instrument AsyncPG
    AsyncPGInstrumentor().instrument()
    
    # Instrument FastAPI
    def request_hook(span: trace.Span, scope: dict):
        """Add custom attributes to HTTP spans."""
        if span and span.is_recording():
            # Add user info if available
            if "user" in scope:
                span.set_attribute("user.id", scope["user"].get("id"))
    
    FastAPIInstrumentor.instrument(
        request_hook=request_hook
    )


# Utility functions
def trace_async(name: str, **attributes):
    """Decorator to trace async functions."""
    def decorator(func):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            with tracer.start_as_current_span(name, attributes=attributes):
                return await func(*args, **kwargs)
        return wrapper
    return decorator


def get_current_trace_id() -> Optional[str]:
    """Get current trace ID."""
    span = trace.get_current_span()
    if span and span.is_recording():
        return format(span.get_span_context().trace_id, '032x')
    return None
```

### Structured Logging

#### Created: `/src/fraiseql/monitoring/logging.py`
```python
"""Structured logging for FraiseQL."""

import logging
import json
import sys
from datetime import datetime
from typing import Dict, Any, Optional
from contextvars import ContextVar

from pythonjsonlogger import jsonlogger

# Context variables for request-scoped data
request_id_var: ContextVar[Optional[str]] = ContextVar('request_id', default=None)
user_id_var: ContextVar[Optional[str]] = ContextVar('user_id', default=None)
trace_id_var: ContextVar[Optional[str]] = ContextVar('trace_id', default=None)


class ContextualFormatter(jsonlogger.JsonFormatter):
    """Adds contextual information to log records."""
    
    def add_fields(self, log_record: Dict[str, Any], record: logging.LogRecord, message_dict: Dict[str, Any]):
        super().add_fields(log_record, record, message_dict)
        
        # Add timestamp
        log_record['timestamp'] = datetime.utcnow().isoformat()
        
        # Add contextual data
        log_record['request_id'] = request_id_var.get()
        log_record['user_id'] = user_id_var.get()
        log_record['trace_id'] = trace_id_var.get()
        
        # Add service metadata
        log_record['service'] = 'fraiseql'
        log_record['environment'] = 'production'
        log_record['version'] = '0.1.0a3'
        
        # Add error details if present
        if record.exc_info:
            log_record['error_type'] = record.exc_info[0].__name__
            log_record['error_message'] = str(record.exc_info[1])


def setup_logging(level: str = "INFO", json_output: bool = True):
    """Configure structured logging."""
    # Create logger
    logger = logging.getLogger('fraiseql')
    logger.setLevel(getattr(logging, level.upper()))
    
    # Remove existing handlers
    logger.handlers.clear()
    
    # Create handler
    handler = logging.StreamHandler(sys.stdout)
    
    if json_output:
        # JSON formatter for production
        formatter = ContextualFormatter(
            '%(timestamp)s %(level)s %(name)s %(message)s'
        )
    else:
        # Human-readable for development
        formatter = logging.Formatter(
            '%(asctime)s - %(name)s - %(levelname)s - %(message)s'
        )
    
    handler.setFormatter(formatter)
    logger.addHandler(handler)
    
    return logger


# Create default logger
logger = setup_logging()


class GraphQLLogger:
    """Specialized logger for GraphQL operations."""
    
    @staticmethod
    def log_query(
        operation_type: str,
        operation_name: str,
        duration: float,
        complexity: int,
        errors: Optional[list] = None
    ):
        """Log GraphQL query execution."""
        logger.info(
            "GraphQL operation executed",
            extra={
                "operation_type": operation_type,
                "operation_name": operation_name,
                "duration_ms": duration * 1000,
                "complexity": complexity,
                "success": errors is None,
                "error_count": len(errors) if errors else 0,
            }
        )
    
    @staticmethod
    def log_field_error(
        type_name: str,
        field_name: str,
        error: Exception
    ):
        """Log field resolver error."""
        logger.error(
            f"Field resolver error: {type_name}.{field_name}",
            extra={
                "graphql_type": type_name,
                "graphql_field": field_name,
                "error_type": type(error).__name__,
                "error_message": str(error),
            },
            exc_info=True
        )
    
    @staticmethod
    def log_n1_detected(query_pattern: str, count: int):
        """Log N+1 query detection."""
        logger.warning(
            "N+1 query pattern detected",
            extra={
                "query_pattern": query_pattern,
                "execution_count": count,
                "detection_type": "n1_query",
            }
        )


class DatabaseLogger:
    """Specialized logger for database operations."""
    
    @staticmethod
    def log_query(
        query: str,
        duration: float,
        rows_affected: Optional[int] = None
    ):
        """Log database query."""
        logger.debug(
            "Database query executed",
            extra={
                "query": query[:200],  # Truncate long queries
                "duration_ms": duration * 1000,
                "rows_affected": rows_affected,
            }
        )
    
    @staticmethod
    def log_slow_query(
        query: str,
        duration: float,
        threshold: float
    ):
        """Log slow database query."""
        logger.warning(
            f"Slow query detected: {duration:.2f}s",
            extra={
                "query": query[:200],
                "duration_ms": duration * 1000,
                "threshold_ms": threshold * 1000,
                "detection_type": "slow_query",
            }
        )
```

### Health Check System

#### Created: `/src/fraiseql/monitoring/health.py`
```python
"""Health check system for FraiseQL."""

from typing import Dict, Any, List, Callable, Optional
from dataclasses import dataclass
from enum import Enum
import asyncio
import time

from fraiseql.monitoring.metrics import (
    database_connections_active,
    memory_usage_bytes,
    cpu_usage_percent
)


class HealthStatus(Enum):
    """Health check status levels."""
    HEALTHY = "healthy"
    DEGRADED = "degraded"
    UNHEALTHY = "unhealthy"


@dataclass
class HealthCheckResult:
    """Result of a health check."""
    name: str
    status: HealthStatus
    message: str
    details: Dict[str, Any]
    duration_ms: float


class HealthChecker:
    """Manages health checks."""
    
    def __init__(self):
        self._checks: Dict[str, Callable] = {}
    
    def register_check(self, name: str, check_func: Callable):
        """Register a health check."""
        self._checks[name] = check_func
    
    async def check_all(self) -> Dict[str, Any]:
        """Run all health checks."""
        results = []
        overall_status = HealthStatus.HEALTHY
        
        # Run checks concurrently
        check_tasks = []
        for name, check_func in self._checks.items():
            task = asyncio.create_task(self._run_check(name, check_func))
            check_tasks.append(task)
        
        check_results = await asyncio.gather(*check_tasks, return_exceptions=True)
        
        # Process results
        for result in check_results:
            if isinstance(result, Exception):
                # Check failed
                results.append(HealthCheckResult(
                    name="unknown",
                    status=HealthStatus.UNHEALTHY,
                    message=str(result),
                    details={},
                    duration_ms=0
                ))
                overall_status = HealthStatus.UNHEALTHY
            else:
                results.append(result)
                
                # Update overall status
                if result.status == HealthStatus.UNHEALTHY:
                    overall_status = HealthStatus.UNHEALTHY
                elif result.status == HealthStatus.DEGRADED and overall_status == HealthStatus.HEALTHY:
                    overall_status = HealthStatus.DEGRADED
        
        return {
            "status": overall_status.value,
            "checks": [
                {
                    "name": r.name,
                    "status": r.status.value,
                    "message": r.message,
                    "details": r.details,
                    "duration_ms": r.duration_ms
                }
                for r in results
            ],
            "timestamp": time.time()
        }
    
    async def _run_check(self, name: str, check_func: Callable) -> HealthCheckResult:
        """Run a single health check."""
        start = time.time()
        
        try:
            result = await check_func()
            duration = (time.time() - start) * 1000
            
            return HealthCheckResult(
                name=name,
                status=result.get("status", HealthStatus.HEALTHY),
                message=result.get("message", "OK"),
                details=result.get("details", {}),
                duration_ms=duration
            )
        except Exception as e:
            duration = (time.time() - start) * 1000
            
            return HealthCheckResult(
                name=name,
                status=HealthStatus.UNHEALTHY,
                message=f"Check failed: {str(e)}",
                details={"error": str(e)},
                duration_ms=duration
            )


# Default health checks
async def check_database():
    """Check database connectivity."""
    from fraiseql.fastapi.dependencies import get_db_pool
    
    pool = get_db_pool()
    if not pool:
        return {
            "status": HealthStatus.UNHEALTHY,
            "message": "No database pool available"
        }
    
    try:
        async with pool.acquire() as conn:
            start = time.time()
            await conn.execute("SELECT 1")
            query_time = time.time() - start
        
        # Check pool health
        pool_usage = (pool.size - pool.idle) / pool.size
        
        if query_time > 1.0:
            status = HealthStatus.DEGRADED
            message = "Database responding slowly"
        elif pool_usage > 0.9:
            status = HealthStatus.DEGRADED
            message = "Database pool near capacity"
        else:
            status = HealthStatus.HEALTHY
            message = "Database healthy"
        
        return {
            "status": status,
            "message": message,
            "details": {
                "query_time_ms": query_time * 1000,
                "pool_size": pool.size,
                "pool_idle": pool.idle,
                "pool_usage_percent": pool_usage * 100
            }
        }
    except Exception as e:
        return {
            "status": HealthStatus.UNHEALTHY,
            "message": f"Database check failed: {str(e)}"
        }


async def check_memory():
    """Check memory usage."""
    import psutil
    
    process = psutil.Process()
    memory = process.memory_info()
    memory_percent = process.memory_percent()
    
    # Get system memory
    virtual_memory = psutil.virtual_memory()
    
    if memory_percent > 90:
        status = HealthStatus.UNHEALTHY
        message = "Critical memory usage"
    elif memory_percent > 70:
        status = HealthStatus.DEGRADED
        message = "High memory usage"
    else:
        status = HealthStatus.HEALTHY
        message = "Memory usage normal"
    
    return {
        "status": status,
        "message": message,
        "details": {
            "process_memory_mb": memory.rss / 1024 / 1024,
            "process_memory_percent": memory_percent,
            "system_memory_percent": virtual_memory.percent,
            "system_memory_available_gb": virtual_memory.available / 1024 / 1024 / 1024
        }
    }


async def check_subscriptions():
    """Check subscription system health."""
    from fraiseql.subscriptions.registry import ConnectionRegistry
    
    registry = ConnectionRegistry.get_current()
    if not registry:
        return {
            "status": HealthStatus.HEALTHY,
            "message": "No active subscription registry"
        }
    
    active_connections = registry.active_connections
    total_subscriptions = registry.total_subscriptions
    
    if active_connections > 10000:
        status = HealthStatus.UNHEALTHY
        message = "Too many active connections"
    elif active_connections > 5000:
        status = HealthStatus.DEGRADED
        message = "High number of connections"
    else:
        status = HealthStatus.HEALTHY
        message = "Subscription system healthy"
    
    return {
        "status": status,
        "message": message,
        "details": {
            "active_connections": active_connections,
            "total_subscriptions": total_subscriptions,
            "avg_subscriptions_per_connection": (
                total_subscriptions / active_connections 
                if active_connections > 0 else 0
            )
        }
    }
```

### Monitoring Dashboard

#### Created: `/src/fraiseql/monitoring/dashboard.py`
```python
"""Monitoring dashboard for FraiseQL."""

from fastapi import FastAPI, Request, Response
from fastapi.responses import HTMLResponse
import json

from fraiseql.monitoring.metrics import MetricsCollector
from fraiseql.monitoring.health import HealthChecker


def add_monitoring_routes(app: FastAPI):
    """Add monitoring routes to FastAPI app."""
    
    # Initialize components
    metrics_collector = MetricsCollector()
    health_checker = HealthChecker()
    
    # Register health checks
    from fraiseql.monitoring.health import (
        check_database, check_memory, check_subscriptions
    )
    health_checker.register_check("database", check_database)
    health_checker.register_check("memory", check_memory)
    health_checker.register_check("subscriptions", check_subscriptions)
    
    @app.on_event("startup")
    async def start_monitoring():
        """Start monitoring systems."""
        await metrics_collector.start()
    
    @app.on_event("shutdown")
    async def stop_monitoring():
        """Stop monitoring systems."""
        await metrics_collector.stop()
    
    @app.get("/metrics", response_class=Response)
    async def metrics():
        """Prometheus metrics endpoint."""
        return Response(
            content=metrics_collector.get_metrics(),
            media_type="text/plain"
        )
    
    @app.get("/health")
    async def health():
        """Health check endpoint."""
        return await health_checker.check_all()
    
    @app.get("/health/{check_name}")
    async def health_check(check_name: str):
        """Individual health check."""
        if check_name not in health_checker._checks:
            return {"error": f"Unknown check: {check_name}"}
        
        check_func = health_checker._checks[check_name]
        result = await health_checker._run_check(check_name, check_func)
        
        return {
            "name": result.name,
            "status": result.status.value,
            "message": result.message,
            "details": result.details,
            "duration_ms": result.duration_ms
        }
    
    @app.get("/dashboard", response_class=HTMLResponse)
    async def dashboard():
        """Simple monitoring dashboard."""
        return HTMLResponse("""
        <!DOCTYPE html>
        <html>
        <head>
            <title>FraiseQL Monitoring</title>
            <style>
                body { font-family: Arial, sans-serif; margin: 20px; }
                .metric { 
                    background: #f0f0f0; 
                    padding: 10px; 
                    margin: 10px 0; 
                    border-radius: 5px; 
                }
                .healthy { border-left: 5px solid #4CAF50; }
                .degraded { border-left: 5px solid #FF9800; }
                .unhealthy { border-left: 5px solid #F44336; }
                h2 { color: #333; }
                pre { background: #fff; padding: 10px; }
            </style>
        </head>
        <body>
            <h1>FraiseQL Monitoring Dashboard</h1>
            
            <h2>Health Status</h2>
            <div id="health"></div>
            
            <h2>Metrics</h2>
            <div id="metrics"></div>
            
            <script>
                async function updateDashboard() {
                    // Fetch health
                    const healthRes = await fetch('/health');
                    const health = await healthRes.json();
                    
                    const healthDiv = document.getElementById('health');
                    healthDiv.innerHTML = health.checks.map(check => `
                        <div class="metric ${check.status}">
                            <h3>${check.name}</h3>
                            <p>Status: ${check.status}</p>
                            <p>${check.message}</p>
                            <pre>${JSON.stringify(check.details, null, 2)}</pre>
                        </div>
                    `).join('');
                    
                    // Update metrics display
                    const metricsRes = await fetch('/metrics');
                    const metricsText = await metricsRes.text();
                    
                    const metricsDiv = document.getElementById('metrics');
                    const relevantMetrics = metricsText
                        .split('\\n')
                        .filter(line => 
                            line.includes('fraiseql_') && 
                            !line.startsWith('#')
                        )
                        .slice(0, 20)
                        .join('\\n');
                    
                    metricsDiv.innerHTML = `<pre>${relevantMetrics}</pre>`;
                }
                
                // Update every 5 seconds
                updateDashboard();
                setInterval(updateDashboard, 5000);
            </script>
        </body>
        </html>
        """)
```

### Viktor's Monitoring Review

*Viktor walks in with multiple monitors showing dashboards*

"Production monitoring! The difference between 'it works' and 'it works in production'. Let's see...

COMPREHENSIVE:
- Prometheus metrics for everything - good
- OpenTelemetry tracing - distributed debugging solved
- Structured logging with context - excellent
- Health checks with gradual degradation - smart

DASHBOARD CHECK:
*Opens monitoring dashboard*
- Real-time metrics ✓
- Health visualization ✓
- Performance tracking ✓
- Error aggregation ✓

LOAD TEST RESULTS:
```
10,000 concurrent requests:
- p50 latency: 12ms
- p95 latency: 45ms  
- p99 latency: 120ms
- Error rate: 0.02%
- Memory stable at 450MB
- CPU average: 35%
```

This is production-grade monitoring!

FINAL REQUIREMENTS:
1. Add alerting rules for Prometheus
2. Create Grafana dashboards
3. Add distributed tracing examples
4. Document SLOs/SLIs

Once those are done, we're ready for staging deployment.

*Actually looks impressed*

You know what? This might actually survive production. Ship it to staging and let's see how it handles real traffic!"

*Pins note: "Monitoring: APPROVED. Deploy to staging Monday."*

---
Next Log: Staging deployment and load testing