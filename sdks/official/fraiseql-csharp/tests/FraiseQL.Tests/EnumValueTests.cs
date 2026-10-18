using System.Text.Json;
using System.Text.Json.Serialization;
using FraiseQL.Export;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// #1611: a member is stored as the value C# gives it, as <c>System.Text.Json</c> writes it:
/// its integer, or its name (or <c>JsonStringEnumMemberName</c>) when the enum is serialized
/// as strings.
/// </summary>
[Collection(RegistryTestCollection.Name)]
public sealed class EnumValueTests : IDisposable
{
    public EnumValueTests() => SchemaRegistry.Instance.Clear();

    public void Dispose() => SchemaRegistry.Instance.Clear();

    public enum Priority
    {
        Low = 1,
        High = 3,
    }

    [JsonConverter(typeof(JsonStringEnumConverter))]
    public enum OrderStatus
    {
        Pending,
        [JsonStringEnumMemberName("transit")]
        InTransit,
    }

    private static string Values(string name)
    {
        using var schema = JsonDocument.Parse(SchemaExporter.Export(pretty: false));
        var found = schema.RootElement.GetProperty("enums").EnumerateArray()
            .Single(e => e.GetProperty("name").GetString() == name);
        return found.GetProperty("values").GetRawText();
    }

    [Fact]
    public void AnEnumIsStoredAsItsIntegers()
    {
        SchemaRegistry.Instance.RegisterEnum<Priority>("The priority of a task");

        Assert.Equal(
            """[{"name":"Low","value":1},{"name":"High","value":3}]""",
            Values("Priority"));
    }

    [Fact]
    public void AnEnumSerializedAsStringsIsStoredAsItsNames()
    {
        SchemaRegistry.Instance.RegisterEnum<OrderStatus>();

        Assert.Equal(
            """[{"name":"Pending"},{"name":"InTransit","value":"transit"}]""",
            Values("OrderStatus"));
    }
}
