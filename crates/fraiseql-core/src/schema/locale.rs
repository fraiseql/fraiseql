//! `[locale]`: the request locale as first-class context (#1512).
//!
//! A deployment declares the locales it serves. Every request, anonymous or not, on every
//! transport, resolves to exactly one of them ([`LocaleConfig::resolve`]); the resolved tag is
//! visible to SQL as the `fraiseql.locale` setting on read transactions and separates result
//! cache entries.
//!
//! A tag reaches SQL text (a literal in a localized field's fallback chain, a collation
//! name), so its syntax is a security boundary: only the configured tags, checked for
//! well-formed BCP 47 syntax here and re-checked when a compiled schema is loaded, can ever
//! be the resolved locale. A request value is only ever *matched against* them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{FraiseQLError, Result};

/// The session setting a read transaction carries the resolved locale in.
///
/// Fixed, not configurable. It is set on **read** transactions only: a write (and so a
/// trigger or a materialized-projection refresh running inside it) never sees a locale, so a
/// stored projection cannot depend on who wrote it (`pg_tviews#193`).
pub const LOCALE_SESSION_VAR: &str = "fraiseql.locale";

/// The suffix of a localized field's translations sibling (#1513): `nameTranslations` lists
/// every allowed label of `name`.
pub const TRANSLATIONS_SUFFIX: &str = "Translations";

/// The type of a translations sibling's elements: `LocalizedString { locale value }`.
pub const LOCALIZED_STRING_TYPE: &str = "LocalizedString";

/// The input type a localized argument or input field takes: exactly one of
/// `{value: String}` (the request locale's label) or `{translations: [LocalizedStringInput!]}`.
pub const LOCALIZED_INPUT_TYPE: &str = "LocalizedInput";

/// One translation in a [`LOCALIZED_INPUT_TYPE`]: `{locale: String!, value: String}`, a `null`
/// value removing the locale's label.
pub const LOCALIZED_STRING_INPUT_TYPE: &str = "LocalizedStringInput";

/// The longest `Accept-Language` value considered. A longer header is ignored (the source
/// falls through) rather than parsed: no browser sends one, and the parse stays bounded.
pub const MAX_ACCEPT_LANGUAGE_BYTES: usize = 1024;

/// The most `Accept-Language` entries considered, in the order the client sent them.
const MAX_ACCEPT_LANGUAGE_ENTRIES: usize = 32;

/// The longest tag considered, as BCP 47's own practical limit.
const MAX_TAG_BYTES: usize = 35;

/// Where a request's locale may come from, tried in the order `resolve` lists them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum LocaleSource {
    /// An explicit request value: GraphQL `extensions.<name>`, REST `?<name>=`.
    Argument {
        /// The argument's name (`locale`).
        argument: String,
    },
    /// A request header. `Accept-Language` is parsed with its q-values; any other header
    /// carries one tag.
    Header {
        /// The header name.
        header: String,
    },
    /// A field of the enriched identity (`[identity.enrichment]`'s `map`).
    Enrichment {
        /// The enriched field.
        enrichment: String,
    },
}

/// What a transport knows about one request, for [`LocaleConfig::resolve`]. A transport
/// leaves a channel it does not have as `None` (an argument on gRPC, a header on stdio MCP).
#[derive(Clone, Copy, Default)]
pub struct LocaleInputs<'a> {
    /// The explicit argument value, by argument name.
    pub argument:   Option<&'a dyn Fn(&str) -> Option<String>>,
    /// A header value, by (case-insensitive) header name.
    pub header:     Option<&'a dyn Fn(&str) -> Option<String>>,
    /// An enriched-identity field, by field name.
    pub enrichment: Option<&'a dyn Fn(&str) -> Option<String>>,
}

/// `[locale]`, as compiled and as loaded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleConfig {
    /// The locale a request gets when no source names an allowed one. Must be allowed.
    pub default:  String,
    /// Every locale the deployment serves, in canonical BCP 47 casing (`fr-FR`).
    pub allowed:  Vec<String>,
    /// Explicit substitutes, tried before truncation: `{ "fr-CA" = "fr-FR" }`. A key may be
    /// any well-formed tag; a target must be allowed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fallback: BTreeMap<String, String>,
    /// The sources, in order. Default: the `locale` argument, then `Accept-Language`.
    #[serde(default = "default_resolve")]
    pub resolve:  Vec<LocaleSource>,
    /// Per allowed tag, the locales a localized value is looked up in, in order. Derived by
    /// [`validate`](Self::validate) from `allowed`, `fallback` and `default`; never read from
    /// a compiled file, so a hand-edited artifact cannot add a literal.
    #[serde(skip)]
    chains:       BTreeMap<String, Vec<String>>,
}

