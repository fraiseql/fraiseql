# Getting Started with Fraisier + MySQL

**Perfect For**: Enterprise environments, existing MySQL infrastructure, cross-platform deployments

**Database**: MySQL 8.0+, MariaDB 10.5+

**Time to Production**: 15-20 minutes

---

## Quick Start

### Start MySQL

**Docker**:
```bash
docker run -d \
  --name fraisier-mysql \
  -e MYSQL_ROOT_PASSWORD=root_password \
  -e MYSQL_DATABASE=fraisier \
  -e MYSQL_USER=fraisier \
  -e MYSQL_PASSWORD=fraisier_password \
  -p 3306:3306 \
  mysql:8.0
```

**Wait for MySQL to be ready**:
```bash
docker exec fraisier-mysql mysqladmin -u fraisier -pfraisier_password ping
```

### Configure Fraisier

Create `.env`:
```bash
FRAISIER_DATABASE=mysql
FRAISIER_DB_PATH=mysql://fraisier:fraisier_password@localhost:3306/fraisier
```

### Initialize Database

```bash
fraisier db init
# ✓ Database initialized
```

---

## Configuration

### fraises.yaml

```yaml
database:
  type: mysql
  url: mysql://fraisier:fraisier_password@localhost:3306/fraisier
  pool_size: 20
  max_overflow: 10
  pool_recycle: 3600
  charset: utf8mb4

fraises:
  my_service:
    type: api
    git_provider: github
    git_repo: your-org/my-service
    git_branch: main

    environments:
      production:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: prod.example.com
              username: deploy
          service_name: my-service
```

### Connection String

**Standard**:
```
mysql://user:password@host:3306/database
```

**With SSL**:
```
mysql://user:password@host:3306/database?ssl_verify_cert=true
```

**Charset**:
```
mysql://user:password@host:3306/database?charset=utf8mb4
```

---

## Database Setup

### Create User and Database

```bash
mysql -u root -proot_password << 'EOF'
CREATE DATABASE fraisier CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
CREATE USER 'fraisier'@'%' IDENTIFIED BY 'fraisier_password';
GRANT ALL PRIVILEGES ON fraisier.* TO 'fraisier'@'%';
FLUSH PRIVILEGES;
EOF
```

### Initialize Schema

```bash
# Create tables and indexes
fraisier db init

# Verify
fraisier db status
# Database: MySQL 8.0.35
# Status: ✓ Healthy
```

---

## Performance Tuning

### MySQL Configuration

Add to `my.cnf`:
```ini
[mysqld]
# Connection pool
max_connections = 1000
max_allowed_packet = 64M

# InnoDB
innodb_buffer_pool_size = 4G
innodb_log_file_size = 512M
innodb_flush_log_at_trx_commit = 2  # Balance performance vs durability

# Query optimization
long_query_time = 2
log_queries_not_using_indexes = ON
```

### Create Indexes

```bash
mysql -u fraisier -pfraisier_password fraisier << 'EOF'
CREATE INDEX idx_deployment_created ON tb_deployment(created_at);
CREATE INDEX idx_deployment_status ON tb_deployment(status);
CREATE INDEX idx_deployment_env ON tb_deployment(environment);
EOF
```

### Connection Pooling

In `fraises.yaml`:
```yaml
database:
  url: mysql://fraisier:password@localhost/fraisier
  pool_size: 50
  max_overflow: 10
  pool_recycle: 3600
  pool_pre_ping: true  # Test connections
```

---

## Backup & Recovery

### Automated Backups

```bash
#!/bin/bash
# backup-fraisier.sh

BACKUP_DIR="/opt/fraisier/backups"
DATE=$(date +%Y-%m-%d-%H%M%S)

mysqldump -u fraisier -pfraisier_password \
  --single-transaction \
  --lock-tables=false \
  fraisier > "$BACKUP_DIR/fraisier_$DATE.sql"

gzip "$BACKUP_DIR/fraisier_$DATE.sql"
find "$BACKUP_DIR" -name "*.sql.gz" -mtime +30 -delete

echo "✓ Backup completed"
```

Schedule:
```bash
# Backup daily at 2 AM
0 2 * * * /opt/fraisier/backup-fraisier.sh
```

### Restore

```bash
mysql -u fraisier -pfraisier_password fraisier < backup.sql
```

---

## Replication Setup

### Primary Server

Add to `my.cnf`:
```ini
[mysqld]
server_id = 1
log_bin = mysql-bin
binlog_format = ROW
```

### Replica Server

```bash
# On replica:
CHANGE REPLICATION SOURCE TO
  SOURCE_HOST='primary.example.com',
  SOURCE_USER='replication',
  SOURCE_PASSWORD='password',
  SOURCE_LOG_FILE='mysql-bin.000001',
  SOURCE_LOG_POS=154;

START REPLICA;
SHOW REPLICA STATUS\G
```

---

## High Availability

### MySQL Group Replication

```bash
# Initialize group on all nodes
mysql -u root << 'EOF'
SET GLOBAL group_replication_bootstrap_group_recovery = ON;
START GROUP_REPLICATION;
EOF
```

### Connection Failover

```yaml
database:
  url: mysql://user:password@primary.com,replica.com/fraisier
  options:
    pool_pre_ping: true
    connect_timeout: 10
```

---

## Monitoring

### Check Database Size

```bash
mysql -u fraisier -pfraisier_password -e \
  "SELECT round(((data_length + index_length) / 1024 / 1024), 2) as size_mb
   FROM information_schema.TABLES
   WHERE table_schema = 'fraisier';"
```

### Slow Query Log

```bash
# Enable in my.cnf
slow_query_log = 1
slow_query_log_file = /var/log/mysql/slow.log
long_query_time = 2

# View slow queries
mysqldumpslow /var/log/mysql/slow.log | head -20
```

### Connection Status

```bash
mysql -u fraisier -pfraisier_password -e "SHOW PROCESSLIST;"
```

---

## Troubleshooting

### Connection Issues

```bash
# Test connection
mysql -u fraisier -pfraisier_password -h localhost fraisier -e "SELECT 1;"

# Check firewall
nc -zv localhost 3306
```

### Slow Performance

```bash
# Analyze table
ANALYZE TABLE tb_deployment;

# Rebuild indexes
REPAIR TABLE tb_deployment;
OPTIMIZE TABLE tb_deployment;
```

### Disk Space

```bash
# Check usage
SELECT SUM(data_length + index_length) / 1024 / 1024 / 1024 as size_gb
FROM information_schema.TABLES
WHERE table_schema = 'fraisier';

# Purge old binary logs
PURGE BINARY LOGS BEFORE DATE_SUB(NOW(), INTERVAL 7 DAY);
```

---

## Production Deployment

```bash
# 1. Test connection
mysql -u fraisier -pfraisier_password -h prod.db.example.com fraisier -e "SELECT 1;"

# 2. Initialize database
fraisier db init

# 3. Deploy
fraisier deploy my_service production --wait --timeout 1200

# 4. Verify
fraisier status my_service production
```

---

## Production Checklist

- [ ] MySQL 8.0+ installed and running
- [ ] Database and user created
- [ ] Connection verified
- [ ] Schema initialized
- [ ] Backup script scheduled
- [ ] Replication configured (optional)
- [ ] Connection pooling tuned
- [ ] SSL/TLS enabled
- [ ] Monitoring configured

---

## Reference

- [MySQL Documentation](https://dev.mysql.com/doc/)
- [CLI_REFERENCE.md](CLI_REFERENCE.md)
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
- [GETTING_STARTED_POSTGRES.md](GETTING_STARTED_POSTGRES.md)
