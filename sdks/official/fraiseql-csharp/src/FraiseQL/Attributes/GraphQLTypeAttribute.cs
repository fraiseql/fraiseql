namespace FraiseQL.Attributes;

/// <summary>
/// Marks a C# class as a GraphQL type for FraiseQL schema export.
/// When <see cref="FraiseQL.Registry.SchemaRegistry.Register"/> processes this class,
/// it reads this attribute to populate the type's metadata in the exported schema.
/// </summary>
/// <example>
/// <code>
/// [GraphQLType(Name = "Author", SqlSource = "v_author", Description = "A blog author")]
/// public class Author
/// {
///     [GraphQLField(Type = "ID", Nullable = false)]
///     public int Id { get; set; }
/// }
/// </code>
/// </example>
[AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, Inherited = false)]
public sealed class GraphQLTypeAttribute : Attribute
{
    /// <summary>Gets or sets the GraphQL type name. Defaults to the C# class name.</summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>Gets or sets the SQL view or table that backs this type (e.g. <c>v_author</c>).</summary>
    public string SqlSource { get; set; } = string.Empty;

    /// <summary>Gets or sets an optional human-readable description for the GraphQL schema.</summary>
    public string Description { get; set; } = string.Empty;

    /// <summary>Gets or sets whether this type is a GraphQL input type.</summary>
    public bool IsInput { get; set; } = false;

    /// <summary>Gets or sets whether this type implements Relay cursor-based pagination.</summary>
    public bool Relay { get; set; } = false;

    /// <summary>Gets or sets whether this type represents a mutation error variant.</summary>
    public bool IsError { get; set; } = false;

    /// <summary>Gets or sets whether this type is scoped to a tenant (multi-tenancy).</summary>
    public bool TenantScoped { get; set; } = false;

    /// <summary>
    /// Gets or sets CRUD operations to auto-generate for this type.
    /// Use <c>new[] { "all" }</c> for all operations, or specific ones like
    /// <c>new[] { "read", "create", "update", "delete" }</c>.
    /// </summary>
    public string[]? Crud { get; set; }

    /// <summary>
    /// Federation key fields for entity resolution.
    /// Defaults to <c>["id"]</c> when federation is enabled on export.
    /// Set explicitly for compound keys, e.g. <c>new[] { "id", "region" }</c>.
    /// </summary>
    public string[]? KeyFields { get; set; }

    /// <summary>
    /// Whether this type extends a type defined in another subgraph.
    /// </summary>
    public bool Extends { get; set; } = false;
}
