using System.Reflection;
using FraiseQL.Attributes;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// A <c>System.DateTime</c> carries its zone in a runtime <c>Kind</c>, not in its type, so
/// the type alone cannot tell a PostgreSQL <c>timestamp</c> from a <c>timestamptz</c>: it
/// stays <c>DateTime</c>. A date and time with no offset is declared with the attribute's
/// type string, <c>[GraphQLField(Type = "LocalDateTime")]</c>, which wins over inference.
/// </summary>
public sealed class LocalDateTimeTests
{
    private sealed class Shift
    {
        [GraphQLField(Type = "LocalDateTime")] public DateTime StartsAt { get; set; }
        [GraphQLField(Type = "LocalDateTime")] public DateTime? EndsAt { get; set; }
        [GraphQLField(Type = "[LocalDateTime!]")] public List<DateTime> BreaksAt { get; set; } = new();
        [GraphQLField] public DateTime LoggedAt { get; set; }
        [GraphQLField] public DateTimeOffset ClockedAt { get; set; }
    }

    private static (string Type, bool Nullable) Detect(string name)
    {
        var prop = typeof(Shift).GetProperty(name)!;
        return TypeMapper.Detect(prop, prop.GetCustomAttribute<GraphQLFieldAttribute>());
    }

    [Theory]
    [InlineData(nameof(Shift.StartsAt), "LocalDateTime", false)]
    [InlineData(nameof(Shift.EndsAt), "LocalDateTime", true)]
    [InlineData(nameof(Shift.BreaksAt), "[LocalDateTime!]", false)]
    public void TheTypeStringDeclaresLocalDateTimeOnADateTimeProperty(
        string property, string type, bool nullable)
    {
        Assert.Equal((type, nullable), Detect(property));
    }

    [Theory]
    [InlineData(nameof(Shift.LoggedAt))]
    [InlineData(nameof(Shift.ClockedAt))]
    public void AnUndeclaredDateTimeStaysDateTime(string property)
    {
        Assert.Equal(("DateTime", false), Detect(property));
    }
}
