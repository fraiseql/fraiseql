using System;
using System.Collections.Generic;
using System.Linq;
using System.Text.Json;
using Xunit;
using FraiseQL;

namespace FraiseQL.Tests
{
    /// <summary>
    /// Phase 18 Cycle 15: Field-Level RBAC for C# SDK
    ///
    /// Tests that field scopes are properly extracted from field configuration,
    /// stored in field registry, and exported to JSON for compiler consumption.
    ///
    /// RED Phase: 21 comprehensive test cases
    /// - 15 happy path tests for scope extraction and export
    /// - 6 validation tests for error handling
    ///
    /// Field format:
    /// - Single scope: { "type": "Float", "requiresScope": "read:user.salary" }
    /// - Multiple scopes: { "type": "String", "requiresScopes": ["admin", "auditor"] }
    /// </summary>
    public class Phase18Cycle15ScopeExtractionTests : IDisposable
    {
        public Phase18Cycle15ScopeExtractionTests()
        {
            Schema.Reset();
        }

        public void Dispose()
        {
            Schema.Reset();
        }

        // =========================================================================
        // HAPPY PATH: SINGLE SCOPE EXTRACTION (3 tests)
        // =========================================================================

        [Fact]
        public void TestSingleScopeExtraction()
        {
            // RED: This test fails because field registry doesn't store scope
            Schema.RegisterType("UserWithScope", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "Int" } } },
                { "salary", new Dictionary<string, object> { { "type", "Float" }, { "requiresScope", "read:user.salary" } } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("UserWithScope");
            Assert.NotNull(typeInfo);

            var salaryField = typeInfo.Value.Fields["salary"] as Dictionary<string, object>;
            Assert.NotNull(salaryField);
            Assert.True(salaryField.ContainsKey("requiresScope"));
            Assert.Equal("read:user.salary", salaryField["requiresScope"]);
        }

