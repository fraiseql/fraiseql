//! Domain-based schema organization types.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Domain-based schema organization
///
/// Automatically discovers schema files in domain directories:
/// ```toml
/// [schema.domain_discovery]
/// enabled = true
/// root_dir = "schema"
/// ```
///
/// Expects structure:
/// ```text
/// schema/
/// ├── auth/
/// │   ├── types.json
/// │   ├── queries.json
/// │   └── mutations.json
/// ├── products/
/// │   ├── types.json
/// │   ├── queries.json
/// │   └── mutations.json
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DomainDiscovery {
    /// Enable automatic domain discovery
    pub enabled:  bool,
    /// Root directory containing domains
    pub root_dir: String,
}

/// Represents a discovered domain
#[derive(Debug, Clone)]
pub struct Domain {
    /// Domain name (directory name)
    pub name: String,
    /// Path to domain root
    pub path: PathBuf,
}

impl DomainDiscovery {
    /// Discover all domains in root_dir
    ///
    /// # Errors
    ///
    /// Returns an error if domain discovery is enabled but `root_dir` does not
    /// exist, if the directory cannot be read, or if a domain entry has an
    /// invalid (non-UTF-8) name.
    pub fn resolve_domains(&self) -> Result<Vec<Domain>> {
        if !self.enabled {
            return Ok(Vec::new());
        }

        let root = PathBuf::from(&self.root_dir);
        if !root.is_dir() {
            anyhow::bail!("Domain discovery root not found: {}", self.root_dir);
        }

        let mut domains = Vec::new();

        for entry in std::fs::read_dir(&root)
            .context(format!("Failed to read domain root: {}", self.root_dir))?
        {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if path.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(std::string::ToString::to_string)
                    .ok_or_else(|| anyhow::anyhow!("Invalid domain name: {}", path.display()))?;

                domains.push(Domain { name, path });
            }
        }

        // Sort for deterministic ordering
        domains.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(domains)
    }
}

/// Schema includes for multi-file composition (glob patterns)
///
/// Supports glob patterns for flexible file inclusion:
/// ```toml
/// [schema.includes]
/// types = ["schema/types/**/*.json"]
/// queries = ["schema/queries/**/*.json"]
/// mutations = ["schema/mutations/**/*.json"]
/// ```
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SchemaIncludes {
    /// Glob patterns for type files
    pub types:     Vec<String>,
    /// Glob patterns for query files
    pub queries:   Vec<String>,
    /// Glob patterns for mutation files
    pub mutations: Vec<String>,
}

impl SchemaIncludes {
    /// Check if any includes are specified
    pub fn is_empty(&self) -> bool {
        self.types.is_empty() && self.queries.is_empty() && self.mutations.is_empty()
    }

    /// Resolve glob patterns to actual file paths
    ///
    /// # Returns
    /// `ResolvedIncludes` with expanded file paths.
    ///
    /// # Errors
    ///
    /// Returns an error if any glob pattern is syntactically invalid or if a
    /// matched path cannot be accessed.
    pub fn resolve_globs(&self) -> Result<ResolvedIncludes> {
        use glob::glob as glob_pattern;

        let mut type_paths = Vec::new();
        let mut query_paths = Vec::new();
        let mut mutation_paths = Vec::new();

        // Resolve type globs
        for pattern in &self.types {
            for entry in glob_pattern(pattern)
                .context(format!("Invalid glob pattern for types: {pattern}"))?
            {
                match entry {
                    Ok(path) => type_paths.push(path),
                    Err(e) => {
                        anyhow::bail!("Error resolving type glob pattern '{pattern}': {e}");
                    },
                }
            }
        }

        // Resolve query globs
        for pattern in &self.queries {
            for entry in glob_pattern(pattern)
                .context(format!("Invalid glob pattern for queries: {pattern}"))?
            {
                match entry {
                    Ok(path) => query_paths.push(path),
                    Err(e) => {
                        anyhow::bail!("Error resolving query glob pattern '{pattern}': {e}");
                    },
                }
            }
        }

        // Resolve mutation globs
        for pattern in &self.mutations {
            for entry in glob_pattern(pattern)
                .context(format!("Invalid glob pattern for mutations: {pattern}"))?
            {
                match entry {
                    Ok(path) => mutation_paths.push(path),
                    Err(e) => {
                        anyhow::bail!("Error resolving mutation glob pattern '{pattern}': {e}");
                    },
                }
            }
        }

        // Sort for deterministic ordering
        type_paths.sort();
        query_paths.sort();
        mutation_paths.sort();

        // Remove duplicates
        type_paths.dedup();
        query_paths.dedup();
        mutation_paths.dedup();

        Ok(ResolvedIncludes {
            types:     type_paths,
            queries:   query_paths,
            mutations: mutation_paths,
        })
    }
}

/// Resolved glob patterns to actual file paths
#[derive(Debug, Clone)]
pub struct ResolvedIncludes {
    /// Resolved type file paths
    pub types:     Vec<PathBuf>,
    /// Resolved query file paths
    pub queries:   Vec<PathBuf>,
    /// Resolved mutation file paths
    pub mutations: Vec<PathBuf>,
}
