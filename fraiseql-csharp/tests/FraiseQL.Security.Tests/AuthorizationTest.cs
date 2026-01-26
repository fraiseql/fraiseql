using Xunit;
using FraiseQL.Security;
using System.Collections.Generic;

namespace FraiseQL.Security.Tests
{
    public class AuthorizationTest
    {
        [Fact]
        public void ShouldCreateSimpleAuthorizationRule()
        {
            var config = new AuthorizeBuilder()
                .Rule("isOwner($context.userId, $field.ownerId)")
                .Description("Ownership check")
                .Build();

            Assert.Equal("isOwner($context.userId, $field.ownerId)", config.Rule);
            Assert.Equal("Ownership check", config.Description);
        }

        [Fact]
        public void ShouldCreateAuthorizationWithPolicy()
        {
            var config = new AuthorizeBuilder()
                .Policy("ownerOnly")
                .Description("References named policy")
                .Build();

            Assert.Equal("ownerOnly", config.Policy);
        }

        [Fact]
        public void ShouldSupportFluentChaining()
        {
            var config = new AuthorizeBuilder()
                .Rule("hasPermission($context)")
                .Description("Complex rule")
                .ErrorMessage("Access denied")
                .Recursive(true)
                .Operations("read")
                .Build();

            Assert.Equal("hasPermission($context)", config.Rule);
            Assert.True(config.Recursive);
            Assert.Equal("read", config.Operations);
        }

        [Fact]
        public void ShouldSetCachingConfiguration()
        {
            var config = new AuthorizeBuilder()
                .Rule("checkAccess($context)")
                .Cacheable(true)
                .CacheDurationSeconds(600)
                .Build();

            Assert.True(config.Cacheable);
            Assert.Equal(600, config.CacheDurationSeconds);
        }

        [Fact]
        public void ShouldSetErrorMessage()
        {
            var config = new AuthorizeBuilder()
                .Rule("adminOnly($context)")
                .ErrorMessage("Only administrators can access this")
                .Build();

            Assert.Equal("Only administrators can access this", config.ErrorMessage);
        }

        [Fact]
        public void ShouldSetRecursiveApplication()
        {
            var config = new AuthorizeBuilder()
                .Rule("checkNested($context)")
                .Recursive(true)
                .Description("Applied to nested types")
                .Build();

            Assert.True(config.Recursive);
        }

        [Fact]
        public void ShouldSetOperationSpecificRule()
        {
            var config = new AuthorizeBuilder()
                .Rule("canDelete($context)")
                .Operations("delete")
                .Description("Only applies to delete operations")
                .Build();

            Assert.Equal("delete", config.Operations);
        }

        [Fact]
        public void ShouldConvertToDict()
        {
            var config = new AuthorizeBuilder()
                .Rule("testRule")
                .Description("Test")
                .Build();

            var dict = config.ToDict();

            Assert.NotNull(dict);
            Assert.Equal("testRule", dict["rule"]);
            Assert.Equal("Test", dict["description"]);
        }

        [Fact]
        public void ShouldUseAnnotationAttribute()
        {
            var type = typeof(ProtectedResource);
            var attr = type.GetCustomAttributes(typeof(AuthorizeAttribute), false)
                .FirstOrDefault() as AuthorizeAttribute;

            Assert.NotNull(attr);
            Assert.Equal("isOwner($context.userId, $resource.ownerId)", attr.Rule);
        }

        [Fact]
        public void ShouldCreateMultipleConfigurations()
        {
            var config1 = new AuthorizeBuilder()
                .Rule("rule1")
                .Build();

            var config2 = new AuthorizeBuilder()
                .Rule("rule2")
                .Build();

            Assert.NotEqual(config1.Rule, config2.Rule);
        }

        [Fact]
        public void ShouldReturnDefaultCacheSettings()
        {
            var config = new AuthorizeBuilder()
                .Rule("test")
                .Build();

            Assert.True(config.Cacheable);
            Assert.Equal(300, config.CacheDurationSeconds);
        }
    }

    [Authorize(Rule = "isOwner($context.userId, $resource.ownerId)")]
    public class ProtectedResource
    {
        public int Id { get; set; }
        public string Content { get; set; } = "";
    }
}