        [Fact]
        public void TestMultipleDifferentScopesExtraction()
        {
            // RED: Tests extraction of different scopes on different fields
            Schema.RegisterType("UserWithMultipleScopes", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "Int" } } },
                { "email", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:user.email" } } },
                { "phone", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:user.phone" } } },
                { "ssn", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:user.ssn" } } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("UserWithMultipleScopes");
            Assert.NotNull(typeInfo);

            var emailField = typeInfo.Value.Fields["email"] as Dictionary<string, object>;
            var phoneField = typeInfo.Value.Fields["phone"] as Dictionary<string, object>;
            var ssnField = typeInfo.Value.Fields["ssn"] as Dictionary<string, object>;

            Assert.Equal("read:user.email", emailField?["requiresScope"]);
            Assert.Equal("read:user.phone", phoneField?["requiresScope"]);
            Assert.Equal("read:user.ssn", ssnField?["requiresScope"]);
        }

        [Fact]
        public void TestPublicFieldNoScopeExtraction()
        {
            // RED: Public fields should have no scope
            Schema.RegisterType("UserWithMixedFields", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "Int" } } },
                { "name", new Dictionary<string, object> { { "type", "String" } } },
                { "email", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:user.email" } } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("UserWithMixedFields");
            Assert.NotNull(typeInfo);

            var idField = typeInfo.Value.Fields["id"] as Dictionary<string, object>;
            Assert.False(idField?.ContainsKey("requiresScope") ?? false);
        }

        // =========================================================================
        // HAPPY PATH: MULTIPLE SCOPES ON SINGLE FIELD (3 tests)
        // =========================================================================

        [Fact]
        public void TestMultipleScopesOnSingleField()
        {
            // RED: Field with requiresScopes array
            Schema.RegisterType("AdminWithMultipleScopes", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "Int" } } },
                { "adminNotes", new Dictionary<string, object>
                {
                    { "type", "String" },
                    { "requiresScopes", new List<object> { "admin", "auditor" } }
                } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("AdminWithMultipleScopes");
            Assert.NotNull(typeInfo);

            var adminField = typeInfo.Value.Fields["adminNotes"] as Dictionary<string, object>;
            Assert.NotNull(adminField);
            Assert.True(adminField.ContainsKey("requiresScopes"));

            var scopes = adminField["requiresScopes"] as List<object>;
            Assert.NotNull(scopes);
            Assert.Equal(2, scopes.Count);
            Assert.Contains("admin", scopes.Cast<string>());
            Assert.Contains("auditor", scopes.Cast<string>());
        }

        [Fact]
        public void TestMixedSingleAndMultipleScopes()
        {
            // RED: Type with both single-scope and multi-scope fields
            Schema.RegisterType("MixedScopeTypes", new Dictionary<string, object>
            {
                { "basicField", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:basic" } } },
                { "advancedField", new Dictionary<string, object>
                {
                    { "type", "String" },
                    { "requiresScopes", new List<object> { "read:advanced", "admin" } }
                } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("MixedScopeTypes");
            Assert.NotNull(typeInfo);

            var basicField = typeInfo.Value.Fields["basicField"] as Dictionary<string, object>;
            var advancedField = typeInfo.Value.Fields["advancedField"] as Dictionary<string, object>;

            Assert.Equal("read:basic", basicField?["requiresScope"]);
            Assert.Equal(2, (advancedField?["requiresScopes"] as List<object>)?.Count);
        }

        [Fact]
        public void TestScopeArrayOrder()
        {
            // RED: Scopes array order must be preserved
            Schema.RegisterType("OrderedScopes", new Dictionary<string, object>
            {
                { "restricted", new Dictionary<string, object>
                {
                    { "type", "String" },
                    { "requiresScopes", new List<object> { "first", "second", "third" } }
                } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("OrderedScopes");
            Assert.NotNull(typeInfo);

            var field = typeInfo.Value.Fields["restricted"] as Dictionary<string, object>;
            var scopes = field?["requiresScopes"] as List<object>;

            Assert.NotNull(scopes);
            Assert.Equal(3, scopes.Count);
            Assert.Equal("first", scopes[0]);
            Assert.Equal("second", scopes[1]);
            Assert.Equal("third", scopes[2]);
        }

        // =========================================================================
        // HAPPY PATH: SCOPE PATTERNS (3 tests)
        // =========================================================================

        [Fact]
        public void TestResourceBasedScopePattern()
        {
            // RED: Resource pattern like read:User.email
            Schema.RegisterType("ResourcePatternScopes", new Dictionary<string, object>
            {
                { "email", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:User.email" } } },
                { "phone", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:User.phone" } } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("ResourcePatternScopes");
            Assert.NotNull(typeInfo);

            var emailField = typeInfo.Value.Fields["email"] as Dictionary<string, object>;
            Assert.Equal("read:User.email", emailField?["requiresScope"]);
        }

        [Fact]
        public void TestActionBasedScopePattern()
        {
            // RED: Action patterns like read:*, write:*, admin:*
            Schema.RegisterType("ActionPatternScopes", new Dictionary<string, object>
            {
                { "readableField", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:User.*" } } },
                { "writableField", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "write:User.*" } } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("ActionPatternScopes");
            Assert.NotNull(typeInfo);

            var readField = typeInfo.Value.Fields["readableField"] as Dictionary<string, object>;
            var writeField = typeInfo.Value.Fields["writableField"] as Dictionary<string, object>;

            Assert.Equal("read:User.*", readField?["requiresScope"]);
            Assert.Equal("write:User.*", writeField?["requiresScope"]);
        }

        [Fact]
        public void TestGlobalWildcardScope()
        {
            // RED: Global wildcard matching all scopes
            Schema.RegisterType("GlobalWildcardScope", new Dictionary<string, object>
            {
                { "adminOverride", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "*" } } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("GlobalWildcardScope");
            Assert.NotNull(typeInfo);

            var field = typeInfo.Value.Fields["adminOverride"] as Dictionary<string, object>;
            Assert.Equal("*", field?["requiresScope"]);
        }

        // =========================================================================
        // HAPPY PATH: JSON EXPORT (3 tests)
        // =========================================================================

        [Fact]
        public void TestSingleScopeJsonExport()
        {
            // RED: Scope must appear in JSON export
            Schema.RegisterType("ExportTestSingleScope", new Dictionary<string, object>
            {
                { "salary", new Dictionary<string, object> { { "type", "Float" }, { "requiresScope", "read:user.salary" } } }
            });

            var json = Schema.ExportTypes(true);
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;

            Assert.True(root.TryGetProperty("types", out var types));
            Assert.Equal(1, types.GetArrayLength());

            var salaryField = types[0].GetProperty("fields")[0];
            Assert.True(salaryField.TryGetProperty("requiresScope", out var scope));
            Assert.Equal("read:user.salary", scope.GetString());
        }

        [Fact]
        public void TestMultipleScopesJsonExport()
        {
            // RED: requiresScopes array exported correctly
            Schema.RegisterType("ExportTestMultipleScopes", new Dictionary<string, object>
            {
                { "restricted", new Dictionary<string, object>
                {
                    { "type", "String" },
                    { "requiresScopes", new List<object> { "scope1", "scope2" } }
                } }
            });

            var json = Schema.ExportTypes(true);
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;

            var field = root.GetProperty("types")[0].GetProperty("fields")[0];
            Assert.True(field.TryGetProperty("requiresScopes", out var scopes));
            Assert.Equal(JsonValueKind.Array, scopes.ValueKind);
            Assert.Equal(2, scopes.GetArrayLength());
        }

        [Fact]
        public void TestPublicFieldJsonExport()
        {
            // RED: Public fields should NOT have scope in JSON
            Schema.RegisterType("ExportTestPublicField", new Dictionary<string, object>
            {
                { "id", new Dictionary<string, object> { { "type", "Int" } } },
                { "name", new Dictionary<string, object> { { "type", "String" } } }
            });

            var json = Schema.ExportTypes(true);
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;

            var idField = root.GetProperty("types")[0].GetProperty("fields")[0];
            Assert.False(idField.TryGetProperty("requiresScope", out _));
            Assert.False(idField.TryGetProperty("requiresScopes", out _));
        }

        // =========================================================================
        // HAPPY PATH: SCOPE WITH OTHER METADATA (3 tests)
        // =========================================================================

        [Fact]
        public void TestScopePreservedWithMetadata()
        {
            // RED: Scope doesn't interfere with type, nullable, description
            Schema.RegisterType("ScopeWithMetadata", new Dictionary<string, object>
            {
                { "salary", new Dictionary<string, object>
                {
                    { "type", "Float" },
                    { "requiresScope", "read:user.salary" },
                    { "description", "User's annual salary" },
                    { "nullable", false }
                } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("ScopeWithMetadata");
            Assert.NotNull(typeInfo);

            var salaryField = typeInfo.Value.Fields["salary"] as Dictionary<string, object>;
            Assert.Equal("Float", salaryField?["type"]);
            Assert.Equal("read:user.salary", salaryField?["requiresScope"]);
            Assert.Equal("User's annual salary", salaryField?["description"]);
            Assert.False((bool?)salaryField?["nullable"] ?? true);
        }

        [Fact]
        public void TestScopeWithNullableField()
        {
            // RED: Scope works on nullable fields
            Schema.RegisterType("ScopeWithNullable", new Dictionary<string, object>
            {
                { "optionalEmail", new Dictionary<string, object>
                {
                    { "type", "String" },
                    { "nullable", true },
                    { "requiresScope", "read:user.email" }
                } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("ScopeWithNullable");
            Assert.NotNull(typeInfo);

            var emailField = typeInfo.Value.Fields["optionalEmail"] as Dictionary<string, object>;
            Assert.True((bool?)emailField?["nullable"] ?? false);
            Assert.Equal("read:user.email", emailField?["requiresScope"]);
        }

        [Fact]
        public void TestMultipleScopedFieldsMetadataIndependence()
        {
            // RED: Each field's metadata is independent
            Schema.RegisterType("MetadataIndependence", new Dictionary<string, object>
            {
                { "field1", new Dictionary<string, object>
                {
                    { "type", "String" },
                    { "requiresScope", "scope1" },
                    { "description", "Desc 1" }
                } },
                { "field2", new Dictionary<string, object>
                {
                    { "type", "String" },
                    { "requiresScope", "scope2" },
                    { "description", "Desc 2" }
                } }
            });

            var typeInfo = SchemaRegistry.Instance.GetType("MetadataIndependence");
            Assert.NotNull(typeInfo);

            var field1 = typeInfo.Value.Fields["field1"] as Dictionary<string, object>;
            var field2 = typeInfo.Value.Fields["field2"] as Dictionary<string, object>;

            Assert.Equal("scope1", field1?["requiresScope"]);
            Assert.Equal("Desc 1", field1?["description"]);
            Assert.Equal("scope2", field2?["requiresScope"]);
            Assert.Equal("Desc 2", field2?["description"]);
        }

        // =========================================================================
        // VALIDATION: ERROR HANDLING (6 tests)
        // =========================================================================

        [Fact]
        public void TestInvalidScopeFormatDetection()
        {
            // RED: Invalid scopes should be detected
            Assert.Throws<InvalidOperationException>(() =>
            {
                Schema.RegisterType("InvalidScopeFormat", new Dictionary<string, object>
                {
                    { "field", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "invalid_scope_no_colon" } } }
                });
            });
        }

        [Fact]
        public void TestEmptyScopeRejection()
        {
            // RED: Empty string scope invalid
            Assert.Throws<InvalidOperationException>(() =>
            {
                Schema.RegisterType("EmptyScope", new Dictionary<string, object>
                {
                    { "field", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "" } } }
                });
            });
        }

        [Fact]
        public void TestEmptyScopesArrayRejection()
        {
            // RED: Empty array not allowed
            Assert.Throws<InvalidOperationException>(() =>
            {
                Schema.RegisterType("EmptyScopesArray", new Dictionary<string, object>
                {
                    { "field", new Dictionary<string, object> { { "type", "String" }, { "requiresScopes", new List<object>() } } }
                });
            });
        }

        [Fact]
        public void TestInvalidActionPrefixValidation()
        {
            // RED: Invalid action prefix format
            Assert.Throws<InvalidOperationException>(() =>
            {
                Schema.RegisterType("InvalidActionWithHyphens", new Dictionary<string, object>
                {
                    { "field", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "invalid-action:resource" } } }
                });
            });
        }

        [Fact]
        public void TestInvalidResourceNameValidation()
        {
            // RED: Invalid resource name format
            Assert.Throws<InvalidOperationException>(() =>
            {
                Schema.RegisterType("InvalidResourceWithHyphens", new Dictionary<string, object>
                {
                    { "field", new Dictionary<string, object> { { "type", "String" }, { "requiresScope", "read:invalid-resource-name" } } }
                });
            });
        }

        [Fact]
        public void TestConflictingBothScopeAndScopes()
        {
            // RED: Can't have both scope and scopes on same field
            Assert.Throws<InvalidOperationException>(() =>
            {
                Schema.RegisterType("ConflictingScopeAndScopes", new Dictionary<string, object>
                {
                    { "field", new Dictionary<string, object>
                    {
                        { "type", "String" },
                        { "requiresScope", "read:user.email" },
                        { "requiresScopes", new List<object> { "admin", "auditor" } }
                    } }
                });
            });
        }
    }
}
