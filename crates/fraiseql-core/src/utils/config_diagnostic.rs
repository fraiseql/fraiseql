//! A refused configuration, as data: a stable code, the file, the table and key it names,
//! and what to write instead.
//!
//! The project configuration (`fraiseql.toml`) and the server's configuration file refuse
//! a spelling with the same codes, so a tool driving `fraiseql` can tell a refused
//! configuration from any other failure, and find the key, without parsing the message.

use std::fmt;

use serde::Serialize;

/// The kind of a configuration refusal. Stable: tools match on [`Self::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DiagnosticCode {
    /// A spelling the file no longer takes; the replacement names the one it takes.
    #[serde(rename = "E-CONFIG-RETIRED")]
    Retired,
    /// A key nothing reads: refused rather than accepted and ignored. No replacement.
    #[serde(rename = "E-CONFIG-UNREAD")]
    Unread,
    /// A key a command never applied.
    #[serde(rename = "E-CONFIG-UNAPPLIED")]
    Unapplied,
    /// A setting written in the wrong file or table.
    #[serde(rename = "E-CONFIG-MISPLACED")]
    Misplaced,
    /// Two sources declare one setting and differ.
    #[serde(rename = "E-CONFIG-DISAGREES")]
    Disagrees,
}

impl DiagnosticCode {
    /// Every code, in the order the documentation lists them.
    pub const ALL: [Self; 5] = [
        Self::Retired,
        Self::Unread,
        Self::Unapplied,
        Self::Misplaced,
        Self::Disagrees,
    ];

    /// The code as tools see it, such as `E-CONFIG-RETIRED`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Retired => "E-CONFIG-RETIRED",
            Self::Unread => "E-CONFIG-UNREAD",
            Self::Unapplied => "E-CONFIG-UNAPPLIED",
            Self::Misplaced => "E-CONFIG-MISPLACED",
            Self::Disagrees => "E-CONFIG-DISAGREES",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A configuration refusal. Displays as its message, so the prose is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigDiagnostic {
    /// What kind of refusal this is.
    pub code:        DiagnosticCode,
    /// The file refused, once the loader that read it knows its path.
    pub file:        Option<String>,
    /// The table, dotted (`security.rate_limiting`); empty for the top level.
    pub table:       String,
    /// The key, or `None` when the whole table is refused.
    pub key:         Option<String>,
    /// What to write instead, when there is something to write.
    pub replacement: Option<String>,
    /// For a misplaced setting, the file it belongs in, when that is another file.
    pub belongs_in:  Option<String>,
    /// The refusal as a person reads it.
    #[serde(skip)]
    pub message:     String,
}

impl ConfigDiagnostic {
    /// A refusal of `table` (and `key`, when not empty), with no file yet.
    #[must_use]
    pub fn new(code: DiagnosticCode, table: &str, key: &str, message: String) -> Self {
        Self {
            code,
            file: None,
            table: table.to_owned(),
            key: (!key.is_empty()).then(|| key.to_owned()),
            replacement: None,
            belongs_in: None,
            message,
        }
    }

    /// The same refusal, naming what to write instead.
    #[must_use]
    pub fn with_replacement(mut self, replacement: &str) -> Self {
        self.replacement = Some(replacement.to_owned());
        self
    }

    /// The same refusal, naming the file the setting belongs in.
    #[must_use]
    pub fn with_belongs_in(mut self, file: &str) -> Self {
        self.belongs_in = Some(file.to_owned());
        self
    }
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConfigDiagnostic {}

#[cfg(test)]
mod tests;
