#!/usr/bin/env python3
"""SQL Parser Utilities for FraiseQL Compliance Verification

Provides utilities to parse SQL files and extract information for rule verification.
Used by Phase 3 automated verification.
"""

import re
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Set, Tuple


@dataclass
class SQLTable:
    """Represents a parsed SQL table definition."""

    name: str
    columns: Dict[str, str]  # column_name -> type
    primary_keys: List[str]
    foreign_keys: Dict[str, Tuple[str, str]]  # fk_column -> (ref_table, ref_column)
    unique_constraints: List[List[str]]
    indexes: List[Dict[str, Any]]


@dataclass
class SQLView:
    """Represents a parsed SQL view definition."""

    name: str
    select_columns: List[str]
    jsonb_fields: Set[str]
    has_data_column: bool
    joins: List[Dict[str, Any]]


@dataclass
class SQLFunction:
    """Represents a parsed SQL function definition."""

    name: str
    parameters: List[Tuple[str, str]]  # (param_name, param_type)
    return_type: str
    body: str
    variables: Dict[str, str]  # var_name -> type


class SQLParser:
    """Parser for SQL files to extract compliance information."""

    def __init__(self):
        self.table_pattern = re.compile(
            r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s*\((.*?)\);",
            re.DOTALL | re.IGNORECASE,
        )

        self.view_pattern = re.compile(
            r"CREATE\s+(?:OR\s+REPLACE\s+)?VIEW\s+(\w+)\s+AS\s+(SELECT.*?);",
            re.DOTALL | re.IGNORECASE,
        )

        self.function_pattern = re.compile(
            r"CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+(\w+)\s*\((.*?)\)\s+RETURNS\s+(\w+)\s+AS\s+\$\$([\s\S]*?)\$\$",
            re.DOTALL | re.IGNORECASE,
        )

    def parse_file(self, file_path: str) -> Dict[str, Any]:
        """Parse a SQL file and return structured information."""
        with open(file_path) as f:
            content = f.read()

        result = {
            "file_path": file_path,
            "tables": [],
            "views": [],
            "functions": [],
            "raw_content": content,
        }

        # Parse tables
        for match in self.table_pattern.finditer(content):
            table = self._parse_table(match.group(1), match.group(2))
            if table:
                result["tables"].append(table)

        # Parse views
        for match in self.view_pattern.finditer(content):
            view = self._parse_view(match.group(1), match.group(2))
            if view:
                result["views"].append(view)

        # Parse functions
        for match in self.function_pattern.finditer(content):
            function = self._parse_function(
                match.group(1), match.group(2), match.group(3), match.group(4)
            )
            if function:
                result["functions"].append(function)

        return result

    def _parse_table(self, name: str, definition: str) -> Optional[SQLTable]:
        """Parse a CREATE TABLE statement."""
        columns = {}
        primary_keys = []
        foreign_keys = {}
        unique_constraints = []
        indexes = []

        # Split by commas and clean up
        lines = [line.strip() for line in definition.split(",") if line.strip()]

        for line in lines:
            line = line.strip()
            if not line or line.startswith("--"):
                continue

            # Primary key
            if "PRIMARY KEY" in line.upper():
                pk_match = re.search(r"(\w+)\s+.*PRIMARY\s+KEY", line, re.IGNORECASE)
                if pk_match:
                    primary_keys.append(pk_match.group(1))

            # Foreign key
            if "REFERENCES" in line.upper():
                fk_match = re.search(
                    r"(\w+)\s+.*REFERENCES\s+(\w+)\s*\(\s*(\w+)\s*\)", line, re.IGNORECASE
                )
                if fk_match:
                    fk_col, ref_table, ref_col = fk_match.groups()
                    foreign_keys[fk_col] = (ref_table, ref_col)

            # Unique constraint
            if "UNIQUE" in line.upper() and "REFERENCES" not in line.upper():
                unique_match = re.search(r"(\w+)\s+.*UNIQUE", line, re.IGNORECASE)
                if unique_match:
                    unique_constraints.append([unique_match.group(1)])

            # Column definition (simplified)
            col_match = re.match(r"(\w+)\s+(\w+)", line)
            if col_match and col_match.group(1) not in [
                "PRIMARY",
                "FOREIGN",
                "UNIQUE",
                "CONSTRAINT",
            ]:
                col_name, col_type = col_match.groups()
                columns[col_name] = col_type

        return SQLTable(
            name=name,
            columns=columns,
            primary_keys=primary_keys,
            foreign_keys=foreign_keys,
            unique_constraints=unique_constraints,
            indexes=indexes,
        )

    def _parse_view(self, name: str, select_stmt: str) -> Optional[SQLView]:
        """Parse a CREATE VIEW statement."""
        select_columns = []
        jsonb_fields = set()
        has_data_column = False
        joins = []

        # Extract SELECT columns
        select_match = re.search(r"SELECT\s+(.*?)\s+FROM", select_stmt, re.IGNORECASE | re.DOTALL)
        if select_match:
            select_part = select_match.group(1)
            # Split by commas, but be careful with jsonb_build_object
            columns = self._split_select_columns(select_part)
            select_columns = [col.strip() for col in columns]

            # Check for data column
            has_data_column = any("data" in col.lower() for col in select_columns)

            # Extract JSONB fields
            jsonb_match = re.search(
                r"jsonb_build_object\s*\((.*?)\)", select_part, re.IGNORECASE | re.DOTALL
            )
            if jsonb_match:
                jsonb_content = jsonb_match.group(1)
                # Extract field names (every other item, starting with first)
                fields = re.findall(r"'(\w+)'", jsonb_content)
                jsonb_fields = set(fields)

        # Extract JOINs
        join_matches = re.findall(r"JOIN\s+(\w+)\s+\w+\s+ON\s+([^,]+)", select_stmt, re.IGNORECASE)
        for table, condition in join_matches:
            joins.append({"table": table, "condition": condition.strip()})

        return SQLView(
            name=name,
            select_columns=select_columns,
            jsonb_fields=jsonb_fields,
            has_data_column=has_data_column,
            joins=joins,
        )

    def _split_select_columns(self, select_part: str) -> List[str]:
        """Split SELECT columns, handling nested functions."""
        columns = []
        current = ""
        paren_depth = 0

        for char in select_part:
            if char == "(":
                paren_depth += 1
            elif char == ")":
                paren_depth -= 1
            elif char == "," and paren_depth == 0:
                columns.append(current)
                current = ""
                continue
            current += char

        if current:
            columns.append(current)

        return columns

    def _parse_function(
        self, name: str, params_str: str, return_type: str, body: str
    ) -> Optional[SQLFunction]:
        """Parse a CREATE FUNCTION statement."""
        parameters = []
        variables = {}

        # Parse parameters
        if params_str.strip():
            param_parts = [p.strip() for p in params_str.split(",")]
            for param in param_parts:
                param_match = re.match(r"(\w+)\s+(\w+)", param)
                if param_match:
                    param_name, param_type = param_match.groups()
                    parameters.append((param_name, param_type))

        # Parse variables from DECLARE block
        declare_match = re.search(r"DECLARE\s+(.*?);", body, re.IGNORECASE | re.DOTALL)
        if declare_match:
            declare_block = declare_match.group(1)
            var_lines = [line.strip() for line in declare_block.split("\n") if line.strip()]
            for line in var_lines:
                var_match = re.match(r"(\w+)\s+(\w+)", line)
                if var_match:
                    var_name, var_type = var_match.groups()
                    variables[var_name] = var_type

        return SQLFunction(
            name=name,
            parameters=parameters,
            return_type=return_type,
            body=body,
            variables=variables,
        )


