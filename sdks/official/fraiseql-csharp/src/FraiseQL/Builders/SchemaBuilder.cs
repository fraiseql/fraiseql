using FraiseQL.Export;
using FraiseQL.Models;
using FraiseQL.Registry;

namespace FraiseQL.Builders;

/// <summary>
/// Fluent code-first builder for constructing a complete GraphQL schema without using
/// <see cref="FraiseQL.Attributes.GraphQLTypeAttribute"/> reflection.
/// </summary>
/// <remarks>
/// <para>
/// <see cref="SchemaBuilder"/> is an alternative to attribute-based registration.
/// On <see cref="ToSchema"/>, it merges its own accumulated types/queries/mutations
/// with anything already registered in <see cref="SchemaRegistry.Instance"/>.
/// If a type name is registered both ways, the fluent registration wins.
/// </para>
/// </remarks>
/// <example>
/// <code>
/// var schema = new SchemaBuilder()
///     .Type("Author", t => t
///         .SqlSource("v_author")
///         .Description("A blog author")
///         .Field("id", "ID", nullable: false)
///         .Field("name", "String", nullable: false))
///     .Query("authors", q => q
///         .ReturnType("Author")
///         .ReturnsList()
///         .SqlSource("v_author"))
///     .ToSchema();
/// </code>
/// </example>
public sealed class SchemaBuilder
{
    private readonly List<TypeConfigurator> _types = new();
    private readonly List<QueryBuilder> _queries = new();
    private readonly List<MutationBuilder> _mutations = new();

    /// <summary>
    /// Adds a type to the schema using a fluent configurator.
    /// </summary>
    /// <param name="name">The GraphQL type name.</param>
    /// <param name="configure">A delegate that configures the type via <see cref="TypeConfigurator"/>.</param>
    /// <returns>This builder for chaining.</returns>
    public SchemaBuilder Type(string name, Action<TypeConfigurator> configure)
    {
        var configurator = new TypeConfigurator(name);
        configure(configurator);
        _types.Add(configurator);
        return this;
    }

    /// <summary>
    /// Adds a query to the schema using a fluent <see cref="QueryBuilder"/>.
    /// </summary>
    /// <param name="name">The GraphQL query name.</param>
    /// <param name="configure">A delegate that configures the query.</param>
    /// <returns>This builder for chaining.</returns>
    public SchemaBuilder Query(string name, Action<QueryBuilder> configure)
    {
        var builder = QueryBuilder.Query(name);
        configure(builder);
        _queries.Add(builder);
        return this;
    }

    /// <summary>
    /// Adds a mutation to the schema using a fluent <see cref="MutationBuilder"/>.
    /// </summary>
    /// <param name="name">The GraphQL mutation name.</param>
    /// <param name="configure">A delegate that configures the mutation.</param>
    /// <returns>This builder for chaining.</returns>
    public SchemaBuilder Mutation(string name, Action<MutationBuilder> configure)
    {
        var builder = MutationBuilder.Mutation(name);
        configure(builder);
        _mutations.Add(builder);
        return this;
    }

    /// <summary>
    /// Builds an <see cref="IntermediateSchema"/> by merging fluent registrations with
    /// anything already in <see cref="SchemaRegistry.Instance"/>.
    /// Fluent registrations win on name conflicts.
    /// </summary>
    /// <returns>The merged intermediate schema.</returns>
    public IntermediateSchema ToSchema()
    {
        var fluentTypeNames = new HashSet<string>(_types.Select(t => t.Name));
        var fluentQueryNames = new HashSet<string>(_queries.Select(q => q.GetName()));
        var fluentMutationNames = new HashSet<string>(_mutations.Select(m => m.GetName()));

        // Start with registry types not overridden by fluent types
        var registryTypes = SchemaRegistry.Instance.GetAllTypes()
            .Where(t => !fluentTypeNames.Contains(t.Name))
            .Select(TypeDefinitionToIntermediate)
            .ToList();

        var registryQueries = SchemaRegistry.Instance.GetAllQueries()
            .Where(q => !fluentQueryNames.Contains(q.Name))
            .ToList();

        var registryMutations = SchemaRegistry.Instance.GetAllMutations()
            .Where(m => !fluentMutationNames.Contains(m.Name))
            .ToList();

        // Merge fluent additions
        var allTypes = registryTypes
            .Concat(_types.Select(c => c.Build()))
            .ToList()
            .AsReadOnly();

        var allQueries = registryQueries
            .Concat(_queries.Select(q => q.Build()))
            .ToList()
            .AsReadOnly();

        var allMutations = registryMutations
            .Concat(_mutations.Select(m => m.Build()))
            .ToList()
            .AsReadOnly();

        return new IntermediateSchema("2.0.0", allTypes, allQueries, allMutations);
    }

    /// <summary>
    /// Exports the schema to a JSON string.
    /// </summary>
    /// <param name="pretty">When <see langword="true"/>, the JSON is indented.</param>
    /// <returns>The JSON string.</returns>
    public string Export(bool pretty = true) =>
        SchemaExporter.Serialize(ToSchema(), pretty);

    /// <summary>
    /// Exports the schema with federation metadata to a JSON string.
    /// </summary>
    /// <param name="federation">Federation configuration specifying the service name and defaults.</param>
    /// <param name="pretty">When <see langword="true"/>, the JSON is indented.</param>
    /// <returns>The JSON string with federation block.</returns>
    public string Export(FederationConfig federation, bool pretty = true) =>
        SchemaExporter.Serialize(ApplyFederation(ToSchema(), federation), pretty);

