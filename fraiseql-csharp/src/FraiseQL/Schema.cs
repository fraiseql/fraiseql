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
            registry.Register(name, fields, description);
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
                    var field = new
                    {
                        name = fieldName,
                        type = config?.GetValueOrDefault("type", "String") ?? "String",
                        nullable = config?.GetValueOrDefault("nullable", false) ?? false
                    };
                    fieldsArray.Add(field);
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
