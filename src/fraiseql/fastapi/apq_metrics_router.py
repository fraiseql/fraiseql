"""FastAPI router for APQ metrics and monitoring endpoints."""

import logging
from pathlib import Path

from fastapi import APIRouter, HTTPException, Request
from fastapi.responses import HTMLResponse, JSONResponse
from fastapi.templating import Jinja2Templates

from fraiseql.monitoring import get_global_metrics
from fraiseql.storage import apq_store

logger = logging.getLogger(__name__)

# Set up Jinja2 templates
TEMPLATE_DIR = Path(__file__).parent / "templates"
templates = Jinja2Templates(directory=str(TEMPLATE_DIR))

# Create router for APQ metrics
router = APIRouter(prefix="/admin/apq", tags=["APQ Metrics"])


@router.get("/dashboard", response_class=HTMLResponse)
async def get_apq_dashboard(request: Request) -> HTMLResponse:
    """Serve the APQ monitoring dashboard.

    Interactive web dashboard for monitoring APQ (Automatic Persisted Queries)
    performance in real-time.

    Features:
    - Real-time metrics with auto-refresh every 5 seconds
    - Query cache and response cache hit/miss visualization
    - Top queries by usage frequency
    - System health status with warnings
    - Interactive charts using Chart.js
    - Responsive design for desktop and mobile

    Returns:
        HTML page with interactive dashboard

    Usage:
        Navigate to http://your-server:port/admin/apq/dashboard in a browser
        to view the monitoring interface.

    Dashboard Sections:
    - **Key Metrics**: Hit rates, request counts, storage usage
    - **Charts**: Visual representation of cache performance
    - **Top Queries**: Most frequently used queries with statistics
    - **Health**: System status with automatic warnings

    The dashboard automatically refreshes data from the /admin/apq/stats
    and /admin/apq/top-queries endpoints every 5 seconds.
    """
    return templates.TemplateResponse("apq_dashboard.html", {"request": request})


@router.get("/stats")
async def get_apq_stats() -> JSONResponse:
    """Get current APQ statistics and metrics.

    Returns comprehensive APQ system statistics including:
    - Query cache hit/miss rates
    - Response cache hit/miss rates (if enabled)
    - Storage statistics
    - Performance metrics
    - Top queries by usage frequency
    - System health assessment

    Example Response:
        ```json
        {
          "current": {
            "timestamp": "2025-10-17T18:44:34.993117+00:00",
            "query_cache": {
              "hits": 150,
              "misses": 10,
              "stores": 10,
              "hit_rate": 0.9375
            },
            "response_cache": {
              "hits": 0,
              "misses": 0,
              "stores": 0,
              "hit_rate": 0.0
            },
            "storage": {
              "stored_queries": 10,
              "cached_responses": 0,
              "total_bytes": 5432
            },
            "performance": {
              "total_requests": 160,
              "overall_hit_rate": 0.9375,
              "avg_parse_time_ms": null
            }
          },
          "top_queries": [
            {
              "hash": "abc123...",
              "total_requests": 50,
              "hit_count": 48,
              "miss_count": 2,
              "avg_parse_time_ms": 25.5,
              "first_seen": "2025-10-17T18:00:00+00:00",
              "last_seen": "2025-10-17T18:44:00+00:00"
            }
          ],
          "health": {
            "status": "healthy",
            "warnings": []
          }
        }
        ```

    Health Status:
    - "healthy": Query cache hit rate >70%, system performing well
    - "warning": Query cache hit rate 50-70%, or response cache issues
    - "critical": Query cache hit rate <50%, system may need attention

    Warnings:
    - Low query cache hit rate (<70%)
    - Low response cache hit rate (<50%, when enabled)
    - High storage usage (>100MB)
    """
    try:
        metrics = get_global_metrics()

        # Get storage stats from backend
        storage_stats = apq_store.get_storage_stats()
        metrics.update_storage_stats(
            stored_queries=storage_stats.get("stored_queries", 0),
            cached_responses=0,  # Memory backend doesn't track this separately
            total_bytes=storage_stats.get("total_size_bytes", 0),
        )

        # Export comprehensive metrics
        metrics_data = metrics.export_metrics()

        return JSONResponse(
            content=metrics_data,
            status_code=200,
        )

    except Exception as e:
        logger.exception("Failed to retrieve APQ stats")
        raise HTTPException(
            status_code=500,
            detail=f"Failed to retrieve APQ statistics: {e!s}",
        )


