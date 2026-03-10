# Troubleshooting Guide

Quick troubleshooting for common Fraisier issues.

## Connection Issues

### SSH Connection Timeout

**Error:**

```
ConnectionError: Failed to connect to prod.example.com:22
```

**Quick Fix:**

```bash
# Test direct SSH connection
ssh -v deploy@prod.example.com

# If fails, check:
# 1. Server is reachable
ping prod.example.com

# 2. SSH port is open
telnet prod.example.com 22

# 3. SSH key is correct
ssh-keygen -l -f ~/.ssh/id_fraisier

# 4. SSH key is authorized on server
ssh deploy@prod.example.com "cat .ssh/authorized_keys | grep $(cat ~/.ssh/id_fraisier.pub | cut -d' ' -f2)"
```

### Docker Connection Issues

**Error:**

```
RuntimeError: Failed to connect to Docker daemon
```

**Quick Fix:**

```bash
# Check Docker is running
sudo systemctl status docker

# If not running, start it
sudo systemctl start docker

# Check Docker socket permissions
ls -la /var/run/docker.sock

# Add user to docker group
sudo usermod -aG docker $USER

# Log out and back in for group changes to take effect
newgrp docker
```

## Health Check Issues

### Health Check Timeout

**Error:**

```
Error: Health check failed: Connect timeout
```

**Quick Fix:**

```bash
# Test endpoint directly
curl -v http://localhost:8000/health

# Check if service is running
systemctl status my-api.service

# Check if port is listening
netstat -tuln | grep 8000

# If service is down, start it
systemctl start my-api.service

# Increase timeout in fraises.yaml:
# health_check:
#   timeout: 60
```

### Health Check Returns 500

**Error:**

```
Error: Health check failed: HTTP 500
```

**Quick Fix:**

```bash
# Check application logs
journalctl -u my-api.service -n 50

# SSH and check manually
ssh deploy@prod.example.com
cd /var/www/api
python manage.py shell
# Debug application issues

# Check database connection
python -c "import os; print(os.environ['DATABASE_URL'])"

# Verify database is running
psql -U api -d api_production -c "SELECT version();"
```

## Deployment Issues

### Deployment Hangs

**Error:**

```
Waiting for deployment... (stuck)
```

**Quick Fix:**

```bash
# Kill the stuck process
^C  # Ctrl+C

# Check what happened
fraisier status my_api production

# If stuck, manually check service
ssh deploy@prod.example.com
systemctl status my-api.service

# If service is not responding, restart it
systemctl restart my-api.service
```

### Git Pull Fails

**Error:**

```
Error: git pull failed
```

**Quick Fix:**

```bash
# Test SSH access to git repository
ssh deploy@prod.example.com "cd /var/www/api && git fetch origin"

# Check repository is correct
ssh deploy@prod.example.com "cd /var/www/api && git remote -v"

# If repository URL is wrong, update it
ssh deploy@prod.example.com "cd /var/www/api && git remote set-url origin https://github.com/myorg/myrepo.git"

# Try pull again
ssh deploy@prod.example.com "cd /var/www/api && git pull origin main"
```

### Database Migration Fails

**Error:**

```
Error: Migration failed: Table already exists
```

**Quick Fix:**

```bash
# Check which migrations have run
ssh deploy@prod.example.com
psql -U api -d api_production
SELECT * FROM alembic_version;

# If migration is stuck, manually run it
python manage.py migrate

# If that fails, check the specific migration
python manage.py migrate --show-migration-plan

# Rollback to previous state
python manage.py migrate --rollback

# Then try again
python manage.py migrate
```

## Performance Issues

### High CPU Usage

**Symptoms:**

- Deployment takes too long
- System becomes unresponsive

**Quick Fix:**

```bash
# Check what's using CPU
top -b -n 1 | head -20

# If git pull is slow, check network
ssh deploy@prod.example.com "ping github.com"

# If database operations are slow, check database
ssh deploy@prod.example.com
psql -U api -d api_production
SELECT * FROM pg_stat_activity;

# If python/migrations are slow, increase timeout
# In fraises.yaml:
# bare_metal:
#   timeout: 600
```

