# Getting Started with Fraisier + PostgreSQL

**Perfect For**: Production deployments, high-traffic applications, multi-region deployments

**Database**: PostgreSQL 14+

**Time to Production**: 15-20 minutes

---

## Overview

PostgreSQL is ideal for:

- ✅ Production deployments (millions of requests/day)
- ✅ High-concurrency applications (1000+ concurrent users)
- ✅ Multi-region deployments with replication
- ✅ Applications requiring ACID transactions
- ✅ Complex reporting and analytics
- ✅ Automatic backup and recovery

---

## Installation

### Prerequisites

- Python 3.11+
- PostgreSQL 14+ (server or Docker)
- psycopg (Python adapter - installed automatically)

### Step 1: Start PostgreSQL

**Using Docker** (recommended for dev):

```bash
docker run -d \
  --name fraisier-postgres \
  -e POSTGRES_USER=fraisier \
  -e POSTGRES_PASSWORD=fraisier_password \
  -e POSTGRES_DB=fraisier \
  -p 5432:5432 \
  postgres:15-alpine
```

**Using Existing Server**:

```bash
# Create database and user
psql -U postgres
CREATE DATABASE fraisier;
CREATE USER fraisier WITH PASSWORD 'fraisier_password';
GRANT ALL PRIVILEGES ON DATABASE fraisier TO fraisier;
```

### Step 2: Install Fraisier

```bash
# Create virtual environment
python3 -m venv venv
source venv/bin/activate

# Install Fraisier
pip install -e ".[dev]"
```

### Step 3: Configure Connection

Create `.env`:

```bash
FRAISIER_DATABASE=postgresql
FRAISIER_DB_PATH=postgresql://fraisier:fraisier_password@localhost:5432/fraisier
```

Load environment:

```bash
set -a
source .env
set +a
```

---

## Configuration

### Create fraises.yaml

```yaml
database:
  type: postgresql
  url: postgresql://fraisier:fraisier_password@localhost:5432/fraisier
  pool_size: 20
  max_overflow: 10
  pool_recycle: 3600

fraises:
  my_api:
    type: api
    git_provider: github
    git_repo: your-org/my-api
    git_branch: main

    environments:
      production:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: prod-1.example.com
              username: deploy
          service_name: my-api
          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 10
```

### Connection String Options

**Standard Format**:

```
postgresql://user:password@host:port/database
```

**With SSL**:

```
postgresql://user:password@host:port/database?sslmode=require
```

**With Replica Failover**:

```
postgresql://user:password@primary:5432,replica:5432/database?target_session_attrs=primary
```

---

## Database Setup

### Initialize Database

```bash
fraisier db init

# Output:
# ✓ Connecting to PostgreSQL
# ✓ Creating schema
# ✓ Creating tables
# ✓ Creating indexes
# ✓ Database initialized successfully
```

### Verify Connection

```bash
# Check database health
fraisier db status

# Output:
# Database: PostgreSQL 15.1
# Host: localhost:5432
# Database: fraisier
# Connected: ✓
# Pool: 1/20
# Status: Healthy
```

### View Schema

```bash
# Using Fraisier
fraisier db show-schema

# Or using psql
psql -U fraisier -d fraisier -c "\dt"
```

---

## Production Deployment

### Pre-Deployment Checklist

```bash
# 1. Test connection
psql postgresql://fraisier:password@localhost:5432/fraisier -c "SELECT version();"

# 2. Check disk space
psql -U fraisier -d fraisier -c "SELECT pg_database_size('fraisier');"

# 3. Verify backup exists
ls -lh fraisier_backup_*.sql
```

### Configure Connection Pooling

For production (in `fraises.yaml`):

```yaml
database:
  type: postgresql
  url: postgresql://fraisier:password@prod.db.example.com/fraisier

  # Connection pooling
  pool_size: 50              # Connections to keep open
  max_overflow: 10           # Extra connections allowed temporarily
  pool_recycle: 3600         # Recycle connections every hour
  pool_pre_ping: true        # Test connection before use
  connect_timeout: 10
  statement_timeout: 30000   # 30 seconds
  idle_in_transaction_session_timeout: 60000  # 60 seconds
```

### Deploy to Production

```bash
# Dry-run first
fraisier deploy my_api production --dry-run

# Execute with verification
fraisier deploy my_api production \
  --strategy blue_green \
  --wait \
  --timeout 1200

# Verify
fraisier status my_api production
```

---

## Performance Tuning

### Create Indexes

```bash
# Fraisier creates these automatically, but you can add custom ones:
psql -U fraisier -d fraisier << 'EOF'
CREATE INDEX idx_deployment_created_at ON tb_deployment(created_at);
CREATE INDEX idx_deployment_status ON tb_deployment(status);
CREATE INDEX idx_deployment_environment ON tb_deployment(environment);
EOF
```

### Query Optimization

```bash
# Analyze query performance
psql -U fraisier -d fraisier << 'EOF'
EXPLAIN ANALYZE
SELECT * FROM v_deployment_history
WHERE environment = 'production'
ORDER BY created_at DESC
LIMIT 100;
EOF
```

### Monitor Connections

