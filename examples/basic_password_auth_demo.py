"""Demo of basic password authentication for development router.

This example shows how to enable simple HTTP Basic Auth for protecting
GraphQL endpoints during development.
"""

import fraiseql
from fraiseql.fastapi.app import create_fraiseql_app


@fraiseql.type
class User:
    """User model for the demo."""

    id: str = fraiseql.fraise_field(description="User ID")
    name: str = fraiseql.fraise_field(description="User name")
    email: str = fraiseql.fraise_field(description="User email")


@fraiseql.type
class QueryRoot:
    """Root query for the demo."""

    user: User = fraiseql.fraise_field(description="Get user", purpose="output")

    def resolve_user(self, info):
        return User(id="1", name="Demo User", email="demo@example.com")


def demo_basic_auth():
    """Demonstrate basic password authentication."""

    print("=== Basic Password Authentication Demo ===\n")

    # Example 1: Development with basic auth enabled
    print("1. Development with basic auth enabled:")
    app = create_fraiseql_app(
        database_url="postgresql://localhost/demo",
        types=[User, QueryRoot],
        production=False,
        dev_auth_username="admin",
        dev_auth_password="secret123",
    )

    print("   - Username: admin")
    print("   - Password: secret123")
    print("   - Protected endpoints: /graphql, /docs, /playground")
    print("   - Unprotected endpoints: /health")
    print("   - Browser will prompt for Basic Auth credentials")
    print()

    # Example 2: Development without auth (default)
    print("2. Development without auth (default):")
    _app_no_auth = create_fraiseql_app(
        database_url="postgresql://localhost/demo",
        types=[User, QueryRoot],
        production=False,
    )

    print("   - No authentication required")
    print("   - All endpoints accessible")
    print()

    # Example 3: Production mode (auth disabled by design)
    print("3. Production mode (auth disabled by design):")
    _app_prod = create_fraiseql_app(
        database_url="postgresql://localhost/demo",
        types=[User, QueryRoot],
        production=True,
        dev_auth_password="ignored_in_production",  # This is ignored
    )

    print("   - Development auth is always disabled in production")
    print("   - Use proper authentication providers for production")
    print()

    # Example 4: Environment variable configuration
    print("4. Environment variable configuration:")
    print("   Set these environment variables:")
    print("   export FRAISEQL_DEV_USERNAME=myuser")
    print("   export FRAISEQL_DEV_PASSWORD=mypassword")
    print()
    print("   Then create app without explicit auth parameters:")
    print("   app = create_fraiseql_app(...")
    print("       production=False,  # Auth only works in dev mode")
    print("   )")
    print()

    print("=== Security Notes ===")
    print("• Development auth is for development environments only")
    print("• Production mode always ignores dev auth settings")
    print("• Use proper auth providers (Auth0, custom) for production")
    print("• Passwords should come from environment variables")
    print("• Uses HTTP Basic Auth (browser will show login dialog)")
    print("• Protected paths: /graphql, /playground, /graphiql, /docs")

    return app


if __name__ == "__main__":
    demo_app = demo_basic_auth()
    print("\nTo run this demo:")
    print("uvicorn examples.basic_password_auth_demo:demo_app --reload")
    print("\nThen visit: http://localhost:8000/docs")
    print("Login with username 'admin' and password 'secret123'")
