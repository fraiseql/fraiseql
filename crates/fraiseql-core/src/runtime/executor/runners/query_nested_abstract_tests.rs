//! A nested level of a union or an interface, gated as a read of every type it can be.
//!
//! `{ ledgers { holder { secret } } }` reads `Bank` documents through `Ledger.holder`, an
//! interface field. Each gate a read of `Bank` applies — a field's scope, the type's role,
//! the authorizer's decision on the type, a policy-gated field, the type's row security —
//! applies to that level, as it does where `Ledger.bank` names the type itself.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;

use super::{DocumentRowFilter, SelectionAccess, guard_relation_filters, type_read_refusal};
use crate::{
    backend::{WhereClause, WhereOperator, types::JsonbValue},
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::{
        RuntimeConfig,
        executor::{Executor, test_support::CapturingMockAdapter},
    },
    schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, InjectedParamSource,
        InterfaceDefinition, QueryDefinition, TypeDefinition, UnionDefinition,
    },
    security::{Authorizer, AuthzDecision, AuthzRequest, SecurityContext},
};

/// A field `name` of type `String` that needs `read:secret`, refused or masked by `on_deny`.
fn scoped(name: &str, on_deny: FieldDenyPolicy) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, FieldType::String);
    field.requires_scope = Some("read:secret".to_string());
    field.on_deny = on_deny;
    field
}

/// `ledgers` over `v_ledger`. A `Ledger` holds a `holder`, of the interface `Holder` that
/// `Bank` implements, and a `party`, of the union `Party` of `Org` and `Person`. `Bank.secret`
/// and `Org.credit` need `read:secret`; a `Bank` has a `branch`.
fn schema_with(on_deny: FieldDenyPolicy, edit: impl FnOnce(&mut CompiledSchema)) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.queries.push(
        QueryDefinition::new("ledgers", "Ledger")
            .returning_list()
            .with_sql_source("v_ledger"),
    );
    let mut ledger = TypeDefinition::new("Ledger", "v_ledger");
    ledger.fields = vec![
        FieldDefinition::nullable("holder", FieldType::Interface("Holder".to_string())),
        FieldDefinition::nullable("party", FieldType::Union("Party".to_string())),
    ];
    let mut bank = TypeDefinition::new("Bank", "v_bank");
    bank.fields = vec![
        FieldDefinition::new("name", FieldType::String),
        scoped("secret", on_deny),
        FieldDefinition::nullable("branch", FieldType::Object("Branch".to_string())),
    ];
    let mut branch = TypeDefinition::new("Branch", "v_branch");
    branch.fields = vec![FieldDefinition::new("name", FieldType::String)];
    bank.implements = vec!["Holder".to_string()];
    let mut org = TypeDefinition::new("Org", "v_org");
    org.fields = vec![
        FieldDefinition::new("name", FieldType::String),
        scoped("credit", on_deny),
    ];
    let mut person = TypeDefinition::new("Person", "v_person");
    // `Person.credit` needs no scope: the same field of another member.
    person.fields = vec![
        FieldDefinition::new("name", FieldType::String),
        FieldDefinition::new("credit", FieldType::String),
    ];
    schema.types = vec![ledger, bank, branch, org, person];
    schema.interfaces.push(
        InterfaceDefinition::new("Holder")
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(scoped("secret", on_deny)),
    );
    let mut party = UnionDefinition::new("Party");
    party.member_types = vec!["Org".to_string(), "Person".to_string()];
    schema.unions.push(party);
    edit(&mut schema);
    schema
}

fn schema(on_deny: FieldDenyPolicy) -> CompiledSchema {
    schema_with(on_deny, |_| {})
}

fn executor_with(
    schema: CompiledSchema,
    config: RuntimeConfig,
) -> (Executor, Arc<CapturingMockAdapter>) {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![JsonbValue::new(serde_json::json!({
        "holder": {"__typename": "Bank", "name": "b", "secret": "TOP"},
        "party": {"__typename": "Org", "name": "o", "credit": "TOP"},
    }))]));
    (Executor::with_config(schema, adapter.clone(), config), adapter)
}

fn executor(schema: CompiledSchema) -> (Executor, Arc<CapturingMockAdapter>) {
    executor_with(schema, RuntimeConfig::default())
}

