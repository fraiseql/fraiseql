# Extracted from: docs/advanced/authentication.md
# Block number: 20
from fastapi import APIRouter, Header, HTTPException

from fraiseql.auth import AuthenticationError

router = APIRouter()


@router.post("/logout")
async def logout(authorization: str = Header(...)):
    """Logout current session."""
    try:
        # Extract token
        token = authorization.replace("Bearer ", "")

        # Validate and decode
        payload = await auth_provider.validate_token(token)

        # Revoke token
        await auth_provider.logout(payload)

        return {"message": "Logged out successfully"}

    except AuthenticationError:
        raise HTTPException(status_code=401, detail="Invalid token")


@router.post("/logout-all")
async def logout_all_sessions(authorization: str = Header(...)):
    """Logout all sessions for current user."""
    try:
        token = authorization.replace("Bearer ", "")
        payload = await auth_provider.validate_token(token)
        user_id = payload["sub"]

        # Revoke all user tokens
        await auth_provider.logout_all_sessions(user_id)

        return {"message": "All sessions logged out"}

    except AuthenticationError:
        raise HTTPException(status_code=401, detail="Invalid token")
