namespace FraiseQL.Attributes;

/// <summary>
/// Declares a relationship to another type, followed by REST resource embedding (#1266).
/// </summary>
/// <remarks>
/// <para>
/// <see cref="Name"/> is what a client writes in <c>?select=orders(id,total)</c>,
/// <c>?select=orders.count</c> and <c>?orders.status=paid</c>; it is also what the
/// generated client's <c>relationships</c> module and the served OpenAPI document publish.
/// </para>
/// <para>
/// <see cref="ForeignKey"/> and <see cref="ReferencedKey"/> are SQL <b>column</b> names,
/// and which side each is read from swaps with the cardinality — <c>OneToMany</c> reads
/// <see cref="ReferencedKey"/> off the declaring type and filters <see cref="ForeignKey"/>
/// on the target; <c>ManyToOne</c> and <c>OneToOne</c> do the reverse. Under the default
/// <c>camelCase</c> naming convention the column <c>fk_user</c> is published as the field
/// <c>fkUser</c>, and the compiler resolves one to the other.
/// </para>
/// <para>
/// Which relationships are <i>followable</i> is the compiler's business, not this SDK's:
/// it refuses a target type it does not declare, a join column no field on that side
/// publishes, and a target no list query returns. This SDK carries no second copy of
/// those rules; a copy is what drifts.
/// </para>
/// </remarks>
/// <example>
/// <code>
/// [GraphQLType(Name = "User", SqlSource = "v_user")]
/// [GraphQLRelationship(Name = "orders", TargetType = "Order",
///     Cardinality = "OneToMany", ForeignKey = "fk_user", ReferencedKey = "id")]
/// public class User { }
/// </code>
/// </example>
[AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct, AllowMultiple = true, Inherited = false)]
public sealed class GraphQLRelationshipAttribute : Attribute
{
    /// <summary>The cardinalities a relationship may declare.</summary>
    public static readonly IReadOnlyList<string> Cardinalities = ["OneToMany", "ManyToOne", "OneToOne"];

    /// <summary>Gets or sets the relationship name — the key in <c>?select=</c> and in the response.</summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the target GraphQL type name. Must be a declared type that some
    /// <b>list</b> query returns: an embed sources its rows from that query.
    /// </summary>
    public string TargetType { get; set; } = string.Empty;

    /// <summary>Gets or sets one of <c>OneToMany</c>, <c>ManyToOne</c>, <c>OneToOne</c>.</summary>
    public string Cardinality { get; set; } = string.Empty;

    /// <summary>Gets or sets the foreign key <b>column</b> on the child table, e.g. <c>fk_user</c>.</summary>
    public string ForeignKey { get; set; } = string.Empty;

    /// <summary>Gets or sets the referenced key <b>column</b> on the parent table, e.g. <c>id</c>.</summary>
    public string ReferencedKey { get; set; } = string.Empty;
}
