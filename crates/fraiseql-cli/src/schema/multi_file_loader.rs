//! Multi-file schema loader - loads and merges JSON schema files from directories
//!
//! Supports flexible schema composition from single files to deeply nested directory structures:
//! - Load all *.json files from a directory recursively
//! - Merge types, queries, mutations arrays
//! - Deduplicate by name with error reporting
//! - Preserve file path information for error messages

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Loads and merges JSON schema files from directories
pub struct MultiFileLoader;

/// Result of loading files
pub struct LoadResult {
    /// Merged JSON value with types, queries, mutations arrays
    pub merged: Value,
}

impl MultiFileLoader {
    /// Load and merge all JSON files from a directory recursively
    ///
    /// # Arguments
    /// * `dir_path` - Path to directory containing *.json files
    ///
    /// # Returns
    /// Merged Value with "types", "queries", "mutations" as arrays
    ///
    /// # Errors
    /// - If directory doesn't exist
    /// - If JSON parsing fails
    /// - If duplicate names are found (with file paths)
    ///
    /// # Example
    /// ```ignore
    /// let merged = MultiFileLoader::load_from_directory("schema/")?;
    /// ```
    pub fn load_from_directory(dir_path: &str) -> Result<Value> {
        let result = Self::load_from_directory_with_tracking(dir_path)?;
        Ok(result.merged)
    }

