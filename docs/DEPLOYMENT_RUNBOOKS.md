# FraiseQL Deployment Runbooks

Operational procedures for common scenarios.

## Scaling Operations

### Scale Up

Increase replicas:
```bash
kubectl scale deployment fraiseql --replicas=5

# Or edit deployment
kubectl edit deployment fraiseql
# Set replicas: 5
```

Verify:
```bash
kubectl get deployment fraiseql
kubectl get pods -l app=fraiseql
```

### Scale Down

Decrease replicas gracefully:
```bash
# Set graceful termination
kubectl patch deployment fraiseql -p \
  '{"spec":{"template":{"spec":{"terminationGracePeriodSeconds":60}}}}'

# Scale down
kubectl scale deployment fraiseql --replicas=2
```

## Updates & Upgrades

### Rolling Update

```bash
# Update image
kubectl set image deployment/fraiseql \
  fraiseql=fraiseql:v2.1.1

# Monitor rollout
kubectl rollout status deployment/fraiseql

# Verify
kubectl get pods -l app=fraiseql -o wide
```

### Using Helm

```bash
# Update values
helm upgrade fraiseql ./deploy/kubernetes/helm/fraiseql \
  --values prod-values.yaml

# Check status
helm status fraiseql
kubectl rollout status deployment/fraiseql
```

## Maintenance Operations

### Database Backup

```bash
# Create backup pod
kubectl run backup --image=postgres --restart=Never -- \
  pg_dump -h postgres -U fraiseql fraiseql \
  > backup-$(date +%Y%m%d).sql

# Verify backup
ls -lh backup-*.sql
gzip backup-*.sql
```

### Database Upgrade

```bash
# Backup first
# ... (see above)

# Scale down app
kubectl scale deployment fraiseql --replicas=0

# Upgrade PostgreSQL image
kubectl set image pod/postgres-0 \
  postgres=postgres:16

# Scale up app
kubectl scale deployment fraiseql --replicas=3

# Verify
kubectl get pods
```

### Cache Invalidation

```bash
# Connect to Redis
kubectl port-forward service/redis 6379:6379

# In another terminal
redis-cli

# Clear cache
FLUSHDB
# or specific keys
DEL fraiseql:*
```

## Troubleshooting Runbooks

### High CPU Usage

1. Check metrics:
   ```bash
   kubectl top pods -l app=fraiseql
   ```

2. Identify heavy queries:
   ```bash
   # In Prometheus: fraiseql_query_duration_ms
   # High values = slow queries
   ```

3. Scale horizontally:
   ```bash
   kubectl scale deployment fraiseql --replicas=10
   ```

4. Optimize hot queries:
   - Add database indexes
   - Optimize GraphQL query complexity

### High Memory Usage

1. Check current usage:
   ```bash
   kubectl top pods -l app=fraiseql
   ```

2. Reduce cache TTL:
   ```bash
   kubectl set env deployment/fraiseql \
     CACHE_TTL_SECS=300  # 5 minutes instead of 10
   ```

3. Reduce connection pool:
   ```bash
   kubectl set env deployment/fraiseql \
     DB_POOL_MAX=10  # from 20
   ```

4. Check for memory leaks:
   - Review recent logs
   - Consider restarting pods

### Connection Pool Exhaustion

1. Check active connections:
   ```bash
   kubectl logs deployment/fraiseql | grep "connections"
   ```

2. Increase pool size:
   ```bash
   kubectl set env deployment/fraiseql \
     DB_POOL_MAX=30  # from 20
   ```

3. Check for connection leaks:
   - Review code for missing close()
   - Check query timeouts

### Database Connectivity Issues

1. Verify database is running:
   ```bash
   kubectl get pod -l app=postgres
   ```

2. Test connectivity:
   ```bash
   kubectl run -it --rm debug --image=postgres --restart=Never -- \
     psql -h postgres -U fraiseql -d fraiseql -c "SELECT 1"
   ```

3. Check environment variables:
   ```bash
   kubectl get pod <fraiseql-pod> -o yaml | grep DATABASE_URL
   ```

4. Restart pod if needed:
   ```bash
   kubectl delete pod <fraiseql-pod>
   ```

## Disaster Recovery

### Pod Failure Recovery

Automatic (handled by Kubernetes):
- Failed pod automatically restarted
- Verify with: `kubectl get events`

### Node Failure Recovery

Pods are rescheduled (handled by Kubernetes):
```bash
# Monitor
kubectl get pods -o wide
# Pods should move to healthy nodes
```

### Complete Cluster Failure

1. Assess backup status:
   ```bash
   # Verify recent backup exists and is valid
   ls -lh backup-*.sql.gz
   ```

2. Restore from backup:
   ```bash
   # Create new cluster
   # Restore database
   gunzip backup-latest.sql.gz
   psql -h newhost -U fraiseql fraiseql < backup-latest.sql
   
   # Deploy FraiseQL
   kubectl apply -f deploy/kubernetes/fraiseql-hardened.yaml
   ```

## Performance Tuning

### Query Optimization

```bash
# Enable query logging
kubectl set env deployment/fraiseql \
  RUST_LOG=debug

# Analyze slow queries
kubectl logs deployment/fraiseql | grep "duration" | sort -r | head -20

# Add indexes to PostgreSQL
psql $DATABASE_URL -c "CREATE INDEX idx_user_email ON users(email);"
```

### Connection Pool Tuning

Optimal pool size = (connections per db * max queries per connection) / 2

```
Min: 5-10
Max: 20-50 (depending on workload)
```

### Caching Strategy

- Short lived: 5-10 minutes (frequently changing data)
- Medium lived: 1 hour (semi-static data)
- Long lived: 24 hours (static configuration)

## Monitoring & Alerting

### Key Metrics to Watch

```
1. Error rate < 0.1%
2. p95 latency < 1000ms
3. CPU usage < 70%
4. Memory usage < 80%
5. Connection pool usage < 80%
```

### Setting up Alerts

In Prometheus:
```yaml
alert: HighErrorRate
expr: rate(fraiseql_errors_total[5m]) > 0.001
for: 5m

alert: HighLatency
expr: fraiseql_query_duration_ms{quantile="0.95"} > 1000
for: 5m
```

## On-Call Procedures

### During Incident

1. **Page On-Call**: Immediate response to critical alerts
2. **Investigate**: Check logs, metrics, recent changes
3. **Communicate**: Update status page
4. **Mitigate**: Apply quick fix or rollback
5. **Resolve**: Implement permanent fix

### Post-Incident

1. **Document**: Create incident report
2. **Root Cause Analysis**: Why did it happen?
3. **Remediation**: Prevent recurrence
4. **Retro**: Team learning session
