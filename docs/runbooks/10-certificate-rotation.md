# Runbook: Certificate Rotation (TLS/SSL Certificate Renewal)

## Symptoms

- TLS certificate expiry warning from monitoring (< 30 days)
- Browsers showing certificate expiry error: `SSL_ERROR_RX_RECORD_TOO_LONG`
- Clients receiving certificate chain validation failures
- Health check endpoint unreachable via HTTPS
- FraiseQL not accepting TLS connections (port 443 timeout)
- Certificate validation errors: `certificate expired` or `not yet valid`
- OIDC provider cannot reach FraiseQL callback URL (TLS issues)
- Vault connectivity issues due to self-signed cert

## Impact

- **High**: HTTPS connections fail (service unavailable over SSL/TLS)
- Clients cannot establish secure connections
- Webhooks cannot be signed/verified with valid cert
- Health checks may fail if they require valid TLS
- OIDC/OAuth flows break if callback URL unreachable

## Investigation

### 1. Certificate Status

```bash
# Check certificate expiry
openssl x509 -in /etc/fraiseql/certs/server.crt -text -noout | grep -A2 "Validity"

# Get expiry date in seconds
EXPIRY=$(openssl x509 -in /etc/fraiseql/certs/server.crt -noout -enddate | cut -d'=' -f2)
EXPIRY_EPOCH=$(date -d "$EXPIRY" +%s)
NOW_EPOCH=$(date +%s)
DAYS_LEFT=$(( (EXPIRY_EPOCH - NOW_EPOCH) / 86400 ))
echo "Days until expiry: $DAYS_LEFT"

# If negative, certificate is already expired

# Check certificate details
openssl x509 -in /etc/fraiseql/certs/server.crt -text -noout | head -30

# Check certificate subject and CN
openssl x509 -in /etc/fraiseql/certs/server.crt -noout -subject

# Check certificate SANs (Subject Alternative Names)
openssl x509 -in /etc/fraiseql/certs/server.crt -noout -text | grep -A1 "Subject Alternative Name"
```

### 2. Certificate Chain Validation

```bash
# Check if certificate chain is complete
openssl x509 -in /etc/fraiseql/certs/server.crt -noout -issuer

# View full chain file
if [ -f /etc/fraiseql/certs/chain.pem ]; then
    openssl crl2pkcs7 -nocrl -certfile /etc/fraiseql/certs/chain.pem | openssl pkcs7 -print_certs -text -noout
fi

# Verify chain
openssl verify -CAfile /etc/fraiseql/certs/ca-bundle.crt /etc/fraiseql/certs/server.crt

# Test TLS handshake from client
openssl s_client -connect localhost:8815 < /dev/null

# Check for certificate warnings in output
# "Verify return code" should be 0 (ok)
```

### 3. FraiseQL TLS Configuration

```bash
# Check TLS certificate path in environment
env | grep -E "TLS|CERT|SSL"

# Check if TLS is enabled
curl -v https://localhost:8815/health 2>&1 | head -20

# Check FraiseQL logs for TLS errors
docker logs fraiseql-server | grep -i "tls\|ssl\|certificate" | tail -20

# Check if FraiseQL has keys and certs
ls -la /etc/fraiseql/certs/
```

### 4. Kubernetes Certificate (if running in K8s)

```bash
# If using Kubernetes secrets for certificates
kubectl get secrets -n fraiseql fraiseql-tls -o yaml | grep -A5 "tls.crt"

# Check expiry in K8s secret
kubectl get secret fraiseql-tls -n fraiseql -o jsonpath='{.data.tls\.crt}' | base64 -d | openssl x509 -noout -enddate

# Check certificate in TLS ingress
kubectl get ingress fraiseql -n fraiseql -o yaml | grep -A10 "tls:"
```

### 5. Certificate Provider Status

