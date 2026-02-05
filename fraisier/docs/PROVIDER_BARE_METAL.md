# Deploying to Bare Metal with Fraisier

**Perfect For**: On-premise servers, private data centers, existing infrastructure

**Components**: SSH + systemd service management

**Setup Time**: 20-30 minutes per server

---

## Overview

The Bare Metal provider deploys services to Linux servers via SSH using systemd for service management. This is ideal for:

- ✅ On-premise deployments
- ✅ Private data centers
- ✅ Existing infrastructure
- ✅ Maximum control over deployment process
- ✅ Legacy systems migration

---

## Prerequisites

### Server Requirements

- Linux OS (Ubuntu 20.04+, CentOS 8+, Debian 11+)
- SSH access enabled
- systemd available (most modern Linux distros)
- Git installed
- Docker or runtime for your application

### Client Requirements

- SSH key pair (ed25519 or RSA)
- SSH access to target servers
- Fraisier CLI installed

---

## Step 1: Prepare SSH Keys

### Generate SSH Key Pair

```bash
# Generate SSH key (if you don't have one)
ssh-keygen -t ed25519 -f ~/.ssh/fraisier -N ""

# Output:
# Generating public/private ed25519 key pair.
# Your identification has been saved in ~/.ssh/fraisier
# Your public key has been saved in ~/.ssh/fraisier.pub
```

### Copy Public Key to Server

```bash
# Copy public key to target server
ssh-copy-id -i ~/.ssh/fraisier deploy@production-server.example.com

# Or manually add to authorized_keys
cat ~/.ssh/fraisier.pub | ssh deploy@production-server.example.com \
  "mkdir -p ~/.ssh && cat >> ~/.ssh/authorized_keys"

# Verify access
ssh -i ~/.ssh/fraisier deploy@production-server.example.com "echo 'SSH working!'"
```

### Add to SSH Config (Optional but Recommended)

Create `~/.ssh/config`:

```
Host prod-1.example.com
    HostName prod-1.example.com
    User deploy
    IdentityFile ~/.ssh/fraisier
    StrictHostKeyChecking accept-new

Host prod-2.example.com
    HostName prod-2.example.com
    User deploy
    IdentityFile ~/.ssh/fraisier
```

Now you can SSH without specifying the key:
```bash
ssh prod-1.example.com
```

---

## Step 2: Configure Bare Metal Provider

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
        provider: bare_metal
        provider_config:
          # SSH connection settings
          hosts:
            - hostname: prod-1.example.com
              port: 22
              username: deploy
              ssh_key_path: ~/.ssh/fraisier
              # Optional: specific server config
              app_path: /opt/my-api
              service_name: my-api

            - hostname: prod-2.example.com
              port: 22
              username: deploy
              ssh_key_path: ~/.ssh/fraisier

          # Service configuration (applies to all hosts if not overridden)
          app_path: /opt/my-api
          service_name: my-api
          systemd_service: my-api.service

          # Health check configuration
          health_check:
            type: http
            url: http://localhost:8000/health
            timeout: 10
            max_retries: 3
            retry_delay: 5

          # Deployment options
          deployment_strategy: rolling
          max_instances_down: 1
          backup_enabled: true
          backup_path: /opt/backups/my-api
```

### Configuration Reference

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `hostname` | string | Required | Server hostname or IP |
| `port` | integer | 22 | SSH port |
| `username` | string | Required | SSH user |
| `ssh_key_path` | string | ~/.ssh/id_rsa | SSH private key path |
| `app_path` | string | /opt/{service} | Application installation directory |
| `service_name` | string | Required | Name for systemd service |
| `systemd_service` | string | {service}.service | Systemd service file name |
| `health_check.type` | string | http | Check type (http, tcp) |
| `health_check.url` | string | http://localhost:8000/health | Health check endpoint |
| `health_check.timeout` | integer | 10 | Timeout in seconds |
| `health_check.max_retries` | integer | 3 | Number of retry attempts |

---

## Step 3: Create Systemd Service

### On Target Server

SSH to each server and create the systemd service file:

```bash
ssh deploy@prod-1.example.com
```

Create `/etc/systemd/system/my-api.service`:

```ini
[Unit]
Description=My API Service
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=deploy
WorkingDirectory=/opt/my-api

