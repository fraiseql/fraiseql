//! The one rule for what a function may import.
//!
//! A specifier is accepted in exactly one form, `@shared/<segment>/…/<file>.<ext>`, with
//! `<ext>` one of `ts`, `js`, `mts`, `mjs`. Everything else is refused, each case with
//! its own [`Refusal`], so an author is told which rule an import broke.
//!
//! Resolution has two halves:
//!
//! - [`resolve_specifier`] is **lexical**: it turns a specifier into a [`ModuleKey`] without
//!   touching the filesystem. The module loader and the isolate both call it, so what loads at boot
//!   and what the isolate is allowed to import are one decision.
//! - [`locate`] places a key under the canonical shared directory and checks, after resolving every
//!   symlink, that the file is still inside it.

use std::path::{Path, PathBuf};

/// The prefix every importable specifier starts with.
pub const SHARED_PREFIX: &str = "@shared/";

/// The extensions an importable module may have.
pub const MODULE_EXTENSIONS: [&str; 4] = ["ts", "js", "mts", "mjs"];

/// Where shared modules live inside the isolate: `file:///fraiseql/shared/<key>`.
const VIRTUAL_ROOT: &str = "file:///fraiseql/shared/";

/// A shared module, named by its `/`-separated path under the shared directory.
///
/// Only [`resolve_specifier`] makes one, so every key is a path that passed the
/// lexical rule: no empty, `.` or `..` segment, no backslash, no percent sign, and a
/// module extension.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleKey(String);

impl ModuleKey {
    /// The key as written after `@shared/`: `llm/client.ts`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The specifier that names this module: `@shared/llm/client.ts`.
    #[must_use]
    pub fn specifier(&self) -> String {
        format!("{SHARED_PREFIX}{}", self.0)
    }

    /// The URL this module has inside the isolate: `file:///fraiseql/shared/llm/client.ts`.
    #[must_use]
    pub fn url(&self) -> String {
        format!("{VIRTUAL_ROOT}{}", self.0)
    }

    /// The key a module URL inside the isolate names, if it is one.
    #[must_use]
    pub fn from_url(url: &str) -> Option<Self> {
        let key = url.strip_prefix(VIRTUAL_ROOT)?;
        resolve_specifier(&format!("{SHARED_PREFIX}{key}")).ok()
    }
}

impl std::fmt::Display for ModuleKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{SHARED_PREFIX}{}", self.0)
    }
}

/// Why an import is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Refusal {
    /// No `[functions] shared_dir` is declared, so nothing is importable.
    NoSharedDir,
    /// A URL or package scheme: `https:`, `file:`, `data:`, `npm:`, `jsr:`, `node:`, ….
    Scheme {
        /// The scheme, without the colon.
        scheme: String,
    },
    /// An absolute path.
    Absolute,
    /// A relative path (`./`, `../`).
    Relative,
    /// A bare specifier: a package name, or anything not starting `@shared/`.
    Bare,
    /// A backslash, which is a separator on one platform and a name byte on another.
    Backslash,
    /// A percent sign, which a URL resolver would decode into another path.
    PercentEncoded,
    /// A control character.
    ControlCharacter,
    /// An empty path segment (`@shared//x.ts`, `@shared/`).
    EmptySegment,
    /// A `.` or `..` path segment.
    DotSegment,
    /// The file name has no module extension (`.ts`, `.js`, `.mts`, `.mjs`).
    NotAModuleExtension,
    /// An import with attributes (`with { type: "json" }`): only code modules load.
    ImportAttributes,
    /// A specifier that is not a valid string (a lone surrogate).
    NotAString,
    /// The file does not exist.
    Missing,
    /// The path names a directory or another non-regular file.
    NotAFile,
    /// The path resolves, through a symlink, outside the shared directory.
    Escapes {
        /// Where it resolves to.
        target: PathBuf,
    },
    /// The file is not UTF-8.
    NotUtf8,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let allowed = "only `@shared/<path>.{ts,js,mts,mjs}` is importable";
        match self {
            Self::NoSharedDir => write!(
                f,
                "no `[functions] shared_dir` is declared, so a function imports nothing; declare \
                 one to import shared modules as `@shared/<path>`"
            ),
            Self::Scheme { scheme } => {
                write!(f, "the `{scheme}:` scheme is not importable; {allowed}")
            },
            Self::Absolute => write!(f, "an absolute path is not importable; {allowed}"),
            Self::Relative => write!(f, "a relative path is not importable; {allowed}"),
            Self::Bare => write!(f, "a bare specifier is not importable; {allowed}"),
            Self::Backslash => write!(f, "a backslash is not allowed in a specifier"),
            Self::PercentEncoded => write!(f, "percent-encoding is not allowed in a specifier"),
            Self::ControlCharacter => {
                write!(f, "a control character is not allowed in a specifier")
            },
            Self::EmptySegment => write!(f, "the path has an empty segment"),
            Self::DotSegment => write!(f, "a `.` or `..` segment is not allowed"),
            Self::NotAModuleExtension => {
                write!(f, "the file has no module extension; {allowed}")
            },
            Self::ImportAttributes => {
                write!(f, "import attributes are not supported: only code modules load")
            },
            Self::NotAString => write!(f, "the specifier is not a valid string"),
            Self::Missing => write!(f, "no such file in the shared directory"),
            Self::NotAFile => write!(f, "not a regular file"),
            Self::Escapes { target } => {
                write!(f, "it resolves to {}, outside the shared directory", target.display())
            },
            Self::NotUtf8 => write!(f, "the file is not UTF-8"),
        }
    }
}

