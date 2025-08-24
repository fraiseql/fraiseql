# Error Arrays RED Phase - Complete Implementation Guide

This guide documents the RED phase tests for **error arrays** - the intended architecture where multiple validation errors are returned as structured arrays following PrintOptim Backend patterns.

## 🎯 The Problem Statement

Instead of returning single errors like this:
```json
{
  "data": {
    "createPost": {
      "__typename": "CreatePostError", 
      "message": "Missing required field: identifier",
      "errorCode": "MISSING_REQUIRED_FIELD"
    }
  }
}
```

We need to return **arrays of errors** like this:
```json
{
  "data": {
    "createPost": {
      "__typename": "CreatePostError",
      "message": "Post creation failed validation", 
      "errors": [  // ← ARRAY of structured errors
        {
          "code": 422,
          "identifier": "missing_required_field",
          "message": "Missing required field: identifier",
          "details": { "field": "identifier", "constraint": "required" }
        },
        {
          "code": 422,
          "identifier": "title_too_long",
          "message": "Title too long: 250 characters (maximum 200)",
          "details": { "field": "title", "constraint": "max_length", "max_length": 200, "current_length": 250 }
        },
        {
          "code": 422,
          "identifier": "unsafe_html",
          "message": "Content contains potentially unsafe HTML: script tags not allowed",
          "details": { "field": "content", "constraint": "security", "violation": "script_tag" }
        }
      ],
      "validationSummary": {
        "totalErrors": 3,
        "hasValidationErrors": true,
        "fieldErrors": {
          "identifier": ["Missing required field: identifier"],
          "title": ["Title too long: 250 characters (maximum 200)"],
          "content": ["Content contains potentially unsafe HTML: script tags not allowed"]
        }
      }
    }
  }
}
```

## 🔴 RED Phase Test Categories

### 1. Multiple Validation Error Arrays

**File**: `test_red_phase_error_arrays.py::TestRedPhaseMultipleValidationErrorArrays`

Tests that demonstrate collecting **ALL** validation errors instead of stopping at the first one:

#### Test: `test_create_author_multiple_missing_fields_returns_error_array`
```python
# Input with multiple missing required fields
{
  "input": {
    "bio": "This author is missing required fields"
    # Missing: identifier, name, email
  }
}

# Expected: Array with 3 structured error objects
"errors": [
  {
    "code": 422,
    "identifier": "missing_required_field", 
    "message": "Missing required field: identifier",
    "details": { "field": "identifier", "constraint": "required" }
  },
  {
    "code": 422,
    "identifier": "missing_required_field",
    "message": "Missing required field: name", 
    "details": { "field": "name", "constraint": "required" }
  },
  {
    "code": 422,
    "identifier": "missing_required_field",
    "message": "Missing required field: email",
    "details": { "field": "email", "constraint": "required" }
  }
]
```

#### Test: `test_create_author_mixed_validation_types_returns_structured_array`
```python
# Input with different types of validation errors
{
  "identifier": "INVALID-CAPS-AND-TOO-LONG-OVER-FIFTY-CHARACTERS-LIMIT",  # Format + length
  "name": "A" * 150,  # Too long (max 100)  
  "email": "not-a-valid-email-format"  # Invalid format
}

# Expected: Array with different error types and codes
"errors": [
  {
    "code": 422,
    "identifier": "invalid_identifier_format",
    "message": "Identifier must contain only lowercase letters, numbers, and hyphens",
    "details": { "field": "identifier", "constraint": "format", "pattern": "^[a-z0-9-]+$" }
  },
  {
    "code": 422, 
    "identifier": "identifier_too_long",
    "message": "Identifier too long: 65 characters (maximum 50)",
    "details": { "field": "identifier", "constraint": "max_length", "max_length": 50, "current_length": 65 }
  },
  // ... more errors
]
```

#### Test: `test_create_post_comprehensive_validation_array_with_security_errors`
```python
# Input with multiple error categories
{
  # Missing: identifier  
  "title": "A" * 250,  # Too long
  "content": very_long_content + '<script>alert("xss")</script>' + '../../../etc/passwd',
  "authorIdentifier": "non-existent-author",  # Missing reference
  "tagIdentifiers": ["missing-tag-1", "missing-tag-2"],  # Missing references
  "status": "invalid-status"  # Invalid enum
}

# Expected: Comprehensive error array with different categories
"errors": [
  { /* missing identifier */ },
  { /* title too long */ },
  { /* content too long */ },
  { /* unsafe html - script tag */ },
  { /* path traversal */ },
  { /* missing author */ },
  { /* invalid tag 1 */ },
  { /* invalid tag 2 */ },
  { /* invalid status */ }
]
```

