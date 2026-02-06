# Fraisier CLI Reference

**Version**: 0.1.0
**Installation**: `pip install fraisier`

## Quick Start

```bash
# Display version
fraisier --version

# Display help
fraisier --help

# Deploy a service
fraisier deploy my_api production

# Check deployment status
fraisier status my_api production

# View deployment history
fraisier history my_api
```

---

## Global Options

These options work with any command:

```bash
fraisier [GLOBAL_OPTIONS] COMMAND [COMMAND_OPTIONS]
```

**Global Options**:

- `--version`: Show version and exit
- `--help`: Show help and exit
- `--config CONFIG`: Path to fraises.yaml (default: ./fraises.yaml)
- `--database DATABASE`: Database type (sqlite, postgresql, mysql)
- `--db-path PATH`: Database path/connection string
- `--verbose / --no-verbose`: Verbose output (default: false)
- `--json`: Output in JSON format (default: false)

**Examples**:

```bash
# Use custom config file
fraisier --config ~/fraisier/config.yaml deploy my_api production

# Verbose output
fraisier --verbose deploy my_api production

# JSON output for parsing
fraisier --json deploy my_api production | jq '.status'

# Specific database
fraisier --database postgresql --db-path postgresql://localhost/fraisier deploy my_api production
```

---

## Deployment Commands

### fraisier deploy

Deploy a service to an environment.

**Usage**:

```bash
fraisier deploy [OPTIONS] FRAISE ENVIRONMENT
```

**Arguments**:

- `FRAISE` (required): Service name to deploy
- `ENVIRONMENT` (required): Target environment (development, staging, production, etc.)

**Options**:

- `--version VERSION`: Specific version to deploy (default: latest from Git)
- `--strategy STRATEGY`: Deployment strategy (default: rolling)
  - `rolling`: One instance at a time with health checks
  - `blue_green`: Switch all instances at once (zero-downtime)
  - `canary`: Gradual rollout to percentage of instances (default: 10%)
  - `progressive`: Progressive with automated percentage increase
- `--health-check-delay SECONDS`: Wait before health checks (default: 10)
- `--health-check-timeout SECONDS`: Max time for health checks (default: 30)
- `--wait`: Wait for deployment to complete (default: false)
- `--timeout SECONDS`: Max time to wait (default: 3600 = 1 hour)
- `--skip-backup`: Skip automatic backup (default: backup enabled)
- `--skip-health-check`: Skip health checks (NOT RECOMMENDED)
- `--dry-run`: Show what would happen, don't actually deploy
- `--force`: Force deployment even if checks fail (requires --yes)
- `--yes / -y`: Skip confirmation prompts
- `--metadata KEY=VALUE`: Add metadata for audit logging (repeatable)

**Examples**:

```bash
# Deploy latest version with default rolling strategy
fraisier deploy my_api production

# Deploy specific version
fraisier deploy my_api production --version 2.0.0

# Deploy with blue-green (zero-downtime)
fraisier deploy my_api production --strategy blue_green

# Deploy with canary (10% at a time)
fraisier deploy my_api production --strategy canary

# Deploy and wait for completion
fraisier deploy my_api production --wait --timeout 600

# Dry-run before actual deployment
fraisier deploy my_api production --dry-run

# Deploy with metadata for audit logging
fraisier deploy my_api production --metadata ticket=DEPLOY-123 --metadata reason="Bug fix"

# Deploy without confirmation prompt
fraisier deploy my_api production -y
```

**Exit Codes**:

- 0: Success
- 1: General error
- 2: Invalid arguments
- 3: Fraise not found
- 4: Environment not found
- 5: Deployment failed
- 6: Health checks failed

---

### fraisier rollback

Rollback to a previous version.

**Usage**:

```bash
fraisier rollback [OPTIONS] FRAISE ENVIRONMENT
```

**Arguments**:

- `FRAISE` (required): Service name to rollback
- `ENVIRONMENT` (required): Target environment

**Options**:

- `--to-version VERSION`: Specific version to rollback to (default: previous version)
- `--reason REASON`: Reason for rollback (for audit logging)
- `--wait`: Wait for rollback to complete
- `--timeout SECONDS`: Max time to wait (default: 3600)
- `--yes / -y`: Skip confirmation prompt

**Examples**:

```bash
# Rollback to previous version
fraisier rollback my_api production

# Rollback to specific version
fraisier rollback my_api production --to-version 1.8.0

# Rollback with reason
fraisier rollback my_api production --reason "Critical bug in 2.0.0"

# Rollback and wait
fraisier rollback my_api production --wait --timeout 600

# Confirm without prompt
fraisier rollback my_api production -y
```

---

### fraisier pause

Pause an ongoing deployment.

