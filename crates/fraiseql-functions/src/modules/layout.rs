//! Where the shared-module directory may be.
//!
//! Only the shared directory is importable, so it must not hold a function's own
//! file. A function lives at `<module_dir>/<name>.<ext>`; a shared directory that is
//! `module_dir` itself, or contains it, would make every function file importable as
//! `@shared/…`. A shared directory inside `module_dir` is fine, and is the documented
//! layout (`functions/shared`): no function file is inside it.

use std::path::{Path, PathBuf};

/// Why a declared `shared_dir` cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SharedDirError {
    /// The directory does not exist, or is not a directory.
    NotADirectory {
        /// The declared directory.
        shared_dir: PathBuf,
    },

    /// The directory is `module_dir`, or contains it, so it holds function files.
    HoldsFunctionFiles {
        /// The declared shared directory.
        shared_dir: PathBuf,
        /// The declared module directory.
        module_dir: PathBuf,
    },
}

impl std::fmt::Display for SharedDirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotADirectory { shared_dir } => write!(
                f,
                "`[functions] shared_dir` is {}, which is not a directory",
                shared_dir.display()
            ),
            Self::HoldsFunctionFiles {
                shared_dir,
                module_dir,
            } => write!(
                f,
                "`[functions] shared_dir` ({}) is or contains `module_dir` ({}), so every \
                 function's own file would be importable. Put shared modules in a directory of \
                 their own, such as `<module_dir>/shared`",
                shared_dir.display(),
                module_dir.display()
            ),
        }
    }
}

impl std::error::Error for SharedDirError {}

/// Check a declared `shared_dir` against `module_dir`, returning its canonical path.
///
/// The canonical path is the root every import is resolved against and checked to
/// stay inside.
///
/// # Errors
///
/// [`SharedDirError::NotADirectory`] when `shared_dir` is not a directory, and
/// [`SharedDirError::HoldsFunctionFiles`] when it is `module_dir` or an ancestor of it.
pub fn check_shared_dir(module_dir: &Path, shared_dir: &Path) -> Result<PathBuf, SharedDirError> {
    let not_a_directory = || SharedDirError::NotADirectory {
        shared_dir: shared_dir.to_path_buf(),
    };
    let shared = shared_dir.canonicalize().map_err(|_| not_a_directory())?;
    if !shared.is_dir() {
        return Err(not_a_directory());
    }
    // A `module_dir` that does not exist holds no function file to expose; the
    // module loader reports its absence in its own terms.
    if let Ok(modules) = module_dir.canonicalize() {
        if modules.starts_with(&shared) {
            return Err(SharedDirError::HoldsFunctionFiles {
                shared_dir: shared_dir.to_path_buf(),
                module_dir: module_dir.to_path_buf(),
            });
        }
    }
    Ok(shared)
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
