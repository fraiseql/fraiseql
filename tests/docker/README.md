# FraiseQL Integration Test Database Setup

This directory contains Docker Compose configuration for running FraiseQL integration tests against multiple databases.

## Quick Start

### 1. Start All Test Databases

```bash
# From project root
docker compose up -d

# Verify all services are healthy
docker compose ps
```

You should see:

```
NAME                 STATUS
fraiseql-postgres    Up (healthy)
fraiseql-mysql       Up (healthy)
fraiseql-sqlserver   Up (healthy)
```

### 2. Run Integration Tests

```bash
# Run all tests including database integration tests
cargo test --all-features

# Run only database adapter tests
cargo test -p fraiseql-core db:: -- --nocapture

# Run specific database adapter tests
cargo test -p fraiseql-core db::postgres:: --test-threads=1
cargo test -p fraiseql-core db::mysql:: --test-threads=1
cargo test -p fraiseql-core db::sqlserver:: --test-threads=1
```

### 3. Stop Databases

```bash
docker compose down

# Remove volumes (clean slate)
docker compose down -v
```

## Database Credentials

All databases use test credentials for development:

### PostgreSQL

- **Host**: localhost:5433 (Docker) → 5432 (container)
- **User**: test_user
- **Password**: test_password
- **Database**: fraiseql

**Connection String**:

```
postgresql://test_user:test_password@localhost:5433/fraiseql
```

**Note**: Port 5433 is used to avoid conflicts if you have PostgreSQL running on the host system on the default port 5432.

### MySQL

- **Host**: localhost:3306
- **User**: test_user
- **Password**: test_password
- **Database**: fraiseql
- **Root Password**: root_password

**Connection String**:

```
mysql://test_user:test_password@localhost:3306/fraiseql
```

### SQL Server

- **Host**: localhost:1433
- **User**: sa (System Administrator)
- **Password**: SqlServer@123
- **Database**: fraiseql

**Connection String**:

```
mssql://sa:SqlServer@123@localhost:1433/fraiseql
```

## Monitoring & Debugging

### View Logs

```bash
# All services
docker compose logs -f

# Specific service
docker compose logs -f postgres
docker compose logs -f mysql
docker compose logs -f sqlserver
```

### Connect to Databases Directly

**PostgreSQL**:

```bash
docker compose exec postgres psql -U test_user -d fraiseql
# Or from host if psql installed:
psql postgresql://test_user:test_password@localhost:5432/fraiseql
```

**MySQL**:

```bash
docker compose exec mysql mysql -u test_user -p test_password -D fraiseql
# Or from host if mysql client installed:
mysql -h localhost -u test_user -p test_password fraiseql
```

**SQL Server**:

```bash
docker compose exec sqlserver /opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P SqlServer@123
# Or use Azure Data Studio / DBeaver with connection string above
```

### Check Database Health

```bash
# PostgreSQL
docker compose exec postgres pg_isready -U test_user -d fraiseql

# MySQL
docker compose exec mysql mysqladmin ping -u test_user -ptest_password

# SQL Server
docker compose exec sqlserver /opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P SqlServer@123 -Q "SELECT 1"
```

## Disk Space

- **PostgreSQL**: ~500 MB (image) + 1-2 GB (test data)
- **MySQL**: ~300 MB (image) + 500 MB - 1 GB (test data)
- **SQL Server**: ~3-4 GB (image) + 1-2 GB (test data)

**Total**: ~8-12 GB for all three databases

Current system has 282 GB available, so this is not a concern.

## Profiles

The docker-compose.yml supports profiles for optional services:

### With Development Server

```bash
docker compose --profile with-server up -d
# Starts: all databases + FraiseQL server on http://localhost:8000
```

### With Redis (Caching)

```bash
docker compose --profile with-redis up -d
# Adds Redis on localhost:6379
```

### With NATS (Event Bus)

```bash
docker compose --profile with-nats up -d
# Adds NATS on localhost:4222
```

### All Together

```bash
docker compose --profile with-server --profile with-redis --profile with-nats up -d
```

## Test Database Configuration

The tests expect the following environment variables (already defaults):

```bash
# PostgreSQL
export POSTGRES_HOST=localhost
export POSTGRES_PORT=5432
export POSTGRES_DB=fraiseql
export POSTGRES_USER=test_user
export POSTGRES_PASSWORD=test_password

# MySQL
export MYSQL_HOST=localhost
export MYSQL_PORT=3306
export MYSQL_DB=fraiseql
export MYSQL_USER=test_user
export MYSQL_PASSWORD=test_password

# SQL Server
export SQLSERVER_HOST=localhost
export SQLSERVER_PORT=1433
export SQLSERVER_DB=fraiseql
export SQLSERVER_USER=sa
export SQLSERVER_PASSWORD=SqlServer@123
```

Or copy `.env.example` to `.env` and adjust:

```bash
cp .env.example .env
# Edit .env if needed
```

## Troubleshooting

### Container Won't Start

```bash
# Check logs
docker compose logs -f sqlserver

# SQL Server needs more resources
# Increase Docker memory to 4GB+ and CPU to 2+
docker stats  # Check current usage
```

### Connection Refused

```bash
# Verify service is healthy
docker compose ps

# Check health status
docker compose exec postgres pg_isready -U test_user -d fraiseql
docker compose exec mysql mysqladmin ping -u test_user -ptest_password

# Wait longer for SQL Server (slowest to start - 30-60 seconds)
sleep 60 && docker compose exec sqlserver /opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P SqlServer@123 -Q "SELECT 1"
```

### Tests Still Failing

1. Ensure all services show `healthy`:

   ```bash
   docker compose ps
   ```

2. Test connectivity directly:

   ```bash
   docker compose exec postgres psql -U test_user -d fraiseql -c "SELECT 1;"
   docker compose exec mysql mysql -u test_user -p test_password -e "SELECT 1;"
   docker compose exec sqlserver /opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P SqlServer@123 -Q "SELECT 1" -C
   ```

3. Check test-specific database creation:

   ```bash
   # Tests create test schemas dynamically
   # You can check if they're being created in logs
   docker compose logs postgres mysql sqlserver
   ```

4. Run with verbose output:

   ```bash
   cargo test --all-features -- --nocapture --test-threads=1
   ```

## Performance Tips

### Parallel Testing

By default, tests run with multiple threads. For database tests:

```bash
# Reduce threads to avoid connection pool exhaustion
cargo test --all-features --test-threads=2

# Single thread for debugging
cargo test --all-features --test-threads=1
```

### Isolate Test Suites

```bash
# PostgreSQL only
cargo test -p fraiseql-core db::postgres --test-threads=2

# MySQL only
cargo test -p fraiseql-core db::mysql --test-threads=2

# SQL Server only (slowest)
cargo test -p fraiseql-core db::sqlserver --test-threads=1
```

## Reference

**docker-compose.yml**: Full stack configuration in project root
**docker-compose ps**: Check container status
**docker compose logs**: View container logs
**docker compose down**: Stop containers

For more info: https://docs.docker.com/compose/
