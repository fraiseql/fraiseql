//! `fraiseql analyze` — recommendations derived from a compiled schema.
//!
//! Usage: `fraiseql analyze <schema.compiled.json> [--json]`
//!
//! # Why this was rewritten
//!
//! Every line of the previous implementation was a literal. The schema was read,
//! parsed into a binding named `_schema`, and discarded; the `security` bucket then
//! stated as fact that "Rate limiting configured and active", "Audit logging enabled
//! for compliance" and "Error sanitization prevents information leakage", and
//! `health_score` was `(categories_count * 20).min(100)` over a constant six
//! categories — so it was 100 for every possible input, including an empty `{}` and a
//! schema that explicitly disabled both controls. An operator or agent running this
//! pre-deploy received an affirmative security attestation about a deployment that
//! had none (#818).
//!
//! The published machine contract (`--show-output-schema analyze`) described a
//! `recommendations` array of `{category, severity, message, suggestion}` that the
//! command never emitted. That contract is the right shape, so the implementation now
//! matches it rather than the other way round.
//!
//! # What it can and cannot see
//!
//! Everything here is derived from the compiled schema alone. Anything that needs the
//! live database — whether an index exists, whether an RLS policy is actually
//! installed — is out of reach, so it is not claimed. `fraiseql doctor --against-db`
//! is the command that inspects a real database.

use std::fs;

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;
use serde::Serialize;

use crate::output::CommandResult;

/// How much a finding matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A security control that is declared off, or absent.
    Warning,
    /// A tuning or structural observation.
    Info,
}

/// One finding about the analysed schema.
#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    /// One of the advertised categories.
    pub category:   &'static str,
    /// Severity of the finding.
    pub severity:   Severity,
    /// What was observed — a statement about *this* schema.
    pub message:    String,
    /// What to do about it.
    pub suggestion: String,
}

/// Counts an operator can check against their own expectations.
#[derive(Debug, Serialize)]
pub struct SchemaFacts {
    /// Object types.
    pub types:         usize,
    /// Fields across all object types.
    pub fields:        usize,
    /// Query root fields.
    pub queries:       usize,
    /// Mutation root fields.
    pub mutations:     usize,
    /// Subscription root fields.
    pub subscriptions: usize,
    /// Whether the schema declares multi-tenancy.
    pub multi_tenant:  bool,
}

/// Summary statistics from analysis.
#[derive(Debug, Serialize)]
pub struct AnalysisSummary {
    /// Total recommendations emitted.
    pub total_recommendations: usize,
    /// How many of them are warnings.
    pub warnings:              usize,
    /// Overall schema health (0–100), computed from the findings.
    pub health_score:          usize,
}

/// Analysis result.
#[derive(Debug, Serialize)]
pub struct AnalysisResult {
    /// Path to the analysed schema.
    pub schema_file:     String,
    /// Findings, in the shape `--show-output-schema analyze` publishes.
    pub recommendations: Vec<Recommendation>,
    /// The counts the findings were derived from.
    pub facts:           SchemaFacts,
    /// Summary statistics.
    pub summary:         AnalysisSummary,
}

/// Run the analyze command.
///
/// # Errors
///
/// Returns an error if the schema file cannot be read, or is not a compiled schema.
pub fn run(schema_path: &str) -> Result<CommandResult> {
    let contents = fs::read_to_string(schema_path)
        .with_context(|| format!("cannot read schema file `{schema_path}`"))?;

    // Parsed into the real type, not `serde_json::Value`. The previous version
    // accepted `{}` and reported on it, which is how an empty file scored 100/100.
    let schema: CompiledSchema = serde_json::from_str(&contents).with_context(|| {
        format!(
            "`{schema_path}` is not a compiled schema. Run `fraiseql compile` first and pass the \
             resulting schema.compiled.json."
        )
    })?;

    let facts = collect_facts(&schema);
    let mut recommendations = security_findings(&schema);
    recommendations.extend(complexity_findings(&facts));
    recommendations.extend(authorization_findings(&schema));

    let warnings = recommendations.iter().filter(|r| r.severity == Severity::Warning).count();
    // Each warning is a control declared off or absent. A schema with none scores 100,
    // and the score falls with what is actually missing — so two schemas with different
    // security postures cannot produce the same number, which is what made the old
    // constant worthless.
    let health_score = 100_usize.saturating_sub(warnings.saturating_mul(10)).max(10);

    let analysis = AnalysisResult {
        schema_file: schema_path.to_string(),
        summary: AnalysisSummary {
            total_recommendations: recommendations.len(),
            warnings,
            health_score,
        },
        recommendations,
        facts,
    };

    Ok(CommandResult::success("analyze", serde_json::to_value(&analysis)?))
}

fn collect_facts(schema: &CompiledSchema) -> SchemaFacts {
    SchemaFacts {
        types:         schema.types.len(),
        fields:        schema.types.iter().map(|t| t.fields.len()).sum(),
        queries:       schema.queries.len(),
        mutations:     schema.mutations.len(),
        subscriptions: schema.subscriptions.len(),
        multi_tenant:  schema.is_multi_tenant(),
    }
}

