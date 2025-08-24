# PrintOptim Backend Error Arrays - Complete Examples

This document demonstrates the **intended architecture** for error arrays in FraiseQL, showing exactly how PrintOptim Backend handles multiple validation errors as structured arrays.

## 🎯 The Problem We're Solving

Instead of returning **single error responses**, we need to return **arrays of errors** that capture ALL validation issues in one request, providing comprehensive feedback to clients.

## 📋 PrintOptim Backend Error Array Structure

Based on the actual PrintOptim Backend codebase, errors should be returned as arrays with this structure:

```typescript
// GraphQL Response Structure
{
  "data": {
    "createAuthor": {
      "__typename": "CreateAuthorError",
      "message": "Author creation failed validation",
      "errors": [  // ← ARRAY of structured error objects
        {
          "code": 422,
          "identifier": "missing_required_field",
          "message": "Missing required field: identifier",
          "details": {
            "field": "identifier",
            "constraint": "required"
          }
        },
        {
          "code": 422,
          "identifier": "missing_required_field",
          "message": "Missing required field: name",
          "details": {
            "field": "name",
            "constraint": "required"
          }
        },
        {
          "code": 422,
          "identifier": "invalid_email_format",
          "message": "Invalid email format: not-an-email",
          "details": {
            "field": "email",
            "constraint": "format",
            "value": "not-an-email"
          }
        }
      ]
    }
  }
}
```

## 🏗️ Database Function Implementation

The PostgreSQL functions collect ALL validation errors before returning:

```sql
-- Enhanced mutation result type with errors array
CREATE TYPE app.mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB,
    errors JSONB  -- ← Array of structured error objects
);

-- Validation accumulator pattern
CREATE TYPE core.validation_result AS (
    is_valid BOOLEAN,
    errors JSONB  -- Array of error objects
);

-- Core validation function with error accumulation
CREATE OR REPLACE FUNCTION core.create_author_with_validation(
    input_created_by UUID,
    input_data app.type_author_input,
    input_payload JSONB
) RETURNS app.mutation_result
LANGUAGE plpgsql AS $$
DECLARE
    v_validation core.validation_result;
BEGIN
    -- Initialize validation accumulator
    v_validation := (true, '[]'::JSONB)::core.validation_result;

    -- Collect ALL validation errors (don't return early)
    IF input_data.identifier IS NULL THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'missing_required_field',
            'Missing required field: identifier',
            jsonb_build_object('field', 'identifier', 'constraint', 'required')
        );
    END IF;

    IF input_data.name IS NULL THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'missing_required_field',
            'Missing required field: name',
            jsonb_build_object('field', 'name', 'constraint', 'required')
        );
    END IF;

    IF input_data.email IS NULL OR NOT core.is_valid_email(input_data.email) THEN
        v_validation := core.add_validation_error(
            v_validation, 422, 'invalid_email_format',
            format('Invalid email format: %s', input_data.email),
            jsonb_build_object('field', 'email', 'constraint', 'format', 'value', input_data.email)
        );
    END IF;

    -- Return ALL errors at once if validation failed
    IF NOT v_validation.is_valid THEN
        RETURN core.log_and_return_mutation_with_errors(
            'author', gen_random_uuid(), 'NOOP', 'noop:validation_failed',
            ARRAY[]::TEXT[], 'Author creation failed validation',
            NULL, NULL,
            jsonb_build_object('validation_errors_count', jsonb_array_length(v_validation.errors)),
            v_validation.errors  -- ← Pass the full errors array
        );
    END IF;

    -- ... proceed with creation if validation passed
END;
$$;
```

## 📊 Complete Error Response Examples

### Example 1: Multiple Missing Required Fields

