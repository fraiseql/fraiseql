# Response: Type Instantiation Issue with Nested Objects in v0.1.0a17

## Issue Confirmed

You've identified a critical limitation in FraiseQL's current architecture. The repository layer attempts to instantiate complete objects with all required fields, regardless of which fields were requested in the GraphQL query. This violates GraphQL's core principle of allowing clients to request only the data they need.

## Root Cause

The issue stems from the separation of concerns between:
1. **GraphQL Layer**: Knows which fields were requested (selection set)
2. **Repository Layer**: Performs type instantiation but has no knowledge of the selection set

When `_instantiate_recursive` is called in the repository, it tries to create a complete Python object with all required fields, causing errors when the JSONB data doesn't contain fields that weren't requested.

## Immediate Workaround

Until this is fixed in FraiseQL, here are your options:

### Option 1: Make Nested Fields Optional (Recommended Short-term)
```python
@fraiseql.type
class Machine:
    id: uuid.UUID
    identifier: str
    machine_serial_number: Optional[str] = None  # Make optional
    model: Optional[Model] = None  # Make optional
    # ... other fields as optional
```

This allows partial instantiation but you're right that it loses type safety.

### Option 2: Use Production Mode for Complex Queries
In production mode, FraiseQL returns raw dictionaries instead of instantiated objects:
```python
# Set environment to production for specific queries
context = {"mode": "production"}
```

### Option 3: Create Separate "Partial" Types
```python
@fraiseql.type
class MachineBasic:
    id: uuid.UUID
    identifier: str

@fraiseql.type  
class AllocationWithBasicMachine:
    id: uuid.UUID
    identifier: str
    start_date: date
    machine: MachineBasic  # Uses simplified type
```

## Proposed Fix for FraiseQL

The proper solution requires architectural changes to FraiseQL:

### 1. Pass Selection Set to Repository
The GraphQL resolver needs to pass the selection set information to the repository:

```python
# In the query resolver
selected_fields = extract_selected_fields(info)
results = await db.find("tv_allocation", 
    selected_fields=selected_fields,  # New parameter
    limit=limit
)
```

### 2. Partial Object Instantiation
The repository should only instantiate requested fields:

```python
def _instantiate_partial(self, type_class: type, data: dict, selected_fields: dict):
    """Create object with only selected fields."""
    # Filter data to only include selected fields
    filtered_data = self._filter_by_selection(data, selected_fields)
    
    # Create object with partial data
    # This might require using a different instantiation method
    # that doesn't validate required fields
```

### 3. Alternative: Lazy Loading Pattern
Another approach would be to create proxy objects that only materialize fields when accessed:

```python
class LazyProxy:
    def __init__(self, data, type_class):
        self._data = data
        self._type = type_class
    
    def __getattr__(self, name):
        # Materialize field on access
        if name in self._data:
            return self._instantiate_field(name)
        raise AttributeError(f"Field {name} was not requested")
```

## Why This Matters

This issue is fundamental because:
1. **Performance**: Forces over-fetching of data
2. **Usability**: Makes nested queries practically unusable
3. **GraphQL Compliance**: Violates the core GraphQL principle of client-specified queries

## Next Steps

1. **For PrintOptim Team**: Use Option 1 (optional fields) as a temporary workaround
2. **For FraiseQL**: This needs to be addressed in the core architecture

I'll create a ticket for implementing proper partial object instantiation in FraiseQL. This is a significant architectural change that will require careful design to maintain backward compatibility.

## Technical Notes

The challenge is that Python's dataclass instantiation expects all required fields to be present. FraiseQL needs to either:
- Use a different instantiation mechanism for partial objects
- Intercept the instantiation process to provide defaults for missing fields
- Move to a lazy-loading pattern where fields are only materialized when accessed

This is a common challenge when bridging GraphQL's flexible field selection with Python's strict type system.