**Usage**:

```bash
fraisier pause DEPLOYMENT_ID
```

**Arguments**:

- `DEPLOYMENT_ID` (required): Deployment ID to pause

**Example**:

```bash
fraisier pause dep_00001
```

---

### fraisier resume

Resume a paused deployment.

**Usage**:

```bash
fraisier resume DEPLOYMENT_ID
```

**Arguments**:

- `DEPLOYMENT_ID` (required): Deployment ID to resume

**Example**:

```bash
fraisier resume dep_00001
```

---

### fraisier cancel

Cancel an ongoing deployment.

**Usage**:

```bash
fraisier cancel DEPLOYMENT_ID
```

**Arguments**:

- `DEPLOYMENT_ID` (required): Deployment ID to cancel

**Example**:

```bash
fraisier cancel dep_00001
```

---

## Status Commands

### fraisier status

Show current status of a service.

**Usage**:

```bash
fraisier status [OPTIONS] FRAISE [ENVIRONMENT]
```

**Arguments**:

- `FRAISE` (required): Service name
- `ENVIRONMENT` (optional): Specific environment (show all if not specified)

**Options**:

- `--long / -l`: Show detailed information
- `--watch / -w`: Watch status (update every 5 seconds)
- `--interval SECONDS`: Update interval (default: 5)

**Examples**:

```bash
# Quick status
fraisier status my_api

# Status for specific environment
fraisier status my_api production

# Detailed status
fraisier status my_api production --long

# Watch status (auto-update)
fraisier status my_api production --watch
```

**Output Example**:

```
my_api / production
├─ Status: Healthy
├─ Current Version: 1.9.0
├─ Previous Version: 1.8.5
├─ Deployment Strategy: rolling
├─ Last Deployment: 2024-01-22 10:05:30 (6 minutes ago)
├─ Health Checks: 3/3 passing
└─ Instances: 4/4 healthy
```

---

### fraisier list

List all services (fraises).

**Usage**:

```bash
fraisier list [OPTIONS]
```

**Options**:

- `--environment ENV`: Filter by environment
- `--type TYPE`: Filter by type (api, etl, scheduled)
- `--status STATUS`: Filter by status (healthy, degraded, unhealthy)
- `--long / -l`: Show detailed information

**Examples**:

```bash
# List all services
fraisier list

# List all API services
fraisier list --type api

# List all services in production
fraisier list --environment production

# Detailed list
fraisier list --long

# List production APIs in detail
fraisier list --environment production --type api --long
```

---

### fraisier health

Check health of all services.

**Usage**:

```bash
fraisier health [OPTIONS]
```

**Options**:

- `--environment ENV`: Check specific environment only
- `--watch / -w`: Watch health (auto-update)
- `--interval SECONDS`: Update interval (default: 5)

**Examples**:

```bash
# Check all services
fraisier health

# Check production only
fraisier health --environment production

# Watch health (auto-update every 10 seconds)
fraisier health --watch --interval 10
```

---

## History Commands

### fraisier history

View deployment history.

**Usage**:

```bash
fraisier history [OPTIONS] FRAISE [ENVIRONMENT]
```

**Arguments**:

- `FRAISE` (required): Service name
- `ENVIRONMENT` (optional): Specific environment

**Options**:

- `--limit N`: Show last N deployments (default: 20)
- `--status STATUS`: Filter by status (success, failed, cancelled)
- `--since TIME`: Show deployments after this time (ISO 8601 or "1h", "1d", "1w")
- `--long / -l`: Show detailed information

**Examples**:

```bash
# Recent deployments
fraisier history my_api

# Last 50 deployments
fraisier history my_api --limit 50

# Production deployments
fraisier history my_api production

# Failed deployments
fraisier history my_api production --status failed

# Deployments in the last 24 hours
fraisier history my_api --since 1d

# Detailed history
fraisier history my_api production --long
```

**Output Example**:

```
my_api / production (last 5):

1. dep_00001 | v1.9.0 ← v1.8.5 | SUCCESS | 2024-01-22 10:05:30 | 325s
2. dep_00000 | v1.8.5 ← v1.8.0 | SUCCESS | 2024-01-21 14:00:00 | 525s
3. dep_99999 | v1.8.0 ← v1.7.2 | SUCCESS | 2024-01-20 09:30:00 | 420s
4. dep_99998 | v1.7.2 ← v1.7.0 | FAILED  | 2024-01-19 16:45:00 | 180s
5. dep_99997 | v1.7.0 ← v1.6.5 | SUCCESS | 2024-01-18 11:00:00 | 310s
```

---

### fraisier logs

View deployment logs.

**Usage**:

```bash
fraisier logs [OPTIONS] DEPLOYMENT_ID
```

