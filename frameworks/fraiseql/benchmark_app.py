"""FraiseQL Benchmark Application

Simple FastAPI application implementing the benchmark GraphQL schema.
"""

import logging

from benchmark_schema import schema
from fastapi import FastAPI, Request
from strawberry.fastapi import GraphQLRouter

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Create FastAPI app
app = FastAPI(title="FraiseQL Benchmark API")

# Add GraphQL router
graphql_app = GraphQLRouter(schema)
app.include_router(graphql_app, prefix="/graphql")


# Health check endpoint
@app.get("/health")
async def health_check():
    """Health check endpoint for Docker."""
    return {"status": "healthy"}


# Metrics endpoint (simple implementation)
@app.get("/metrics")
async def metrics():
    """Simple metrics endpoint."""
    return {
        "status": "ok",
        "message": "Basic metrics - full implementation would track query performance",
    }


# Request logging middleware
@app.middleware("http")
async def log_requests(request: Request, call_next):
    """Log all requests."""
    logger.info(f"{request.method} {request.url}")
    response = await call_next(request)
    logger.info(f"Response: {response.status_code}")
    return response


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)
