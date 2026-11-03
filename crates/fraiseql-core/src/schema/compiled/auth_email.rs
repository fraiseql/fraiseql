//! The mail `[auth.local]` sends: `[auth.local.email.{otp,reset,verification}]`.

use serde::{Deserialize, Serialize};

/// `[auth.local.email]`: the subject and body of each mail `[auth.local]` sends. A kind left
/// unset keeps the built-in English text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalEmailConfig {
    /// The sign-in mail: its code, and its magic link when `magic_link_template` is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otp: Option<EmailTemplate>,

    /// The password-reset mail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<EmailTemplate>,

    /// The email-verification mail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<EmailTemplate>,
}

/// One mail's subject and bodies.
///
/// In `fraiseql.toml` the bodies are files, `text_template` and `html_template`, relative to
/// the file. `fraiseql compile` reads them into `text` and `html` and drops the paths, so the
/// server needs only the compiled schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailTemplate {
    /// The subject line, sent as written: no placeholders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    /// Path of the plain-text body, in `fraiseql.toml` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_template: Option<String>,

    /// Path of the `HTML` body, in `fraiseql.toml` only. Sent beside the text body as
    /// `multipart/alternative`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html_template: Option<String>,

    /// The plain-text body, as the compiler read it from `text_template`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// The `HTML` body, as the compiler read it from `html_template`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
}

/// Which mail a template is for: it decides the placeholders the template may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailKind {
    /// The sign-in mail: `{code}`, `{link}` (the magic link), `{expires_minutes}`.
    Otp,
    /// The password-reset mail: `{link}`, `{expires_minutes}`.
    Reset,
    /// The email-verification mail: `{link}`, `{expires_minutes}`.
    Verification,
}

impl EmailKind {
    /// The key under `[auth.local.email]`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Otp => "otp",
            Self::Reset => "reset",
            Self::Verification => "verification",
        }
    }

    const fn placeholders(self) -> &'static [&'static str] {
        match self {
            Self::Otp => &["code", "link", "expires_minutes"],
            Self::Reset | Self::Verification => &["link", "expires_minutes"],
        }
    }
}

