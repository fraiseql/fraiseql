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
