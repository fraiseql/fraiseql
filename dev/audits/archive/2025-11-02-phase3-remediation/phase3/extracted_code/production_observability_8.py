# Extracted from: docs/production/observability.md
# Block number: 8
from opentelemetry.instrumentation.asyncpg import AsyncPGInstrumentor
from opentelemetry.instrumentation.fastapi import FastAPIInstrumentor

# Instrument FastAPI automatically
FastAPIInstrumentor.instrument_app(app)

# Instrument asyncpg (PostgreSQL driver)
AsyncPGInstrumentor().instrument()
