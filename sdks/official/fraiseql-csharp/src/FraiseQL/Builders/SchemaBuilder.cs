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

        var mergedQueries = registryQueries
            .Concat(_queries.Select(q => q.Build()))
            .ToList();

        var mergedMutations = registryMutations
            .Concat(_mutations.Select(m => m.Build()))
            .ToList();

        // Generate CRUD operations for fluent types that have crud enabled
        foreach (var tc in _types.Where(t => t.HasCrud))
        {
            var (crudQueries, crudMutations) = CrudGenerator.Generate(
                tc.Name, tc.GetFields(), tc.GetSqlSource(), tc.HasCascade);
            mergedQueries.AddRange(crudQueries);
            mergedMutations.AddRange(crudMutations);
        }

        return new IntermediateSchema("2.0.0", allTypes, mergedQueries.AsReadOnly(), mergedMutations.AsReadOnly());
    }

    /// <summary>
    /// Exports the schema to a JSON string.
    /// </summary>
    /// <param name="pretty">When <see langword="true"/>, the JSON is indented.</param>
    /// <returns>The JSON string.</returns>
    public string Export(bool pretty = true) =>
        SchemaExporter.Serialize(ToSchema(), pretty);

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

        return new IntermediateType(td.Name, td.SqlSource, td.Description, fields);
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
    private bool _crud;
    private bool _cascade;
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

    /// <summary>Enables auto-generation of CRUD operations for this type.</summary>
    /// <param name="crud">Whether to generate CRUD operations.</param>
    /// <returns>This configurator for chaining.</returns>
    public TypeConfigurator Crud(bool crud = true) { _crud = crud; return this; }

    /// <summary>Enables cascade support on generated CRUD mutations.</summary>
    /// <param name="cascade">Whether generated mutations include cascade.</param>
    /// <returns>This configurator for chaining.</returns>
    public TypeConfigurator Cascade(bool cascade = true) { _cascade = cascade; return this; }

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

    internal bool HasCrud => _crud;
    internal bool HasCascade => _cascade;
    internal IReadOnlyList<IntermediateField> GetFields() => _fields.AsReadOnly();
    internal string GetSqlSource() => _sqlSource;

    internal IntermediateType Build() =>
        new(Name, _sqlSource, _description, _fields.AsReadOnly());
}
