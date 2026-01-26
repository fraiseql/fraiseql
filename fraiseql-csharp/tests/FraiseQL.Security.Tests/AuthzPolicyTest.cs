using Xunit;
using FraiseQL.Security;
using System.Collections.Generic;
using System.Linq;

namespace FraiseQL.Security.Tests
{
    public class AuthzPolicyTest
    {
        [Fact]
        public void ShouldCreateRBACPolicy()
        {
            var config = new AuthzPolicyBuilder("adminOnly")
                .Type(new AuthzPolicyType.Rbac())
                .Rule("hasRole($context, 'admin')")
                .Description("Access restricted to administrators")
                .AuditLogging(true)
                .Build();

            Assert.Equal("adminOnly", config.Name);
            Assert.IsType<AuthzPolicyType.Rbac>(config.Type);
            Assert.Equal("hasRole($context, 'admin')", config.Rule);
            Assert.True(config.AuditLogging);
        }

        [Fact]
        public void ShouldCreateABACPolicy()
        {
            var config = new AuthzPolicyBuilder("secretClearance")
                .Type(new AuthzPolicyType.Abac())
                .Description("Requires top secret clearance")
                .Attributes("clearance_level >= 3", "background_check == true")
                .Build();

            Assert.Equal("secretClearance", config.Name);
            Assert.IsType<AuthzPolicyType.Abac>(config.Type);
            Assert.Equal(2, config.Attributes.Count);
        }

        [Fact]
        public void ShouldCreateCustomPolicy()
        {
            var config = new AuthzPolicyBuilder("customRule")
                .Type(new AuthzPolicyType.Custom())
                .Rule("isOwner($context.userId, $resource.ownerId)")
                .Description("Custom ownership rule")
                .Build();

            Assert.IsType<AuthzPolicyType.Custom>(config.Type);
        }

        [Fact]
        public void ShouldCreateHybridPolicy()
        {
            var config = new AuthzPolicyBuilder("auditAccess")
                .Type(new AuthzPolicyType.Hybrid())
                .Description("Role and attribute-based access")
                .Rule("hasRole($context, 'auditor')")
                .Attributes("audit_enabled == true")
                .Build();

            Assert.IsType<AuthzPolicyType.Hybrid>(config.Type);
            Assert.Equal("hasRole($context, 'auditor')", config.Rule);
        }

        [Fact]
        public void ShouldCreateMultiplePolicies()
        {
            var policy1 = new AuthzPolicyBuilder("policy1")
                .Type(new AuthzPolicyType.Rbac())
                .Build();

            var policy2 = new AuthzPolicyBuilder("policy2")
                .Type(new AuthzPolicyType.Abac())
                .Build();

            var policy3 = new AuthzPolicyBuilder("policy3")
                .Type(new AuthzPolicyType.Custom())
                .Build();

            Assert.Equal("policy1", policy1.Name);
            Assert.Equal("policy2", policy2.Name);
            Assert.Equal("policy3", policy3.Name);
        }

        [Fact]
        public void ShouldCreatePIIAccessPolicy()
        {
            var config = new AuthzPolicyBuilder("piiAccess")
                .Type(new AuthzPolicyType.Rbac())
                .Description("Access to Personally Identifiable Information")
                .Rule("hasRole($context, 'data_manager') OR hasScope($context, 'read:pii')")
                .Build();

            Assert.Equal("piiAccess", config.Name);
        }

        [Fact]
        public void ShouldCreateAdminOnlyPolicy()
        {
            var config = new AuthzPolicyBuilder("adminOnly")
                .Type(new AuthzPolicyType.Rbac())
                .Description("Admin-only access")
                .Rule("hasRole($context, 'admin')")
                .AuditLogging(true)
                .Build();

            Assert.True(config.AuditLogging);
        }

        [Fact]
        public void ShouldCreateRecursivePolicy()
        {
            var config = new AuthzPolicyBuilder("recursiveProtection")
                .Type(new AuthzPolicyType.Custom())
                .Rule("canAccessNested($context)")
                .Recursive(true)
                .Description("Recursively applies to nested types")
                .Build();

            Assert.True(config.Recursive);
        }

        [Fact]
        public void ShouldCreateOperationSpecificPolicy()
        {
            var config = new AuthzPolicyBuilder("readOnly")
                .Type(new AuthzPolicyType.Custom())
                .Rule("hasRole($context, 'viewer')")
                .Operations("read")
                .Description("Policy applies only to read operations")
                .Build();

            Assert.Equal("read", config.Operations);
        }