```bash
# If using Let's Encrypt via cert-bot
certbot certificates 2>/dev/null | grep -A5 "fraiseql"

# Check Certbot renewal job
systemctl status certbot.timer || echo "Certbot timer not running"

# Check renewal logs
tail -20 /var/log/letsencrypt/letsencrypt.log

# Manual renewal dry-run
certbot renew --dry-run

# If using AWS Certificate Manager
aws acm describe-certificate --certificate-arn arn:aws:acm:... --region us-east-1

# If using Vault for certificate generation
curl -s -H "X-Vault-Token: $VAULT_TOKEN" \
     "$VAULT_ADDR/v1/pki/certs" | jq '.data.keys'
```

## Mitigation

### For Expiring Certificate (< 30 days)

1. **Initiate certificate renewal process**
   ```bash
   # For Let's Encrypt (certbot)
   certbot renew --force-renewal

   # Manual renewal
   certbot certonly --standalone -d fraiseql.example.com

   # Verify renewal succeeded
   openssl x509 -in /etc/letsencrypt/live/fraiseql.example.com/cert.pem -noout -enddate
   ```

2. **Request certificate from provider**
   ```bash
   # AWS Certificate Manager
   aws acm request-certificate \
     --domain-name fraiseql.example.com \
     --domain-name "*.fraiseql.example.com" \
     --validation-method DNS

   # DigiCert/other provider: Submit CSR
   openssl req -new -newkey rsa:2048 -nodes \
     -out fraiseql.csr -keyout fraiseql.key \
     -subj "/CN=fraiseql.example.com"

   # Upload CSR to provider, get signed certificate
   ```

3. **Install new certificate**
   ```bash
   # Backup old certificate
   cp /etc/fraiseql/certs/server.crt /etc/fraiseql/certs/server.crt.backup-$(date +%s)
   cp /etc/fraiseql/certs/server.key /etc/fraiseql/certs/server.key.backup-$(date +%s)

   # Copy new certificate
   cp new-cert.crt /etc/fraiseql/certs/server.crt
   cp new-key.key /etc/fraiseql/certs/server.key
   cp chain.pem /etc/fraiseql/certs/chain.pem

   # Fix permissions
   chmod 644 /etc/fraiseql/certs/server.crt
   chmod 600 /etc/fraiseql/certs/server.key

   # Restart FraiseQL to load new certificate
   docker restart fraiseql-server
   sleep 5

   # Verify new certificate
   openssl x509 -in /etc/fraiseql/certs/server.crt -noout -enddate
   ```

### For Already-Expired Certificate

4. **Emergency certificate renewal** (if expired)
   ```bash
   # For Let's Encrypt (if using)
   sudo systemctl stop nginx  # or whatever is binding port 80/443
   certbot certonly --standalone -d fraiseql.example.com

   # For self-signed (temporary, only for testing)
   openssl req -x509 -newkey rsa:4096 -keyout server.key -out server.crt \
     -days 365 -nodes \
     -subj "/CN=fraiseql.example.com"

   # Install and restart
   cp server.crt /etc/fraiseql/certs/
   cp server.key /etc/fraiseql/certs/
   docker restart fraiseql-server
   ```

### For Kubernetes/Cloud Environment

5. **Update Kubernetes secret**
   ```bash
   # Generate new certificate (if self-signed for testing)
   openssl req -x509 -newkey rsa:4096 -keyout tls.key -out tls.crt \
     -days 365 -nodes \
     -subj "/CN=fraiseql.example.com"

   # Update K8s secret
   kubectl delete secret fraiseql-tls -n fraiseql
   kubectl create secret tls fraiseql-tls \
     --cert=tls.crt \
     --key=tls.key \
     -n fraiseql

   # Restart deployment to load new secret
   kubectl rollout restart deployment fraiseql-server -n fraiseql
   kubectl rollout status deployment fraiseql-server -n fraiseql
   ```

6. **Update AWS/Cloud provider certificates**
   ```bash
   # AWS Load Balancer
   aws elbv2 modify-listener \
     --listener-arn arn:aws:elasticloadbalancing:... \
     --certificates CertificateArn=arn:aws:acm:...

   # Azure Application Gateway
   az network application-gateway ssl-cert update \
     --resource-group myRG \
     --gateway-name myAppGW \
     --name fraiseql-cert \
     --cert-file new-cert.pfx \
     --cert-password password
   ```

## Resolution

### Complete Certificate Rotation Workflow

