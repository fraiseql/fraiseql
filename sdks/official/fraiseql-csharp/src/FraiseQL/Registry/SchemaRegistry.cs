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

    // inject_defaults: base applies to both queries and mutations;
    // query/mutation-specific maps override base.
    private Dictionary<string, string>? _injectDefaults;
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
            TenantScoped: typeAttr.TenantScoped,
            Fields: fields,
            Crud: typeAttr.Crud);

        lock (_lock)
        {
            _types.Add(definition);

            if (typeAttr.Crud is { Length: > 0 })
            {
                GenerateCrudOperations(typeName, typeAttr.SqlSource, fields, typeAttr.Crud);
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
            _injectDefaults = null;
            _injectDefaultsQueries = null;
            _injectDefaultsMutations = null;
        }
    }

    /// <summary>
    /// Configures default inject_params that are merged into every query and/or mutation
    /// at schema export time. The base map applies to both queries and mutations.
    /// The queries and mutations maps override base for their respective operation types.
    /// Pass <see langword="null"/> for any map you don't need.
    /// </summary>
    /// <param name="baseDefaults">Defaults applied to both queries and mutations.</param>
    /// <param name="queryDefaults">Overrides for queries only.</param>
    /// <param name="mutationDefaults">Overrides for mutations only.</param>
    public void SetInjectDefaults(
        Dictionary<string, string>? baseDefaults,
        Dictionary<string, string>? queryDefaults = null,
        Dictionary<string, string>? mutationDefaults = null)
    {
        lock (_lock)
        {
            _injectDefaults = baseDefaults;
            _injectDefaultsQueries = queryDefaults;
            _injectDefaultsMutations = mutationDefaults;
        }
    }

    /// <summary>
    /// Returns the merged inject defaults for queries (base + query-specific).
    /// </summary>
    internal Dictionary<string, string>? GetMergedQueryDefaults()
    {
        return MergeStringMaps(_injectDefaults, _injectDefaultsQueries);
    }

    /// <summary>
    /// Returns the merged inject defaults for mutations (base + mutation-specific).
    /// </summary>
    internal Dictionary<string, string>? GetMergedMutationDefaults()
    {
        return MergeStringMaps(_injectDefaults, _injectDefaultsMutations);
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

    /// <summary>Converts a PascalCase name to snake_case.</summary>
    internal static string PascalToSnake(string name)
    {
        if (string.IsNullOrEmpty(name))
            return name;

        var result = new System.Text.StringBuilder();
        for (int i = 0; i < name.Length; i++)
        {
            var c = name[i];
            if (char.IsUpper(c) && i > 0)
                result.Append('_');
            result.Append(char.ToLowerInvariant(c));
        }
        return result.ToString();
    }

    /// <summary>
    /// Merges two string dictionaries; overlay values override base values.
    /// Returns <see langword="null"/> if both are null/empty.
    /// </summary>
    private static Dictionary<string, string>? MergeStringMaps(
        Dictionary<string, string>? baseMap, Dictionary<string, string>? overlay)
    {
        if ((baseMap == null || baseMap.Count == 0) && (overlay == null || overlay.Count == 0))
            return null;

        var merged = new Dictionary<string, string>();
        if (baseMap != null)
        {
            foreach (var kvp in baseMap)
                merged[kvp.Key] = kvp.Value;
        }
        if (overlay != null)
        {
            foreach (var kvp in overlay)
                merged[kvp.Key] = kvp.Value;
        }
        return merged;
    }

    /// <summary>
    /// Converts "jwt:claim_name" into { "source": "jwt", "claim": "claim_name" }.
    /// If no colon, returns { "source": value }.
    /// </summary>
    internal static Dictionary<string, string> ParseInjectParamValue(string value)
    {
        var parts = value.Split(':', 2);
        if (parts.Length == 2)
            return new Dictionary<string, string> { ["source"] = parts[0], ["claim"] = parts[1] };
        return new Dictionary<string, string> { ["source"] = value };
    }

    /// <summary>
    /// Applies inject defaults to a query, returning a new query with merged inject_params.
    /// Existing params on the query take precedence over defaults.
    /// </summary>
    internal static IntermediateQuery ApplyInjectDefaults(
        IntermediateQuery query, Dictionary<string, string>? defaults)
    {
        if (defaults == null || defaults.Count == 0)
            return query;

        var merged = query.InjectParams != null
            ? new Dictionary<string, Dictionary<string, string>>(query.InjectParams)
            : new Dictionary<string, Dictionary<string, string>>();

        foreach (var kvp in defaults)
        {
            if (!merged.ContainsKey(kvp.Key))
                merged[kvp.Key] = ParseInjectParamValue(kvp.Value);
        }

        return query with { InjectParams = merged };
    }

    /// <summary>
    /// Applies inject defaults to a mutation, returning a new mutation with merged inject_params.
    /// Existing params on the mutation take precedence over defaults.
    /// </summary>
    internal static IntermediateMutation ApplyInjectDefaults(
        IntermediateMutation mutation, Dictionary<string, string>? defaults)
    {
        if (defaults == null || defaults.Count == 0)
            return mutation;

        var merged = mutation.InjectParams != null
            ? new Dictionary<string, Dictionary<string, string>>(mutation.InjectParams)
            : new Dictionary<string, Dictionary<string, string>>();

        foreach (var kvp in defaults)
        {
            if (!merged.ContainsKey(kvp.Key))
                merged[kvp.Key] = ParseInjectParamValue(kvp.Value);
        }

        return mutation with { InjectParams = merged };
    }

    /// <summary>
    /// Generates CRUD queries and mutations for a type based on its fields.
    /// Must be called inside the lock.
    /// </summary>
    private void GenerateCrudOperations(
        string typeName, string sqlSource, IReadOnlyList<FieldDefinition> fields, string[] crud)
    {
        var ops = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var op in crud)
        {
            if (string.Equals(op, "all", StringComparison.OrdinalIgnoreCase))
            {
                ops.Add("read");
                ops.Add("create");
                ops.Add("update");
                ops.Add("delete");
            }
            else
            {
                ops.Add(op);
            }
        }

        if (fields.Count == 0)
            return;

        var snake = PascalToSnake(typeName);
        var view = string.IsNullOrEmpty(sqlSource) ? "v_" + snake : sqlSource;
        var pkField = fields[0];

        if (ops.Contains("read"))
        {
            // get-by-ID: nullable single result
            _queries.Add(new IntermediateQuery(
                Name: snake,
                ReturnType: typeName,
                ReturnsList: false,
                Nullable: true,
                SqlSource: view,
                Arguments: new List<IntermediateArgument>
                {
                    new(pkField.Name, pkField.Type, false)
                }.AsReadOnly()));

            // list: returns_list with auto_params
            _queries.Add(new IntermediateQuery(
                Name: snake + "s",
                ReturnType: typeName,
                ReturnsList: true,
                Nullable: false,
                SqlSource: view,
                Arguments: Array.Empty<IntermediateArgument>(),
                AutoParams: new Dictionary<string, bool>
                {
                    ["where"] = true,
                    ["order_by"] = true,
                    ["limit"] = true,
                    ["offset"] = true
                }));
        }

        if (ops.Contains("create"))
        {
            var args = fields
                .Select(f => new IntermediateArgument(f.Name, f.Type, f.Nullable))
                .ToList()
                .AsReadOnly();

            _mutations.Add(new IntermediateMutation(
                Name: "create_" + snake,
                ReturnType: typeName,
                SqlSource: "fn_create_" + snake,
                Operation: "insert",
                Arguments: args));
        }

        if (ops.Contains("update"))
        {
            // PK required, all others nullable
            var args = new List<IntermediateArgument> { new(pkField.Name, pkField.Type, false) };
            foreach (var f in fields.Skip(1))
            {
                args.Add(new IntermediateArgument(f.Name, f.Type, true));
            }

            _mutations.Add(new IntermediateMutation(
                Name: "update_" + snake,
                ReturnType: typeName,
                SqlSource: "fn_update_" + snake,
                Operation: "update",
                Arguments: args.AsReadOnly()));
        }

        if (ops.Contains("delete"))
        {
            _mutations.Add(new IntermediateMutation(
                Name: "delete_" + snake,
                ReturnType: typeName,
                SqlSource: "fn_delete_" + snake,
                Operation: "delete",
                Arguments: new List<IntermediateArgument>
                {
                    new(pkField.Name, pkField.Type, false)
                }.AsReadOnly()));
        }
    }
}
