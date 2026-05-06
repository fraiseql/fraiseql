//! Multi-file schema loader - loads and merges JSON schema files from directories
//!
//! Supports flexible schema composition from single files to deeply nested directory structures:
//! - Load all *.json files from a directory recursively
//! - Merge types, queries, mutations arrays
//! - Deduplicate by name with error reporting
//! - Preserve file path information for error messages

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use walkdir::WalkDir;

/// Maximum number of JSON schema files accepted from a single directory tree.
///
/// Prevents runaway resource use when pointed at an unexpectedly large directory
/// (e.g. a mounted filesystem root or a node_modules tree).
pub(crate) const MAX_SCHEMA_FILES: usize = 1_000;

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
    /// ```no_run
    /// // Requires: a "schema/" directory containing JSON schema files on disk.
    /// use fraiseql_cli::schema::multi_file_loader::MultiFileLoader;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let merged = MultiFileLoader::load_from_directory("schema/")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_from_directory(dir_path: &str) -> Result<Value> {
        let result = Self::load_from_directory_with_tracking(dir_path)?;
        Ok(result.merged)
    }

    /// Load from directory with file path tracking for conflict detection
    ///
    /// # Errors
    ///
    /// Returns an error if `dir_path` is not a directory, if more than
    /// `MAX_SCHEMA_FILES` JSON files are found, if any file cannot be read or
    /// parsed as JSON, or if duplicate type/query/mutation names are detected.
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
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        {
            json_files.push(entry.path().to_path_buf());
            if json_files.len() > MAX_SCHEMA_FILES {
                bail!(
                    "Schema directory {dir_path:?} contains more than {MAX_SCHEMA_FILES} JSON \
                     files. Point --schema-dir at a directory containing only schema files."
                );
            }
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
                                "Duplicate type '{name}' found in:\n  - {existing}\n  - {file_path_str}"
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
                                "Duplicate query '{name}' found in:\n  - {existing}\n  - {file_path_str}"
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
                                "Duplicate mutation '{name}' found in:\n  - {existing}\n  - {file_path_str}"
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
    /// Merged `Value` with "types", "queries", "mutations" as arrays.
    ///
    /// # Errors
    ///
    /// Returns an error if any path does not exist, cannot be read, or cannot
    /// be parsed as JSON.
    pub fn load_from_paths(paths: &[PathBuf]) -> Result<Value> {
        let mut types = Vec::new();
        let mut queries = Vec::new();
        let mut mutations = Vec::new();

        for path in paths {
            if !path.exists() {
                bail!("File not found: {}", path.display());
            }

            let content =
                fs::read_to_string(path).context(format!("Failed to read {}", path.display()))?;
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

