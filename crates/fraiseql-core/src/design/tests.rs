//! Tests for `design/` modules.
#![allow(unused_imports)] // Reason: blanket re-exports for test convenience

pub use serde_json::json;

mod authorization_tests {
    #![allow(clippy::unwrap_used)]
    use super::super::*;
    use crate::design::authorization::analyze;

    #[test]
    fn test_auth_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic
    }
}

mod cache_tests {
    #![allow(clippy::unwrap_used)]
    use super::super::*;
    use crate::design::cache::analyze;

    #[test]
    fn test_cache_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic
    }
}

mod compilation_tests {
    #![allow(clippy::unwrap_used)]
    use super::super::*;
    use crate::design::compilation::analyze;

    #[test]
    fn test_compilation_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
    }

    #[test]
    fn test_circular_types_detection() {
        let schema = serde_json::json!({
            "types": [
                {
                    "name": "User",
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "posts", "type": "[Post]"}
                    ]
                },
                {
                    "name": "Post",
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "author", "type": "User"}
                    ]
                }
            ]
        });

        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);

        assert!(!audit.schema_issues.is_empty());
    }
}

mod cost_tests {
    #![allow(clippy::unwrap_used)]
    use super::super::*;
    use crate::design::cost::analyze;

    #[test]
    fn test_cost_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic
    }
}

mod federation_tests {
    #![allow(clippy::unwrap_used)]
    use super::super::*;
    use crate::design::federation::analyze;

    #[test]
    fn test_federation_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic or error
    }

    #[test]
    fn test_over_federation_detection() {
        let schema = serde_json::json!({
            "subgraphs": [
                {"name": "service-a", "entities": ["User"]},
                {"name": "service-b", "entities": ["User"]},
                {"name": "service-c", "entities": ["User"]},
            ]
        });
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        assert!(!audit.federation_issues.is_empty());
    }
}

mod design_mod_tests {
    #![allow(clippy::unwrap_used)]
    use super::super::*;

    #[test]
    fn test_issue_severity_weight() {
        assert_eq!(IssueSeverity::Critical.weight(), 3);
        assert_eq!(IssueSeverity::Warning.weight(), 2);
        assert_eq!(IssueSeverity::Info.weight(), 1);
    }

    #[test]
    fn test_empty_audit_score() {
        let audit = DesignAudit::new();
        assert_eq!(audit.score(), 100);
    }

    #[test]
    fn test_severity_count_empty() {
        let audit = DesignAudit::new();
        assert_eq!(audit.severity_count(IssueSeverity::Critical), 0);
        assert_eq!(audit.severity_count(IssueSeverity::Warning), 0);
        assert_eq!(audit.severity_count(IssueSeverity::Info), 0);
    }
}

mod schema_patterns_tests {
    #![allow(clippy::unwrap_used)]
    use super::super::*;
    use crate::design::schema_patterns::analyze;

    #[test]
    fn test_schema_analyze_empty_schema() {
        let schema = serde_json::json!({});
        let mut audit = DesignAudit::new();
        analyze(&schema, &mut audit);
        // Should not panic
    }
}
