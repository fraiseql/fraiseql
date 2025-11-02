# Extracted from: docs/production/health-checks.md
# Block number: 22
# ❌ Bad: Health check runs on every GraphQL request
@app.middleware("http")
async def health_middleware(request, call_next):
    await health.run_checks()  # Expensive!
    return await call_next(request)


# ✅ Good: Dedicated health endpoint
@app.get("/health")
async def health_endpoint():
    return await health.run_checks()
