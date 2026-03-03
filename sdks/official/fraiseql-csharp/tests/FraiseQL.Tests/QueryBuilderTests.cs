using System.Text.Json;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

[Collection(RegistryTestCollection.Name)]
public sealed class QueryBuilderTests : IDisposable
{
    public QueryBuilderTests() => SchemaRegistry.Instance.Clear();
    public void Dispose() => SchemaRegistry.Instance.Clear();

    [Fact]
    public void TestQueryBuilderBasicQuery()
    {
        var query = QueryBuilder.Query("authors")
            .ReturnType("Author")
            .ReturnsList(true)
            .SqlSource("v_author")
            .Build();

        Assert.Equal("authors", query.Name);
        Assert.Equal("Author", query.ReturnType);
        Assert.True(query.ReturnsList);
        Assert.Equal("v_author", query.SqlSource);
    }

    [Fact]
    public void TestQueryBuilderSingleItem()
    {
        var query = QueryBuilder.Query("author")
            .ReturnType("Author")
            .ReturnsList(false)
            .SqlSource("v_author")
            .Build();

        Assert.False(query.ReturnsList);
    }

    [Fact]
    public void TestQueryBuilderWithArgument()
    {
        var query = QueryBuilder.Query("author")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Argument("id", "ID", nullable: false)
            .Build();

        Assert.Single(query.Arguments);
        Assert.Equal("id", query.Arguments[0].Name);
        Assert.Equal("ID", query.Arguments[0].Type);
        Assert.False(query.Arguments[0].Nullable);
    }

    [Fact]
    public void TestQueryBuilderMultipleArguments()
    {
        var query = QueryBuilder.Query("searchAuthors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Argument("name", "String", nullable: true)
            .Argument("limit", "Int", nullable: false)
            .Build();

        Assert.Equal(2, query.Arguments.Count);
        Assert.Equal("name", query.Arguments[0].Name);
        Assert.Equal("limit", query.Arguments[1].Name);
    }

    [Fact]
    public void TestQueryBuilderCacheTtl()
    {
        var query = QueryBuilder.Query("cachedAuthors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .CacheTtlSeconds(300)
            .Build();

        Assert.Equal(300, query.CacheTtlSeconds);
    }

    [Fact]
    public void TestQueryBuilderNullableFalseByDefault()
    {
        var query = QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Build();

        Assert.False(query.Nullable);
    }

    [Fact]
    public void TestQueryBuilderRegister()
    {
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Register();

        var queries = SchemaRegistry.Instance.GetAllQueries();
        Assert.Single(queries);
        Assert.Equal("authors", queries[0].Name);
    }

    [Fact]
    public void TestQueryBuilderExportedInSchema()
    {
        QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .ReturnsList()
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        var queries = doc.RootElement.GetProperty("queries");
        Assert.Equal(JsonValueKind.Array, queries.ValueKind);
        Assert.Equal(1, queries.GetArrayLength());
    }

    [Fact]
    public void TestQueryBuilderRequiresReturnType()
    {
        var ex = Assert.Throws<InvalidOperationException>(() =>
            QueryBuilder.Query("authors")
                .SqlSource("v_author")
                .Build());

        Assert.Contains("ReturnType", ex.Message);
    }

    [Fact]
    public void TestQueryBuilderRequiresSqlSource()
    {
        var ex = Assert.Throws<InvalidOperationException>(() =>
            QueryBuilder.Query("authors")
                .ReturnType("Author")
                .Build());

        Assert.Contains("SqlSource", ex.Message);
    }

    [Fact]
    public void TestQueryBuilderEmptyArgumentsByDefault()
    {
        var query = QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Build();

        Assert.NotNull(query.Arguments);
        Assert.Empty(query.Arguments);
    }

    [Fact]
    public void TestQueryBuilderDescriptionIsOptional()
    {
        var query = QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Build();

        Assert.Null(query.Description);
    }

    [Fact]
    public void TestQueryBuilderDescriptionSet()
    {
        var query = QueryBuilder.Query("authors")
            .ReturnType("Author")
            .SqlSource("v_author")
            .Description("List all authors")
            .Build();

        Assert.Equal("List all authors", query.Description);
    }
}
