//! Test binary for full pipeline validation

use fraiseql_rs::pipeline::builder::build_graphql_response;

fn main() {
    println!("🔧 Testing Full Pipeline - Phase 1 Validation");
    println!("=============================================");
    println!();

    // Test data - single user
    let json_rows = vec![r#"{"id":123,"first_name":"John","last_name":"Doe","email":"john@example.com","is_active":true}"#.to_string()];

    println!("📝 Input JSON rows: {json_rows:?}");
    println!();

    // Test the full pipeline
    match build_graphql_response(
        &json_rows,
        "users",
        Some("User"),
        None, // No field projection
        None, // No field selections
        None, // Use default is_list
        None, // Use default include_graphql_wrapper
    ) {
        Ok(bytes) => {
            let result_str = String::from_utf8_lossy(&bytes);
            println!("✅ Pipeline transformation successful!");
            println!("📤 Output: {result_str}");
            println!("📏 Output size: {} bytes", bytes.len());

            // Try to parse as JSON to validate structure
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(parsed) => {
                    println!("✅ Valid JSON structure!");
                    match serde_json::to_string_pretty(&parsed) {
                        Ok(pretty) => {
                            println!("📊 Structure: {pretty}");
                        }
                        Err(e) => {
                            println!("❌ Failed to format JSON: {e:?}");
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Invalid JSON: {e:?}");
                }
            }
        }
        Err(e) => {
            println!("❌ Pipeline failed: {e:?}");
        }
    }

    println!();
    println!("🎉 Phase 1 Pipeline Test Complete!");
}
