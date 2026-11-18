//! `${VAR}` placeholders in a configuration file's text.
//!
//! The one expander both configuration files use: `fraiseql.toml`, read by the compiler, and
//! the server's configuration file. A value is substituted into the file's text before it is
//! parsed, so a value that could end its TOML string, or start a line, is refused.

use std::sync::LazyLock;

/// What an unset variable does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsetVariable {
    /// The placeholder stays in the text as written. `fraiseql.toml` is compiled where the
    /// deployment's variables may not be set.
    KeepLiteral,
    /// The file is refused. The server's file is read where the deployment runs, and a
    /// placeholder kept as a value (a token, a URL) would be used as that value.
    Refuse,
}

static PLACEHOLDER: LazyLock<regex::Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)] // Reason: the pattern is a constant and valid
    regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("placeholder pattern is valid")
});

/// Expand every `${VAR}` in `content` from the process environment.
///
/// # Errors
///
/// Returns a message naming the variable when its value holds a newline, a carriage return,
/// a NUL or a TOML metacharacter (`"`, `'`, `\`, `[`, `]`, `{`, `}`), or when it is unset and
/// `unset` is [`UnsetVariable::Refuse`].
pub fn expand_env_vars(content: &str, unset: UnsetVariable) -> Result<String, String> {
    expand_with(content, unset, |name| std::env::var(name).ok())
}

/// [`expand_env_vars`] over a lookup other than the process environment.
///
/// # Errors
///
/// As [`expand_env_vars`].
pub fn expand_with(
    content: &str,
    unset: UnsetVariable,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;
    for cap in PLACEHOLDER.captures_iter(content) {
        let (Some(whole), Some(name)) = (cap.get(0), cap.get(1)) else {
            continue;
        };
        result.push_str(&content[last_end..whole.start()]);
        let name = name.as_str();
        match (lookup(name), unset) {
            (Some(value), _) => {
                refuse_unsafe_value(name, &value)?;
                result.push_str(&value);
            },
            (None, UnsetVariable::KeepLiteral) => result.push_str(whole.as_str()),
            (None, UnsetVariable::Refuse) => {
                return Err(format!(
                    "environment variable {name} is not set, and `${{{name}}}` names it"
                ));
            },
        }
        last_end = whole.end();
    }
    result.push_str(&content[last_end..]);
    Ok(result)
}

fn refuse_unsafe_value(name: &str, value: &str) -> Result<(), String> {
    if value.contains('\n') {
        return Err(format!("Environment variable {name} contains newline character"));
    }
    if value.contains('\r') {
        return Err(format!("Environment variable {name} contains carriage return character"));
    }
    if value.contains('\0') {
        return Err(format!("Environment variable {name} contains null character"));
    }
    if value.contains(['"', '\'', '\\', ']', '[', '{', '}']) {
        return Err(format!(
            "Environment variable {name} contains TOML metacharacter that could break TOML \
             parsing"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
