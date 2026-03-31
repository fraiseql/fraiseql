using System.Text.Json.Serialization;

namespace FraiseQL.Models;

/// <summary>
/// REST endpoint annotation for a query or mutation, emitted in the intermediate schema
/// so the compiler can generate REST routes alongside GraphQL.
/// </summary>
/// <param name="Path">The REST path (e.g. <c>"/api/users"</c>).</param>
/// <param name="Method">The HTTP method (e.g. <c>"GET"</c>, <c>"POST"</c>). Always uppercase.</param>
public record RestAnnotation(
    [property: JsonPropertyName("path")]   string Path,
    [property: JsonPropertyName("method")] string Method);
