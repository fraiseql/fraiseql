"""Test importing types individually to isolate issues."""

def test_blog_types():
    """Test importing blog_types."""
    try:
        from .types import blog_types
        print("✅ blog_types imported successfully")
        assert True  # Import successful
    except Exception as e:
        print(f"❌ blog_types import failed: {e}")
        assert False, f"blog_types import failed: {e}"

def test_blog_mutations():
    """Test importing blog_mutations."""
    try:
        from .types import blog_mutations
        print("✅ blog_mutations imported successfully")
        assert True  # Import successful
    except Exception as e:
        print(f"❌ blog_mutations import failed: {e}")
        assert False, f"blog_mutations import failed: {e}"

def test_blog_queries():
    """Test importing blog_queries."""
    try:
        from .types import blog_queries
        print("✅ blog_queries imported successfully")
        assert True  # Import successful
    except Exception as e:
        print(f"❌ blog_queries import failed: {e}")
        assert False, f"blog_queries import failed: {e}"

if __name__ == "__main__":
    print("🧪 Testing individual type imports...")
    success_count = 0
    total_count = 3

    try:
        test_blog_types()
        success_count += 1
    except:
        pass

    try:
        test_blog_mutations()
        success_count += 1
    except:
        pass

    try:
        test_blog_queries()
        success_count += 1
    except:
        pass

    print(f"\n📊 Results: {success_count}/{total_count} successful")
