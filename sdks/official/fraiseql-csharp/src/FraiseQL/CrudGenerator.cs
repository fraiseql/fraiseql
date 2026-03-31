using FraiseQL.Models;
using System.Text.RegularExpressions;

namespace FraiseQL;

/// <summary>
/// Generates standard CRUD queries and mutations for a GraphQL type.
/// </summary>
/// <remarks>
/// <para>Generated operations follow FraiseQL conventions:</para>
/// <list type="bullet">
/// <item>Read: query <c>&lt;snake&gt;</c> (get by PK) + query <c>&lt;snake&gt;s</c> (list with auto_params)</item>
/// <item>Create: mutation <c>create_&lt;snake&gt;</c> with all fields as arguments</item>
/// <item>Update: mutation <c>update_&lt;snake&gt;</c> with PK required, other fields nullable</item>
/// <item>Delete: mutation <c>delete_&lt;snake&gt;</c> with PK only</item>
/// </list>
/// </remarks>
public static class CrudGenerator
{
    private static readonly Regex CamelRe = new(@"(?<!^)([A-Z])", RegexOptions.Compiled);

    /// <summary>Converts a PascalCase name to snake_case.</summary>
    /// <param name="name">The PascalCase name.</param>
    /// <returns>The snake_case equivalent.</returns>
    public static string PascalToSnake(string name) =>
        CamelRe.Replace(name, "_$1").ToLowerInvariant();

    /// <summary>
    /// Applies basic English pluralization rules to a snake_case name.
    /// </summary>
    /// <param name="name">The singular name.</param>
    /// <returns>The pluralized name.</returns>
    public static string Pluralize(string name)
    {
        if (name.EndsWith("s") && !name.EndsWith("ss")) return name;
        foreach (var suffix in new[] { "ss", "sh", "ch", "x", "z" })
            if (name.EndsWith(suffix)) return name + "es";
        if (name.Length >= 2 && name[^1] == 'y' && !"aeiou".Contains(name[^2]))
            return name[..^1] + "ies";
        return name + "s";
    }

    /// <summary>
    /// Generates CRUD queries and mutations for the given type.
    /// </summary>
    /// <param name="typeName">The GraphQL type name (PascalCase).</param>
    /// <param name="fields">The fields on this type. The first field is assumed to be the primary key.</param>
    /// <param name="sqlSource">Optional SQL source override. Defaults to <c>v_&lt;snake&gt;</c>.</param>
    /// <param name="cascade">When <see langword="true"/>, generated mutations include cascade support.</param>
    /// <returns>A tuple of generated queries and mutations.</returns>
    /// <exception cref="InvalidOperationException">Thrown when <paramref name="fields"/> is empty.</exception>
    public static (IReadOnlyList<IntermediateQuery> Queries, IReadOnlyList<IntermediateMutation> Mutations)
        Generate(string typeName, IReadOnlyList<IntermediateField> fields, string? sqlSource = null, bool cascade = false)
    {
        if (fields.Count == 0)
            throw new InvalidOperationException(
                $"Type '{typeName}' has no fields; cannot generate CRUD operations");

        var snake = PascalToSnake(typeName);
        var view = string.IsNullOrEmpty(sqlSource) ? $"v_{snake}" : sqlSource;
        var pkField = fields[0];
        bool? cascadeValue = cascade ? true : null;

        var queries = new List<IntermediateQuery>
        {
            // Get-by-ID query
            new(
                Name: snake,
                ReturnType: typeName,
                ReturnsList: false,
                Nullable: true,
                SqlSource: view,
                Arguments: new[] { new IntermediateArgument(pkField.Name, pkField.Type, false) }.ToList().AsReadOnly(),
                Description: $"Get {typeName} by ID."),

            // List query
            new(
                Name: Pluralize(snake),
                ReturnType: typeName,
                ReturnsList: true,
                Nullable: false,
                SqlSource: view,
                Arguments: Array.Empty<IntermediateArgument>().ToList().AsReadOnly(),
                Description: $"List {typeName} records.")
        };

        var mutations = new List<IntermediateMutation>();

        // Create mutation — all fields as arguments
        var createArgs = fields
            .Select(f => new IntermediateArgument(f.Name, f.Type, f.Nullable))
            .ToList()
            .AsReadOnly();
        mutations.Add(new IntermediateMutation(
            Name: $"create_{snake}",
            ReturnType: typeName,
            SqlSource: $"fn_create_{snake}",
            Operation: "INSERT",
            Arguments: createArgs,
            Description: $"Create a new {typeName}.",
            Cascade: cascadeValue));

        // Update mutation — PK required, rest nullable
        var updateArgs = new List<IntermediateArgument>
        {
            new(pkField.Name, pkField.Type, false)
        };
        updateArgs.AddRange(fields.Skip(1).Select(f => new IntermediateArgument(f.Name, f.Type, true)));
        mutations.Add(new IntermediateMutation(
            Name: $"update_{snake}",
            ReturnType: typeName,
            SqlSource: $"fn_update_{snake}",
            Operation: "UPDATE",
            Arguments: updateArgs.AsReadOnly(),
            Description: $"Update an existing {typeName}.",
            Cascade: cascadeValue));

        // Delete mutation — PK only
        mutations.Add(new IntermediateMutation(
            Name: $"delete_{snake}",
            ReturnType: typeName,
            SqlSource: $"fn_delete_{snake}",
            Operation: "DELETE",
            Arguments: new[] { new IntermediateArgument(pkField.Name, pkField.Type, false) }.ToList().AsReadOnly(),
            Description: $"Delete a {typeName}.",
            Cascade: cascadeValue));

        return (queries.AsReadOnly(), mutations.AsReadOnly());
    }
}
