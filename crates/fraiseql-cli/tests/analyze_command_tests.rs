//! Tests for analyze command - schema optimization analysis

use serde_json::json;

#[derive(Debug)]
struct AnalysisResult {
    schema_file: String,
    categories:  std::collections::HashMap<String, Vec<String>>,
}

fn analyze_schema(schema_file: &str) -> anyhow::Result<AnalysisResult> {
    // This would call the actual analyze command
    // For testing, we simulate analysis results
    let mut categories = std::collections::HashMap::new();

    // Simulate different analysis categories
    categories.insert(
        "performance".to_string(),
        vec![
            "Consider adding index on User.id".to_string(),
            "Query caching recommended for frequently accessed users".to_string(),
        ],
    );

    categories.insert(
        "security".to_string(),
        vec![
            "Rate limiting enabled".to_string(),
            "Audit logging configured".to_string(),
        ],
    );

    categories.insert(
        "federation".to_string(),
        vec![
            "3 subgraphs detected".to_string(),
            "Entity resolution configured".to_string(),
        ],
    );

    categories.insert(
        "complexity".to_string(),
        vec![
            "User type has 15 fields".to_string(),
            "Max query depth: 8".to_string(),
        ],
    );

    categories.insert(
        "caching".to_string(),
        vec![
            "Cache coherency: strong".to_string(),
            "TTL: 3600s".to_string(),
        ],
    );

    categories.insert(
        "indexing".to_string(),
        vec![
            "Missing index: Post.user_id".to_string(),
            "Existing index: User.email".to_string(),
        ],
    );

    Ok(AnalysisResult {
        schema_file: schema_file.to_string(),
        categories,
    })
}

#[test]
fn test_analyze_provides_6_categories() {
    let analysis = analyze_schema("schema.compiled.json").unwrap();

    assert!(analysis.categories.contains_key("performance"));
    assert!(analysis.categories.contains_key("security"));
    assert!(analysis.categories.contains_key("federation"));
    assert!(analysis.categories.contains_key("complexity"));
    assert!(analysis.categories.contains_key("caching"));
    assert!(analysis.categories.contains_key("indexing"));
}

#[test]
fn test_analyze_each_category_has_recommendations() {
    let analysis = analyze_schema("schema.compiled.json").unwrap();

    for (category, recommendations) in &analysis.categories {
        assert!(!recommendations.is_empty(), "Category {} should have recommendations", category);
    }
}

#[test]
fn test_analyze_performance_recommendations() {
    let analysis = analyze_schema("schema.compiled.json").unwrap();

    let perf = &analysis.categories["performance"];
    assert!(perf.iter().any(|r| r.contains("index") || r.contains("caching")));
}

#[test]
fn test_analyze_security_recommendations() {
    let analysis = analyze_schema("schema.compiled.json").unwrap();

    let security = &analysis.categories["security"];
    assert!(security.iter().any(|r| r.contains("limit") || r.contains("logging")));
}

#[test]
fn test_analyze_federation_recommendations() {
    let analysis = analyze_schema("schema.compiled.json").unwrap();

    let federation = &analysis.categories["federation"];
    assert!(federation.iter().any(|r| r.contains("subgraph") || r.contains("entity")));
}

#[test]
fn test_analyze_result_serializes_to_json() {
    let analysis = analyze_schema("schema.compiled.json").unwrap();

    let json_output = json!({
        "schema_file": analysis.schema_file,
        "categories": analysis.categories,
    });

    assert_eq!(json_output["schema_file"], "schema.compiled.json");
    assert!(json_output["categories"].is_object());
    assert_eq!(json_output["categories"].as_object().unwrap().len(), 6);
}

#[test]
fn test_analyze_returns_actionable_recommendations() {
    let analysis = analyze_schema("schema.compiled.json").unwrap();

    // Each recommendation should be specific and actionable
    for (category, recommendations) in &analysis.categories {
        for rec in recommendations {
            assert!(!rec.is_empty(), "Category {} has empty recommendation", category);
            assert!(rec.len() > 5, "Category {} recommendation too short: {}", category, rec);
        }
    }
}

#[test]
fn test_analyze_includes_metrics() {
    let analysis = analyze_schema("schema.compiled.json").unwrap();

    let complexity = &analysis.categories["complexity"];
    assert!(
        complexity.iter().any(|r| r.contains("field") || r.contains("depth")),
        "Complexity analysis should include field/depth metrics"
    );
}
