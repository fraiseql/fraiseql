# Rust Mutation Pipeline Examples

## Basic Usage

### Simple Entity Creation

```python
from fraiseql_rs import build_mutation_response
import json

# Simple format - just entity data
mutation_json = json.dumps({
    "id": "user-123",
    "name": "John Doe",
    "email": "john@example.com",
    "created_at": "2024-01-01T00:00:00Z"
})

result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="createUser",
    success_type="CreateUserSuccess",
    error_type="CreateUserError",
    entity_field_name="user",
    entity_type="User",
    auto_camel_case=True
)

# Result is bytes - parse to dict
response = json.loads(result.decode('utf-8'))
print(response)
```

**Output:**
```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserSuccess",
      "message": "Success",
      "user": {
        "__typename": "User",
        "id": "user-123",
        "name": "John Doe",
        "email": "john@example.com",
        "createdAt": "2024-01-01T00:00:00Z"
      }
    }
  }
}
```

### Full Format with Status

```python
# Full v2 format with explicit status
mutation_json = json.dumps({
    "status": "created",
    "message": "User account created successfully",
    "entity_type": "User",
    "entity": {
        "id": "user-123",
        "name": "John Doe",
        "email": "john@example.com"
    },
    "updated_fields": ["name", "email"],
    "metadata": {
        "operation_id": "op-456",
        "timestamp": "2024-01-01T00:00:00Z"
    }
})

result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="createUser",
    success_type="CreateUserSuccess",
    error_type="CreateUserError",
    entity_field_name="user",
    entity_type=None,  # Will use entity_type from JSON
    auto_camel_case=True
)

response = json.loads(result.decode('utf-8'))
print(json.dumps(response, indent=2))
```

**Output:**
```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserSuccess",
      "message": "User account created successfully",
      "user": {
        "__typename": "User",
        "id": "user-123",
        "name": "John Doe",
        "email": "john@example.com"
      },
      "updatedFields": ["name", "email"]
    }
  }
}
```

## Error Handling

### Validation Error

```python
mutation_json = json.dumps({
    "status": "validation:",
    "message": "Email address is already in use",
    "entity_id": null,
    "entity_type": null,
    "entity": null,
    "metadata": {
        "errors": [
            {
                "field": "email",
                "code": "duplicate",
                "message": "This email address is already registered"
            }
        ]
    }
})

result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="createUser",
    success_type="CreateUserSuccess",
    error_type="CreateUserError",
    entity_field_name="user",
    entity_type="User",
    auto_camel_case=True
)

response = json.loads(result.decode('utf-8'))
print(json.dumps(response, indent=2))
```

**Output:**
```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserError",
      "status": "validation:",
      "message": "Email address is already in use",
      "code": 422,
      "errors": [
        {
          "field": "email",
          "code": "duplicate",
          "message": "This email address is already registered"
        }
      ]
    }
  }
}
```

### Different Error Types

```python
# Unauthorized error
mutation_json = json.dumps({
    "status": "unauthorized:token_expired",
    "message": "Authentication token has expired",
    "entity_id": null,
    "entity_type": null,
    "entity": null
})

# Forbidden error
mutation_json = json.dumps({
    "status": "forbidden:insufficient_permissions",
    "message": "You don't have permission to perform this action"
})

# Not found error
mutation_json = json.dumps({
    "status": "not_found:user_missing",
    "message": "The requested user was not found"
})

# Conflict error
mutation_json = json.dumps({
    "status": "conflict:duplicate_email",
    "message": "A user with this email already exists"
})
```

## Cascade Data

### Update with Side Effects

