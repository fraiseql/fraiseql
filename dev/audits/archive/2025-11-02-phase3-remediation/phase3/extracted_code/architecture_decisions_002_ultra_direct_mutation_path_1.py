# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 1
# mutation_decorator.py (line ~145)
result = await db.execute_function(full_function_name, input_data)
# Returns: dict {'success': True, 'customer': {...}, ...}

parsed_result = parse_mutation_result(
    result,  # Parse dict into dataclass
    self.success_type,
    self.error_type,
)
# Returns: DeleteCustomerSuccess(customer=Customer(...), ...)

return parsed_result  # GraphQL serializes back to JSON!
