"""Test dict_keys_to_snake_case utility function."""

from fraiseql.utils.casing import dict_keys_to_snake_case


class TestDictKeysToSnakeCase:
    """Test dictionary key conversion from camelCase to snake_case."""

    def test_simple_dict_conversion(self) -> None:
        """Test basic camelCase to snake_case conversion."""
        input_dict = {
            "firstName": "John",
            "lastName": "Doe",
            "emailAddress": "john@example.com",
        }
        expected = {
            "first_name": "John",
            "last_name": "Doe",
            "email_address": "john@example.com",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_nested_dict_conversion(self) -> None:
        """Test recursive conversion in nested dicts."""
        input_dict = {
            "userId": "123",
            "userProfile": {
                "firstName": "John",
                "phoneNumber": "555-1234",
            },
        }
        expected = {
            "user_id": "123",
            "user_profile": {
                "first_name": "John",
                "phone_number": "555-1234",
            },
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_list_of_dicts_conversion(self) -> None:
        """Test conversion in lists of dicts."""
        input_dict = {
            "userId": "123",
            "userTags": [
                {"tagName": "admin", "tagColor": "red"},
                {"tagName": "developer", "tagColor": "blue"},
            ],
        }
        expected = {
            "user_id": "123",
            "user_tags": [
                {"tag_name": "admin", "tag_color": "red"},
                {"tag_name": "developer", "tag_color": "blue"},
            ],
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_deeply_nested_structures(self) -> None:
        """Test conversion in deeply nested structures."""
        input_dict = {
            "contractData": {
                "lineItems": [
                    {
                        "itemId": "A1",
                        "priceInfo": {
                            "startDate": "2025-01-01",
                            "endDate": "2025-12-31",
                        },
                    }
                ]
            }
        }
        expected = {
            "contract_data": {
                "line_items": [
                    {
                        "item_id": "A1",
                        "price_info": {
                            "start_date": "2025-01-01",
                            "end_date": "2025-12-31",
                        },
                    }
                ]
            }
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_preserves_snake_case_keys(self) -> None:
        """Test that already snake_case keys are preserved."""
        input_dict = {
            "user_id": "123",
            "first_name": "John",
        }
        expected = {
            "user_id": "123",
            "first_name": "John",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_mixed_case_keys(self) -> None:
        """Test mixed camelCase and snake_case keys."""
        input_dict = {
            "userId": "123",
            "first_name": "John",
            "emailAddress": "john@example.com",
        }
        expected = {
            "user_id": "123",
            "first_name": "John",
            "email_address": "john@example.com",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_empty_dict(self) -> None:
        """Test empty dict returns empty dict."""
        assert dict_keys_to_snake_case({}) == {}

    def test_preserves_non_string_values(self) -> None:
        """Test that values are preserved as-is."""
        from datetime import date
        from uuid import UUID

        input_dict = {
            "userId": UUID("12345678-1234-5678-1234-567812345678"),
            "startDate": date(2025, 1, 1),
            "isActive": True,
            "retryCount": 42,
            "metadata": None,
        }
        expected = {
            "user_id": UUID("12345678-1234-5678-1234-567812345678"),
            "start_date": date(2025, 1, 1),
            "is_active": True,
            "retry_count": 42,
            "metadata": None,
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_handles_empty_strings(self) -> None:
        """Test empty string values are preserved."""
        input_dict = {
            "firstName": "",
            "lastName": "Doe",
        }
        expected = {
            "first_name": "",
            "last_name": "Doe",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_handles_lists_of_primitives(self) -> None:
        """Test lists of primitive values are preserved."""
        input_dict = {
            "userIds": ["123", "456", "789"],
            "statusCodes": [200, 404, 500],
        }
        expected = {
            "user_ids": ["123", "456", "789"],
            "status_codes": [200, 404, 500],
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_handles_mixed_lists(self) -> None:
        """Test lists containing both dicts and primitives."""
        input_dict = {
            "mixedData": [
                "string",
                123,
                {"itemName": "value"},
            ]
        }
        expected = {
            "mixed_data": [
                "string",
                123,
                {"item_name": "value"},
            ]
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_acronyms_in_keys(self) -> None:
        """Test handling of acronyms (e.g., 'IP', 'DNS', 'URL')."""
        input_dict = {
            "ipAddress": "192.168.1.1",
            "dnsServerName": "ns1.example.com",
            "apiURL": "https://api.example.com",
        }
        expected = {
            "ip_address": "192.168.1.1",
            "dns_server_name": "ns1.example.com",
            "api_url": "https://api.example.com",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_consecutive_capitals(self) -> None:
        """Test keys with consecutive capital letters."""
        input_dict = {
            "HTTPStatusCode": 200,
            "URLPath": "/api/v1/users",
        }
        expected = {
            "http_status_code": 200,
            "url_path": "/api/v1/users",
        }
        assert dict_keys_to_snake_case(input_dict) == expected

    def test_single_letter_keys(self) -> None:
        """Test single-letter keys are preserved."""
        input_dict = {
            "x": 10,
            "y": 20,
            "z": 30,
        }
        expected = {
            "x": 10,
            "y": 20,
            "z": 30,
        }
        assert dict_keys_to_snake_case(input_dict) == expected