```python
mutation_json = json.dumps({
    "status": "updated",
    "message": "User profile updated",
    "entity_type": "User",
    "entity": {
        "id": "user-123",
        "name": "John Smith",
        "email": "johnsmith@example.com"
    },
    "updated_fields": ["name", "email"],
    "cascade": {
        "updated": [
            {
                "id": "post-456",
                "author_name": "John Smith",
                "updated_at": "2024-01-01T00:00:00Z"
            }
        ],
        "invalidations": ["User:user-123", "Post:post-456"],
        "metadata": {
            "operation": "profile_update",
            "affected_entities": 2
        }
    }
})

result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="updateUser",
    success_type="UpdateUserSuccess",
    error_type="UpdateUserError",
    entity_field_name="user",
    entity_type="User",
    auto_camel_case=True
)

response = json.loads(result.decode('utf-8'))
print(json.dumps(response, indent=2))
```

**Output:**
```json
{
  "data": {
    "updateUser": {
      "__typename": "UpdateUserSuccess",
      "message": "User profile updated",
      "user": {
        "__typename": "User",
        "id": "user-123",
        "name": "John Smith",
        "email": "johnsmith@example.com"
      },
      "updatedFields": ["name", "email"],
      "cascade": {
        "updated": [
          {
            "id": "post-456",
            "authorName": "John Smith",
            "updatedAt": "2024-01-01T00:00:00Z"
          }
        ],
        "invalidations": ["User:user-123", "Post:post-456"],
        "metadata": {
          "operation": "profile_update",
          "affectedEntities": 2
        }
      }
    }
  }
}
```

## Array Entities

### Bulk Operations

```python
mutation_json = json.dumps([
    {
        "id": "user-1",
        "name": "Alice Johnson",
        "email": "alice@example.com"
    },
    {
        "id": "user-2",
        "name": "Bob Wilson",
        "email": "bob@example.com"
    },
    {
        "id": "user-3",
        "name": "Charlie Brown",
        "email": "charlie@example.com"
    }
])

result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="createUsers",
    success_type="CreateUsersSuccess",
    error_type="CreateUsersError",
    entity_field_name="users",
    entity_type="User",
    auto_camel_case=True
)

response = json.loads(result.decode('utf-8'))
print(json.dumps(response, indent=2))
```

**Output:**
```json
{
  "data": {
    "createUsers": {
      "__typename": "CreateUsersSuccess",
      "message": "Success",
      "users": [
        {
          "__typename": "User",
          "id": "user-1",
          "name": "Alice Johnson",
          "email": "alice@example.com"
        },
        {
          "__typename": "User",
          "id": "user-2",
          "name": "Bob Wilson",
          "email": "bob@example.com"
        },
        {
          "__typename": "User",
          "id": "user-3",
          "name": "Charlie Brown",
          "email": "charlie@example.com"
        }
      ]
    }
  }
}
```

## Noop Operations

### Unchanged Data

```python
mutation_json = json.dumps({
    "status": "noop:unchanged",
    "message": "No changes were needed",
    "entity_id": "user-123",
    "entity_type": "User",
    "entity": {
        "id": "user-123",
        "name": "John Doe",
        "email": "john@example.com"
    }
})

result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="updateUser",
    success_type="UpdateUserSuccess",
    error_type="UpdateUserError",
    entity_field_name="user",
    entity_type="User",
    auto_camel_case=True
)

response = json.loads(result.decode('utf-8'))
print(json.dumps(response, indent=2))
```

**Output:**
```json
{
  "data": {
    "updateUser": {
      "__typename": "UpdateUserSuccess",
      "message": "User updated",
      "user": {
        "__typename": "User",
        "id": "user-123",
        "name": "John Doe",
        "email": "john@example.com"
      },
      "updatedFields": []
    }
  }
}
```

## Advanced Features

### Custom Field Names

```python
# Use custom field names
result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="createAccount",  # Custom GraphQL field name
    success_type="AccountCreationSuccess",
    error_type="AccountCreationError",
    entity_field_name="account",  # Custom entity field name
    entity_type="Account",
    auto_camel_case=True
)
```

### Disable CamelCase Conversion

```python
# Keep original field names
result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="createUser",
    success_type="CreateUserSuccess",
    error_type="CreateUserError",
    entity_field_name="user",
    entity_type="User",
    auto_camel_case=False  # Keep snake_case
)
```

