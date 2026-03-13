using FraiseQL.Attributes;
using FraiseQL.Models;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

[Collection(RegistryTestCollection.Name)]
public sealed class SchemaRegistryTests : IDisposable
{
    public SchemaRegistryTests() => SchemaRegistry.Instance.Clear();
    public void Dispose() => SchemaRegistry.Instance.Clear();

    // --- Fixture types ---

    [GraphQLType(Name = "Author", SqlSource = "v_author", Description = "A blog author")]
    private class AuthorFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "String", Nullable = false)]
        public string Name { get; set; } = string.Empty;

        [GraphQLField(Nullable = true)]
        public string? Bio { get; set; }
    }

    [GraphQLType(Name = "Post", SqlSource = "v_post")]
    private class PostFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public Guid PostId { get; set; }

        [GraphQLField(Type = "String", Nullable = false)]
        public string Title { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "InputType", SqlSource = "v_input", IsInput = true)]
    private class InputFixture
    {
        [GraphQLField]
        public string Value { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "RelayType", SqlSource = "v_item", Relay = true)]
    private class RelayFixture
    {
        [GraphQLField]
        public int Id { get; set; }
    }

    [GraphQLType(Name = "ErrorType", SqlSource = "v_error", IsError = true)]
    private class ErrorFixture
    {
        [GraphQLField]
        public string Message { get; set; } = string.Empty;
    }

    private class NoAttributeFixture
    {
        public string Value { get; set; } = string.Empty;
    }

    // --- Tests: Basic Registration ---

    [Fact]
    public void TestRegisterTypeWithAttribute()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Author");
        Assert.NotNull(td);
        Assert.Equal("Author", td.Name);
        Assert.Equal("v_author", td.SqlSource);
    }

    [Fact]
    public void TestRegisterTypeDescription()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Author");
        Assert.NotNull(td);
        Assert.Equal("A blog author", td.Description);
    }

    [Fact]
    public void TestRegisterTypeWithoutDescription()
    {
        SchemaRegistry.Instance.Register(typeof(PostFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Post");
        Assert.NotNull(td);
        Assert.Null(td.Description);
    }

    [Fact]
    public void TestRegisterTypeNotFound()
    {
        var td = SchemaRegistry.Instance.GetTypeDefinition("NonExistent");
        Assert.Null(td);
    }

    [Fact]
    public void TestRegisterWithoutAttributeThrows()
    {
        var ex = Assert.Throws<InvalidOperationException>(
            () => SchemaRegistry.Instance.Register(typeof(NoAttributeFixture)));
        Assert.Contains("NoAttributeFixture", ex.Message);
        Assert.Contains("[GraphQLType]", ex.Message);
    }

    // --- Tests: Field Reflection ---

    [Fact]
    public void TestFieldReflection()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Author");
        Assert.NotNull(td);
        Assert.Equal(3, td.Fields.Count);
    }

    [Fact]
    public void TestFieldNamesAreCamelCase()
    {
        SchemaRegistry.Instance.Register(typeof(PostFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Post");
        Assert.NotNull(td);
        Assert.Equal("postId", td.Fields[0].Name);
        Assert.Equal("title", td.Fields[1].Name);
    }

    [Fact]
    public void TestExplicitTypeOverridesAutoDetection()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Author");
        Assert.NotNull(td);
        var idField = td.Fields.First(f => f.Name == "id");
        Assert.Equal("ID", idField.Type);
    }

    [Fact]
    public void TestNullableFieldDetected()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Author");
        Assert.NotNull(td);
        var bioField = td.Fields.First(f => f.Name == "bio");
        Assert.True(bioField.Nullable);
    }

    [Fact]
    public void TestNonNullableFieldDetected()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Author");
        Assert.NotNull(td);
        var nameField = td.Fields.First(f => f.Name == "name");
        Assert.False(nameField.Nullable);
    }

    // --- Tests: Type Flags ---

    [Fact]
    public void TestIsInputFlag()
    {
        SchemaRegistry.Instance.Register(typeof(InputFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("InputType");
        Assert.NotNull(td);
        Assert.True(td.IsInput);
    }

    [Fact]
    public void TestRelayFlag()
    {
        SchemaRegistry.Instance.Register(typeof(RelayFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("RelayType");
        Assert.NotNull(td);
        Assert.True(td.Relay);
    }

    [Fact]
    public void TestIsErrorFlag()
    {
        SchemaRegistry.Instance.Register(typeof(ErrorFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("ErrorType");
        Assert.NotNull(td);
        Assert.True(td.IsError);
    }

    // --- Tests: GetAllTypes ---

    [Fact]
    public void TestGetAllTypesReturnsAll()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));
        SchemaRegistry.Instance.Register(typeof(PostFixture));

        var types = SchemaRegistry.Instance.GetAllTypes();
        Assert.Equal(2, types.Count);
    }

    [Fact]
    public void TestGetAllTypesEmptyWhenNoneRegistered()
    {
        var types = SchemaRegistry.Instance.GetAllTypes();
        Assert.Empty(types);
    }

    // --- Tests: Query/Mutation Registration ---

    [Fact]
    public void TestRegisterQuery()
    {
        var query = new IntermediateQuery(
            "authors", "Author", true, false, "v_author",
            Array.Empty<IntermediateArgument>());

        SchemaRegistry.Instance.RegisterQuery(query);

        var queries = SchemaRegistry.Instance.GetAllQueries();
        Assert.Single(queries);
        Assert.Equal("authors", queries[0].Name);
    }

    [Fact]
    public void TestRegisterMutation()
    {
        var mutation = new IntermediateMutation(
            "createAuthor", "Author", "fn_create_author", "insert",
            Array.Empty<IntermediateArgument>());

        SchemaRegistry.Instance.RegisterMutation(mutation);

        var mutations = SchemaRegistry.Instance.GetAllMutations();
        Assert.Single(mutations);
        Assert.Equal("createAuthor", mutations[0].Name);
    }

    // --- Tests: Clear ---

    [Fact]
    public void TestClearRemovesAllTypes()
    {
        SchemaRegistry.Instance.Register(typeof(AuthorFixture));
        SchemaRegistry.Instance.Clear();

        Assert.Empty(SchemaRegistry.Instance.GetAllTypes());
    }

    [Fact]
    public void TestClearRemovesAllQueriesAndMutations()
    {
        SchemaRegistry.Instance.RegisterQuery(
            new IntermediateQuery("q", "T", false, false, "v", Array.Empty<IntermediateArgument>()));
        SchemaRegistry.Instance.RegisterMutation(
            new IntermediateMutation("m", "T", "fn", "insert", Array.Empty<IntermediateArgument>()));

        SchemaRegistry.Instance.Clear();

        Assert.Empty(SchemaRegistry.Instance.GetAllQueries());
        Assert.Empty(SchemaRegistry.Instance.GetAllMutations());
    }
}
