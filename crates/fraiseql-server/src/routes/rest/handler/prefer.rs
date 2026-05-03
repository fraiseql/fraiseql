//! Prefer header parsing and preference types.
//!
//! Implements RFC 7240 preferences for REST operations: `count=`, `return=`,
//! `resolution=`, `tx=`, `handling=`, and `max-affected=`.

use axum::http::HeaderMap;

/// Count preference mode for collection queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CountPreference {
    /// `count=exact` — execute a parallel `SELECT COUNT(*)` query.
    Exact,
    /// `count=planned` — extract row estimate from `EXPLAIN` output (PostgreSQL).
    Planned,
    /// `count=estimated` — read `n_live_tup` from `pg_stat_user_tables` (PostgreSQL).
    Estimated,
}

/// Handling preference (RFC 7240 §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HandlingPreference {
    /// Unknown parameters/preferences are silently ignored.
    Lenient,
    /// Unknown parameters cause a 400 Bad Request.
    Strict,
}

/// Parsed `Prefer` header values relevant to REST transport (RFC 7240).
#[derive(Debug, Clone, Default)]
pub struct PreferHeader {
    /// `count=exact` — execute a parallel COUNT query.
    pub count_exact:           bool,
    /// `count=planned` — EXPLAIN-based estimate (PostgreSQL).
    pub count_planned:         bool,
    /// `count=estimated` — `pg_stats` estimate (PostgreSQL).
    pub count_estimated:       bool,
    /// `return=representation` — return entity body on mutating operations.
    pub return_representation: bool,
    /// `return=minimal` — return empty body on mutating operations.
    pub return_minimal:        bool,
    /// `resolution=merge-duplicates` or `resolution=ignore-duplicates` — upsert mode.
    pub resolution:            Option<String>,
    /// `tx=rollback` — dry-run mode (execute then rollback).
    pub tx_rollback:           bool,
    /// `handling=strict` or `handling=lenient` (default: strict).
    pub handling:              Option<HandlingPreference>,
    /// `max-affected=N` — limit bulk operation scope.
    pub max_affected:          Option<u64>,
}

impl PreferHeader {
    /// Return the active count preference, if any.
    #[must_use]
    pub const fn count_preference(&self) -> Option<CountPreference> {
        if self.count_exact {
            Some(CountPreference::Exact)
        } else if self.count_planned {
            Some(CountPreference::Planned)
        } else if self.count_estimated {
            Some(CountPreference::Estimated)
        } else {
            None
        }
    }

    /// Collect all applied preferences as a comma-separated header value.
    #[must_use]
    pub fn applied_header_value(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.count_exact {
            parts.push("count=exact");
        } else if self.count_planned {
            parts.push("count=planned");
        } else if self.count_estimated {
            parts.push("count=estimated");
        }
        if self.return_representation {
            parts.push("return=representation");
        } else if self.return_minimal {
            parts.push("return=minimal");
        }
        if let Some(ref res) = self.resolution {
            // Handled separately since it needs the value
            let _ = res;
        }
        if self.tx_rollback {
            parts.push("tx=rollback");
        }
        if self.handling == Some(HandlingPreference::Strict) {
            parts.push("handling=strict");
        } else if self.handling == Some(HandlingPreference::Lenient) {
            parts.push("handling=lenient");
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }
}

impl PreferHeader {
    /// Parse a `Prefer` header value (RFC 7240).
    ///
    /// Supports `count=exact|planned|estimated`, `return=representation|minimal`,
    /// `resolution=merge-duplicates|ignore-duplicates`, `tx=rollback|commit`,
    /// `handling=strict|lenient`, and `max-affected=N`.
    /// Unknown preferences are silently ignored per RFC 7240.
    #[must_use]
    pub fn parse(header_value: &str) -> Self {
        let mut result = Self::default();
        for pref in header_value.split(',') {
            let pref = pref.trim();
            if pref.eq_ignore_ascii_case("count=exact") {
                result.count_exact = true;
                result.count_planned = false;
                result.count_estimated = false;
            } else if pref.eq_ignore_ascii_case("count=planned") {
                result.count_planned = true;
                result.count_exact = false;
                result.count_estimated = false;
            } else if pref.eq_ignore_ascii_case("count=estimated") {
                result.count_estimated = true;
                result.count_exact = false;
                result.count_planned = false;
            } else if pref.eq_ignore_ascii_case("return=representation") {
                result.return_representation = true;
                result.return_minimal = false;
            } else if pref.eq_ignore_ascii_case("return=minimal") {
                result.return_minimal = true;
                result.return_representation = false;
            } else if pref.eq_ignore_ascii_case("tx=rollback") {
                result.tx_rollback = true;
            } else if pref.eq_ignore_ascii_case("tx=commit") {
                // Default behavior — acknowledged but no-op.
                result.tx_rollback = false;
            } else if pref.eq_ignore_ascii_case("handling=strict") {
                result.handling = Some(HandlingPreference::Strict);
            } else if pref.eq_ignore_ascii_case("handling=lenient") {
                result.handling = Some(HandlingPreference::Lenient);
            } else if let Some(val) = strip_prefix_ci(pref, "resolution=") {
                result.resolution = Some(val.to_string());
            } else if let Some(val) = strip_prefix_ci(pref, "max-affected=") {
                if let Ok(n) = val.parse::<u64>() {
                    result.max_affected = Some(n);
                }
            }
            // Unknown preferences silently ignored (per RFC 7240 §2)
        }
        result
    }

