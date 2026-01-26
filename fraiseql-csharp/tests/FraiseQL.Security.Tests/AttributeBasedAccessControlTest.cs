using Xunit;
using FraiseQL.Security;
using System.Collections.Generic;

namespace FraiseQL.Security.Tests
{
    public class AttributeBasedAccessControlTest
    {
        [Fact]
        public void ShouldCreateABACPolicy()
        {
            var config = new AuthzPolicyBuilder("accessControl")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("clearance_level >= 2")
                .Description("Basic clearance")
                .Build();

            Assert.Equal("accessControl", config.Name);
            Assert.IsType<AuthzPolicyType.Abac>(config.Type);
        }

        [Fact]
        public void ShouldHandleMultipleAttributes()
        {
            var config = new AuthzPolicyBuilder("secretAccess")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("clearance_level >= 3", "background_check == true")
                .Build();

            Assert.Equal(2, config.Attributes.Count);
        }

        [Fact]
        public void ShouldCreateClearanceLevelPolicy()
        {
            var config = new AuthzPolicyBuilder("topSecret")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("clearance_level >= 3")
                .Description("Top secret clearance required")
                .Build();

            Assert.Single(config.Attributes);
        }

        [Fact]
        public void ShouldCreateDepartmentPolicy()
        {
            var config = new AuthzPolicyBuilder("financeDept")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("department == \"finance\"")
                .Description("Finance department only")
                .Build();

            Assert.Equal("financeDept", config.Name);
        }

        [Fact]
        public void ShouldCreateTimeBasedPolicy()
        {
            var config = new AuthzPolicyBuilder("businessHours")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("now >= 9:00 AM", "now <= 5:00 PM")
                .Description("During business hours")
                .Build();

            Assert.Equal(2, config.Attributes.Count);
        }

        [Fact]
        public void ShouldCreateGeographicPolicy()
        {
            var config = new AuthzPolicyBuilder("usOnly")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("country == \"US\"")
                .Description("United States only")
                .Build();

            Assert.Single(config.Attributes);
        }

        [Fact]
        public void ShouldCreateGDPRPolicy()
        {
            var config = new AuthzPolicyBuilder("gdprCompliance")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("gdpr_compliant == true", "data_residency == \"EU\"")
                .Description("GDPR compliance required")
                .Build();

            Assert.Equal(2, config.Attributes.Count);
        }

        [Fact]
        public void ShouldCreateDataClassificationPolicy()
        {
            var config = new AuthzPolicyBuilder("classifiedData")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("classification >= 2")
                .Description("For classified documents")
                .Build();

            Assert.Single(config.Attributes);
        }

        [Fact]
        public void ShouldSupportCachingInABACPolicy()
        {
            var config = new AuthzPolicyBuilder("cachedAccess")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("role == \"viewer\"")
                .Cacheable(true)
                .CacheDurationSeconds(600)
                .Build();

            Assert.True(config.Cacheable);
            Assert.Equal(600, config.CacheDurationSeconds);
        }

        [Fact]
        public void ShouldSupportAuditLoggingInABACPolicy()
        {
            var config = new AuthzPolicyBuilder("auditedAccess")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("audit_enabled == true")
                .AuditLogging(true)
                .Build();

            Assert.True(config.AuditLogging);
        }

        [Fact]
        public void ShouldSupportRecursiveApplicationInABACPolicy()
        {
            var config = new AuthzPolicyBuilder("recursiveAccess")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("permission >= 1")
                .Recursive(true)
                .Description("Applies to nested types")
                .Build();

            Assert.True(config.Recursive);
        }

        [Fact]
        public void ShouldSetOperationSpecificAttributePolicy()
        {
            var config = new AuthzPolicyBuilder("readOnly")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("can_read == true")
                .Operations("read")
                .Build();

            Assert.Equal("read", config.Operations);
        }

        [Fact]
        public void ShouldHandleAttributesArray()
        {
            var attrs = new List<string> { "attr1 >= 1", "attr2 == true" };
            var config = new AuthzPolicyBuilder("arrayTest")
                .Type(new AuthzPolicyType.Abac())
                .AttributesArray(attrs)
                .Build();

            Assert.Equal(2, config.Attributes.Count);
        }

        [Fact]
        public void ShouldCreateComplexABACPolicy()
        {
            var config = new AuthzPolicyBuilder("complex")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("level >= 2", "verified == true", "active == true")
                .Description("Complex attribute rules")
                .AuditLogging(true)
                .Cacheable(true)
                .Build();

            Assert.Equal(3, config.Attributes.Count);
            Assert.True(config.AuditLogging);
        }

        [Fact]
        public void ShouldSetErrorMessageInABACPolicy()
        {
            var config = new AuthzPolicyBuilder("restricted")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("clearance >= 2")
                .ErrorMessage("Insufficient clearance level")
                .Build();

            Assert.Equal("Insufficient clearance level", config.ErrorMessage);
        }
    }
}