fn security_findings(schema: &CompiledSchema) -> Vec<Recommendation> {
    const CATEGORY: &str = "security";
    let mut out = Vec::new();

    // `None` means the section is absent, which is materially different from a
    // section present with `enabled = false` — "never configured" versus
    // "deliberately turned off" — and both differ from on. These are the typed
    // `SecurityConfig` fields (#977); the previous string lookup read an
    // `audit_logging` key no producer ever wrote, so that recommendation could
    // never report "enabled" — audit logging actually lives on
    // `enterprise.audit_logging_enabled`.
    let sec = schema.security.as_ref();
    for (flag, label, suggestion) in [
        (
            sec.and_then(|s| s.rate_limiting.as_ref()).map(|c| c.enabled),
            "Rate limiting",
            "Add [fraiseql.security.rate_limiting] with enabled = true to throttle per-IP \
             request rates.",
        ),
        (
            sec.and_then(|s| s.enterprise.as_ref()).map(|e| e.audit_logging_enabled),
            "Audit logging",
            "Add [fraiseql.security.enterprise] with audit_logging_enabled = true to record \
             security events.",
        ),
        (
            sec.and_then(|s| s.error_sanitization.as_ref()).map(|c| c.enabled),
            "Error sanitization",
            "Add [fraiseql.security.error_sanitization] with enabled = true so internal error \
             detail is not returned to clients.",
        ),
    ] {
        out.push(match flag {
            Some(true) => Recommendation {
                category:   CATEGORY,
                severity:   Severity::Info,
                message:    format!("{label} is enabled in the compiled schema."),
                suggestion: "No action needed.".to_string(),
            },
            Some(false) => Recommendation {
                category:   CATEGORY,
                severity:   Severity::Warning,
                message:    format!("{label} is present in the compiled schema but disabled."),
                suggestion: suggestion.to_string(),
            },
            None => Recommendation {
                category:   CATEGORY,
                severity:   Severity::Warning,
                message:    format!("{label} is not configured in this schema."),
                suggestion: suggestion.to_string(),
            },
        });
    }

    if schema.is_multi_tenant() {
        let rls_declared = schema.security.as_ref().is_some_and(|s| s.rls.enabled);
        out.push(if rls_declared {
            Recommendation {
                category:   CATEGORY,
                severity:   Severity::Info,
                message:    "Schema is multi-tenant and declares row-level security.".to_string(),
                suggestion: "The server verifies the declaration against the live database at \
                             boot; `fraiseql doctor --against-db` checks it ahead of deployment."
                    .to_string(),
            }
        } else {
            Recommendation {
                category:   CATEGORY,
                severity:   Severity::Warning,
                message:    "Schema is multi-tenant but declares no row-level security."
                    .to_string(),
                suggestion: "Declare [security.rls] and install the matching policies; without \
                             them tenant isolation rests on query construction alone."
                    .to_string(),
            }
        });
    }

    out
}

fn complexity_findings(facts: &SchemaFacts) -> Vec<Recommendation> {
    const CATEGORY: &str = "complexity";
    let mut out = vec![Recommendation {
        category:   CATEGORY,
        severity:   Severity::Info,
        message:    format!(
            "{} type(s), {} field(s), {} query root field(s), {} mutation(s), {} \
             subscription(s).",
            facts.types, facts.fields, facts.queries, facts.mutations, facts.subscriptions
        ),
        suggestion: "Compare against what you expect this deployment to expose.".to_string(),
    }];

    if facts.types == 0 {
        out.push(Recommendation {
            category:   CATEGORY,
            severity:   Severity::Warning,
            message:    "The schema declares no object types.".to_string(),
            suggestion: "Check that the compile step picked up your type definitions.".to_string(),
        });
    }

    out
}

/// Report how much of the schema is behind a role gate.
///
/// Deliberately *not* a "caching" bucket: nothing in the compiled schema expresses a
/// per-type cache TTL, so any statement about caching would have to be invented — and
/// inventing statements is the defect this rewrite removes. Role coverage is
/// derivable, so it is what gets reported.
fn authorization_findings(schema: &CompiledSchema) -> Vec<Recommendation> {
    let gated = schema.types.iter().filter(|t| t.requires_role.is_some()).count();
    let roles = schema.security.as_ref().map_or(0, |s| s.role_definitions.len());

    vec![Recommendation {
        category:   "security",
        severity:   Severity::Info,
        message:    format!(
            "{gated} of {} type(s) declare `requires_role`; {roles} role definition(s) are              declared.",
            schema.types.len()
        ),
        suggestion: if gated == 0 {
            "Types with no `requires_role` are reachable by any authenticated caller; gate the              ones that should not be."
                .to_string()
        } else {
            "Check that every gated type's role is one the identity provider actually issues."
                .to_string()
        },
    }]
}

#[cfg(test)]
mod tests;