# Environment variables
Environment="NODE_ENV=production"
Environment="PORT=8000"
Environment="LOG_LEVEL=info"

# Start command
ExecStart=/opt/my-api/bin/start.sh

# Stop command
ExecStop=/bin/kill -s TERM $MAINPID

# Restart policy
Restart=on-failure
RestartSec=10
StartLimitInterval=60
StartLimitBurst=3

# Resource limits (optional)
MemoryLimit=512M
CPUQuota=50%

[Install]
WantedBy=multi-user.target
```

### Enable and Start Service

```bash
# Reload systemd
sudo systemctl daemon-reload

# Enable service (start on boot)
sudo systemctl enable my-api

# Start service
sudo systemctl start my-api

# Check status
sudo systemctl status my-api

# View logs
sudo journalctl -u my-api -f
```

### Application Startup Script

Create `/opt/my-api/bin/start.sh`:

```bash
#!/bin/bash
set -e

# Source environment
source /opt/my-api/.env

# Navigate to app directory
cd /opt/my-api

# Start application
exec python -m myapp.server
```

Make executable:
```bash
chmod +x /opt/my-api/bin/start.sh
```

---

## Step 4: Deploy

### Prepare Application

The application directory structure should be:

```
/opt/my-api/
├── .env               # Environment variables
├── .git/              # Git repository
├── bin/
│   └── start.sh       # Startup script
├── requirements.txt
├── myapp/
│   └── server.py
└── README.md
```

### Initial Setup

```bash
# On server, initialize application directory
ssh deploy@prod-1.example.com << 'EOF'
mkdir -p /opt/my-api
cd /opt/my-api

# Clone repository
git clone https://github.com/your-org/my-api .

# Install dependencies
pip install -r requirements.txt

# Copy environment file
cp .env.example .env

# Create startup script
mkdir -p bin
cat > bin/start.sh << 'SCRIPT'
#!/bin/bash
cd /opt/my-api
python -m myapp.server
SCRIPT
chmod +x bin/start.sh
EOF
```

### Deploy via Fraisier

```bash
# First deployment
fraisier deploy my_api production

# Output:
# Starting deployment to production (2 servers)...
#
# [1/2] prod-1.example.com
#   ✓ SSH connected
#   ✓ Code pulled (3 commits)
#   ✓ Dependencies installed
#   ✓ Service stopped
#   ✓ Service started
#   ✓ Health checks passed (50ms)
#
# [2/2] prod-2.example.com
#   ✓ SSH connected
#   ✓ Code pulled (3 commits)
#   ✓ Dependencies installed
#   ✓ Service stopped
#   ✓ Service started
#   ✓ Health checks passed (48ms)
#
# ✅ Deployment successful in 45 seconds
```

---

## Step 5: Configure Health Checks

### HTTP Health Check

Your application must respond to health check requests:

**Python Flask Example**:
```python
from flask import Flask, jsonify

app = Flask(__name__)

@app.route('/health')
def health():
    return jsonify({
        'status': 'healthy',
        'version': '2.0.0',
        'timestamp': datetime.utcnow().isoformat()
    }), 200
```

**Node.js Express Example**:
```javascript
app.get('/health', (req, res) => {
  res.json({
    status: 'healthy',
    version: '2.0.0',
    timestamp: new Date().toISOString()
  });
});
```

**Go Example**:
```go
func healthHandler(w http.ResponseWriter, r *http.Request) {
    w.Header().Set("Content-Type", "application/json")
    json.NewEncoder(w).Encode(map[string]interface{}{
        "status": "healthy",
        "version": "2.0.0",
    })
}
```

### TCP Health Check

If HTTP health check is not available:

```yaml
health_check:
  type: tcp
  host: localhost
  port: 8000
  timeout: 5
  max_retries: 3
```

---

## Deployment Strategies

### Rolling Deployment

Deploy one server at a time (default):

```yaml
provider_config:
  deployment_strategy: rolling
  max_instances_down: 1
  health_check_delay: 10
```

**Deployment Flow**:

1. Stop service on server 1
2. Deploy new version
3. Start service and run health checks
4. If healthy, move to server 2
5. Repeat until all servers updated

### Blue-Green Deployment

Switch all servers at once:

```yaml
provider_config:
  deployment_strategy: blue_green
  health_check_delay: 10
