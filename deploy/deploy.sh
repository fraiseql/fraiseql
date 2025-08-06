#!/bin/bash
# Deploy script for fraiseql.dev to RNSWEB01p

REMOTE_HOST="RNSWEB01p"
REMOTE_USER="lionel"
REMOTE_DIR="/var/www/fraiseql.dev"
LOCAL_DIR="./website"

echo "🚀 Deploying fraiseql.dev to RNSWEB01p..."

# Create remote directory if it doesn't exist
ssh $REMOTE_USER@$REMOTE_HOST "mkdir -p $REMOTE_DIR"

# Sync files (delete removed files, preserve permissions)
rsync -avz --delete \
    --exclude '.git' \
    --exclude '.DS_Store' \
    --exclude '*.log' \
    $LOCAL_DIR/ $REMOTE_USER@$REMOTE_HOST:$REMOTE_DIR/

# Set correct permissions
ssh $REMOTE_USER@$REMOTE_HOST "chmod -R 755 $REMOTE_DIR"

# Reload nginx
ssh $REMOTE_USER@$REMOTE_HOST "sudo nginx -t && sudo systemctl reload nginx"

echo "✅ Deployment complete!"
echo "🌐 Visit https://fraiseql.dev"
