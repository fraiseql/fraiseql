"""HTTP client for Coolify API integration.

Handles authenticated requests to Coolify API with:
- Token-based authentication
- Error handling and retries
- Response parsing
- Timeout management
"""

from typing import Any

try:
    import requests
    from requests.adapters import HTTPAdapter
    from urllib3.util.retry import Retry
except ImportError as e:
    raise ImportError(
        "requests library is required for Coolify provider. "
        "Install with: pip install requests"
    ) from e


class CoolifyAPIError(Exception):
    """Base exception for Coolify API errors."""

    pass


class CoolifyAuthError(CoolifyAPIError):
    """Exception for authentication failures."""

    pass


class CoolifyNotFoundError(CoolifyAPIError):
    """Exception for resource not found errors."""

    pass


class CoolifyTimeoutError(CoolifyAPIError):
    """Exception for API timeout errors."""

    pass


class CoolifyClient:
    """HTTP client for Coolify API.

    Provides authenticated access to Coolify REST API with automatic
    retry logic, timeout handling, and error management.

    Args:
        base_url: Coolify instance URL (e.g., "https://coolify.example.com")
        api_key: Coolify API key for authentication
        timeout: Request timeout in seconds (default: 30)
        max_retries: Maximum number of retries (default: 3)

    Example:
        client = CoolifyClient("https://coolify.example.com", "api_key_xyz")
        app = client.get_application("app_id")
        result = client.deploy_application("app_id", {"tag": "v1.0.0"})
    """

    def __init__(
        self,
        base_url: str,
        api_key: str,
        timeout: int = 30,
        max_retries: int = 3,
    ):
        """Initialize Coolify API client.

        Args:
            base_url: Base URL of Coolify instance
            api_key: API key for authentication
            timeout: Request timeout in seconds
            max_retries: Maximum retry attempts

        Raises:
            ValueError: If base_url or api_key is empty
        """
        if not base_url or not api_key:
            raise ValueError("base_url and api_key are required")

        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.timeout = timeout
        self.max_retries = max_retries

        # Configure session with retry strategy
        self.session = requests.Session()
        self._configure_retry_strategy()

    def _configure_retry_strategy(self) -> None:
        """Configure automatic retry strategy for failed requests."""
        retry_strategy = Retry(
            total=self.max_retries,
            status_forcelist=[429, 500, 502, 503, 504],
            allowed_methods=["GET", "POST", "PUT", "DELETE"],
            backoff_factor=1,
        )

        adapter = HTTPAdapter(max_retries=retry_strategy)
        self.session.mount("http://", adapter)
        self.session.mount("https://", adapter)

    def _get_headers(self) -> dict[str, str]:
        """Get request headers with authentication.

        Returns:
            Dict with authorization and content-type headers
        """
        return {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
            "Accept": "application/json",
        }

    def _make_request(
        self,
        method: str,
        endpoint: str,
        data: dict[str, Any] | None = None,
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Make HTTP request to Coolify API.

        Args:
            method: HTTP method (GET, POST, PUT, DELETE)
            endpoint: API endpoint path (without base URL)
            data: Request body data (for POST/PUT)
            params: Query parameters

        Returns:
            Parsed JSON response

        Raises:
            CoolifyAuthError: If authentication fails (401)
            CoolifyNotFoundError: If resource not found (404)
            CoolifyTimeoutError: If request times out
            CoolifyAPIError: For other API errors
        """
        url = f"{self.base_url}/api/{endpoint.lstrip('/')}"
        headers = self._get_headers()

        try:
            response = self.session.request(
                method,
                url,
                headers=headers,
                json=data,
                params=params,
                timeout=self.timeout,
            )

            # Handle authentication errors
            if response.status_code == 401:
                raise CoolifyAuthError("Authentication failed: Invalid API key")

            # Handle not found errors
            if response.status_code == 404:
                raise CoolifyNotFoundError(
                    f"Resource not found: {endpoint}"
                )

            # Handle other HTTP errors
            if response.status_code >= 400:
                try:
                    error_data = response.json()
                    error_msg = error_data.get("message", response.text)
                except Exception:
                    error_msg = response.text

                raise CoolifyAPIError(
                    f"API error ({response.status_code}): {error_msg}"
                )

            # Parse successful response
            return response.json() if response.text else {}

        except requests.Timeout as e:
            raise CoolifyTimeoutError(f"Request timeout after {self.timeout}s") from e
        except requests.RequestException as e:
            raise CoolifyAPIError(f"Request error: {str(e)}") from e

    def get_application(self, application_id: str) -> dict[str, Any]:
        """Get application details from Coolify.

        Args:
            application_id: UUID of the application

        Returns:
            Application details dict

        Raises:
            CoolifyNotFoundError: If application not found
            CoolifyAPIError: For other API errors
        """
        return self._make_request("GET", f"v1/applications/{application_id}")

    def list_applications(self, project_id: str) -> list[dict[str, Any]]:
        """List applications in a project.

        Args:
            project_id: UUID of the project

        Returns:
            List of application details

        Raises:
            CoolifyAPIError: For API errors
        """
        response = self._make_request(
            "GET",
            "v1/applications",
            params={"projectId": project_id},
        )
        return response.get("applications", [])

    def deploy_application(
        self,
        application_id: str,
        deploy_data: dict[str, Any],
    ) -> dict[str, Any]:
        """Trigger deployment of an application.

        Args:
            application_id: UUID of the application
            deploy_data: Deployment configuration (tag, env vars, etc.)

        Returns:
            Deployment status dict

        Raises:
            CoolifyAPIError: For API errors
        """
        return self._make_request(
            "POST",
            f"v1/applications/{application_id}/deploy",
            data=deploy_data,
        )

    def get_deployment_status(
        self,
        application_id: str,
        deployment_id: str,
    ) -> dict[str, Any]:
        """Get deployment status.

        Args:
            application_id: UUID of the application
            deployment_id: UUID of the deployment

        Returns:
            Deployment status dict with status, progress, logs, etc.

        Raises:
            CoolifyNotFoundError: If deployment not found
            CoolifyAPIError: For other API errors
        """
        return self._make_request(
            "GET",
            f"v1/applications/{application_id}/deployments/{deployment_id}",
        )

    def get_application_logs(
        self,
        application_id: str,
        lines: int = 100,
    ) -> str:
        """Get application logs.

        Args:
            application_id: UUID of the application
            lines: Number of log lines to retrieve

        Returns:
            Log output as string

        Raises:
            CoolifyAPIError: For API errors
        """
        response = self._make_request(
            "GET",
            f"v1/applications/{application_id}/logs",
            params={"lines": lines},
        )
        return response.get("logs", "")

    def update_application_config(
        self,
        application_id: str,
        config: dict[str, Any],
    ) -> dict[str, Any]:
        """Update application configuration.

        Args:
            application_id: UUID of the application
            config: Configuration to update (env vars, ports, etc.)

        Returns:
            Updated application config

        Raises:
            CoolifyAPIError: For API errors
        """
        return self._make_request(
            "PUT",
            f"v1/applications/{application_id}",
            data=config,
        )

    def get_application_status(
        self,
        application_id: str,
    ) -> dict[str, Any]:
        """Get application health status.

        Args:
            application_id: UUID of the application

        Returns:
            Status dict with status, uptime, container info, etc.

        Raises:
            CoolifyAPIError: For API errors
        """
        return self._make_request(
            "GET",
            f"v1/applications/{application_id}/status",
        )

    def health_check(self) -> bool:
        """Check if Coolify API is accessible.

        Returns:
            True if API is accessible, False otherwise
        """
        try:
            response = self.session.get(
                f"{self.base_url}/api/health",
                timeout=5,
            )
            return response.status_code == 200
        except Exception:
            return False
