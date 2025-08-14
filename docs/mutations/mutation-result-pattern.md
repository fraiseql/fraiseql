# Mutation Result Pattern

> **In this section:** Implement the standardized mutation result pattern for enterprise-grade applications
> **Prerequisites:** Understanding of PostgreSQL types, CQRS principles, GraphQL mutations
> **Time to complete:** 45 minutes

Complete guide to implementing FraiseQL's standardized mutation result pattern, based on enterprise-proven patterns from production systems. This pattern provides consistent mutation responses, comprehensive metadata, audit trails, and structured NOOP handling.

## Overview

The Mutation Result Pattern establishes a standardized structure for all mutation responses in FraiseQL applications. Unlike ad-hoc JSON returns, this pattern provides:

- **Consistent Response Structure** - All mutations return the same `app.mutation_result` type
- **Rich Metadata** - Complete audit trails and debugging information
- **Field-Level Change Tracking** - Know exactly which fields were modified
- **Structured NOOP Handling** - Graceful handling of edge cases and validation failures
- **Enterprise Audit Support** - Complete change history for compliance requirements

## Type Definition

### The app.mutation_result Type

[Content placeholder - SQL type structure will be documented here]

### Status Code Semantics

[Content placeholder - All status codes and their meanings]

## Logging Function

### Core Logging Mechanism

[Content placeholder - core.log_and_return_mutation function documentation]

### Function Signature

[Content placeholder - Complete function signature and parameters]

## GraphQL Integration

### Python Resolver Patterns

[Content placeholder - How to parse mutation_result in resolvers]

### Success/Error Type Mapping

[Content placeholder - Converting mutation_result to GraphQL union types]

## Metadata Patterns

### Extra Metadata Structure

[Content placeholder - What goes in extra_metadata field]

### Debugging Information

[Content placeholder - Debug context patterns]

## Change Tracking

### Updated Fields Array

[Content placeholder - How updated_fields array works]

### Field-Level Auditing

[Content placeholder - Tracking specific field changes]

## Examples

### Simple Create Mutation

[Content placeholder - Complete create example]

### Update with Change Tracking

[Content placeholder - Update example with field tracking]

### NOOP Handling Scenario

[Content placeholder - NOOP example with proper status codes]

### Complex Business Logic

[Content placeholder - Advanced mutation example]

## Error Handling Patterns

### Validation Failures

[Content placeholder - Handling validation errors]

### Business Rule Violations

[Content placeholder - Business logic error patterns]

## Migration Guide

### Converting Existing Mutations

[Content placeholder - How to migrate from ad-hoc returns]

### Backward Compatibility

[Content placeholder - Maintaining compatibility during migration]

## Best Practices

### Do's and Don'ts

[Content placeholder - Best practices for using mutation result pattern]

### Performance Considerations

[Content placeholder - Performance implications and optimizations]

## Troubleshooting

### Common Issues

[Content placeholder - Common problems and solutions]

### Debugging Techniques

[Content placeholder - How to debug mutation result issues]

## Integration Points

### Authentication and Authorization

[Content placeholder - How mutation results work with auth]

### Multi-Tenant Patterns

[Content placeholder - Tenant context in mutation results]

### Cache Invalidation

[Content placeholder - Cache invalidation with mutation results]

## See Also

- [PostgreSQL Function-Based Mutations](./postgresql-function-based.md) - Core mutation patterns
- [Migration Guide](./migration-guide.md) - Converting existing mutations
- [Multi-Tenancy](../advanced/multi-tenancy.md) - Tenant context patterns
- [CQRS](../advanced/cqrs.md) - Command-query separation principles
