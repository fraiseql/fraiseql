//! SCIM 2.0 resource representations and discovery documents (RFC 7643) (#946).
//!
//! The discovery trio — `/ServiceProviderConfig`, `/ResourceTypes`, `/Schemas` — is not
//! optional decoration: Okta, Entra and every conformance client fetch it first to decide
//! what the server supports, and a client that cannot discover the surface will not
//! provision against it.

use fraiseql_auth::scim::{ScimGroup, ScimUser};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// `urn:ietf:params:scim:schemas:core:2.0:User`.
pub const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
/// `urn:ietf:params:scim:schemas:core:2.0:Group`.
pub const GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
/// `urn:ietf:params:scim:api:messages:2.0:ListResponse`.
pub const LIST_RESPONSE_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
/// `urn:ietf:params:scim:api:messages:2.0:Error`.
pub const ERROR_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
/// `urn:ietf:params:scim:api:messages:2.0:PatchOp`.
pub const PATCH_OP_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
/// The SCIM media type. Clients send and expect it; a JSON-only server fails their checks.
pub const SCIM_CONTENT_TYPE: &str = "application/scim+json";

/// `meta.version` / `ETag` for a resource version. Weak because the entity tag tracks the
/// stored row, not a byte-for-byte rendering.
#[must_use]
pub fn etag(version: i64) -> String {
    format!("W/\"{version}\"")
}

/// Render a [`ScimUser`] as a SCIM `User` resource.
#[must_use]
pub fn user_to_json(user: &ScimUser, base_url: &str, groups: &[String]) -> Value {
    let mut value = json!({
        "schemas": [USER_SCHEMA],
        "id": user.id,
        "userName": user.user_name,
        "active": user.active,
        "meta": {
            "resourceType": "User",
            "created": user.created_at.to_rfc3339(),
            "lastModified": user.updated_at.to_rfc3339(),
            "location": format!("{base_url}/Users/{}", user.id),
            "version": etag(user.version),
        },
    });

    // SCIM omits absent attributes rather than sending nulls; a client that round-trips a
    // null into a PUT would otherwise be told it sent an invalid value.
    let obj = value.as_object_mut().expect("object literal");
    if let Some(external_id) = &user.external_id {
        obj.insert("externalId".to_string(), json!(external_id));
    }
    if let Some(display_name) = &user.display_name {
        obj.insert("displayName".to_string(), json!(display_name));
    }
    if user.given_name.is_some() || user.family_name.is_some() {
        let mut name = serde_json::Map::new();
        if let Some(given) = &user.given_name {
            name.insert("givenName".to_string(), json!(given));
        }
        if let Some(family) = &user.family_name {
            name.insert("familyName".to_string(), json!(family));
        }
        obj.insert("name".to_string(), Value::Object(name));
    }
    if let Some(email) = &user.email {
        obj.insert("emails".to_string(), json!([{ "value": email, "primary": true }]));
    }
    if !groups.is_empty() {
        obj.insert(
            "groups".to_string(),
            Value::Array(groups.iter().map(|g| json!({ "display": g })).collect()),
        );
    }
    value
}

/// Render a [`ScimGroup`] as a SCIM `Group` resource.
#[must_use]
pub fn group_to_json(group: &ScimGroup, base_url: &str) -> Value {
    let mut value = json!({
        "schemas": [GROUP_SCHEMA],
        "id": group.id,
        "displayName": group.display_name,
        "meta": {
            "resourceType": "Group",
            "created": group.created_at.to_rfc3339(),
            "lastModified": group.updated_at.to_rfc3339(),
            "location": format!("{base_url}/Groups/{}", group.id),
            "version": etag(group.version),
        },
    });
    let obj = value.as_object_mut().expect("object literal");
    if let Some(external_id) = &group.external_id {
        obj.insert("externalId".to_string(), json!(external_id));
    }
    // `value` only: `$ref`, `type` and `display` are optional sub-attributes (RFC 7643
    // §4.2) that buy a provisioning client nothing and give strict ones another shape to
    // disagree about. And an empty multi-valued attribute is omitted rather than sent as
    // `[]`, which a client that asked for it to be removed reads as "still there".
    if !group.members.is_empty() {
        obj.insert(
            "members".to_string(),
            Value::Array(group.members.iter().map(|m| json!({ "value": m })).collect()),
        );
    }
    value
}

