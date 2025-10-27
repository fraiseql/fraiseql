"""Comprehensive tests for conflict entity resolution across all entity types.

This test suite covers diverse entity types to ensure the conflict resolution
fix works universally across the FraiseQL framework.

Entity types: Location, DnsServer, Gateway, Router, SmtpServer, FileServer,
covering network entities, geographic entities, and service entities.
"""

import pytest
import fraiseql
from fraiseql.mutations.error_config import DEFAULT_ERROR_CONFIG
from fraiseql.mutations.parser import parse_mutation_result


# Define all entity types mentioned in the bug ticket
@fraiseql.type
class Location:
    """Location entity for geographic entities."""

    id: str
    name: str
    identifier: str
    level: str | None = None

    @classmethod
    def from_dict(cls, data: dict) -> "Location":
        return cls(**data)


@fraiseql.type
class DnsServer:
    """DNS Server entity for network entities."""

    id: str
    name: str
    ip_address: str
    port: int | None = None

    @classmethod
    def from_dict(cls, data: dict) -> "DnsServer":
        return cls(**data)


@fraiseql.type
class Gateway:
    """Gateway entity for network routing."""

    id: str
    name: str
    ip_address: str
    subnet: str | None = None

    @classmethod
    def from_dict(cls, data: dict) -> "Gateway":
        return cls(**data)


@fraiseql.type
class Router:
    """Router entity for network infrastructure."""

    id: str
    name: str
    model: str
    firmware_version: str | None = None

    @classmethod
    def from_dict(cls, data: dict) -> "Router":
        return cls(**data)


@fraiseql.type
class SmtpServer:
    """SMTP Server entity for mail services."""

    id: str
    name: str
    hostname: str
    port: int = 587

    @classmethod
    def from_dict(cls, data: dict) -> "SmtpServer":
        return cls(**data)


@fraiseql.type
class FileServer:
    """File Server entity for file services."""

    id: str
    name: str
    hostname: str
    share_path: str | None = None

    @classmethod
    def from_dict(cls, data: dict) -> "FileServer":
        return cls(**data)


# Define error types for each entity
@fraiseql.failure
class LocationError:
    message: str
    conflict_location: Location | None = None
    errors: list[dict] | None = None


@fraiseql.failure
class DnsServerError:
    message: str
    conflict_dns_server: DnsServer | None = None
    errors: list[dict] | None = None


@fraiseql.failure
class GatewayError:
    message: str
    conflict_gateway: Gateway | None = None
    errors: list[dict] | None = None


@fraiseql.failure
class RouterError:
    message: str
    conflict_router: Router | None = None
    errors: list[dict] | None = None


@fraiseql.failure
class SmtpServerError:
    message: str
    conflict_smtp_server: SmtpServer | None = None
    errors: list[dict] | None = None


@fraiseql.failure
class FileServerError:
    message: str
    conflict_file_server: FileServer | None = None
    errors: list[dict] | None = None


# Success types (minimal, just for parser requirements)
@fraiseql.success
class GenericSuccess:
    message: str