```bash
# Check active connections
psql -U fraisier -d fraisier -c \
  "SELECT datname, count(*) FROM pg_stat_activity GROUP BY datname;"

# Check long-running queries
psql -U fraisier -d fraisier -c \
  "SELECT pid, usename, query, query_start FROM pg_stat_activity WHERE state = 'active';"
```

---

## Backup & Recovery

### Automated Backups

```bash
#!/bin/bash
# backup-fraisier.sh - Daily backup script

BACKUP_DIR="/opt/fraisier/backups"
DB_NAME="fraisier"
BACKUP_DATE=$(date +%Y-%m-%d-%H%M%S)

# Create backup
pg_dump postgresql://fraisier:password@localhost/fraisier > \
  "$BACKUP_DIR/fraisier_$BACKUP_DATE.sql"

# Compress
gzip "$BACKUP_DIR/fraisier_$BACKUP_DATE.sql"

# Keep last 30 days
find "$BACKUP_DIR" -name "fraisier_*.sql.gz" -mtime +30 -delete

echo "Backup completed: fraisier_$BACKUP_DATE.sql.gz"
```

Schedule with cron:

```bash
# Backup daily at 2 AM
0 2 * * * /opt/fraisier/backup-fraisier.sh
```

### Point-in-Time Recovery

```bash
# Enable WAL archiving in postgresql.conf
wal_level = replica
max_wal_senders = 3
wal_keep_size = 1GB

# Then use pg_basebackup for continuous archiving
pg_basebackup -h localhost -U fraisier -D /mnt/backup/base -Xstream -P
```

### Restore from Backup

```bash
# Full restore
psql -U fraisier -d fraisier < fraisier_2024-01-22.sql

# Verify
fraisier history my_api --limit 5
```

---

## High Availability

### Replication Setup

**Primary Server**:

```bash
# postgresql.conf
wal_level = replica
max_wal_senders = 10
max_replication_slots = 10
```

**Replica Server**:

```bash
# Create from primary
pg_basebackup -h primary.db.example.com -U replication -D /var/lib/postgresql/data -Xstream -C -S replica1

# Start replica
systemctl start postgresql
```

### Connection Failover

In `fraises.yaml`:

```yaml
database:
  url: postgresql://fraisier:password@primary.db.example.com,replica.db.example.com/fraisier
  options:
    target_session_attrs: primary  # Connect to primary only
    keepalives_idle: 30           # Detect failed connections
```

---

## Monitoring

### PostgreSQL Metrics

```bash
# Database size
SELECT pg_database_size('fraisier') / 1024 / 1024 AS size_mb;

# Table sizes
SELECT relname, pg_size_pretty(pg_total_relation_size(relid))
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC;

# Index usage
SELECT schemaname, tablename, indexname, idx_scan, idx_tup_read, idx_tup_fetch
FROM pg_stat_user_indexes
ORDER BY idx_scan DESC;
```

### Fraisier Metrics

```bash
# View metrics endpoint
curl http://localhost:9090/metrics | grep fraisier_

# Deployment count by status
SELECT status, COUNT(*) FROM tb_deployment GROUP BY status;

# Average deployment duration
SELECT environment, AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) as avg_seconds
FROM tb_deployment
WHERE status = 'success'
GROUP BY environment;
```

---

## Troubleshooting

### Connection Issues

```bash
# Test connection
psql postgresql://fraisier:password@localhost/fraisier -c "SELECT 1;"

# Check firewall
nc -zv localhost 5432

# View PostgreSQL logs
tail -f /var/log/postgresql/postgresql.log
```

### Performance Issues

```bash
# Slow queries
SELECT query, calls, mean_time
FROM pg_stat_statements
ORDER BY mean_time DESC
LIMIT 10;

# Re-analyze
ANALYZE;
REINDEX DATABASE fraisier;
```

### Disk Space

```bash
# Check usage
SELECT pg_database_size('fraisier') / 1024 / 1024 / 1024 AS size_gb;

# Clear old WAL files
pg_archivecleanup /mnt/wal_archive $(pg_controldata /var/lib/postgresql/data | grep 'Last checkpoint location' | awk '{print $4}')
```

---

## Migration from SQLite

```bash
# 1. Export from SQLite
sqlite3 fraisier.db ".output fraisier_sqlite.sql" ".dump"

# 2. Create PostgreSQL database
createdb -U fraisier fraisier

# 3. Import (manual conversion may be needed for data types)
fraisier db migrate --from sqlite --to postgresql

# 4. Verify data
fraisier history my_api
```

---

## Production Checklist

- [ ] PostgreSQL 14+ running and accessible
- [ ] Connection string configured in .env
- [ ] Database initialized: `fraisier db init`
- [ ] Backup script scheduled
- [ ] Monitoring configured
- [ ] Connection pooling tuned
- [ ] Indexes created
- [ ] SSL/TLS enabled
- [ ] User permissions restricted
- [ ] Read replicas configured (optional)

---

## Next Steps

1. **Deploy Services**: `fraisier deploy my_api production`
2. **Monitor**: Set up Prometheus and Grafana
3. **Automate**: Add GitHub Actions for CD
4. **Scale**: Configure read replicas for reporting

---

## Reference

- [PostgreSQL Documentation](https://www.postgresql.org/docs/)
- [psycopg3 Documentation](https://www.psycopg.org/)
- [CLI_REFERENCE.md](CLI_REFERENCE.md)
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