```

**Deployment Flow**:

1. Deploy to all servers (without stopping old version)
2. Run health checks on new version
3. If healthy, stop old version and switch traffic
4. Otherwise, rollback and keep old version

---

## Monitoring & Logging

### View Service Logs

```bash
# Follow logs in real-time
ssh deploy@prod-1.example.com "sudo journalctl -u my-api -f"

# Last 100 lines
ssh deploy@prod-1.example.com "sudo journalctl -u my-api -n 100"

# Logs from last hour
ssh deploy@prod-1.example.com "sudo journalctl -u my-api --since '1 hour ago'"

# Errors only
ssh deploy@prod-1.example.com "sudo journalctl -u my-api -p err"
```

### Check Service Status

```bash
# All servers
for host in prod-1 prod-2; do
  echo "=== $host ==="
  ssh deploy@$host.example.com "sudo systemctl status my-api"
done

# Or via Fraisier
fraisier status my_api production
```

### Monitor Deployment

```bash
# Watch deployment progress
fraisier status my_api production --watch

# View deployment history
fraisier history my_api production

# View deployment logs
fraisier logs dep_00001
```

---

## Rollback & Recovery

### Manual Rollback

```bash
# Rollback to previous version
fraisier rollback my_api production

# Rollback to specific version
fraisier rollback my_api production --to-version 1.9.0

# Manually via SSH
ssh deploy@prod-1.example.com << 'EOF'
cd /opt/my-api
git checkout v1.9.0
pip install -r requirements.txt
sudo systemctl restart my-api
EOF
```

### Automatic Rollback on Health Check Failure

```yaml
provider_config:
  health_check:
    auto_rollback_on_failure: true
    rollback_if_errors_exceed: 5  # Rollback if 5+ errors
    check_interval: 60             # Check every 60 seconds
```

---

## Backup & Restore

### Automated Backups

Configure backups before deployment:

```yaml
provider_config:
  backup_enabled: true
  backup_path: /opt/backups/my-api
  backup_retention_days: 30
```

Fraisier will automatically backup before deployment:

```bash
# Manual backup
ssh deploy@prod-1.example.com << 'EOF'
mkdir -p /opt/backups/my-api
cp -r /opt/my-api /opt/backups/my-api/backup-$(date +%Y-%m-%d-%H%M%S)
EOF
```

### Restore from Backup

```bash
ssh deploy@prod-1.example.com << 'EOF'
# List available backups
ls -la /opt/backups/my-api/

# Restore specific backup
rm -rf /opt/my-api
cp -r /opt/backups/my-api/backup-2024-01-22-100000 /opt/my-api

# Restart service
sudo systemctl restart my-api
EOF
```

---

## Scaling

### Add More Servers

Add to `fraises.yaml`:

```yaml
provider_config:
  hosts:
    - hostname: prod-1.example.com
      username: deploy
      ssh_key_path: ~/.ssh/fraisier
    - hostname: prod-2.example.com
      username: deploy
      ssh_key_path: ~/.ssh/fraisier
    - hostname: prod-3.example.com  # New server
      username: deploy
      ssh_key_path: ~/.ssh/fraisier
```

Deploy to new server:

```bash
fraisier deploy my_api production
# Will deploy to all servers, including new one
```

### Remove Server

Simply remove from `fraises.yaml` and deploy:

```bash
fraisier deploy my_api production
# Will only deploy to remaining servers
```

---

## Troubleshooting

### SSH Connection Issues

```bash
# Test SSH connection
ssh -i ~/.ssh/fraisier deploy@prod-1.example.com "echo SSH working"

# Verify key permissions
ls -la ~/.ssh/fraisier
# Should be: -rw------- (600)

# Check server authorized_keys
ssh deploy@prod-1.example.com "cat ~/.ssh/authorized_keys"

# Debug SSH connection
ssh -v -i ~/.ssh/fraisier deploy@prod-1.example.com "echo test"
```

### Service Not Starting

```bash
# Check service status
ssh deploy@prod-1.example.com "sudo systemctl status my-api"

# View error logs
ssh deploy@prod-1.example.com "sudo journalctl -u my-api -n 50 -p err"

