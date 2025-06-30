# Response: GraphQL Mutation Return Types in FraiseQL

## Summary

**FraiseQL already fully supports returning complete objects from mutations!** No framework changes are needed. The printoptim_backend project can immediately start using this feature.

## Key Findings

### 1. Built-in Support Exists

FraiseQL's mutation parser (`src/fraiseql/mutations/parser.py`) automatically:
- Detects object fields in success/error types
- Maps `object_data` JSONB to typed Python objects
- Handles nested relationships and complex types
- Supports lists of objects

### 2. How It Works

When a PostgreSQL function returns:
```sql
result.object_data := (SELECT data FROM tv_location WHERE id = location_id);
```

FraiseQL automatically instantiates:
```python
CreateLocationSuccess(
    location=Location(...),  # Full object from object_data
    message="Location created successfully"
)
```

### 3. No Additional Queries

The SQL functions **already return complete data** in the `object_data` field. The GraphQL layer has been discarding this valuable data by only using the ID field.

## Implementation Guide for printoptim_backend

### Step 1: Update Success/Error Types

```python
# Before (current implementation)
@fraiseql.success
class CreateLocationSuccess(MutationResultBase):
    location_id: UUID | None = None
    message: str
    status: str

# After (recommended implementation)
@fraiseql.success
class CreateLocationSuccess(MutationResultBase):
    location: Location | None = None  # Full object instead of ID
    message: str
    status: str
    location_id: UUID | None = None  # Keep for backward compatibility
```

### Step 2: Update Error Types for Conflicts

```python
@fraiseql.failure
class CreateLocationError(MutationResultBase):
    message: str
    status: str
    errors: list[Error]
    conflict_location: Location | None = None  # Include existing entity
```

### Step 3: No Resolver Changes Needed!

The existing resolver code will work automatically:
```python
result = await repo.call_function("create_location_with_log", input_dict)
# FraiseQL automatically maps object_data to the location field
return CreateLocationSuccess(**result)
```

### Step 4: SQL Functions Already Return Full Data

The ticket confirms SQL functions already populate `object_data`:
```sql
-- After creating location
CALL refresh_location();
SELECT data INTO v_payload_after FROM tv_location WHERE id = v_id;

RETURN (
    v_id,
    v_updated_fields,
    v_status,
    v_description,
    v_payload_after,    -- Complete location data here!
    v_extra_metadata
);
```

## Benefits

1. **Zero Performance Cost**: Data is already being fetched and returned
2. **Immediate Implementation**: No SQL changes needed
3. **Better Developer Experience**: Clients get complete data in one call
4. **Cache-Friendly**: Apollo/Relay can update caches immediately

## Migration Strategy

### Phase 1: Add New Fields (Backward Compatible)
```python
@fraiseql.success
class CreateLocationSuccess:
    location: Location | None = None     # New
    location_id: UUID | None = None      # Deprecated but kept
    message: str
    status: str
```

### Phase 2: Update Clients
- Update GraphQL queries to request full objects
- Remove separate query calls after mutations

### Phase 3: Remove Deprecated Fields
- Remove ID-only fields once all clients updated

## Example GraphQL Query

```graphql
# Before: Requires second query
mutation CreateLocation($input: CreateLocationInput!) {
    createLocation(input: $input) {
        ... on CreateLocationSuccess {
            locationId  # Only get ID
            message
        }
    }
}

# Separate query needed
query GetLocation($id: ID!) {
    location(id: $id) {
        id
        name
        address { ... }
    }
}

# After: Complete in one call
mutation CreateLocation($input: CreateLocationInput!) {
    createLocation(input: $input) {
        ... on CreateLocationSuccess {
            location {  # Full object!
                id
                name
                identifier
                address {
                    streetName
                    city
                    postalCode
                }
                parent {
                    id
                    name
                }
            }
            message
        }
    }
}
```

## Documentation Updates

I've created comprehensive documentation in FraiseQL:

1. **New Guide**: `/docs/mutations/returning-full-objects.md` - Complete guide with examples
2. **Updated**: `/docs/mutations/postgresql-function-based.md` - Added section on returning objects
3. **Updated**: `/docs/mutations/index.md` - Added link to new guide

## Conclusion

The printoptim_backend project can immediately start returning full objects from mutations. The infrastructure is already in place - it just needs to update the type definitions to include object fields instead of just IDs.

This will significantly improve the developer experience and reduce the number of GraphQL queries needed after mutations.
