using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using Xunit;
using FraiseQL;

namespace FraiseQL.Tests
{
    public class ExportTypesTests : IDisposable
    {
        public ExportTypesTests()
        {
            Schema.Reset();
        }

        public void Dispose()
        {
            Schema.Reset();
        }

        [Fact]
        public void TestExportTypesMinimalSingleType()
        {
            Schema.RegisterType("User", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "ID" }, { "nullable", false } } },
                { "name", new Dictionary<string, object> { { "type", "String" }, { "nullable", false } } },
                { "email", new Dictionary<string, object> { { "type", "String" }, { "nullable", false } } }
            }, "User in the system");

            var json = Schema.ExportTypes(true);
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;

            Assert.True(root.TryGetProperty("types", out var types));
            Assert.Equal(JsonValueKind.Array, types.ValueKind);
            Assert.Equal(1, types.GetArrayLength());

            Assert.False(root.TryGetProperty("queries", out _));
            Assert.False(root.TryGetProperty("mutations", out _));
            Assert.False(root.TryGetProperty("observers", out _));

            var userDef = types[0];
            Assert.Equal("User", userDef.GetProperty("name").GetString());
            Assert.Equal("User in the system", userDef.GetProperty("description").GetString());
        }

        [Fact]
        public void TestExportTypesMultipleTypes()
        {
            Schema.RegisterType("User", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "ID" }, { "nullable", false } } },
                { "name", new Dictionary<string, object> { { "type", "String" }, { "nullable", false } } }
            });

            Schema.RegisterType("Post", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "ID" }, { "nullable", false } } },
                { "title", new Dictionary<string, object> { { "type", "String" }, { "nullable", false } } }
            });

            var json = Schema.ExportTypes(true);
            using var doc = JsonDocument.Parse(json);
            var types = doc.RootElement.GetProperty("types");

            Assert.Equal(2, types.GetArrayLength());

            var typeNames = types.EnumerateArray().Select(t => t.GetProperty("name").GetString()).ToList();
            Assert.Contains("User", typeNames);
            Assert.Contains("Post", typeNames);
        }

        [Fact]
        public void TestExportTypesNoQueries()
        {
            Schema.RegisterType("User", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "ID" }, { "nullable", false } } }
            });

            var json = Schema.ExportTypes(true);
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;

            Assert.True(root.TryGetProperty("types", out _));
            Assert.False(root.TryGetProperty("queries", out _));
            Assert.False(root.TryGetProperty("mutations", out _));
        }

        [Fact]
        public void TestExportTypesCompactFormat()
        {
            Schema.RegisterType("User", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "ID" }, { "nullable", false } } }
            });

            var compact = Schema.ExportTypes(false);
            var pretty = Schema.ExportTypes(true);

            Assert.True(compact.Length <= pretty.Length);

            var compactDoc = JsonDocument.Parse(compact);
            Assert.True(compactDoc.RootElement.TryGetProperty("types", out _));

            var prettyDoc = JsonDocument.Parse(pretty);
            Assert.True(prettyDoc.RootElement.TryGetProperty("types", out _));
        }

        [Fact]
        public void TestExportTypesPrettyFormat()
        {
            Schema.RegisterType("User", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "ID" }, { "nullable", false } } }
            });

            var json = Schema.ExportTypes(true);
            Assert.Contains("\n", json);

            using var doc = JsonDocument.Parse(json);
            Assert.True(doc.RootElement.TryGetProperty("types", out _));
        }

        [Fact]
        public void TestExportTypesFile()
        {
            Schema.RegisterType("User", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "ID" }, { "nullable", false } } },
                { "name", new Dictionary<string, object> { { "type", "String" }, { "nullable", false } } }
            });

            var tmpFile = "/tmp/fraiseql_types_test_csharp.json";
            if (File.Exists(tmpFile))
                File.Delete(tmpFile);

            Schema.ExportTypesFile(tmpFile);

            Assert.True(File.Exists(tmpFile));

            var content = File.ReadAllText(tmpFile);
            using var doc = JsonDocument.Parse(content);
            var types = doc.RootElement.GetProperty("types");
            Assert.Equal(1, types.GetArrayLength());

            File.Delete(tmpFile);
        }

        [Fact]
        public void TestExportTypesEmpty()
        {
            var json = Schema.ExportTypes(true);
            using var doc = JsonDocument.Parse(json);
            var types = doc.RootElement.GetProperty("types");

            Assert.Equal(JsonValueKind.Array, types.ValueKind);
            Assert.Equal(0, types.GetArrayLength());
        }
    }
}
