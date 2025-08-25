import pytest

"""Integration tests for auto camelCase feature with real GraphQL types."""

from enum import Enum

import fraiseql
from fraiseql.core.translate_query import translate_query


@pytest.mark.unit
class TestAutoCamelCaseIntegration:
    """Test auto camelCase feature with real FraiseQL types."""

    def test_fraise_type_with_snake_case_fields(self, clear_registry) -> None:
        """Test that @fraiseql.type works with snake_case fields when auto_camel_case is enabled."""

        @fraiseql.type
        class User:
            user_id: str = fraiseql.fraise_field(description="User ID")
            first_name: str = fraiseql.fraise_field(description="First name")
            last_name: str = fraiseql.fraise_field(description="Last name")
            email_address: str = fraiseql.fraise_field(description="Email")
            is_active: bool = fraiseql.fraise_field(description="Active status")
            created_at: str = fraiseql.fraise_field(description="Creation time")

        # Verify that fields were created with snake_case names (in Python)
        assert hasattr(User, "__gql_fields__")
        fields = User.__gql_fields__
        assert "user_id" in fields
        assert "first_name" in fields
        assert "last_name" in fields
        assert "email_address" in fields
        assert "is_active" in fields
        assert "created_at" in fields

        # Test GraphQL query with camelCase field names
        query = """
        query {
            userId
            firstName
            lastName
            emailAddress
            isActive
            createdAt
        }
        """
        # Without auto_camel_case, it should look for camelCase in DB
        sql_without = translate_query(
            query=query, table="users", typename="User", auto_camel_case=False
        )

        sql_str_without = sql_without.as_string(None)
        # String fields use ->> operator
        assert "'userId', data->>'userId'" in sql_str_without
        assert "'firstName', data->>'firstName'" in sql_str_without
        # Boolean fields use -> operator for type preservation
        assert "'isActive', data->'isActive'" in sql_str_without

        # With auto_camel_case, it should convert to snake_case for DB lookup
        sql_with = translate_query(
            query=query, table="users", typename="User", auto_camel_case=True
        )

        sql_str_with = sql_with.as_string(None)
        # String fields use ->> operator
        assert "'userId', data->>'user_id'" in sql_str_with
        assert "'firstName', data->>'first_name'" in sql_str_with
        assert "'lastName', data->>'last_name'" in sql_str_with
        assert "'emailAddress', data->>'email_address'" in sql_str_with
        assert "'createdAt', data->>'created_at'" in sql_str_with
        # Boolean fields use -> operator for type preservation
        assert "'isActive', data->'is_active'" in sql_str_with

    def test_nested_types_with_auto_camel_case(self, clear_registry) -> None:
        """Test nested types with auto camelCase conversion."""

        @fraiseql.type
        class Address:
            street_address: str = fraiseql.fraise_field(description="Street")
            postal_code: str = fraiseql.fraise_field(description="ZIP")
            country_code: str = fraiseql.fraise_field(description="Country")

        @fraiseql.type
        class UserProfile:
            display_name: str = fraiseql.fraise_field(description="Display name")
            phone_number: str = fraiseql.fraise_field(description="Phone")
            home_address: Address = (fraiseql.fraise_field(description="Address"),)

        query = """
        query {
            displayName
            phoneNumber
            homeAddress {
                streetAddress
                postalCode
                countryCode
            }
        }
        """
        sql = translate_query(
            query=query, table="user_profiles", typename="UserProfile", auto_camel_case=True
        )

        sql_str = sql.as_string(None)
        assert "'displayName', data->>'display_name'" in sql_str
        assert "'phoneNumber', data->>'phone_number'" in sql_str
        assert "'streetAddress', data->'home_address'->>'street_address'" in sql_str
        assert "'postalCode', data->'home_address'->>'postal_code'" in sql_str
        assert "'countryCode', data->'home_address'->>'country_code'" in sql_str

    def test_mixed_case_scenarios(self, clear_registry) -> None:
        """Test scenarios with mixed camelCase and snake_case."""

        @fraiseql.type
        class MixedType:
            # Already snake_case (should remain unchanged)
            user_id: str = fraiseql.fraise_field(description="ID")
            created_at: str = fraiseql.fraise_field(description="Creation time")

            # Single words (should remain unchanged)
            name: str = fraiseql.fraise_field(description="Name")
            email: str = (fraiseql.fraise_field(description="Email"),)

        query = """
        query {
            userId
            createdAt
            name
            email
        }
        """
        sql = translate_query(
            query=query, table="mixed_types", typename="MixedType", auto_camel_case=True
        )

        sql_str = sql.as_string(None)
        # camelCase should be converted to snake_case
        assert "'userId', data->>'user_id'" in sql_str
        assert "'createdAt', data->>'created_at'" in sql_str
        # Single words should remain unchanged
        assert "'name', data->>'name'" in sql_str
        assert "'email', data->>'email'" in sql_str

    def test_enum_fields_with_auto_camel_case(self, clear_registry) -> None:
        """Test enum fields with auto camelCase conversion."""

        @fraiseql.enum
        class UserStatus(Enum):
            ACTIVE = "active"
            INACTIVE = "inactive"
            PENDING_VERIFICATION = "pending_verification"

        @fraiseql.type
        class User:
            user_id: str = fraiseql.fraise_field(description="ID")
            account_status: UserStatus = fraiseql.fraise_field(description="Status")
            preferred_language: str = fraiseql.fraise_field(description="Language")

        query = """
        query {
            userId
            accountStatus
            preferredLanguage
        }
        """
        sql = translate_query(query=query, table="users", typename="User", auto_camel_case=True)

        sql_str = sql.as_string(None)
        assert "'userId', data->>'user_id'" in sql_str
        assert "'accountStatus', data->>'account_status'" in sql_str
        assert "'preferredLanguage', data->>'preferred_language'" in sql_str

    def test_complex_real_world_scenario(self, clear_registry) -> None:
        """Test a complex real-world scenario with multiple levels of nesting."""

        @fraiseql.enum
        class OrderStatus(Enum):
            PENDING_PAYMENT = "pending_payment"
            PAYMENT_CONFIRMED = "payment_confirmed"
            IN_PREPARATION = "in_preparation"
            READY_FOR_PICKUP = "ready_for_pickup"
            COMPLETED = "completed"

        @fraiseql.type
        class OrderItem:
            item_id: str = fraiseql.fraise_field(description="Item ID")
            product_name: str = fraiseql.fraise_field(description="Product name")
            unit_price: float = fraiseql.fraise_field(description="Unit price")
            quantity_ordered: int = fraiseql.fraise_field(description="Quantity")
            special_instructions: str = fraiseql.fraise_field(description="Instructions")

        @fraiseql.type
        class CustomerInfo:
            customer_id: str = fraiseql.fraise_field(description="Customer ID")
            full_name: str = fraiseql.fraise_field(description="Full name")
            email_address: str = fraiseql.fraise_field(description="Email")
            phone_number: str = fraiseql.fraise_field(description="Phone")
            delivery_address: str = fraiseql.fraise_field(description="Address")

        @fraiseql.type
        class Order:
            order_id: str = fraiseql.fraise_field(description="Order ID")
            order_number: str = fraiseql.fraise_field(description="Order number")
            customer_info: CustomerInfo = fraiseql.fraise_field(description="Customer")
            order_items: list[OrderItem] = fraiseql.fraise_field(description="Items")
            order_status: OrderStatus = fraiseql.fraise_field(description="Status")
            total_amount: float = fraiseql.fraise_field(description="Total")
            created_at: str = fraiseql.fraise_field(description="Created")
            estimated_pickup_time: str = (fraiseql.fraise_field(description="Pickup time"),)

        query = """
        query {
            orderId
            orderNumber
            customerInfo {
                customerId
                fullName
                emailAddress
                phoneNumber
                deliveryAddress
            }
            orderItems {
                itemId
                productName
                unitPrice
                quantityOrdered
                specialInstructions
            }
            orderStatus
            totalAmount
            createdAt
            estimatedPickupTime
        }
        """
        sql = translate_query(query=query, table="orders", typename="Order", auto_camel_case=True)

        sql_str = sql.as_string(None)

        # Top-level fields
        assert "'orderId', data->>'order_id'" in sql_str  # string
        assert "'orderNumber', data->>'order_number'" in sql_str  # string
        assert "'totalAmount', data->'total_amount'" in sql_str  # float - type preservation
        assert "'createdAt', data->>'created_at'" in sql_str  # string
        assert "'estimatedPickupTime', data->>'estimated_pickup_time'" in sql_str  # string

        # Nested customer info fields
        assert "'customerId', data->'customer_info'->>'customer_id'" in sql_str
        assert "'fullName', data->'customer_info'->>'full_name'" in sql_str
        assert "'emailAddress', data->'customer_info'->>'email_address'" in sql_str
        assert "'phoneNumber', data->'customer_info'->>'phone_number'" in sql_str
        assert "'deliveryAddress', data->'customer_info'->>'delivery_address'" in sql_str

        # Nested order items fields
        assert "'itemId', data->'order_items'->>'item_id'" in sql_str  # string
        assert "'productName', data->'order_items'->>'product_name'" in sql_str  # string
        assert (
            "'unitPrice', data->'order_items'->'unit_price'" in sql_str
        )  # float - type preservation
        assert (
            "'quantityOrdered', data->'order_items'->'quantity_ordered'" in sql_str
        )  # int - type preservation
        assert (
            "'specialInstructions', data->'order_items'->>'special_instructions'" in sql_str
        )  # string
