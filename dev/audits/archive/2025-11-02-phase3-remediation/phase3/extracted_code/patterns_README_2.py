# Extracted from: docs/patterns/README.md
# Block number: 2
@authorized(roles=["admin", "editor"])
@mutation
class DeletePost:
    input: DeletePostInput
    success: DeleteSuccess