    /// Load from directory with file path tracking for conflict detection
    pub fn load_from_directory_with_tracking(dir_path: &str) -> Result<LoadResult> {
        let dir = Path::new(dir_path);
        if !dir.is_dir() {
            bail!("Schema directory not found: {dir_path}");
        }

        let mut types = Vec::new();
        let mut queries = Vec::new();
        let mut mutations = Vec::new();
        let mut name_to_file = HashMap::new();

        // Collect all JSON files and sort for deterministic ordering
        let mut json_files = Vec::new();
        for entry in WalkDir::new(dir_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
        {
            json_files.push(entry.path().to_path_buf());
        }

        json_files.sort();

        // Load and merge each file
        for file_path in json_files {
            let content = fs::read_to_string(&file_path)
                .context(format!("Failed to read {}", file_path.display()))?;
            let value: Value = serde_json::from_str(&content)
                .context(format!("Failed to parse JSON from {}", file_path.display()))?;

            // Track source for each item
            let file_path_str = file_path.to_string_lossy().to_string();

            // Merge types
            if let Some(Value::Array(type_items)) = value.get("types") {
                for item in type_items {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        let type_key = format!("type:{name}");
                        if let Some(existing) = name_to_file.get(&type_key) {
                            bail!(
                                "Duplicate type '{}' found in:\n  - {}\n  - {}",
                                name,
                                existing,
                                file_path_str
                            );
                        }
                        name_to_file.insert(type_key, file_path_str.clone());
                    }
                    types.push(item.clone());
                }
            }

            // Merge queries
            if let Some(Value::Array(query_items)) = value.get("queries") {
                for item in query_items {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        let query_key = format!("query:{name}");
                        if let Some(existing) = name_to_file.get(&query_key) {
                            bail!(
                                "Duplicate query '{}' found in:\n  - {}\n  - {}",
                                name,
                                existing,
                                file_path_str
                            );
                        }
                        name_to_file.insert(query_key, file_path_str.clone());
                    }
                    queries.push(item.clone());
                }
            }

            // Merge mutations
            if let Some(Value::Array(mutation_items)) = value.get("mutations") {
                for item in mutation_items {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        let mutation_key = format!("mutation:{name}");
                        if let Some(existing) = name_to_file.get(&mutation_key) {
                            bail!(
                                "Duplicate mutation '{}' found in:\n  - {}\n  - {}",
                                name,
                                existing,
                                file_path_str
                            );
                        }
                        name_to_file.insert(mutation_key, file_path_str.clone());
                    }
                    mutations.push(item.clone());
                }
            }
        }

        let merged = json!({
            "types": types,
            "queries": queries,
            "mutations": mutations,
        });

        Ok(LoadResult { merged })
    }

    /// Load specific files and merge them
    ///
    /// # Arguments
    /// * `paths` - Vector of file paths to load
    ///
    /// # Returns
    /// Merged Value with "types", "queries", "mutations" as arrays
    pub fn load_from_paths(paths: &[PathBuf]) -> Result<Value> {
        let mut types = Vec::new();
        let mut queries = Vec::new();
        let mut mutations = Vec::new();

        for path in paths {
            if !path.exists() {
                bail!("File not found: {}", path.display());
            }

            let content = fs::read_to_string(path)
                .context(format!("Failed to read {}", path.display()))?;
            let value: Value = serde_json::from_str(&content)
                .context(format!("Failed to parse JSON from {}", path.display()))?;

            // Merge types
            if let Some(Value::Array(type_items)) = value.get("types") {
                types.extend(type_items.clone());
            }

            // Merge queries
            if let Some(Value::Array(query_items)) = value.get("queries") {
                queries.extend(query_items.clone());
            }

            // Merge mutations
            if let Some(Value::Array(mutation_items)) = value.get("mutations") {
                mutations.extend(mutation_items.clone());
            }
        }

        Ok(json!({
            "types": types,
            "queries": queries,
            "mutations": mutations,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &str) -> Result<()> {
        let path = dir.join(name);
        fs::write(path, content)?;
        Ok(())
    }

    #[test]
    fn test_load_single_type_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let schema = json!({
            "types": [
                {"name": "User", "fields": []}
            ],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "types.json", &schema.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 1);
        assert_eq!(result["types"][0]["name"], "User");
        assert_eq!(result["queries"].as_array().unwrap().len(), 0);
        assert_eq!(result["mutations"].as_array().unwrap().len(), 0);

        Ok(())
    }

    #[test]
    fn test_merge_multiple_type_files() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let user_schema = json!({
            "types": [
                {"name": "User", "fields": []}
            ],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "user.json", &user_schema.to_string())?;

        let post_schema = json!({
            "types": [
                {"name": "Post", "fields": []}
            ],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "post.json", &post_schema.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 2);
        let type_names: Vec<&str> = result["types"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(type_names.contains(&"User"));
        assert!(type_names.contains(&"Post"));

        Ok(())
    }

    #[test]
    fn test_merge_respects_alphabetical_order() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let c_schema = json!({
            "types": [{"name": "C", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "c.json", &c_schema.to_string())?;

        let a_schema = json!({
            "types": [{"name": "A", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "a.json", &a_schema.to_string())?;

        let b_schema = json!({
            "types": [{"name": "B", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "b.json", &b_schema.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        let type_names: Vec<&str> = result["types"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();

        // Should be ordered by file load order (a.json, b.json, c.json alphabetically)
        assert_eq!(type_names[0], "A");
        assert_eq!(type_names[1], "B");
        assert_eq!(type_names[2], "C");

        Ok(())
    }

    #[test]
    fn test_merge_queries_and_mutations() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let schema = json!({
            "types": [
                {"name": "User", "fields": []}
            ],
            "queries": [
                {"name": "getUser", "return_type": "User"}
            ],
            "mutations": [
                {"name": "createUser", "return_type": "User"}
            ]
        });
        create_test_file(temp_dir.path(), "schema.json", &schema.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 1);
        assert_eq!(result["queries"].as_array().unwrap().len(), 1);
        assert_eq!(result["queries"][0]["name"], "getUser");
        assert_eq!(result["mutations"].as_array().unwrap().len(), 1);
        assert_eq!(result["mutations"][0]["name"], "createUser");

        Ok(())
    }

    #[test]
    fn test_nested_directory_structure() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create nested structure
        fs::create_dir_all(temp_dir.path().join("types"))?;
        fs::create_dir_all(temp_dir.path().join("queries"))?;

        let user_type = json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(
            temp_dir.path().join("types").as_path(),
            "user.json",
            &user_type.to_string(),
        )?;

        let post_type = json!({
            "types": [{"name": "Post", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(
            temp_dir.path().join("types").as_path(),
            "post.json",
            &post_type.to_string(),
        )?;

        let user_queries = json!({
            "types": [],
            "queries": [{"name": "getUser", "return_type": "User"}],
            "mutations": []
        });
        create_test_file(
            temp_dir.path().join("queries").as_path(),
            "user_queries.json",
            &user_queries.to_string(),
        )?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 2);
        assert_eq!(result["queries"].as_array().unwrap().len(), 1);

        Ok(())
    }

    #[test]
    fn test_duplicate_type_names_error() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let file1 = json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "file1.json", &file1.to_string())?;

        let file2 = json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "file2.json", &file2.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap());

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Duplicate type 'User'"));
        assert!(err_msg.contains("file1.json"));
        assert!(err_msg.contains("file2.json"));

        Ok(())
    }

    #[test]
    fn test_duplicate_query_names_error() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let file1 = json!({
            "types": [],
            "queries": [{"name": "getUser", "return_type": "User"}],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "file1.json", &file1.to_string())?;

        let file2 = json!({
            "types": [],
            "queries": [{"name": "getUser", "return_type": "User"}],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "file2.json", &file2.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap());

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Duplicate query 'getUser'"));

        Ok(())
    }

    #[test]
    fn test_empty_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 0);
        assert_eq!(result["queries"].as_array().unwrap().len(), 0);
        assert_eq!(result["mutations"].as_array().unwrap().len(), 0);

        Ok(())
    }

    #[test]
    fn test_nonexistent_directory() {
        let result = MultiFileLoader::load_from_directory("/nonexistent/path/to/schema");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_paths() -> Result<()> {
        let temp_dir = TempDir::new()?;

        let schema1 = json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "schema1.json", &schema1.to_string())?;

        let schema2 = json!({
            "types": [{"name": "Post", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "schema2.json", &schema2.to_string())?;

        let paths = vec![
            temp_dir.path().join("schema1.json"),
            temp_dir.path().join("schema2.json"),
        ];

        let result = MultiFileLoader::load_from_paths(&paths)?;

        assert_eq!(result["types"].as_array().unwrap().len(), 2);

        Ok(())
    }

}
