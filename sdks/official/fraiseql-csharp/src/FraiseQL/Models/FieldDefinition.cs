namespace FraiseQL.Models;

/// <summary>
/// Represents a single field on a GraphQL type, as produced by <see cref="FraiseQL.Registry.SchemaRegistry"/>
/// via reflection on <see cref="FraiseQL.Attributes.GraphQLFieldAttribute"/>.
/// </summary>
/// <param name="Name">The GraphQL field name (camelCase).</param>
/// <param name="Type">The GraphQL scalar or type name (e.g. <c>"String"</c>, <c>"ID"</c>).</param>
/// <param name="Nullable">Whether the field may return <c>null</c> in the GraphQL schema.</param>
/// <param name="Description">Optional human-readable description.</param>
/// <param name="Resolver">Optional custom resolver name for computed fields.</param>
/// <param name="Scope">Optional single required OAuth scope.</param>
/// <param name="Scopes">Optional multiple required OAuth scopes.</param>
/// <param name="Computed">
/// When <see langword="true"/>, this field is server-computed and excluded from CRUD input types.
/// Computed fields remain visible in query results.
/// </param>
/// <param name="Vector">Optional pgvector configuration, on a vector-typed field.</param>
/// <param name="VectorDistance">
/// Optional name of the vector field whose search distance this <c>Float</c> field carries.
/// </param>
/// <param name="Deprecated">
/// Optional deprecation. When present the field surfaces as <c>isDeprecated</c> /
/// <c>deprecationReason</c> through introspection.
/// </param>
public record FieldDefinition(
    string Name,
    string Type,
    bool Nullable,
    string? Description,
    string? Resolver,
    string? Scope,
    IReadOnlyList<string>? Scopes,
    bool Computed = false,
    VectorConfig? Vector = null,
    string? VectorDistance = null,
    DeprecationInfo? Deprecated = null);
