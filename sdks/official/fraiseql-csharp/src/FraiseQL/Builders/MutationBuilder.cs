using FraiseQL.Models;
using FraiseQL.Registry;

namespace FraiseQL.Builders;

/// <summary>
/// Fluent builder for constructing a <see cref="IntermediateMutation"/> and optionally
/// registering it in the <see cref="SchemaRegistry"/>.
/// </summary>
/// <example>
/// <code>
/// MutationBuilder.Mutation("createAuthor")
///     .ReturnType("Author")
///     .SqlSource("fn_create_author")
///     .Operation("insert")
///     .Argument("name", "String")
///     .Register();
/// </code>
/// </example>
public sealed class MutationBuilder
{
    private static readonly HashSet<string> ValidOperations =
        new(StringComparer.OrdinalIgnoreCase) { "insert", "update", "delete", "upsert" };

    private readonly string _name;
    private string _returnType = string.Empty;
    private string _sqlSource = string.Empty;
    private string _operation = string.Empty;
    private string? _description;
    private string? _restPath;
    private string? _restMethod;
    private readonly List<IntermediateArgument> _arguments = new();

    private MutationBuilder(string name) => _name = name;

    /// <summary>Creates a new <see cref="MutationBuilder"/> for a mutation with the given name.</summary>
    /// <param name="name">The GraphQL mutation name.</param>
    /// <returns>A new builder instance.</returns>
    public static MutationBuilder Mutation(string name) => new(name);

    /// <summary>Sets the GraphQL return type name.</summary>
    /// <param name="type">The return type name (e.g. <c>"Author"</c>).</param>
    /// <returns>This builder for chaining.</returns>
    public MutationBuilder ReturnType(string type) { _returnType = type; return this; }

    /// <summary>Sets the backing SQL function name.</summary>
    /// <param name="source">The SQL function name (e.g. <c>"fn_create_author"</c>).</param>
    /// <returns>This builder for chaining.</returns>
    public MutationBuilder SqlSource(string source) { _sqlSource = source; return this; }

    /// <summary>
    /// Sets the mutation operation kind.
    /// </summary>
    /// <param name="operation">One of <c>"insert"</c>, <c>"update"</c>, <c>"delete"</c>, or <c>"upsert"</c>.</param>
    /// <returns>This builder for chaining.</returns>
    /// <exception cref="ArgumentException">Thrown when <paramref name="operation"/> is not a valid operation.</exception>
    public MutationBuilder Operation(string operation)
    {
        if (!ValidOperations.Contains(operation))
            throw new ArgumentException(
                $"Invalid operation '{operation}'. Must be one of: insert, update, delete, upsert.",
                nameof(operation));
        _operation = operation;
        return this;
    }

    /// <summary>Sets a human-readable description for this mutation.</summary>
    /// <param name="desc">The description text.</param>
    /// <returns>This builder for chaining.</returns>
    public MutationBuilder Description(string desc) { _description = desc; return this; }

    /// <summary>Sets the REST endpoint path for this mutation.</summary>
    /// <param name="path">The REST path (e.g. <c>"/api/users"</c>).</param>
    /// <returns>This builder for chaining.</returns>
    public MutationBuilder RestPath(string path) { _restPath = path; return this; }

    /// <summary>Sets the HTTP method for the REST endpoint. Defaults to POST for mutations.</summary>
    /// <param name="method">The HTTP method (GET, POST, PUT, PATCH, DELETE).</param>
    /// <returns>This builder for chaining.</returns>
    public MutationBuilder RestMethod(string method) { _restMethod = method; return this; }

    /// <summary>Adds a typed argument to this mutation.</summary>
    /// <param name="name">The argument name.</param>
    /// <param name="type">The GraphQL type name.</param>
    /// <param name="nullable">Whether the argument accepts <c>null</c>.</param>
    /// <returns>This builder for chaining.</returns>
    public MutationBuilder Argument(string name, string type, bool nullable = false)
    {
        _arguments.Add(new IntermediateArgument(name, type, nullable));
        return this;
    }

    /// <summary>
    /// Builds the <see cref="IntermediateMutation"/> from the current configuration.
    /// </summary>
    /// <returns>The constructed mutation.</returns>
    /// <exception cref="InvalidOperationException">
    /// Thrown when <c>ReturnType</c>, <c>SqlSource</c>, or <c>Operation</c> has not been set.
    /// </exception>
    public IntermediateMutation Build()
    {
        if (string.IsNullOrEmpty(_returnType))
            throw new InvalidOperationException(
                $"MutationBuilder: ReturnType must be set before Build() (mutation: '{_name}')");
        if (string.IsNullOrEmpty(_sqlSource))
            throw new InvalidOperationException(
                $"MutationBuilder: SqlSource must be set before Build() (mutation: '{_name}')");
        if (string.IsNullOrEmpty(_operation))
            throw new InvalidOperationException(
                $"MutationBuilder: Operation must be set before Build() (mutation: '{_name}')");

        RestAnnotation? rest = _restPath != null
            ? new RestAnnotation(_restPath, (_restMethod ?? "POST").ToUpperInvariant())
            : null;

        return new IntermediateMutation(
            Name: _name,
            ReturnType: _returnType,
            SqlSource: _sqlSource,
            Operation: _operation,
            Arguments: _arguments.AsReadOnly(),
            Description: _description,
            Rest: rest);
    }

    /// <summary>
    /// Builds the mutation and registers it in <see cref="SchemaRegistry.Instance"/>.
    /// Equivalent to calling <c>SchemaRegistry.Instance.RegisterMutation(Build())</c>.
    /// </summary>
    public void Register() => SchemaRegistry.Instance.RegisterMutation(Build());

    /// <summary>Returns the mutation name set on this builder.</summary>
    /// <returns>The mutation name.</returns>
    internal string GetName() => _name;
}
