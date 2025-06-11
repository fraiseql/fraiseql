# Installation

## Requirements

FraiseQL requires:
- Python 3.13+
- PostgreSQL 14+
- psycopg3

## Install from PyPI

```bash
pip install fraiseql
```

## Install from Source

For the latest development version:

```bash
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql
pip install -e .
```

## Optional Dependencies

### FastAPI Integration
```bash
pip install fraiseql[fastapi]
```

### Auth0 Support
```bash
pip install fraiseql[auth0]
```

### Development Tools
```bash
pip install fraiseql[dev]
```

## Database Setup

FraiseQL requires a PostgreSQL database with JSONB support (PostgreSQL 9.4+).

### Create a Database

```bash
createdb myapp
```

### Enable Required Extensions

Connect to your database and run:

```sql
-- Enable UUID generation
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Enable ltree for hierarchical data (optional)
CREATE EXTENSION IF NOT EXISTS ltree;
```

## Verify Installation

```python
import fraiseql as fql

print(fql.__version__)
```

## Next Steps

Now that you have FraiseQL installed, let's [create your first API](./quickstart.md)!
