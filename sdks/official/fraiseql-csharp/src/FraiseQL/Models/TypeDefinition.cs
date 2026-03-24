namespace FraiseQL.Models;

/// <summary>
/// Represents a registered GraphQL type, produced by <see cref="FraiseQL.Registry.SchemaRegistry"/>
/// via reflection on <see cref="FraiseQL.Attributes.GraphQLTypeAttribute"/>.
/// </summary>
/// <param name="Name">The GraphQL type name.</param>
/// <param name="SqlSource">The backing SQL view or function (e.g. <c>v_author</c>).</param>
/// <param name="Description">Optional human-readable description.</param>
/// <param name="IsInput">Whether this is a GraphQL input type.</param>
/// <param name="Relay">Whether this type supports Relay cursor-based pagination.</param>
/// <param name="IsError">Whether this type is a mutation error variant.</param>
/// <param name="TenantScoped">Whether this type is scoped to a tenant.</param>
/// <param name="Fields">Ordered list of fields on this type.</param>
/// <param name="Crud">Optional CRUD operations to auto-generate.</param>
/// <param name="KeyFields">Optional federation key fields for entity resolution.</param>
/// <param name="Extends">Whether this type extends a type defined in another subgraph.</param>
public record TypeDefinition(
    string Name,
    string SqlSource,
    string? Description,
    bool IsInput,
    bool Relay,
    bool IsError,
    bool TenantScoped,
    IReadOnlyList<FieldDefinition> Fields,
    string[]? Crud = null,
    string[]? KeyFields = null,
    bool Extends = false);