@router.get("/metrics")
async def get_apq_metrics() -> JSONResponse:
    """Get APQ metrics in Prometheus-compatible format.

    Returns metrics formatted for easy integration with monitoring systems
    like Prometheus, Grafana, or custom dashboards.

    Example Response:
        ```json
        {
          "apq_query_cache_hits_total": 150,
          "apq_query_cache_misses_total": 10,
          "apq_query_cache_hit_rate": 0.9375,
          "apq_response_cache_hits_total": 0,
          "apq_response_cache_misses_total": 0,
          "apq_response_cache_hit_rate": 0.0,
          "apq_stored_queries_total": 10,
          "apq_cached_responses_total": 0,
          "apq_storage_bytes_total": 5432,
          "apq_requests_total": 160,
          "apq_overall_hit_rate": 0.9375,
          "apq_health_status": "healthy"
        }
        ```

    Metric Descriptions:
    - `apq_query_cache_hits_total`: Number of query string cache hits
    - `apq_query_cache_misses_total`: Number of query string cache misses
    - `apq_query_cache_hit_rate`: Query cache hit rate (0.0 to 1.0)
    - `apq_response_cache_hits_total`: Number of response cache hits
    - `apq_response_cache_misses_total`: Number of response cache misses
    - `apq_response_cache_hit_rate`: Response cache hit rate (0.0 to 1.0)
    - `apq_stored_queries_total`: Number of queries in storage
    - `apq_cached_responses_total`: Number of cached responses
    - `apq_storage_bytes_total`: Total storage size in bytes
    - `apq_requests_total`: Total number of APQ requests
    - `apq_overall_hit_rate`: Overall cache hit rate across both layers
    - `apq_health_status`: System health ("healthy", "warning", "critical")
    """
    try:
        metrics = get_global_metrics()
        snapshot = metrics.get_snapshot()
        metrics_data = metrics.export_metrics()

        # Format for Prometheus-style metrics
        prometheus_metrics = {
            # Query cache metrics
            "apq_query_cache_hits_total": snapshot.query_cache_hits,
            "apq_query_cache_misses_total": snapshot.query_cache_misses,
            "apq_query_cache_stores_total": snapshot.query_cache_stores,
            "apq_query_cache_hit_rate": snapshot.query_cache_hit_rate,
            # Response cache metrics
            "apq_response_cache_hits_total": snapshot.response_cache_hits,
            "apq_response_cache_misses_total": snapshot.response_cache_misses,
            "apq_response_cache_stores_total": snapshot.response_cache_stores,
            "apq_response_cache_hit_rate": snapshot.response_cache_hit_rate,
            # Storage metrics
            "apq_stored_queries_total": snapshot.stored_queries_count,
            "apq_cached_responses_total": snapshot.cached_responses_count,
            "apq_storage_bytes_total": snapshot.total_storage_bytes,
            # Performance metrics
            "apq_requests_total": snapshot.total_requests,
            "apq_overall_hit_rate": snapshot.overall_hit_rate,
            # Health
            "apq_health_status": metrics_data["health"]["status"],
        }

        if snapshot.avg_query_parse_time_ms is not None:
            prometheus_metrics["apq_avg_parse_time_ms"] = snapshot.avg_query_parse_time_ms

        return JSONResponse(
            content=prometheus_metrics,
            status_code=200,
        )

    except Exception as e:
        logger.exception("Failed to retrieve APQ metrics")
        raise HTTPException(
            status_code=500,
            detail=f"Failed to retrieve APQ metrics: {e!s}",
        )


@router.get("/top-queries")
async def get_top_queries(limit: int = 10) -> JSONResponse:
    """Get top N queries by usage frequency.

    Args:
        limit: Number of top queries to return (default: 10, max: 100)

    Returns:
        List of most frequently requested queries with statistics

    Example Response:
        ```json
        {
          "top_queries": [
            {
              "hash": "abc123...",
              "total_requests": 50,
              "hit_count": 48,
              "miss_count": 2,
              "avg_parse_time_ms": 25.5,
              "first_seen": "2025-10-17T18:00:00+00:00",
              "last_seen": "2025-10-17T18:44:00+00:00"
            }
          ],
          "count": 1
        }
        ```

    Use Cases:
    - Identify most frequently used queries for optimization
    - Detect queries with low cache hit rates
    - Pre-warm cache with popular queries
    - Analyze query patterns for caching strategy
    """
    if limit < 1 or limit > 100:
        raise HTTPException(
            status_code=400,
            detail="Limit must be between 1 and 100",
        )

    try:
        metrics = get_global_metrics()
        top_queries = metrics.get_top_queries(limit=limit)

        return JSONResponse(
            content={
                "top_queries": top_queries,
                "count": len(top_queries),
            },
            status_code=200,
        )

    except Exception as e:
        logger.exception("Failed to retrieve top queries")
        raise HTTPException(
            status_code=500,
            detail=f"Failed to retrieve top queries: {e!s}",
        )