@pytest.mark.unit
class TestAllEntityTypesConflictResolution:
    """Test conflict entity instantiation for all entity types mentioned in bug ticket."""

    def test_location_conflict_entity_instantiation(self):
        """Test conflict resolution for Location entities (geographic entities)."""
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Location already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "location-123",
                                    "name": "Main Office Building",
                                    "identifier": "main.office.building",
                                    "level": "building",
                                }
                            }
                        }
                    }
                ]
            },
        }

        result = parse_mutation_result(
            mutation_result, GenericSuccess, LocationError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(result, LocationError)
        assert result.conflict_location is not None
        assert isinstance(result.conflict_location, Location)
        assert result.conflict_location.id == "location-123"
        assert result.conflict_location.name == "Main Office Building"
        assert result.conflict_location.identifier == "main.office.building"
        assert result.conflict_location.level == "building"

    def test_dns_server_conflict_entity_instantiation(self):
        """Test conflict resolution for DnsServer entities (network entities)."""
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "DNS server already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "dns-server-456",
                                    "name": "Primary DNS",
                                    "ip_address": "8.8.8.8",
                                    "port": 53,
                                }
                            }
                        }
                    }
                ]
            },
        }

        result = parse_mutation_result(
            mutation_result, GenericSuccess, DnsServerError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(result, DnsServerError)
        assert result.conflict_dns_server is not None
        assert isinstance(result.conflict_dns_server, DnsServer)
        assert result.conflict_dns_server.id == "dns-server-456"
        assert result.conflict_dns_server.name == "Primary DNS"
        assert result.conflict_dns_server.ip_address == "8.8.8.8"
        assert result.conflict_dns_server.port == 53

    def test_gateway_conflict_entity_instantiation(self):
        """Test conflict resolution for Gateway entities (network entities)."""
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Gateway already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "gateway-789",
                                    "name": "Main Gateway",
                                    "ip_address": "192.168.1.1",
                                    "subnet": "192.168.1.0/24",
                                }
                            }
                        }
                    }
                ]
            },
        }

        result = parse_mutation_result(
            mutation_result, GenericSuccess, GatewayError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(result, GatewayError)
        assert result.conflict_gateway is not None
        assert isinstance(result.conflict_gateway, Gateway)
        assert result.conflict_gateway.id == "gateway-789"
        assert result.conflict_gateway.name == "Main Gateway"
        assert result.conflict_gateway.ip_address == "192.168.1.1"
        assert result.conflict_gateway.subnet == "192.168.1.0/24"

    def test_router_conflict_entity_instantiation(self):
        """Test conflict resolution for Router entities (network entities)."""
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Router already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "router-101",
                                    "name": "Core Router",
                                    "model": "Cisco ASR9000",
                                    "firmware_version": "7.3.2",
                                }
                            }
                        }
                    }
                ]
            },
        }

        result = parse_mutation_result(
            mutation_result, GenericSuccess, RouterError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(result, RouterError)
        assert result.conflict_router is not None
        assert isinstance(result.conflict_router, Router)
        assert result.conflict_router.id == "router-101"
        assert result.conflict_router.name == "Core Router"
        assert result.conflict_router.model == "Cisco ASR9000"
        assert result.conflict_router.firmware_version == "7.3.2"

    def test_smtp_server_conflict_entity_instantiation(self):
        """Test conflict resolution for SmtpServer entities (mail service entities)."""
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "SMTP server already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "smtp-server-202",
                                    "name": "Corporate Mail Server",
                                    "hostname": "mail.company.com",
                                    "port": 587,
                                }
                            }
                        }
                    }
                ]
            },
        }

        result = parse_mutation_result(
            mutation_result, GenericSuccess, SmtpServerError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(result, SmtpServerError)
        assert result.conflict_smtp_server is not None
        assert isinstance(result.conflict_smtp_server, SmtpServer)
        assert result.conflict_smtp_server.id == "smtp-server-202"
        assert result.conflict_smtp_server.name == "Corporate Mail Server"
        assert result.conflict_smtp_server.hostname == "mail.company.com"
        assert result.conflict_smtp_server.port == 587

    def test_file_server_conflict_entity_instantiation(self):
        """Test conflict resolution for FileServer entities (file service entities)."""
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "File server already exists",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "file-server-303",
                                    "name": "Main File Server",
                                    "hostname": "files.office.com",
                                    "share_path": "/shared/documents",
                                }
                            }
                        }
                    }
                ]
            },
        }

        result = parse_mutation_result(
            mutation_result, GenericSuccess, FileServerError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(result, FileServerError)
        assert result.conflict_file_server is not None
        assert isinstance(result.conflict_file_server, FileServer)
        assert result.conflict_file_server.id == "file-server-303"
        assert result.conflict_file_server.name == "Main File Server"
        assert result.conflict_file_server.hostname == "files.office.com"
        assert result.conflict_file_server.share_path == "/shared/documents"

    def test_multiple_conflict_fields_single_entity_type(self):
        """Test that multiple conflict_* fields of the same type can be populated."""

        @fraiseql.failure
        class MultiLocationError:
            message: str
            conflict_location: Location | None = None
            conflict_backup_location: Location | None = None  # Another location field
            errors: list[dict] | None = None

        # Test data that can instantiate Location objects
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "Multiple location conflicts detected",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "multi-location-123",
                                    "name": "Conflict Location",
                                    "identifier": "conflict.location",
                                }
                            }
                        }
                    }
                ]
            },
        }

        result = parse_mutation_result(
            mutation_result, GenericSuccess, MultiLocationError, DEFAULT_ERROR_CONFIG
        )

        assert isinstance(result, MultiLocationError)

        # Both conflict fields should be populated with the same source data
        # The parser tries to instantiate each conflict_* field independently
        assert result.conflict_location is not None
        assert result.conflict_backup_location is not None

        # Check that both got the same data (both are Location objects)
        assert result.conflict_location.id == "multi-location-123"
        assert result.conflict_location.name == "Conflict Location"
        assert result.conflict_backup_location.id == "multi-location-123"
        assert result.conflict_backup_location.name == "Conflict Location"

    def test_enterprise_pattern_compatibility(self):
        """Test that the fix maintains compatibility with enterprise patterns from the bug ticket."""
        # This test simulates the exact enterprise pattern from the bug ticket
        mutation_result = {
            "updated_fields": [],
            "status": "noop:already_exists",
            "message": "A location with this name already exists in this organization",
            "object_data": None,
            "extra_metadata": {
                "errors": [
                    {
                        "details": {
                            "conflict": {
                                "conflictObject": {
                                    "id": "01411222-4111-0000-1000-000000000002",
                                    "name": "21411-1 child",
                                    "identifier": "test_create_location_deduplication.child",
                                }
                            }
                        }
                    }
                ]
            },
        }

        result = parse_mutation_result(
            mutation_result, GenericSuccess, LocationError, DEFAULT_ERROR_CONFIG
        )

        # This should now work (the original bug)
        assert isinstance(result, LocationError)
        assert result.message == "A location with this name already exists in this organization"

        # ✅ THE FIX: conflict_location should now be populated
        assert result.conflict_location is not None
        assert isinstance(result.conflict_location, Location)
        assert result.conflict_location.id == "01411222-4111-0000-1000-000000000002"
        assert result.conflict_location.name == "21411-1 child"
        assert result.conflict_location.identifier == "test_create_location_deduplication.child"

        # Also check that standard error handling still works
        assert result.errors is not None  # Should be populated
        assert len(result.errors) == 1

        # The errors field might contain the original error structure from extra_metadata
        # Let's just verify there's error information available
        error_obj = result.errors[0]
        assert "details" in error_obj or "code" in error_obj  # Either structure is fine

        # The key point is that conflict_location is now populated (the bug fix)
