using System.Reflection;
using FraiseQL.Attributes;
using FraiseQL.Models;

namespace FraiseQL.Registry;

/// <summary>
/// Thread-safe singleton registry that accumulates GraphQL types, queries, and mutations
/// for export via <see cref="FraiseQL.Export.SchemaExporter"/>.
/// </summary>
/// <remarks>
/// This is a singleton — use <see cref="Instance"/> to access it.
/// Call <see cref="Clear"/> between tests to prevent cross-test pollution.
/// </remarks>
public sealed class SchemaRegistry
{
    private static readonly Lazy<SchemaRegistry> LazyInstance =
        new(() => new SchemaRegistry(), LazyThreadSafetyMode.ExecutionAndPublication);

    /// <summary>Gets the global singleton instance of the registry.</summary>
    public static SchemaRegistry Instance => LazyInstance.Value;

    private readonly object _lock = new();
    private readonly List<TypeDefinition> _types = new();
    private readonly List<IntermediateQuery> _queries = new();
    private readonly List<IntermediateMutation> _mutations = new();
    private Dictionary<string, string>? _injectDefaultsBase;
    private Dictionary<string, string>? _injectDefaultsQueries;
    private Dictionary<string, string>? _injectDefaultsMutations;

    private SchemaRegistry() { }

    /// <summary>
    /// Registers a C# type decorated with <see cref="GraphQLTypeAttribute"/> into the registry.
    /// All properties decorated with <see cref="GraphQLFieldAttribute"/> are included as fields.
    /// Properties without the attribute are ignored.
    /// </summary>
    /// <param name="type">The C# type to register.</param>
    /// <exception cref="InvalidOperationException">
    /// Thrown when <paramref name="type"/> is not decorated with <see cref="GraphQLTypeAttribute"/>.
    /// </exception>
    public void Register(Type type)
    {
        var typeAttr = type.GetCustomAttribute<GraphQLTypeAttribute>()
            ?? throw new InvalidOperationException(
                $"Type '{type.Name}' is not decorated with [GraphQLType]");

        var typeName = string.IsNullOrEmpty(typeAttr.Name) ? type.Name : typeAttr.Name;
        var fields = BuildFields(type);

        var definition = new TypeDefinition(
            Name: typeName,
            SqlSource: typeAttr.SqlSource,
            Description: string.IsNullOrEmpty(typeAttr.Description) ? null : typeAttr.Description,
            IsInput: typeAttr.IsInput,
            Relay: typeAttr.Relay,
            IsError: typeAttr.IsError,
            Fields: fields);

        lock (_lock)
        {
            _types.Add(definition);
        }

        if (typeAttr.Crud)
        {
            var intermediateFields = fields
                .Select(f => new IntermediateField(f.Name, f.Type, f.Nullable, f.Description, f.Resolver, f.Scope, f.Scopes))
                .ToList()
                .AsReadOnly();

            var (crudQueries, crudMutations) = CrudGenerator.Generate(
                typeName, intermediateFields, typeAttr.SqlSource, typeAttr.Cascade);

            lock (_lock)
            {
                foreach (var q in crudQueries) _queries.Add(q);
                foreach (var m in crudMutations) _mutations.Add(m);
            }
        }
    }

    /// <summary>
    /// Registers a pre-built query (typically produced by <see cref="FraiseQL.Builders.QueryBuilder"/>).
    /// </summary>
    /// <param name="query">The query to register.</param>
    public void RegisterQuery(IntermediateQuery query)
    {
        lock (_lock)
        {
            _queries.Add(query);
        }
    }

    /// <summary>
    /// Registers a pre-built mutation (typically produced by <see cref="FraiseQL.Builders.MutationBuilder"/>).
    /// </summary>
    /// <param name="mutation">The mutation to register.</param>
    public void RegisterMutation(IntermediateMutation mutation)
    {
        lock (_lock)
        {
            _mutations.Add(mutation);
        }
    }

