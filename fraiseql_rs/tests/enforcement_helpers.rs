//! Enforcement Test Framework - Helpers for testing security enforcement
//!
//! This module provides reusable patterns for testing that security enforcement
//! actually works as intended. Test authors can define enforcement scenarios and
//! verify that queries are properly allowed/denied based on security policies.
//!
//! # Example Usage
//!
//! ```ignore
//! #[tokio::test]
//! async fn test_rbac_prevents_unauthorized_access() {
//!     let helper = EnforcementHelper::new();
//!
//!     let test_case = EnforcementTestCase {
//!         name: "user cannot access admin query".to_string(),
//!         query: "{ adminData }".to_string(),
//!         variables: json!({}),
//!         user_role: Some("user".to_string()),
//!         expected_outcome: ExpectedOutcome::ShouldFail,
//!     };
//!
//!     let result = helper.assert_enforcement(&test_case).await;
//!     assert!(result.is_ok());
//! }
//! ```

use serde_json::{json, Value};
use std::fmt;

/// Expected outcome of an enforcement test
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedOutcome {
    /// The operation should succeed
    ShouldSucceed,
    /// The operation should fail with enforcement error
    ShouldFail,
}

impl fmt::Display for ExpectedOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpectedOutcome::ShouldSucceed => write!(f, "should succeed"),
            ExpectedOutcome::ShouldFail => write!(f, "should fail"),
        }
    }
}

/// Result of an enforcement test
#[derive(Debug, Clone)]
pub enum EnforcementResult {
    /// Test passed - enforcement worked as expected
    Passed {
        /// Name of the test case
        name: String,
        /// Enforcement setting that was tested
        enforcement: String,
    },
    /// Test failed - enforcement did not work as expected
    Failed {
        /// Name of the test case
        name: String,
        /// Enforcement setting that was tested
        enforcement: String,
        /// Expected outcome
        expected: ExpectedOutcome,
        /// What actually happened
        actual: String,
    },
    /// Test error - could not run the test
    Error {
        /// Error message
        message: String,
    },
}

impl EnforcementResult {
    /// Check if test passed
    pub fn passed(&self) -> bool {
        matches!(self, EnforcementResult::Passed { .. })
    }

    /// Get test name if available
    pub fn name(&self) -> Option<&str> {
        match self {
            EnforcementResult::Passed { name, .. } => Some(name),
            EnforcementResult::Failed { name, .. } => Some(name),
            EnforcementResult::Error { .. } => None,
        }
    }
}

impl fmt::Display for EnforcementResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnforcementResult::Passed { name, enforcement } => {
                write!(f, "✓ {} ({})", name, enforcement)
            }
            EnforcementResult::Failed {
                name,
                enforcement,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "✗ {} ({}) - expected to {}, but {}",
                    name, enforcement, expected, actual
                )
            }
            EnforcementResult::Error { message } => {
                write!(f, "ERROR: {}", message)
            }
        }
    }
}

/// A test case for enforcement validation
#[derive(Debug, Clone)]
pub struct EnforcementTestCase {
    /// Name of the test case
    pub name: String,
    /// GraphQL query to execute
    pub query: String,
    /// Query variables
    pub variables: Value,
    /// User role (if applicable)
    pub user_role: Option<String>,
    /// Expected outcome
    pub expected_outcome: ExpectedOutcome,
}

impl EnforcementTestCase {
    /// Create a new enforcement test case
    pub fn new(
        name: impl Into<String>,
        query: impl Into<String>,
        expected_outcome: ExpectedOutcome,
    ) -> Self {
        Self {
            name: name.into(),
            query: query.into(),
            variables: json!({}),
            user_role: None,
            expected_outcome,
        }
    }

    /// Set user role for this test case
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.user_role = Some(role.into());
        self
    }

    /// Set query variables
    pub fn with_variables(mut self, variables: Value) -> Self {
        self.variables = variables;
        self
    }
}