impl PartialEq for LocaleConfig {
    fn eq(&self, other: &Self) -> bool {
        // `chains` is derived from the other four.
        self.default == other.default
            && self.allowed == other.allowed
            && self.fallback == other.fallback
            && self.resolve == other.resolve
    }
}

impl Eq for LocaleConfig {}

fn default_resolve() -> Vec<LocaleSource> {
    vec![
        LocaleSource::Argument {
            argument: "locale".to_string(),
        },
        LocaleSource::Header {
            header: "Accept-Language".to_string(),
        },
    ]
}

impl LocaleConfig {
    /// A config from its four declared parts, validated and with its chains derived.
    ///
    /// # Errors
    ///
    /// As [`validate`](Self::validate).
    pub fn new(
        default: impl Into<String>,
        allowed: Vec<String>,
        fallback: BTreeMap<String, String>,
        resolve: Vec<LocaleSource>,
    ) -> Result<Self> {
        let mut config = Self {
            default: default.into(),
            allowed,
            fallback,
            resolve,
            chains: BTreeMap::new(),
        };
        config.validate()?;
        Ok(config)
    }

    /// Check the declaration and derive the per-locale chains. Run by the compiler and again
    /// when a compiled schema is loaded: a hand-written artifact must not pass what the
    /// compiler refuses.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Validation`] naming the offending value: a tag that is not
    /// well-formed BCP 47 in canonical casing, an empty or duplicated `allowed`, a `default`
    /// or `fallback` target that is not allowed, a fallback cycle, or an empty or duplicated
    /// source.
    pub fn validate(&mut self) -> Result<()> {
        if self.allowed.is_empty() {
            return Err(refuse("[locale] allowed must list at least one locale"));
        }
        for tag in &self.allowed {
            check_tag(tag, "[locale] allowed")?;
        }
        for (i, tag) in self.allowed.iter().enumerate() {
            if self.allowed[..i].contains(tag) {
                return Err(refuse(format!("[locale] allowed lists `{tag}` twice")));
            }
        }
        check_tag(&self.default, "[locale] default")?;
        if !self.allowed.contains(&self.default) {
            return Err(refuse(format!(
                "[locale] default `{}` is not in allowed ({})",
                self.default,
                self.allowed.join(", ")
            )));
        }
        for (from, to) in &self.fallback {
            check_tag(from, "[locale] fallback key")?;
            check_tag(to, "[locale] fallback target")?;
            if !self.allowed.contains(to) {
                return Err(refuse(format!(
                    "[locale] fallback `{from}` = `{to}`: the target is not in allowed ({})",
                    self.allowed.join(", ")
                )));
            }
            if from == to {
                return Err(refuse(format!("[locale] fallback `{from}` names itself")));
            }
        }
        for from in self.fallback.keys() {
            let mut seen = vec![from.as_str()];
            let mut at = from.as_str();
            while let Some(next) = self.fallback.get(at) {
                if seen.contains(&next.as_str()) {
                    seen.push(next);
                    return Err(refuse(format!(
                        "[locale] fallback has a cycle: {}",
                        seen.join(" → ")
                    )));
                }
                seen.push(next);
                at = next;
            }
        }
        for (i, source) in self.resolve.iter().enumerate() {
            let (kind, name) = match source {
                LocaleSource::Argument { argument } => ("argument", argument),
                LocaleSource::Header { header } => ("header", header),
                LocaleSource::Enrichment { enrichment } => ("enrichment", enrichment),
            };
            if name.is_empty()
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(refuse(format!(
                    "[locale] resolve: {kind} `{name}` must be a non-empty name of letters, \
                     digits, `_` or `-`"
                )));
            }
            if self.resolve[..i].contains(source) {
                return Err(refuse(format!("[locale] resolve lists {kind} `{name}` twice")));
            }
        }
        self.chains =
            self.allowed.iter().map(|tag| (tag.clone(), self.derive_chain(tag))).collect();
        Ok(())
    }

    /// The locales a localized value is read from for a request in `locale`, most preferred
    /// first: the tag, its explicit fallbacks, its truncations that are allowed, the default.
    /// Every entry is an allowed tag. `None` for a tag that is not allowed (which no resolved
    /// locale is).
    #[must_use]
    pub fn chain(&self, locale: &str) -> Option<&[String]> {
        self.chains.get(locale).map(Vec::as_slice)
    }

    /// The ICU collation a text sort uses in `locale`: `{locale}-x-icu`, the name
    /// PostgreSQL gives the collation it creates for that ICU locale. `None` for a tag that is
    /// not allowed. The server checks at boot that each allowed tag's collation exists.
    #[must_use]
    pub fn collation(&self, locale: &str) -> Option<String> {
        self.allowed.iter().any(|a| a == locale).then(|| format!("{locale}-x-icu"))
    }

    /// Every allowed tag's collation, for the boot check.
    pub fn collations(&self) -> impl Iterator<Item = (&str, String)> {
        self.allowed.iter().map(|tag| (tag.as_str(), format!("{tag}-x-icu")))
    }

    /// The enriched-identity fields `resolve` reads, for the server's boot check.
    pub fn enrichment_fields(&self) -> impl Iterator<Item = &str> {
        self.resolve.iter().filter_map(|s| match s {
            LocaleSource::Enrichment { enrichment } => Some(enrichment.as_str()),
            _ => None,
        })
    }

    /// The locale a request resolves to: always one of `allowed`.
    ///
    /// Each source in `resolve` order offers candidates (an `Accept-Language` header several,
    /// by q-value); the first candidate that [matches](Self::match_tag) wins. A source with
    /// no match, a malformed value, `*`, or an over-long header falls through to the next;
    /// when none matches, the request gets `default`.
    #[must_use]
    pub fn resolve(&self, inputs: &LocaleInputs<'_>) -> &str {
        for source in &self.resolve {
            let candidates: Vec<String> = match source {
                LocaleSource::Argument { argument } => {
                    inputs.argument.and_then(|f| f(argument)).into_iter().collect()
                },
                LocaleSource::Header { header } => {
                    let Some(value) = inputs.header.and_then(|f| f(header)) else {
                        continue;
                    };
                    if header.eq_ignore_ascii_case("accept-language") {
                        accept_language(&value)
                    } else {
                        vec![value]
                    }
                },
                LocaleSource::Enrichment { enrichment } => {
                    inputs.enrichment.and_then(|f| f(enrichment)).into_iter().collect()
                },
            };
            if let Some(tag) = candidates.iter().find_map(|c| self.match_tag(c)) {
                return tag;
            }
        }
        &self.default
    }

    /// The allowed tag a requested one stands for, per RFC 4647 lookup: the tag itself
    /// (case-insensitively), else its `fallback` entry, else the same for each truncation
    /// (`fr-BE` → `fr`). `None` when nothing matches or the value is not tag-shaped.
    #[must_use]
    pub fn match_tag(&self, requested: &str) -> Option<&str> {
        let requested = requested.trim();
        if requested.is_empty()
            || requested.len() > MAX_TAG_BYTES
            || !requested.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return None;
        }
        let mut candidate = requested;
        loop {
            if let Some(tag) = self.allowed.iter().find(|t| t.eq_ignore_ascii_case(candidate)) {
                return Some(tag);
            }
            if let Some((_, to)) =
                self.fallback.iter().find(|(from, _)| from.eq_ignore_ascii_case(candidate))
            {
                return Some(to);
            }
            candidate = truncate(candidate)?;
        }
    }

    pub(crate) fn derive_chain(&self, tag: &str) -> Vec<String> {
        let mut chain: Vec<String> = vec![tag.to_string()];
        let mut push = |t: &str| {
            if !chain.iter().any(|c| c == t) {
                chain.push(t.to_string());
            }
        };
        let mut at = tag;
        while let Some(next) = self.fallback.get(at) {
            push(next);
            at = next;
        }
        let mut shorter = truncate(tag);
        while let Some(t) = shorter {
            if self.allowed.iter().any(|a| a == t) {
                push(t);
            }
            shorter = truncate(t);
        }
        push(&self.default);
        chain
    }
}

