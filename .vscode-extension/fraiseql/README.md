# FraiseQL Tools for VS Code

Productivity tools for writing FraiseQL mutations in PostgreSQL.

## Features

### Snippets

- `fraiseql-mutation` - Complete mutation function template
- `fraiseql-error-simple` - Simple error (Pattern 1)
- `fraiseql-error-explicit` - Explicit errors (Pattern 2)
- `fraiseql-not-found` - Not found check
- `fraiseql-duplicate` - Duplicate check
- `fraiseql-assert` - Validation assertion
- `fraiseql-extract` - Extract from input_payload
- `fraiseql-success` - Success response
- `fraiseql-collect-errors` - Multiple errors collection

### Autocomplete

Type `fraiseql-` in a .sql file to see all available snippets.

### Syntax Highlighting

Status strings are highlighted:
- `'created'` - Green
- `'failed:validation'` - Red
- `'not_found:user'` - Orange
- `'conflict:duplicate'` - Yellow

## Installation

### From VSIX
1. Download `fraiseql-tools-1.0.0.vsix`
2. VS Code → Extensions → ... → Install from VSIX

### From Marketplace
1. Search "FraiseQL Tools"
2. Click Install

## Usage

1. Open .sql file
2. Type `fraiseql-` to see snippets
3. Use Tab to navigate placeholders
4. Use dropdown for status string options

## Examples

### Create Mutation Function
```sql
-- Type: fraiseql-mutation
-- Result: Complete function template
```

### Add Validation
```sql
-- Type: fraiseql-error-simple
-- Result: Simple error return
```

### Collect Errors
```sql
-- Type: fraiseql-collect-errors
-- Result: Error collection pattern
```

## Documentation

- [Mutation SQL Requirements](https://github.com/fraiseql/fraiseql/docs/guides/mutation-sql-requirements.md)
- [Quick Reference](https://github.com/fraiseql/fraiseql/docs/quick-reference/mutations-cheat-sheet.md)

## Feedback

Issues: https://github.com/fraiseql/fraiseql/issues