@router.post("/reset")
async def reset_apq_metrics() -> JSONResponse:
    """Reset APQ metrics to zero.

    ⚠️ WARNING: This will clear all accumulated metrics data.
    Use with caution, typically only in development/testing.

    Returns:
        Confirmation message

    Example Response:
        ```json
        {
          "status": "success",
          "message": "APQ metrics have been reset"
        }
        ```
    """
    try:
        metrics = get_global_metrics()
        metrics.reset()

        return JSONResponse(
            content={
                "status": "success",
                "message": "APQ metrics have been reset",
            },
            status_code=200,
        )

    except Exception as e:
        logger.exception("Failed to reset APQ metrics")
        raise HTTPException(
            status_code=500,
            detail=f"Failed to reset APQ metrics: {e!s}",
        )


@router.get("/health")
async def get_apq_health() -> JSONResponse:
    """Get APQ system health status.

    Provides a simple health check endpoint for monitoring and alerting.

    Returns:
        Health status and any warnings

    Example Response (Healthy):
        ```json
        {
          "status": "healthy",
          "query_cache_hit_rate": 0.9375,
          "response_cache_hit_rate": 0.0,
          "total_requests": 160,
          "warnings": []
        }
        ```

    Example Response (Warning):
        ```json
        {
          "status": "warning",
          "query_cache_hit_rate": 0.65,
          "response_cache_hit_rate": 0.0,
          "total_requests": 160,
          "warnings": [
            "Low query cache hit rate: 65.0% (target: >70%)"
          ]
        }
        ```

    Status Codes:
    - 200: System healthy or has warnings
    - 500: Failed to retrieve health status
    - 503: System unhealthy (critical status)
    """
    try:
        metrics = get_global_metrics()
        metrics_data = metrics.export_metrics()
        snapshot = metrics.get_snapshot()

        health_data = {
            "status": metrics_data["health"]["status"],
            "query_cache_hit_rate": snapshot.query_cache_hit_rate,
            "response_cache_hit_rate": snapshot.response_cache_hit_rate,
            "total_requests": snapshot.total_requests,
            "warnings": metrics_data["health"]["warnings"],
        }

        # Return 503 for critical status
        status_code = 503 if health_data["status"] == "critical" else 200

        return JSONResponse(
            content=health_data,
            status_code=status_code,
        )

    except Exception as e:
        logger.exception("Failed to retrieve APQ health")
        raise HTTPException(
            status_code=500,
            detail=f"Failed to retrieve APQ health: {e!s}",
        )


@router.get("/history")
async def get_apq_history(limit: int = 10) -> JSONResponse:
    """Get historical APQ metrics snapshots.

    Provides time-series data for trend analysis and graphing.

    Args:
        limit: Number of historical snapshots to return (default: 10, max: 100)

    Returns:
        List of historical metric snapshots (most recent first)

    Example Response:
        ```json
        {
          "snapshots": [
            {
              "timestamp": "2025-10-17T18:44:34+00:00",
              "query_cache": {
                "hits": 150,
                "misses": 10,
                "hit_rate": 0.9375
              },
              ...
            },
            {
              "timestamp": "2025-10-17T18:43:34+00:00",
              "query_cache": {
                "hits": 140,
                "misses": 9,
                "hit_rate": 0.9396
              },
              ...
            }
          ],
          "count": 2
        }
        ```

    Use Cases:
    - Trend analysis
    - Performance graphing
    - Anomaly detection
    - Capacity planning
    """
    if limit < 1 or limit > 100:
        raise HTTPException(
            status_code=400,
            detail="Limit must be between 1 and 100",
        )

    try:
        metrics = get_global_metrics()
        snapshots = metrics.get_historical_snapshots(limit=limit)

        return JSONResponse(
            content={
                "snapshots": [s.to_dict() for s in snapshots],
                "count": len(snapshots),
            },
            status_code=200,
        )

    except Exception as e:
        logger.exception("Failed to retrieve APQ history")
        raise HTTPException(
            status_code=500,
            detail=f"Failed to retrieve APQ history: {e!s}",
        )