### Complex Nested Data

```python
mutation_json = json.dumps({
    "id": "user-123",
    "profile": {
        "personal": {
            "first_name": "John",
            "last_name": "Doe",
            "date_of_birth": "1990-01-01"
        },
        "contact": {
            "email": "john@example.com",
            "phone": "+1-555-0123"
        }
    },
    "preferences": {
        "notifications": {
            "email_enabled": true,
            "sms_enabled": false
        }
    }
})

result = build_mutation_response(
    mutation_json=mutation_json,
    field_name="createUser",
    success_type="CreateUserSuccess",
    error_type="CreateUserError",
    entity_field_name="user",
    entity_type="User",
    auto_camel_case=True
)

response = json.loads(result.decode('utf-8'))
print(json.dumps(response, indent=2))
```

**Output:**
```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserSuccess",
      "message": "Success",
      "user": {
        "__typename": "User",
        "id": "user-123",
        "profile": {
          "personal": {
            "firstName": "John",
            "lastName": "Doe",
            "dateOfBirth": "1990-01-01"
          },
          "contact": {
            "email": "john@example.com",
            "phone": "+1-555-0123"
          }
        },
        "preferences": {
          "notifications": {
            "emailEnabled": true,
            "smsEnabled": false
          }
        }
      }
    }
  }
}
```

## Integration with Web Frameworks

### FastAPI Example

```python
from fastapi import FastAPI, HTTPException
from fraiseql_rs import build_mutation_response
import json

app = FastAPI()

@app.post("/graphql")
async def graphql_endpoint(query: dict):
    # ... parse GraphQL query ...

    # Simulate database mutation result
    db_result = {
        "status": "created",
        "message": "User created",
        "entity": {
            "id": "123",
            "name": "John Doe",
            "email": "john@example.com"
        }
    }

    # Build GraphQL response using Rust pipeline
    mutation_json = json.dumps(db_result)
    result_bytes = build_mutation_response(
        mutation_json=mutation_json,
        field_name="createUser",
        success_type="CreateUserSuccess",
        error_type="CreateUserError",
        entity_field_name="user",
        entity_type="User"
    )

    # Return bytes directly (FastAPI handles serialization)
    return result_bytes
```

### Django Example

```python
from django.http import JsonResponse
from django.views.decorators.csrf import csrf_exempt
from fraiseql_rs import build_mutation_response
import json

@csrf_exempt
def graphql_view(request):
    if request.method != 'POST':
        return JsonResponse({'error': 'Method not allowed'}, status=405)

    # ... parse GraphQL query ...

    # Database operation
    db_result = perform_database_mutation(request_data)

    # Build response
    mutation_json = json.dumps(db_result)
    result_bytes = build_mutation_response(
        mutation_json=mutation_json,
        field_name=field_name,
        success_type=success_type,
        error_type=error_type,
        entity_field_name=entity_field,
        entity_type=entity_type
    )

    # Parse and return
    response_data = json.loads(result_bytes.decode('utf-8'))
    return JsonResponse(response_data)
```

## Performance Comparison

```python
import time
from fraiseql_rs import build_mutation_response
import json

# Test data
mutation_json = json.dumps({
    "status": "created",
    "entity": {"id": "123", "name": "Test", "email": "test@example.com"}
})

# Benchmark Rust pipeline
start = time.time()
for _ in range(1000):
    result = build_mutation_response(
        mutation_json=mutation_json,
        field_name="createUser",
        success_type="CreateUserSuccess",
        error_type="CreateUserError",
        entity_field_name="user",
        entity_type="User"
    )
rust_time = time.time() - start

print(f"Rust pipeline: {rust_time:.4f}s for 1000 operations")
print(f"Average: {rust_time/1000*1000:.2f}ms per operation")
```

Expected performance: ~0.5-2ms per operation depending on complexity.