/// Apply the `attributes` / `excludedAttributes` projection of RFC 7644 §3.9.
///
/// A client asks for a narrower resource to keep responses small; returning more than it
/// asked for is a conformance failure, not a harmless extra. `schemas`, `id` and `meta` are
/// always returned — RFC 7643 §7 marks them `returned = always`, so they survive both forms.
#[must_use]
pub fn project(mut resource: Value, attributes: Option<&str>, excluded: Option<&str>) -> Value {
    const ALWAYS: [&str; 3] = ["schemas", "id", "meta"];

    let Some(obj) = resource.as_object_mut() else {
        return resource;
    };
    if let Some(list) = attributes {
        let wanted: Vec<String> = list
            .split(',')
            .map(|a| a.trim().to_ascii_lowercase())
            .filter(|a| !a.is_empty())
            .collect();
        if !wanted.is_empty() {
            obj.retain(|key, _| {
                ALWAYS.contains(&key.as_str()) || wanted.contains(&key.to_ascii_lowercase())
            });
            return resource;
        }
    }
    if let Some(list) = excluded {
        let unwanted: Vec<String> = list
            .split(',')
            .map(|a| a.trim().to_ascii_lowercase())
            .filter(|a| !a.is_empty())
            .collect();
        obj.retain(|key, _| {
            ALWAYS.contains(&key.as_str()) || !unwanted.contains(&key.to_ascii_lowercase())
        });
    }
    resource
}

/// Wrap a page of resources in a SCIM `ListResponse`.
#[must_use]
pub fn list_response(resources: &[Value], total: i64, start_index: i64) -> Value {
    json!({
        "schemas": [LIST_RESPONSE_SCHEMA],
        "totalResults": total,
        "itemsPerPage": resources.len(),
        "startIndex": start_index,
        "Resources": resources,
    })
}

/// A SCIM error body (RFC 7644 §3.12). `scim_type` is the machine-readable reason a client
/// branches on — `uniqueness` in particular, which is how it learns a `userName` collided.
#[must_use]
pub fn error_response(status: u16, detail: &str, scim_type: Option<&str>) -> Value {
    let mut value = json!({
        "schemas": [ERROR_SCHEMA],
        "status": status.to_string(),
        "detail": detail,
    });
    if let Some(scim_type) = scim_type {
        value
            .as_object_mut()
            .expect("object literal")
            .insert("scimType".to_string(), json!(scim_type));
    }
    value
}

/// The inbound `User` body for POST and PUT.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserBody {
    /// Declared schema URIs. Accepted and not enforced: clients legitimately send vendor
    /// extensions alongside the core schema.
    #[serde(default)]
    pub schemas:      Vec<String>,
    /// SCIM `userName` — required by the core schema.
    #[serde(default)]
    pub user_name:    Option<String>,
    /// The `IdP`'s own identifier.
    #[serde(default)]
    pub external_id:  Option<String>,
    /// `name.givenName` / `name.familyName`.
    #[serde(default)]
    pub name:         Option<NameBody>,
    /// `displayName`.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Multi-valued emails; the primary one (or the first) becomes the account email.
    #[serde(default)]
    pub emails:       Vec<EmailBody>,
    /// SCIM `active`. Absent means active — RFC 7643 defaults a created user to enabled.
    #[serde(default)]
    pub active:       Option<bool>,
}

/// `name` sub-attribute.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameBody {
    /// `givenName`.
    #[serde(default)]
    pub given_name:  Option<String>,
    /// `familyName`.
    #[serde(default)]
    pub family_name: Option<String>,
}

/// One entry of the multi-valued `emails` attribute.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EmailBody {
    /// The address.
    #[serde(default)]
    pub value:   Option<String>,
    /// Whether this is the primary address.
    #[serde(default)]
    pub primary: Option<bool>,
}

impl UserBody {
    /// The address to store: the primary if one is marked, else the first present.
    #[must_use]
    pub fn primary_email(&self) -> Option<String> {
        self.emails
            .iter()
            .find(|e| e.primary == Some(true))
            .or_else(|| self.emails.first())
            .and_then(|e| e.value.clone())
    }
}

/// The inbound `Group` body for POST and PUT.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupBody {
    /// Declared schema URIs.
    #[serde(default)]
    pub schemas:      Vec<String>,
    /// SCIM `displayName` — required by the core schema.
    #[serde(default)]
    pub display_name: Option<String>,
    /// The `IdP`'s own identifier.
    #[serde(default)]
    pub external_id:  Option<String>,
    /// Members, each `value` being a user id.
    #[serde(default)]
    pub members:      Vec<MemberBody>,
}

/// One entry of the multi-valued `members` attribute.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MemberBody {
    /// The member's user id.
    #[serde(default)]
    pub value: Option<String>,
}

impl GroupBody {
    /// Member user ids, dropping entries with no `value`.
    #[must_use]
    pub fn member_ids(&self) -> Vec<String> {
        self.members.iter().filter_map(|m| m.value.clone()).collect()
    }
}