    /// <summary>
    /// Stores inject-default parameter mappings loaded from <c>fraiseql.toml</c>.
    /// Base defaults apply to all queries and mutations; section-specific defaults
    /// override the base for their respective operation kind.
    /// </summary>
    /// <param name="baseDefaults">Defaults from <c>[inject_defaults]</c>, or <see langword="null"/>.</param>
    /// <param name="queryDefaults">Defaults from <c>[inject_defaults.queries]</c>, or <see langword="null"/>.</param>
    /// <param name="mutationDefaults">Defaults from <c>[inject_defaults.mutations]</c>, or <see langword="null"/>.</param>
    public void SetInjectDefaults(
        Dictionary<string, string>? baseDefaults,
        Dictionary<string, string>? queryDefaults,
        Dictionary<string, string>? mutationDefaults)
    {
        lock (_lock)
        {
            _injectDefaultsBase = baseDefaults;
            _injectDefaultsQueries = queryDefaults;
            _injectDefaultsMutations = mutationDefaults;
        }
    }

    /// <summary>
    /// Returns the stored inject defaults as a tuple of (base, queries, mutations).
    /// Any element may be <see langword="null"/> if that section was not configured.
    /// </summary>
    public (IReadOnlyDictionary<string, string>? Base,
            IReadOnlyDictionary<string, string>? Queries,
            IReadOnlyDictionary<string, string>? Mutations) GetInjectDefaults()
    {
        lock (_lock)
        {
            return (_injectDefaultsBase, _injectDefaultsQueries, _injectDefaultsMutations);
        }
    }

    /// <summary>
    /// Returns a snapshot of all registered type definitions.
    /// </summary>
    /// <returns>An immutable list of registered types.</returns>
    public IReadOnlyList<TypeDefinition> GetAllTypes()
    {
        lock (_lock)
        {
            return _types.ToList().AsReadOnly();
        }
    }

    /// <summary>
    /// Returns a snapshot of all registered queries.
    /// </summary>
    /// <returns>An immutable list of registered queries.</returns>
    public IReadOnlyList<IntermediateQuery> GetAllQueries()
    {
        lock (_lock)
        {
            return _queries.ToList().AsReadOnly();
        }
    }

    /// <summary>
    /// Returns a snapshot of all registered mutations.
    /// </summary>
    /// <returns>An immutable list of registered mutations.</returns>
    public IReadOnlyList<IntermediateMutation> GetAllMutations()
    {
        lock (_lock)
        {
            return _mutations.ToList().AsReadOnly();
        }
    }

    /// <summary>
    /// Retrieves a registered type by its GraphQL name, or <see langword="null"/> if not found.
    /// </summary>
    /// <param name="name">The GraphQL type name to look up.</param>
    /// <returns>The <see cref="TypeDefinition"/> or <see langword="null"/>.</returns>
    public TypeDefinition? GetTypeDefinition(string name)
    {
        lock (_lock)
        {
            return _types.Find(t => t.Name == name);
        }
    }

    /// <summary>
    /// Clears all registered types, queries, and mutations.
    /// Call this in test teardown to prevent cross-test state pollution.
    /// </summary>
    public void Clear()
    {
        lock (_lock)
        {
            _types.Clear();
            _queries.Clear();
            _mutations.Clear();
            _injectDefaultsBase = null;
            _injectDefaultsQueries = null;
            _injectDefaultsMutations = null;
        }
    }

    private static IReadOnlyList<FieldDefinition> BuildFields(Type type)
    {
        var fields = new List<FieldDefinition>();

        foreach (var prop in type.GetProperties(BindingFlags.Public | BindingFlags.Instance))
        {
            var fieldAttr = prop.GetCustomAttribute<GraphQLFieldAttribute>();
            if (fieldAttr == null)
                continue;

            var fieldName = MapPropertyName(prop.Name);
            var (graphqlType, nullable) = TypeMapper.Detect(prop, fieldAttr);

            fields.Add(new FieldDefinition(
                Name: fieldName,
                Type: graphqlType,
                Nullable: nullable,
                Description: fieldAttr.Description,
                Resolver: fieldAttr.Resolver,
                Scope: fieldAttr.Scope,
                Scopes: fieldAttr.Scopes));
        }

        return fields.AsReadOnly();
    }

    /// <summary>Converts a PascalCase property name to a camelCase GraphQL field name.</summary>
    private static string MapPropertyName(string propertyName)
    {
        if (string.IsNullOrEmpty(propertyName))
            return propertyName;

        return char.ToLowerInvariant(propertyName[0]) + propertyName[1..];
    }
}