        [Fact]
        public void ShouldCreateCachedPolicy()
        {
            var config = new AuthzPolicyBuilder("cachedAccess")
                .Type(new AuthzPolicyType.Custom())
                .Rule("hasRole($context, 'viewer')")
                .Cacheable(true)
                .CacheDurationSeconds(3600)
                .Description("Access control with result caching")
                .Build();

            Assert.True(config.Cacheable);
            Assert.Equal(3600, config.CacheDurationSeconds);
        }

        [Fact]
        public void ShouldCreateAuditedPolicy()
        {
            var config = new AuthzPolicyBuilder("auditedAccess")
                .Type(new AuthzPolicyType.Rbac())
                .Rule("hasRole($context, 'auditor')")
                .AuditLogging(true)
                .Description("Access with comprehensive audit logging")
                .Build();

            Assert.True(config.AuditLogging);
        }

        [Fact]
        public void ShouldCreatePolicyWithErrorMessage()
        {
            var config = new AuthzPolicyBuilder("restrictedAccess")
                .Type(new AuthzPolicyType.Rbac())
                .Rule("hasRole($context, 'executive')")
                .ErrorMessage("Only executive level users can access this resource")
                .Build();

            Assert.Equal("Only executive level users can access this resource", config.ErrorMessage);
        }

        [Fact]
        public void ShouldSupportFluentChaining()
        {
            var config = new AuthzPolicyBuilder("complexPolicy")
                .Type(new AuthzPolicyType.Hybrid())
                .Description("Complex hybrid policy")
                .Rule("hasRole($context, 'admin')")
                .Attributes("security_clearance >= 3")
                .Cacheable(true)
                .CacheDurationSeconds(1800)
                .Recursive(false)
                .Operations("create,update,delete")
                .AuditLogging(true)
                .ErrorMessage("Insufficient privileges")
                .Build();

            Assert.Equal("complexPolicy", config.Name);
            Assert.IsType<AuthzPolicyType.Hybrid>(config.Type);
            Assert.True(config.Cacheable);
            Assert.True(config.AuditLogging);
        }

        [Fact]
        public void ShouldCreatePolicyComposition()
        {
            var publicPolicy = new AuthzPolicyBuilder("publicAccess")
                .Type(new AuthzPolicyType.Rbac())
                .Rule("true")
                .Build();

            var piiPolicy = new AuthzPolicyBuilder("piiAccess")
                .Type(new AuthzPolicyType.Rbac())
                .Rule("hasRole($context, 'data_manager')")
                .Build();

            var adminPolicy = new AuthzPolicyBuilder("adminAccess")
                .Type(new AuthzPolicyType.Rbac())
                .Rule("hasRole($context, 'admin')")
                .Build();

            Assert.Equal("publicAccess", publicPolicy.Name);
            Assert.Equal("piiAccess", piiPolicy.Name);
            Assert.Equal("adminAccess", adminPolicy.Name);
        }

        [Fact]
        public void ShouldCreateFinancialDataPolicy()
        {
            var config = new AuthzPolicyBuilder("financialData")
                .Type(new AuthzPolicyType.Abac())
                .Description("Access to financial records")
                .Attributes("clearance_level >= 2", "department == \"finance\"")
                .Build();

            Assert.Equal("financialData", config.Name);
            Assert.Equal(2, config.Attributes.Count);
        }

        [Fact]
        public void ShouldCreateSecurityClearancePolicy()
        {
            var config = new AuthzPolicyBuilder("secretClearance")
                .Type(new AuthzPolicyType.Abac())
                .Attributes("clearance_level >= 3", "background_check == true")
                .Description("Requires top secret clearance")
                .Build();

            Assert.Equal(2, config.Attributes.Count);
        }

        [Fact]
        public void ShouldSupportAnnotationAttribute()
        {
            var type = typeof(AdminPolicy);
            var attr = type.GetCustomAttributes(typeof(AuthzPolicyAttribute), false)
                .FirstOrDefault() as AuthzPolicyAttribute;

            Assert.NotNull(attr);
            Assert.Equal("adminOnly", attr.Name);
        }

        [Fact]
        public void ShouldCreateDefaultConfiguration()
        {
            var config = new AuthzPolicyBuilder("default").Build();

            Assert.Equal("default", config.Name);
            Assert.IsType<AuthzPolicyType.Custom>(config.Type);
            Assert.True(config.Cacheable);
            Assert.Equal(300, config.CacheDurationSeconds);
        }
    }

    [AuthzPolicy(
        Name = "adminOnly",
        Type = "rbac",
        Rule = "hasRole($context, 'admin')"
    )]
    public class AdminPolicy
    {
        public int Id { get; set; }
    }
}
