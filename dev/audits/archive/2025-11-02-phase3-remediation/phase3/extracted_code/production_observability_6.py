# Extracted from: docs/production/observability.md
# Block number: 6
from fraiseql.monitoring.exporters import PostgreSQLSpanExporter
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor


# Configure OpenTelemetry to export to PostgreSQL
def setup_tracing(db_pool):
    # Create PostgreSQL exporter
    exporter = PostgreSQLSpanExporter(db_pool)

    # Configure tracer provider
    provider = TracerProvider()
    processor = BatchSpanProcessor(exporter)
    provider.add_span_processor(processor)

    # Set as global tracer provider
    trace.set_tracer_provider(provider)

    return trace.get_tracer(__name__)


tracer = setup_tracing(db_pool)
