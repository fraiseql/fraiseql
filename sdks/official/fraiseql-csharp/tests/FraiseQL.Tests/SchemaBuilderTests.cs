using System.Text.Json;
using FraiseQL.Attributes;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

[Collection(RegistryTestCollection.Name)]
public sealed class SchemaBuilderTests : IDisposable
{
    public SchemaBuilderTests() => SchemaRegistry.Instance.Clear();
    public void Dispose() => SchemaRegistry.Instance.Clear();

    [GraphQLType(Name = "Author", SqlSource = "v_author", Description = "A blog author")]
    private class AuthorFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "String", Nullable = false)]
        public string Name { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "AuthorInput", IsInput = true)]
    private class AuthorInputFixture
    {
        [GraphQLField(Type = "String", Nullable = false)]
        public string Name { get; set; } = string.Empty;
    }

    private enum Genre { Fiction, Poetry }

    /// <summary>
    /// One of everything the registry holds, each through its own registration path:
    /// an attribute type, an attribute input type, a registered input type, a query, a
    /// mutation, an enum, a function and an observer.
    /// </summary>
    private static void RegisterOneOfEach()
    {
        var registry = SchemaRegistry.Instance;
        registry.Register(typeof(AuthorFixture));
        registry.Register(typeof(AuthorInputFixture));
        registry.RegisterInputType("AuthorFilter",
            new[] { new FraiseQL.Models.IntermediateInputField("name", "String", true) });
        QueryBuilder.Query("authors").ReturnType("Author").ReturnsList().SqlSource("v_author").Register();
        MutationBuilder.Mutation("createAuthor")
            .ReturnType("Author").SqlSource("fn_create_author").Operation("insert").Register();
        registry.RegisterEnum<Genre>();
        FunctionBuilder.Function("notify_author").Trigger("after:mutation:Author:insert").Register();
        ObserverBuilder.Observer("authorCreated")
            .Entity("Author")
            .Event("INSERT")
            .Action(ObserverAction.Email("ops@example.com", "s", "b"))
            .Register();
    }

    [Fact]
    public void ABuilderWithNoFluentAdditionsExportsWhatTheExporterExports()
    {
        RegisterOneOfEach();

        Assert.Equal(SchemaExporter.Export(), new SchemaBuilder().Export());
    }

    [Fact]
    public void AFluentTypeKeepsTheRegistrysEnumsFunctionsAndObservers()
    {
        RegisterOneOfEach();

        var schema = new SchemaBuilder()
            .Type("Book", t => t.SqlSource("v_book").Field("id", "ID"))
            .ToSchema();

        Assert.Equal(new[] { "Genre" }, schema.Enums!.Select(e => e.Name));
        Assert.Equal(new[] { "notify_author" }, schema.Functions!.Select(f => f.Name));
        Assert.Equal(new[] { "authorCreated" }, schema.Observers!.Select(o => o.Name));
    }

    [Fact]
    public void AnAttributeInputTypeIsAnInputTypeNotAnOutputType()
    {
        RegisterOneOfEach();

        var schema = new SchemaBuilder().ToSchema();

        Assert.DoesNotContain("AuthorInput", schema.Types.Select(t => t.Name));
        Assert.Contains("AuthorInput", schema.InputTypes!.Select(t => t.Name));
    }

    [Fact]
    public void TestSchemaBuilderSingleType()
    {
        var schema = new SchemaBuilder()
            .Type("Author", t => t
                .SqlSource("v_author")
                .Description("A blog author")
                .Field("id", "ID", nullable: false)
                .Field("name", "String", nullable: false))
            .ToSchema();

        Assert.Single(schema.Types);
        Assert.Equal("Author", schema.Types[0].Name);
        Assert.Equal("v_author", schema.Types[0].SqlSource);
        Assert.Equal(2, schema.Types[0].Fields.Count);
    }

    [Fact]
    public void TestSchemaBuilderTypeDescription()
    {
        var schema = new SchemaBuilder()
            .Type("Author", t => t
                .SqlSource("v_author")
                .Description("A blog author")
                .Field("id", "ID"))
            .ToSchema();

        Assert.Equal("A blog author", schema.Types[0].Description);
    }

    [Fact]
    public void TestSchemaBuilderTypeWithoutDescription()
    {
        var schema = new SchemaBuilder()
            .Type("Tag", t => t
                .SqlSource("v_tag")
                .Field("id", "ID"))
            .ToSchema();

        Assert.Null(schema.Types[0].Description);
    }

    [Fact]
    public void TestSchemaBuilderFieldNullability()
    {
        var schema = new SchemaBuilder()
            .Type("Author", t => t
                .SqlSource("v_author")
                .Field("id", "ID", nullable: false)
                .Field("bio", "String", nullable: true))
            .ToSchema();

        var fields = schema.Types[0].Fields;
        Assert.False(fields[0].Nullable);
        Assert.True(fields[1].Nullable);
    }

    [Fact]
    public void TestSchemaBuilderWithQueryAndMutation()
    {
        var schema = new SchemaBuilder()
            .Type("Author", t => t.SqlSource("v_author").Field("id", "ID"))
            .Query("authors", q => q
                .ReturnType("Author")
                .ReturnsList()
                .SqlSource("v_author"))
            .Mutation("createAuthor", m => m
                .ReturnType("Author")
                .SqlSource("fn_create_author")
                .Operation("insert")
                .Argument("name", "String"))
            .ToSchema();

        Assert.Single(schema.Queries);
        Assert.Single(schema.Mutations);
        Assert.Equal("authors", schema.Queries[0].Name);
        Assert.Equal("createAuthor", schema.Mutations[0].Name);
    }

    [Fact]
    public void TestSchemaBuilderMultipleTypes()
    {
        var schema = new SchemaBuilder()
            .Type("Author", t => t.SqlSource("v_author").Field("id", "ID"))
            .Type("Post", t => t.SqlSource("v_post").Field("id", "ID"))
            .ToSchema();

        Assert.Equal(2, schema.Types.Count);
    }

    [Fact]
    public void TestSchemaBuilderVersionIs200()
    {
        var schema = new SchemaBuilder().ToSchema();
        Assert.Equal("2.0.0", schema.Version);
    }

    [Fact]
    public void TestSchemaBuilderExportToFile()
    {
        var path = Path.Combine(Path.GetTempPath(), $"fraiseql_builder_{Guid.NewGuid():N}.json");
        try
        {
            new SchemaBuilder()
                .Type("Author", t => t.SqlSource("v_author").Field("id", "ID"))
                .ExportToFile(path);

            Assert.True(File.Exists(path));
            var content = File.ReadAllText(path);
            var doc = JsonDocument.Parse(content);
            Assert.Equal("2.0.0", doc.RootElement.GetProperty("version").GetString());
        }
        finally
        {
            if (File.Exists(path)) File.Delete(path);
        }
    }

    [Fact]
    public void TestMixedAttributeAndFluent()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var schema = new SchemaBuilder()
            .Query("authors", q => q
                .ReturnType("Author")
                .ReturnsList()
                .SqlSource("v_author"))
            .ToSchema();

        Assert.Single(schema.Types);   // from attribute
        Assert.Single(schema.Queries); // from fluent
        Assert.Equal("Author", schema.Types[0].Name);
        Assert.Equal("authors", schema.Queries[0].Name);
    }

    [Fact]
    public void TestFluentTypeOverridesAttributeTypeOnConflict()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var schema = new SchemaBuilder()
            .Type("Author", t => t
                .SqlSource("v_author_overridden")
                .Field("id", "ID"))
            .ToSchema();

        Assert.Single(schema.Types);
        Assert.Equal("v_author_overridden", schema.Types[0].SqlSource);
    }

    [Fact]
    public void TestSchemaBuilderEmptyQueriesAndMutations()
    {
        var schema = new SchemaBuilder()
            .Type("Author", t => t.SqlSource("v_author").Field("id", "ID"))
            .ToSchema();

        Assert.Empty(schema.Queries);
        Assert.Empty(schema.Mutations);
    }

    [Fact]
    public void TestSchemaBuilderExportSnakeCaseKeys()
    {
        var json = new SchemaBuilder()
            .Type("Author", t => t.SqlSource("v_author").Field("id", "ID"))
            .Query("authors", q => q.ReturnType("Author").SqlSource("v_author").ReturnsList())
            .Export(pretty: false);

        Assert.Contains("\"sql_source\"", json);
        Assert.Contains("\"return_type\"", json);
        Assert.Contains("\"returns_list\"", json);
    }
}
