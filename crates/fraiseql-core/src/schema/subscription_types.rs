//! Subscription definition types for GraphQL subscriptions.

use serde::{Deserialize, Serialize};

use super::compiled::ArgumentDefinition;

/// A subscription definition.
///
/// Subscriptions are declarative bindings to event topics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionDefinition {
    /// Subscription name.
    pub name: String,

    /// Return type name.
    pub return_type: String,

    /// Arguments.
    #[serde(default)]
    pub arguments: Vec<ArgumentDefinition>,

    /// Description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Event topic to subscribe to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,

    /// Compiled filter expression for event matching.
    /// Maps argument names to JSONB paths in event data.
    /// Example: `{"orderId": "$.id", "status": "$.order_status"}`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<SubscriptionFilter>,

    /// Fields to project from event data.
    /// If empty, all fields are returned.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,

    /// Shorthand: argument names that should auto-generate `argument_paths`
    /// entries using `/<field_name>` as the JSON pointer path.
    ///
    /// Example: `filter_fields: ["user_id"]` generates an `argument_paths`
    /// entry of `"user_id" -> "/user_id"` at runtime.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter_fields: Vec<String>,

    /// Deprecation information (from @deprecated directive).
    /// When set, this subscription is marked as deprecated in the schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecation: Option<super::field_type::DeprecationInfo>,
}

/// Filter configuration for subscription event matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionFilter {
    /// Mapping of argument names to JSONB paths in event data.
    /// The path uses JSON pointer syntax (e.g., "/id", "/user/name").
    #[serde(default)]
    pub argument_paths: std::collections::HashMap<String, String>,

    /// Static filter conditions that must always match.
    /// Each entry is a path and expected value.
    #[serde(default)]
    pub static_filters: Vec<StaticFilterCondition>,
}

/// A static filter condition for subscription matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticFilterCondition {
    /// JSONB path in event data.
    pub path:     String,
    /// Comparison operator.
    pub operator: FilterOperator,
    /// Value to compare against.
    pub value:    serde_json::Value,
}

/// Filter comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    /// Equals (==).
    Eq,
    /// Not equals (!=).
    Ne,
    /// Greater than (>).
    Gt,
    /// Greater than or equal (>=).
    Gte,
    /// Less than (<).
    Lt,
    /// Less than or equal (<=).
    Lte,
    /// Contains (for arrays/strings).
    Contains,
    /// Starts with (for strings).
    StartsWith,
    /// Ends with (for strings).
    EndsWith,
}

impl SubscriptionDefinition {
    /// Create a new subscription definition.
    #[must_use]
    pub fn new(name: impl Into<String>, return_type: impl Into<String>) -> Self {
        Self {
            name:          name.into(),
            return_type:   return_type.into(),
            arguments:     Vec::new(),
            description:   None,
            topic:         None,
            filter:        None,
            fields:        Vec::new(),
            filter_fields: Vec::new(),
            deprecation:   None,
        }
    }

    /// Set the event topic for this subscription.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::SubscriptionDefinition;
    ///
    /// let subscription = SubscriptionDefinition::new("orderCreated", "Order")
    ///     .with_topic("order_created");
    /// assert_eq!(subscription.topic, Some("order_created".to_string()));
    /// ```
    #[must_use]
    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Set the description for this subscription.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an argument to this subscription.
    #[must_use]
    pub fn with_argument(mut self, arg: ArgumentDefinition) -> Self {
        self.arguments.push(arg);
        self
    }

    /// Set the filter configuration for event matching.
    #[must_use]
    pub fn with_filter(mut self, filter: SubscriptionFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set the fields to project from event data.
    #[must_use]
    pub fn with_fields(mut self, fields: Vec<String>) -> Self {
        self.fields = fields;
        self
    }

    /// Add a field to project from event data.
    #[must_use]
    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.fields.push(field.into());
        self
    }

    /// Mark this subscription as deprecated.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::SubscriptionDefinition;
    ///
    /// let subscription = SubscriptionDefinition::new("oldUserEvents", "User")
    ///     .deprecated(Some("Use 'userEvents' instead".to_string()));
    /// assert!(subscription.is_deprecated());
    /// ```
    #[must_use]
    pub fn deprecated(mut self, reason: Option<String>) -> Self {
        self.deprecation = Some(super::field_type::DeprecationInfo { reason });
        self
    }

    /// Check if this subscription is deprecated.
    #[must_use]
    pub const fn is_deprecated(&self) -> bool {
        self.deprecation.is_some()
    }

    /// Get the deprecation reason if deprecated.
    #[must_use]
    pub fn deprecation_reason(&self) -> Option<&str> {
        self.deprecation.as_ref().and_then(|d| d.reason.as_deref())
    }
}