```json
{
  "data": {
    "createAuthor": {
      "__typename": "CreateAuthorError",
      "message": "Author creation failed validation",
      "errors": [
        {
          "code": 422,
          "identifier": "missing_required_field",
          "message": "Missing required field: identifier",
          "details": {
            "field": "identifier",
            "constraint": "required"
          }
        },
        {
          "code": 422,
          "identifier": "missing_required_field",
          "message": "Missing required field: name",
          "details": {
            "field": "name",
            "constraint": "required"
          }
        },
        {
          "code": 422,
          "identifier": "missing_required_field",
          "message": "Missing required field: email",
          "details": {
            "field": "email",
            "constraint": "required"
          }
        }
      ],
      "validationSummary": {
        "total_errors": 3,
        "field_errors": {
          "identifier": ["Missing required field: identifier"],
          "name": ["Missing required field: name"],
          "email": ["Missing required field: email"]
        },
        "constraint_violations": {
          "required": 3
        },
        "has_validation_errors": true,
        "has_conflicts": false
      }
    }
  }
}
```

### Example 2: Mixed Validation and Business Rule Errors

```json
{
  "data": {
    "createPost": {
      "__typename": "CreatePostError",
      "message": "Post creation failed validation",
      "errors": [
        {
          "code": 422,
          "identifier": "missing_required_field",
          "message": "Missing required field: identifier",
          "details": {
            "field": "identifier",
            "constraint": "required"
          }
        },
        {
          "code": 422,
          "identifier": "title_too_long",
          "message": "Title too long: 250 characters (maximum 200)",
          "details": {
            "field": "title",
            "constraint": "max_length",
            "max_length": 200,
            "current_length": 250
          }
        },
        {
          "code": 422,
          "identifier": "content_too_long",
          "message": "Content too long: 10001 characters (maximum 10000)",
          "details": {
            "field": "content",
            "constraint": "max_length",
            "max_length": 10000,
            "current_length": 10001
          }
        },
        {
          "code": 422,
          "identifier": "missing_author",
          "message": "Author with identifier \"missing-author\" not found",
          "details": {
            "field": "author_identifier",
            "constraint": "foreign_key",
            "missing_identifier": "missing-author"
          }
        },
        {
          "code": 422,
          "identifier": "invalid_tag",
          "message": "Tag with identifier \"missing-tag-1\" not found",
          "details": {
            "field": "tag_identifiers",
            "constraint": "foreign_key",
            "missing_identifier": "missing-tag-1"
          }
        },
        {
          "code": 422,
          "identifier": "invalid_tag",
          "message": "Tag with identifier \"missing-tag-2\" not found",
          "details": {
            "field": "tag_identifiers",
            "constraint": "foreign_key",
            "missing_identifier": "missing-tag-2"
          }
        }
      ],
      "validationSummary": {
        "total_errors": 6,
        "field_errors": {
          "identifier": ["Missing required field: identifier"],
          "title": ["Title too long: 250 characters (maximum 200)"],
          "content": ["Content too long: 10001 characters (maximum 10000)"],
          "author_identifier": ["Author with identifier \"missing-author\" not found"],
          "tag_identifiers": [
            "Tag with identifier \"missing-tag-1\" not found",
            "Tag with identifier \"missing-tag-2\" not found"
          ]
        },
        "constraint_violations": {
          "required": 1,
          "max_length": 2,
          "foreign_key": 3
        },
        "has_validation_errors": true,
        "has_conflicts": false
      }
    }
  }
}
```

### Example 3: Security Validation Errors

```json
{
  "data": {
    "createPost": {
      "__typename": "CreatePostError",
      "message": "Post creation failed validation",
      "errors": [
        {
          "code": 422,
          "identifier": "unsafe_html",
          "message": "Content contains potentially unsafe HTML: script tags not allowed",
          "details": {
            "field": "content",
            "constraint": "security",
            "violation": "script_tag"
          }
        },
        {
          "code": 422,
          "identifier": "unsafe_javascript",
          "message": "Content contains potentially unsafe JavaScript URIs",
          "details": {
            "field": "content",
            "constraint": "security",
            "violation": "javascript_uri"
          }
        },
        {
          "code": 422,
          "identifier": "path_traversal",
          "message": "Content contains potential path traversal attack",
          "details": {
            "field": "content",
            "constraint": "security",
            "violation": "path_traversal"
          }
        }
      ],
      "securityViolations": ["script_tag", "javascript_uri", "path_traversal"],
      "validationSummary": {
        "total_errors": 3,
        "security_issues": ["script_tag", "javascript_uri", "path_traversal"],
        "constraint_violations": {
          "security": 3
        }
      }
    }
  }
}
```

