#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

mod mod_tests {
    use super::super::*;

    #[test]
    fn test_auth0_role_mapping() {
        let roles = auth0::Auth0OAuth::map_auth0_roles_to_fraiseql(vec!["admin".to_string()]);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_github_role_mapping() {
        let roles = github::GitHubOAuth::map_teams_to_roles(vec![
            "org:admin".to_string(),
            "org:operator".to_string(),
        ]);
        assert_eq!(roles.len(), 2);
    }

    #[test]
    fn test_google_role_mapping() {
        let roles = google::GoogleOAuth::map_groups_to_roles(vec![
            "fraiseql-admins@company.com".to_string(),
        ]);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_keycloak_role_mapping() {
        let roles =
            keycloak::KeycloakOAuth::map_keycloak_roles_to_fraiseql(vec!["admin".to_string()]);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_okta_group_mapping() {
        let groups = okta::OktaOAuth::map_okta_groups_to_fraiseql(vec![
            "fraiseql-admin".to_string(),
            "everyone".to_string(),
        ]);
        assert_eq!(groups.len(), 2);
        assert!(groups.contains(&"admin".to_string()));
        assert!(groups.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_azure_ad_role_mapping() {
        let roles =
            azure_ad::AzureADOAuth::map_azure_roles_to_fraiseql(vec!["fraiseql.admin".to_string()]);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_ory_group_mapping() {
        let groups = ory::OryOAuth::map_ory_groups_to_fraiseql(vec![
            "admin".to_string(),
            "ory-operator".to_string(),
        ]);
        assert_eq!(groups.len(), 2);
        assert!(groups.contains(&"admin".to_string()));
        assert!(groups.contains(&"operator".to_string()));
    }

    #[test]
    fn test_logto_role_mapping() {
        let roles = logto::LogtoOAuth::map_logto_roles_to_fraiseql(vec![
            "admin".to_string(),
            "logto-operator".to_string(),
        ]);
        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
    }
}

mod auth0_tests {
    use super::super::auth0::*;

    #[test]
    fn test_extract_roles_from_custom_namespace() {
        let claims = serde_json::json!({
            "https://fraiseql.dev/roles": ["admin", "operator", "viewer"]
        });

        let roles = Auth0OAuth::extract_roles(&claims);
        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
        assert!(roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_roles_fallback() {
        let claims = serde_json::json!({
            "roles": ["admin", "user"]
        });

        let roles = Auth0OAuth::extract_roles(&claims);
        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_extract_roles_missing() {
        let claims = serde_json::json!({});
        let roles = Auth0OAuth::extract_roles(&claims);
        assert!(roles.is_empty());
    }

    #[test]
    fn test_map_auth0_roles_to_fraiseql() {
        let roles = vec![
            "admin".to_string(),
            "fraiseql-operator".to_string(),
            "viewer".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = Auth0OAuth::map_auth0_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_auth0_roles_underscore_separator() {
        let roles = vec![
            "fraiseql_admin".to_string(),
            "fraiseql_operator".to_string(),
            "fraiseql_viewer".to_string(),
        ];

        let fraiseql_roles = Auth0OAuth::map_auth0_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_auth0_roles_case_insensitive() {
        let roles = vec![
            "ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];

        let fraiseql_roles = Auth0OAuth::map_auth0_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
    }

    #[test]
    fn test_map_auth0_roles_common_patterns() {
        let roles = vec![
            "admin_user".to_string(),
            "operator_user".to_string(),
            "viewer_user".to_string(),
            "read_only".to_string(),
        ];

        let fraiseql_roles = Auth0OAuth::map_auth0_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 4);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_claim() {
        let claims = serde_json::json!({
            "org_id": "example-corp"
        });

        let org_id = Auth0OAuth::extract_org_id(&claims, "user@company.com");
        assert_eq!(org_id, Some("example-corp".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_email_domain() {
        let claims = serde_json::json!({});

        let org_id = Auth0OAuth::extract_org_id(&claims, "user@example.com");
        assert_eq!(org_id, Some("example".to_string()));
    }

    #[test]
    fn test_extract_org_id_missing() {
        let claims = serde_json::json!({});

        let org_id = Auth0OAuth::extract_org_id(&claims, "user@localhost");
        assert_eq!(org_id, Some("localhost".to_string()));
    }

    #[test]
    fn test_extract_org_id_claim_takes_precedence() {
        let claims = serde_json::json!({
            "org_id": "explicit-org"
        });

        let org_id = Auth0OAuth::extract_org_id(&claims, "user@other.com");
        assert_eq!(org_id, Some("explicit-org".to_string()));
    }
}

mod azure_ad_tests {
    use super::super::azure_ad::*;

    #[test]
    fn test_extract_app_roles() {
        let claims = serde_json::json!({
            "roles": ["fraiseql.admin", "fraiseql.operator"]
        });

        let roles = AzureADOAuth::extract_app_roles(&claims);
        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"fraiseql.admin".to_string()));
    }

    #[test]
    fn test_extract_groups() {
        let claims = serde_json::json!({
            "groups": [
                "00000000-0000-0000-0000-000000000001",
                "00000000-0000-0000-0000-000000000002"
            ]
        });

        let groups = AzureADOAuth::extract_groups(&claims);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_map_azure_roles_to_fraiseql() {
        let roles = vec![
            "fraiseql.admin".to_string(),
            "fraiseql.operator".to_string(),
            "fraiseql.viewer".to_string(),
            "other.role".to_string(),
        ];

        let fraiseql_roles = AzureADOAuth::map_azure_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_azure_roles_underscore_format() {
        let roles = vec![
            "fraiseql_admin".to_string(),
            "fraiseql_operator".to_string(),
        ];

        let fraiseql_roles = AzureADOAuth::map_azure_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 2);
    }

    #[test]
    fn test_map_azure_roles_case_insensitive() {
        let roles = vec![
            "FRAISEQL.ADMIN".to_string(),
            "FraiseQL.Operator".to_string(),
        ];

        let fraiseql_roles = AzureADOAuth::map_azure_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 2);
    }

    #[test]
    fn test_get_user_identifier_upn() {
        let claims = serde_json::json!({
            "preferred_username": "user@contoso.com"
        });

        let identifier = AzureADOAuth::get_user_identifier(&claims);
        assert_eq!(identifier, Some("user@contoso.com".to_string()));
    }

    #[test]
    fn test_get_user_identifier_email_fallback() {
        let claims = serde_json::json!({
            "email": "user@contoso.com"
        });

        let identifier = AzureADOAuth::get_user_identifier(&claims);
        assert_eq!(identifier, Some("user@contoso.com".to_string()));
    }

    #[test]
    fn test_extract_app_roles_missing() {
        let claims = serde_json::json!({});
        let roles = AzureADOAuth::extract_app_roles(&claims);
        assert!(roles.is_empty());
    }
}

mod google_tests {
    use super::super::google::*;

    #[test]
    fn test_map_google_workspace_groups_to_roles() {
        let groups = vec![
            "fraiseql-admins@company.com".to_string(),
            "fraiseql-operators@company.com".to_string(),
            "other-group@company.com".to_string(),
            "fraiseql-viewer@company.com".to_string(),
        ];

        let roles = GoogleOAuth::map_groups_to_roles(groups);

        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
        assert!(roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_groups_case_insensitive() {
        let groups = vec![
            "FRAISEQL-ADMINS@COMPANY.COM".to_string(),
            "FraiseQL-Operators@Company.Com".to_string(),
        ];

        let roles = GoogleOAuth::map_groups_to_roles(groups);

        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_extract_roles_from_domain_company() {
        let roles = GoogleOAuth::extract_roles_from_domain("user@company.com");
        assert_eq!(roles, vec!["operator".to_string()]);
    }

    #[test]
    fn test_extract_roles_from_domain_external() {
        let roles = GoogleOAuth::extract_roles_from_domain("user@external.com");
        assert_eq!(roles, vec!["viewer".to_string()]);
    }

    #[test]
    fn test_map_groups_empty() {
        let roles = GoogleOAuth::map_groups_to_roles(vec![]);
        assert!(roles.is_empty());
    }
}

mod keycloak_tests {
    use super::super::keycloak::*;

    #[test]
    fn test_extract_realm_roles() {
        let claims = serde_json::json!({
            "realm_access": {
                "roles": ["admin", "user", "operator"]
            }
        });

        let roles = KeycloakOAuth::extract_realm_roles(&claims);
        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_extract_client_roles() {
        let claims = serde_json::json!({
            "resource_access": {
                "fraiseql": {
                    "roles": ["client-admin", "client-user"]
                }
            }
        });

        let roles = KeycloakOAuth::extract_client_roles(&claims, "fraiseql");
        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"client-admin".to_string()));
    }

    #[test]
    fn test_map_keycloak_roles_to_fraiseql() {
        let roles = vec![
            "admin".to_string(),
            "fraiseql-operator".to_string(),
            "user".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = KeycloakOAuth::map_keycloak_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_realm_roles_missing() {
        let claims = serde_json::json!({});
        let roles = KeycloakOAuth::extract_realm_roles(&claims);
        assert!(roles.is_empty());
    }

    #[test]
    fn test_map_roles_case_insensitive() {
        let roles = vec![
            "ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];
        let fraiseql_roles = KeycloakOAuth::map_keycloak_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
    }
}

mod logto_tests {
    use super::super::logto::*;

    #[test]
    fn test_extract_roles_from_claim() {
        let claims = serde_json::json!({
            "roles": ["admin", "operator", "viewer"]
        });

        let roles = LogtoOAuth::extract_roles(&claims);
        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_extract_roles_missing() {
        let claims = serde_json::json!({});
        let roles = LogtoOAuth::extract_roles(&claims);
        assert!(roles.is_empty());
    }

    #[test]
    fn test_extract_organizations() {
        let claims = serde_json::json!({
            "organizations": ["org-1", "org-2", "org-3"]
        });

        let orgs = LogtoOAuth::extract_organizations(&claims);
        assert_eq!(orgs.len(), 3);
        assert!(orgs.contains(&"org-1".to_string()));
    }

    #[test]
    fn test_extract_organizations_missing() {
        let claims = serde_json::json!({});
        let orgs = LogtoOAuth::extract_organizations(&claims);
        assert!(orgs.is_empty());
    }

    #[test]
    fn test_extract_organization_roles() {
        let claims = serde_json::json!({
            "organization_roles": {
                "org-1": ["admin"],
                "org-2": ["member", "operator"]
            }
        });

        let org_roles = LogtoOAuth::extract_organization_roles(&claims);
        assert_eq!(org_roles.len(), 3);
        assert!(org_roles.contains(&"admin".to_string()));
        assert!(org_roles.contains(&"member".to_string()));
        assert!(org_roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_extract_organization_roles_missing() {
        let claims = serde_json::json!({});
        let org_roles = LogtoOAuth::extract_organization_roles(&claims);
        assert!(org_roles.is_empty());
    }

    #[test]
    fn test_extract_organization_id() {
        let claims = serde_json::json!({
            "organization_id": "current-org"
        });

        let org_id = LogtoOAuth::extract_organization_id(&claims);
        assert_eq!(org_id, Some("current-org".to_string()));
    }

    #[test]
    fn test_extract_organization_id_missing() {
        let claims = serde_json::json!({});
        let org_id = LogtoOAuth::extract_organization_id(&claims);
        assert!(org_id.is_none());
    }

    #[test]
    fn test_map_logto_roles_to_fraiseql() {
        let roles = vec![
            "admin".to_string(),
            "logto-operator".to_string(),
            "user".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_logto_roles_case_insensitive() {
        let roles = vec![
            "ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];

        let fraiseql_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_logto_roles_organization_pattern() {
        let roles = vec![
            "organization:admin".to_string(),
            "organization:member".to_string(),
            "organization:operator".to_string(),
        ];

        let fraiseql_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
    }

    #[test]
    fn test_map_logto_roles_substring_matching() {
        let roles = vec![
            "my_custom_admin_role".to_string(),
            "operator_special".to_string(),
            "viewer_guest".to_string(),
        ];

        let fraiseql_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_org_id_fallback_to_first_org() {
        let claims = serde_json::json!({
            "organizations": ["org-1", "org-2"]
        });

        let org_id = LogtoOAuth::extract_organization_id(&claims);
        assert!(org_id.is_none()); // Should be None because organization_id is missing

        // Simulating the fallback logic from user_info()
        let orgs = LogtoOAuth::extract_organizations(&claims);
        let fallback_org = if orgs.is_empty() {
            None
        } else {
            Some(orgs[0].clone())
        };

        assert_eq!(fallback_org, Some("org-1".to_string()));
    }

    #[test]
    fn test_extract_all_claims() {
        let claims = serde_json::json!({
            "roles": ["admin", "operator"],
            "organizations": ["org-1", "org-2"],
            "organization_id": "org-1",
            "organization_roles": {
                "org-1": ["admin"]
            }
        });

        let roles = LogtoOAuth::extract_roles(&claims);
        let orgs = LogtoOAuth::extract_organizations(&claims);
        let org_id = LogtoOAuth::extract_organization_id(&claims);
        let org_roles = LogtoOAuth::extract_organization_roles(&claims);
        let mapped_roles = LogtoOAuth::map_logto_roles_to_fraiseql(roles.clone());

        assert_eq!(roles.len(), 2);
        assert_eq!(orgs.len(), 2);
        assert_eq!(org_id, Some("org-1".to_string()));
        assert_eq!(org_roles.len(), 1);
        assert_eq!(mapped_roles.len(), 2);
    }
}

mod okta_tests {
    use super::super::okta::*;

    #[test]
    fn test_extract_groups_from_claim() {
        let claims = serde_json::json!({
            "groups": ["fraiseql-admin", "fraiseql-operator", "everyone"]
        });

        let groups = OktaOAuth::extract_groups(&claims);
        assert_eq!(groups.len(), 3);
        assert!(groups.contains(&"fraiseql-admin".to_string()));
    }

    #[test]
    fn test_extract_groups_fallback_to_roles() {
        let claims = serde_json::json!({
            "roles": ["admin", "user"]
        });

        let groups = OktaOAuth::extract_groups(&claims);
        assert_eq!(groups.len(), 2);
        assert!(groups.contains(&"admin".to_string()));
    }

    #[test]
    fn test_extract_groups_missing() {
        let claims = serde_json::json!({});
        let groups = OktaOAuth::extract_groups(&claims);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_map_okta_groups_to_fraiseql() {
        let groups = vec![
            "fraiseql-admin".to_string(),
            "fraiseql-operator".to_string(),
            "everyone".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = OktaOAuth::map_okta_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_okta_groups_underscore_separator() {
        let groups = vec![
            "fraiseql_admin".to_string(),
            "fraiseql_operator".to_string(),
            "fraiseql_viewer".to_string(),
        ];

        let fraiseql_roles = OktaOAuth::map_okta_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_okta_groups_case_insensitive() {
        let groups = vec![
            "FRAISEQL-ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];

        let fraiseql_roles = OktaOAuth::map_okta_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
    }

    #[test]
    fn test_map_okta_groups_partial_match() {
        let groups = vec![
            "it-admins".to_string(),
            "sales-operators".to_string(),
            "support-read-only".to_string(),
        ];

        let fraiseql_roles = OktaOAuth::map_okta_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_okta_groups_everyone_becomes_viewer() {
        let groups = vec!["everyone".to_string()];
        let fraiseql_roles = OktaOAuth::map_okta_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 1);
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_claim() {
        let claims = serde_json::json!({
            "org_id": "example-corp"
        });

        let org_id = OktaOAuth::extract_org_id(&claims, "user@company.com");
        assert_eq!(org_id, Some("example-corp".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_okta_org_claim() {
        let claims = serde_json::json!({
            "org": "okta-company"
        });

        let org_id = OktaOAuth::extract_org_id(&claims, "user@company.com");
        assert_eq!(org_id, Some("okta-company".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_email_domain() {
        let claims = serde_json::json!({});

        let org_id = OktaOAuth::extract_org_id(&claims, "user@example.com");
        assert_eq!(org_id, Some("example".to_string()));
    }

    #[test]
    fn test_extract_org_id_claim_takes_precedence() {
        let claims = serde_json::json!({
            "org_id": "explicit-org"
        });

        let org_id = OktaOAuth::extract_org_id(&claims, "user@other.com");
        assert_eq!(org_id, Some("explicit-org".to_string()));
    }

    #[test]
    fn test_get_okta_id() {
        let claims = serde_json::json!({
            "sub": "00u1234567890abcdefg"
        });

        let okta_id = OktaOAuth::get_okta_id(&claims);
        assert_eq!(okta_id, Some("00u1234567890abcdefg".to_string()));
    }

    #[test]
    fn test_get_okta_id_missing() {
        let claims = serde_json::json!({});
        let okta_id = OktaOAuth::get_okta_id(&claims);
        assert!(okta_id.is_none());
    }
}

mod ory_tests {
    use super::super::ory::*;

    #[test]
    fn test_extract_groups_from_array() {
        let claims = serde_json::json!({
            "groups": ["admin", "operators", "viewers"]
        });

        let groups = OryOAuth::extract_groups(&claims);
        assert_eq!(groups.len(), 3);
        assert!(groups.contains(&"admin".to_string()));
        assert!(groups.contains(&"operators".to_string()));
    }

    #[test]
    fn test_extract_groups_from_string() {
        let claims = serde_json::json!({
            "groups": "admin"
        });

        let groups = OryOAuth::extract_groups(&claims);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], "admin");
    }

    #[test]
    fn test_extract_groups_missing() {
        let claims = serde_json::json!({});
        let groups = OryOAuth::extract_groups(&claims);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_map_ory_groups_to_fraiseql() {
        let groups = vec![
            "admin".to_string(),
            "ory-operator".to_string(),
            "user".to_string(),
            "unknown".to_string(),
        ];

        let fraiseql_roles = OryOAuth::map_ory_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_ory_groups_case_insensitive() {
        let groups = vec![
            "ADMIN".to_string(),
            "Operator".to_string(),
            "VIEWER".to_string(),
        ];

        let fraiseql_roles = OryOAuth::map_ory_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_ory_groups_keto_patterns() {
        let groups = vec![
            "fraiseql:admin".to_string(),
            "fraiseql:operator".to_string(),
            "fraiseql:viewer".to_string(),
            "other:role".to_string(),
        ];

        let fraiseql_roles = OryOAuth::map_ory_groups_to_fraiseql(groups);

        assert_eq!(fraiseql_roles.len(), 3);
        assert!(fraiseql_roles.contains(&"admin".to_string()));
        assert!(fraiseql_roles.contains(&"operator".to_string()));
        assert!(fraiseql_roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_claim() {
        let claims = serde_json::json!({
            "org_id": "acme-corp"
        });

        let org_id = OryOAuth::extract_org_id(&claims, "user@example.com");
        assert_eq!(org_id, Some("acme-corp".to_string()));
    }

    #[test]
    fn test_extract_org_id_from_email_domain() {
        let claims = serde_json::json!({});

        let org_id = OryOAuth::extract_org_id(&claims, "user@example.com");
        assert_eq!(org_id, Some("example.com".to_string()));
    }

    #[test]
    fn test_extract_org_id_missing() {
        let claims = serde_json::json!({});

        let org_id = OryOAuth::extract_org_id(&claims, "");
        assert!(org_id.is_none());
    }

    #[test]
    fn test_extract_all_roles_and_org() {
        let claims = serde_json::json!({
            "groups": ["admin", "operators"],
            "org_id": "my-org"
        });

        let groups = OryOAuth::extract_groups(&claims);
        let roles = OryOAuth::map_ory_groups_to_fraiseql(groups);
        let org_id = OryOAuth::extract_org_id(&claims, "user@example.com");

        assert_eq!(roles.len(), 2);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
        assert_eq!(org_id, Some("my-org".to_string()));
    }
}

mod github_tests {
    use super::super::github::*;

    #[test]
    fn test_map_github_teams_to_roles() {
        let teams = vec![
            "acme-corp:admin".to_string(),
            "acme-corp:operators".to_string(),
            "acme-corp:unknown".to_string(),
            "other-org:viewer".to_string(),
        ];

        let roles = GitHubOAuth::map_teams_to_roles(teams);

        assert_eq!(roles.len(), 3);
        assert!(roles.contains(&"admin".to_string()));
        assert!(roles.contains(&"operator".to_string()));
        assert!(roles.contains(&"viewer".to_string()));
    }

    #[test]
    fn test_map_teams_empty() {
        let roles = GitHubOAuth::map_teams_to_roles(vec![]);
        assert!(roles.is_empty());
    }

    #[test]
    fn test_map_teams_no_matches() {
        let teams = vec!["org:unknown-team".to_string(), "org:other".to_string()];
        let roles = GitHubOAuth::map_teams_to_roles(teams);
        assert!(roles.is_empty());
    }

    // ── S23-H3: GitHub API response size caps ─────────────────────────────────

    #[test]
    fn github_response_cap_constant_is_reasonable() {
        const { assert!(MAX_GITHUB_RESPONSE_BYTES >= 1024 * 1024) }
        const { assert!(MAX_GITHUB_RESPONSE_BYTES <= 100 * 1024 * 1024) }
    }

    #[test]
    fn github_request_timeout_is_set() {
        let secs = GITHUB_REQUEST_TIMEOUT.as_secs();
        assert!(secs > 0 && secs <= 120, "GitHub timeout should be 1–120 s, got {secs}");
    }

    // ── #368: GitHub is a plain OAuth2 provider, not an OIDC one ────────────

    #[test]
    fn github_constructs_offline_against_wellknown_endpoints() {
        // github.com serves no OIDC discovery document
        // (/.well-known/openid-configuration is a 404), so the old
        // discovery-based constructor could never have produced a working
        // provider. Construction must be network-free against the fixed
        // well-known endpoints.
        let p = GitHubOAuth::new(
            "the-client-id".to_string(),
            "the-secret".to_string(),
            "https://app.example.com/auth/v1/callback".to_string(),
        )
        .expect("offline construction");
        let url = crate::provider::OAuthProvider::authorization_url(&p, "STATE123");
        assert!(
            url.starts_with("https://github.com/login/oauth/authorize?"),
            "authorize URL must target the well-known GitHub endpoint: {url}"
        );
        assert!(url.contains("client_id=the-client-id"), "{url}");
        assert!(url.contains("state=STATE123"), "{url}");
        assert!(
            url.contains("scope=read%3Auser%20user%3Aemail"),
            "the scope must request read:user + user:email so the /user/emails \
             second hop is authorized: {url}"
        );
    }

    #[test]
    fn github_endpoint_overrides_are_ssrf_guarded() {
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let err = GitHubOAuth::with_endpoints(
                "id".to_string(),
                "secret".to_string(),
                "https://app.example.com/cb".to_string(),
                "http://169.254.169.254".to_string(),
                "https://api.github.com".to_string(),
            )
            .expect_err("a link-local base_url must be refused");
            let msg = err.to_string();
            assert!(msg.contains("SSRF") || msg.contains("private"), "{msg}");
        });
    }

    #[test]
    fn github_primary_verified_email_is_linkable() {
        let emails = vec![
            GitHubEmail {
                email:    "old@example.com".to_string(),
                primary:  false,
                verified: true,
            },
            GitHubEmail {
                email:    "me@example.com".to_string(),
                primary:  true,
                verified: true,
            },
        ];
        assert_eq!(
            select_linkable_email(&emails),
            Some(("me@example.com".to_string(), true)),
            "the primary verified email is the linkable identity"
        );
    }

    #[test]
    fn github_primary_unverified_email_is_never_verified() {
        let emails = vec![GitHubEmail {
            email:    "me@example.com".to_string(),
            primary:  true,
            verified: false,
        }];
        assert_eq!(
            select_linkable_email(&emails),
            Some(("me@example.com".to_string(), false)),
            "an unverified primary email must carry verified = false"
        );
    }

    #[test]
    fn github_no_primary_email_selects_none() {
        let emails = vec![GitHubEmail {
            email:    "side@example.com".to_string(),
            primary:  false,
            verified: true,
        }];
        assert_eq!(
            select_linkable_email(&emails),
            None,
            "without a primary entry there is no canonical identity email"
        );
    }
}

/// Sign in with Apple (#943).
mod apple_tests {
    use base64::Engine as _;

    use crate::{
        provider::{OAuthProvider as _, TokenResponse},
        providers::apple::{AppleFirstAuthUser, AppleOAuth, is_private_relay_email},
    };

    /// A throwaway P-256 key, generated for this test file and used nowhere
    /// else. Apple issues a PKCS#8 PEM `.p8`, which is exactly this shape.
    const TEST_P8: &str = "-----BEGIN PRIVATE KEY-----\n\
        MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg6mOJdB87DG8anytc\n\
        jGCaH3eI4OkrTGRc6sZGu1DqyiGhRANCAARKwlK3b7SIes76KDfwwP1Dxf3OgSsa\n\
        dTtT/3rfS3QYTqqyzOH6LW51mUpy3vxAi/IKx1oEdLAJzOCm1Z1p5wFw\n\
        -----END PRIVATE KEY-----\n";

    const CLIENT_ID: &str = "com.example.service";
    const ISSUER: &str = "https://appleid.apple.com";

    fn provider() -> AppleOAuth {
        AppleOAuth::new(
            CLIENT_ID.to_string(),
            "TEAM123456".to_string(),
            "KEY7890AB".to_string(),
            TEST_P8.to_string(),
            "https://app.example.com/auth/v1/callback".to_string(),
        )
        .expect("a valid .p8 key constructs")
    }

    fn decode_part(part: &str) -> serde_json::Value {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(part)
            .expect("JWT part is base64url");
        serde_json::from_slice(&bytes).expect("JWT part is JSON")
    }

    /// Build an unsigned `id_token` — the provider validates claims, not the
    /// signature (see the module docs), so a claims-only token is exactly what
    /// the parser sees.
    fn id_token(claims: &serde_json::Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"alg":"ES256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(claims).unwrap());
        format!("{header}.{payload}.sig")
    }

    fn far_future() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600
    }

    // ── Client-secret assertion ──────────────────────────────────────────

    #[test]
    fn client_secret_is_an_es256_assertion_over_the_apple_triple() {
        let assertion = provider().client_secret().expect("assertion mints");
        let parts: Vec<&str> = assertion.split('.').collect();
        assert_eq!(parts.len(), 3, "the client secret is a signed JWT, not a static string");

        let header = decode_part(parts[0]);
        assert_eq!(header["alg"], "ES256", "Apple accepts only ES256");
        assert_eq!(header["kid"], "KEY7890AB", "the header must name the .p8 key");

        let claims = decode_part(parts[1]);
        assert_eq!(claims["iss"], "TEAM123456", "iss is the developer team ID");
        assert_eq!(claims["sub"], CLIENT_ID, "sub is the services ID");
        assert_eq!(
            claims["aud"], "https://appleid.apple.com",
            "aud names Apple itself, not the endpoint host"
        );
        let iat = claims["iat"].as_u64().unwrap();
        let exp = claims["exp"].as_u64().unwrap();
        assert!(exp > iat, "the assertion must expire after it was issued");
        assert!(
            exp - iat <= 15_777_000,
            "Apple refuses an assertion living longer than six months"
        );
    }

    #[test]
    fn client_secret_is_reused_until_it_nears_expiry() {
        let provider = provider();
        let first = provider.client_secret().unwrap();
        let second = provider.client_secret().unwrap();
        assert_eq!(first, second, "a still-valid assertion is reused, not re-signed per request");
    }

    #[test]
    fn a_key_that_cannot_sign_is_refused_at_construction() {
        let err = AppleOAuth::new(
            CLIENT_ID.to_string(),
            "TEAM123456".to_string(),
            "KEY7890AB".to_string(),
            "-----BEGIN PRIVATE KEY-----\nnot-a-key\n-----END PRIVATE KEY-----\n".to_string(),
            "https://app.example.com/auth/v1/callback".to_string(),
        )
        .expect_err("an unusable .p8 must not construct a provider");
        assert!(
            err.to_string().contains("ES256"),
            "the error must name what is wrong with the key: {err}"
        );
    }

    // ── Authorization URL ────────────────────────────────────────────────

    #[test]
    fn authorization_url_requests_form_post_and_the_name_email_scopes() {
        let url = provider().authorization_url("state-abc");
        assert!(url.starts_with("https://appleid.apple.com/auth/authorize?"), "{url}");
        assert!(
            url.contains("response_mode=form_post"),
            "the name/email scopes make Apple POST the callback: {url}"
        );
        assert!(url.contains("scope=name%20email"), "{url}");
        assert!(url.contains("response_type=code"), "{url}");
        assert!(url.contains("state=state-abc"), "{url}");
    }

    // ── id_token → UserInfo ──────────────────────────────────────────────

    #[test]
    fn id_token_yields_the_subject_and_verified_email() {
        let info = provider()
            .user_info_from_id_token(&id_token(&serde_json::json!({
                "iss": ISSUER,
                "aud": CLIENT_ID,
                "sub": "001234.abcdef",
                "exp": far_future(),
                "email": "user@example.com",
                "email_verified": true,
            })))
            .expect("a well-formed id_token identifies the user");
        assert_eq!(info.id, "001234.abcdef");
        assert_eq!(info.email.as_deref(), Some("user@example.com"));
        assert!(info.email_verified);
    }

    #[test]
    fn email_verified_is_honoured_in_apples_string_spelling() {
        // Apple renders its booleans as `"true"` in some flows. A parser that
        // accepts only the JSON bool drops the claim — and dropping it here
        // fails *open* into the email-keyed linking space.
        let info = provider()
            .user_info_from_id_token(&id_token(&serde_json::json!({
                "iss": ISSUER,
                "aud": CLIENT_ID,
                "sub": "001234.abcdef",
                "exp": far_future(),
                "email": "user@example.com",
                "email_verified": "true",
                "is_private_email": "false",
            })))
            .expect("the string spelling parses");
        assert!(info.email_verified, "email_verified: \"true\" must be honoured");
        assert_eq!(info.raw_claims["is_private_email"], serde_json::json!(false));
    }

    #[test]
    fn an_email_with_no_verified_claim_is_unverified() {
        let info = provider()
            .user_info_from_id_token(&id_token(&serde_json::json!({
                "iss": ISSUER,
                "aud": CLIENT_ID,
                "sub": "001234.abcdef",
                "exp": far_future(),
                "email": "user@example.com",
            })))
            .expect("the token parses");
        assert!(
            !info.email_verified,
            "an address with no verification flag must not enter the email-keyed linking space"
        );
    }

    #[test]
    fn an_id_token_for_another_audience_is_refused() {
        let err = provider()
            .user_info_from_id_token(&id_token(&serde_json::json!({
                "iss": ISSUER,
                "aud": "com.someone.else",
                "sub": "001234.abcdef",
                "exp": far_future(),
            })))
            .expect_err("a token minted for a different client must not identify our user");
        assert!(err.to_string().contains("audience"), "{err}");
    }

    #[test]
    fn an_id_token_from_another_issuer_is_refused() {
        let err = provider()
            .user_info_from_id_token(&id_token(&serde_json::json!({
                "iss": "https://evil.example.com",
                "aud": CLIENT_ID,
                "sub": "001234.abcdef",
                "exp": far_future(),
            })))
            .expect_err("a token from another issuer must be refused");
        assert!(err.to_string().contains("issuer"), "{err}");
    }

    #[test]
    fn an_expired_id_token_is_refused() {
        let err = provider()
            .user_info_from_id_token(&id_token(&serde_json::json!({
                "iss": ISSUER,
                "aud": CLIENT_ID,
                "sub": "001234.abcdef",
                "exp": 1_000_000_u64,
            })))
            .expect_err("an expired token must be refused");
        assert!(err.to_string().contains("expired"), "{err}");
    }

    #[tokio::test]
    async fn user_info_by_access_token_refuses_loudly() {
        // Apple publishes no userinfo endpoint. A caller on this method has
        // skipped `user_info_from_tokens`; failing beats returning nothing.
        let err = provider()
            .user_info("any-access-token")
            .await
            .expect_err("there is no userinfo endpoint to call");
        assert!(err.to_string().contains("user_info_from_tokens"), "{err}");
    }

    #[tokio::test]
    async fn a_token_response_with_no_id_token_identifies_nobody() {
        let err = provider()
            .user_info_from_tokens(&TokenResponse {
                access_token:  "at".to_string(),
                refresh_token: None,
                expires_in:    3600,
                token_type:    "Bearer".to_string(),
                id_token:      None,
            })
            .await
            .expect_err("without an id_token there is no identity");
        assert!(err.to_string().contains("id_token"), "{err}");
    }

    // ── Private Relay ────────────────────────────────────────────────────

    #[test]
    fn private_relay_addresses_are_recognised() {
        assert!(is_private_relay_email("abc123@privaterelay.appleid.com"));
        assert!(is_private_relay_email("  ABC123@PrivateRelay.AppleID.com  "));
        assert!(!is_private_relay_email("user@example.com"));
        assert!(
            !is_private_relay_email("user@privaterelay.appleid.com.evil.example"),
            "the domain must match at the end, not anywhere"
        );
    }

    // ── First-authorization user payload ─────────────────────────────────

    #[test]
    fn the_first_auth_payload_yields_a_display_name() {
        let user = AppleFirstAuthUser::parse(
            r#"{"name":{"firstName":"Ada","lastName":"Lovelace"},"email":"ada@example.com"}"#,
        )
        .expect("Apple's documented shape parses");
        assert_eq!(user.display_name().as_deref(), Some("Ada Lovelace"));
    }

    #[test]
    fn the_first_auth_payloads_email_is_not_even_modelled() {
        // The payload arrives in a POST the *browser* makes, so its email is
        // attacker-chosen. Nothing in `AppleFirstAuthUser` can carry it into
        // account resolution, and this test pins that: the struct that reaches
        // the callback has a name and nothing else.
        let user = AppleFirstAuthUser::parse(r#"{"email":"victim@example.com"}"#)
            .expect("an email-only payload still parses");
        assert!(user.name.is_none());
        assert_eq!(user.display_name(), None, "there is no path from the payload to an address");
        assert!(
            !format!("{user:?}").contains("victim@example.com"),
            "the parsed payload must not retain the browser-supplied address anywhere: {user:?}"
        );
    }

    #[test]
    fn a_malformed_first_auth_payload_is_ignored_not_fatal() {
        assert!(AppleFirstAuthUser::parse("not json").is_none());
        let partial = AppleFirstAuthUser::parse(r#"{"name":{"firstName":"  "}}"#).unwrap();
        assert_eq!(partial.display_name(), None, "a blank name is no name");
    }
}

/// Discord (#944).
mod discord_tests {
    use crate::{
        provider::OAuthProvider as _,
        providers::discord::{DiscordOAuth, DiscordUser, select_linkable_email},
    };

    fn provider() -> DiscordOAuth {
        DiscordOAuth::new(
            "discord-client".to_string(),
            "discord-secret".to_string(),
            "https://app.example.com/auth/v1/callback".to_string(),
        )
        .expect("construction is network-free")
    }

    fn user(email: Option<&str>, verified: Option<bool>) -> DiscordUser {
        DiscordUser {
            id: "776655443322110099".to_string(),
            username: "carol".to_string(),
            global_name: Some("Carol".to_string()),
            email: email.map(str::to_string),
            verified,
            avatar: None,
        }
    }

    #[test]
    fn authorization_url_requests_identify_and_email() {
        let url = provider().authorization_url("state-abc");
        assert!(url.starts_with("https://discord.com/oauth2/authorize?"), "{url}");
        // Without `email` the user object carries no address at all, and
        // without `identify` there is no user object.
        assert!(url.contains("scope=identify%20email"), "{url}");
        assert!(url.contains("response_type=code"), "{url}");
        assert!(url.contains("state=state-abc"), "{url}");
    }

    #[test]
    fn a_verified_email_is_linkable() {
        let (email, verified) = select_linkable_email(&user(Some("carol@example.com"), Some(true)));
        assert_eq!(email.as_deref(), Some("carol@example.com"));
        assert!(verified);
    }

    #[test]
    fn an_unverified_email_is_reported_unverified() {
        // The whole reason `discord` may sit in the default trusted set: the
        // flag is read, not assumed. Assuming it would put an address the user
        // never confirmed into the email-keyed linking space.
        let (email, verified) =
            select_linkable_email(&user(Some("carol@example.com"), Some(false)));
        assert_eq!(email.as_deref(), Some("carol@example.com"));
        assert!(!verified);
    }

    #[test]
    fn an_absent_verified_flag_is_not_verified() {
        let (_, verified) = select_linkable_email(&user(Some("carol@example.com"), None));
        assert!(!verified, "a missing flag must fail closed, not default to trusted");
    }

    #[test]
    fn an_absent_or_blank_email_is_no_email() {
        assert_eq!(select_linkable_email(&user(None, Some(true))), (None, false));
        assert_eq!(select_linkable_email(&user(Some("   "), Some(true))), (None, false));
    }

    #[test]
    fn discord_is_trusted_for_email_verification_by_default() {
        // Trust is warranted only because `verified` is honoured above; the two
        // travel together.
        assert!(crate::TrustedEmailProviders::default().is_trusted("discord"));
    }

    #[test]
    fn a_private_base_url_is_refused_by_the_ssrf_guard() {
        // The bypass is process-global, and sibling tests in this binary switch
        // it on to reach a loopback stub. Clear it for this assertion's window,
        // matching `github_endpoint_overrides_are_ssrf_guarded`.
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let err = DiscordOAuth::with_base_url(
                "c".to_string(),
                "s".to_string(),
                "https://app.example.com/cb".to_string(),
                "http://169.254.169.254".to_string(),
            )
            .expect_err("a link-local override must be refused");
            assert!(err.to_string().contains("SSRF") || err.to_string().contains("https"), "{err}");
        });
    }

    /// The whole flow, against a stub: an address Discord reports as
    /// unverified must arrive at the callback as **unverified**, so the account
    /// store keys it on `(discord, id)`. The trust gate is a second, separate
    /// belt — this pins the provider's own half.
    #[tokio::test]
    async fn user_info_reports_discords_verified_flag_verbatim() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        for (reported, expected) in [(Some(true), true), (Some(false), false), (None, false)] {
            let server = MockServer::start().await;
            let mut body = serde_json::json!({
                "id": "776655443322110099",
                "username": "carol",
                "email": "carol@example.com",
            });
            if let Some(v) = reported {
                body["verified"] = serde_json::json!(v);
            }
            Mock::given(method("GET"))
                .and(path("/api/users/@me"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;

            // Only *construction* consults the SSRF guard, so the bypass is held
            // for that call alone rather than across the HTTP round-trip. The
            // variable is process-global: a wider window races the guard tests
            // in this same binary, which is exactly how this suite first went
            // red in CI while passing locally.
            let provider = temp_env::with_vars(
                [
                    ("FRAISEQL_OIDC_ALLOW_INSECURE", Some("1")),
                    ("FRAISEQL_ENV", Some("development")),
                    ("FRAISEQL_PROFILE", None),
                    ("KUBERNETES_SERVICE_HOST", None),
                ],
                || {
                    DiscordOAuth::with_base_url(
                        "c".to_string(),
                        "s".to_string(),
                        "https://app.example.com/cb".to_string(),
                        server.uri(),
                    )
                    .expect("stub provider constructs")
                },
            );
            let info = provider.user_info("discord-at").await.expect("the user object parses");

            assert_eq!(info.id, "776655443322110099");
            assert_eq!(info.email.as_deref(), Some("carol@example.com"));
            assert_eq!(
                info.email_verified, expected,
                "verified: {reported:?} must surface as email_verified = {expected}"
            );
            assert_eq!(info.raw_claims["email_verified"], serde_json::json!(expected));
        }
    }
}

/// Facebook (#944).
mod facebook_tests {
    use crate::{
        provider::OAuthProvider as _,
        providers::facebook::{DEFAULT_API_VERSION, FacebookOAuth},
    };

    fn provider() -> FacebookOAuth {
        FacebookOAuth::new(
            "facebook-client".to_string(),
            "facebook-secret".to_string(),
            "https://app.example.com/auth/v1/callback".to_string(),
        )
        .expect("construction is network-free")
    }

    #[test]
    fn the_api_version_is_in_the_path_and_configurable() {
        let url = provider().authorization_url("state-abc");
        assert!(
            url.starts_with(&format!(
                "https://www.facebook.com/{DEFAULT_API_VERSION}/dialog/oauth?"
            )),
            "{url}"
        );

        // Meta deprecates versions on its own schedule, so an operator must be
        // able to move without waiting for a FraiseQL release.
        let pinned = FacebookOAuth::with_endpoints(
            "facebook-client".to_string(),
            "facebook-secret".to_string(),
            "https://app.example.com/auth/v1/callback".to_string(),
            "https://www.facebook.com".to_string(),
            "https://graph.facebook.com".to_string(),
            "v23.0".to_string(),
        )
        .expect("a later version constructs");
        assert!(
            pinned.authorization_url("s").contains("/v23.0/dialog/oauth"),
            "the configured version must reach the request path"
        );
    }

    #[test]
    fn an_api_version_that_would_repoint_the_request_is_refused() {
        for bad in [
            "",
            "  ",
            "v21.0/../../evil",
            "v21.0?x=1",
            "v21.0#f",
            "v21.0\\x",
        ] {
            FacebookOAuth::with_endpoints(
                "c".to_string(),
                "s".to_string(),
                "https://app.example.com/cb".to_string(),
                "https://www.facebook.com".to_string(),
                "https://graph.facebook.com".to_string(),
                bad.to_string(),
            )
            .expect_err(&format!("api_version {bad:?} lands in a URL path and must be refused"));
        }
    }

    #[test]
    fn facebook_is_never_trusted_for_email_verification() {
        // Two independent reasons an address cannot become a linking key: the
        // provider reports `email_verified = false` unconditionally (there is no
        // signal to report), and the trust gate would downgrade it anyway.
        assert!(!crate::TrustedEmailProviders::default().is_trusted("facebook"));
    }

    #[test]
    fn a_private_graph_url_is_refused_by_the_ssrf_guard() {
        // See the Discord twin: the bypass is process-global, so clear it here.
        temp_env::with_vars([("FRAISEQL_OIDC_ALLOW_INSECURE", None::<&str>)], || {
            let err = FacebookOAuth::with_endpoints(
                "c".to_string(),
                "s".to_string(),
                "https://app.example.com/cb".to_string(),
                "https://www.facebook.com".to_string(),
                "http://127.0.0.1:8080".to_string(),
                DEFAULT_API_VERSION.to_string(),
            )
            .expect_err("a loopback graph override must be refused");
            assert!(err.to_string().contains("SSRF") || err.to_string().contains("https"), "{err}");
        });
    }

    /// Facebook publishes no verification signal, so the provider itself must
    /// report `email_verified = false` — independently of the trust gate, which
    /// would also downgrade it. Two belts, proven one at a time.
    #[tokio::test]
    async fn user_info_never_claims_a_facebook_email_is_verified() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/{DEFAULT_API_VERSION}/me")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "10223344556677889",
                "name": "Dave",
                "email": "dave@example.com",
            })))
            .mount(&server)
            .await;

        // See the Discord twin: the bypass is held for construction only, because
        // it is process-global and a wider window races the guard tests.
        let provider = temp_env::with_vars(
            [
                ("FRAISEQL_OIDC_ALLOW_INSECURE", Some("1")),
                ("FRAISEQL_ENV", Some("development")),
                ("FRAISEQL_PROFILE", None),
                ("KUBERNETES_SERVICE_HOST", None),
            ],
            || {
                FacebookOAuth::with_endpoints(
                    "c".to_string(),
                    "s".to_string(),
                    "https://app.example.com/cb".to_string(),
                    "https://www.facebook.com".to_string(),
                    server.uri(),
                    DEFAULT_API_VERSION.to_string(),
                )
                .expect("stub provider constructs")
            },
        );
        let info = provider.user_info("facebook-at").await.expect("the profile parses");

        assert_eq!(info.id, "10223344556677889");
        assert_eq!(info.email.as_deref(), Some("dave@example.com"));
        assert!(
            !info.email_verified,
            "there is no signal to base a verified claim on, so it must be false"
        );
        assert_eq!(info.raw_claims["email_verified"], serde_json::json!(false));
    }
}
