import pytest

"""Integration test for blog demo components."""

import sys
import traceback



@pytest.mark.blog_demo
def test_imports():
    """Test that all blog components can be imported successfully."""
    print("🧪 Testing blog demo imports...")

    try:
        # Test types imports
        from .types import blog_types, blog_mutations, blog_queries
        print("✅ Types imported successfully")

        # Test app imports
        from . import app
        print("✅ App imported successfully")

        # Test specific classes
        post_class = blog_types.Post
        author_class = blog_types.Author
        create_post_mutation = blog_mutations.CreatePost

        print("✅ Core classes accessible")

        # Test decorators are applied
        if hasattr(post_class, '__fraiseql_type__'):
            print("✅ FraiseQL decorators applied to types")
        else:
            print("⚠️  FraiseQL decorators may not be applied")

        print("\n🎉 All imports successful!")
        # Test passes if no exception is raised

    except Exception as e:
        print(f"❌ Import error: {e}")
        print("\nFull traceback:")
        traceback.print_exc()
        pytest.fail(f"Import test failed: {e}")


def test_app_creation():
    """Test that the FastAPI app can be created."""
    print("\n🧪 Testing app creation...")

    try:
        from .app import create_app

        # This should work without database connection
        app_instance = create_app()
        print("✅ FastAPI app created successfully")

        # Check if GraphQL endpoint is mounted
        if any('/graphql' in str(route.path) for route in app_instance.routes):
            print("✅ GraphQL endpoint mounted")
        else:
            print("⚠️  GraphQL endpoint may not be mounted")

        # Test passes if no exception is raised

    except Exception as e:
        print(f"❌ App creation error: {e}")
        print("\nFull traceback:")
        traceback.print_exc()
        pytest.fail(f"App creation test failed: {e}")


def main():
    """Run all integration tests."""
    print("🚀 FraiseQL Blog Demo - Integration Tests\n")

    results = []
    results.append(test_imports())
    results.append(test_app_creation())

    print(f"\n📊 Test Results: {sum(results)}/{len(results)} passed")

    if all(results):
        print("🎉 All integration tests passed!")
        return 0
    else:
        print("❌ Some tests failed")
        return 1


if __name__ == "__main__":
    sys.exit(main())
