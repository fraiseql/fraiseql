using System.Text.Json;
using System.Text.Json.Serialization;
using FraiseQL.Models;
using FraiseQL.Registry;

namespace FraiseQL.Export;

/// <summary>
/// Serializes the contents of <see cref="SchemaRegistry"/> into the <c>schema.json</c>
/// intermediate format consumed by <c>fraiseql compile</c>.
/// </summary>
/// <remarks>
/// This is a static utility class. All methods read from <see cref="SchemaRegistry.Instance"/>.
/// </remarks>
public static class SchemaExporter
{
    private const string SchemaVersion = "2.0.0";

    /// <summary>
    /// Builds an <see cref="IntermediateSchema"/> from the current state of <see cref="SchemaRegistry.Instance"/>.
    /// </summary>
    /// <returns>The populated intermediate schema.</returns>
    public static IntermediateSchema ToSchema()
    {
        var registry = SchemaRegistry.Instance;
        var types = registry.GetAllTypes()
            .Select(TypeDefinitionToIntermediate)
            .ToList()
            .AsReadOnly();

        var inputTypes = registry.GetAllInputTypes();

        return new IntermediateSchema(
            Version: SchemaVersion,
            Types: types,
            Queries: registry.GetAllQueries(),
            Mutations: registry.GetAllMutations(),
            InputTypes: inputTypes.Count > 0 ? inputTypes : null);
    }

    /// <summary>
    /// Builds an <see cref="IntermediateSchema"/> from the provided schema data directly,
    /// without reading from the registry singleton.
    /// </summary>
    /// <param name="schema">The schema to serialize.</param>
    /// <param name="pretty">When <see langword="true"/>, output is indented.</param>
    /// <returns>The JSON string.</returns>
    public static string Serialize(IntermediateSchema schema, bool pretty = true)
    {
        return JsonSerializer.Serialize(schema, BuildJsonSerializerOptions(pretty));
    }

    /// <summary>
    /// Exports the current registry contents to a JSON string.
    /// </summary>
    /// <param name="pretty">When <see langword="true"/>, the JSON is indented for readability.</param>
    /// <returns>The JSON representation of the schema.</returns>
    public static string Export(bool pretty = true)
    {
        return Serialize(ToSchema(), pretty);
    }

    /// <summary>
    /// Exports the current registry contents to a file at the given path.
    /// Parent directories are created automatically if they do not exist.
    /// </summary>
    /// <param name="path">The output file path.</param>
    /// <param name="pretty">When <see langword="true"/>, the JSON is indented for readability.</param>
    public static void ExportToFile(string path, bool pretty = true)
    {
        var dir = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(dir))
            Directory.CreateDirectory(dir);

        File.WriteAllText(path, Export(pretty));
    }

    /// <summary>
    /// Builds <see cref="JsonSerializerOptions"/> configured for the FraiseQL schema format:
    /// snake_case keys via <c>[JsonPropertyName]</c> attributes, and null values omitted.
    /// </summary>
    /// <param name="pretty">Whether to use indented formatting.</param>
    /// <returns>Configured serializer options.</returns>
    private static JsonSerializerOptions BuildJsonSerializerOptions(bool pretty)
    {
        return new JsonSerializerOptions
        {
            WriteIndented = pretty,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        };
    }

    /// <summary>Converts a <see cref="TypeDefinition"/> to its serializable intermediate form.</summary>
    private static IntermediateType TypeDefinitionToIntermediate(TypeDefinition td)
    {
        var fields = td.Fields
            .Select(f => new IntermediateField(
                Name: f.Name,
                Type: f.Type,
                Nullable: f.Nullable,
                Description: f.Description,
                Resolver: f.Resolver,
                Scope: f.Scope,
                Scopes: f.Scopes,
                Computed: f.Computed ? true : null))
            .ToList()
            .AsReadOnly();

        return new IntermediateType(
            Name: td.Name,
            SqlSource: td.SqlSource,
            Description: td.Description,
            Fields: fields);
    }
}