```bash
#!/bin/bash
set -e

echo "=== Certificate Rotation ==="

DOMAIN="fraiseql.example.com"
CERT_PATH="/etc/fraiseql/certs"

# 1. Check current certificate
echo "1. Current certificate status:"
openssl x509 -in $CERT_PATH/server.crt -noout -enddate
EXPIRY=$(openssl x509 -in $CERT_PATH/server.crt -noout -enddate | cut -d'=' -f2)
EXPIRY_EPOCH=$(date -d "$EXPIRY" +%s)
NOW_EPOCH=$(date +%s)
DAYS_LEFT=$(( (EXPIRY_EPOCH - NOW_EPOCH) / 86400 ))
echo "   Days remaining: $DAYS_LEFT"

# 2. Request new certificate
echo ""
echo "2. Requesting new certificate..."

# For Let's Encrypt
if command -v certbot &> /dev/null; then
    echo "   Using certbot..."
    certbot renew --force-renewal --non-interactive --agree-tos \
      -d $DOMAIN -d "*.$DOMAIN" 2>&1 | tail -5

    # Copy from certbot location
    RENEWED_CERT="/etc/letsencrypt/live/$DOMAIN/fullchain.pem"
    RENEWED_KEY="/etc/letsencrypt/live/$DOMAIN/privkey.pem"

    if [ ! -f "$RENEWED_CERT" ]; then
        echo "   ✗ Certificate renewal failed"
        exit 1
    fi
else
    echo "   ! Certbot not found, using manual process"
    echo "   Please obtain new certificate manually and place in:"
    echo "   - $CERT_PATH/server.crt"
    echo "   - $CERT_PATH/server.key"
    exit 1
fi

# 3. Backup old certificates
echo ""
echo "3. Backing up old certificates..."
BACKUP_TS=$(date +%s)
cp $CERT_PATH/server.crt $CERT_PATH/server.crt.backup-$BACKUP_TS
cp $CERT_PATH/server.key $CERT_PATH/server.key.backup-$BACKUP_TS
echo "   ✓ Backup: $CERT_PATH/server.crt.backup-$BACKUP_TS"

# 4. Install new certificates
echo ""
echo "4. Installing new certificates..."
cp $RENEWED_CERT $CERT_PATH/server.crt
cp $RENEWED_KEY $CERT_PATH/server.key
chmod 644 $CERT_PATH/server.crt
chmod 600 $CERT_PATH/server.key
echo "   ✓ Certificates installed"

# 5. Verify new certificate
echo ""
echo "5. Verifying new certificate..."
openssl x509 -in $CERT_PATH/server.crt -noout -text | head -20
NEW_EXPIRY=$(openssl x509 -in $CERT_PATH/server.crt -noout -enddate | cut -d'=' -f2)
echo "   New expiry: $NEW_EXPIRY"

# 6. Restart FraiseQL
echo ""
echo "6. Restarting FraiseQL..."
docker restart fraiseql-server
sleep 5

# 7. Verify TLS is working
echo ""
echo "7. Verifying TLS connection..."
RESULT=$(openssl s_client -connect localhost:8815 < /dev/null 2>&1)
if echo "$RESULT" | grep -q "Verify return code: 0"; then
    echo "   ✓ TLS certificate valid"
else
    echo "   ✗ TLS validation failed"
    echo "$RESULT" | grep "Verify return code"
    exit 1
fi

# 8. Health check
echo ""
echo "8. Health check..."
if curl -s https://localhost:8815/health -k | jq -e '.status == "healthy"' > /dev/null; then
    echo "   ✓ Service is healthy"
else
    echo "   ✗ Service health check failed"
    exit 1
fi

echo ""
echo "✓ Certificate rotation complete"
```

### Manual Certificate Generation (for testing/self-signed)

