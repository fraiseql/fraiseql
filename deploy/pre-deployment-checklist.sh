#!/bin/bash
# Pre-deployment checklist for fraiseql.dev

echo "🔍 FraiseQL Pre-Deployment Checklist"
echo "===================================="

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Check website files
echo -e "\n${YELLOW}[1/5]${NC} Checking website files..."
if [ -f "website/index.html" ] && [ -f "website/style.css" ]; then
    echo -e "${GREEN}✓${NC} Website files present"
else
    echo -e "${RED}✗${NC} Website files missing!"
    exit 1
fi

# Check nginx configs
echo -e "\n${YELLOW}[2/5]${NC} Checking nginx configurations..."
if [ -f "deploy/nginx-fraiseql-dns4eu.conf" ]; then
    echo -e "${GREEN}✓${NC} Nginx config present"
else
    echo -e "${RED}✗${NC} Nginx config missing!"
    exit 1
fi

# Check deployment scripts
echo -e "\n${YELLOW}[3/5]${NC} Checking deployment scripts..."
if [ -x "deploy/deploy-direct.sh" ] && [ -x "deploy/check-dns.sh" ]; then
    echo -e "${GREEN}✓${NC} Scripts are executable"
else
    echo -e "${RED}✗${NC} Scripts not executable!"
    exit 1
fi

# Test SSH connection
echo -e "\n${YELLOW}[4/5]${NC} Testing SSH connection to RNSWEB01p..."
if ssh -o ConnectTimeout=5 lionel@RNSWEB01p "echo 'Connected'" &>/dev/null; then
    echo -e "${GREEN}✓${NC} SSH connection successful"
else
    echo -e "${RED}✗${NC} Cannot connect to server!"
    exit 1
fi

# Check DNS (might not be ready yet)
echo -e "\n${YELLOW}[5/5]${NC} Checking DNS propagation..."
if dig +short fraiseql.dev A | grep -q "82.66.42.150"; then
    echo -e "${GREEN}✓${NC} DNS is propagated!"
else
    echo -e "${YELLOW}⏳${NC} DNS not propagated yet (this is normal if just configured)"
fi

echo -e "\n${GREEN}Ready for deployment!${NC}"
echo "===================================="
echo "1. Configure DNS at Alwaysdata (see alwaysdata-dns-setup.md)"
echo "2. Wait for DNS propagation (run ./deploy/check-dns.sh)"
echo "3. Deploy with: ./deploy/deploy-direct.sh"