    /// Parse from a header map (handles missing and multiple Prefer headers).
    #[must_use]
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let mut result = Self::default();
        for value in headers.get_all("prefer") {
            if let Ok(s) = value.to_str() {
                let parsed = Self::parse(s);
                // Count: last-write-wins (mutually exclusive)
                if parsed.count_exact {
                    result.count_exact = true;
                    result.count_planned = false;
                    result.count_estimated = false;
                } else if parsed.count_planned {
                    result.count_planned = true;
                    result.count_exact = false;
                    result.count_estimated = false;
                } else if parsed.count_estimated {
                    result.count_estimated = true;
                    result.count_exact = false;
                    result.count_planned = false;
                }
                // Return: last-write-wins (mutually exclusive)
                if parsed.return_representation {
                    result.return_representation = true;
                    result.return_minimal = false;
                }
                if parsed.return_minimal {
                    result.return_minimal = true;
                    result.return_representation = false;
                }
                if parsed.tx_rollback {
                    result.tx_rollback = true;
                }
                if parsed.handling.is_some() {
                    result.handling = parsed.handling;
                }
                if parsed.resolution.is_some() {
                    result.resolution = parsed.resolution;
                }
                if parsed.max_affected.is_some() {
                    result.max_affected = parsed.max_affected;
                }
            }
        }
        result
    }
}

/// Case-insensitive prefix strip.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefer_parse_count_exact() {
        let prefer = PreferHeader::parse("count=exact");
        assert!(prefer.count_exact);
        assert!(!prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_return_representation() {
        let prefer = PreferHeader::parse("return=representation");
        assert!(!prefer.count_exact);
        assert!(prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_return_minimal() {
        let prefer = PreferHeader::parse("return=minimal");
        assert!(!prefer.count_exact);
        assert!(!prefer.return_representation);
        assert!(prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_combined() {
        let prefer = PreferHeader::parse("count=exact, return=representation");
        assert!(prefer.count_exact);
        assert!(prefer.return_representation);
        assert!(!prefer.return_minimal);
    }

    #[test]
    fn prefer_parse_case_insensitive() {
        let prefer = PreferHeader::parse("Count=Exact");
        assert!(prefer.count_exact);
    }

    #[test]
    fn prefer_parse_unknown_ignored() {
        let prefer = PreferHeader::parse("respond-async, count=exact");
        assert!(prefer.count_exact);
    }

    #[test]
    fn prefer_minimal_overrides_representation() {
        let prefer = PreferHeader::parse("return=representation, return=minimal");
        assert!(prefer.return_minimal);
        assert!(!prefer.return_representation);
    }

    #[test]
    fn prefer_from_headers_multiple() {
        let mut headers = axum::http::HeaderMap::new();
        headers.append("prefer", axum::http::HeaderValue::from_static("count=exact"));
        headers.append("prefer", axum::http::HeaderValue::from_static("return=representation"));
        let prefer = PreferHeader::from_headers(&headers);
        assert!(prefer.count_exact);
        assert!(prefer.return_representation);
    }

    #[test]
    fn prefer_parse_resolution_merge() {
        let prefer = PreferHeader::parse("resolution=merge-duplicates");
        assert_eq!(prefer.resolution.as_deref(), Some("merge-duplicates"));
    }

    #[test]
    fn prefer_parse_resolution_ignore() {
        let prefer = PreferHeader::parse("resolution=ignore-duplicates");
        assert_eq!(prefer.resolution.as_deref(), Some("ignore-duplicates"));
    }

    #[test]
    fn prefer_parse_tx_rollback() {
        let prefer = PreferHeader::parse("tx=rollback");
        assert!(prefer.tx_rollback);
    }

    #[test]
    fn prefer_parse_tx_commit() {
        let prefer = PreferHeader::parse("tx=commit");
        assert!(!prefer.tx_rollback);
    }

    #[test]
    fn prefer_parse_handling_strict() {
        let prefer = PreferHeader::parse("handling=strict");
        assert_eq!(prefer.handling, Some(HandlingPreference::Strict));
    }

    #[test]
    fn prefer_parse_handling_lenient() {
        let prefer = PreferHeader::parse("handling=lenient");
        assert_eq!(prefer.handling, Some(HandlingPreference::Lenient));
    }

    #[test]
    fn prefer_parse_max_affected() {
        let prefer = PreferHeader::parse("max-affected=100");
        assert_eq!(prefer.max_affected, Some(100));
    }

    #[test]
    fn prefer_count_preference_exact() {
        let prefer = PreferHeader::parse("count=exact");
        assert_eq!(prefer.count_preference(), Some(CountPreference::Exact));
    }

    #[test]
    fn prefer_count_preference_none() {
        let prefer = PreferHeader::parse("return=minimal");
        assert_eq!(prefer.count_preference(), None);
    }

    #[test]
    fn prefer_applied_header_value() {
        let prefer = PreferHeader::parse("count=exact, return=representation");
        let value = prefer.applied_header_value();
        assert!(value.is_some());
        let value_str = value.unwrap();
        assert!(value_str.contains("count=exact"));
        assert!(value_str.contains("return=representation"));
    }
}
