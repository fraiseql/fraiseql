# Deploying to Coolify with Fraisier

**Perfect For**: PaaS deployments, managed infrastructure, simplified DevOps

**Components**: Coolify API + Fraisier integration

**Setup Time**: 20-25 minutes

---

## Overview

The Coolify provider deploys services to Coolify, a self-hosted platform-as-a-service. This is ideal for:

- ✅ Self-hosted PaaS deployments
- ✅ Simplified DevOps workflows
- ✅ Private infrastructure with managed deployment
- ✅ Teams wanting Git-based deployments
- ✅ Multi-region deployments

---

## Prerequisites

### Coolify Setup

- Coolify 4.0+ installed and running
- Coolify API token generated
- Docker installed on Coolify host
- Git repository accessible

### Fraisier Setup

- Fraisier CLI installed
- Coolify API credentials configured
- Network access to Coolify API

---

## Step 1: Configure Coolify

### Generate API Token

In Coolify UI:

1. Go to **Settings → API Tokens**
2. Click **Generate Token**
3. Copy the token (you won't see it again)

Store in environment:

```bash
export COOLIFY_API_TOKEN="your_token_here"
export COOLIFY_API_URL="https://coolify.example.com/api"
```

### Create Project in Coolify

1. Go to **Projects**
2. Click **Create Project**
3. Name: "my-api-production"
4. Description: "My API Production Deployment"

### Create Environment

1. In project, click **Environments**
2. Create new environment (e.g., "production")
3. Configure SSH key for application deployment

### Get Project and Environment IDs

Via Coolify API:

```bash
curl -X GET https://coolify.example.com/api/projects \
  -H "Authorization: Bearer $COOLIFY_API_TOKEN" | jq '.[] | {id, name}'

# Output:
# {
#   "id": "proj_12345",
#   "name": "my-api-production"
# }
```

---

## Step 2: Configure Fraisier Provider

### In fraises.yaml

```yaml
fraises:
  my_api:
    type: api
    git_provider: github
    git_repo: your-org/my-api
    git_branch: main

    environments:
      production:
        provider: coolify
        provider_config:
          # Coolify API settings
          api_url: https://coolify.example.com/api
          api_token: ${COOLIFY_API_TOKEN}  # From environment variable

          # Project and environment
          project_id: proj_12345
          environment_id: env_67890

          # Application settings
          application_name: my-api
          application_type: node  # or python, docker, etc.
          port: 8000

          # Git settings
          git_repo: https://github.com/your-org/my-api
          git_branch: main
          git_token: ${GITHUB_TOKEN}  # For private repos

          # Health check
          health_check:
            type: http
            url: http://my-api-production.coolify.example.com/health
            timeout: 10
            max_retries: 3

          # Deployment settings
          deployment_strategy: rolling
          auto_deploy: true  # Deploy on push
          pull_request_previews: true
```

### Configuration Reference

| Setting | Type | Required | Description |
|---------|------|----------|-------------|
| `api_url` | string | Yes | Coolify API URL |
| `api_token` | string | Yes | Coolify API token |
| `project_id` | string | Yes | Coolify project ID |
| `environment_id` | string | Yes | Coolify environment ID |
| `application_name` | string | Yes | Application name in Coolify |
| `application_type` | string | Yes | Application type (node, python, docker, etc.) |
| `port` | integer | Yes | Application port |
| `git_repo` | string | Yes | Git repository URL |
| `git_branch` | string | Yes | Git branch to deploy |
| `git_token` | string | No | GitHub/GitLab token (for private repos) |

---

## Step 3: Create Application in Coolify

### Via Fraisier (Automated)

```bash
# Fraisier can create the application
fraisier coolify:create-app \
  --project-id proj_12345 \
  --environment-id env_67890 \
  --name my-api \
  --type node \
  --git-repo https://github.com/your-org/my-api
```

### Via Coolify UI (Manual)

1. Go to **Project → Environment → Applications**
2. Click **Create Application**
3. Select **Git Repository**
4. Configure:
   - **Name**: my-api
   - **Repository**: https://github.com/your-org/my-api
   - **Branch**: main
   - **Build Pack**: Node (or your runtime)
   - **Port**: 8000
   - **Domain**: my-api-production.coolify.example.com

### Configure Build Settings

In Coolify UI or via environment variables:

```bash
# Node.js
NIXPACKS_NODE_VERSION=18
NIXPACKS_NPM_INSTALL_FLAGS="--legacy-peer-deps"

# Python
NIXPACKS_PYTHON_VERSION=3.11
NIXPACKS_PIP_INSTALL_FLAGS="--no-cache-dir"

# Docker
NIXPACKS_DOCKER_REGISTRY_URL=registry.example.com
```

---

## Step 4: Deploy

### First Deployment

```bash
# Deploy via Fraisier
fraisier deploy my_api production

# Output:
# Starting deployment to Coolify...
#
# ✓ Connecting to Coolify API
# ✓ Retrieving application
# ✓ Creating deployment
# ✓ Building application (45 seconds)
# ✓ Starting application (5 seconds)
# ✓ Health checks passed (50ms)
#
# ✅ Deployment successful in 55 seconds
#
# Service: my-api
# URL: https://my-api-production.coolify.example.com
# Version: 2.0.0
# Status: healthy
```

### Deployment Flow

1. Push code to GitHub
2. Fraisier detects push (via webhook)
3. Triggers deployment via Coolify API
4. Coolify builds application (nixpacks)
5. Coolify starts application
6. Fraisier runs health checks
7. Deployment completes

---

## Step 5: Configure Auto-Deployment

### Enable GitHub Webhook

In Fraisier:

```yaml
provider_config:
  auto_deploy: true
  auto_deploy_branch: main
```

In Coolify:

1. Go to **Project → Settings → Webhooks**
2. Ensure GitHub webhook is enabled
3. Verify secret is configured

### Push to Deploy

```bash
# Push code
git push origin main

# Coolify automatically deploys
# Monitor deployment
fraisier status my_api production --watch
```

---

## Environment Variables

### Configure in Coolify

Via Coolify UI:

1. **Project → Environment → Variables**
2. Add variables:
   - `DATABASE_URL`: postgresql://user:pass@db.example.com/app
   - `REDIS_URL`: redis://redis.example.com:6379
   - `NODE_ENV`: production
   - `LOG_LEVEL`: info

Or via API:

```bash
curl -X POST https://coolify.example.com/api/applications/app_123/variables \
  -H "Authorization: Bearer $COOLIFY_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "key": "DATABASE_URL",
    "value": "postgresql://user:pass@db.example.com/app"
  }'
```

### Using in Fraisier

```bash
# Set via environment
export MY_API_DATABASE_URL="postgresql://..."

# Or in .env file
echo "MY_API_DATABASE_URL=postgresql://..." >> .env
source .env

fraisier deploy my_api production
```

---

## Monitoring & Logs

### View Deployment Logs

Via Coolify UI:

1. **Project → Environment → Applications → my-api**
2. Click **Logs** tab
3. View real-time logs

Via Fraisier:

```bash
# View deployment logs
fraisier logs dep_00001

# View application logs
fraisier coolify:logs --application my-api --tail 100

# Follow logs
fraisier coolify:logs --application my-api --follow
```

### Monitor Application

```bash
# Check status
fraisier status my_api production

# View health metrics
fraisier coolify:metrics --application my-api

# View resource usage
fraisier coolify:stats --application my-api
```

---

## Rollback & Recovery

### Automatic Rollback

```yaml
provider_config:
  auto_rollback_on_failure: true
  health_check_delay: 60
```

### Manual Rollback

```bash
# Rollback to previous version
fraisier rollback my_api production

# View deployment history
fraisier history my_api production

# See what was rolled back
fraisier logs dep_00001
```

### Via Coolify API

```bash
# Get deployment history
curl -X GET https://coolify.example.com/api/applications/app_123/deployments \
  -H "Authorization: Bearer $COOLIFY_API_TOKEN"

# Deploy specific commit
curl -X POST https://coolify.example.com/api/applications/app_123/deploy \
  -H "Authorization: Bearer $COOLIFY_API_TOKEN" \
  -d '{"commit_sha": "abc123..."}'
```

---

## Scaling

### Horizontal Scaling

In Coolify:

1. **Project → Environment → Applications → my-api**
2. **Settings → Scaling**
3. Set number of replicas

Via API:

```bash
curl -X PATCH https://coolify.example.com/api/applications/app_123 \
  -H "Authorization: Bearer $COOLIFY_API_TOKEN" \
  -d '{"replicas": 3}'
```

### Load Balancing

Coolify automatically load-balances across replicas using Caddy reverse proxy.

Access via domain:
```
https://my-api-production.coolify.example.com
```

---

## Custom Domains

### Configure in Coolify

1. **Application → Settings → Domains**
2. Add domain: `api.example.com`
3. Configure DNS CNAME or A record

Via API:

```bash
curl -X POST https://coolify.example.com/api/applications/app_123/domains \
  -H "Authorization: Bearer $COOLIFY_API_TOKEN" \
  -d '{"domain": "api.example.com"}'
```

### SSL/TLS

Coolify automatically provisions Let's Encrypt certificates. Domains must be publicly resolvable.

---

## Advanced Configuration

### Pre-deployment Hooks

```yaml
provider_config:
  hooks:
    pre_deployment:
      - command: npm run lint
        on_failure: fail
      - command: npm run test
        on_failure: warn
```

### Post-deployment Hooks

```yaml
provider_config:
  hooks:
    post_deployment:
      - command: npm run migrate
      - command: npm run seed
      - command: curl -X POST https://example.com/deploy-webhook
```

### Docker Image Deployment

Instead of Git repository:

```yaml
provider_config:
  deployment_type: docker_image
  docker_image: my-registry.com/my-api:2.0.0
  registry_username: ${DOCKER_USERNAME}
  registry_password: ${DOCKER_PASSWORD}
```

---

## Troubleshooting

### Build Failures

```bash
# View build logs
fraisier coolify:logs --application my-api --component build

# Check build pack
# Coolify auto-detects based on package.json, requirements.txt, etc.

# Force specific build pack
# In Coolify UI → Application → Settings → Build Pack
```

### Deployment Stuck

```bash
# Check Coolify status
curl -X GET https://coolify.example.com/api/system/status \
  -H "Authorization: Bearer $COOLIFY_API_TOKEN"

# Check application status
curl -X GET https://coolify.example.com/api/applications/app_123 \
  -H "Authorization: Bearer $COOLIFY_API_TOKEN"

# Force restart
fraisier coolify:restart --application my-api
```

### Health Check Failing

```bash
# Test endpoint manually
curl -v https://my-api-production.coolify.example.com/health

# Check if domain resolves
nslookup my-api-production.coolify.example.com

# Check Coolify reverse proxy
# Coolify uses Caddy internally
```

### Performance Issues

```bash
# Check resource allocation
fraisier coolify:stats --application my-api

# Increase memory/CPU in Coolify UI
# Application → Settings → Resources

# View slow deployments
fraisier history my_api production --limit 20
```

---

## Security Best Practices

### 1. Protect API Token

```bash
# Store in environment
export COOLIFY_API_TOKEN="..."

# Or in .env (add to .gitignore)
echo "COOLIFY_API_TOKEN=..." >> .env

# Never commit to Git
echo ".env" >> .gitignore
```

### 2. Restrict Coolify Access

In Coolify:

1. Create API token with minimal permissions
2. Enable IP whitelist
3. Set token expiration

### 3. Secure Git Access

For private repositories:

```bash
export GITHUB_TOKEN="ghp_..."

fraisier deploy my_api production
```

### 4. Configure SSH Keys

Coolify needs SSH key for some operations:

1. **Project → SSH Keys**
2. Add your SSH public key
3. Use for Git authentication

### 5. Enable SSL/TLS

Ensure:
- [ ] HTTPS configured
- [ ] Certificate valid and not expired
- [ ] HSTS enabled (via Caddy config)

---

## Production Checklist

- [ ] Coolify installed and accessible
- [ ] API token generated and stored
- [ ] Project created in Coolify
- [ ] Environment configured
- [ ] Application created
- [ ] Git repository accessible
- [ ] Build settings configured
- [ ] Environment variables set
- [ ] Health check endpoint implemented
- [ ] SSL certificate configured
- [ ] Custom domain configured
- [ ] Auto-deployment enabled
- [ ] First deployment successful
- [ ] Health checks passing
- [ ] Monitoring configured
- [ ] Backup strategy defined

---

## Multi-Region Setup

Deploy same application to multiple Coolify instances:

```yaml
fraises:
  my_api:
    environments:
      us-east:
        provider: coolify
        provider_config:
          api_url: https://coolify-us.example.com/api
          api_token: ${COOLIFY_US_TOKEN}
          project_id: proj_us_001

      eu-west:
        provider: coolify
        provider_config:
          api_url: https://coolify-eu.example.com/api
          api_token: ${COOLIFY_EU_TOKEN}
          project_id: proj_eu_001
```

Deploy to both:

```bash
fraisier deploy my_api us-east
fraisier deploy my_api eu-west
```

---

## Reference

- [Coolify Documentation](https://coolify.io/docs)
- [Coolify API Reference](https://coolify.io/docs/api)
- [Nixpacks Documentation](https://nixpacks.com/)
- [PROVIDER_BARE_METAL.md](PROVIDER_BARE_METAL.md) - Bare Metal provider
- [PROVIDER_DOCKER_COMPOSE.md](PROVIDER_DOCKER_COMPOSE.md) - Docker Compose provider
- [CLI_REFERENCE.md](CLI_REFERENCE.md) - CLI commands
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues
