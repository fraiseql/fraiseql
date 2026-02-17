//! Database fixture and test helper utilities
//!
//! Provides utilities for setting up test databases and executing GraphQL queries.

/// Test database connection configuration
#[derive(Debug, Clone)]
pub struct DatabaseFixture {
    /// PostgreSQL connection string for test database
    pub postgres_url:  String,
    /// Whether to clean up tables after tests
    pub cleanup_after: bool,
}

#[allow(dead_code)]
impl DatabaseFixture {
    /// Create a new test database fixture
    pub fn new() -> Self {
        Self {
            postgres_url:  std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
                "postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql"
                    .to_string()
            }),
            cleanup_after: true,
        }
    }

    /// Create fixture with custom database URL
    pub fn with_url(url: &str) -> Self {
        Self {
            postgres_url:  url.to_string(),
            cleanup_after: true,
        }
    }

    /// Disable cleanup after test
    pub fn no_cleanup(mut self) -> Self {
        self.cleanup_after = false;
        self
    }

    /// Wait for database to become available
    pub async fn wait_for_ready(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut retries = 0;
        const MAX_RETRIES: u32 = 30;

        loop {
            if self.check_connection().await.is_ok() {
                return Ok(());
            }

            retries += 1;
            if retries >= MAX_RETRIES {
                return Err("Database failed to become ready after 30 seconds".into());
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    /// Check if database connection is available
    async fn check_connection(&self) -> Result<(), Box<dyn std::error::Error>> {
        // This would check if the database is available
        // For now, this is a placeholder - actual implementation depends on the database driver
        Ok(())
    }

    /// Get connection URL
    pub fn connection_url(&self) -> &str {
        &self.postgres_url
    }
}

impl Default for DatabaseFixture {
    fn default() -> Self {
        Self::new()
    }
}

/// GraphQL execution result
#[derive(Debug, Clone)]
pub struct GraphQLResult {
    /// Response data as JSON string
    pub data:   Option<String>,
    /// Error messages if any
    pub errors: Vec<String>,
    /// HTTP status code
    pub status: u16,
}

impl GraphQLResult {
    /// Create success result
    pub fn success(data: &str, status: u16) -> Self {
        Self {
            data: Some(data.to_string()),
            errors: vec![],
            status,
        }
    }

    /// Create error result
    pub fn error(message: &str, status: u16) -> Self {
        Self {
            data: None,
            errors: vec![message.to_string()],
            status,
        }
    }

    /// Check if result is successful (200-299 status code)
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if result has data
    pub fn has_data(&self) -> bool {
        self.data.is_some() && self.errors.is_empty()
    }

    /// Check if result has errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Test user fixture data
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserFixture {
    pub id:     String,
    pub name:   String,
    pub email:  String,
    pub active: bool,
}

impl UserFixture {
    /// Create test user
    pub fn new(id: &str, name: &str, email: &str) -> Self {
        Self {
            id:     id.to_string(),
            name:   name.to_string(),
            email:  email.to_string(),
            active: true,
        }
    }
}

/// Test post fixture data
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PostFixture {
    pub id:        String,
    pub title:     String,
    pub author_id: String,
    pub published: bool,
}

impl PostFixture {
    /// Create test post
    pub fn new(id: &str, title: &str, author_id: &str) -> Self {
        Self {
            id:        id.to_string(),
            title:     title.to_string(),
            author_id: author_id.to_string(),
            published: false,
        }
    }

    /// Mark post as published
    pub fn published(mut self) -> Self {
        self.published = true;
        self
    }
}

/// Test data builders for common scenarios
pub struct TestDataBuilder;

impl TestDataBuilder {
    /// Create standard test users
    pub fn users() -> Vec<UserFixture> {
        vec![
            UserFixture::new("user-1", "Alice Smith", "alice@example.com"),
            UserFixture::new("user-2", "Bob Jones", "bob@example.com"),
            UserFixture::new("user-3", "Charlie Brown", "charlie@example.com"),
        ]
    }

    /// Create standard test posts
    pub fn posts() -> Vec<PostFixture> {
        vec![
            PostFixture::new("post-1", "Introduction to GraphQL", "user-1"),
            PostFixture::new("post-2", "Rust Performance", "user-1").published(),
            PostFixture::new("post-3", "Database Design", "user-2").published(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_fixture_creation() {
        let fixture = DatabaseFixture::new();
        assert!(!fixture.postgres_url.is_empty());
        assert!(fixture.cleanup_after);
    }

    #[test]
    fn test_graphql_result_success() {
        let result = GraphQLResult::success(r#"{"data": "test"}"#, 200);
        assert!(result.is_success());
        assert!(result.has_data());
        assert!(!result.has_errors());
    }

    #[test]
    fn test_graphql_result_error() {
        let result = GraphQLResult::error("Test error", 400);
        assert!(!result.is_success());
        assert!(!result.has_data());
        assert!(result.has_errors());
    }

    #[test]
    fn test_user_fixture_creation() {
        let user = UserFixture::new("test-1", "Test User", "test@example.com");
        assert_eq!(user.id, "test-1");
        assert_eq!(user.name, "Test User");
        assert!(user.active);
    }

    #[test]
    fn test_post_fixture_creation() {
        let post = PostFixture::new("post-1", "Test Post", "user-1");
        assert_eq!(post.id, "post-1");
        assert!(!post.published);

        let published = post.published();
        assert!(published.published);
    }

    #[test]
    fn test_standard_test_data() {
        let users = TestDataBuilder::users();
        assert_eq!(users.len(), 3);

        let posts = TestDataBuilder::posts();
        assert_eq!(posts.len(), 3);
    }
}
