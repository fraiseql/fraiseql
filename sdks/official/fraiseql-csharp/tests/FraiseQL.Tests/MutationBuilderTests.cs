using System.Text.Json;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

[Collection(RegistryTestCollection.Name)]
public sealed class MutationBuilderTests : IDisposable
{
    public MutationBuilderTests() => SchemaRegistry.Instance.Clear();
    public void Dispose() => SchemaRegistry.Instance.Clear();

    [Fact]
    public void TestMutationBuilderBasicMutation()
    {
        var mutation = MutationBuilder.Mutation("createAuthor")
            .ReturnType("Author")
            .SqlSource("fn_create_author")
            .Operation("CREATE")
            .Build();

        Assert.Equal("createAuthor", mutation.Name);
        Assert.Equal("Author", mutation.ReturnType);
        Assert.Equal("fn_create_author", mutation.SqlSource);
        Assert.Equal("CREATE", mutation.Operation);
    }

    [Fact]
    public void TestMutationBuilderWithArgument()
    {
        var mutation = MutationBuilder.Mutation("createAuthor")
            .ReturnType("Author")
            .SqlSource("fn_create_author")
            .Operation("CREATE")
            .Argument("name", "String", nullable: false)
            .Build();

        Assert.Single(mutation.Arguments);
        Assert.Equal("name", mutation.Arguments[0].Name);
        Assert.Equal("String", mutation.Arguments[0].Type);
        Assert.False(mutation.Arguments[0].Nullable);
    }

    [Fact]
    public void TestMutationBuilderMultipleArguments()
    {
        var mutation = MutationBuilder.Mutation("updateAuthor")
            .ReturnType("Author")
            .SqlSource("fn_update_author")
            .Operation("UPDATE")
            .Argument("id", "ID", nullable: false)
            .Argument("name", "String", nullable: true)
            .Build();

        Assert.Equal(2, mutation.Arguments.Count);
    }

    [Fact]
    public void TestMutationBuilderEmptyArgumentsArray()
    {
        var mutation = MutationBuilder.Mutation("deleteAuthor")
            .ReturnType("Author")
            .SqlSource("fn_delete_author")
            .Operation("DELETE")
            .Build();

        Assert.NotNull(mutation.Arguments);
        Assert.Empty(mutation.Arguments);
    }

    [Fact]
    public void TestMutationBuilderInsertOperation()
    {
        var mutation = MutationBuilder.Mutation("m")
            .ReturnType("T")
            .SqlSource("fn_m")
            .Operation("CREATE")
            .Build();
        Assert.Equal("CREATE", mutation.Operation);
    }

    [Fact]
    public void TestMutationBuilderUpdateOperation()
    {
        var mutation = MutationBuilder.Mutation("m")
            .ReturnType("T")
            .SqlSource("fn_m")
            .Operation("UPDATE")
            .Build();
        Assert.Equal("UPDATE", mutation.Operation);
    }

    [Fact]
    public void TestMutationBuilderDeleteOperation()
    {
        var mutation = MutationBuilder.Mutation("m")
            .ReturnType("T")
            .SqlSource("fn_m")
            .Operation("DELETE")
            .Build();
        Assert.Equal("DELETE", mutation.Operation);
    }

    [Fact]
    public void TestMutationBuilderCustomOperation()
    {
        var mutation = MutationBuilder.Mutation("m")
            .ReturnType("T")
            .SqlSource("fn_m")
            .Operation("CUSTOM")
            .Build();
        Assert.Equal("CUSTOM", mutation.Operation);
    }

    [Fact]
    public void TestMutationBuilderInvalidOperationThrows()
    {
        var ex = Assert.Throws<ArgumentException>(() =>
            MutationBuilder.Mutation("m")
                .ReturnType("T")
                .SqlSource("fn_m")
                .Operation("INVALID"));

        Assert.Contains("CREATE", ex.Message);
    }

    [Fact]
    public void TestMutationBuilderRegister()
    {
        MutationBuilder.Mutation("createAuthor")
            .ReturnType("Author")
            .SqlSource("fn_create_author")
            .Operation("CREATE")
            .Register();

        var mutations = SchemaRegistry.Instance.GetAllMutations();
        Assert.Single(mutations);
        Assert.Equal("createAuthor", mutations[0].Name);
    }

    [Fact]
    public void TestMutationBuilderExportedInSchema()
    {
        MutationBuilder.Mutation("createAuthor")
            .ReturnType("Author")
            .SqlSource("fn_create_author")
            .Operation("CREATE")
            .Register();

        var json = SchemaExporter.Export(pretty: false);
        var doc = JsonDocument.Parse(json);
        var mutations = doc.RootElement.GetProperty("mutations");
        Assert.Equal(JsonValueKind.Array, mutations.ValueKind);
        Assert.Equal(1, mutations.GetArrayLength());
    }

    [Fact]
    public void TestMutationBuilderRequiresReturnType()
    {
        var ex = Assert.Throws<InvalidOperationException>(() =>
            MutationBuilder.Mutation("m")
                .SqlSource("fn_m")
                .Operation("CREATE")
                .Build());
        Assert.Contains("ReturnType", ex.Message);
    }

    [Fact]
    public void TestMutationBuilderRequiresSqlSource()
    {
        var ex = Assert.Throws<InvalidOperationException>(() =>
            MutationBuilder.Mutation("m")
                .ReturnType("T")
                .Operation("CREATE")
                .Build());
        Assert.Contains("SqlSource", ex.Message);
    }

    [Fact]
    public void TestMutationBuilderRequiresOperation()
    {
        var ex = Assert.Throws<InvalidOperationException>(() =>
            MutationBuilder.Mutation("m")
                .ReturnType("T")
                .SqlSource("fn_m")
                .Build());
        Assert.Contains("Operation", ex.Message);
    }
}