    /// <summary>
    /// Exports the schema to a file at the given path. Parent directories are created automatically.
    /// </summary>
    /// <param name="path">The output file path.</param>
    /// <param name="pretty">When <see langword="true"/>, the JSON is indented.</param>
    public void ExportToFile(string path, bool pretty = true)
    {
        var dir = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(dir))
            Directory.CreateDirectory(dir);

        File.WriteAllText(path, Export(pretty));
    }

    /// <summary>
    /// Exports the schema with federation metadata to a file.
    /// Parent directories are created automatically.
    /// </summary>
    /// <param name="path">The output file path.</param>
    /// <param name="federation">Federation configuration specifying the service name and defaults.</param>
    /// <param name="pretty">When <see langword="true"/>, the JSON is indented.</param>
    public void ExportToFile(string path, FederationConfig federation, bool pretty = true)
    {
        var dir = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(dir))
            Directory.CreateDirectory(dir);

        File.WriteAllText(path, Export(federation, pretty));
    }

    /// <summary>
    /// Builds a federation block from the schema types and attaches it.
    /// Uses the fluent-built types (which lack IsError), plus registry types for error filtering.
    /// </summary>
    private static IntermediateSchema ApplyFederation(
        IntermediateSchema schema, FederationConfig federation)
    {
        var registry = SchemaRegistry.Instance;
        var registryErrorNames = new HashSet<string>(
            registry.GetAllTypes().Where(t => t.IsError).Select(t => t.Name));

        var entities = new List<FederationEntity>();
        foreach (var typeDef in schema.Types)
        {
            // Skip error types — they are not federation entities
            if (registryErrorNames.Contains(typeDef.Name))
                continue;

            var keyFields = typeDef.KeyFields is { Count: > 0 }
                ? typeDef.KeyFields
                : federation.DefaultKeyFields;

            entities.Add(new FederationEntity(typeDef.Name, keyFields));
        }

        var apolloVersion = federation.Version == "v2" ? 2 : 1;

        var block = new FederationBlock(
            Enabled: true,
            ServiceName: federation.ServiceName,
            ApolloVersion: apolloVersion,
            Entities: entities.AsReadOnly());

        return schema with { Federation = block };
    }

    private static IntermediateType TypeDefinitionToIntermediate(
        FraiseQL.Models.TypeDefinition td)
    {
        var fields = td.Fields
            .Select(f => new IntermediateField(
                Name: f.Name,
                Type: f.Type,
                Nullable: f.Nullable,
                Description: f.Description,
                Resolver: f.Resolver,
                Scope: f.Scope,
                Scopes: f.Scopes))
            .ToList()
            .AsReadOnly();

        return new IntermediateType(td.Name, td.SqlSource, td.Description, fields,
            td.TenantScoped ? true : null,
            KeyFields: td.KeyFields is { Length: > 0 }
                ? td.KeyFields.ToList().AsReadOnly()
                : null);
    }
}

/// <summary>
/// Configures a GraphQL type within a <see cref="SchemaBuilder"/> definition.
/// </summary>
public sealed class TypeConfigurator
{
    internal string Name { get; }
    private string _sqlSource = string.Empty;
    private string? _description;
    private string[]? _keyFields;
    private bool _extends;
    private readonly List<IntermediateField> _fields = new();

    internal TypeConfigurator(string name) => Name = name;

    /// <summary>Sets the backing SQL view name.</summary>
    /// <param name="source">The SQL view name (e.g. <c>"v_author"</c>).</param>
    /// <returns>This configurator for chaining.</returns>
    public TypeConfigurator SqlSource(string source) { _sqlSource = source; return this; }

    /// <summary>Sets a human-readable description for this type.</summary>
    /// <param name="desc">The description text.</param>
    /// <returns>This configurator for chaining.</returns>
    public TypeConfigurator Description(string desc) { _description = desc; return this; }

    /// <summary>
    /// Sets federation key fields for entity resolution.
    /// Defaults to <c>["id"]</c> when federation is enabled on export.
    /// Set explicitly for compound keys, e.g. <c>new[] { "id", "region" }</c>.
    /// </summary>
    /// <param name="fields">The key field names.</param>
    /// <returns>This configurator for chaining.</returns>
    public TypeConfigurator KeyFields(params string[] fields) { _keyFields = fields; return this; }

    /// <summary>
    /// Marks this type as extending a type defined in another subgraph.
    /// </summary>
    /// <param name="extends_">Whether this type extends a remote type.</param>
    /// <returns>This configurator for chaining.</returns>
    public TypeConfigurator Extends(bool extends_ = true) { _extends = extends_; return this; }

    /// <summary>Adds a field to this type.</summary>
    /// <param name="name">The GraphQL field name.</param>
    /// <param name="type">The GraphQL type name.</param>
    /// <param name="nullable">Whether the field is nullable.</param>
    /// <param name="description">Optional field description.</param>
    /// <param name="scope">Optional required OAuth scope.</param>
    /// <returns>This configurator for chaining.</returns>
    public TypeConfigurator Field(
        string name,
        string type,
        bool nullable = false,
        string? description = null,
        string? scope = null)
    {
        _fields.Add(new IntermediateField(name, type, nullable, description, null, scope));
        return this;
    }

    internal IntermediateType Build() =>
        new(Name, _sqlSource, _description, _fields.AsReadOnly(),
            KeyFields: _keyFields is { Length: > 0 }
                ? _keyFields.ToList().AsReadOnly()
                : null);
}
