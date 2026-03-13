using System.Reflection;
using System.Runtime.Loader;
using FraiseQL.Attributes;
using FraiseQL.Export;
using FraiseQL.Registry;

namespace FraiseQL.Tool;

/// <summary>
/// Loads a user-compiled assembly, scans it for <see cref="GraphQLTypeAttribute"/>-annotated
/// types, registers them in <see cref="SchemaRegistry"/>, and exports <c>schema.json</c>.
/// </summary>
internal static class AssemblyLoader
{
    /// <summary>
    /// Loads the assembly at <paramref name="assemblyPath"/>, registers all
    /// <see cref="GraphQLTypeAttribute"/>-decorated types, and writes the schema to
    /// <paramref name="outputPath"/>.
    /// </summary>
    /// <param name="assemblyPath">Absolute or relative path to the compiled <c>.dll</c>.</param>
    /// <param name="outputPath">Destination file for the exported <c>schema.json</c>.</param>
    /// <param name="pretty">When <see langword="true"/>, the JSON is indented.</param>
    /// <returns><c>0</c> on success, <c>1</c> on error.</returns>
    internal static int LoadAndExport(string assemblyPath, string outputPath, bool pretty)
    {
        Assembly assembly;
        try
        {
            var context = new AssemblyLoadContext("FraiseQLToolContext", isCollectible: true);
            assembly = context.LoadFromAssemblyPath(Path.GetFullPath(assemblyPath));
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Error: Could not load assembly '{assemblyPath}': {ex.Message}");
            return 1;
        }

        Type[] exportedTypes;
        try
        {
            exportedTypes = assembly.GetExportedTypes();
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Error: Could not read types from assembly: {ex.Message}");
            return 1;
        }

        var graphqlTypes = exportedTypes
            .Where(t => t.GetCustomAttribute<GraphQLTypeAttribute>() != null)
            .ToList();

        if (graphqlTypes.Count == 0)
        {
            Console.Error.WriteLine(
                $"Warning: No [GraphQLType]-annotated types found in '{assemblyPath}'. Exporting empty schema.");
        }

        var registry = SchemaRegistry.Instance;
        registry.Clear();

        foreach (var type in graphqlTypes)
        {
            try
            {
                registry.Register(type);
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"Warning: Could not register type '{type.Name}': {ex.Message}");
            }
        }

        try
        {
            SchemaExporter.ExportToFile(outputPath, pretty);
            Console.WriteLine($"Schema exported to '{outputPath}' ({registry.GetAllTypes().Count} types).");
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Error: Could not write schema to '{outputPath}': {ex.Message}");
            return 1;
        }
    }
}
