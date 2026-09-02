using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// A relationship entry, in the AuthoringIR's spelling (#1266).
/// </summary>
/// <param name="Name">Relationship name — the key in <c>?select=</c> and in the response.</param>
/// <param name="TargetType">Target type name; must be returned by some list query.</param>
/// <param name="Cardinality"><c>OneToMany</c>, <c>ManyToOne</c> or <c>OneToOne</c>.</param>
/// <param name="ForeignKey">Foreign key column on the child table.</param>
/// <param name="ReferencedKey">Referenced key column on the parent table.</param>
public record RelationshipDefinition(
    [property: JsonPropertyName("name")]           string Name,
    [property: JsonPropertyName("target_type")]    string TargetType,
    [property: JsonPropertyName("cardinality")]    string Cardinality,
    [property: JsonPropertyName("foreign_key")]    string ForeignKey,
    [property: JsonPropertyName("referenced_key")] string ReferencedKey);