/// `tag` with its last subtag removed, and a singleton left dangling by that removed too
/// (RFC 4647 §3.4). `None` for a single subtag.
fn truncate(tag: &str) -> Option<&str> {
    let mut shorter = &tag[..tag.rfind('-')?];
    if let Some(i) = shorter.rfind('-') {
        if shorter.len() - i == 2 {
            shorter = &shorter[..i];
        }
    }
    Some(shorter)
}

/// The tags an `Accept-Language` value asks for, best first: `q=0` and `*` dropped,
/// equal weights kept in the order sent. An over-long value asks for nothing.
fn accept_language(value: &str) -> Vec<String> {
    if value.len() > MAX_ACCEPT_LANGUAGE_BYTES {
        return Vec::new();
    }
    let mut weighted: Vec<(u16, String)> = value
        .split(',')
        .take(MAX_ACCEPT_LANGUAGE_ENTRIES)
        .filter_map(|entry| {
            let mut parts = entry.split(';');
            let tag = parts.next()?.trim();
            let mut q = 1000;
            for param in parts {
                let (key, val) = param.split_once('=')?;
                if key.trim().eq_ignore_ascii_case("q") {
                    q = parse_qvalue(val.trim())?;
                }
            }
            (q > 0 && !tag.is_empty() && tag != "*").then(|| (q, tag.to_string()))
        })
        .collect();
    weighted.sort_by_key(|(q, _)| std::cmp::Reverse(*q));
    weighted.into_iter().map(|(_, tag)| tag).collect()
}

