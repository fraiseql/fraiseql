using FraiseQL.Models;
using FraiseQL.Registry;

namespace FraiseQL.Builders;

/// <summary>
/// Fluent builder for constructing a <see cref="IntermediateQuery"/> and optionally
/// registering it in the <see cref="SchemaRegistry"/>.
/// </summary>
/// <example>
/// <code>
/// QueryBuilder.Query("authors")
///     .ReturnType("Author")
///     .ReturnsList()
///     .SqlSource("v_author")
///     .Register();
/// </code>
/// </example>
public sealed class QueryBuilder
{
    private readonly string _name;
    private string _returnType = string.Empty;
    private bool _returnsList;
    private bool _nullable;
    private string _sqlSource = string.Empty;
    private int? _cacheTtlSeconds;
    private string? _description;
    private readonly List<IntermediateArgument> _arguments = new();

    private QueryBuilder(string name) => _name = name;

    /// <summary>Creates a new <see cref="QueryBuilder"/> for a query with the given name.</summary>
    /// <param name="name">The GraphQL query name.</param>
    /// <returns>A new builder instance.</returns>
    public static QueryBuilder Query(string name) => new(name);

    /// <summary>Sets the GraphQL return type name.</summary>
    /// <param name="type">The return type name (e.g. <c>"Author"</c>).</param>
    /// <returns>This builder for chaining.</returns>
    public QueryBuilder ReturnType(string type) { _returnType = type; return this; }

    /// <summary>Sets whether the query returns a list.</summary>
    /// <param name="value"><see langword="true"/> to return a list; <see langword="false"/> for a single item.</param>
    /// <returns>This builder for chaining.</returns>
    public QueryBuilder ReturnsList(bool value = true) { _returnsList = value; return this; }

    /// <summary>Sets whether the query result may be <c>null</c>.</summary>
    /// <param name="value"><see langword="true"/> if the result is nullable.</param>
    /// <returns>This builder for chaining.</returns>
    public QueryBuilder Nullable(bool value = true) { _nullable = value; return this; }

    /// <summary>Sets the backing SQL view name.</summary>
    /// <param name="source">The SQL view name (e.g. <c>"v_author"</c>).</param>
    /// <returns>This builder for chaining.</returns>
    public QueryBuilder SqlSource(string source) { _sqlSource = source; return this; }

    /// <summary>Sets the cache TTL in seconds for this query's results.</summary>
    /// <param name="seconds">Cache duration in seconds.</param>
    /// <returns>This builder for chaining.</returns>
    public QueryBuilder CacheTtlSeconds(int seconds) { _cacheTtlSeconds = seconds; return this; }

    /// <summary>Sets a human-readable description for this query.</summary>
    /// <param name="desc">The description text.</param>
    /// <returns>This builder for chaining.</returns>
    public QueryBuilder Description(string desc) { _description = desc; return this; }

    /// <summary>Adds a typed argument to this query.</summary>
    /// <param name="name">The argument name.</param>
    /// <param name="type">The GraphQL type name.</param>
    /// <param name="nullable">Whether the argument accepts <c>null</c>.</param>
    /// <returns>This builder for chaining.</returns>
    public QueryBuilder Argument(string name, string type, bool nullable = false)
    {
        _arguments.Add(new IntermediateArgument(name, type, nullable));
        return this;
    }

    /// <summary>
    /// Builds the <see cref="IntermediateQuery"/> from the current configuration.
    /// </summary>
    /// <returns>The constructed query.</returns>
    /// <exception cref="InvalidOperationException">
    /// Thrown when <c>ReturnType</c> or <c>SqlSource</c> has not been set.
    /// </exception>
    public IntermediateQuery Build()
    {
        if (string.IsNullOrEmpty(_returnType))
            throw new InvalidOperationException(
                $"QueryBuilder: ReturnType must be set before Build() (query: '{_name}')");
        if (string.IsNullOrEmpty(_sqlSource))
            throw new InvalidOperationException(
                $"QueryBuilder: SqlSource must be set before Build() (query: '{_name}')");

        return new IntermediateQuery(
            Name: _name,
            ReturnType: _returnType,
            ReturnsList: _returnsList,
            Nullable: _nullable,
            SqlSource: _sqlSource,
            Arguments: _arguments.AsReadOnly(),
            CacheTtlSeconds: _cacheTtlSeconds,
            Description: _description);
    }

    /// <summary>
    /// Builds the query and registers it in <see cref="SchemaRegistry.Instance"/>.
    /// Equivalent to calling <c>SchemaRegistry.Instance.RegisterQuery(Build())</c>.
    /// </summary>
    public void Register() => SchemaRegistry.Instance.RegisterQuery(Build());

    /// <summary>Returns the query name set on this builder.</summary>
    /// <returns>The query name.</returns>
    internal string GetName() => _name;
}
