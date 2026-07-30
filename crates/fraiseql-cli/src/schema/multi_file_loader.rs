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
use serde_json::Value;
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

        let mut merged = crate::schema::seam::empty_accumulator();
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

            // Duplicate detection covers **every** named section, not just the three that
            // used to be merged: a duplicate enum or interface across two files is the
            // same authoring mistake as a duplicate type, and silently keeping both
            // produces a schema whose behaviour depends on directory iteration order.
            for section in crate::schema::seam::AUTHORABLE_ARRAY_SECTIONS {
                let Some(Value::Array(items)) = value.get(*section) else {
                    continue;
                };
                for item in items {
                    let Some(name) = item.get("name").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let key = format!("{section}:{name}");
                    if let Some(existing) = name_to_file.get(&key) {
                        let noun = crate::schema::seam::section_noun(section);
                        bail!(
                            "Duplicate {noun} '{name}' found in:\n  - {existing}\n  \
                             - {file_path_str}"
                        );
                    }
                    name_to_file.insert(key, file_path_str.clone());
                }
            }

            crate::schema::seam::absorb_sections(&mut merged, &value, &file_path_str)?;
        }

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
        let mut merged = crate::schema::seam::empty_accumulator();

        for path in paths {
            if !path.exists() {
                bail!("File not found: {}", path.display());
            }

            let content =
                fs::read_to_string(path).context(format!("Failed to read {}", path.display()))?;
            let value: Value = serde_json::from_str(&content)
                .context(format!("Failed to parse JSON from {}", path.display()))?;

            crate::schema::seam::absorb_sections(&mut merged, &value, &path.display().to_string())?;
        }

        Ok(merged)
    }
}