/// The names of the `{name}` placeholders in `template`, in order.
///
/// A name starts with an ASCII letter or `_` and continues with ASCII letters, digits and `_`.
/// Other braces, such as `HTML` styles (`a {color: red}`), are text.
fn placeholders_in(template: &str) -> impl Iterator<Item = (usize, &str)> {
    template.match_indices('{').filter_map(move |(at, _)| {
        let rest = &template[at + 1..];
        let end = rest.find('}')?;
        let name = &rest[..end];
        let mut chars = name.chars();
        let starts = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        (starts && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some((at, name))
    })
}

/// `template` with each placeholder named in `values` replaced.
///
/// One pass: a value is never read again, so one that contains `{code}` stays as written.
/// Values are `HTML`-escaped when `html` is set. A placeholder not in `values` is left as
/// written; [`EmailTemplate::check`] refuses those.
#[must_use]
pub fn render_email_template(template: &str, values: &[(&str, &str)], html: bool) -> String {
    let mut out = String::with_capacity(template.len());
    let mut copied = 0;
    for (at, name) in placeholders_in(template) {
        let Some((_, value)) = values.iter().find(|(key, _)| *key == name) else {
            continue;
        };
        out.push_str(&template[copied..at]);
        if html {
            escape_html_into(value, &mut out);
        } else {
            out.push_str(value);
        }
        copied = at + name.len() + 2;
    }
    out.push_str(&template[copied..]);
    out
}

/// A link to the operator's front end: `template` with each placeholder in `values` replaced.
///
/// Values are percent-encoded except for the characters a URL leaves as they are
/// (`A-Z a-z 0-9 - . _ ~`). One pass, as [`render_email_template`].
#[must_use]
pub fn render_link_template(template: &str, values: &[(&str, &str)]) -> String {
    let encoded: Vec<(&str, String)> =
        values.iter().map(|(name, value)| (*name, percent_encode(value))).collect();
    let encoded: Vec<(&str, &str)> =
        encoded.iter().map(|(name, value)| (*name, value.as_str())).collect();
    render_email_template(template, &encoded, false)
}

fn percent_encode(value: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            // Reason: writing to a String cannot fail
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

fn escape_html_into(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

/// Refuse a value bound for a mail header that breaks the line: `CR` or `LF` would let it
/// add headers of its own.
///
/// # Errors
///
/// Returns a message naming `key` when `value` holds a line break.
pub fn check_email_header(key: &str, value: &str) -> Result<(), String> {
    if value.contains(['\r', '\n']) {
        return Err(format!("{key} must be one line, without a line break: {value:?}"));
    }
    Ok(())
}

impl EmailTemplate {
    /// Check a template the server is about to send.
    ///
    /// `magic_link` says whether `magic_link_template` is set: without it there is no link to
    /// put in a sign-in mail.
    ///
    /// # Errors
    ///
    /// Returns a message naming the key and the problem when:
    /// - a body is still a path, so the schema was not compiled by `fraiseql compile`;
    /// - the subject breaks its line, or holds a placeholder (it is sent as written);
    /// - an `HTML` body has no text body beside it;
    /// - a body holds a placeholder its mail does not have, or lacks the one it needs: `{code}` or
    ///   `{link}` in a sign-in mail, `{link}` in the others.
    pub fn check(&self, kind: EmailKind, magic_link: bool) -> Result<(), String> {
        let at = format!("[auth.local.email.{}]", kind.key());
        for (key, path) in [
            ("text_template", &self.text_template),
            ("html_template", &self.html_template),
        ] {
            if let Some(path) = path {
                return Err(format!(
                    "{at} {key} = {path:?} is a file that fraiseql compile reads into the \
                     schema, but this schema carries the path, not the body. Build it with \
                     fraiseql compile."
                ));
            }
        }
        if let Some(subject) = &self.subject {
            check_email_header(&format!("{at} subject"), subject)?;
            if let Some((_, name)) = placeholders_in(subject).next() {
                return Err(format!(
                    "{at} subject is sent as written, so {{{name}}} would reach the reader \
                     verbatim. Put values in the body."
                ));
            }
        }
        if self.html.is_some() && self.text.is_none() {
            return Err(format!(
                "{at} has an HTML body but no text body. Set text_template too: mail readers \
                 that do not show HTML show the text."
            ));
        }
        for (key, body) in [("text_template", &self.text), ("html_template", &self.html)] {
            if let Some(body) = body {
                check_body(&at, key, body, kind, magic_link)?;
            }
        }
        Ok(())
    }
}

fn check_body(
    at: &str,
    key: &str,
    body: &str,
    kind: EmailKind,
    magic_link: bool,
) -> Result<(), String> {
    let known = kind.placeholders();
    let uses = |wanted: &str| placeholders_in(body).any(|(_, name)| name == wanted);
    if let Some((_, name)) = placeholders_in(body).find(|(_, name)| !known.contains(name)) {
        let allowed = known.iter().map(|n| format!("{{{n}}}")).collect::<Vec<_>>().join(", ");
        return Err(format!(
            "{at} {key} has {{{name}}}, which this mail does not have. It may use {allowed}."
        ));
    }
    match kind {
        EmailKind::Otp => {
            if !uses("code") && !uses("link") {
                return Err(format!(
                    "{at} {key} must contain {{code}} or {{link}}: without either the mail \
                     cannot sign anyone in."
                ));
            }
            if uses("link") && !magic_link {
                return Err(format!(
                    "{at} {key} has {{link}}, the magic link, but [auth.local] sets no \
                     magic_link_template to build it from."
                ));
            }
        },
        EmailKind::Reset | EmailKind::Verification => {
            if !uses("link") {
                return Err(format!(
                    "{at} {key} must contain {{link}}: it is the only way to act on this mail."
                ));
            }
        },
    }
    Ok(())
}

impl super::LocalAuthConfig {
    /// Check the sender name and every `[auth.local.email]` template. The compiler and the
    /// server's boot check both call this.
    ///
    /// # Errors
    ///
    /// Returns the first problem [`check_email_header`] or [`EmailTemplate::check`] finds.
    pub fn check_email(&self) -> Result<(), String> {
        if let Some(name) = &self.email_from_name {
            check_email_header("[auth.local] email_from_name", name)?;
        }
        let Some(email) = &self.email else {
            return Ok(());
        };
        let magic_link = self.magic_link_template.is_some();
        for (kind, template) in [
            (EmailKind::Otp, &email.otp),
            (EmailKind::Reset, &email.reset),
            (EmailKind::Verification, &email.verification),
        ] {
            if let Some(template) = template {
                template.check(kind, magic_link)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "auth_email_tests.rs"]
mod tests;
