# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 2
# mutation_decorator.py (NEW)
result_json = await db.execute_function_raw_json(
    full_function_name,
    input_data,
    type_name=self.success_type.__name__,  # For Rust transformer
)
# Returns: RawJSONResult (JSON string, no parsing!)

# Rust transformer already applied:
# - snake_case → camelCase ✅
# - __typename injection ✅
# - All nested objects transformed ✅

return result_json  # FastAPI returns directly, no serialization!
