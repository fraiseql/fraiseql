#!/bin/bash
# Local deployment script - Run this directly on RNSWEB01p

set -e

echo "🚀 FraiseQL Local Deployment"
echo "==========================="

WEBSITE_DIR="/var/www/fraiseql.dev"
NGINX_SITE="fraiseql.dev"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}[1/6]${NC} Creating website structure..."
sudo mkdir -p $WEBSITE_DIR

echo -e "${YELLOW}[2/6]${NC} Copying website files..."
sudo cp -r ~/website/* $WEBSITE_DIR/

echo -e "${YELLOW}[3/6]${NC} Setting permissions..."
sudo chown -R www-data:www-data $WEBSITE_DIR
sudo chmod -R 755 $WEBSITE_DIR

echo -e "${YELLOW}[4/6]${NC} Installing nginx configuration..."
sudo cp ~/nginx-fraiseql.conf /etc/nginx/sites-available/fraiseql.dev
sudo ln -sf /etc/nginx/sites-available/fraiseql.dev /etc/nginx/sites-enabled/

# Test nginx config
echo -e "${YELLOW}[5/6]${NC} Testing nginx configuration..."
if sudo nginx -t; then
    echo "✅ Nginx configuration valid"
else
    echo "❌ Nginx configuration error!"
    exit 1
fi

echo -e "${YELLOW}[6/6]${NC} Setting up Let's Encrypt SSL..."
# Check if certbot is installed
if ! command -v certbot &> /dev/null; then
    echo "Installing certbot..."
    sudo apt update && sudo apt install -y certbot python3-certbot-nginx
fi

# Get certificate
sudo certbot --nginx -d fraiseql.dev -d www.fraiseql.dev --non-interactive --agree-tos --email lionel.hamayon@evolution-digitale.fr

# Reload nginx
sudo systemctl reload nginx

echo -e "${GREEN}✨ Deployment Complete!${NC}"
echo "============================="
echo "🌐 Site: https://fraiseql.dev"
echo "🔒 SSL: Let's Encrypt (auto-renew enabled)"
echo "🛡️  Protection: OPNsense + CrowdSec"
echo "🚀 Server: Direct connection, no CDN"
echo ""
echo "Test with: curl -I https://fraiseql.dev"
