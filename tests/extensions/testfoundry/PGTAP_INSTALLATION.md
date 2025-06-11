# pgTAP Installation Guide

This guide provides various methods to install pgTAP for running FraiseQL's TestFoundry tests.

## Installation Methods

### 1. Using APT Package Manager (Debian/Ubuntu)

The easiest method on Debian-based systems:

```bash
# Install for your PostgreSQL version (e.g., PostgreSQL 16)
sudo apt update
sudo apt install postgresql-16-pgtap

# Or install the generic package
sudo apt install pgtap
```

### 2. Using PostgreSQL Extension

If pgTAP is installed as a PostgreSQL extension:

```sql
CREATE EXTENSION IF NOT EXISTS pgtap;
```

### 3. Using Docker

#### Option A: Use a pre-built pgTAP Docker image

```dockerfile
FROM lmergner/docker-pgtap:latest
# Or
FROM subzerocloud/pgtap-docker:latest
```

#### Option B: Install in existing PostgreSQL container

```dockerfile
FROM postgres:16

RUN apt-get update && apt-get install -y \
    postgresql-16-pgtap \
    && rm -rf /var/lib/apt/lists/*
```

### 4. Manual Installation from Source

```bash
# Download from PGXN
wget https://api.pgxn.org/dist/pgtap/1.3.3/pgtap-1.3.3.zip
unzip pgtap-1.3.3.zip
cd pgtap-1.3.3

# Install
make
make install
make installcheck
```

### 5. Using PGDG Repository

For the latest versions:

```bash
# Add PostgreSQL APT repository
sudo apt install -y postgresql-common
sudo /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh

# Install pgTAP
sudo apt update
sudo apt install postgresql-16-pgtap
```

## Testing pgTAP Installation

After installation, test that pgTAP works:

```sql
-- Connect to your database
CREATE EXTENSION IF NOT EXISTS pgtap;

-- Run a simple test
SELECT plan(1);
SELECT pass('pgTAP is working!');
SELECT * FROM finish();
```

## Troubleshooting

1. **Extension not found error**: Make sure the pgTAP package matches your PostgreSQL version
2. **Permission denied**: pgTAP requires superuser privileges to install
3. **Missing dependencies**: Install `postgresql-server-dev-XX` package for your PostgreSQL version

## Notes for CI/CD

For GitHub Actions or other CI systems, consider:

1. Using a PostgreSQL service container with pgTAP pre-installed
2. Installing pgTAP in the setup phase of your workflow
3. Caching the pgTAP installation between runs

## Alternative Download URLs

The test suite tries multiple sources:

1. **PGXN API**: `https://api.pgxn.org/dist/pgtap/1.3.3/pgtap-1.3.3.zip`
2. **GitHub Release**: `https://github.com/theory/pgtap/archive/refs/tags/v1.3.3.tar.gz`
3. **Direct SQL**: Available in the repository at `/sql/pgtap.sql` after extraction
