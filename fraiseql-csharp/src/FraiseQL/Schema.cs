using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace FraiseQL
{
    /// <summary>
    /// Facade for schema management and minimal types export (TOML-based workflow)
    /// </summary>
    public static class Schema
    {
        private static SchemaRegistry registry = SchemaRegistry.Instance;

        /// <summary>
        /// Register a type definition
        /// </summary>
        public static void RegisterType(string name, Dictionary<string, object> fields, string description = null)
        {
            // Validate and extract scopes from fields
            var validatedFields = new Dictionary<string, object>();

            foreach (var (fieldName, fieldConfig) in fields)
            {
                var config = fieldConfig as Dictionary<string, object>;
                if (config != null)
                {
                    // Validate scope if present
                    if (config.TryGetValue("requiresScope", out var scopeObj) && scopeObj is string scope)
                    {
                        ValidateScope(scope, name, fieldName);
                    }

                    // Validate scopes array if present
                    if (config.TryGetValue("requiresScopes", out var scopesObj) && scopesObj is List<object> scopes)
                    {
                        if (scopes.Count == 0)
                        {
                            throw new InvalidOperationException(
                                $"Field {name}.{fieldName} has empty scopes array");
                        }
                        foreach (var s in scopes)
                        {
                            if (s is string scopeStr)
                            {
                                if (string.IsNullOrEmpty(scopeStr))
                                {
                                    throw new InvalidOperationException(
                                        $"Field {name}.{fieldName} has empty scope in scopes array");
                                }
                                ValidateScope(scopeStr, name, fieldName);
                            }
                        }
                    }

                    // Ensure not both scope and scopes
                    if (config.ContainsKey("requiresScope") && config.ContainsKey("requiresScopes"))
                    {
                        throw new InvalidOperationException(
                            $"Field {name}.{fieldName} cannot have both requiresScope and requiresScopes");
                    }
                }

                validatedFields[fieldName] = fieldConfig;
            }

            registry.Register(name, validatedFields, description);
        }

        /// <summary>
        /// Export minimal schema with only types (TOML workflow)
        /// </summary>
        public static string ExportTypes(bool pretty = true)
        {
            var types = new List<object>();

            foreach (var typeName in registry.GetTypeNames())
            {
                var typeInfo = registry.GetType(typeName);
                if (typeInfo == null) continue;

                var fieldsArray = new List<object>();
                foreach (var (fieldName, fieldConfig) in typeInfo.Fields)
                {
                    var config = fieldConfig as Dictionary<string, object>;
                    var fieldDict = new Dictionary<string, object>
                    {
                        { "name", fieldName },
                        { "type", config?.GetValueOrDefault("type", "String") ?? "String" },
                        { "nullable", config?.GetValueOrDefault("nullable", false) ?? false }
                    };

                    // Include scope fields if present
                    if (config?.ContainsKey("requiresScope") ?? false)
                    {
                        fieldDict["requiresScope"] = config["requiresScope"];
                    }
                    if (config?.ContainsKey("requiresScopes") ?? false)
                    {
                        fieldDict["requiresScopes"] = config["requiresScopes"];
                    }

                    fieldsArray.Add(fieldDict);
                }

                var typeObj = new Dictionary<string, object>
                {
                    { "name", typeName },
                    { "fields", fieldsArray }
                };

                if (!string.IsNullOrEmpty(typeInfo.Description))
                {
                    typeObj["description"] = typeInfo.Description;
                }

                types.Add(typeObj);
            }

            var schema = new Dictionary<string, object> { { "types", types } };

            var options = new JsonSerializerOptions
            {
                WriteIndented = pretty,
                PropertyNamingPolicy = null
            };

            return JsonSerializer.Serialize(schema, options);
        }

        /// <summary>
        /// Export minimal types to a file
        /// </summary>
        public static void ExportTypesFile(string outputPath)
        {
            try
            {
                var typesJson = ExportTypes(pretty: true);
                var directory = Path.GetDirectoryName(outputPath);

                if (!string.IsNullOrEmpty(directory) && !Directory.Exists(directory))
                {
                    Directory.CreateDirectory(directory);
                }

                File.WriteAllText(outputPath, typesJson);

                var typesCount = registry.GetTypeNames().Count();
                Console.WriteLine($"✅ Types exported to {outputPath}");
                Console.WriteLine($"   Types: {typesCount}");
                Console.WriteLine();
                Console.WriteLine("🎯 Next steps:");
                Console.WriteLine($"   1. fraiseql compile fraiseql.toml --types {outputPath}");
                Console.WriteLine("   2. This merges types with TOML configuration");
                Console.WriteLine("   3. Result: schema.compiled.json with types + all config");
            }
            catch (Exception)
            {
                throw new InvalidOperationException($"Failed to write types file: {outputPath}");
            }
        }

        /// <summary>
        /// Reset schema registry (useful for testing)
        /// </summary>
        public static void Reset()
        {
            registry.Clear();
        }

        /// <summary>
        /// Get all registered type names
        /// </summary>
        public static IEnumerable<string> GetTypeNames()
        {
            return registry.GetTypeNames();
        }

        /// <summary>
        /// Validate scope format: action:resource
        /// Valid patterns:
        /// - * (global wildcard)
        /// - action:resource (read:user.email, write:User.salary)
        /// - action:* (admin:*, read:*)
        /// </summary>
        private static void ValidateScope(string scope, string typeName, string fieldName)
        {
            if (string.IsNullOrEmpty(scope))
            {
                throw new InvalidOperationException($"Field {typeName}.{fieldName} has empty scope");
            }

            // Global wildcard is always valid
            if (scope == "*")
            {
                return;
            }

            // Must contain at least one colon
            if (!scope.Contains(":"))
            {
                throw new InvalidOperationException(
                    $"Field {typeName}.{fieldName} has invalid scope '{scope}' (missing colon)");
            }

            var parts = scope.Split(new[] { ':' }, 2);
            if (parts.Length != 2)
            {
                throw new InvalidOperationException(
                    $"Field {typeName}.{fieldName} has invalid scope '{scope}'");
            }

            var action = parts[0];
            var resource = parts[1];

            // Validate action: [a-zA-Z_][a-zA-Z0-9_]*
            if (!IsValidAction(action))
            {
                throw new InvalidOperationException(
                    $"Field {typeName}.{fieldName} has invalid action in scope '{scope}' (must be alphanumeric + underscore)");
            }

            // Validate resource: [a-zA-Z_][a-zA-Z0-9_.]*|*
            if (!IsValidResource(resource))
            {
                throw new InvalidOperationException(
                    $"Field {typeName}.{fieldName} has invalid resource in scope '{scope}' (must be alphanumeric + underscore + dot, or *)");
            }
        }

        /// <summary>
        /// Check if action matches [a-zA-Z_][a-zA-Z0-9_]*
        /// </summary>
        private static bool IsValidAction(string action)
        {
            if (string.IsNullOrEmpty(action))
            {
                return false;
            }

            // First character must be letter or underscore
            var firstChar = action[0];
            if (!char.IsLetter(firstChar) && firstChar != '_')
            {
                return false;
            }

            // Rest must be letters, digits, or underscores
            for (int i = 1; i < action.Length; i++)
            {
                var ch = action[i];
                if (!char.IsLetterOrDigit(ch) && ch != '_')
                {
                    return false;
                }
            }

            return true;
        }

        /// <summary>
        /// Check if resource matches [a-zA-Z_][a-zA-Z0-9_.]*|*
        /// </summary>
        private static bool IsValidResource(string resource)
        {
            if (resource == "*")
            {
                return true;
            }

            if (string.IsNullOrEmpty(resource))
            {
                return false;
            }

            // First character must be letter or underscore
            var firstChar = resource[0];
            if (!char.IsLetter(firstChar) && firstChar != '_')
            {
                return false;
            }

            // Rest must be letters, digits, underscores, or dots
            for (int i = 1; i < resource.Length; i++)
            {
                var ch = resource[i];
                if (!char.IsLetterOrDigit(ch) && ch != '_' && ch != '.')
                {
                    return false;
                }
            }

            return true;
        }
    }

    /// <summary>
    /// Central registry for GraphQL type definitions
    /// </summary>
    public class SchemaRegistry
    {
        private static SchemaRegistry instance;
        private static object lockObj = new object();
        private Dictionary<string, (Dictionary<string, object> Fields, string Description)> types = new();

        public static SchemaRegistry Instance
        {
            get
            {
                if (instance == null)
                {
                    lock (lockObj)
                    {
                        if (instance == null)
                        {
                            instance = new SchemaRegistry();
                        }
                    }
                }
                return instance;
            }
        }

        public void Register(string name, Dictionary<string, object> fields, string description = null)
        {
            lock (lockObj)
            {
                types[name] = (fields, description);
            }
        }

        public (Dictionary<string, object> Fields, string Description)? GetType(string name)
        {
            lock (lockObj)
            {
                return types.ContainsKey(name) ? types[name] : null;
            }
        }

        public IEnumerable<string> GetTypeNames()
        {
            lock (lockObj)
            {
                return types.Keys.ToList();
            }
        }

        public void Clear()
        {
            lock (lockObj)
            {
                types.Clear();
            }
        }
    }
}
