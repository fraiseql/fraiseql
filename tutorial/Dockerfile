# FraiseQL Interactive Tutorial Server
# Serves interactive learning platform for newcomers

FROM node:20-alpine AS builder

WORKDIR /app

# Copy package files
COPY tutorial/package.json ./

# Install dependencies
RUN npm install --omit=dev

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
COPY tutorial/src ./src
COPY tutorial/web ./web
COPY tutorial/assets ./assets

# Expose tutorial port
EXPOSE 3001

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3001/health || exit 1

# Environment defaults
ENV NODE_ENV=production
ENV TUTORIAL_PORT=3001
ENV FRAISEQL_API_URL=http://fraiseql-server:8000

# Start tutorial server
CMD ["node", "src/server.js"]