```bash
# Generate private key
openssl genrsa -out server.key 2048

# Generate CSR
openssl req -new -key server.key -out server.csr \
  -subj "/C=US/ST=CA/L=SF/O=Company/CN=fraiseql.example.com"

# Generate self-signed cert (365 days)
openssl x509 -req -days 365 -in server.csr \
  -signkey server.key -out server.crt

# Or generate in one step (self-signed)
openssl req -x509 -newkey rsa:2048 -keyout server.key -out server.crt \
  -days 365 -nodes \
  -subj "/CN=fraiseql.example.com" \
  -addext "subjectAltName=DNS:fraiseql.example.com,DNS:*.fraiseql.example.com"

# Install
cp server.crt /etc/fraiseql/certs/
cp server.key /etc/fraiseql/certs/
chmod 644 /etc/fraiseql/certs/server.crt
chmod 600 /etc/fraiseql/certs/server.key
```

## Prevention

### Automated Certificate Renewal

```bash
# For Let's Encrypt with Certbot
# 1. Install and configure certbot
sudo apt-get install certbot

# 2. Set up renewal (systemd timer)
sudo systemctl enable certbot.timer
sudo systemctl start certbot.timer

# 3. Verify timer is active
sudo systemctl status certbot.timer

# 4. Test renewal
sudo certbot renew --dry-run

# 5. On renewal, hook to restart FraiseQL
# Create hook script: /usr/local/bin/fraiseql-cert-hook.sh
cat > /usr/local/bin/fraiseql-cert-hook.sh << 'EOF'
#!/bin/bash
cp /etc/letsencrypt/live/fraiseql.example.com/fullchain.pem /etc/fraiseql/certs/server.crt
cp /etc/letsencrypt/live/fraiseql.example.com/privkey.pem /etc/fraiseql/certs/server.key
docker restart fraiseql-server
EOF

chmod +x /usr/local/bin/fraiseql-cert-hook.sh

# 6. Configure certbot to use hook
# In /etc/letsencrypt/renewal/fraiseql.example.com.conf
# renew_hook = /usr/local/bin/fraiseql-cert-hook.sh
```

### Monitoring and Alerting

```bash
# Prometheus alerts for certificate expiry
cat > /etc/prometheus/rules/fraiseql-certs.yml << 'EOF'
groups:
  - name: fraiseql_certificates
    rules:
      - alert: CertificateExpiryWarning
        expr: days_until_cert_expiry < 30
        for: 1h
        action: notify

      - alert: CertificateExpiryUrgent
        expr: days_until_cert_expiry < 7
        for: 15m
        action: page

      - alert: CertificateAlreadyExpired
        expr: days_until_cert_expiry < 0
        for: 1m
        action: page
EOF

# Script to export cert expiry metric
cat > /usr/local/bin/cert-metrics.sh << 'EOF'
#!/bin/bash
# Export as Prometheus metric
CERT_FILE="/etc/fraiseql/certs/server.crt"
EXPIRY=$(openssl x509 -in $CERT_FILE -noout -enddate | cut -d'=' -f2)
EXPIRY_EPOCH=$(date -d "$EXPIRY" +%s)
NOW_EPOCH=$(date +%s)
DAYS_LEFT=$(( (EXPIRY_EPOCH - NOW_EPOCH) / 86400 ))
echo "days_until_cert_expiry $DAYS_LEFT"
EOF

# Run as cron job hourly
0 * * * * /usr/local/bin/cert-metrics.sh > /var/lib/node_exporter/textfile_collector/certs.prom
```

### Quarterly Certificate Review

```bash
# List all certificates and expiry dates
for cert_file in /etc/fraiseql/certs/*.crt; do
    echo "=== $cert_file ==="
    openssl x509 -in "$cert_file" -noout -subject -enddate
done

# Check expiry of all endpoints
for endpoint in localhost:8815 fraiseql.example.com:443; do
    echo "=== $endpoint ==="
    echo | openssl s_client -connect $endpoint -servername ${endpoint/:*/} 2>/dev/null | \
      openssl x509 -noout -subject -enddate
done
```

## Escalation

- **Certificate generation issues**: DevOps / Infrastructure team
- **DNS validation issues (Let's Encrypt)**: DNS / Network team
- **Cloud provider certificate issues**: Cloud team (AWS/Azure/GCP)
- **Vault certificate generation issues**: Security / Vault team
- **Emergency expired certificate**: Incident commander + all above teams
