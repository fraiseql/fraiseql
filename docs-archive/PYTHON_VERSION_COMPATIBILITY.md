# Python Version Compatibility Analysis for FraiseQL

## Current Configuration

The project is currently configured for **Python 3.13** as specified in:
- `pyproject.toml`: `requires-python = ">=3.13"`
- `tool.black.target-version = ["py313"]`
- `tool.ruff.target-version = "py313"`
- `tool.pyright.pythonVersion = "3.13"`

## Modern Python Features Used

### 1. Union Type Operator `|` (Python 3.10+)
Found extensive usage throughout the codebase:
- Type hints like `list[str] | None` instead of `Union[list[str], None]`
- Examples:
  - `/src/fraiseql/core/ast_parser.py`: `path: list[str] | None = None`
  - `/src/fraiseql/sql/where_generator.py`: `dict[str, Any] | None`
  - `/src/fraiseql/mutations/sql_generator.py`: `isinstance(value, uuid.UUID | ipaddress.IPv4Address | ipaddress.IPv6Address)`

### 2. Built-in Generic Types (Python 3.9+)
Using `list[...]`, `dict[...]` instead of `List[...]`, `Dict[...]` from typing:
- Found in 18+ files using `list[...]` syntax
- No imports of `List`, `Dict` from typing module

### 3. Walrus Operator `:=` (Python 3.8+)
Found in `/src/fraiseql/mutations/sql_generator.py` line 90:
```python
if (value := getattr(input_object, f.name)) is not UNSET
```

### 4. Other Modern Features
- Using `collections.abc` imports (modern approach)
- No match statements found (would require Python 3.10+)
- No dict merge operators `|`, `|=` for dicts found (would require Python 3.9+)
- No positional-only parameters found (would require Python 3.8+)
- No f-strings with `=` specifier found (would require Python 3.8+)

## Minimum Python Version Required

Based on the analysis, the **absolute minimum Python version** required is **Python 3.10** due to:
- Extensive use of the union type operator `|` in type hints
- Use of built-in generic types without typing imports

## Recommendations

1. **Update `pyproject.toml`** to reflect the actual minimum version:
   ```toml
   requires-python = ">=3.10"
   ```

2. **Consider the trade-offs**:
   - **Python 3.10**: Minimum required, supports all current syntax
   - **Python 3.11**: Better performance, improved error messages
   - **Python 3.12**: Type parameter syntax, better f-string formatting
   - **Python 3.13**: Latest features, but limits user adoption

3. **For broader compatibility**, consider:
   - Replacing `|` with `Union` from typing
   - Using `List`, `Dict` from typing instead of built-in generics
   - This would allow Python 3.8+ support

## Dependencies Python Version Requirements

Most dependencies support Python 3.8+:
- FastAPI: Python 3.8+
- Pydantic v2: Python 3.8+
- psycopg3: Python 3.8+
- graphql-core: Python 3.8+

The dependencies would support a lower Python version if the code syntax was adjusted.