### Example 4: Conflict Errors with Validation

```json
{
  "data": {
    "createAuthor": {
      "__typename": "CreateAuthorError",
      "message": "Author creation failed validation",
      "errors": [
        {
          "code": 409,
          "identifier": "duplicate_identifier",
          "message": "Author with identifier \"existing-author\" already exists",
          "details": {
            "field": "identifier",
            "constraint": "unique",
            "conflict_id": "12345678-1234-1234-1234-123456789012",
            "conflict_identifier": "existing-author"
          }
        },
        {
          "code": 409,
          "identifier": "duplicate_email",
          "message": "Author with email \"existing@example.com\" already exists",
          "details": {
            "field": "email",
            "constraint": "unique",
            "conflict_id": "12345678-1234-1234-1234-123456789012",
            "conflict_email": "existing@example.com"
          }
        },
        {
          "code": 422,
          "identifier": "name_too_long",
          "message": "Name too long: 150 characters (maximum 100)",
          "details": {
            "field": "name",
            "constraint": "max_length",
            "max_length": 100,
            "current_length": 150
          }
        }
      ],
      "conflictAuthor": {
        "id": "12345678-1234-1234-1234-123456789012",
        "identifier": "existing-author",
        "name": "Existing Author"
      },
      "validationSummary": {
        "total_errors": 3,
        "has_conflicts": true,
        "has_validation_errors": true,
        "constraint_violations": {
          "unique": 2,
          "max_length": 1
        }
      }
    }
  }
}
```

## 🎯 Key Benefits of Error Arrays

1. **Complete Feedback**: Clients get ALL validation issues in one request
2. **Structured Information**: Each error has code, identifier, message, details
3. **Field-Level Grouping**: Clients can group errors by field for UI display
4. **Constraint Classification**: Errors are categorized by constraint type
5. **Debugging Context**: Rich metadata helps with troubleshooting
6. **Programmatic Handling**: Machine-readable identifiers enable code branching

## 🔧 Client-Side Usage Patterns

### React Hook Form Integration

```typescript
// Map error array to React Hook Form errors
function mapValidationErrors(errorArray: Error[]) {
  const fieldErrors: Record<string, { message: string }> = {};

  errorArray.forEach(error => {
    const field = error.details?.field;
    if (field) {
      fieldErrors[field] = { message: error.message };
    }
  });

  return fieldErrors;
}

// Usage in component
const { data } = await createAuthor({ variables: { input } });

if (data.createAuthor.__typename === "CreateAuthorError") {
  const fieldErrors = mapValidationErrors(data.createAuthor.errors);
  setError(fieldErrors); // React Hook Form
}
```

### Error Summary Display

```typescript
// Group errors by type for summary display
function createErrorSummary(errors: Error[]) {
  const summary = {
    validation: errors.filter(e => e.code === 422),
    conflicts: errors.filter(e => e.code === 409),
    security: errors.filter(e => e.details?.constraint === 'security'),
    byField: {} as Record<string, Error[]>
  };

  // Group by field
  errors.forEach(error => {
    const field = error.details?.field || 'general';
    if (!summary.byField[field]) summary.byField[field] = [];
    summary.byField[field].push(error);
  });

  return summary;
}
```

## 📝 Testing the Error Array Implementation

The test suite in `test_error_arrays.py` demonstrates:

- ✅ Multiple validation errors in single response
- ✅ Mixed error types (validation + conflicts + security)
- ✅ Consistent error object structure
- ✅ Field-level error grouping
- ✅ Performance with many errors
- ✅ Empty arrays for successful operations

## 🎓 Summary

This error array architecture provides:

1. **Comprehensive Validation**: All errors captured in one request
2. **Structured Response**: Consistent error object format
3. **Rich Metadata**: Detailed context for debugging and client handling
4. **Performance**: Single request for complete validation feedback
5. **Developer Experience**: Clear patterns for client-side error handling

The PrintOptim Backend demonstrates this pattern throughout its mutation system, providing a robust foundation for comprehensive error handling in GraphQL APIs.
