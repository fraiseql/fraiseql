# FraiseQL Admin Dashboard
# System monitoring and debugging interface

FROM node:20-alpine AS builder

WORKDIR /app

COPY admin-dashboard/package.json .

RUN npm ci --only=production

# ============================================================================
# RUNTIME
# ============================================================================
FROM node:20-alpine

WORKDIR /app

# Install curl for health checks
RUN apk add --no-cache curl

# Copy node_modules from builder
COPY --from=builder /app/node_modules ./node_modules

# Copy application files
COPY admin-dashboard/src ./src
COPY admin-dashboard/public ./public

# Expose admin port
EXPOSE 3002

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3002/health || exit 1

# Environment defaults
ENV NODE_ENV=production
ENV ADMIN_PORT=3002
ENV FRAISEQL_API_URL=http://fraiseql-server:8000

# Start admin dashboard
CMD ["node", "src/server.js"]
