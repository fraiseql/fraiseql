"""Demo of auto camelCase conversion feature.

This example shows how to use snake_case in Python models and database views
while exposing a camelCase GraphQL API automatically.
"""

from enum import Enum

import fraiseql
from fraiseql.core.translate_query import translate_query


@fraiseql.enum
class UserStatus(Enum):
    """User account status."""

    ACTIVE = "active"
    INACTIVE = "inactive"
    PENDING_VERIFICATION = "pending_verification"


@fraiseql.type
class UserProfile:
    """User profile information."""

    display_name: str = fraiseql.fraise_field(description="User's display name")
    phone_number: str = fraiseql.fraise_field(description="Phone number")
    profile_picture_url: str = fraiseql.fraise_field(description="Profile picture URL")
    date_of_birth: str = fraiseql.fraise_field(description="Date of birth")


@fraiseql.type
class User:
    """User model with snake_case fields."""

    user_id: str = fraiseql.fraise_field(description="Unique user identifier")
    first_name: str = fraiseql.fraise_field(description="User's first name")
    last_name: str = fraiseql.fraise_field(description="User's last name")
    email_address: str = fraiseql.fraise_field(description="Email address")
    account_status: UserStatus = fraiseql.fraise_field(description="Account status")
    is_email_verified: bool = fraiseql.fraise_field(
        description="Email verification status"
    )
    created_at: str = fraiseql.fraise_field(description="Account creation timestamp")
    last_login_at: str = fraiseql.fraise_field(description="Last login timestamp")
    user_profile: UserProfile = fraiseql.fraise_field(description="User profile")


def demo_auto_camel_case():
    """Demonstrate auto camelCase conversion."""

    print("=== Auto CamelCase Conversion Demo ===\n")

    # GraphQL query using camelCase (as is standard in GraphQL)
    graphql_query = """
    query {
        userId
        firstName
        lastName
        emailAddress
        accountStatus
        isEmailVerified
        createdAt
        lastLoginAt
        userProfile {
            displayName
            phoneNumber
            profilePictureUrl
            dateOfBirth
        }
    }
    """

    print("GraphQL Query (camelCase - standard GraphQL convention):")
    print(graphql_query)

    print("\n" + "=" * 60 + "\n")

    # Without auto_camel_case (traditional approach)
    print("WITHOUT auto_camel_case (expects camelCase in database):")
    sql_without = translate_query(
        query=graphql_query,
        table="users",
        typename="User",
        auto_camel_case=False,
    )
    print(sql_without.as_string(None))

    print("\n" + "=" * 60 + "\n")

    # With auto_camel_case (new feature - converts to snake_case for DB)
    print("WITH auto_camel_case=True (converts to snake_case for database):")
    sql_with = translate_query(
        query=graphql_query,
        table="users",
        typename="User",
        auto_camel_case=True,
    )
    print(sql_with.as_string(None))

    print("\n" + "=" * 60 + "\n")

    print("Benefits of auto_camel_case=True:")
    print("1. ✅ Python models use snake_case (Pythonic)")
    print("2. ✅ Database views use snake_case (SQL convention)")
    print("3. ✅ GraphQL API exposes camelCase (GraphQL convention)")
    print("4. ✅ No manual case conversion needed in SQL views")
    print("5. ✅ Single source of truth for field names")

    print("\nExample database view (with auto_camel_case=True):")
    print("""
    CREATE VIEW v_users AS
    SELECT
        u.id,
        jsonb_build_object(
            '__typename', 'User',
            'user_id', u.id,
            'first_name', u.first_name,           -- Pure snake_case
            'last_name', u.last_name,             -- Pure snake_case
            'email_address', u.email,             -- Pure snake_case
            'account_status', u.status,           -- Pure snake_case
            'is_email_verified', u.email_verified,-- Pure snake_case
            'created_at', u.created_at,           -- Pure snake_case
            'last_login_at', u.last_login_at,     -- Pure snake_case
            'user_profile', jsonb_build_object(
                'display_name', up.display_name,       -- Pure snake_case
                'phone_number', up.phone,              -- Pure snake_case
                'profile_picture_url', up.avatar_url,  -- Pure snake_case
                'date_of_birth', up.birth_date         -- Pure snake_case
            )
        ) AS data
    FROM tb_users u
    LEFT JOIN tb_user_profiles up ON u.id = up.user_id;
    """)


if __name__ == "__main__":
    demo_auto_camel_case()