class ComplianceChecker:
    """Checks SQL files against compliance rules."""

    def __init__(self, rules_file: str):
        self.parser = SQLParser()
        self.rules = self._load_rules(rules_file)

    def _load_rules(self, rules_file: str) -> Dict[str, Any]:
        """Load rules from YAML file."""
        try:
            import yaml

            with open(rules_file) as f:
                return yaml.safe_load(f)
        except ImportError:
            # Fallback if yaml not available
            return {}

    def check_file(self, file_path: str) -> Dict[str, Any]:
        """Check a SQL file against all rules."""
        parsed = self.parser.parse_file(file_path)
        violations = []
        compliance_score = 0

        # Check each rule
        for rule_id, rule in self.rules.get("rules", {}).items():
            result = self._check_rule(rule_id, rule, parsed)
            if result["violations"]:
                violations.extend(result["violations"])
            compliance_score += result["score"]

        return {
            "file_path": file_path,
            "total_rules": len(self.rules.get("rules", {})),
            "violations": violations,
            "compliance_score": compliance_score,
            "parsed_data": parsed,
        }

    def _check_rule(
        self, rule_id: str, rule: Dict[str, Any], parsed: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Check a single rule against parsed SQL."""
        violations = []
        score = 0

        # This would implement the actual rule checking logic
        # For now, return placeholder
        return {"rule_id": rule_id, "violations": violations, "score": score}


def main():
    """Command-line interface for SQL parsing."""
    import argparse

    parser = argparse.ArgumentParser(description="Parse SQL files for compliance checking")
    parser.add_argument("file", help="SQL file to parse")
    parser.add_argument("--format", choices=["json", "text"], default="text", help="Output format")

    args = parser.parse_args()

    sql_parser = SQLParser()
    result = sql_parser.parse_file(args.file)

    if args.format == "json":
        import json

        print(json.dumps(result, indent=2, default=str))
    else:
        print(f"File: {result['file_path']}")
        print(f"Tables: {len(result['tables'])}")
        print(f"Views: {len(result['views'])}")
        print(f"Functions: {len(result['functions'])}")

        for table in result["tables"]:
            print(f"  Table: {table.name}")
            print(f"    Columns: {list(table.columns.keys())}")
            print(f"    PKs: {table.primary_keys}")
            print(f"    FKs: {table.foreign_keys}")

        for view in result["views"]:
            print(f"  View: {view.name}")
            print(f"    Columns: {view.select_columns}")
            print(f"    JSONB fields: {view.jsonb_fields}")
            print(f"    Has data column: {view.has_data_column}")


if __name__ == "__main__":
    main()
