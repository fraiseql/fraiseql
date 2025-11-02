# Extracted from: docs/production/health-checks.md
# Block number: 19
# Liveness: App is running (no external dependencies)
@app.get("/health/live")
async def liveness():
    return {"status": "alive"}


# Readiness: App can serve traffic (check dependencies)
@app.get("/health/ready")
async def readiness():
    return await health.run_checks()