**Arguments**:

- `DEPLOYMENT_ID` (required): Deployment ID

**Options**:

- `--lines N`: Show last N lines (default: 100, max: 1000)
- `--follow / -f`: Follow logs (stream new lines)
- `--level LEVEL`: Filter by log level (info, warn, error)
- `--component COMPONENT`: Filter by component (deployment, health_check, provider)
- `--timestamps`: Show timestamps (default: true)
- `--no-timestamps`: Hide timestamps

**Examples**:

```bash
# Show last 100 lines
fraisier logs dep_00001

# Show last 50 lines
fraisier logs dep_00001 --lines 50

# Follow logs in real-time
fraisier logs dep_00001 --follow

# Show only errors
fraisier logs dep_00001 --level error

# Show provider component logs
fraisier logs dep_00001 --component provider
```

---

## Configuration Commands

### fraisier config

Manage configuration.

**Usage**:

```bash
fraisier config [OPTIONS] ACTION
```

**Actions**:

- `show`: Display current configuration
- `validate`: Validate configuration file
- `init`: Initialize configuration file

**Examples**:

```bash
# Show current configuration
fraisier config show

# Validate configuration
fraisier config validate

# Initialize new configuration
fraisier config init
```

---

### fraisier env

Manage environment configuration.

**Usage**:

```bash
fraisier env [OPTIONS] ACTION [ENVIRONMENT]
```

**Actions**:

- `list`: List all environments
- `show`: Show environment details
- `add`: Add new environment
- `remove`: Remove environment
- `update`: Update environment

**Examples**:

```bash
# List environments
fraisier env list

# Show production environment
fraisier env show production

# Add new environment
fraisier env add staging --provider bare_metal

# Update environment
fraisier env update production --health-check-timeout 60
```

---

## Database Commands

### fraisier db

Database management.

**Usage**:

```bash
fraisier db [OPTIONS] ACTION
```

**Actions**:

- `init`: Initialize database schema
- `migrate`: Run database migrations
- `status`: Check database status
- `backup`: Backup database
- `restore`: Restore database from backup

**Options**:

- `--database TYPE`: Database type (sqlite, postgresql, mysql)
- `--path PATH`: Database path/connection string

**Examples**:

```bash
# Initialize SQLite database
fraisier db init --database sqlite --path fraisier.db

# Initialize PostgreSQL database
fraisier db init --database postgresql --path postgresql://localhost/fraisier

# Migrate database
fraisier db migrate

# Check database status
fraisier db status

# Backup database
fraisier db backup --output backup.sql

# Restore from backup
fraisier db restore --input backup.sql
```

---

## Monitoring Commands

### fraisier metrics

Display system metrics.

**Usage**:

```bash
fraisier metrics [OPTIONS]
```

**Options**:

- `--service SERVICE`: Filter by service
- `--environment ENV`: Filter by environment
- `--watch / -w`: Watch metrics (auto-update)
- `--interval SECONDS`: Update interval (default: 5)

**Examples**:

```bash
# Show all metrics
fraisier metrics

# Show my_api metrics
fraisier metrics --service my_api

# Production metrics
fraisier metrics --environment production

# Watch metrics
fraisier metrics --watch
```

---

### fraisier alerts

Manage alerts.

**Usage**:

```bash
fraisier alerts [OPTIONS] ACTION
```

**Actions**:

- `list`: List active alerts
- `show`: Show alert details
- `acknowledge`: Acknowledge alert
- `resolve`: Mark alert as resolved

**Examples**:

```bash
# List active alerts
fraisier alerts list

# Show alert details
fraisier alerts show alert_123

# Acknowledge alert
fraisier alerts acknowledge alert_123

# Resolve alert
fraisier alerts resolve alert_123
```

---

## Webhook Commands

### fraisier webhook

Manage webhooks.

**Usage**:

```bash
fraisier webhook [OPTIONS] ACTION
```

**Actions**:

- `list`: List webhooks
- `add`: Add webhook
- `remove`: Remove webhook
- `test`: Test webhook delivery

**Options**:

- `--event EVENT`: Filter by event type

**Examples**:

```bash
# List webhooks
fraisier webhook list

# Add webhook for deployment events
fraisier webhook add --event deployment.completed \
  https://example.com/webhook

# Test webhook
fraisier webhook test webhook_123

# Remove webhook
fraisier webhook remove webhook_123
```

---

## Authentication Commands

### fraisier auth

Authentication management.

**Usage**:

```bash
fraisier auth [OPTIONS] ACTION
```

**Actions**:

- `login`: Authenticate with Fraisier
- `logout`: Remove stored credentials
- `token`: Display current auth token
- `status`: Show authentication status

**Examples**:

