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
    /// Returns an error if any glob pattern is syntactically invalid, if a matched path
    /// cannot be accessed, or if a **configured pattern matches no files** — see
    /// `resolve_group`.
    pub fn resolve_globs(&self) -> Result<ResolvedIncludes> {
        Ok(ResolvedIncludes {
            types:     Self::resolve_group(&self.types, "types")?,
            queries:   Self::resolve_group(&self.queries, "queries")?,
            mutations: Self::resolve_group(&self.mutations, "mutations")?,
        })
    }

    /// Expand one `[includes]` group's patterns into a sorted, deduplicated file list.
    ///
    /// A configured pattern that matches **nothing** is an error. It is either a typo or a
    /// build-ordering mistake, and the alternative — compiling a schema silently missing
    /// everything that file was going to contribute — is the #723/#612 failure mode: the
    /// user configured a source, the compile reported success, and the types were not there.
    ///
    /// The three groups shared three copies of this loop, which is how none of them acquired
    /// the check.
    ///
    /// # Errors
    ///
    /// Returns an error naming the group and the offending pattern.
    fn resolve_group(patterns: &[String], group: &str) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();

        for pattern in patterns {
            let before = paths.len();
            for entry in glob::glob(pattern)
                .context(format!("Invalid glob pattern for {group}: {pattern}"))?
            {
                match entry {
                    Ok(path) => paths.push(path),
                    Err(e) => {
                        anyhow::bail!("Error resolving {group} glob pattern '{pattern}': {e}");
                    },
                }
            }
            if paths.len() == before {
                anyhow::bail!(
                    "[includes] {group} pattern '{pattern}' matched no files. Fix the path or \
                     remove the entry — compiling without it would silently produce a schema \
                     missing whatever it was meant to contribute."
                );
            }
        }

        paths.sort();
        paths.dedup();
        Ok(paths)
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
