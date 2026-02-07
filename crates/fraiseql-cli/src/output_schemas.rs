//! Output schemas for CLI commands
//!
//! Provides JSON Schema definitions for the output of each command,
//! enabling AI agents to understand and validate command responses.

use serde_json::{Value, json};

use crate::output::OutputSchema;

/// Get the output schema for a specific command
pub fn get_output_schema(command: &str) -> Option<OutputSchema> {
    let (success, error) = match command {
        "compile" => (compile_success_schema(), error_schema()),
        "validate" => (validate_success_schema(), validation_error_schema()),
        "lint" => (lint_success_schema(), error_schema()),
        "analyze" => (analyze_success_schema(), error_schema()),
        "explain" => (explain_success_schema(), error_schema()),
        "cost" => (cost_success_schema(), error_schema()),
        "dependency-graph" => (dependency_graph_success_schema(), error_schema()),
        _ => return None,
    };

    Some(OutputSchema {
        command:        command.to_string(),
        schema_version: "1.0".to_string(),
        format:         "json".to_string(),
        success,
        error,
    })
}

/// List all commands that have output schemas
pub fn list_schema_commands() -> Vec<&'static str> {
    vec![
        "compile",
        "validate",
        "lint",
        "analyze",
        "explain",
        "cost",
        "dependency-graph",
    ]
}

fn compile_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": {
                "type": "string",
                "const": "success"
            },
            "command": {
                "type": "string",
                "const": "compile"
            },
            "data": {
                "type": "object",
                "properties": {
                    "output_file": {
                        "type": "string",
                        "description": "Path to the generated schema.compiled.json"
                    },
                    "types_count": {
                        "type": "integer",
                        "description": "Number of types compiled"
                    },
                    "queries_count": {
                        "type": "integer",
                        "description": "Number of queries compiled"
                    },
                    "mutations_count": {
                        "type": "integer",
                        "description": "Number of mutations compiled"
                    }
                }
            },
            "warnings": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
}

fn validate_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": {
                "type": "string",
                "enum": ["success", "validation-failed"]
            },
            "command": {
                "type": "string",
                "const": "validate"
            },
            "data": {
                "type": "object",
                "properties": {
                    "types_validated": {
                        "type": "integer"
                    },
                    "cycles_detected": {
                        "type": "array",
                        "items": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    },
                    "unused_types": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                }
            },
            "errors": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Validation errors (when status is validation-failed)"
            },
            "warnings": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    })
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

fn lint_success_schema() -> Value {
    json!({
        "type": "object",
        "required": ["status", "command", "data"],
        "properties": {
            "status": {
                "type": "string",
                "const": "success"
            },
            "command": {
                "type": "string",
                "const": "lint"
            },
            "data": {
                "type": "object",
                "properties": {
                    "audits": {
                        "type": "object",
                        "additionalProperties": {
                            "type": "object",
                            "properties": {
                                "issues": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "severity": { "type": "string", "enum": ["critical", "warning", "info"] },
                                            "message": { "type": "string" },
                                            "location": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "summary": {
                        "type": "object",
                        "properties": {
                            "critical": { "type": "integer" },
                            "warning": { "type": "integer" },
                            "info": { "type": "integer" }
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
            "status": {
                "type": "string",
                "const": "success"
            },
            "command": {
                "type": "string",
                "const": "analyze"
            },
            "data": {
                "type": "object",
                "properties": {
                    "recommendations": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "category": {
                                    "type": "string",
                                    "enum": ["performance", "security", "federation", "complexity", "caching", "indexing"]
                                },
                                "severity": { "type": "string" },
                                "message": { "type": "string" },
                                "suggestion": { "type": "string" }
                            }
                        }
                    }
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
            "status": {
                "type": "string",
                "const": "success"
            },
            "command": {
                "type": "string",
                "const": "explain"
            },
            "data": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "execution_plan": {
                        "type": "object",
                        "properties": {
                            "steps": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        }
                    },
                    "sql": { "type": "string" },
                    "complexity": {
                        "type": "object",
                        "properties": {
                            "depth": { "type": "integer" },
                            "field_count": { "type": "integer" },
                            "score": { "type": "integer" }
                        }
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
            "status": {
                "type": "string",
                "const": "success"
            },
            "command": {
                "type": "string",
                "const": "cost"
            },
            "data": {
                "type": "object",
                "required": ["depth", "field_count", "score"],
                "properties": {
                    "depth": {
                        "type": "integer",
                        "description": "Maximum nesting depth of the query"
                    },
                    "field_count": {
                        "type": "integer",
                        "description": "Total number of fields requested"
                    },
                    "score": {
                        "type": "integer",
                        "description": "Calculated complexity score"
                    }
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
            "status": {
                "type": "string",
                "const": "success"
            },
            "command": {
                "type": "string",
                "const": "dependency-graph"
            },
            "data": {
                "type": "object",
                "properties": {
                    "format": {
                        "type": "string",
                        "enum": ["json", "dot", "mermaid", "d2", "console"]
                    },
                    "graph": {
                        "type": "object",
                        "description": "Graph data (format depends on output format)",
                        "properties": {
                            "nodes": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "name": { "type": "string" },
                                        "type": { "type": "string" }
                                    }
                                }
                            },
                            "edges": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "from": { "type": "string" },
                                        "to": { "type": "string" },
                                        "relationship": { "type": "string" }
                                    }
                                }
                            }
                        }
                    },
                    "cycles": {
                        "type": "array",
                        "items": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    },
                    "unused_types": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_output_schema_compile() {
        let schema = get_output_schema("compile");
        assert!(schema.is_some());
        let schema = schema.unwrap();
        assert_eq!(schema.command, "compile");
        assert_eq!(schema.format, "json");
    }

    #[test]
    fn test_get_output_schema_unknown() {
        let schema = get_output_schema("unknown-command");
        assert!(schema.is_none());
    }

    #[test]
    fn test_list_schema_commands() {
        let commands = list_schema_commands();
        assert!(commands.contains(&"compile"));
        assert!(commands.contains(&"validate"));
        assert!(commands.contains(&"lint"));
    }

    #[test]
    fn test_success_schema_structure() {
        let schema = get_output_schema("cost").unwrap();
        let success = &schema.success;

        assert_eq!(success["type"], "object");
        assert!(success["required"].is_array());
        assert!(success["properties"].is_object());
    }

    #[test]
    fn test_error_schema_structure() {
        let schema = get_output_schema("compile").unwrap();
        let error = &schema.error;

        assert_eq!(error["type"], "object");
        assert!(error["properties"]["message"].is_object());
        assert!(error["properties"]["code"].is_object());
    }
}
