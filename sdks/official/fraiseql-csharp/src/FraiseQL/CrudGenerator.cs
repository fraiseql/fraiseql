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
/// <item>Create: mutation <c>create_&lt;snake&gt;</c> with a <c>Create{Type}Input</c> input object</item>
/// <item>Update: mutation <c>update_&lt;snake&gt;</c> with an <c>Update{Type}Input</c> input object</item>
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
    /// <returns>A tuple of generated queries, mutations, and input types.</returns>
    /// <exception cref="InvalidOperationException">Thrown when <paramref name="fields"/> is empty.</exception>
    public static (IReadOnlyList<IntermediateQuery> Queries, IReadOnlyList<IntermediateMutation> Mutations, IReadOnlyList<InputTypeDefinition> InputTypes)
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
        var inputTypes = new List<InputTypeDefinition>();

        // Create mutation — input object with all non-computed fields
        var createInputName = $"Create{typeName}Input";
        var createInputFields = fields
            .Where(f => f.Computed != true)
            .Select(f => new InputFieldDefinition(f.Name, f.Type, f.Nullable))
            .ToList()
            .AsReadOnly();
        inputTypes.Add(new InputTypeDefinition(
            createInputName, createInputFields, $"Input for creating a new {typeName}."));

        mutations.Add(new IntermediateMutation(
            Name: $"create_{snake}",
            ReturnType: typeName,
            SqlSource: $"fn_create_{snake}",
            Operation: "INSERT",
            Arguments: new[] { new IntermediateArgument("input", createInputName, false) }.ToList().AsReadOnly(),
            Description: $"Create a new {typeName}.",
            Cascade: cascadeValue));

        // Update mutation — input object with PK required, rest nullable
        var updateInputName = $"Update{typeName}Input";
        var updateInputFields = new List<InputFieldDefinition>
        {
            new(pkField.Name, pkField.Type, false)
        };
        updateInputFields.AddRange(fields.Skip(1).Where(f => f.Computed != true).Select(f => new InputFieldDefinition(f.Name, f.Type, true)));
        inputTypes.Add(new InputTypeDefinition(
            updateInputName, updateInputFields.AsReadOnly(), $"Input for updating an existing {typeName}."));

        mutations.Add(new IntermediateMutation(
            Name: $"update_{snake}",
            ReturnType: typeName,
            SqlSource: $"fn_update_{snake}",
            Operation: "UPDATE",
            Arguments: new[] { new IntermediateArgument("input", updateInputName, false) }.ToList().AsReadOnly(),
            Description: $"Update an existing {typeName}.",
            Cascade: cascadeValue));

        // Delete mutation — PK only (no input object)
        mutations.Add(new IntermediateMutation(
            Name: $"delete_{snake}",
            ReturnType: typeName,
            SqlSource: $"fn_delete_{snake}",
            Operation: "DELETE",
            Arguments: new[] { new IntermediateArgument(pkField.Name, pkField.Type, false) }.ToList().AsReadOnly(),
            Description: $"Delete a {typeName}.",
            Cascade: cascadeValue));

        return (queries.AsReadOnly(), mutations.AsReadOnly(), inputTypes.AsReadOnly());
    }

    /// <summary>
    /// Represents a field within a generated input type.
    /// </summary>
    /// <param name="Name">The field name.</param>
    /// <param name="Type">The GraphQL type name.</param>
    /// <param name="Nullable">Whether this field accepts null.</param>
    public record InputFieldDefinition(string Name, string Type, bool Nullable);

    /// <summary>
    /// Represents a generated input type (e.g. <c>CreateUserInput</c>, <c>UpdateUserInput</c>).
    /// </summary>
    /// <param name="Name">The input type name.</param>
    /// <param name="Fields">Ordered list of fields on this input type.</param>
    /// <param name="Description">Human-readable description.</param>
    public record InputTypeDefinition(
        string Name,
        IReadOnlyList<InputFieldDefinition> Fields,
        string Description);
}
