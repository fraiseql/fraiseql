using System.Reflection;
using FraiseQL.Attributes;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

public sealed class TypeMapperTests
{
    // Fixture to hold properties of various C# types for reflection
    [GraphQLType(Name = "AllTypes", SqlSource = "v_all")]
    private class AllTypesFixture
    {
        [GraphQLField] public int IntProp { get; set; }
        [GraphQLField] public long LongProp { get; set; }
        [GraphQLField] public short ShortProp { get; set; }
        [GraphQLField] public float FloatProp { get; set; }
        [GraphQLField] public double DoubleProp { get; set; }
        [GraphQLField] public decimal DecimalProp { get; set; }
        [GraphQLField] public bool BoolProp { get; set; }
        [GraphQLField] public Guid GuidProp { get; set; }
        [GraphQLField] public string StringProp { get; set; } = string.Empty;
        [GraphQLField] public DateTime DateTimeProp { get; set; }
        [GraphQLField] public DateTimeOffset DateTimeOffsetProp { get; set; }
        [GraphQLField] public int? NullableIntProp { get; set; }
        [GraphQLField] public long? NullableLongProp { get; set; }
        [GraphQLField] public float? NullableFloatProp { get; set; }
        [GraphQLField] public double? NullableDoubleProp { get; set; }
        [GraphQLField] public bool? NullableBoolProp { get; set; }
        [GraphQLField] public Guid? NullableGuidProp { get; set; }
        [GraphQLField] public string? NullableStringProp { get; set; }
        [GraphQLField(Type = "ID")] public int ExplicitIdProp { get; set; }
        [GraphQLField(Type = "CustomType")] public string ExplicitCustomProp { get; set; } = string.Empty;
    }

    private static PropertyInfo GetProp(string name) =>
        typeof(AllTypesFixture).GetProperty(name)!;

    private static (string type, bool nullable) Detect(string propName)
    {
        var prop = GetProp(propName);
        var attr = prop.GetCustomAttribute<GraphQLFieldAttribute>();
        return TypeMapper.Detect(prop, attr);
    }

    [Fact]
    public void TestIntMapsToInt()
    {
        var (type, nullable) = Detect(nameof(AllTypesFixture.IntProp));
        Assert.Equal("Int", type);
        Assert.False(nullable);
    }

    [Fact]
    public void TestLongMapsToInt()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.LongProp));
        Assert.Equal("Int", type);
    }

    [Fact]
    public void TestShortMapsToInt()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.ShortProp));
        Assert.Equal("Int", type);
    }

    [Fact]
    public void TestFloatMapsToFloat()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.FloatProp));
        Assert.Equal("Float", type);
    }

    [Fact]
    public void TestDoubleMapsToFloat()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.DoubleProp));
        Assert.Equal("Float", type);
    }

    [Fact]
    public void TestDecimalMapsToFloat()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.DecimalProp));
        Assert.Equal("Float", type);
    }

    [Fact]
    public void TestBoolMapsToBoolean()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.BoolProp));
        Assert.Equal("Boolean", type);
    }

    [Fact]
    public void TestGuidMapsToId()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.GuidProp));
        Assert.Equal("ID", type);
    }

    [Fact]
    public void TestStringMapsToString()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.StringProp));
        Assert.Equal("String", type);
    }

    [Fact]
    public void TestDateTimeMapsToString()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.DateTimeProp));
        Assert.Equal("String", type);
    }

    [Fact]
    public void TestDateTimeOffsetMapsToString()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.DateTimeOffsetProp));
        Assert.Equal("String", type);
    }

    [Fact]
    public void TestNullableIntIsNullable()
    {
        var (type, nullable) = Detect(nameof(AllTypesFixture.NullableIntProp));
        Assert.Equal("Int", type);
        Assert.True(nullable);
    }

    [Fact]
    public void TestNullableLongIsNullable()
    {
        var (type, nullable) = Detect(nameof(AllTypesFixture.NullableLongProp));
        Assert.Equal("Int", type);
        Assert.True(nullable);
    }

    [Fact]
    public void TestNullableFloatIsNullable()
    {
        var (type, nullable) = Detect(nameof(AllTypesFixture.NullableFloatProp));
        Assert.Equal("Float", type);
        Assert.True(nullable);
    }

    [Fact]
    public void TestNullableBoolIsNullable()
    {
        var (type, nullable) = Detect(nameof(AllTypesFixture.NullableBoolProp));
        Assert.Equal("Boolean", type);
        Assert.True(nullable);
    }

    [Fact]
    public void TestNullableGuidIsNullable()
    {
        var (type, nullable) = Detect(nameof(AllTypesFixture.NullableGuidProp));
        Assert.Equal("ID", type);
        Assert.True(nullable);
    }

    [Fact]
    public void TestNullableStringIsNullable()
    {
        var (type, nullable) = Detect(nameof(AllTypesFixture.NullableStringProp));
        Assert.Equal("String", type);
        Assert.True(nullable);
    }

    [Fact]
    public void TestExplicitTypeOverridesAutoDetection()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.ExplicitIdProp));
        Assert.Equal("ID", type);
    }

    [Fact]
    public void TestExplicitCustomTypePreserved()
    {
        var (type, _) = Detect(nameof(AllTypesFixture.ExplicitCustomProp));
        Assert.Equal("CustomType", type);
    }
}