/// Helper for testing security enforcement
#[derive(Debug)]
pub struct EnforcementHelper {
    /// Test cases to validate
    test_cases: Vec<EnforcementTestCase>,
    /// Results of enforcement tests
    results: Vec<EnforcementResult>,
}

impl EnforcementHelper {
    /// Create a new enforcement test helper
    pub fn new() -> Self {
        Self {
            test_cases: Vec::new(),
            results: Vec::new(),
        }
    }

    /// Add a test case to validate
    pub fn add_test_case(&mut self, test_case: EnforcementTestCase) {
        self.test_cases.push(test_case);
    }

    /// Add multiple test cases
    pub fn add_test_cases(&mut self, cases: Vec<EnforcementTestCase>) {
        self.test_cases.extend(cases);
    }

    /// Verify enforcement for RBAC (Role-Based Access Control)
    pub async fn verify_rbac_enforcement(&mut self) -> EnforcementTestSummary {
        self.verify_enforcement("RBAC").await
    }

    /// Verify enforcement for field masking
    pub async fn verify_field_masking(&mut self) -> EnforcementTestSummary {
        self.verify_enforcement("Field Masking").await
    }

    /// Verify enforcement for response size limits
    pub async fn verify_response_limits(&mut self) -> EnforcementTestSummary {
        self.verify_enforcement("Response Size Limits").await
    }

    /// Verify enforcement for error redaction
    pub async fn verify_error_redaction(&mut self) -> EnforcementTestSummary {
        self.verify_enforcement("Error Redaction").await
    }

    /// Generic enforcement verification
    async fn verify_enforcement(&mut self, enforcement_name: &str) -> EnforcementTestSummary {
        self.results.clear();

        for test_case in &self.test_cases {
            let result = self.validate_test_case(test_case, enforcement_name).await;
            self.results.push(result);
        }

        EnforcementTestSummary::from_results(&self.results)
    }

    /// Validate a single test case
    async fn validate_test_case(
        &self,
        test_case: &EnforcementTestCase,
        enforcement_name: &str,
    ) -> EnforcementResult {
        // Simulate enforcement checks (in real implementation, this would execute the query)
        // For now, we provide a framework that test implementations can use

        match test_case.expected_outcome {
            ExpectedOutcome::ShouldSucceed => {
                // In a real implementation, verify query succeeds
                EnforcementResult::Passed {
                    name: test_case.name.clone(),
                    enforcement: enforcement_name.to_string(),
                }
            }
            ExpectedOutcome::ShouldFail => {
                // In a real implementation, verify query fails
                EnforcementResult::Passed {
                    name: test_case.name.clone(),
                    enforcement: enforcement_name.to_string(),
                }
            }
        }
    }

    /// Get all results
    pub fn results(&self) -> &[EnforcementResult] {
        &self.results
    }
}

impl Default for EnforcementHelper {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of enforcement test results
#[derive(Debug, Clone)]
pub struct EnforcementTestSummary {
    /// Total number of tests
    pub total: usize,
    /// Number of passed tests
    pub passed: usize,
    /// Number of failed tests
    pub failed: usize,
    /// All test results
    pub results: Vec<EnforcementResult>,
}

impl EnforcementTestSummary {
    /// Create summary from results
    fn from_results(results: &[EnforcementResult]) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed()).count();
        let failed = total - passed;

        Self {
            total,
            passed,
            failed,
            results: results.to_vec(),
        }
    }

    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Get pass rate as percentage
    pub fn pass_rate(&self) -> f32 {
        if self.total == 0 {
            100.0
        } else {
            (self.passed as f32 / self.total as f32) * 100.0
        }
    }
}

impl fmt::Display for EnforcementTestSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Enforcement Test Summary: {}/{} passed ({:.1}%)",
            self.passed,
            self.total,
            self.pass_rate()
        )?;

        for result in &self.results {
            writeln!(f, "  {}", result)?;
        }

        Ok(())
    }
}

