using FraiseQL.Attributes;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

[Collection(RegistryTestCollection.Name)]
public sealed class ExportTypesTests : IDisposable
{
    public ExportTypesTests() => SchemaRegistry.Instance.Clear();
    public void Dispose() => SchemaRegistry.Instance.Clear();

    [GraphQLType(Name = "User", SqlSource = "v_user", Description = "User in the system")]
    private class UserFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "String", Nullable = false)]
        public string Name { get; set; } = string.Empty;

        [GraphQLField(Type = "String", Nullable = false)]
        public string Email { get; set; } = string.Empty;
    }

    [GraphQLType(Name = "Product", SqlSource = "v_product")]
    private class ProductFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public Guid ProductId { get; set; }

        [GraphQLField(Type = "String", Nullable = false)]
        public string Label { get; set; } = string.Empty;

        [GraphQLField(Type = "Float", Nullable = false)]
        public decimal Price { get; set; }

        [GraphQLField(Type = "Int", Nullable = false)]
        public int Stock { get; set; }

        [GraphQLField(Type = "Boolean", Nullable = false)]
        public bool Active { get; set; }
    }

    [Fact]
    public void TestRegisterSingleTypeDescription()
    {
        SchemaRegistry.Instance.Register(typeof(UserFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("User");
        Assert.NotNull(td);
        Assert.Equal("User in the system", td.Description);
    }

    [Fact]
    public void TestRegisterSingleTypeSqlSource()
    {
        SchemaRegistry.Instance.Register(typeof(UserFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("User");
        Assert.NotNull(td);
        Assert.Equal("v_user", td.SqlSource);
    }

    [Fact]
    public void TestRegisterSingleTypeFieldCount()
    {
        SchemaRegistry.Instance.Register(typeof(UserFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("User");
        Assert.NotNull(td);
        Assert.Equal(3, td.Fields.Count);
    }

    [Fact]
    public void TestRegisterSingleTypeFieldNames()
    {
        SchemaRegistry.Instance.Register(typeof(UserFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("User");
        Assert.NotNull(td);
        Assert.Equal("id", td.Fields[0].Name);
        Assert.Equal("name", td.Fields[1].Name);
        Assert.Equal("email", td.Fields[2].Name);
    }

    [Fact]
    public void TestRegisterMultipleTypes()
    {
        SchemaRegistry.Instance.Register(typeof(UserFixture));
        SchemaRegistry.Instance.Register(typeof(ProductFixture));

        Assert.Equal(2, SchemaRegistry.Instance.GetAllTypes().Count);
    }

    [Fact]
    public void TestProductTypeFieldTypes()
    {
        SchemaRegistry.Instance.Register(typeof(ProductFixture));

        var td = SchemaRegistry.Instance.GetTypeDefinition("Product");
        Assert.NotNull(td);
        Assert.Equal("ID", td.Fields.First(f => f.Name == "productId").Type);
        Assert.Equal("String", td.Fields.First(f => f.Name == "label").Type);
        Assert.Equal("Float", td.Fields.First(f => f.Name == "price").Type);
        Assert.Equal("Int", td.Fields.First(f => f.Name == "stock").Type);
        Assert.Equal("Boolean", td.Fields.First(f => f.Name == "active").Type);
    }
}
