# Extracted from: docs/rust/RUST_FIELD_PROJECTION.md
# Block number: 1
# src/fraiseql/core/ast_parser.py (existing code)


def extract_field_paths_from_info(info, transform_path=None):
    """Extract requested fields from GraphQL query.

    Example:
        query {
          users {
            id
            firstName
            email
          }
        }

    Returns:
        ["id", "first_name", "email"]  # snake_case
    """
    # ... existing implementation ...
