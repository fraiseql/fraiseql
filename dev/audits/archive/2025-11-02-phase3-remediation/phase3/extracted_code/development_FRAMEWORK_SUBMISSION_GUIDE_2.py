# Extracted from: docs/development/FRAMEWORK_SUBMISSION_GUIDE.md
# Block number: 2
def test_n_plus_one_prevention():
    query = """
    query {
        users(limit: 10) {
            id
            name
            posts {
                id
                title
            }
        }
    }
    """
    # Enable query logging
    response = execute_query(query)

    # Should execute exactly 2 queries:
    # 1. SELECT users
    # 2. SELECT posts WHERE author_id IN (...)
    assert query_count == 2  # Not 11 queries (1 + 10)
