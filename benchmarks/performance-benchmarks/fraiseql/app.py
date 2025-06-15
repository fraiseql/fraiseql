"""FraiseQL benchmark application using updated architecture."""

import os
from datetime import datetime

from fastapi import Response
from fraiseql import create_fraiseql_app
from fraiseql.fastapi import FraiseQLConfig
from prometheus_client import Counter, Histogram, generate_latest
from pydantic_settings import BaseSettings

from models import (
    Category, Order, Product, User, ProductReview,
    PopularProduct, ProductsByCategory, UserStats
)
from queries import Query
from mutations import (
    CreateUser, CreateUserInput, CreateUserSuccess, CreateUserError,
    CreateOrder, CreateOrderInput, OrderItemInput, CreateOrderSuccess, CreateOrderError,
    AddProductReview, AddProductReviewInput, AddProductReviewSuccess, AddProductReviewError
)


class Settings(BaseSettings):
    database_url: str = "postgresql://benchmark:benchmark@postgres:5432/benchmark_db?options=-csearch_path=benchmark"
    results_dir: str = "/app/results"
    
    class Config:
        env_file = ".env"


settings = Settings()

# Prometheus metrics
query_counter = Counter(
    'fraiseql_queries_total',
    'Total number of GraphQL queries',
    ['operation', 'type']
)

query_histogram = Histogram(
    'fraiseql_query_duration_seconds',
    'GraphQL query duration in seconds',
    ['operation', 'type']
)


# Configure FraiseQL
config = FraiseQLConfig(
    database_url=settings.database_url,
    auto_camel_case=True,  # Enable automatic snake_case to camelCase conversion
    enable_introspection=True,
    enable_playground=True,
)

# Create FraiseQL app with GraphQL endpoint
app = create_fraiseql_app(
    config=config,
    types=[
        # Core types
        User, Product, Order, Category, ProductReview,
        # Specialized view types
        PopularProduct, ProductsByCategory, UserStats,
        # Query root
        Query,
        # Mutations
        CreateUser, CreateUserInput, CreateUserSuccess, CreateUserError,
        CreateOrder, CreateOrderInput, OrderItemInput, CreateOrderSuccess, CreateOrderError,
        AddProductReview, AddProductReviewInput, AddProductReviewSuccess, AddProductReviewError,
    ],
    title="FraiseQL Benchmark API",
    version="1.0.0",
    description="Benchmark API for FraiseQL performance testing",
)

# Add health check endpoint
@app.get("/health")
async def health_check():
    """Health check endpoint"""
    return {"status": "healthy", "timestamp": datetime.utcnow().isoformat()}

# Add metrics endpoint
@app.get("/metrics")
async def metrics():
    """Prometheus metrics endpoint"""
    return Response(generate_latest(), media_type="text/plain")

# Initialize results directory
os.makedirs(settings.results_dir, exist_ok=True)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)