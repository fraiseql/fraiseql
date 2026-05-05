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
        let roles = azure_ad::AzureADOAuth::map_azure_roles_to_fraiseql(vec![
            "fraiseql.admin".to_string(),
        ]);
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
}
