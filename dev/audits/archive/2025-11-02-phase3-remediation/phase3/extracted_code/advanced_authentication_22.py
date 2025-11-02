# Extracted from: docs/advanced/authentication.md
# Block number: 22
from starlette.middleware.sessions import SessionMiddleware

app.add_middleware(
    SessionMiddleware,
    secret_key="your-session-secret-key",
    session_cookie="fraiseql_session",
    max_age=86400,  # 24 hours
    same_site="lax",
    https_only=True,  # Production only
)
