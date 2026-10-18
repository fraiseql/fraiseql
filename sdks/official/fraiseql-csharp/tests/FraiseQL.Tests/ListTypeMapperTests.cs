using System.Reflection;
using FraiseQL.Attributes;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// A list position is non-null unless C# marks it nullable (#1586, #1608): <c>T?</c> on a
/// value type (<c>Nullable&lt;T&gt;</c>) or on a reference type (nullable reference types),
/// at every level, for a collection as for an array.
/// </summary>
public sealed class ListTypeMapperTests
{
    [GraphQLType(Name = "Series", SqlSource = "v_series")]
    private class Series
    {
        [GraphQLField] public List<string> Labels { get; set; } = new();
        [GraphQLField] public List<double?>? Readings { get; set; }
        [GraphQLField] public List<string?> Notes { get; set; } = new();
        [GraphQLField] public int[] Counts { get; set; } = Array.Empty<int>();
        [GraphQLField] public IReadOnlyList<Guid> Ids { get; set; } = Array.Empty<Guid>();
        [GraphQLField] public List<List<double?>> Grid { get; set; } = new();
        [GraphQLField] public List<List<double>?> Rows { get; set; } = new();
    }

    private static (string Type, bool Nullable) Detect(string name)
    {
        var prop = typeof(Series).GetProperty(name)!;
        return TypeMapper.Detect(prop, prop.GetCustomAttribute<GraphQLFieldAttribute>());
    }

    [Theory]
    [InlineData(nameof(Series.Labels), "[String!]", false)]
    [InlineData(nameof(Series.Readings), "[Float]", true)]
    [InlineData(nameof(Series.Notes), "[String]", false)]
    [InlineData(nameof(Series.Counts), "[Int!]", false)]
    [InlineData(nameof(Series.Ids), "[ID!]", false)]
    [InlineData(nameof(Series.Grid), "[[Float]!]", false)]
    [InlineData(nameof(Series.Rows), "[[Float!]]", false)]
    public void EachListLevelIsNonNullUnlessMarkedNullable(string property, string type, bool nullable)
    {
        Assert.Equal((type, nullable), Detect(property));
    }
}