```bash
# Login
fraisier auth login

# Display current token
fraisier auth token

# Check auth status
fraisier auth status

# Logout
fraisier auth logout
```

---

## Advanced Usage

### Scripting with JSON Output

```bash
#!/bin/bash

# Get all production deployments as JSON
DEPLOYMENTS=$(fraisier history my_api production --limit 100 --json)

# Parse JSON
echo "$DEPLOYMENTS" | jq '.deployments[] | select(.status=="failed")'

# Extract IDs
echo "$DEPLOYMENTS" | jq -r '.deployments[].id'
```

### Watch Deployment in Loop

```bash
#!/bin/bash

DEPLOY_ID=$1
while true; do
  STATUS=$(fraisier logs $DEPLOY_ID --json | jq -r '.deployment_id')
  if [ "$STATUS" != "pending" ] && [ "$STATUS" != "in_progress" ]; then
    break
  fi
  echo "Deployment in progress..."
  sleep 5
done

echo "Deployment complete!"
```

### Batch Deployments

```bash
#!/bin/bash

# Deploy multiple services
for service in api worker scheduler; do
  echo "Deploying $service..."
  fraisier deploy $service production --wait --yes
  echo "$service deployed successfully!"
done
```

### Conditional Deployment

```bash
#!/bin/bash

# Only deploy if staging is healthy
if fraisier status my_api staging --json | jq -e '.status=="healthy"' > /dev/null; then
  echo "Staging is healthy, deploying to production..."
  fraisier deploy my_api production --wait --yes
else
  echo "Staging is not healthy, aborting deployment"
  exit 1
fi
```

---

## Exit Codes

| Code | Meaning | Common Causes |
|------|---------|---------------|
| 0 | Success | Command completed successfully |
| 1 | General Error | Unspecified error |
| 2 | Invalid Arguments | Bad flags or arguments |
| 3 | Not Found | Resource not found (service, environment) |
| 4 | Conflict | Resource already exists or state conflict |
| 5 | Failed | Operation failed (deployment, health check) |
| 6 | Unauthorized | Authentication required or invalid |
| 7 | Permission Denied | Insufficient permissions |
| 8 | Timeout | Operation timed out |
| 9 | Database Error | Database connection or query error |

---

## Configuration File

Default location: `./fraises.yaml`

```yaml
fraises:
  my_api:
    type: api
    description: Main API service
    git_provider: github
    git_repo: my-org/my-api
    git_branch: main

    environments:
      development:
        provider: docker_compose
        provider_config:
          docker_compose_file: ./docker-compose.yml
          service: api

      staging:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: staging-server.example.com
              username: deploy
          service_name: my-api

      production:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: prod-1.example.com
            - hostname: prod-2.example.com
          service_name: my-api
          deployment_strategy: blue_green
```

---

## Tips & Tricks

### Faster Status Checks

```bash
# Instead of:
fraisier status my_api production

# Use watch mode:
fraisier status my_api production --watch
```

### Real-time Logs

```bash
# Follow deployment logs as they happen
fraisier logs dep_00001 --follow
```

### Batch Operations

```bash
# Deploy all services to staging
for service in $(fraisier list --json | jq -r '.fraises[].name'); do
  fraisier deploy $service staging
done
```

### Database Backups

```bash
# Daily backup
0 2 * * * fraisier db backup --output backup-$(date +\%Y-\%m-\%d).sql
```

---

## Environment Variables

Fraisier can be configured via environment variables:

```bash
# Configuration
export FRAISIER_CONFIG=/etc/fraisier/fraises.yaml
export FRAISIER_DATABASE=postgresql
export FRAISIER_DB_PATH=postgresql://localhost/fraisier

# Output
export FRAISIER_JSON_OUTPUT=false
export FRAISIER_VERBOSE=false

# Authentication
export FRAISIER_TOKEN=eyJ...

# Deployment defaults
export FRAISIER_DEFAULT_STRATEGY=rolling
export FRAISIER_DEFAULT_TIMEOUT=3600
```

---

## Troubleshooting

### Command Not Found

```bash
# Ensure Fraisier is installed
pip install fraisier

# Or use full path
python -m fraisier deploy my_api production
```

### Authentication Errors

```bash
# Check if logged in
fraisier auth status

# Login again
fraisier auth login

# Or set token directly
export FRAISIER_TOKEN=$(fraisier auth token)
```

### Timeout Errors

```bash
# Increase timeout
fraisier deploy my_api production --timeout 7200

# Or disable waiting
fraisier deploy my_api production  # Don't use --wait
```

---

## Next Steps

- See [API_REFERENCE.md](API_REFERENCE.md) for REST API documentation
- See [PROVIDER_*.md] files for provider-specific setup
- See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for common issues
