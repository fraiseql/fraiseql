#!/bin/bash
# Fix SSL setup with webroot method (CrowdSec compatible)

set -e

echo "🔐 Setting up SSL with webroot method"
echo "===================================="

WEBSITE_DIR="/var/www/fraiseql.dev"
DOMAIN="fraiseql.dev"

# Create .well-known directory for Let's Encrypt
echo "[1/4] Creating webroot directory..."
sudo mkdir -p $WEBSITE_DIR/.well-known/acme-challenge

# Get certificate using webroot
echo "[2/4] Getting SSL certificate..."
sudo certbot certonly --webroot \
    -w $WEBSITE_DIR \
    -d fraiseql.dev \
    -d www.fraiseql.dev \
    --non-interactive \
    --agree-tos \
    --email lionel.hamayon@evolution-digitale.fr

# Update nginx config to use SSL
echo "[3/4] Updating nginx configuration for SSL..."
cat > /tmp/nginx-fraiseql-ssl.conf << 'EOF'
server {
    listen 80;
    listen [::]:80;
    server_name fraiseql.dev www.fraiseql.dev;

    # Allow Let's Encrypt challenges
    location /.well-known/acme-challenge/ {
        root /var/www/fraiseql.dev;
    }

    # Redirect everything else to HTTPS
    location / {
        return 301 https://$server_name$request_uri;
    }
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name fraiseql.dev www.fraiseql.dev;

    root /var/www/fraiseql.dev;
    index index.html;

    # SSL Configuration
    ssl_certificate /etc/letsencrypt/live/fraiseql.dev/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/fraiseql.dev/privkey.pem;

    # Modern SSL configuration
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers off;

    # HSTS
    add_header Strict-Transport-Security "max-age=63072000" always;

    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "no-referrer-when-downgrade" always;
    add_header Content-Security-Policy "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self';" always;

    # Gzip compression
    gzip on;
    gzip_vary on;
    gzip_min_length 1024;
    gzip_types text/plain text/css text/xml text/javascript application/javascript application/xml+rss application/json;

    # Cache static assets
    location ~* \.(css|js|png|jpg|jpeg|gif|ico|svg|woff|woff2)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }

    # Main site
    location / {
        try_files $uri $uri/ =404;
    }

    # Block access to hidden files
    location ~ /\. {
        deny all;
    }
}
EOF

sudo cp /tmp/nginx-fraiseql-ssl.conf /etc/nginx/sites-available/fraiseql.dev

# Test and reload
echo "[4/4] Testing and reloading nginx..."
if sudo nginx -t; then
    sudo systemctl reload nginx
    echo "✅ SSL setup complete!"
    echo ""
    echo "🌐 Site: https://fraiseql.dev"
    echo "🔒 SSL: Let's Encrypt (webroot method)"
    echo "🛡️  Protection: OPNsense + CrowdSec"
else
    echo "❌ Nginx configuration error!"
    exit 1
fi