/// A principal holding the role `viewer`, which grants no scope.
fn unscoped() -> SecurityContext {
    SecurityContext {
        user_id:          "user-42".into(),
        roles:            vec!["viewer".to_string()],
        tenant_id:        Some("tenant-abc".into()),
        scopes:           Vec::new(),
        attributes:       HashMap::from([(
            "tenant_id".to_string(),
            serde_json::json!("tenant-abc"),
        )]),
        request_id:       "req-abstract".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// `document` read anonymously, then by an unscoped principal.
async fn both_callers(exec: &Executor, document: &str) -> [Result<serde_json::Value>; 2] {
    [
        exec.execute(document, None).await,
        exec.execute_with_security(document, None, &unscoped()).await,
    ]
}

/// The selections below the root of `document`.
fn root_fields(exec: &Executor, document: &str) -> Vec<FieldSelection> {
    let query_match = exec.matcher().match_query(document, None).unwrap();
    query_match.selections[0].nested_fields.clone()
}

const SCOPED_READS: [&str; 3] = [
    "{ ledgers { holder { secret } } }",
    "{ ledgers { holder { ... on Bank { secret } } } }",
    "{ ledgers { party { ... on Org { credit } } } }",
];

#[tokio::test]
async fn a_rejected_field_of_a_member_refuses_the_read_before_it_reaches_the_database() {
    for document in SCOPED_READS {
        let (exec, adapter) = executor(schema(FieldDenyPolicy::Reject));
        for result in both_callers(&exec, document).await {
            let err = result.expect_err(document);
            assert!(matches!(err, FraiseQLError::Authorization { .. }), "{document}: {err}");
        }
        assert!(adapter.captured_where().is_none(), "{document} was read");
    }
}

/// A member's gated field refuses only a selection that reaches it.
#[tokio::test]
async fn a_selection_that_reaches_no_gated_field_of_a_member_is_answered() {
    let (exec, _) = executor(schema(FieldDenyPolicy::Reject));
    for result in
        both_callers(&exec, "{ ledgers { party { ... on Person { name credit } } } }").await
    {
        let response = result.unwrap().to_string();
        assert!(!response.contains("TOP"), "{response}");
    }
}

#[tokio::test]
async fn a_masked_field_of_an_interface_is_served_as_null() {
    let (exec, _) = executor(schema(FieldDenyPolicy::Mask));
    for result in both_callers(&exec, "{ ledgers { holder { name secret } } }").await {
        let response = result.unwrap().to_string();
        assert!(!response.contains("TOP"), "{response}");
        assert!(response.contains(r#""secret":null"#), "{response}");
    }
}

/// Masking at a level of a union or an interface, whatever the projector serves there.
#[test]
fn a_masked_field_of_a_member_is_nulled_at_the_level_that_holds_it() {
    let (exec, _) = executor(schema(FieldDenyPolicy::Mask));
    let schema = schema(FieldDenyPolicy::Mask);
    for (document, field) in [
        ("{ ledgers { holder { ... on Bank { secret } } } }", "/holder/secret"),
        ("{ ledgers { party { ... on Org { credit } } } }", "/party/credit"),
    ] {
        let fields = root_fields(&exec, document);
        for caller in [None, Some(unscoped())] {
            let access = SelectionAccess::classify(
                &schema,
                "Ledger",
                &fields,
                Vec::new(),
                caller.as_ref(),
                None,
            )
            .unwrap();
            for named in [true, false] {
                let mut value = serde_json::json!({
                    "holder": {"__typename": "Bank", "secret": "TOP"},
                    "party": {"__typename": "Org", "credit": "TOP"},
                });
                if !named {
                    // A document that does not say which type it is is masked as every one.
                    value["holder"].as_object_mut().unwrap().remove("__typename");
                    value["party"].as_object_mut().unwrap().remove("__typename");
                }
                access.null_masked(&mut value, "Ledger", &fields, &schema);
                assert_eq!(value.pointer(field), Some(&serde_json::Value::Null), "{document}");
            }
        }
    }
}

#[test]
fn a_document_that_names_its_type_is_masked_as_that_type_alone() {
    let (exec, _) = executor(schema(FieldDenyPolicy::Mask));
    let schema = schema(FieldDenyPolicy::Mask);
    let document = "{ ledgers { party { ... on Org { credit } ... on Person { credit } } } }";
    let fields = root_fields(&exec, document);
    let access =
        SelectionAccess::classify(&schema, "Ledger", &fields, Vec::new(), None, None).unwrap();
    let mut value = serde_json::json!({"party": {"__typename": "Person", "credit": "OK"}});
    access.null_masked(&mut value, "Ledger", &fields, &schema);
    assert_eq!(value["party"]["credit"], "OK", "a Person's credit needs no scope");
}

/// A read whose root is a union, classified as a read of each type it can be.
#[test]
fn a_root_of_a_union_is_classified_as_each_of_its_types() {
    let (exec, _) = executor(schema(FieldDenyPolicy::Mask));
    let party = root_fields(&exec, "{ ledgers { party { ... on Org { credit } } } }")
        .remove(0)
        .nested_fields;
    let access = SelectionAccess::classify(
        &schema(FieldDenyPolicy::Mask),
        "Party",
        &party,
        Vec::new(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(access.root.masked, ["credit"]);
    let err = SelectionAccess::classify(
        &schema(FieldDenyPolicy::Reject),
        "Party",
        &party,
        Vec::new(),
        None,
        None,
    )
    .err()
    .expect("Org.credit is refused");
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err}");
}

#[test]
fn reading_an_interface_is_refused_when_reading_a_type_that_implements_it_is() {
    let gated = schema_with(FieldDenyPolicy::Mask, |s| {
        s.types.iter_mut().find(|t| t.name == "Bank").unwrap().requires_role =
            Some("admin".to_string());
    });
    assert!(type_read_refusal(&gated, "Holder", Some(&unscoped())).is_some());
    assert!(type_read_refusal(&gated, "Party", Some(&unscoped())).is_none());
}

fn select(name: &str, nested_fields: Vec<FieldSelection>) -> FieldSelection {
    FieldSelection {
        name: name.to_string(),
        alias: None,
        arguments: Vec::new(),
        nested_fields,
        directives: Vec::new(),
    }
}

#[test]
fn a_level_of_a_type_the_schema_does_not_declare_is_refused() {
    let ghostly = schema_with(FieldDenyPolicy::Mask, |s| {
        s.types[0]
            .fields
            .push(FieldDefinition::nullable("ghost", FieldType::Object("Nope".to_string())));
    });
    let fields = [select("ghost", vec![select("name", vec![])])];
    let err = SelectionAccess::classify(&ghostly, "Ledger", &fields, Vec::new(), None, None)
        .err()
        .expect("a level of no type is not read ungated");
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err}");
    assert!(
        crate::security::field_authorizer::selection_set_selects_gated_field(
            &ghostly, "Ledger", &fields
        ),
        "a level of no type cannot be told free of policy-gated fields"
    );
}

#[tokio::test]
async fn a_member_whose_read_requires_a_role_refuses_its_level() {
    let gated = schema_with(FieldDenyPolicy::Mask, |s| {
        s.types.iter_mut().find(|t| t.name == "Bank").unwrap().requires_role =
            Some("admin".to_string());
        s.types.iter_mut().find(|t| t.name == "Org").unwrap().requires_role =
            Some("admin".to_string());
    });
    for document in [
        "{ ledgers { holder { name } } }",
        "{ ledgers { party { ... on Org { name } } } }",
    ] {
        let (exec, adapter) = executor(gated.clone());
        for result in both_callers(&exec, document).await {
            let err = result.expect_err(document);
            assert!(matches!(err, FraiseQLError::Authorization { .. }), "{document}: {err}");
        }
        assert!(adapter.captured_where().is_none(), "{document} was read");
    }
}

#[tokio::test]
async fn a_member_is_put_to_the_authorizer_as_a_read_of_its_own_type() {
    struct DenyBanksAndOrgs;
    impl Authorizer for DenyBanksAndOrgs {
        fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
            Ok(match req.target_type {
                Some("Bank" | "Org") => AuthzDecision::Deny {
                    reason: "no".to_string(),
                },
                _ => AuthzDecision::Allow,
            })
        }
    }
    let config = RuntimeConfig::default().with_authorizer(Arc::new(DenyBanksAndOrgs));
    for document in [
        "{ ledgers { holder { name } } }",
        "{ ledgers { party { ... on Org { name } } } }",
    ] {
        let (exec, adapter) = executor_with(schema(FieldDenyPolicy::Mask), config.clone());
        let err = exec
            .execute_with_security(document, None, &unscoped())
            .await
            .expect_err(document);
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{document}: {err}");
        assert!(adapter.captured_where().is_none(), "{document} was read");
    }
}

#[tokio::test]
async fn a_policy_gated_field_of_a_member_is_refused_with_no_authorizer_to_decide_it() {
    let gated = schema_with(FieldDenyPolicy::Mask, |s| {
        for t in &mut s.types {
            if let Some(name) = t.fields.iter_mut().find(|f| f.name == "name") {
                if t.name == "Bank" || t.name == "Org" {
                    name.authorize = true;
                }
            }
        }
    });
    for document in [
        "{ ledgers { holder { name } } }",
        "{ ledgers { holder { ... on Bank { name } } } }",
        "{ ledgers { party { ... on Org { name } } } }",
    ] {
        let (exec, adapter) = executor(gated.clone());
        for result in both_callers(&exec, document).await {
            let err = result.expect_err(document);
            assert!(matches!(err, FraiseQLError::Authorization { .. }), "{document}: {err}");
        }
        assert!(adapter.captured_where().is_none(), "{document} was read");
    }
}

/// The schema with each of `types` read by tenant, through a list query over its view.
fn tenant_scoped(types: &[(&str, &str, &str)]) -> CompiledSchema {
    schema_with(FieldDenyPolicy::Mask, |s| {
        for (query, type_name, view) in types {
            let mut read =
                QueryDefinition::new(*query, *type_name).returning_list().with_sql_source(*view);
            read.inject_params
                .insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
            s.queries.push(read);
        }
    })
}

/// `Bank` and `Org` read by tenant; the `Ledger` view embeds them for no tenant.
fn scoped_members() -> CompiledSchema {
    tenant_scoped(&[("banks", "Bank", "v_bank"), ("orgs", "Org", "v_org")])
}

/// `Branch` read by tenant, below a `Bank` read through `Ledger.holder`.
fn scoped_below_a_member() -> CompiledSchema {
    tenant_scoped(&[("branches", "Branch", "v_branch")])
}

const BELOW_A_MEMBER: &str = "{ ledgers { holder { ... on Bank { branch { name } } } } }";

#[tokio::test]
async fn a_member_whose_rows_are_scoped_refuses_its_level() {
    let scoped_rows = scoped_members();
    for document in [
        "{ ledgers { holder { name } } }",
        "{ ledgers { party { ... on Org { name } } } }",
    ] {
        let (exec, adapter) = executor(scoped_rows.clone());
        let err = exec
            .execute_with_security(document, None, &unscoped())
            .await
            .expect_err(document);
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{document}: {err}");
        assert!(adapter.captured_where().is_none(), "{document} was read");
    }
}

#[tokio::test]
async fn a_scoped_level_below_a_member_is_refused() {
    let (exec, adapter) = executor(scoped_below_a_member());
    let err = exec
        .execute_with_security(BELOW_A_MEMBER, None, &unscoped())
        .await
        .expect_err("a Branch below a Bank is read by tenant");
    assert!(matches!(err, FraiseQLError::Authorization { .. }), "{err}");
    assert!(adapter.captured_where().is_none(), "the read was made");
}

/// A write's returned document is filtered by declared type, which a level of a union or
/// an interface is not.
#[test]
fn a_returned_document_is_refused_where_a_member_level_is_scoped() {
    for (schema, document) in [
        (scoped_members(), "{ ledgers { holder { name } } }"),
        (scoped_below_a_member(), BELOW_A_MEMBER),
    ] {
        let (exec, _) = executor(schema);
        let fields = root_fields(&exec, document);
        let err = DocumentRowFilter::default()
            .plan(&exec.ctx, "Ledger", &fields, Some(&unscoped()))
            .expect_err(document);
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{document}: {err}");
    }
}

#[test]
fn a_filter_through_a_union_or_an_interface_is_refused_where_a_member_is_scoped() {
    let schema = scoped_members();
    for field in ["holder", "party"] {
        let clause = WhereClause::Field {
            path:     vec![field.to_string(), "name".to_string()],
            operator: WhereOperator::Eq,
            value:    serde_json::json!("x"),
        };
        let err = guard_relation_filters(&schema, None, "Ledger", &clause, Some(&unscoped()))
            .expect_err(field);
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{field}: {err}");
    }
}

#[cfg(feature = "federation")]
#[test]
fn an_entity_lookup_is_refused_where_a_member_level_is_scoped() {
    for (schema, document) in [
        (scoped_members(), "{ ledgers { holder { name } } }"),
        (scoped_below_a_member(), BELOW_A_MEMBER),
    ] {
        let (exec, _) = executor(schema);
        let fields = root_fields(&exec, document);
        let err = exec
            .query_runner()
            .refuse_row_gated_levels("Ledger", &fields, Some(&unscoped()), "an entity lookup")
            .expect_err(document);
        assert!(matches!(err, FraiseQLError::Authorization { .. }), "{document}: {err}");
    }
}
