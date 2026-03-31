namespace FraiseQL.Export;

/// <summary>
/// Configuration for Apollo Federation schema export.
/// When passed to <see cref="SchemaExporter"/>,
/// a top-level <c>"federation"</c> block is emitted in the JSON with
/// <c>enabled: true</c> and an auto-derived <c>entities</c> list.
/// </summary>
/// <remarks>
/// Each registered type becomes a federation entity. Types with explicit
/// <c>KeyFields</c> use those; all others default to <c>["id"]</c>.
/// Error types (<see cref="Attributes.GraphQLTypeAttribute.IsError"/>) are skipped.
/// </remarks>
/// <example>
/// <code>
/// var federation = new FederationConfig("users-service");
/// SchemaExporter.ExportToFile("schema.json", federation);
/// </code>
/// </example>
public sealed class FederationConfig
{
    /// <summary>Gets the logical name of this subgraph (e.g. "users-service").</summary>
    public string ServiceName { get; }

    /// <summary>Gets the Apollo Federation spec version. Defaults to <c>"v2"</c>.</summary>
    public string Version { get; }

    /// <summary>
    /// Gets the default key fields applied to types that do not declare their own
    /// <c>KeyFields</c>. Defaults to <c>["id"]</c>.
    /// </summary>
    public IReadOnlyList<string> DefaultKeyFields { get; }

    /// <summary>
    /// Creates a new federation configuration.
    /// </summary>
    /// <param name="serviceName">Logical name of this subgraph (e.g. "users-service").</param>
    /// <param name="version">Apollo Federation spec version. Defaults to <c>"v2"</c>.</param>
    /// <param name="defaultKeyFields">
    /// Default key fields for types without explicit <c>KeyFields</c>.
    /// Defaults to <c>["id"]</c> when <see langword="null"/>.
    /// </param>
    public FederationConfig(
        string serviceName,
        string version = "v2",
        IReadOnlyList<string>? defaultKeyFields = null)
    {
        ServiceName = serviceName;
        Version = version;
        DefaultKeyFields = defaultKeyFields ?? new[] { "id" };
    }
}
