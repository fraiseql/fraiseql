//! Which `fraiseql.toml` applies to a schema input.
//!
//! One answer for every command that pairs a schema with its project config, so
//! `compile` and `run` can never read two different files for one schema (#1387).
//! `compile` used to read `fraiseql.toml` from the working directory while `run`
//! read the one beside the schema: compiled from a sibling subgraph's directory,
//! the artifact silently took the other subgraph's settings.
//!
//! The rule, first match wins:
//!
//! 1. the input **is** a `.toml` file — it is the project config;
//! 2. an explicit `--config <path>`, which must exist;
//! 3. the nearest `fraiseql.toml` in the input's directory or an ancestor of it, stopping at the
//!    repository root (the first directory holding `.git`), so a stray file above the project is
//!    never picked up;
//! 4. none: defaults apply.
//!
//! The working directory plays no part: the same command, run from anywhere,
//! compiles a schema with the same config.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

/// The file name a project config is discovered by.
pub const PROJECT_CONFIG_FILE: &str = "fraiseql.toml";

/// Where a schema input's project config comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// The input is itself a TOML project file.
    Input(PathBuf),
    /// Named on the command line with `--config`.
    Explicit(PathBuf),
    /// The nearest `fraiseql.toml` beside or above the input.
    Discovered(PathBuf),
    /// No config applies; `searched_from` is the directory the search started in.
    Absent {
        /// The input's directory, where discovery began.
        searched_from: PathBuf,
    },
}

impl ConfigSource {
    /// Resolve the config for `input`, honouring an explicit `--config` path.
    ///
    /// # Errors
    ///
    /// Returns an error when `explicit` is given and does not name a file, when
    /// `explicit` is given for an input that is itself a TOML project file, or when
    /// the input's absolute path cannot be determined.
    pub fn resolve(input: &Path, explicit: Option<&Path>) -> Result<Self> {
        let is_toml = input
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"));
        if is_toml {
            if let Some(explicit) = explicit {
                bail!(
                    "--config {} cannot be combined with the TOML input {}: that file is \
                     already the project config. Pass one or the other.",
                    explicit.display(),
                    input.display()
                );
            }
            return Ok(Self::Input(input.to_path_buf()));
        }
        if let Some(explicit) = explicit {
            if !explicit.is_file() {
                bail!("--config {}: no such file", explicit.display());
            }
            return Ok(Self::Explicit(explicit.to_path_buf()));
        }

        let absolute = std::path::absolute(input)
            .with_context(|| format!("Cannot resolve the directory of {}", input.display()))?;
        let start = absolute.parent().unwrap_or(&absolute).to_path_buf();
        Ok(discover_from(&start).map_or(
            Self::Absent {
                searched_from: start,
            },
            Self::Discovered,
        ))
    }

    /// The project config file to read, if any.
    ///
    /// `None` both when no config applies and when the input is itself the
    /// project file (which the TOML workflow reads as a schema, not as a
    /// [`TomlProjectConfig`](super::TomlProjectConfig)).
    #[must_use]
    pub fn project_config(&self) -> Option<&Path> {
        match self {
            Self::Explicit(path) | Self::Discovered(path) => Some(path),
            Self::Input(_) | Self::Absent { .. } => None,
        }
    }
}

impl fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(path) => write!(f, "{} (the input)", path.display()),
            Self::Explicit(path) => write!(f, "{} (--config)", path.display()),
            Self::Discovered(path) => write!(f, "{}", path.display()),
            Self::Absent { searched_from } => write!(
                f,
                "none — no {PROJECT_CONFIG_FILE} in {} or above it; defaults apply",
                searched_from.display()
            ),
        }
    }
}

/// The nearest `fraiseql.toml` in `start` or its ancestors, up to the repository root.
fn discover_from(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join(PROJECT_CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        if dir.join(".git").exists() {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests;
