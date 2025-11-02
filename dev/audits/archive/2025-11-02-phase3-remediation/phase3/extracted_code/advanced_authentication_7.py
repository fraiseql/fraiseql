# Extracted from: docs/advanced/authentication.md
# Block number: 7
import httpx


async def get_management_api_token(domain: str, client_id: str, client_secret: str) -> str:
    """Get Management API access token."""
    async with httpx.AsyncClient() as client:
        response = await client.post(
            f"https://{domain}/oauth/token",
            json={
                "grant_type": "client_credentials",
                "client_id": client_id,
                "client_secret": client_secret,
                "audience": f"https://{domain}/api/v2/",
            },
        )
        return response.json()["access_token"]
