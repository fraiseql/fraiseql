#!/bin/bash
# Direct deployment to RNSWEB01p - No CDN, No Compromise!

set -e

echo "🚀 FraiseQL Direct Deployment - Going Full Metal!"
echo "================================================"

REMOTE_HOST="RNSWEB01p"
REMOTE_USER="lionel"
WEBSITE_DIR="/var/www/fraiseql.dev"
NGINX_SITE="fraiseql.dev"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}[1/6]${NC} Creating website structure on server..."
ssh -t $REMOTE_USER@$REMOTE_HOST "sudo mkdir -p $WEBSITE_DIR"

echo -e "${YELLOW}[2/6]${NC} Deploying website files..."
rsync -avz --delete \
    --exclude '.git' \
    --exclude '.DS_Store' \
    --exclude 'node_modules' \
    ./website/ $REMOTE_USER@$REMOTE_HOST:$WEBSITE_DIR/

echo -e "${YELLOW}[3/6]${NC} Setting permissions..."
ssh $REMOTE_USER@$REMOTE_HOST "sudo chown -R www-data:www-data $WEBSITE_DIR && sudo chmod -R 755 $WEBSITE_DIR"

echo -e "${YELLOW}[4/6]${NC} Copying nginx configuration..."
scp ./deploy/nginx-fraiseql-dns4eu.conf $REMOTE_USER@$REMOTE_HOST:/tmp/

ssh $REMOTE_USER@$REMOTE_HOST << 'EOF'
    sudo cp /tmp/nginx-fraiseql-dns4eu.conf /etc/nginx/sites-available/fraiseql.dev
    sudo ln -sf /etc/nginx/sites-available/fraiseql.dev /etc/nginx/sites-enabled/

    # Test nginx config
    if sudo nginx -t; then
        echo "✅ Nginx configuration valid"
    else
        echo "❌ Nginx configuration error!"
        exit 1
    fi
EOF

echo -e "${YELLOW}[5/6]${NC} Setting up Let's Encrypt SSL..."
ssh $REMOTE_USER@$REMOTE_HOST << 'EOF'
    # Check if certbot is installed
    if ! command -v certbot &> /dev/null; then
        echo "Installing certbot..."
        sudo apt update && sudo apt install -y certbot python3-certbot-nginx
    fi

    # Get certificate (will modify nginx config automatically)
    sudo certbot --nginx -d fraiseql.dev -d www.fraiseql.dev --non-interactive --agree-tos --email lionel.hamayon@evolution-digitale.fr
EOF

echo -e "${YELLOW}[6/6]${NC} Reloading services..."
ssh $REMOTE_USER@$REMOTE_HOST "sudo systemctl reload nginx"

echo -e "${GREEN}✨ Deployment Complete!${NC}"
echo "============================="
echo "🌐 Site: https://fraiseql.dev"
echo "🔒 SSL: Let's Encrypt (auto-renew enabled)"
echo "🛡️  Protection: OPNsense + CrowdSec"
echo "🚀 Server: Direct connection, no CDN"
echo ""
echo "Test with: curl -I https://fraiseql.dev"
