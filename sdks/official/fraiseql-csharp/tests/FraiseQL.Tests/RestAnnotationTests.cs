using FraiseQL.Builders;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

[Collection(RegistryTestCollection.Name)]
public sealed class RestAnnotationTests : IDisposable
{
    public RestAnnotationTests() => SchemaRegistry.Instance.Clear();
    public void Dispose() => SchemaRegistry.Instance.Clear();

    [Fact]
    public void Query_RestPath_EmitsRestBlock()
    {
        var query = QueryBuilder.Query("users")
            .ReturnType("User")
            .ReturnsList()
            .SqlSource("v_user")
            .RestPath("/api/users")
            .RestMethod("GET")
            .Build();

        Assert.NotNull(query.Rest);
        Assert.Equal("/api/users", query.Rest.Path);
        Assert.Equal("GET", query.Rest.Method);
    }

    [Fact]
    public void Query_RestPath_DefaultsToGet()
    {
        var query = QueryBuilder.Query("users")
            .ReturnType("User")
            .ReturnsList()
            .SqlSource("v_user")
            .RestPath("/api/users")
            .Build();

        Assert.NotNull(query.Rest);
        Assert.Equal("GET", query.Rest.Method);
    }

    [Fact]
    public void Query_NoRestPath_OmitsRestBlock()
    {
        var query = QueryBuilder.Query("users")
            .ReturnType("User")
            .ReturnsList()
            .SqlSource("v_user")
            .Build();

        Assert.Null(query.Rest);
    }

    [Fact]
    public void Mutation_RestPath_EmitsRestBlock()
    {
        var mutation = MutationBuilder.Mutation("createUser")
            .ReturnType("User")
            .SqlSource("fn_create_user")
            .Operation("insert")
            .RestPath("/api/users")
            .RestMethod("POST")
            .Build();

        Assert.NotNull(mutation.Rest);
        Assert.Equal("/api/users", mutation.Rest.Path);
        Assert.Equal("POST", mutation.Rest.Method);
    }

    [Fact]
    public void Mutation_RestPath_DefaultsToPost()
    {
        var mutation = MutationBuilder.Mutation("createUser")
            .ReturnType("User")
            .SqlSource("fn_create_user")
            .Operation("insert")
            .RestPath("/api/users")
            .Build();

        Assert.NotNull(mutation.Rest);
        Assert.Equal("POST", mutation.Rest.Method);
    }

    [Fact]
    public void Mutation_NoRestPath_OmitsRestBlock()
    {
        var mutation = MutationBuilder.Mutation("createUser")
            .ReturnType("User")
            .SqlSource("fn_create_user")
            .Operation("insert")
            .Build();

        Assert.Null(mutation.Rest);
    }

    [Fact]
    public void RestMethod_CaseInsensitive()
    {
        var query = QueryBuilder.Query("users")
            .ReturnType("User")
            .ReturnsList()
            .SqlSource("v_user")
            .RestPath("/api/users")
            .RestMethod("post")
            .Build();

        Assert.NotNull(query.Rest);
        Assert.Equal("POST", query.Rest.Method);
    }
}
