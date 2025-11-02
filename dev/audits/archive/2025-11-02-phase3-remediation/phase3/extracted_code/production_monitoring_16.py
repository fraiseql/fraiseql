# Extracted from: docs/production/monitoring.md
# Block number: 16
import sentry_sdk

# Initialize Sentry
sentry_sdk.init(
    dsn=os.getenv("SENTRY_DSN"),
    environment="production",
    traces_sample_rate=0.1,  # 10% of traces
    profiles_sample_rate=0.1,
    release=f"fraiseql@{VERSION}",
)


# In GraphQL context
@app.middleware("http")
async def sentry_middleware(request: Request, call_next):
    # Set user context
    if hasattr(request.state, "user"):
        user = request.state.user
        sentry_sdk.set_user({"id": user.user_id, "email": user.email, "username": user.name})

    # Set GraphQL context
    if request.url.path == "/graphql":
        query = await request.body()
        sentry_sdk.set_context(
            "graphql",
            {
                "query": query.decode()[:1000],  # Limit size
                "operation": request.headers.get("X-Operation-Name"),
            },
        )

    response = await call_next(request)
    return response