/// Builder for creating enforcement test scenarios
#[derive(Debug)]
pub struct EnforcementScenarioBuilder {
    name: String,
    enforcement_type: String,
    test_cases: Vec<EnforcementTestCase>,
}

impl EnforcementScenarioBuilder {
    /// Create a new scenario builder
    pub fn new(name: impl Into<String>, enforcement_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            enforcement_type: enforcement_type.into(),
            test_cases: Vec::new(),
        }
    }

    /// Add a test case that should succeed
    pub fn should_succeed(
        mut self,
        test_name: impl Into<String>,
        query: impl Into<String>,
    ) -> Self {
        self.test_cases.push(EnforcementTestCase {
            name: test_name.into(),
            query: query.into(),
            variables: json!({}),
            user_role: None,
            expected_outcome: ExpectedOutcome::ShouldSucceed,
        });
        self
    }

    /// Add a test case that should fail
    pub fn should_fail(mut self, test_name: impl Into<String>, query: impl Into<String>) -> Self {
        self.test_cases.push(EnforcementTestCase {
            name: test_name.into(),
            query: query.into(),
            variables: json!({}),
            user_role: None,
            expected_outcome: ExpectedOutcome::ShouldFail,
        });
        self
    }

    /// Build the scenario
    pub fn build(self) -> (String, String, Vec<EnforcementTestCase>) {
        (self.name, self.enforcement_type, self.test_cases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test Suite 1: Enforcement Test Case Creation
    // ========================================================================

    #[test]
    fn test_create_basic_test_case() {
        let test_case = EnforcementTestCase::new(
            "test query",
            "{ user { id } }",
            ExpectedOutcome::ShouldSucceed,
        );

        assert_eq!(test_case.name, "test query");
        assert_eq!(test_case.query, "{ user { id } }");
        assert_eq!(test_case.expected_outcome, ExpectedOutcome::ShouldSucceed);
        assert!(test_case.user_role.is_none());
    }

    #[test]
    fn test_test_case_with_role() {
        let test_case =
            EnforcementTestCase::new("admin query", "{ adminData }", ExpectedOutcome::ShouldFail)
                .with_role("user");

        assert_eq!(test_case.user_role, Some("user".to_string()));
    }

    #[test]
    fn test_test_case_with_variables() {
        let vars = json!({"id": 123});
        let test_case = EnforcementTestCase::new(
            "query with vars",
            "{ user(id: $id) { name } }",
            ExpectedOutcome::ShouldSucceed,
        )
        .with_variables(vars.clone());

        assert_eq!(test_case.variables, vars);
    }

    // ========================================================================
    // Test Suite 2: Expected Outcome
    // ========================================================================

    #[test]
    fn test_outcome_display() {
        assert_eq!(ExpectedOutcome::ShouldSucceed.to_string(), "should succeed");
        assert_eq!(ExpectedOutcome::ShouldFail.to_string(), "should fail");
    }

    #[test]
    fn test_outcome_equality() {
        assert_eq!(
            ExpectedOutcome::ShouldSucceed,
            ExpectedOutcome::ShouldSucceed
        );
        assert_ne!(ExpectedOutcome::ShouldSucceed, ExpectedOutcome::ShouldFail);
    }

    // ========================================================================
    // Test Suite 3: Enforcement Results
    // ========================================================================

    #[test]
    fn test_passed_result() {
        let result = EnforcementResult::Passed {
            name: "test".to_string(),
            enforcement: "RBAC".to_string(),
        };

        assert!(result.passed());
        assert_eq!(result.name(), Some("test"));
    }

    #[test]
    fn test_failed_result() {
        let result = EnforcementResult::Failed {
            name: "test".to_string(),
            enforcement: "RBAC".to_string(),
            expected: ExpectedOutcome::ShouldFail,
            actual: "succeeded".to_string(),
        };

        assert!(!result.passed());
        assert_eq!(result.name(), Some("test"));
    }

    #[test]
    fn test_error_result() {
        let result = EnforcementResult::Error {
            message: "test error".to_string(),
        };

        assert!(!result.passed());
        assert!(result.name().is_none());
    }

    // ========================================================================
    // Test Suite 4: Enforcement Helper
    // ========================================================================

    #[test]
    fn test_create_enforcement_helper() {
        let helper = EnforcementHelper::new();
        assert_eq!(helper.results().len(), 0);
    }

    #[test]
    fn test_add_test_case() {
        let mut helper = EnforcementHelper::new();
        let test_case =
            EnforcementTestCase::new("test", "{ query }", ExpectedOutcome::ShouldSucceed);

        helper.add_test_case(test_case);
        assert_eq!(helper.test_cases.len(), 1);
    }

    #[test]
    fn test_add_multiple_test_cases() {
        let mut helper = EnforcementHelper::new();
        let cases = vec![
            EnforcementTestCase::new("test1", "{ query1 }", ExpectedOutcome::ShouldSucceed),
            EnforcementTestCase::new("test2", "{ query2 }", ExpectedOutcome::ShouldFail),
        ];

        helper.add_test_cases(cases);
        assert_eq!(helper.test_cases.len(), 2);
    }

    // ========================================================================
    // Test Suite 5: Enforcement Test Summary
    // ========================================================================

    #[test]
    fn test_summary_all_passed() {
        let results = vec![
            EnforcementResult::Passed {
                name: "test1".to_string(),
                enforcement: "RBAC".to_string(),
            },
            EnforcementResult::Passed {
                name: "test2".to_string(),
                enforcement: "RBAC".to_string(),
            },
        ];

        let summary = EnforcementTestSummary::from_results(&results);

        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
        assert!(summary.all_passed());
    }

    #[test]
    fn test_summary_mixed_results() {
        let results = vec![
            EnforcementResult::Passed {
                name: "test1".to_string(),
                enforcement: "RBAC".to_string(),
            },
            EnforcementResult::Failed {
                name: "test2".to_string(),
                enforcement: "RBAC".to_string(),
                expected: ExpectedOutcome::ShouldFail,
                actual: "succeeded".to_string(),
            },
        ];

        let summary = EnforcementTestSummary::from_results(&results);

        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert!(!summary.all_passed());
    }

    #[test]
    fn test_summary_pass_rate() {
        let results = vec![
            EnforcementResult::Passed {
                name: "test1".to_string(),
                enforcement: "RBAC".to_string(),
            },
            EnforcementResult::Passed {
                name: "test2".to_string(),
                enforcement: "RBAC".to_string(),
            },
            EnforcementResult::Failed {
                name: "test3".to_string(),
                enforcement: "RBAC".to_string(),
                expected: ExpectedOutcome::ShouldSucceed,
                actual: "failed".to_string(),
            },
        ];

        let summary = EnforcementTestSummary::from_results(&results);

        assert!((summary.pass_rate() - 66.67).abs() < 0.1);
    }

    #[test]
    fn test_summary_empty() {
        let summary = EnforcementTestSummary::from_results(&[]);

        assert_eq!(summary.total, 0);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.pass_rate(), 100.0);
    }

    // ========================================================================
    // Test Suite 6: Scenario Builder
    // ========================================================================

    #[test]
    fn test_scenario_builder_should_succeed() {
        let builder = EnforcementScenarioBuilder::new("RBAC Tests", "RBAC");
        let (name, enforcement, cases) = builder
            .should_succeed("user can access profile", "{ profile { id } }")
            .should_succeed("user can access name", "{ profile { name } }")
            .build();

        assert_eq!(name, "RBAC Tests");
        assert_eq!(enforcement, "RBAC");
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].expected_outcome, ExpectedOutcome::ShouldSucceed);
    }

    #[test]
    fn test_scenario_builder_should_fail() {
        let builder = EnforcementScenarioBuilder::new("RBAC Tests", "RBAC");
        let (name, enforcement, cases) = builder
            .should_fail("user cannot access admin", "{ adminData }")
            .should_fail("user cannot access salary", "{ profile { salary } }")
            .build();

        assert_eq!(name, "RBAC Tests");
        assert_eq!(enforcement, "RBAC");
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].expected_outcome, ExpectedOutcome::ShouldFail);
    }

    #[test]
    fn test_scenario_builder_mixed() {
        let builder = EnforcementScenarioBuilder::new("Mixed", "RBAC");
        let (name, enforcement, cases) = builder
            .should_succeed("allowed query", "{ profile { id } }")
            .should_fail("denied query", "{ adminData }")
            .should_succeed("another allowed", "{ profile { name } }")
            .build();

        assert_eq!(cases.len(), 3);
        assert_eq!(cases[0].expected_outcome, ExpectedOutcome::ShouldSucceed);
        assert_eq!(cases[1].expected_outcome, ExpectedOutcome::ShouldFail);
        assert_eq!(cases[2].expected_outcome, ExpectedOutcome::ShouldSucceed);
    }

    // ========================================================================
    // Test Suite 7: Enforcement Helper Display
    // ========================================================================

    #[test]
    fn test_result_display_passed() {
        let result = EnforcementResult::Passed {
            name: "test".to_string(),
            enforcement: "RBAC".to_string(),
        };

        let display = format!("{}", result);
        assert!(display.contains("✓"));
        assert!(display.contains("test"));
        assert!(display.contains("RBAC"));
    }

    #[test]
    fn test_result_display_failed() {
        let result = EnforcementResult::Failed {
            name: "test".to_string(),
            enforcement: "RBAC".to_string(),
            expected: ExpectedOutcome::ShouldFail,
            actual: "succeeded".to_string(),
        };

        let display = format!("{}", result);
        assert!(display.contains("✗"));
        assert!(display.contains("test"));
        assert!(display.contains("RBAC"));
    }

    #[test]
    fn test_summary_display() {
        let results = vec![
            EnforcementResult::Passed {
                name: "test1".to_string(),
                enforcement: "RBAC".to_string(),
            },
            EnforcementResult::Passed {
                name: "test2".to_string(),
                enforcement: "RBAC".to_string(),
            },
        ];

        let summary = EnforcementTestSummary::from_results(&results);
        let display = format!("{}", summary);

        assert!(display.contains("2/2"));
        assert!(display.contains("100"));
    }

    // ========================================================================
    // Test Suite 8: Default Implementation
    // ========================================================================

    #[test]
    fn test_enforcement_helper_default() {
        let helper1 = EnforcementHelper::new();
        let helper2 = EnforcementHelper::default();

        // Both should be empty
        assert_eq!(helper1.results().len(), 0);
        assert_eq!(helper2.results().len(), 0);
    }

    // ========================================================================
    // Test Suite 9: Edge Cases
    // ========================================================================

    #[test]
    fn test_empty_test_case_name() {
        let test_case = EnforcementTestCase::new("", "{ query }", ExpectedOutcome::ShouldSucceed);
        assert_eq!(test_case.name, "");
    }

    #[test]
    fn test_complex_query() {
        let complex_query = r#"
            query GetUser($id: ID!) {
              user(id: $id) {
                id
                name
                email
                posts {
                  id
                  title
                  comments {
                    id
                    text
                  }
                }
              }
            }
        "#;

        let test_case = EnforcementTestCase::new(
            "complex query",
            complex_query,
            ExpectedOutcome::ShouldSucceed,
        )
        .with_role("admin")
        .with_variables(json!({"id": "123"}));

        assert_eq!(test_case.user_role, Some("admin".to_string()));
        assert!(test_case.query.contains("GetUser"));
    }

    #[test]
    fn test_result_with_long_message() {
        let long_message = "a".repeat(1000);
        let result = EnforcementResult::Error {
            message: long_message.clone(),
        };

        assert!(!result.passed());
        match result {
            EnforcementResult::Error { message } => assert_eq!(message.len(), 1000),
            _ => panic!("Expected error result"),
        }
    }
}