/// The key `specifier` names, by the lexical rule alone.
///
/// # Errors
///
/// The [`Refusal`] for the first rule the specifier breaks.
pub fn resolve_specifier(specifier: &str) -> Result<ModuleKey, Refusal> {
    if specifier.chars().any(char::is_control) {
        return Err(Refusal::ControlCharacter);
    }
    if let Some(scheme) = scheme_of(specifier) {
        return Err(Refusal::Scheme {
            scheme: scheme.to_string(),
        });
    }
    if specifier.starts_with('/') {
        return Err(Refusal::Absolute);
    }
    if specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
    {
        return Err(Refusal::Relative);
    }
    let Some(path) = specifier.strip_prefix(SHARED_PREFIX) else {
        return Err(Refusal::Bare);
    };
    if path.contains('\\') {
        return Err(Refusal::Backslash);
    }
    if path.contains('%') {
        return Err(Refusal::PercentEncoded);
    }
    for segment in path.split('/') {
        match segment {
            "" => return Err(Refusal::EmptySegment),
            "." | ".." => return Err(Refusal::DotSegment),
            _ => {},
        }
    }
    let has_module_extension =
        path.rsplit('/').next().and_then(|file| file.rsplit_once('.')).is_some_and(
            |(stem, extension)| !stem.is_empty() && MODULE_EXTENSIONS.contains(&extension),
        );
    if !has_module_extension {
        return Err(Refusal::NotAModuleExtension);
    }
    Ok(ModuleKey(path.to_string()))
}

/// The URL scheme `specifier` starts with, if any: letters, digits, `+`, `-`, `.`,
/// starting with a letter, ending at the first `:` before any `/`.
fn scheme_of(specifier: &str) -> Option<&str> {
    let (scheme, _) = specifier.split_once(':')?;
    let mut chars = scheme.chars();
    let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest_is_scheme = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    (starts_with_letter && rest_is_scheme).then_some(scheme)
}

/// The file `key` names under `shared_root`, which must be canonical.
///
/// The joined path is canonicalized, which resolves every symlink, and must still lie
/// inside `shared_root`, compared component by component.
///
/// # Errors
///
/// [`Refusal::Missing`], [`Refusal::Escapes`] or [`Refusal::NotAFile`].
pub fn locate(shared_root: &Path, key: &ModuleKey) -> Result<PathBuf, Refusal> {
    let joined = key
        .as_str()
        .split('/')
        .fold(shared_root.to_path_buf(), |path, segment| path.join(segment));
    let target = joined.canonicalize().map_err(|_| Refusal::Missing)?;
    if !target.starts_with(shared_root) {
        return Err(Refusal::Escapes { target });
    }
    if !target.is_file() {
        return Err(Refusal::NotAFile);
    }
    Ok(target)
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
