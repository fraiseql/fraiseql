"""Performance benchmarks for field selection filtering."""

import json
import time

import pytest

from fraiseql import _get_fraiseql_rs


@pytest.fixture
def fraiseql_rs():
    """Get Rust module."""
    return _get_fraiseql_rs()


def test_performance_small_response_field_filtering(fraiseql_rs, benchmark=None):
    """Benchmark field filtering on small response (5 fields, request 2)."""
    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Test"},
        "updated_fields": ["name"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    selected_fields = ["status", "machine"]

    def run_filtering():
        return fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "createMachine",
            "CreateMachineSuccess",
            "CreateMachineError",
            "machine",
            "Machine",
            None,
            True,
            selected_fields,
        )

    if benchmark:
        # Using pytest-benchmark
        result = benchmark(run_filtering)
    else:
        # Manual timing
        start = time.perf_counter()
        iterations = 10000
        for _ in range(iterations):
            run_filtering()
        end = time.perf_counter()
        avg_time = (end - start) / iterations
        print(f"✅ Small response: {avg_time * 1000:.3f}ms avg ({iterations} iterations)")


def test_performance_medium_response_field_filtering(fraiseql_rs, benchmark=None):
    """Benchmark field filtering on medium response (20 fields, request 5)."""
    # Create response with many auto-injected fields + entity
    fake_result = {
        "status": "success",
        "message": "Machine created with full configuration",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {
            "id": "123",
            "name": "Machine X",
            "serial_number": "SN-001",
            "model": "Model A",
            "manufacturer": "Manufacturer B",
            "location": {"id": "loc1", "name": "Warehouse A"},
            "contract": {"id": "c1", "name": "Contract 1"},
            "status": "active",
            "notes": "Some notes here",
            "metadata": {"key1": "value1", "key2": "value2"},
        },
        "updated_fields": ["name", "serial_number", "model", "location_id"],
        "cascade": {"deleted": {}, "updated": {}},
        "metadata": None,
        "is_simple_format": False,
    }

    # Request only 5 fields out of many available
    selected_fields = ["status", "message", "machine", "updatedFields", "id"]

    def run_filtering():
        return fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "createMachine",
            "CreateMachineSuccess",
            "CreateMachineError",
            "machine",
            "Machine",
            None,
            True,
            selected_fields,
        )

    if benchmark:
        result = benchmark(run_filtering)
    else:
        start = time.perf_counter()
        iterations = 5000
        for _ in range(iterations):
            run_filtering()
        end = time.perf_counter()
        avg_time = (end - start) / iterations
        print(f"✅ Medium response: {avg_time * 1000:.3f}ms avg ({iterations} iterations)")


def test_performance_large_cascade_field_filtering(fraiseql_rs, benchmark=None):
    """Benchmark field filtering on response with large cascade (100 entities)."""
    # Create 100 reservation entities in cascade
    cascade_entities = [
        {"id": f"r{i}", "name": f"Reservation {i}", "status": "cancelled"} for i in range(100)
    ]

    fake_result = {
        "status": "success",
        "message": "Machine deleted with cascade",
        "entity_id": "123",
        "entity_type": "Machine",
        "entity": {"id": "123", "name": "Machine X"},
        "updated_fields": [],
        "cascade": {"deleted": {"Reservation": cascade_entities}, "updated": {}},
        "metadata": None,
        "is_simple_format": False,
    }

    # Request cascade but filter entity fields
    selected_fields = ["status", "machine", "cascade"]
    cascade_selections = {
        "fields": ["deleted"],
        "deleted": {
            "fields": ["Reservation"],
            "Reservation": {
                "fields": ["id", "status"]  # Don't request 'name'
            },
        },
    }

    def run_filtering():
        return fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "deleteMachine",
            "DeleteMachineSuccess",
            "DeleteMachineError",
            "machine",
            "Machine",
            json.dumps(cascade_selections),
            True,
            selected_fields,
        )

    if benchmark:
        result = benchmark(run_filtering)
    else:
        start = time.perf_counter()
        iterations = 1000
        for _ in range(iterations):
            run_filtering()
        end = time.perf_counter()
        avg_time = (end - start) / iterations
        print(f"✅ Large cascade: {avg_time * 1000:.3f}ms avg ({iterations} iterations)")


def test_performance_no_filtering_vs_filtering(fraiseql_rs):
    """Compare performance: filtering vs no filtering."""
    fake_result = {
        "status": "success",
        "message": "Created",
        "entity_id": "123",
        "entity_type": "Test",
        "entity": {"id": "123", "name": "Test", "serial": "SN-001", "model": "A"},
        "updated_fields": ["name", "serial"],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Test 1: No filtering (None)
    iterations = 5000
    start = time.perf_counter()
    for _ in range(iterations):
        fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "createMachine",
            "CreateMachineSuccess",
            "CreateMachineError",
            "machine",
            "Machine",
            None,
            True,
            None,  # No filtering
        )
    no_filter_time = time.perf_counter() - start

    # Test 2: With filtering (request 2 fields)
    start = time.perf_counter()
    for _ in range(iterations):
        fraiseql_rs.build_mutation_response(
            json.dumps(fake_result),
            "createMachine",
            "CreateMachineSuccess",
            "CreateMachineError",
            "machine",
            "Machine",
            None,
            True,
            ["status", "machine"],  # Filtering
        )
    with_filter_time = time.perf_counter() - start

    no_filter_avg = (no_filter_time / iterations) * 1000
    with_filter_avg = (with_filter_time / iterations) * 1000
    overhead = with_filter_avg - no_filter_avg

    print(f"✅ No filtering: {no_filter_avg:.3f}ms avg")
    print(f"✅ With filtering: {with_filter_avg:.3f}ms avg")
    print(f"✅ Overhead: {overhead:.3f}ms ({(overhead / no_filter_avg) * 100:.1f}%)")

    # Filtering should have minimal overhead (< 20% slower)
    assert overhead < (no_filter_avg * 0.2), (
        f"Field filtering overhead too high: {overhead:.3f}ms ({(overhead / no_filter_avg) * 100:.1f}%)"
    )


def test_performance_canary():
    """Canary: Field filtering performance regression detector."""
    from fraiseql import _get_fraiseql_rs

    fraiseql_rs = _get_fraiseql_rs()

    fake_result = {
        "status": "success",
        "message": "Test",
        "entity_id": "123",
        "entity_type": "Test",
        "entity": {"id": "123"},
        "updated_fields": [],
        "cascade": None,
        "metadata": None,
        "is_simple_format": False,
    }

    # Single call should be < 1ms
    start = time.perf_counter()
    fraiseql_rs.build_mutation_response(
        json.dumps(fake_result),
        "test",
        "TestSuccess",
        "TestError",
        "entity",
        "Test",
        None,
        True,
        ["status"],
    )
    elapsed = (time.perf_counter() - start) * 1000

    print(f"✅ Single call: {elapsed:.3f}ms")

    # If this fails, field filtering has severe performance regression
    assert elapsed < 5.0, f"Field filtering is too slow: {elapsed:.3f}ms (expected < 5ms)"
