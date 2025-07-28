# Disaster Recovery & Backup Strategies

Comprehensive guide for implementing disaster recovery and backup strategies for FraiseQL in production environments.

## Table of Contents
- [Overview](#overview)
- [Backup Strategies](#backup-strategies)
- [Recovery Procedures](#recovery-procedures)
- [High Availability Setup](#high-availability-setup)
- [Data Replication](#data-replication)
- [Automated Recovery](#automated-recovery)
- [Testing & Validation](#testing--validation)
- [Monitoring & Alerting](#monitoring--alerting)

## Overview

### Recovery Objectives

**Recovery Time Objective (RTO)**: Maximum acceptable downtime
- **Critical**: < 15 minutes
- **High**: < 1 hour
- **Medium**: < 4 hours
- **Low**: < 24 hours

**Recovery Point Objective (RPO)**: Maximum acceptable data loss
- **Critical**: < 1 minute
- **High**: < 15 minutes
- **Medium**: < 1 hour
- **Low**: < 24 hours

### Architecture for Disaster Recovery

```
┌─────────────────┐    ┌─────────────────┐
│   Primary DC    │    │   Secondary DC  │
│                 │    │                 │
│ ┌─────────────┐ │    │ ┌─────────────┐ │
│ │ FraiseQL    │ │────┼▶│ FraiseQL    │ │
│ │ (Active)    │ │    │ │ (Standby)   │ │
│ └─────────────┘ │    │ └─────────────┘ │
│                 │    │                 │
│ ┌─────────────┐ │    │ ┌─────────────┐ │
│ │ PostgreSQL  │ │────┼▶│ PostgreSQL  │ │
│ │ (Primary)   │ │    │ │ (Replica)   │ │
│ └─────────────┘ │    │ └─────────────┘ │
│                 │    │                 │
│ ┌─────────────┐ │    │ ┌─────────────┐ │
│ │ Storage     │ │────┼▶│ Storage     │ │
│ │ (Primary)   │ │    │ │ (Backup)    │ │
│ └─────────────┘ │    │ └─────────────┘ │
└─────────────────┘    └─────────────────┘
```

## Backup Strategies

### 1. Database Backup Configuration

#### PostgreSQL Continuous Archiving (WAL-E/WAL-G)

```yaml
# wal-g-config.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: wal-g-config
data:
  WALG_S3_PREFIX: "s3://fraiseql-backups/wal-g"
  AWS_REGION: "us-east-1"
  WALG_COMPRESSION_METHOD: "lz4"
  WALG_DELTA_MAX_STEPS: "5"
  WALG_RETAIN_COUNT: "7"
  POSTGRES_PASSWORD: "from-secret"
---
apiVersion: batch/v1
kind: CronJob
metadata:
  name: postgres-backup
spec:
  schedule: "0 2 * * *"  # Daily at 2 AM
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: wal-g-backup
            image: wal-g/wal-g:latest
            command:
            - /bin/sh
            - -c
            - |
              export PGPASSWORD=$POSTGRES_PASSWORD
              wal-g backup-push
              wal-g delete retain FULL 7
              wal-g delete retain FIND_FULL 30
            envFrom:
            - configMapRef:
                name: wal-g-config
            - secretRef:
                name: postgres-credentials
            volumeMounts:
            - name: postgres-data
              mountPath: /var/lib/postgresql/data
          volumes:
          - name: postgres-data
            persistentVolumeClaim:
              claimName: postgres-data-pvc
          restartPolicy: OnFailure
```

#### Point-in-Time Recovery Setup

```sql
-- PostgreSQL configuration for PITR
-- In postgresql.conf:
wal_level = replica
archive_mode = on
archive_command = 'wal-g wal-push %p'
max_wal_senders = 3
wal_keep_size = 1GB
checkpoint_completion_target = 0.9

-- Create replication user
CREATE USER replicator REPLICATION LOGIN ENCRYPTED PASSWORD 'strong_password';

-- Grant necessary permissions
GRANT CONNECT ON DATABASE fraiseql_production TO replicator;
GRANT USAGE ON SCHEMA public TO replicator;
```

### 2. Application State Backup

#### Configuration and Schema Backup

```bash
#!/bin/bash
# backup-fraiseql.sh - Complete FraiseQL backup script

set -euo pipefail

BACKUP_DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="/backups/fraiseql_${BACKUP_DATE}"
S3_BUCKET="s3://fraiseql-backups"

# Create backup directory
mkdir -p "${BACKUP_DIR}"

echo "Starting FraiseQL backup at $(date)"

# 1. Backup database schema and data
echo "Backing up database schema..."
pg_dump \
  --host="${POSTGRES_HOST}" \
  --port="${POSTGRES_PORT}" \
  --username="${POSTGRES_USER}" \
  --dbname="${POSTGRES_DB}" \
  --schema-only \
  --file="${BACKUP_DIR}/schema.sql"

echo "Backing up database data..."
pg_dump \
  --host="${POSTGRES_HOST}" \
  --port="${POSTGRES_PORT}" \
  --username="${POSTGRES_USER}" \
  --dbname="${POSTGRES_DB}" \
  --data-only \
  --format=custom \
  --file="${BACKUP_DIR}/data.pgdump"

# 2. Backup application configuration
echo "Backing up application configuration..."
kubectl get configmap fraiseql-config -o yaml > "${BACKUP_DIR}/configmap.yaml"
kubectl get secret fraiseql-secrets -o yaml > "${BACKUP_DIR}/secrets.yaml"

# 3. Backup Kubernetes manifests
echo "Backing up Kubernetes manifests..."
kubectl get deployment fraiseql -o yaml > "${BACKUP_DIR}/deployment.yaml"
kubectl get service fraiseql -o yaml > "${BACKUP_DIR}/service.yaml"
kubectl get ingress fraiseql -o yaml > "${BACKUP_DIR}/ingress.yaml"

# 4. Backup monitoring configuration
echo "Backing up monitoring configuration..."
kubectl get prometheusrule fraiseql-alerts -o yaml > "${BACKUP_DIR}/prometheus-rules.yaml"
kubectl get servicemonitor fraiseql -o yaml > "${BACKUP_DIR}/service-monitor.yaml"

# 5. Create backup metadata
cat > "${BACKUP_DIR}/metadata.json" << EOF
{
  "backup_date": "${BACKUP_DATE}",
  "database_version": "$(psql --host=${POSTGRES_HOST} --port=${POSTGRES_PORT} --username=${POSTGRES_USER} --dbname=${POSTGRES_DB} -t -c 'SELECT version();' | xargs)",
  "fraiseql_version": "$(kubectl get deployment fraiseql -o jsonpath='{.spec.template.spec.containers[0].image}')",
  "kubernetes_version": "$(kubectl version --short | grep Server | cut -d' ' -f3)",
  "backup_size": "$(du -sh ${BACKUP_DIR} | cut -f1)"
}
EOF

# 6. Compress and upload to S3
echo "Compressing backup..."
tar -czf "${BACKUP_DIR}.tar.gz" -C "$(dirname ${BACKUP_DIR})" "$(basename ${BACKUP_DIR})"

echo "Uploading to S3..."
aws s3 cp "${BACKUP_DIR}.tar.gz" "${S3_BUCKET}/fraiseql/${BACKUP_DATE}.tar.gz"

# 7. Cleanup local files (keep last 3 days)
find /backups -name "fraiseql_*" -type d -mtime +3 -exec rm -rf {} +
find /backups -name "fraiseql_*.tar.gz" -mtime +3 -delete

echo "Backup completed successfully at $(date)"

# 8. Verify backup integrity
echo "Verifying backup integrity..."
if aws s3 ls "${S3_BUCKET}/fraiseql/${BACKUP_DATE}.tar.gz" > /dev/null; then
    echo "✅ Backup verified in S3"
else
    echo "❌ Backup verification failed"
    exit 1
fi
```

### 3. Automated Backup Schedule

```yaml
# backup-cronjob.yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: fraiseql-backup
spec:
  schedule: "0 3 * * *"  # Daily at 3 AM
  concurrencyPolicy: Forbid
  successfulJobsHistoryLimit: 3
  failedJobsHistoryLimit: 1
  jobTemplate:
    spec:
      template:
        spec:
          serviceAccountName: backup-service-account
          containers:
          - name: backup
            image: fraiseql/backup-tools:latest
            command: ["/scripts/backup-fraiseql.sh"]
            env:
            - name: POSTGRES_HOST
              value: "postgres-primary"
            - name: POSTGRES_PORT
              value: "5432"
            - name: POSTGRES_USER
              valueFrom:
                secretKeyRef:
                  name: postgres-credentials
                  key: username
            - name: POSTGRES_PASSWORD
              valueFrom:
                secretKeyRef:
                  name: postgres-credentials
                  key: password
            - name: POSTGRES_DB
              value: "fraiseql_production"
            - name: AWS_DEFAULT_REGION
              value: "us-east-1"
            volumeMounts:
            - name: backup-scripts
              mountPath: /scripts
            - name: backup-storage
              mountPath: /backups
          volumes:
          - name: backup-scripts
            configMap:
              name: backup-scripts
              defaultMode: 0755
          - name: backup-storage
            persistentVolumeClaim:
              claimName: backup-storage-pvc
          restartPolicy: OnFailure
```

## Recovery Procedures

### 1. Database Point-in-Time Recovery

```bash
#!/bin/bash
# restore-database.sh - Database restore from backup

set -euo pipefail

RESTORE_DATE="${1:-latest}"
BACKUP_LOCATION="${2:-s3://fraiseql-backups/wal-g}"

echo "Starting database restore to ${RESTORE_DATE}"

# 1. Stop FraiseQL application
kubectl scale deployment fraiseql --replicas=0

# 2. Stop PostgreSQL
kubectl scale statefulset postgres --replicas=0

# 3. Clear existing data directory
kubectl exec postgres-0 -- rm -rf /var/lib/postgresql/data/*

# 4. Restore from backup
if [[ "${RESTORE_DATE}" == "latest" ]]; then
    # Restore latest backup
    kubectl exec postgres-0 -- wal-g backup-fetch /var/lib/postgresql/data LATEST
else
    # Restore specific backup
    kubectl exec postgres-0 -- wal-g backup-fetch /var/lib/postgresql/data "${RESTORE_DATE}"
fi

# 5. Configure recovery
cat > recovery.conf << EOF
restore_command = 'wal-g wal-fetch %f %p'
recovery_target_time = '${RESTORE_DATE}'
recovery_target_action = 'promote'
EOF

kubectl cp recovery.conf postgres-0:/var/lib/postgresql/data/

# 6. Start PostgreSQL in recovery mode
kubectl scale statefulset postgres --replicas=1

# 7. Wait for recovery completion
echo "Waiting for PostgreSQL recovery..."
until kubectl exec postgres-0 -- pg_isready; do
    sleep 5
done

# 8. Verify database state
kubectl exec postgres-0 -- psql -d fraiseql_production -c "SELECT now(), count(*) FROM users;"

# 9. Restart FraiseQL application
kubectl scale deployment fraiseql --replicas=3

echo "Database restore completed successfully"
```

### 2. Application Recovery

```bash
#!/bin/bash
# restore-application.sh - Complete application restore

set -euo pipefail

BACKUP_DATE="${1:?Backup date required (YYYYMMDD_HHMMSS)}"
BACKUP_LOCATION="s3://fraiseql-backups/fraiseql/${BACKUP_DATE}.tar.gz"

echo "Starting application restore from ${BACKUP_DATE}"

# 1. Download and extract backup
aws s3 cp "${BACKUP_LOCATION}" "/tmp/${BACKUP_DATE}.tar.gz"
tar -xzf "/tmp/${BACKUP_DATE}.tar.gz" -C /tmp/

BACKUP_DIR="/tmp/fraiseql_${BACKUP_DATE}"

# 2. Restore database
echo "Restoring database..."
kubectl exec postgres-0 -- psql -d postgres -c "DROP DATABASE IF EXISTS fraiseql_production;"
kubectl exec postgres-0 -- psql -d postgres -c "CREATE DATABASE fraiseql_production;"

# Restore schema
kubectl cp "${BACKUP_DIR}/schema.sql" postgres-0:/tmp/
kubectl exec postgres-0 -- psql -d fraiseql_production -f /tmp/schema.sql

# Restore data
kubectl cp "${BACKUP_DIR}/data.pgdump" postgres-0:/tmp/
kubectl exec postgres-0 -- pg_restore -d fraiseql_production /tmp/data.pgdump

# 3. Restore Kubernetes resources
echo "Restoring Kubernetes resources..."
kubectl delete deployment fraiseql --ignore-not-found=true
kubectl delete service fraiseql --ignore-not-found=true
kubectl delete ingress fraiseql --ignore-not-found=true

kubectl apply -f "${BACKUP_DIR}/deployment.yaml"
kubectl apply -f "${BACKUP_DIR}/service.yaml"
kubectl apply -f "${BACKUP_DIR}/ingress.yaml"

# 4. Restore configuration
echo "Restoring configuration..."
kubectl delete configmap fraiseql-config --ignore-not-found=true
kubectl delete secret fraiseql-secrets --ignore-not-found=true

kubectl apply -f "${BACKUP_DIR}/configmap.yaml"
kubectl apply -f "${BACKUP_DIR}/secrets.yaml"

# 5. Restore monitoring
echo "Restoring monitoring configuration..."
kubectl apply -f "${BACKUP_DIR}/prometheus-rules.yaml"
kubectl apply -f "${BACKUP_DIR}/service-monitor.yaml"

# 6. Wait for application to be ready
echo "Waiting for application to be ready..."
kubectl wait --for=condition=available --timeout=300s deployment/fraiseql

# 7. Verify application health
echo "Verifying application health..."
HEALTH_URL="https://api.company.com/health"
if curl -f "${HEALTH_URL}" > /dev/null 2>&1; then
    echo "✅ Application health check passed"
else
    echo "❌ Application health check failed"
    exit 1
fi

# 8. Run post-restore validation
echo "Running post-restore validation..."
kubectl exec deployment/fraiseql -- python -m fraiseql.tools.validate_data

echo "Application restore completed successfully"

# 9. Cleanup
rm -rf "${BACKUP_DIR}" "/tmp/${BACKUP_DATE}.tar.gz"
```

## High Availability Setup

### 1. Multi-Region Deployment

```yaml
# primary-region.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: fraiseql-primary
  labels:
    region: us-east-1
    role: primary
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
  namespace: fraiseql-primary
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
      role: primary
  template:
    metadata:
      labels:
        app: fraiseql
        role: primary
    spec:
      containers:
      - name: fraiseql
        image: fraiseql:latest
        env:
        - name: DATABASE_URL
          value: "postgresql://user:pass@postgres-primary:5432/fraiseql"
        - name: REDIS_URL
          value: "redis://redis-primary:6379"
        - name: REGION
          value: "us-east-1"
        - name: ROLE
          value: "primary"
        ports:
        - containerPort: 8000
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
---
# secondary-region.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: fraiseql-secondary
  labels:
    region: us-west-2
    role: secondary
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
  namespace: fraiseql-secondary
spec:
  replicas: 2
  selector:
    matchLabels:
      app: fraiseql
      role: secondary
  template:
    metadata:
      labels:
        app: fraiseql
        role: secondary
    spec:
      containers:
      - name: fraiseql
        image: fraiseql:latest
        env:
        - name: DATABASE_URL
          value: "postgresql://user:pass@postgres-replica:5432/fraiseql"
        - name: REDIS_URL
          value: "redis://redis-replica:6379"
        - name: REGION
          value: "us-west-2"
        - name: ROLE
          value: "secondary"
        - name: READ_ONLY
          value: "true"
        ports:
        - containerPort: 8000
```

### 2. Database Replication Setup

```yaml
# postgres-primary.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres-primary
spec:
  serviceName: postgres-primary
  replicas: 1
  selector:
    matchLabels:
      app: postgres
      role: primary
  template:
    metadata:
      labels:
        app: postgres
        role: primary
    spec:
      containers:
      - name: postgres
        image: postgres:15
        env:
        - name: POSTGRES_DB
          value: fraiseql_production
        - name: POSTGRES_USER
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: username
        - name: POSTGRES_PASSWORD
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: password
        - name: POSTGRES_REPLICATION_USER
          value: replicator
        - name: POSTGRES_REPLICATION_PASSWORD
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: replication_password
        volumeMounts:
        - name: postgres-config
          mountPath: /etc/postgresql/postgresql.conf
          subPath: postgresql.conf
        - name: postgres-hba
          mountPath: /etc/postgresql/pg_hba.conf
          subPath: pg_hba.conf
        - name: postgres-data
          mountPath: /var/lib/postgresql/data
        command:
        - postgres
        - -c
        - config_file=/etc/postgresql/postgresql.conf
        - -c
        - hba_file=/etc/postgresql/pg_hba.conf
      volumes:
      - name: postgres-config
        configMap:
          name: postgres-config
      - name: postgres-hba
        configMap:
          name: postgres-hba
  volumeClaimTemplates:
  - metadata:
      name: postgres-data
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 100Gi
---
# postgres-replica.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres-replica
spec:
  serviceName: postgres-replica
  replicas: 1
  selector:
    matchLabels:
      app: postgres
      role: replica
  template:
    metadata:
      labels:
        app: postgres
        role: replica
    spec:
      initContainers:
      - name: setup-replica
        image: postgres:15
        command:
        - bash
        - -c
        - |
          if [ ! -f /var/lib/postgresql/data/PG_VERSION ]; then
            echo "Setting up replica from primary..."
            pg_basebackup -h postgres-primary -D /var/lib/postgresql/data -U replicator -W -v -P
            cat > /var/lib/postgresql/data/postgresql.auto.conf << EOF
          primary_conninfo = 'host=postgres-primary port=5432 user=replicator'
          promote_trigger_file = '/tmp/promote_trigger'
          EOF
          fi
        env:
        - name: PGPASSWORD
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: replication_password
        volumeMounts:
        - name: postgres-data
          mountPath: /var/lib/postgresql/data
      containers:
      - name: postgres
        image: postgres:15
        env:
        - name: POSTGRES_DB
          value: fraiseql_production
        - name: POSTGRES_USER
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: username
        - name: POSTGRES_PASSWORD
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: password
        volumeMounts:
        - name: postgres-data
          mountPath: /var/lib/postgresql/data
        command:
        - postgres
        - -c
        - hot_standby=on
  volumeClaimTemplates:
  - metadata:
      name: postgres-data
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 100Gi
```

## Data Replication

### 1. Cross-Region Replication

```bash
#!/bin/bash
# setup-cross-region-replication.sh

set -euo pipefail

PRIMARY_REGION="us-east-1"
SECONDARY_REGION="us-west-2"
REPLICATION_USER="replicator"

echo "Setting up cross-region replication from ${PRIMARY_REGION} to ${SECONDARY_REGION}"

# 1. Create replication slot on primary
kubectl exec -n fraiseql-primary postgres-primary-0 -- psql -d fraiseql_production -c "
    SELECT pg_create_physical_replication_slot('replica_${SECONDARY_REGION}');
"

# 2. Configure replica in secondary region
kubectl exec -n fraiseql-secondary postgres-replica-0 -- bash -c "
    cat > /var/lib/postgresql/data/postgresql.auto.conf << EOF
primary_conninfo = 'host=postgres-primary.fraiseql-primary.svc.cluster.local port=5432 user=${REPLICATION_USER} application_name=replica_${SECONDARY_REGION}'
primary_slot_name = 'replica_${SECONDARY_REGION}'
hot_standby = on
max_standby_streaming_delay = 30s
wal_receiver_status_interval = 10s
hot_standby_feedback = on
EOF
"

# 3. Restart replica to apply configuration
kubectl rollout restart statefulset/postgres-replica -n fraiseql-secondary

# 4. Verify replication status
echo "Verifying replication status..."
kubectl exec -n fraiseql-primary postgres-primary-0 -- psql -d fraiseql_production -c "
    SELECT client_addr, state, sent_lsn, write_lsn, flush_lsn, replay_lsn,
           write_lag, flush_lag, replay_lag
    FROM pg_stat_replication;
"

echo "Cross-region replication setup completed"
```

### 2. Real-time Data Synchronization

```python
# sync_monitor.py - Monitor replication lag and data consistency
import asyncio
import asyncpg
import logging
from datetime import datetime, timedelta
from prometheus_client import Gauge, Counter

# Metrics
replication_lag_gauge = Gauge('postgres_replication_lag_seconds', 'Replication lag in seconds')
data_consistency_counter = Counter('postgres_data_consistency_checks_total', 'Data consistency checks', ['status'])

async def monitor_replication_lag():
    """Monitor and report replication lag."""
    primary_conn = await asyncpg.connect("postgresql://user:pass@postgres-primary:5432/fraiseql")
    replica_conn = await asyncpg.connect("postgresql://user:pass@postgres-replica:5432/fraiseql")

    try:
        while True:
            # Get current LSN from primary
            primary_lsn = await primary_conn.fetchval("SELECT pg_current_wal_lsn()")

            # Get replay LSN from replica
            replica_lsn = await replica_conn.fetchval("SELECT pg_last_wal_replay_lsn()")

            # Calculate lag in bytes and seconds
            lag_bytes = await primary_conn.fetchval(
                "SELECT $1::pg_lsn - $2::pg_lsn", primary_lsn, replica_lsn
            )

            # Estimate lag in seconds (approximate)
            lag_seconds = lag_bytes / (1024 * 1024)  # Rough estimate

            replication_lag_gauge.set(lag_seconds)

            logging.info(f"Replication lag: {lag_seconds:.2f} seconds ({lag_bytes} bytes)")

            # Alert if lag is too high
            if lag_seconds > 60:  # 1 minute threshold
                logging.warning(f"High replication lag detected: {lag_seconds:.2f} seconds")

            await asyncio.sleep(10)

    finally:
        await primary_conn.close()
        await replica_conn.close()

async def verify_data_consistency():
    """Verify data consistency between primary and replica."""
    primary_conn = await asyncpg.connect("postgresql://user:pass@postgres-primary:5432/fraiseql")
    replica_conn = await asyncpg.connect("postgresql://user:pass@postgres-replica:5432/fraiseql")

    try:
        # Check critical tables for consistency
        tables = ['users', 'posts', 'sessions']

        for table in tables:
            # Count records
            primary_count = await primary_conn.fetchval(f"SELECT count(*) FROM {table}")
            replica_count = await replica_conn.fetchval(f"SELECT count(*) FROM {table}")

            if primary_count == replica_count:
                data_consistency_counter.labels(status='consistent').inc()
                logging.info(f"Table {table}: consistent ({primary_count} records)")
            else:
                data_consistency_counter.labels(status='inconsistent').inc()
                logging.error(f"Table {table}: inconsistent (primary: {primary_count}, replica: {replica_count})")

        # Check recent data
        cutoff_time = datetime.now() - timedelta(minutes=5)
        recent_primary = await primary_conn.fetchval(
            "SELECT count(*) FROM users WHERE created_at > $1", cutoff_time
        )
        recent_replica = await replica_conn.fetchval(
            "SELECT count(*) FROM users WHERE created_at > $1", cutoff_time
        )

        logging.info(f"Recent data (5min): primary={recent_primary}, replica={recent_replica}")

    finally:
        await primary_conn.close()
        await replica_conn.close()

if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)

    # Run monitoring tasks
    loop = asyncio.get_event_loop()
    tasks = [
        monitor_replication_lag(),
        verify_data_consistency()
    ]

    try:
        loop.run_until_complete(asyncio.gather(*tasks))
    except KeyboardInterrupt:
        logging.info("Monitoring stopped")
```

## Automated Recovery

### 1. Failover Automation

```python
# failover_controller.py - Automated failover controller
import asyncio
import logging
import subprocess
from datetime import datetime
from kubernetes import client, config

class FailoverController:
    """Automated failover controller for FraiseQL."""

    def __init__(self):
        config.load_incluster_config()  # Running inside cluster
        self.k8s_apps = client.AppsV1Api()
        self.k8s_core = client.CoreV1Api()

    async def check_primary_health(self):
        """Check if primary database is healthy."""
        try:
            # Check if primary pod is running
            pods = self.k8s_core.list_namespaced_pod(
                namespace="fraiseql-primary",
                label_selector="app=postgres,role=primary"
            )

            if not pods.items:
                return False

            pod = pods.items[0]
            if pod.status.phase != "Running":
                return False

            # Check database connectivity
            result = subprocess.run([
                "kubectl", "exec", "-n", "fraiseql-primary",
                pod.metadata.name, "--",
                "pg_isready", "-h", "localhost", "-p", "5432"
            ], capture_output=True, text=True, timeout=10)

            return result.returncode == 0

        except Exception as e:
            logging.error(f"Primary health check failed: {e}")
            return False

    async def promote_replica(self):
        """Promote replica to primary."""
        logging.info("Starting replica promotion...")

        try:
            # 1. Promote PostgreSQL replica
            pods = self.k8s_core.list_namespaced_pod(
                namespace="fraiseql-secondary",
                label_selector="app=postgres,role=replica"
            )

            if not pods.items:
                raise Exception("No replica pod found")

            replica_pod = pods.items[0].metadata.name

            # Create promote trigger file
            subprocess.run([
                "kubectl", "exec", "-n", "fraiseql-secondary",
                replica_pod, "--",
                "touch", "/tmp/promote_trigger"
            ], check=True)

            # Wait for promotion to complete
            await asyncio.sleep(10)

            # 2. Update FraiseQL application to use promoted database
            deployment = self.k8s_apps.read_namespaced_deployment(
                name="fraiseql",
                namespace="fraiseql-secondary"
            )

            # Update environment variables
            for container in deployment.spec.template.spec.containers:
                if container.name == "fraiseql":
                    for env in container.env:
                        if env.name == "READ_ONLY":
                            env.value = "false"
                        elif env.name == "ROLE":
                            env.value = "primary"

            # Apply the update
            self.k8s_apps.patch_namespaced_deployment(
                name="fraiseql",
                namespace="fraiseql-secondary",
                body=deployment
            )

            # 3. Scale up secondary region
            deployment.spec.replicas = 3
            self.k8s_apps.patch_namespaced_deployment(
                name="fraiseql",
                namespace="fraiseql-secondary",
                body=deployment
            )

            # 4. Update DNS/Load Balancer to point to secondary region
            # This would depend on your specific setup (AWS Route 53, etc.)
            await self.update_dns_failover()

            logging.info("Replica promotion completed successfully")
            return True

        except Exception as e:
            logging.error(f"Replica promotion failed: {e}")
            return False

    async def update_dns_failover(self):
        """Update DNS to point to secondary region."""
        # Implementation depends on your DNS provider
        # Example for AWS Route 53:

        import boto3

        route53 = boto3.client('route53')

        # Update A record to point to secondary region load balancer
        response = route53.change_resource_record_sets(
            HostedZoneId='Z1234567890',
            ChangeBatch={
                'Changes': [{
                    'Action': 'UPSERT',
                    'ResourceRecordSet': {
                        'Name': 'api.company.com',
                        'Type': 'A',
                        'TTL': 60,
                        'ResourceRecords': [
                            {'Value': '203.0.113.2'}  # Secondary region IP
                        ]
                    }
                }]
            }
        )

        logging.info(f"DNS failover initiated: {response['ChangeInfo']['Id']}")

    async def run_failover_monitoring(self):
        """Main failover monitoring loop."""
        consecutive_failures = 0
        failure_threshold = 3

        while True:
            try:
                if await self.check_primary_health():
                    consecutive_failures = 0
                    logging.info("Primary database is healthy")
                else:
                    consecutive_failures += 1
                    logging.warning(f"Primary health check failed ({consecutive_failures}/{failure_threshold})")

                    if consecutive_failures >= failure_threshold:
                        logging.critical("Primary database is down, initiating failover...")

                        success = await self.promote_replica()
                        if success:
                            logging.info("Failover completed successfully")
                            # Send notification
                            await self.send_failover_notification()
                            break
                        else:
                            logging.error("Failover failed, retrying in 30 seconds...")
                            await asyncio.sleep(30)

                await asyncio.sleep(30)  # Check every 30 seconds

            except Exception as e:
                logging.error(f"Failover monitoring error: {e}")
                await asyncio.sleep(60)

    async def send_failover_notification(self):
        """Send notification about failover event."""
        # Implementation for your notification system (Slack, PagerDuty, etc.)
        message = f"""
🚨 FAILOVER ALERT 🚨

FraiseQL has automatically failed over to the secondary region due to primary database failure.

Time: {datetime.now()}
Event: Primary → Secondary failover
Status: Completed
Action Required: Investigate primary region issues

Monitoring Dashboard: https://grafana.company.com/d/fraiseql-disaster-recovery
"""

        # Send to Slack, PagerDuty, etc.
        logging.info("Failover notification sent")

if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)

    controller = FailoverController()
    asyncio.run(controller.run_failover_monitoring())
```

## Testing & Validation

### 1. Disaster Recovery Testing

```bash
#!/bin/bash
# dr-test.sh - Disaster recovery testing script

set -euo pipefail

TEST_DATE=$(date +%Y%m%d_%H%M%S)
TEST_LOG="/tmp/dr_test_${TEST_DATE}.log"

echo "Starting DR test at $(date)" | tee -a "${TEST_LOG}"

# Test 1: Backup and Restore
echo "Test 1: Backup and Restore" | tee -a "${TEST_LOG}"
./backup-fraiseql.sh 2>&1 | tee -a "${TEST_LOG}"

BACKUP_DATE=$(date +%Y%m%d_%H%M%S)
./restore-application.sh "${BACKUP_DATE}" 2>&1 | tee -a "${TEST_LOG}"

if [[ $? -eq 0 ]]; then
    echo "✅ Backup and Restore: PASS" | tee -a "${TEST_LOG}"
else
    echo "❌ Backup and Restore: FAIL" | tee -a "${TEST_LOG}"
fi

# Test 2: Database Failover
echo "Test 2: Database Failover" | tee -a "${TEST_LOG}"

# Simulate primary database failure
kubectl patch statefulset postgres-primary -p '{"spec":{"replicas":0}}'

# Wait for failover to trigger
sleep 120

# Check if application is still responding
if curl -f "https://api.company.com/health" > /dev/null 2>&1; then
    echo "✅ Database Failover: PASS" | tee -a "${TEST_LOG}"
else
    echo "❌ Database Failover: FAIL" | tee -a "${TEST_LOG}"
fi

# Restore primary
kubectl patch statefulset postgres-primary -p '{"spec":{"replicas":1}}'

# Test 3: Cross-Region Failover
echo "Test 3: Cross-Region Failover" | tee -a "${TEST_LOG}"

# Scale down primary region
kubectl scale deployment fraiseql --replicas=0 -n fraiseql-primary

# Update DNS to point to secondary
# (Implementation specific to your DNS provider)

# Wait for DNS propagation
sleep 60

# Test application availability
if curl -f "https://api.company.com/health" > /dev/null 2>&1; then
    echo "✅ Cross-Region Failover: PASS" | tee -a "${TEST_LOG}"
else
    echo "❌ Cross-Region Failover: FAIL" | tee -a "${TEST_LOG}"
fi

# Restore primary region
kubectl scale deployment fraiseql --replicas=3 -n fraiseql-primary

echo "DR test completed at $(date)" | tee -a "${TEST_LOG}"
echo "Test log: ${TEST_LOG}"
```

### 2. Recovery Time Testing

```python
# rto_test.py - Recovery Time Objective testing
import asyncio
import aiohttp
import time
from datetime import datetime, timedelta

async def test_recovery_times():
    """Test various recovery scenarios and measure RTO."""

    scenarios = [
        {
            "name": "Application Pod Restart",
            "target_rto": 60,  # 1 minute
            "test_func": test_pod_restart
        },
        {
            "name": "Database Failover",
            "target_rto": 900,  # 15 minutes
            "test_func": test_db_failover
        },
        {
            "name": "Cross-Region Failover",
            "target_rto": 1800,  # 30 minutes
            "test_func": test_region_failover
        }
    ]

    results = {}

    for scenario in scenarios:
        print(f"\nTesting {scenario['name']}...")

        start_time = time.time()
        success = await scenario["test_func"]()
        end_time = time.time()

        rto = end_time - start_time
        target_rto = scenario["target_rto"]

        results[scenario["name"]] = {
            "rto": rto,
            "target_rto": target_rto,
            "success": success,
            "within_target": rto <= target_rto
        }

        status = "✅ PASS" if success and rto <= target_rto else "❌ FAIL"
        print(f"{scenario['name']}: {rto:.1f}s (target: {target_rto}s) {status}")

    return results

async def test_pod_restart():
    """Test application pod restart recovery time."""
    # Implementation for pod restart test
    pass

async def test_db_failover():
    """Test database failover recovery time."""
    # Implementation for database failover test
    pass

async def test_region_failover():
    """Test cross-region failover recovery time."""
    # Implementation for region failover test
    pass

if __name__ == "__main__":
    results = asyncio.run(test_recovery_times())

    # Generate report
    print("\n" + "="*50)
    print("RECOVERY TIME OBJECTIVE TEST REPORT")
    print("="*50)

    for test_name, result in results.items():
        print(f"\n{test_name}:")
        print(f"  Measured RTO: {result['rto']:.1f} seconds")
        print(f"  Target RTO: {result['target_rto']} seconds")
        print(f"  Success: {'Yes' if result['success'] else 'No'}")
        print(f"  Within Target: {'Yes' if result['within_target'] else 'No'}")
```

## Monitoring & Alerting

### 1. DR-Specific Alerts

```yaml
# dr-alerts.yml - Disaster recovery specific alerts
groups:
- name: disaster-recovery
  rules:
  - alert: BackupFailed
    expr: |
      time() - fraiseql_last_successful_backup_timestamp > 86400  # 24 hours
    for: 0m
    labels:
      severity: critical
      team: sre
      component: backup
    annotations:
      summary: "Backup has not completed successfully in 24 hours"
      description: "Last successful backup was {{ $value | humanizeDuration }} ago"
      action: "Check backup job logs and storage accessibility"

  - alert: ReplicationLagHigh
    expr: |
      postgres_replication_lag_seconds > 300  # 5 minutes
    for: 2m
    labels:
      severity: warning
      team: sre
      component: replication
    annotations:
      summary: "PostgreSQL replication lag is high"
      description: "Replication lag is {{ $value }} seconds"
      action: "Check network connectivity and replica performance"

  - alert: ReplicationBroken
    expr: |
      postgres_replication_lag_seconds > 3600 OR
      up{job="postgres-replica"} == 0
    for: 1m
    labels:
      severity: critical
      team: sre
      component: replication
    annotations:
      summary: "PostgreSQL replication is broken"
      description: "Replication lag exceeds 1 hour or replica is down"
      action: "Investigate replication status and restore if necessary"

  - alert: CrossRegionLatencyHigh
    expr: |
      avg(fraiseql_cross_region_latency_seconds) > 0.5
    for: 5m
    labels:
      severity: warning
      team: sre
      component: network
    annotations:
      summary: "Cross-region latency is high"
      description: "Average cross-region latency is {{ $value }}s"
      action: "Check network connectivity between regions"

  - alert: FailoverRequired
    expr: |
      up{job="fraiseql", region="primary"} == 0 AND
      postgres_primary_available == 0
    for: 2m
    labels:
      severity: critical
      team: sre
      component: failover
    annotations:
      summary: "Primary region is down, failover required"
      description: "Primary region has been unavailable for 2 minutes"
      action: "Initiate manual failover to secondary region"
```

### 2. DR Dashboard

```json
{
  "title": "FraiseQL Disaster Recovery Dashboard",
  "panels": [
    {
      "title": "Backup Status",
      "type": "stat",
      "targets": [{
        "expr": "(time() - fraiseql_last_successful_backup_timestamp) / 3600",
        "legendFormat": "Hours Since Last Backup"
      }],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "green", "value": 0},
              {"color": "yellow", "value": 12},
              {"color": "red", "value": 24}
            ]
          },
          "unit": "h"
        }
      }
    },
    {
      "title": "Replication Lag",
      "type": "timeseries",
      "targets": [{
        "expr": "postgres_replication_lag_seconds",
        "legendFormat": "{{ instance }}"
      }]
    },
    {
      "title": "Cross-Region Health",
      "type": "table",
      "targets": [{
        "expr": "up{job=\"fraiseql\"}",
        "format": "table"
      }]
    },
    {
      "title": "Recovery Time Objectives",
      "type": "table",
      "targets": [{
        "expr": "fraiseql_rto_test_duration_seconds",
        "format": "table"
      }]
    }
  ]
}
```

## Best Practices

### 1. Backup Strategy
- **3-2-1 Rule**: 3 copies, 2 different media types, 1 offsite
- **Automated Testing**: Regular restore testing in non-production
- **Encryption**: All backups encrypted at rest and in transit
- **Retention**: Tiered retention (daily for 30 days, weekly for 3 months, monthly for 1 year)

### 2. Recovery Planning
- **Document Procedures**: Step-by-step recovery runbooks
- **Regular Testing**: Monthly DR tests with different scenarios
- **Communication Plans**: Clear escalation and notification procedures
- **Performance Baselines**: Know expected recovery times

### 3. High Availability
- **Multi-Region**: Deploy across multiple availability zones/regions
- **Load Balancing**: Automatic traffic routing and health checks
- **Circuit Breakers**: Graceful degradation during partial failures
- **Monitoring**: Comprehensive health checks and alerting

### 4. Security
- **Access Control**: Limited access to recovery procedures
- **Audit Trail**: Log all recovery actions
- **Encryption**: End-to-end encryption for all data
- **Network Isolation**: Secure networks for replication traffic

## Next Steps

- [Performance Tuning](./performance-tuning.md) - Optimize for high-scale deployments
- [Security Guide](./security.md) - Comprehensive security implementation
- [Monitoring](./monitoring.md) - Production monitoring and observability
