# FraiseQL Utils

This directory contains utility functions for the FraiseQL library.

## Field Collection Process

The field collection logic in `fraiseql_builder.py` is responsible for transforming Python class annotations into FraiseQL field definitions. Here's how it works:

### Overview

1. **Annotation Collection**: Walks through the class's Method Resolution Order (MRO) to collect all type annotations, respecting inheritance.

2. **Field Extraction**: For each annotation, determines the appropriate `FraiseQLField`:
   - If the class attribute is already a `FraiseQLField`, use it directly
   - If the annotation includes a `fraise_field()` in `Annotated[T, fraise_field()]`, use that
   - Otherwise, create a new `FraiseQLField` with any default value found

3. **Field Configuration**: Each field is configured with:
   - Type information from the annotation
   - A unique name and index for ordering
   - A purpose ("input", "output", "both", or "type")
   - Init behavior based on the type kind

4. **Field Filtering**: Fields are included or excluded based on their purpose and the type being created:
   - Input types include fields with purpose "input" or "both"
   - Output types include fields with purpose "output" or "both"
   - Type types include fields with purpose "type", "both", or "output"

### Example

```python
@fraise_type
class User:
    id: str  # Creates fraise_field(field_type=str)
    name: str = "Anonymous"  # Creates fraise_field(field_type=str, default="Anonymous")
    email: Annotated[str, fraise_field(description="User email")]  # Uses explicit field
    admin: bool = fraise_field(default=False)  # Uses the field directly
```

### Field Purpose and Init Behavior

- Fields in `@fraise_type` decorated classes with `purpose="output"` are excluded from `__init__`
- Fields in `@success` and `@failure` decorated classes keep all fields in `__init__` regardless of purpose
- This allows mutation results to be constructed with all fields while preventing output-only fields in regular types from being set during initialization
