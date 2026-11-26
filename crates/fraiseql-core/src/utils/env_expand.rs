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
/// Returns a message naming the variable, never its value, when its value is not UTF-8; when
/// it holds a TOML metacharacter (`"`, `'`, `\`, `[`, `]`, `{`, `}`) or a character that is not
/// printable text (a control character, tab included; an invisible format character; a
/// separator other than the plain space), wherever the placeholder sits; when the placeholder
/// sits outside a quoted string and its value holds `#`, `,`, `=` or a space; or when it is
/// unset and `unset` is [`UnsetVariable::Refuse`].
pub fn expand_env_vars(content: &str, unset: UnsetVariable) -> Result<String, String> {
    expand(content, unset).map(|expanded| expanded.text)
}

/// [`expand_env_vars`], keeping the values it substituted so that a parse error over the
/// result can be reported without them.
///
/// # Errors
///
/// As [`expand_env_vars`].
pub fn expand(content: &str, unset: UnsetVariable) -> Result<Expanded, String> {
    expand_looked_up(content, unset, |name| {
        std::env::var_os(name).map(|value| value.into_string().map_err(|_| NotUtf8))
    })
}

/// A variable whose value is not UTF-8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotUtf8;

/// A configuration file's text after expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    /// The text, each set placeholder replaced by its value.
    pub text: String,
    values:   Vec<String>,
}

impl Expanded {
    /// The message for `error`, raised parsing [`Self::text`], opening with `label`.
    ///
    /// When nothing was substituted it is the whole error, the line's text included. When a
    /// value was, it names the line and the reason only, since the line's text may hold the
    /// value. The reason a typed parse gives can quote it too (`invalid type: string "…"`), in
    /// whatever form the parser renders it, so every quoted span of the reason is replaced by
    /// `<value>`; a reason still holding a substituted value is replaced whole.
    #[must_use]
    pub fn parse_error(&self, error: &toml::de::Error, label: &str) -> String {
        if self.values.is_empty() {
            return format!("{label}: {error}");
        }
        let reason = without_quoted_spans(error.message());
        let reason = if self.values.iter().any(|value| reason.contains(value.as_str())) {
            "a value expanded from an environment variable is not valid there".to_owned()
        } else {
            reason
        };
        match error.span() {
            Some(span) => {
                let line = self.text[..span.start.min(self.text.len())].matches('\n').count() + 1;
                format!("{label} at line {line}: {reason}")
            },
            None => format!("{label}: {reason}"),
        }
    }
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
    expand_tracked(content, unset, lookup).map(|expanded| expanded.text)
}

/// [`expand_with`], keeping the values it substituted.
///
/// # Errors
///
/// As [`expand_env_vars`].
pub fn expand_tracked(
    content: &str,
    unset: UnsetVariable,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Expanded, String> {
    expand_looked_up(content, unset, |name| lookup(name).map(Ok))
}

/// [`expand_tracked`] over a lookup that can find a value that is not UTF-8.
fn expand_looked_up(
    content: &str,
    unset: UnsetVariable,
    lookup: impl Fn(&str) -> Option<Result<String, NotUtf8>>,
) -> Result<Expanded, String> {
    let mut values = Vec::new();
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
            (Some(Err(NotUtf8)), _) => {
                return Err(format!(
                    "Environment variable {name} is not UTF-8 text; a value must be printable text"
                ));
            },
            (Some(Ok(value)), _) => {
                refuse_unsafe_value(name, &value)?;
                refuse_unprintable_value(name, &value)?;
                if lexer.advance_to(content.as_bytes(), whole.start()) == Context::Bare {
                    refuse_unsafe_bare_value(name, &value)?;
                }
                result.push_str(&value);
                if !value.is_empty() {
                    values.push(value);
                }
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
    Ok(Expanded {
        text: result,
        values,
    })
}

/// `reason` with every span a parser quotes, in `"` or backticks, replaced by `<value>`,
/// whatever it holds: a value is quoted in whatever form the parser renders it, so no form of
/// the value is looked for. A backslash inside a `"` span escapes the next character; a span
/// never closed runs to the end.
pub(crate) fn without_quoted_spans(reason: &str) -> String {
    let mut out = String::with_capacity(reason.len());
    let mut chars = reason.chars();
    while let Some(c) = chars.next() {
        if c != '"' && c != '`' {
            out.push(c);
            continue;
        }
        out.push_str("<value>");
        while let Some(inner) = chars.next() {
            if inner == c {
                break;
            }
            if c == '"' && inner == '\\' {
                chars.next();
            }
        }
    }
    out
}

/// The characters a value may not hold, wherever its placeholder sits, each with what the
/// refusal calls it.
static UNPRINTABLE: LazyLock<[(regex::Regex, &str); 3]> = LazyLock::new(|| {
    #[allow(clippy::expect_used)] // Reason: the patterns are constants and valid
    let class = |pattern: &str| regex::Regex::new(pattern).expect("class pattern is valid");
    [
        (class(r"\p{Cc}"), "a control character (C0, C1 or DEL, a tab included)"),
        (class(r"\p{Cf}"), "an invisible format character (Unicode Cf)"),
        (
            class(r"[\p{Zl}\p{Zp}[\p{Zs}--\x20]]"),
            "a separator other than the plain space (Unicode Zs, Zl or Zp)",
        ),
    ]
});

/// Refuse a value that is not printable text: it would be invisible in the file it lands in,
/// and a parser quoting it renders it escaped.
fn refuse_unprintable_value(name: &str, value: &str) -> Result<(), String> {
    for (class, what) in UNPRINTABLE.iter() {
        if class.is_match(value) {
            return Err(format!(
                "Environment variable {name} holds {what}; a value must be printable text"
            ));
        }
    }
    Ok(())
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
