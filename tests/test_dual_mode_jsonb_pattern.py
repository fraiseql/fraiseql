"""Test dual-mode repository with JSONB data column pattern (PrintOptim style)."""

from datetime import date
from typing import Optional
from uuid import UUID, uuid4

import pytest

from fraiseql import fraise_field, fraise_type
from fraiseql.db import FraiseQLRepository, register_type_for_view

# Import database fixtures for this database test
from tests.database_conftest import *  # noqa: F403


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


@pytest.mark.database
class TestDualModeJSONBPattern:
    """Test repository with JSONB data column pattern."""

    @pytest.fixture
    async def test_tables(self, db_connection):
        """Create test tables with JSONB data columns."""
        # Create allocation table with JSONB data column
        await db_connection.execute("""
            CREATE TABLE IF NOT EXISTS allocations (
                id UUID PRIMARY KEY,
                tenant_id UUID NOT NULL,
                machine_id UUID,
                location_id UUID,
                data JSONB NOT NULL
            )
        """)

        # Create view for allocations
        await db_connection.execute("""
            CREATE OR REPLACE VIEW allocation_view AS
            SELECT id, tenant_id, machine_id, location_id, data
            FROM allocations
        """)

        # Register the type for the view
        register_type_for_view("allocation_view", Allocation)

    @pytest.fixture
    async def sample_allocation_data(self, db_connection, test_tables):
        """Insert sample allocation data into database."""
        machine_id = uuid4()
        location_id = uuid4()
        allocation_id = uuid4()
        tenant_id = uuid4()

        # Insert allocation with JSONB data
        await db_connection.execute(
            """
            INSERT INTO allocations (id, tenant_id, machine_id, location_id, data)
            VALUES (%s, %s, %s, %s, %s::jsonb)
        """,
            (
                allocation_id,
                tenant_id,
                machine_id,
                location_id,
                {
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
            ),
        )

        return allocation_id

    async def test_development_mode_instantiates_from_data_column(
        self, db_pool, sample_allocation_data
    ):
        """Test that dev mode instantiates from JSONB data column."""
        repo = FraiseQLRepository(db_pool, {"mode": "development"})

        # Find the allocation - in dev mode it should instantiate the type
        result = await repo.find_one("allocation_view", id=sample_allocation_data)

        # In development mode with registered type, should get Allocation instance
        assert isinstance(result, Allocation)
        assert result.identifier == "ALLOC-001"
        # Note: Date conversion from string is handled by the repository
        assert str(result.valid_from) == "2024-01-01"
        assert str(result.valid_until) == "2024-12-31"
        assert result.is_current is True
        assert result.notes == "Test allocation"

        # Nested objects are dictionaries in current implementation
        assert isinstance(result.machine, dict)
        assert result.machine["name"] == "Printer XYZ"
        assert result.machine["model"] == "LaserJet Pro"

        assert isinstance(result.location, dict)
        assert result.location["name"] == "Main Office"
        assert result.location["building"] == "Building A"

    async def test_production_mode_returns_raw_dict(self, db_pool, sample_allocation_data):
        """Test that production mode returns raw dict."""
        repo = FraiseQLRepository(db_pool, {"mode": "production"})

        # Find the allocation - in production mode it should return raw dict
        result = await repo.find_one("allocation_view", id=sample_allocation_data)

        # In production mode, returns raw dictionary
        assert isinstance(result, dict)
        assert result["data"]["identifier"] == "ALLOC-001"
        assert result["tenant_id"]  # Used for filtering

    async def test_data_column_extraction_in_production(self, db_pool, sample_allocation_data):
        """Test that production mode extracts from data column when present."""
        repo = FraiseQLRepository(db_pool, {"mode": "production"})

        # Find the allocation
        result = await repo.find_one("allocation_view", id=sample_allocation_data)

        # Production mode should extract JSONB data when 'data' column exists
        assert isinstance(result, dict)
        # The repository extracts and returns the data column content in production
        assert "identifier" in result  # Should be extracted from data column
        assert (
            result.get("identifier") == "ALLOC-001" or result["data"]["identifier"] == "ALLOC-001"
        )
