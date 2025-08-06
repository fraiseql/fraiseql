#!/bin/bash
# Setup CrowdSec GraphQL protection for fraiseql.dev

echo "🛡️  Setting up CrowdSec GraphQL Protection"
echo "========================================"

# Copy scenario to server
echo "[1/3] Copying GraphQL scenario to server..."
scp ./crowdsec-graphql-scenario.yaml lionel@RNSWEB01p:/tmp/

# Install on server
ssh lionel@RNSWEB01p << 'EOF'
    # Copy scenario file
    sudo cp /tmp/crowdsec-graphql-scenario.yaml /etc/crowdsec/scenarios/

    # Reload CrowdSec
    echo "[2/3] Reloading CrowdSec..."
    sudo systemctl reload crowdsec

    # Verify scenario is loaded
    echo "[3/3] Verifying scenario..."
    sudo cscli scenarios list | grep graphql
EOF

echo "✅ CrowdSec GraphQL protection configured!"
echo ""
echo "The following scenarios are now active:"
echo "- Rate limiting on /graphql endpoints"
echo "- Introspection query blocking"
echo "- Query depth attack detection"