/// An RFC 9110 qvalue in thousandths: `0`, `0.5`, `1.000`. `None` when malformed.
fn parse_qvalue(text: &str) -> Option<u16> {
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    if frac.len() > 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let frac_value = format!("{frac:0<3}").parse::<u16>().ok()?;
    match int {
        "0" => Some(frac_value),
        "1" if frac_value == 0 => Some(1000),
        _ => None,
    }
}

/// Refuse `tag` unless it is well-formed BCP 47 (language, then optional script, region and
/// variants; no extensions or private use) in canonical casing.
fn check_tag(tag: &str, what: &str) -> Result<()> {
    match canonical_tag(tag) {
        Some(canonical) if canonical == tag => Ok(()),
        Some(canonical) => Err(refuse(format!(
            "{what} `{tag}` must be written in canonical casing: `{canonical}`"
        ))),
        None => Err(refuse(format!(
            "{what} `{tag}` is not a well-formed BCP 47 language tag (language, then \
             optional script, region and variants, e.g. `fr`, `fr-FR`, `zh-Hant-TW`)"
        ))),
    }
}

/// The canonical casing of a well-formed `language[-script][-region][-variant]*` tag, or
/// `None` when `tag` is not one.
fn canonical_tag(tag: &str) -> Option<String> {
    if tag.is_empty() || tag.len() > MAX_TAG_BYTES {
        return None;
    }
    let mut subtags = tag.split('-').peekable();
    let language = subtags.next()?;
    if !(matches!(language.len(), 2 | 3 | 5..=8)
        && language.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        return None;
    }
    let mut out = vec![language.to_ascii_lowercase()];
    if let Some(script) =
        subtags.next_if(|s| s.len() == 4 && s.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        let mut chars = script.chars();
        let first = chars.next()?.to_ascii_uppercase();
        out.push(format!("{first}{}", chars.as_str().to_ascii_lowercase()));
    }
    if let Some(region) = subtags.next_if(|s| {
        (s.len() == 2 && s.bytes().all(|b| b.is_ascii_alphabetic()))
            || (s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
    }) {
        out.push(region.to_ascii_uppercase());
    }
    for variant in subtags {
        let alnum = variant.bytes().all(|b| b.is_ascii_alphanumeric());
        let ok = alnum
            && (matches!(variant.len(), 5..=8)
                || (variant.len() == 4 && variant.as_bytes()[0].is_ascii_digit()));
        if !ok {
            return None;
        }
        out.push(variant.to_ascii_lowercase());
    }
    Some(out.join("-"))
}

fn refuse(message: impl Into<String>) -> FraiseQLError {
    FraiseQLError::Validation {
        message: message.into(),
        path:    Some("locale".to_string()),
    }
}

#[cfg(test)]
mod tests;
