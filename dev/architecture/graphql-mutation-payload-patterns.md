# GraphQL Mutation Payload Patterns
## Production Patterns from PrintOptim Backend

This document describes the client-side GraphQL mutation patterns observed in production code, covering query structure, payload handling, and response patterns.

---

## 1. Standard Mutation Structure

All mutations follow this pattern:

```graphql
mutation OperationName($input: InputTypeName!) {
    operationName(input: $input) {
        __typename
        ... on SuccessTypeName {
            message
            entity { id name }
        }
        ... on ErrorTypeName {
            message
            errors { code identifier message }
        }
    }
}
```

**Key Elements:**
- Single `$input` variable with exact type
- Union/interface response requiring `__typename`
- Inline fragments for success/error cases

---

## 2. Payload Patterns

### Variable Passing

**Always use variables:**
```python
input_data = {"name": "value", "id": "uuid"}
result = await client.execute(mutation, variables={"input": input_data})
```

### Field Handling

| Scenario | Python | GraphQL | Behavior |
|----------|--------|---------|----------|
| **Omitted** | Key not in dict | Not sent | Unchanged in update |
| **Null** | `"field": None` | `null` | Set to NULL |
| **Empty String** | `"field": ""` | `""` | Converted to null |

**Example:**
```python
# Update only name, leave other fields unchanged
input_data = {
    "id": "uuid",
    "name": "New Name"
    # other fields omitted = unchanged
}

# Clear a field
input_data = {
    "id": "uuid",
    "description": None  # Sets to NULL
}
```

---

## 3. Response Patterns

### Success Response
```graphql
... on CreateEntitySuccess {
    message
    entity {
        id
        identifier
        name
    }
}
```

**Python:**
```python
result = await client.execute(mutation, variables)
assert "errors" not in result

mutation_result = result["data"]["createEntity"]
if mutation_result["__typename"] == "CreateEntitySuccess":
    entity = mutation_result["entity"]
```

### Error Response
```graphql
... on CreateEntityError {
    message
    conflictEntity { id name }  # For duplicates
    errors {
        code        # 404, 409, 422
        identifier  # Machine-readable
        message     # Human-readable
        details     # Additional data
    }
}
```

**Python:**
```python
if mutation_result["__typename"] == "CreateEntityError":
    error = mutation_result["errors"][0]

    if error["code"] == 409:
        conflict = mutation_result["conflictEntity"]
    elif error["code"] == 404:
        # Not found
        pass
```

---

## 4. Common Operations

### Create
```graphql
mutation CreateLocation($input: CreateLocationInput!) {
    createLocation(input: $input) {
        __typename
        ... on CreateLocationSuccess {
            message
            location {
                id
                name
                address {
                    id
                    streetNumber
                    city
                }
            }
        }
        ... on CreateLocationError {
            message
            conflictLocation { id name }
            errors { code identifier message }
        }
    }
}
```

```python
input_data = {
    "name": "Building A",
    "locationLevelId": "uuid",
    "address": {  # Nested object
        "streetNumber": "42",
        "streetName": "Main St",
        "city": "Paris"
    }
}
```

### Update
```graphql
mutation UpdateLocation($input: UpdateLocationInput!) {
    updateLocation(input: $input) {
        __typename
        ... on UpdateLocationSuccess {
            message
            location {
                id
                name
                hasElevator
            }
        }
        ... on UpdateLocationError {
            message
            errors { code identifier message }
        }
    }
}
```

```python
input_data = {
    "id": "uuid",
    "name": "Updated Name",
    "hasElevator": True
    # Other fields omitted = unchanged
}
```

### Delete
```graphql
mutation DeleteLocation($input: DeletionInput!) {
    deleteLocation(input: $input) {
        __typename
        ... on DeleteLocationSuccess {
            message
            deletedId
        }
        ... on DeleteLocationError {
            errors {
                code
                identifier
                message
                details
            }
        }
    }
}
```

```python
input_data = {"id": "uuid"}
```

---

## 5. Best Practices

### ✅ Always Do
1. Use variables, not inline values
2. Check `"errors"` in result first
3. Check `__typename` to discriminate unions
4. Use `null` to clear fields (not empty strings)
5. Omit fields in updates to leave unchanged
6. Request `errors` array for error details

### ❌ Never Do
1. Inline mutation arguments
2. Assume success without checking `__typename`
3. Use empty strings for null
4. Ignore `errors` array
5. Request all fields (only request what you need)

---

## 6. Error Handling

```python
# 1. Check GraphQL-level errors
if "errors" in result:
    # Schema/validation errors
    pytest.fail(f"GraphQL error: {result['errors']}")

# 2. Check operation result
mutation_result = result["data"]["operationName"]

# 3. Handle union response
if mutation_result["__typename"] == "OperationSuccess":
    # Success path
    entity = mutation_result["entity"]
elif mutation_result["__typename"] == "OperationError":
    # Error path
    error = mutation_result["errors"][0]

    match error["code"]:
        case 409:
            # Duplicate
            conflict = mutation_result["conflictEntity"]
        case 422:
            # Validation/constraint
            details = error["details"]
        case 404:
            # Not found
            pass
```

---

## 7. Testing Patterns

### Verify Success
```python
assert create_result["__typename"] == "CreateEntitySuccess"
entity = create_result["entity"]
assert entity["id"] is not None
assert entity["name"] == input_data["name"]
```

### Verify Duplicate Detection
```python
# Create first time
result1 = await client.execute(mutation, variables={"input": input_data})
assert result1["data"]["createEntity"]["__typename"] == "CreateEntitySuccess"

# Create again - should fail
result2 = await client.execute(mutation, variables={"input": input_data})
error_result = result2["data"]["createEntity"]

assert error_result["__typename"] == "CreateEntityError"
assert error_result["errors"][0]["code"] == 409
assert error_result["conflictEntity"]["id"] is not None
```

---

## 8. Domain-Specific Examples

### Geographic (with nested address)
```python
input_data = {
    "locationLevelId": "uuid",
    "name": "Building",
    "address": {
        "streetNumber": "15",
        "streetName": "de Rivoli",
        "postalCode": "75001",
        "city": "Paris",
        "latitude": 48.8584,
        "longitude": 2.2945
    }
}
```

### Hierarchical (with path recalculation)
```python
input_data = {
    "id": "org-unit-uuid",
    "parentId": "new-parent-uuid"  # Triggers ltree path update
}

# Response includes computed fields
... on UpdateOrganizationalUnitSuccess {
    organizationalUnit {
        ltreePath     # PostgreSQL ltree
        ltreeNlevel   # Depth
        pathOfNames   # Ancestor names
    }
}
```

### Temporal (SCD pattern)
```python
input_data = {
    "id": "allocation-uuid",
    "startDate": "2025-02-01",  # ISO date
    "endDate": "2025-11-30"
}

# Response includes temporal flags
... on UpdateAllocationSuccess {
    allocation {
        isCurrent   # now between start/end
        isPast      # end < now
        isFuture    # start > now
    }
    previousAllocation {  # Historical record
        endDate    # Truncated
    }
}
```

---

**Source**: PrintOptim Backend production codebase
**Analyzed**: 10+ mutation test files across 5 domains
**Validated**: geo, org, network, mat, scd patterns
