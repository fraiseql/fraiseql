//! `${VAR}` placeholders in a configuration file's text.
//!
//! The one expander both configuration files use: `fraiseql.toml`, read by the compiler, and
//! the server's configuration file. A value is substituted into the file's text before it is
//! parsed, so a value that could end its TOML string, or start a line, is refused. Outside a
//! quoted string the value is read as bare TOML, so one that could end the line early or add
//! a key there is refused too.

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
/// Returns a message naming the variable, never its value, when its value holds a newline, a
/// carriage return, a NUL or a TOML metacharacter (`"`, `'`, `\`, `[`, `]`, `{`, `}`); when
/// the placeholder sits outside a quoted string and its value holds `#`, `,`, `=` or
/// whitespace; or when it is unset and `unset` is [`UnsetVariable::Refuse`].
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
    let mut lexer = Lexer::default();
    for cap in PLACEHOLDER.captures_iter(content) {
        let (Some(whole), Some(name)) = (cap.get(0), cap.get(1)) else {
            continue;
        };
        result.push_str(&content[last_end..whole.start()]);
        let name = name.as_str();
        match (lookup(name), unset) {
            (Some(value), _) => {
                refuse_unsafe_value(name, &value)?;
                if lexer.advance_to(content.as_bytes(), whole.start()) == Context::Bare {
                    refuse_unsafe_bare_value(name, &value)?;
                }
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

/// Refuse a value that, read as bare TOML, would end the line early or add a key.
fn refuse_unsafe_bare_value(name: &str, value: &str) -> Result<(), String> {
    if value.contains(['#', ',', '=']) || value.chars().any(char::is_whitespace) {
        return Err(format!(
            "Environment variable {name} is placed outside a quoted string, and its value holds \
             `#`, `,`, `=` or whitespace, which would change the TOML read there; quote the \
             placeholder, or set a value without them"
        ));
    }
    Ok(())
}

/// Where a position in TOML text sits, as far as a substituted value is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    /// Outside any string or comment: the value is read as TOML.
    Bare,
    /// Inside a string or a comment: the value is text.
    Text,
}

/// What the lexer is inside of.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum State {
    #[default]
    Bare,
    Comment,
    Basic,
    Literal,
    MultiLineBasic,
    MultiLineLiteral,
}

/// A forward-only scan of TOML text, tracking strings and comments.
///
/// It reads the text as written, before any substitution: a substituted value cannot open or
/// close a string (quotes, backslashes and newlines are refused in it), so the context at each
/// placeholder is the one the written text gives it.
#[derive(Debug, Default)]
struct Lexer {
    at:    usize,
    state: State,
}

impl Lexer {
    /// Scan up to `end` and say where `end` sits.
    fn advance_to(&mut self, text: &[u8], end: usize) -> Context {
        while self.at < end {
            let rest = &text[self.at..];
            let (state, step) = match (self.state, rest[0]) {
                (State::Bare, b'#') => (State::Comment, 1),
                (State::Bare, b'"') if rest.starts_with(b"\"\"\"") => (State::MultiLineBasic, 3),
                (State::Bare, b'"') => (State::Basic, 1),
                (State::Bare, b'\'') if rest.starts_with(b"'''") => (State::MultiLineLiteral, 3),
                (State::Bare, b'\'') => (State::Literal, 1),
                (State::Comment | State::Basic | State::Literal, b'\n')
                | (State::Basic, b'"')
                | (State::Literal, b'\'') => (State::Bare, 1),
                (State::Basic | State::MultiLineBasic, b'\\') => (self.state, 2),
                (State::MultiLineBasic, b'"') if rest.starts_with(b"\"\"\"") => (State::Bare, 3),
                (State::MultiLineLiteral, b'\'') if rest.starts_with(b"'''") => (State::Bare, 3),
                (state, _) => (state, 1),
            };
            self.state = state;
            self.at += step;
        }
        if self.state == State::Bare {
            Context::Bare
        } else {
            Context::Text
        }
    }
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