### Disk Space Issues

**Error:**

```
Error: Disk space exhausted
```

**Quick Fix:**

```bash
# Check disk usage
ssh deploy@prod.example.com "df -h /"

# Clean up old logs
ssh deploy@prod.example.com "sudo journalctl --vacuum=size=100M"

# Clean up old Docker images
docker image prune -a

# Remove old git branches
ssh deploy@prod.example.com "cd /var/www/api && git prune"
```

## Monitoring Issues

### Metrics Endpoint Fails

**Error:**

```
Error: Could not connect to metrics endpoint
```

**Quick Fix:**

```bash
# Check if metrics server is running
curl http://localhost:8001/metrics

# If fails, start it
fraisier metrics --port 8001

# Check if port is available
netstat -tuln | grep 8001

# If port is in use, use different port
fraisier metrics --port 8002
```

### Missing Metrics

**Error:**

```
No metrics available for deployment
```

**Quick Fix:**

```bash
# Check if prometheus-client is installed
python -c "import prometheus_client; print(prometheus_client.__version__)"

# If not installed:
pip install prometheus-client

# Check if metrics are being recorded
curl http://localhost:8001/metrics | grep fraisier
```

## Rollback Issues

### Rollback Fails

**Error:**

```
Error: Rollback failed
```

**Quick Fix:**

```bash
# Get deployment history
fraisier history my_api production --limit 10

# Manually rollback
ssh deploy@prod.example.com
cd /var/www/api
git revert <commit-hash>
git push origin main
systemctl restart my-api.service

# Verify
curl https://api.prod.example.com/health
```

### Previous Deployment Not Available

**Error:**

```
Error: Cannot rollback - no previous deployment
```

**Quick Fix:**

```bash
# Check deployment history
fraisier history my_api production

# If no history, manually revert to known-good version
ssh deploy@prod.example.com
cd /var/www/api
git checkout <known-good-commit>
systemctl restart my-api.service
```

## Database Issues

### Connection Refused

**Error:**

```
Error: Database connection refused
```

**Quick Fix:**

```bash
# Check database is running
psql -U api -d api_production -c "SELECT 1;"

# If fails, check PostgreSQL status
sudo systemctl status postgresql

# Check PostgreSQL is listening on correct port
sudo netstat -tuln | grep 5432

# Check connection string
echo $DATABASE_URL

# Test connection
psql "$DATABASE_URL"
```

### Migration Timeout

**Error:**

```
Error: Migration timeout after 300 seconds
```

**Quick Fix:**

```bash
# Increase timeout in fraises.yaml:
# bare_metal:
#   timeout: 600

# Or check what migration is taking time
ssh deploy@prod.example.com
psql -U api -d api_production
SELECT * FROM pg_stat_activity WHERE state = 'active';

# Cancel long-running query if needed
SELECT pg_cancel_backend(pid);
```

## Webhook Issues

### Webhook Not Received

**Error:**

```
Webhook received but not processed
```

**Quick Fix:**

```bash
# Check webhook secret matches
echo $FRAISIER_WEBHOOK_SECRET

# Check webhook logs
tail -f /var/log/fraisier/webhooks.log

# Manually trigger deployment
fraisier deploy my_api production

# Check if webhook server is running
curl http://localhost:5000/health
```

## Getting Help

If the above doesn't help:

1. Collect logs:

   ```bash
   fraisier logs my_api production > deployment.log
   journalctl -u my-api.service > service.log
   ```

2. Check Fraisier version:

   ```bash
   fraisier --version
   ```

3. Test in dry-run mode:

   ```bash
   fraisier deploy my_api production --dry-run
   ```

4. Enable debug logging:

   ```bash
   export FRAISIER_LOG_LEVEL=DEBUG
   fraisier deploy my_api production
   ```

5. Report issue with logs:
   - GitHub: https://github.com/fraiseql/fraisier/issues
   - Include error message, logs, and steps to reproduce
