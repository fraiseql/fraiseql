# Extracted from: docs/architecture/decisions/002_ultra_direct_mutation_path.md
# Block number: 9
# mutations.py (UNCHANGED)
from fraiseql import mutation


@mutation(function="app.delete_customer")
class DeleteCustomer:
    input: DeleteCustomerInput
    success: DeleteCustomerSuccess
    failure: DeleteCustomerError
