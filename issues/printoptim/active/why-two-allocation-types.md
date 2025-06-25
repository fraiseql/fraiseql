# Question: Why Two Allocation Types?

Following up on the JSONB nested objects discussion - I'm confused about why we need two separate types for Allocation.

## Current Understanding

You suggested this structure:

```python
@fraiseql.type
class Allocation:
    """Allocation type representing machine item allocations."""
    # Direct fields from table columns
    id: UUID
    machine_id: Optional[UUID]
    # ... other direct fields ...
    
    # The JSONB column containing nested data
    data: Optional["AllocationData"]

@fraiseql.type
class AllocationData:
    """Nested data structure within Allocation."""
    identifier: Optional[str]
    start_date: Optional[date]
    # ... other nested fields ...
    machine: Optional[Machine]
    location: Optional[Location]
    # ... etc
```

## The Question

Why do we need two types (`Allocation` and `AllocationData`)? 

The `data` JSONB column contains the "real" allocation information (identifier, dates, machine, location, etc.), while the outer type mostly has IDs and flags. It seems like we're representing one conceptual entity (an allocation) with two types.

Is there a way to have just one `Allocation` type that combines both the direct columns and the nested JSONB data? 

Or is the two-type pattern required because of how FraiseQL handles JSONB columns?

## Ideal Structure?

Would something like this be possible, where FraiseQL knows some fields come from columns and others from the JSONB data?

```python
@fraiseql.type
class Allocation:
    # From direct columns
    id: UUID
    machine_id: Optional[UUID]
    is_current: bool
    # ... etc
    
    # From data JSONB column
    identifier: Optional[str]
    machine: Optional[Machine]
    location: Optional[Location]
    # ... etc
```