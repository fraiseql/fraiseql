using Xunit;

namespace FraiseQL.Tests;

/// <summary>
/// Marks all tests that depend on the <see cref="FraiseQL.Registry.SchemaRegistry"/> singleton
/// as sequential. Because the registry is a process-wide singleton, tests that modify it must
/// not run concurrently with each other.
/// </summary>
[CollectionDefinition(Name)]
public sealed class RegistryTestCollection : ICollectionFixture<object>
{
    /// <summary>Collection name used in <see cref="CollectionAttribute"/>.</summary>
    public const string Name = "RegistryTests";
}
