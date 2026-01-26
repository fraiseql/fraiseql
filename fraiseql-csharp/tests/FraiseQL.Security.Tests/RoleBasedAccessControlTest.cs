using Xunit;
using FraiseQL.Security;
using System.Collections.Generic;
using System.Linq;

namespace FraiseQL.Security.Tests
{
    public class RoleBasedAccessControlTest
    {
        [Fact]
        public void ShouldCreateSingleRoleRequirement()
        {
            var config = new RoleRequiredBuilder()
                .Roles("admin")
                .Build();

            Assert.Single(config.Roles);
            Assert.Equal("admin", config.Roles[0]);
        }

        [Fact]
        public void ShouldCreateMultipleRoleRequirements()
        {
            var config = new RoleRequiredBuilder()
                .Roles("manager", "director")
                .Build();

            Assert.Equal(2, config.Roles.Count);
            Assert.Contains("manager", config.Roles);
            Assert.Contains("director", config.Roles);
        }

        [Fact]
        public void ShouldUseAnyRoleMatchingStrategy()
        {
            var config = new RoleRequiredBuilder()
                .Roles("viewer", "editor")
                .Strategy(new RoleMatchStrategy.Any())
                .Description("At least one role")
                .Build();

            Assert.IsType<RoleMatchStrategy.Any>(config.Strategy);
            Assert.Equal("any", config.Strategy.Value);
        }

        [Fact]
        public void ShouldUseAllRoleMatchingStrategy()
        {
            var config = new RoleRequiredBuilder()
                .Roles("admin", "auditor")
                .Strategy(new RoleMatchStrategy.All())
                .Description("All roles required")
                .Build();

            Assert.IsType<RoleMatchStrategy.All>(config.Strategy);
            Assert.Equal("all", config.Strategy.Value);
        }

        [Fact]
        public void ShouldUseExactlyRoleMatchingStrategy()
        {
            var config = new RoleRequiredBuilder()
                .Roles("exact_role")
                .Strategy(new RoleMatchStrategy.Exactly())
                .Description("Exactly these roles")
                .Build();

            Assert.IsType<RoleMatchStrategy.Exactly>(config.Strategy);
            Assert.Equal("exactly", config.Strategy.Value);
        }

        [Fact]
        public void ShouldSupportRoleHierarchy()
        {
            var config = new RoleRequiredBuilder()
                .Roles("admin")
                .Hierarchy(true)
                .Description("With hierarchy")
                .Build();

            Assert.True(config.Hierarchy);
        }

        [Fact]
        public void ShouldSupportRoleInheritance()
        {
            var config = new RoleRequiredBuilder()
                .Roles("editor")
                .Inherit(true)
                .Description("Inherits from parent")
                .Build();

            Assert.True(config.Inherit);
        }

        [Fact]
        public void ShouldSetOperationSpecificRoles()
        {
            var config = new RoleRequiredBuilder()
                .Roles("editor")
                .Operations("create,update")
                .Description("Only for edit operations")
                .Build();

            Assert.Equal("create,update", config.Operations);
        }

        [Fact]
        public void ShouldSetCustomErrorMessage()
        {
            var config = new RoleRequiredBuilder()
                .Roles("admin")
                .ErrorMessage("Administrator access required")
                .Build();

            Assert.Equal("Administrator access required", config.ErrorMessage);
        }

        [Fact]
        public void ShouldConfigureCaching()
        {
            var config = new RoleRequiredBuilder()
                .Roles("viewer")
                .Cacheable(true)
                .CacheDurationSeconds(1800)
                .Build();

            Assert.True(config.Cacheable);
            Assert.Equal(1800, config.CacheDurationSeconds);
        }

        [Fact]
        public void ShouldCreateAdminPattern()
        {
            var config = new RoleRequiredBuilder()
                .Roles("admin")
                .Strategy(new RoleMatchStrategy.Any())
                .Description("Admin access")
                .Build();

            Assert.Single(config.Roles);
            Assert.Equal("admin", config.Roles[0]);
        }

        [Fact]
        public void ShouldCreateManagerDirectorPattern()
        {
            var config = new RoleRequiredBuilder()
                .Roles("manager", "director")
                .Strategy(new RoleMatchStrategy.Any())
                .Description("Managers and directors")
                .Build();

            Assert.Equal(2, config.Roles.Count);
            Assert.Equal("any", config.Strategy.Value);
        }

        [Fact]
        public void ShouldCreateDataScientistPattern()
        {
            var config = new RoleRequiredBuilder()
                .Roles("data_scientist", "analyst")
                .Strategy(new RoleMatchStrategy.Any())
                .Description("Data professionals")
                .Build();

            Assert.Equal(2, config.Roles.Count);
        }

        [Fact]
        public void ShouldConvertToDict()
        {
            var config = new RoleRequiredBuilder()
                .Roles("admin", "editor")
                .Strategy(new RoleMatchStrategy.Any())
                .Build();

            var dict = config.ToDict();

            Assert.NotNull(dict);
            Assert.NotNull(dict["roles"]);
            Assert.Equal("any", dict["strategy"]);
        }

        [Fact]
        public void ShouldUseAnnotationAttribute()
        {
            var type = typeof(SalaryData);
            var attr = type.GetCustomAttributes(typeof(RoleRequiredAttribute), false)
                .FirstOrDefault() as RoleRequiredAttribute;

            Assert.NotNull(attr);
            Assert.Contains("manager", attr.Roles);
            Assert.Contains("director", attr.Roles);
        }

        [Fact]
        public void ShouldSetDescription()
        {
            var config = new RoleRequiredBuilder()
                .Roles("viewer")
                .Description("Read-only access")
                .Build();

            Assert.Equal("Read-only access", config.Description);
        }

        [Fact]
        public void ShouldReturnDefaultValues()
        {
            var config = new RoleRequiredBuilder()
                .Roles("user")
                .Build();

            Assert.False(config.Hierarchy);
            Assert.False(config.Inherit);
            Assert.True(config.Cacheable);
            Assert.Equal(300, config.CacheDurationSeconds);
        }
    }

    [RoleRequired(Roles = new[] { "manager", "director" }, Strategy = "any")]
    public class SalaryData
    {
        public string EmployeeId { get; set; } = "";
        public double Salary { get; set; }
    }
}