# Manually start and check
ssh deploy@prod-1.example.com << 'EOF'
cd /opt/my-api
sudo systemctl stop my-api
sudo -u deploy /opt/my-api/bin/start.sh
EOF
```

### Health Check Failures

```bash
# Test health check manually
ssh deploy@prod-1.example.com "curl http://localhost:8000/health"

# Check if port is listening
ssh deploy@prod-1.example.com "netstat -tlnp | grep 8000"

# View application logs
ssh deploy@prod-1.example.com "sudo journalctl -u my-api -f"

# Check firewall
ssh deploy@prod-1.example.com "sudo ufw status"
```

### Deployment Stuck

```bash
# Check deployment status
fraisier status my_api production

# View logs
fraisier logs dep_00001

# Cancel deployment
fraisier cancel dep_00001

# Force restart service
ssh deploy@prod-1.example.com "sudo systemctl restart my-api"
```

---

## Security Best Practices

### 1. Restrict SSH Key

```bash
# Set proper permissions on SSH key
chmod 600 ~/.ssh/fraisier
chmod 600 ~/.ssh/fraisier.pub

# Create SSH key with passphrase
ssh-keygen -t ed25519 -f ~/.ssh/fraisier -N "your_passphrase"

# Use ssh-agent for passphrase management
ssh-add ~/.ssh/fraisier
```

### 2. Restrict SSH Access

In `/etc/ssh/sshd_config` on target server:

```
# Allow only deploy user
AllowUsers deploy

# Disable password authentication
PasswordAuthentication no
PubkeyAuthentication yes

# Restrict SSH port (optional)
Port 2222
```

### 3. Use SSH Certificates (Optional)

```bash
# Create SSH certificate
ssh-keygen -s ca_key -n deploy ~/.ssh/fraisier.pub

# Deploy certificate
scp ~/.ssh/fraisier-cert.pub deploy@prod-1.example.com:.ssh/
```

### 4. Firewall Configuration

```bash
# Allow SSH from specific IP only
ssh deploy@prod-1.example.com << 'EOF'
sudo ufw allow from 203.0.113.0/24 to any port 22
sudo ufw allow 8000  # Application port
EOF
```

### 5. Service Account Permissions

```bash
# Create dedicated deploy user
ssh deploy@prod-1.example.com << 'EOF'
sudo useradd -m -s /bin/bash deploy
sudo usermod -aG docker deploy  # If using Docker
sudo visudo  # Allow passwordless sudo for systemctl
EOF
```

---

## Advanced Configuration

### Multiple Environments

```yaml
fraises:
  my_api:
    environments:
      staging:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: staging.example.com
          app_path: /opt/my-api-staging

      production:
        provider: bare_metal
        provider_config:
          hosts:
            - hostname: prod-1.example.com
            - hostname: prod-2.example.com
            - hostname: prod-3.example.com
          deployment_strategy: blue_green
```

### Environment-Specific Configuration

```bash
# On server, create environment file
ssh deploy@prod-1.example.com << 'EOF'
cat > /opt/my-api/.env << 'ENVFILE'
NODE_ENV=production
DATABASE_URL=postgresql://user:password@db.example.com/prod
LOG_LEVEL=info
PORT=8000
ENVFILE
EOF
```

### Log Aggregation

```bash
# Configure rsyslog to send logs to central server
ssh deploy@prod-1.example.com << 'EOF'
sudo cat >> /etc/rsyslog.d/my-api.conf << 'CONFIG'
:programname, isequal, "my-api" @@log-server.example.com:514
CONFIG
sudo systemctl restart rsyslog
EOF
```

---

## Production Checklist

- [ ] SSH keys generated and distributed
- [ ] SSH configuration in ~/.ssh/config
- [ ] Application directory created: `/opt/my-api`
- [ ] Git repository cloned
- [ ] Dependencies installed
- [ ] systemd service file created and enabled
- [ ] Health check endpoint implemented
- [ ] Backup strategy configured
- [ ] Firewall rules configured
- [ ] Logging configured
- [ ] First deployment successful
- [ ] Health checks passing
- [ ] Rollback tested

---

## Reference

- [GETTING_STARTED_DOCKER.md](GETTING_STARTED_DOCKER.md) - Docker provider alternative
- [CLI_REFERENCE.md](CLI_REFERENCE.md) - CLI commands
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues
- [systemd Documentation](https://systemd.io/)
