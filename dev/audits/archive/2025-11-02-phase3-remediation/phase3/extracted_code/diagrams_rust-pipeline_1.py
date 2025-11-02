# Extracted from: docs/diagrams/rust-pipeline.md
# Block number: 1
from fraiseql import RustPipeline


class GraphQLApp:
    def __init__(self):
        self.rust_pipeline = RustPipeline()

    async def execute_query(self, query, variables):
        # Execute SQL query
        raw_data = await db.execute_sql_query(query, variables)

        # Optional Rust processing
        if self.should_use_rust_pipeline(query):
            processed_data = self.rust_pipeline.process(raw_data)
            return processed_data

        # Fallback to Python processing
        return self.python_response_builder.build(raw_data)
