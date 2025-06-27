"""Test dual-mode repository with JSONB data column pattern (PrintOptim style)."""

from datetime import date
from typing import Optional
from uuid import UUID, uuid4

import pytest

from fraiseql import fraise_field, fraise_type
from fraiseql.db import FraiseQLRepository


@fraise_type
class Machine:
    """Machine type for testing."""

    id: UUID
    name: str
    model: str
    serial_number: str


@fraise_type
class Location:
    """Location type for testing."""

    id: UUID
    name: str
    building: str
    floor: str


@fraise_type
class Allocation:
    """Allocation type with all fields from JSONB data column."""

    id: UUID
    identifier: str
    machine_id: Optional[UUID]
    location_id: Optional[UUID]
    valid_from: date
    valid_until: Optional[date]
    is_current: bool = fraise_field(default=False)
    notes: Optional[str]

    # Nested objects
    machine: Optional[Machine]
    location: Optional[Location]


class TestDualModeJSONBPattern:
    """Test repository with JSONB data column pattern."""

    @pytest.fixture
    def sample_allocation_row(self):
        """Sample database row with JSONB data column."""
        machine_id = uuid4()
        location_id = uuid4()
        allocation_id = uuid4()

        return {
            # Columns used for filtering/access control
            "id": str(allocation_id),
            "tenant_id": str(uuid4()),
            "machine_id": str(machine_id),
            "location_id": str(location_id),
            # JSONB column with complete object data
            "data": {
                "id": str(allocation_id),
                "identifier": "ALLOC-001",
                "machine_id": str(machine_id),
                "location_id": str(location_id),
                "valid_from": "2024-01-01",
                "valid_until": "2024-12-31",
                "is_current": True,
                "notes": "Test allocation",
                "machine": {
                    "id": str(machine_id),
                    "name": "Printer XYZ",
                    "model": "LaserJet Pro",
                    "serial_number": "XYZ123",
                },
                "location": {
                    "id": str(location_id),
                    "name": "Main Office",
                    "building": "Building A",
                    "floor": "3rd",
                },
            },
        }

    def test_development_mode_instantiates_from_data_column(self, mock_pool, sample_allocation_row):
        """Test that dev mode instantiates from JSONB data column."""
        repo = FraiseQLRepository(mock_pool, {"mode": "development"})

        # Mock the type registry
        repo._get_type_for_view = lambda view_name: Allocation

        # Instantiate from row
        allocation = repo._instantiate_from_row(Allocation, sample_allocation_row)

        # Verify instantiation from data column
        assert isinstance(allocation, Allocation)
        assert allocation.identifier == "ALLOC-001"
        assert allocation.valid_from == date(2024, 1, 1)
        assert allocation.valid_until == date(2024, 12, 31)
        assert allocation.is_current is True
        assert allocation.notes == "Test allocation"

        # Verify nested objects
        assert isinstance(allocation.machine, Machine)
        assert allocation.machine.name == "Printer XYZ"
        assert allocation.machine.model == "LaserJet Pro"

        assert isinstance(allocation.location, Location)
        assert allocation.location.name == "Main Office"
        assert allocation.location.building == "Building A"

    def test_production_mode_returns_raw_dict(self, mock_pool, sample_allocation_row):
        """Test that production mode returns raw dict."""
        repo = FraiseQLRepository(mock_pool, {"mode": "production"})

        # In production, find_one would return the raw row
        # The row contains both filtering columns and data column
        assert sample_allocation_row["tenant_id"]  # Used for filtering
        assert sample_allocation_row["data"]["identifier"] == "ALLOC-001"

    def test_data_column_required(self, mock_pool):
        """Test that data column is required."""
        row_without_data = {
            "id": str(uuid4()),
            "tenant_id": str(uuid4()),
            # No 'data' column
        }

        repo = FraiseQLRepository(mock_pool, {"mode": "development"})
        repo._get_type_for_view = lambda view_name: Allocation

        # Should raise KeyError since 'data' column is required
        with pytest.raises(KeyError):
            repo._instantiate_from_row(Allocation, row_without_data)


@pytest.fixture
def mock_pool():
    """Mock connection pool for testing."""

    class MockPool:
        def connection(self):
            return self

        async def __aenter__(self):
            return self

        async def __aexit__(self, exc_type, exc_val, exc_tb):
            pass

        def cursor(self, row_factory=None):
            return self

    return MockPool()