### 2. Mixed Error Types (Validation + Conflicts)

#### Test: `test_create_author_conflicts_with_validation_errors_mixed_array`
```python
# Setup: Create existing author first
# Then: Try to create with conflicts AND validation errors

{
  "identifier": "existing-author",  # 409 Conflict
  "name": "B" * 150,              # 422 Validation (too long)
  "email": "existing@example.com" # 409 Conflict
}

# Expected: Mix of 409 (conflict) and 422 (validation) errors
"errors": [
  {
    "code": 409,
    "identifier": "duplicate_identifier", 
    "message": "Author with identifier \"existing-author\" already exists",
    "details": { "field": "identifier", "constraint": "unique", "conflict_id": "..." }
  },
  {
    "code": 409,
    "identifier": "duplicate_email",
    "message": "Author with email \"existing@example.com\" already exists", 
    "details": { "field": "email", "constraint": "unique", "conflict_id": "..." }
  },
  {
    "code": 422,
    "identifier": "name_too_long",
    "message": "Name too long: 150 characters (maximum 100)",
    "details": { "field": "name", "constraint": "max_length", "max_length": 100, "current_length": 150 }
  }
]
```

### 3. Error Array Structure Consistency

**File**: `test_red_phase_error_arrays.py::TestRedPhaseErrorArrayStructure`

#### Test: `test_error_array_structure_follows_printoptim_patterns`
Validates that every error object follows the PrintOptim Backend structure:
- ✅ `code`: Integer HTTP status code
- ✅ `identifier`: Snake_case machine-readable identifier
- ✅ `message`: Human-readable description
- ✅ `details`: Structured context with `field` and `constraint`

#### Test: `test_success_response_has_empty_errors_array`
Ensures successful operations return `errors: []` (empty array, not null).

### 4. Field-Level Error Grouping

**File**: `test_red_phase_error_arrays.py::TestRedPhaseFieldLevelErrorGrouping`

#### Test: `test_validation_summary_groups_errors_by_field`
```python
# Expected validation summary structure
"validationSummary": {
  "totalErrors": 5,
  "fieldErrors": {
    "identifier": ["Format error", "Length error"],
    "title": ["Too long error"],
    "content": ["Security error", "Length error"]
  },
  "constraintViolations": {
    "format": 1,
    "max_length": 3,
    "security": 1
  },
  "hasValidationErrors": true,
  "hasConflicts": false
}
```

### 5. Security Validation Arrays

**File**: `test_red_phase_error_arrays.py::TestRedPhaseSecurityValidationArrays`

#### Test: `test_multiple_security_violations_in_structured_array`
```python
# Content with multiple security issues
dangerous_content = '''
<script>alert("XSS");</script>
<a href="javascript:void(0)">Link</a>  
<img src="../../../etc/passwd" />
'''

# Expected: Multiple security errors with violation details
"errors": [
  {
    "code": 422,
    "identifier": "unsafe_html",
    "message": "Content contains potentially unsafe HTML: script tags not allowed",
    "details": { "field": "content", "constraint": "security", "violation": "script_tag" }
  },
  {
    "code": 422,
    "identifier": "unsafe_javascript", 
    "message": "Content contains potentially unsafe JavaScript URIs",
    "details": { "field": "content", "constraint": "security", "violation": "javascript_uri" }
  },
  {
    "code": 422,
    "identifier": "path_traversal",
    "message": "Content contains potential path traversal attack", 
    "details": { "field": "content", "constraint": "security", "violation": "path_traversal" }
  }
],
"securityViolations": ["script_tag", "javascript_uri", "path_traversal"]
```

### 6. Performance with Error Arrays

**File**: `test_red_phase_error_arrays.py::TestRedPhasePerformanceWithErrorArrays`

#### Test: `test_many_validation_errors_handled_efficiently`
```python
# Input that generates 100+ validation errors
{
  "identifier": "X" * 100,        # Too long
  "title": "Y" * 300,            # Too long  
  "content": "Z" * 15000,        # Too long
  "authorIdentifier": "missing", # Missing reference
  "tagIdentifiers": [f"missing-tag-{i}" for i in range(100)],  # 100 missing
  "status": "invalid"            # Invalid enum
}

# Expected: Efficient handling of 105+ errors in < 5 seconds
"validationSummary": {
  "totalErrors": 105  // 100 tag errors + 5 other validation errors
}
```

