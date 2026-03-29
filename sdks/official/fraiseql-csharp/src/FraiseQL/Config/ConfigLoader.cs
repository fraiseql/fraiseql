using FraiseQL.Registry;

namespace FraiseQL.Config;

/// <summary>
/// Utility class that loads a <c>fraiseql.toml</c> configuration file and applies
/// the <c>[inject_defaults]</c> section to the <see cref="SchemaRegistry"/>.
/// Uses simple line-by-line parsing (no TOML library dependency).
/// </summary>
public static class ConfigLoader
{
    /// <summary>
    /// Loads inject defaults from a <c>fraiseql.toml</c> file and applies them
    /// to <see cref="SchemaRegistry.Instance"/>.
    /// </summary>
    /// <param name="path">Path to the TOML configuration file.</param>
    /// <exception cref="FileNotFoundException">Thrown when the file does not exist.</exception>
    public static void LoadFromFile(string path)
    {
        if (!File.Exists(path))
            throw new FileNotFoundException($"Configuration file not found: {path}", path);

        var lines = File.ReadAllLines(path);
        ParseAndApply(lines);
    }

    /// <summary>
    /// Parses TOML lines and applies inject_defaults to the registry.
    /// Recognizes three sections:
    /// <c>[inject_defaults]</c> (base), <c>[inject_defaults.queries]</c>, <c>[inject_defaults.mutations]</c>.
    /// </summary>
    internal static void ParseAndApply(string[] lines)
    {
        Dictionary<string, string>? baseDefaults = null;
        Dictionary<string, string>? queryDefaults = null;
        Dictionary<string, string>? mutationDefaults = null;

        string? currentSection = null;

        foreach (var rawLine in lines)
        {
            var line = rawLine.Trim();

            // Skip empty lines and comments
            if (string.IsNullOrEmpty(line) || line.StartsWith('#'))
                continue;

            // Section header
            if (line.StartsWith('[') && line.EndsWith(']'))
            {
                currentSection = line[1..^1].Trim();
                continue;
            }

            // Only process inject_defaults sections
            if (currentSection == null)
                continue;

            if (!currentSection.StartsWith("inject_defaults"))
                continue;

            // Parse key = "value" or key = 'value'
            var eqIndex = line.IndexOf('=');
            if (eqIndex < 0)
                continue;

            var key = line[..eqIndex].Trim();
            var value = line[(eqIndex + 1)..].Trim();

            // Strip quotes
            if ((value.StartsWith('"') && value.EndsWith('"')) ||
                (value.StartsWith('\'') && value.EndsWith('\'')))
            {
                value = value[1..^1];
            }

            switch (currentSection)
            {
                case "inject_defaults":
                    baseDefaults ??= new Dictionary<string, string>();
                    baseDefaults[key] = value;
                    break;
                case "inject_defaults.queries":
                    queryDefaults ??= new Dictionary<string, string>();
                    queryDefaults[key] = value;
                    break;
                case "inject_defaults.mutations":
                    mutationDefaults ??= new Dictionary<string, string>();
                    mutationDefaults[key] = value;
                    break;
            }
        }

        if (baseDefaults != null || queryDefaults != null || mutationDefaults != null)
        {
            SchemaRegistry.Instance.SetInjectDefaults(baseDefaults, queryDefaults, mutationDefaults);
        }
    }
}
