using System.Text.Json;
using FraiseQL.Attributes;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

[Collection(RegistryTestCollection.Name)]
public sealed class SchemaExporterTests : IDisposable
{
    private static readonly string TempFile = Path.Combine(
        Path.GetTempPath(), $"fraiseql_test_{Guid.NewGuid():N}.json");

    public SchemaExporterTests() => SchemaRegistry.Instance.Clear();

    public void Dispose()
    {
        SchemaRegistry.Instance.Clear();
        if (File.Exists(TempFile))
            File.Delete(TempFile);
    }

    // --- Fixture types ---

    [GraphQLType(Name = "Author", SqlSource = "v_author", Description = "A blog author")]
    private class AuthorFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "String", Nullable = false)]
        public string Name { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Nullable = true)]
        public string? Bio { get; set; }
    }

    [GraphQLType(Name = "Tag", SqlSource = "v_tag")]
    private class TagFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "String", Nullable = false)]
        public string Label { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Nullable = false)]
        public string Slug { get; set; } = string.Empty;
    }

    // --- Version field ---

    [Fact]
    public void TestExportVersionField()
    {
        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        Assert.Equal("2.0.0", doc.RootElement.GetProperty("version").GetString());
    }

    // --- Array shapes ---

    [Fact]
    public void TestExportTypesIsArray()
    {
        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        Assert.Equal(JsonValueKind.Array, doc.RootElement.GetProperty("types").ValueKind);
    }

    [Fact]
    public void TestGoldenQueriesIsArray()
    {
        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        Assert.Equal(JsonValueKind.Array, doc.RootElement.GetProperty("queries").ValueKind);
    }

    [Fact]
    public void TestGoldenMutationsIsArray()
    {
        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        Assert.Equal(JsonValueKind.Array, doc.RootElement.GetProperty("mutations").ValueKind);
    }

    // --- snake_case key names ---

    [Fact]
    public void TestExportTypesSqlSourceKey()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var json = SchemaExporter.Export(pretty: false);
        // Verify the raw JSON string contains "sql_source", not "sqlSource"
        Assert.Contains("\"sql_source\"", json);
        Assert.DoesNotContain("\"sqlSource\"", json);
    }

    [Fact]
    public void TestExportFieldsNullableKey()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var json = SchemaExporter.Export(pretty: false);
        Assert.Contains("\"nullable\"", json);
        Assert.DoesNotContain("\"isNullable\"", json);
    }

    [Fact]
    public void TestGoldenReturnTypeKey()
    {
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .ReturnsList()
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        Assert.Contains("\"return_type\"", json);
        Assert.DoesNotContain("\"returnType\"", json);
    }

    [Fact]
    public void TestGoldenReturnsListKey()
    {
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .ReturnsList()
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        Assert.Contains("\"returns_list\"", json);
        Assert.DoesNotContain("\"returnsList\"", json);
    }

    [Fact]
    public void TestGoldenSqlSourceKey()
    {
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        Assert.Contains("\"sql_source\"", json);
    }

    // --- Optional field omission ---

    [Fact]
    public void TestExportDescriptionOmittedWhenNull()
    {
        SchemaRegistry.Instance.Register(typeof(TagFixture));

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        var type = doc.RootElement.GetProperty("types")[0];
        Assert.False(type.TryGetProperty("description", out _));
    }

    [Fact]
    public void TestGoldenDescriptionPresent()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        var type = doc.RootElement.GetProperty("types")[0];
        Assert.True(type.TryGetProperty("description", out var desc));
        Assert.Equal("A blog author", desc.GetString());
    }

    [Fact]
    public void TestGoldenCacheTtlSecondsOmitted()
    {
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        Assert.DoesNotContain("\"cache_ttl_seconds\"", json);
    }

    [Fact]
    public void TestGoldenCacheTtlSecondsPresent()
    {
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .CacheTtlSeconds(300)
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        Assert.Contains("\"cache_ttl_seconds\"", json);
        var doc = JsonDocument.Parse(json);
        var query = doc.RootElement.GetProperty("queries")[0];
        Assert.Equal(300, query.GetProperty("cache_ttl_seconds").GetInt32());
    }

    // --- Type export ---

    [Fact]
    public void TestExportTypeMultipleFields()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        var fields = doc.RootElement.GetProperty("types")[0].GetProperty("fields");
        Assert.Equal(3, fields.GetArrayLength());
    }

    [Fact]
    public void TestGoldenArgumentsNullableFalse()
    {
        QueryBuilder.Query("author")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Argument("id", "ID", nullable: false)
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        var arg = doc.RootElement.GetProperty("queries")[0].GetProperty("arguments")[0];
        Assert.False(arg.GetProperty("nullable").GetBoolean());
    }

    // --- File export ---

    [Fact]
    public void TestGoldenExportToFileCreatesFile()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));
        SchemaExporter.ExportToFile(TempFile);

        Assert.True(File.Exists(TempFile));
        var content = File.ReadAllText(TempFile);
        var doc = JsonDocument.Parse(content);
        Assert.Equal("2.0.0", doc.RootElement.GetProperty("version").GetString());
    }

    // --- Golden full examples ---

    [Fact]
    public void TestGoldenMinimalSchema()
    {
        SchemaRegistry.Instance.Register(typeof(TagFixture));

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);

        Assert.Equal("2.0.0", doc.RootElement.GetProperty("version").GetString());
        Assert.Equal(1, doc.RootElement.GetProperty("types").GetArrayLength());
        Assert.Equal(0, doc.RootElement.GetProperty("queries").GetArrayLength());
        Assert.Equal(0, doc.RootElement.GetProperty("mutations").GetArrayLength());
    }

    [Fact]
    public void TestGoldenSchemaWithQuery()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .ReturnsList()
            .SqlSource("v_author")
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);

        Assert.Equal(1, doc.RootElement.GetProperty("types").GetArrayLength());
        Assert.Equal(1, doc.RootElement.GetProperty("queries").GetArrayLength());

        var query = doc.RootElement.GetProperty("queries")[0];
        Assert.Equal("authors", query.GetProperty("name").GetString());
        Assert.Equal("Author", query.GetProperty("return_type").GetString());
        Assert.True(query.GetProperty("returns_list").GetBoolean());
    }

    [Fact]
    public void TestGoldenSchemaWithMutation()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));
        MutationBuilder.Mutation("createAuthor")
            .ReturnType("Author")
            .SqlSource("fn_create_author")
            .Operation("insert")
            .Argument("name", "String")
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);

        Assert.Equal(1, doc.RootElement.GetProperty("mutations").GetArrayLength());
        var mutation = doc.RootElement.GetProperty("mutations")[0];
        Assert.Equal("createAuthor", mutation.GetProperty("name").GetString());
        Assert.Equal("Author", mutation.GetProperty("return_type").GetString());
        Assert.Equal("fn_create_author", mutation.GetProperty("sql_source").GetString());
        Assert.Equal("insert", mutation.GetProperty("operation").GetString());
    }

    [Fact]
    public void TestGoldenSchemaFullExample()
    {
        // Matches the spec's example from Appendix A
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .ReturnsList()
            .SqlSource("v_author")
            .Register();
        QueryBuilder.Query("author")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Argument("id", "ID", nullable: false)
            .Register();
        MutationBuilder.Mutation("createAuthor")
            .ReturnType("Author")
            .SqlSource("fn_create_author")
            .Operation("insert")
            .Argument("name", "String", nullable: false)
            .Register();

        var json = SchemaExporter.Export(pretty: true);
        var doc = JsonDocument.Parse(json);

        Assert.Equal("2.0.0", doc.RootElement.GetProperty("version").GetString());
        Assert.Equal(1, doc.RootElement.GetProperty("types").GetArrayLength());
        Assert.Equal(2, doc.RootElement.GetProperty("queries").GetArrayLength());
        Assert.Equal(1, doc.RootElement.GetProperty("mutations").GetArrayLength());

        var type = doc.RootElement.GetProperty("types")[0];
        Assert.Equal("Author", type.GetProperty("name").GetString());
        Assert.Equal("v_author", type.GetProperty("sql_source").GetString());
        Assert.Equal("A blog author", type.GetProperty("description").GetString());
        Assert.Equal(3, type.GetProperty("fields").GetArrayLength());
    }
}