## 🏗️ Implementation Requirements

To make these RED phase tests pass, the GREEN phase must implement:

### 1. Enhanced Database Functions

```sql
-- Error accumulation pattern
CREATE TYPE core.validation_result AS (
    is_valid BOOLEAN,
    errors JSONB  -- Array of error objects
);

-- Function to add errors to accumulator
CREATE FUNCTION core.add_validation_error(
    current_result core.validation_result,
    error_code INTEGER,
    error_identifier TEXT,
    error_message TEXT,
    error_details JSONB
) RETURNS core.validation_result;

-- Enhanced mutation result with errors array
CREATE TYPE app.mutation_result AS (
    id UUID,
    updated_fields TEXT[],
    status TEXT,
    message TEXT,
    object_data JSONB,
    extra_metadata JSONB,
    errors JSONB  -- Array of structured error objects
);
```

### 2. Comprehensive Validation Logic

```sql
-- Pattern: Collect ALL errors before returning
CREATE FUNCTION core.create_author_with_validation(...) AS $$
DECLARE
    v_validation core.validation_result;
BEGIN
    -- Initialize empty validation result
    v_validation := (true, '[]'::JSONB)::core.validation_result;
    
    -- Collect ALL validation errors (don't return early!)
    IF input_data.identifier IS NULL THEN
        v_validation := core.add_validation_error(v_validation, ...);
    END IF;
    
    IF input_data.name IS NULL THEN
        v_validation := core.add_validation_error(v_validation, ...);
    END IF;
    
    IF NOT core.is_valid_email(input_data.email) THEN
        v_validation := core.add_validation_error(v_validation, ...);
    END IF;
    
    -- Check business rules
    -- Check conflicts
    -- etc.
    
    -- Return ALL errors at once if validation failed
    IF NOT v_validation.is_valid THEN
        RETURN (..., v_validation.errors);  -- Pass full errors array
    END IF;
END;
$$;
```

### 3. Enhanced GraphQL Types

```python
@fraiseql.type
class Error:
    code: int
    identifier: str
    message: str
    details: dict[str, Any] | None = None

@fraiseql.failure  
class CreateAuthorEnhancedError(MutationResultBase):
    message: str
    errors: list[Error]  # Array of structured errors
    validation_summary: dict[str, Any] | None = None
    conflict_author: Author | None = None
```

### 4. Error Categorization Logic

```python
def create_validation_summary(errors: list[Error]) -> dict[str, Any]:
    field_errors = {}
    constraint_violations = {}
    security_issues = []
    
    for error in errors:
        field = error.details.get("field") if error.details else None
        constraint = error.details.get("constraint") if error.details else None
        
        if field:
            if field not in field_errors:
                field_errors[field] = []
            field_errors[field].append(error.message)
        
        if constraint:
            constraint_violations[constraint] = constraint_violations.get(constraint, 0) + 1
            
            if constraint == "security":
                violation = error.details.get("violation")
                if violation:
                    security_issues.append(violation)
    
    return {
        "total_errors": len(errors),
        "field_errors": field_errors,
        "constraint_violations": constraint_violations,
        "security_issues": security_issues or None,
        "has_validation_errors": any(e.code == 422 for e in errors),
        "has_conflicts": any(e.code == 409 for e in errors)
    }
```

## 🎯 Expected Benefits

Once implemented, the error arrays architecture provides:

1. **Complete Validation Feedback**: Users get ALL validation errors in one request
2. **Structured Error Information**: Each error has code, identifier, message, details
3. **Field-Level Grouping**: Errors can be grouped by field for UI display
4. **Constraint Classification**: Errors categorized by validation type
5. **Security Context**: Security violations clearly identified
6. **Performance**: Single request for comprehensive validation
7. **Developer Experience**: Rich debugging information and programmatic handling

## 🚀 Running the RED Phase

```bash
# Run the error arrays RED phase tests
./run_red_phase_error_arrays.py

# Expected output: ALL TESTS SHOULD FAIL
# This demonstrates the intended architecture before implementation
```

The RED phase failures show exactly what the error arrays architecture should provide - comprehensive, structured, categorized validation feedback in arrays following PrintOptim Backend patterns!

---

*This RED phase defines the target architecture for comprehensive error handling with structured arrays. The failures demonstrate exactly what needs to be implemented in the GREEN phase.*