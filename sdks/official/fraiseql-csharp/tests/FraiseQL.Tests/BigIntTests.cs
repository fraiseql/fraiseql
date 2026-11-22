using System.Reflection;
using FraiseQL.Attributes;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// An integer maps to the narrowest scalar holding its range: <c>Int</c> is 32 bits and
/// <c>BigInt</c> a signed 64, so <c>long</c> and <c>uint</c> are <c>BigInt</c>, and no integer
/// scalar holds a <c>ulong</c>.
/// </summary>
public sealed class BigIntTests
{
    private sealed class Ledger
    {
        [GraphQLField] public long Amount { get; set; }
        [GraphQLField] public long? Balance { get; set; }
        [GraphQLField] public uint Count { get; set; }
        [GraphQLField] public List<long> TransferIds { get; set; } = new();
        [GraphQLField] public long?[] Holds { get; set; } = [];
        [GraphQLField] public int Small { get; set; }
    }

    private sealed class Unsigned
    {
        [GraphQLField] public ulong Total { get; set; }
        [GraphQLField] public ulong? MaybeTotal { get; set; }
        [GraphQLField] public List<ulong> Totals { get; set; } = new();
        [GraphQLField(Type = "Decimal")] public ulong Declared { get; set; }
    }

    private static (string Type, bool Nullable) Detect(Type owner, string name)
    {
        var prop = owner.GetProperty(name)!;
        return TypeMapper.Detect(prop, prop.GetCustomAttribute<GraphQLFieldAttribute>());
    }

    [Theory]
    [InlineData(nameof(Ledger.Amount), "BigInt", false)]
    [InlineData(nameof(Ledger.Balance), "BigInt", true)]
    [InlineData(nameof(Ledger.Count), "BigInt", false)]
    [InlineData(nameof(Ledger.TransferIds), "[BigInt!]", false)]
    [InlineData(nameof(Ledger.Holds), "[BigInt]", false)]
    [InlineData(nameof(Ledger.Small), "Int", false)]
    public void SixtyFourBitAndUnsignedThirtyTwoBitIntegersAreBigInt(
        string property, string type, bool nullable)
    {
        Assert.Equal((type, nullable), Detect(typeof(Ledger), property));
    }

    [Theory]
    [InlineData(nameof(Unsigned.Total))]
    [InlineData(nameof(Unsigned.MaybeTotal))]
    [InlineData(nameof(Unsigned.Totals))]
    public void AnUnsignedSixtyFourBitIntegerIsRefusedPointingToDecimal(string property)
    {
        var refused = Assert.Throws<NotSupportedException>(() => Detect(typeof(Unsigned), property));
        Assert.Contains($"Unsigned.{property}", refused.Message);
        Assert.Contains("UInt64", refused.Message);
        Assert.Contains("Decimal", refused.Message);
    }

    [Fact]
    public void TheRefusalsAdviceDeclaresTheField()
    {
        Assert.Equal(("Decimal", false), Detect(typeof(Unsigned), nameof(Unsigned.Declared)));
    }
}
