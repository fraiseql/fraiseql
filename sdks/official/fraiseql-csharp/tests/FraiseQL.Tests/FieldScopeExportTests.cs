using System.Text.Json;
using FraiseQL.Attributes;
using FraiseQL.Builders;
using FraiseQL.Export;
using FraiseQL.Registry;
using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// A field's required scope, declared as <c>Scope</c> or as a one-item <c>Scopes</c> list,
/// reaches the exported <c>requires_scope</c> on every export path; a declaration the
/// compiled schema cannot hold is refused.
/// </summary>
[Collection(RegistryTestCollection.Name)]
public sealed class FieldScopeExportTests : IDisposable
{
    public FieldScopeExportTests() => SchemaRegistry.Instance.Clear();

    public void Dispose() => SchemaRegistry.Instance.Clear();

    [GraphQLType(Name = "Payroll", SqlSource = "v_payroll")]
    private class PayrollFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "Float", Nullable = true, Scopes = new[] { "read:Payroll.salary" })]
        public double? Salary { get; set; }

        [GraphQLField(Type = "String", Nullable = true, Scope = "read:Payroll.iban")]
        public string? Iban { get; set; }
    }

    [GraphQLType(Name = "TwoScopes", SqlSource = "v_two_scopes")]
    private class TwoScopesFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "String", Nullable = true, Scopes = new[] { "a", "b" })]
        public string? Secret { get; set; }
    }

    [GraphQLType(Name = "BothForms", SqlSource = "v_both_forms")]
    private class BothFormsFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "String", Nullable = true, Scope = "a", Scopes = new[] { "b" })]
        public string? Secret { get; set; }
    }

    [GraphQLType(Name = "CrudTwoScopes", SqlSource = "v_crud_two_scopes", Crud = true)]
    private class CrudTwoScopesFixture
    {
        [GraphQLField(Type = "ID", Nullable = false)]
        public int Id { get; set; }

        [GraphQLField(Type = "String", Nullable = true, Scopes = new[] { "a", "b" })]
        public string? Secret { get; set; }
    }

    private static readonly (string Name, Func<string> Export)[] ExportPaths =
    [
        ("SchemaExporter", () => SchemaExporter.Export()),
        ("SchemaBuilder", () => new SchemaBuilder().Export()),
    ];

    private static JsonElement Field(string json, string typeName, string fieldName)
    {
        using var document = JsonDocument.Parse(json);
        var type = document.RootElement.GetProperty("types").EnumerateArray()
            .Single(t => t.GetProperty("name").GetString() == typeName);
        return type.GetProperty("fields").EnumerateArray()
            .Single(f => f.GetProperty("name").GetString() == fieldName)
            .Clone();
    }

    [Fact]
    public void A_one_item_scopes_list_is_exported_as_the_required_scope()
    {
        SchemaRegistry.Instance.Register(typeof(PayrollFixture));

        foreach (var (name, export) in ExportPaths)
        {
            var json = export();
            Assert.Equal(
                "read:Payroll.salary",
                Field(json, "Payroll", "salary").GetProperty("requires_scope").GetString());
            Assert.Equal(
                "read:Payroll.iban",
                Field(json, "Payroll", "iban").GetProperty("requires_scope").GetString());
            Assert.False(
                Field(json, "Payroll", "id").TryGetProperty("requires_scope", out _),
                $"{name}: an ungated field carries no scope");
        }
    }

    [Fact]
    public void A_longer_scopes_list_is_refused_on_every_export_path()
    {
        SchemaRegistry.Instance.Register(typeof(TwoScopesFixture));

        foreach (var (name, export) in ExportPaths)
        {
            var thrown = Assert.Throws<InvalidOperationException>(() => export());
            Assert.Contains("secret", thrown.Message, StringComparison.Ordinal);
            Assert.Contains("2 scopes", thrown.Message, StringComparison.Ordinal);
        }
    }

    [Fact]
    public void Scope_and_scopes_together_are_refused_on_every_export_path()
    {
        SchemaRegistry.Instance.Register(typeof(BothFormsFixture));

        foreach (var (name, export) in ExportPaths)
        {
            var thrown = Assert.Throws<InvalidOperationException>(() => export());
            Assert.Contains("both Scope and Scopes", thrown.Message, StringComparison.Ordinal);
        }
    }

    /// <summary>
    /// A type generating CRUD operations converts its fields when it is registered, so the
    /// declaration is refused there.
    /// </summary>
    [Fact]
    public void A_longer_scopes_list_on_a_crud_type_is_refused_at_registration()
    {
        var thrown = Assert.Throws<InvalidOperationException>(
            () => SchemaRegistry.Instance.Register(typeof(CrudTwoScopesFixture)));
        Assert.Contains("2 scopes", thrown.Message, StringComparison.Ordinal);
    }
}
