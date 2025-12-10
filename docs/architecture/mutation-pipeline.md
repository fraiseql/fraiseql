# FraiseQL Mutation Pipeline Architecture

This document describes the mutation response processing pipeline in FraiseQL, including format auto-detection, response building, and GraphQL integration.

## Overview

FraiseQL supports two mutation response formats that are automatically detected and processed:

## Glossary

**Two Formats Only** (no versioning):

- **Simple Format**: Entity-only JSONB response
  - No `status` field
  - Entire JSON is the entity
  - Auto-detected when status field missing or invalid
  - Example: `{"id": "123", "name": "John"}`

- **Full Format**: Complete mutation_response type
  - Has `status` field with valid mutation status
  - Includes message, entity_type, entity, cascade, metadata
  - Auto-detected when valid status field present
  - Example: `{"status": "created", "message": "User created", "entity": {...}}`

**Historical Note**: You may see "v2 format" in older code/tests. This refers to "Full format" and should be updated.

**Not to be confused with**:
- ❌ Format versioning (there is no v1, v2, v3)
- ❌ API versioning (this is format auto-detection, not versions)

## Pipeline Architecture

The mutation pipeline processes PostgreSQL function responses through several layers:

1. **Format Detection**: Automatically determines if response is Simple or Full format
2. **Parsing**: Extracts status, message, entity, and metadata
3. **Response Building**: Constructs GraphQL-compliant JSON responses
4. **Type Integration**: Maps to Success/Error GraphQL types

## Implementation Details

### Format Detection

Format detection is based on the presence and validity of a `status` field:

- **Simple Format**: No `status` field, or `status` contains invalid mutation status
- **Full Format**: Valid `status` field with recognized mutation status

### Status Taxonomy

See `docs/mutations/status-strings.md` for complete status string documentation.

### Response Building

The Rust pipeline builds GraphQL responses with:
- Proper `__typename` fields
- CamelCase field transformation
- Array handling for entity collections
- Cascade data inclusion
- Error handling and HTTP status codes

## Testing

Comprehensive test coverage includes:
- Format detection edge cases
- Status parsing and validation
- Response building for all mutation types
- Property-based testing for invariants
- Integration tests with Python layer