/// A `PatchOp` body (RFC 7644 §3.5.2).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PatchBody {
    /// Declared schema URIs.
    #[serde(default)]
    pub schemas:    Vec<String>,
    /// The operations, applied in order.
    #[serde(default, rename = "Operations")]
    pub operations: Vec<PatchOperation>,
}

/// One patch operation.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PatchOperation {
    /// `add`, `remove` or `replace`. Case-insensitive per the RFC.
    #[serde(default)]
    pub op:    String,
    /// The attribute path, absent for a whole-resource `replace`.
    #[serde(default)]
    pub path:  Option<String>,
    /// The value, whose shape depends on the path.
    #[serde(default)]
    pub value: Option<Value>,
}

/// `/ServiceProviderConfig` — what this server supports.
///
/// Every flag is the truth about the implementation, not an aspiration: a client that is
/// told `patch.supported = true` will send PATCH and fail if it is not really there.
#[must_use]
pub fn service_provider_config(base_url: &str) -> Value {
    json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
        "documentationUri": "https://github.com/fraiseql/fraiseql/blob/dev/docs/auth/scim.md",
        "patch":         { "supported": true },
        "bulk":          { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
        "filter":        { "supported": true, "maxResults": 200 },
        "changePassword":{ "supported": false },
        "sort":          { "supported": false },
        "etag":          { "supported": true },
        "authenticationSchemes": [{
            "type": "oauthbearertoken",
            "name": "OAuth Bearer Token",
            "description": "Provisioning bearer token, distinct from the admin token",
            "specUri": "http://www.rfc-editor.org/info/rfc6750",
            "primary": true,
        }],
        "meta": {
            "resourceType": "ServiceProviderConfig",
            "location": format!("{base_url}/ServiceProviderConfig"),
        },
    })
}

/// `/ResourceTypes` — the resources this server exposes.
#[must_use]
pub fn resource_types(base_url: &str) -> Vec<Value> {
    vec![
        json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"],
            "id": "User",
            "name": "User",
            "endpoint": "/Users",
            "description": "User Account",
            "schema": USER_SCHEMA,
            "meta": {
                "resourceType": "ResourceType",
                "location": format!("{base_url}/ResourceTypes/User"),
            },
        }),
        json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"],
            "id": "Group",
            "name": "Group",
            "endpoint": "/Groups",
            "description": "Group",
            "schema": GROUP_SCHEMA,
            "meta": {
                "resourceType": "ResourceType",
                "location": format!("{base_url}/ResourceTypes/Group"),
            },
        }),
    ]
}

/// `/Schemas` — the attribute definitions for `User` and `Group`.
#[must_use]
pub fn schemas(base_url: &str) -> Vec<Value> {
    fn attr(name: &str, ty: &str, multi: bool, required: bool, unique: &str) -> Value {
        json!({
            "name": name,
            "type": ty,
            "multiValued": multi,
            "required": required,
            "caseExact": false,
            "mutability": "readWrite",
            "returned": "default",
            "uniqueness": unique,
        })
    }

    vec![
        json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Schema"],
            "id": USER_SCHEMA,
            "name": "User",
            "description": "User Account",
            "attributes": [
                attr("userName", "string", false, true, "server"),
                json!({
                    "name": "name",
                    "type": "complex",
                    "multiValued": false,
                    "required": false,
                    "mutability": "readWrite",
                    "returned": "default",
                    "subAttributes": [
                        attr("givenName", "string", false, false, "none"),
                        attr("familyName", "string", false, false, "none"),
                    ],
                }),
                attr("displayName", "string", false, false, "none"),
                json!({
                    "name": "emails",
                    "type": "complex",
                    "multiValued": true,
                    "required": false,
                    "mutability": "readWrite",
                    "returned": "default",
                    "subAttributes": [
                        attr("value", "string", false, false, "none"),
                        attr("primary", "boolean", false, false, "none"),
                    ],
                }),
                attr("active", "boolean", false, false, "none"),
            ],
            "meta": {
                "resourceType": "Schema",
                "location": format!("{base_url}/Schemas/{USER_SCHEMA}"),
            },
        }),
        json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Schema"],
            "id": GROUP_SCHEMA,
            "name": "Group",
            "description": "Group",
            "attributes": [
                attr("displayName", "string", false, true, "none"),
                json!({
                    "name": "members",
                    "type": "complex",
                    "multiValued": true,
                    "required": false,
                    "mutability": "readWrite",
                    "returned": "default",
                    "subAttributes": [attr("value", "string", false, false, "none")],
                }),
            ],
            "meta": {
                "resourceType": "Schema",
                "location": format!("{base_url}/Schemas/{GROUP_SCHEMA}"),
            },
        }),
    ]
}
