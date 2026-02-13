# Installation Guide

## System Requirements

- Python 3.10 or higher
- pip or uv package manager

## Install from PyPI

```bash
pip install fraiseql
```

Or with `uv`:

```bash
uv pip install fraiseql
```

## Verify Installation

```bash
python -c "from fraiseql import type, query, mutation, export_schema; print('✅ FraiseQL installed successfully')"
```

## Development Installation

If you're contributing to FraiseQL, install from source with development dependencies:

```bash
git clone https://github.com/yourusername/fraiseql.git
cd fraiseql/fraiseql-python

# Install in editable mode with dev dependencies
pip install -e ".[dev]"

# Run tests
pytest tests/ -v

# Check linting
ruff check src/ tests/
```

## Troubleshooting

### ModuleNotFoundError: No module named 'fraiseql'

Ensure the package is installed:

```bash
pip list | grep fraiseql
```

If not present, reinstall:

```bash
pip install --force-reinstall fraiseql
```

### Python version too old

Check your Python version:

```bash
python --version
```

FraiseQL requires Python 3.10+. If you have an older version, upgrade Python or use a virtual environment with a newer version.

### Permission denied on installation

Use `--user` flag:

```bash
pip install --user fraiseql
```

Or use a virtual environment (recommended):

```bash
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate
pip install fraiseql
```

## Next Steps

- Read the [Getting Started Guide](GETTING_STARTED.md)
- Check out [Example Schemas](EXAMPLES.md)
- Review the [Decorators Reference](DECORATORS_REFERENCE.md)
