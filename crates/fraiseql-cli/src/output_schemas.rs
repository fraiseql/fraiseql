//! Output schemas for CLI commands
//!
//! Provides JSON Schema definitions for the output of each command,
//! enabling AI agents to understand and validate command responses.

use serde_json::{Value, json};

use crate::output::OutputSchema;

/// Get the output schema for a specific command
pub fn get_output_schema(command: &str) -> Option<OutputSchema> {
    let (success, error) = match command {
        "validate" => (validate_success_schema(), validation_error_schema()),
        "lint" => (lint_success_schema(), validation_error_schema()),
        "analyze" => (analyze_success_schema(), error_schema()),
        "explain" => (explain_success_schema(), error_schema()),
        "cost" => (cost_success_schema(), error_schema()),
        "dependency-graph" => (dependency_graph_success_schema(), error_schema()),
        _ => return None,
    };

    Some(OutputSchema {
        command: command.to_string(),
        schema_version: "1.0".to_string(),
        format: "json".to_string(),
        success,
        error,
    })
}

/// List all commands that have output schemas
pub fn list_schema_commands() -> Vec<&'static str> {
    vec![
        "validate",
        "lint",
        "analyze",
        "explain",
        "cost",
        "dependency-graph",
    ]
}

fn validation_error_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "errors"],
        "properties": {
            "status": {
                "type": "string",
                "const": "validation-failed"
            },
            "command": {
                "type": "string"
            },
            "errors": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 1
            }
        }
    })
}

fn validate_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": { "type": "string", "const": "success" },
            "command": { "type": "string", "const": "validate" },
            "data": {
                "type": "object",
                "required": ["schema_path", "valid", "type_count", "query_count",
                             "mutation_count"],
                "properties": {
                    "schema_path": { "type": "string" },
                    "valid": { "type": "boolean" },
                    "type_count": { "type": "integer" },
                    "query_count": { "type": "integer" },
                    "mutation_count": { "type": "integer" },
                    "cycles": {
                        "type": "array",
                        "items": { "type": "object" }
                    },
                    "unused_types": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "type_analysis": {
                        "type": "array",
                        "items": { "type": "object" }
                    }
                }
            }
        }
    })
}

fn lint_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": { "type": "string", "const": "success" },
            "command": { "type": "string", "const": "lint" },
            "data": {
                "type": "object",
                "required": ["overall_score", "severity_counts", "categories"],
                "properties": {
                    "overall_score": { "type": "integer", "minimum": 0, "maximum": 100 },
                    "severity_counts": {
                        "type": "object",
                        "required": ["critical", "warning", "info"],
                        "properties": {
                            "critical": { "type": "integer" },
                            "warning": { "type": "integer" },
                            "info": { "type": "integer" }
                        }
                    },
                    "categories": {
                        "type": "object",
                        "required": ["federation", "cost", "cache", "authorization"],
                        "properties": {
                            "federation": { "type": "integer" },
                            "cost": { "type": "integer" },
                            "cache": { "type": "integer" },
                            "authorization": { "type": "integer" }
                        }
                    }
                }
            }
        }
    })
}

fn analyze_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": { "type": "string", "const": "success" },
            "command": { "type": "string", "const": "analyze" },
            "data": {
                "type": "object",
                "required": ["schema_file", "recommendations", "facts", "summary"],
                "properties": {
                    "schema_file": { "type": "string" },
                    "recommendations": {
                        "type": "array",
                        "items": { "type": "object" }
                    },
                    "facts": { "type": "object" },
                    "summary": { "type": "object" }
                }
            }
        }
    })
}

fn explain_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": { "type": "string", "const": "success" },
            "command": { "type": "string", "const": "explain" },
            "data": {
                "type": "object",
                "required": ["query", "estimated_cost", "complexity"],
                "properties": {
                    "query": { "type": "string" },
                    "estimated_cost": { "type": "integer" },
                    "complexity": {
                        "type": "object",
                        "required": ["depth", "score", "alias_count"],
                        "properties": {
                            "depth": { "type": "integer" },
                            "score": { "type": "integer" },
                            "alias_count": { "type": "integer" }
                        }
                    },
                    "warnings": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                }
            }
        }
    })
}

fn cost_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": { "type": "string", "const": "success" },
            "command": { "type": "string", "const": "cost" },
            "data": {
                "type": "object",
                "required": ["query", "complexity_score", "estimated_cost", "depth", "alias_count"],
                "properties": {
                    "query": { "type": "string" },
                    "complexity_score": { "type": "integer" },
                    "estimated_cost": { "type": "integer" },
                    "depth": { "type": "integer" },
                    "alias_count": { "type": "integer" }
                }
            }
        }
    })
}

fn dependency_graph_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": { "type": "string", "const": "success" },
            "command": { "type": "string", "const": "dependency-graph" },
            "data": {
                "type": "object",
                "required": ["type_count", "nodes", "edges", "cycles", "unused_types", "stats"],
                "properties": {
                    "type_count": { "type": "integer" },
                    "nodes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["name", "dependency_count", "dependent_count",
                                         "is_root"],
                            "properties": {
                                "name": { "type": "string" },
                                "dependency_count": { "type": "integer" },
                                "dependent_count": { "type": "integer" },
                                "is_root": { "type": "boolean" }
                            }
                        }
                    },
                    "edges": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["from", "to"],
                            "properties": {
                                "from": { "type": "string" },
                                "to": { "type": "string" }
                            }
                        }
                    },
                    "cycles": {
                        "type": "array",
                        "items": { "type": "object" }
                    },
                    "unused_types": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "stats": { "type": "object" }
                }
            }
        }
    })
}
fn error_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "message", "code"],
        "properties": {
            "status": {
                "type": "string",
                "const": "error"
            },
            "command": {
                "type": "string"
            },
            "message": {
                "type": "string",
                "description": "Human-readable error message"
            },
            "code": {
                "type": "string",
                "description": "Machine-readable error code"
            }
        }
    })
}
