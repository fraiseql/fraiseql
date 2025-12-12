#!/usr/bin/env python3
"""SQL Parser and Analyzer for FraiseQL Compliance Verification

Parses SQL files to extract table, view, and function definitions for pattern verification.
"""

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional


@dataclass
class TableDefinition:
    """Parsed table structure."""

    name: str
    columns: Dict[str, Dict[str, Any]]  # column_name -> {type, constraints}
    primary_keys: List[str]
    unique_columns: List[str]
    foreign_keys: List[Dict[str, str]]  # {column, references_table, references_column}

    def has_trinity_pattern(self) -> Dict[str, bool]:
        """Check if table follows Trinity pattern."""
        return {
            "has_pk": any(col.startswith("pk_") for col in self.primary_keys),
            "has_id_uuid": "id" in self.columns and "UUID" in self.columns["id"]["type"].upper(),
            "has_identifier": "identifier" in self.columns,
            "pk_is_integer": any(
                "INTEGER" in self.columns[pk]["type"].upper()
                for pk in self.primary_keys
                if pk.startswith("pk_")
            ),
        }


@dataclass
class ViewDefinition:
    """Parsed view structure."""

    name: str
    select_columns: List[str]  # Direct SELECT columns (not in JSONB)
    jsonb_column: Optional[str]  # Name of JSONB column (usually 'data')
    jsonb_fields: List[str]  # Fields inside jsonb_build_object()
    joins: List[Dict[str, str]]  # {table, alias, condition}

    def has_id_column(self) -> bool:
        """Check if view has direct 'id' column (not just in JSONB)."""
        return "id" in self.select_columns

    def has_pk_column(self) -> bool:
        """Check if view includes pk_* column."""
        return any(col.startswith("pk_") for col in self.select_columns)

    def jsonb_exposes_pk(self) -> bool:
        """Check if JSONB contains pk_* field (security violation!)."""
        return any(field.startswith("pk_") for field in self.jsonb_fields)


@dataclass
class FunctionDefinition:
    """Parsed function structure."""

    name: str
    schema: Optional[str]
    parameters: List[Dict[str, str]]  # {name, type}
    return_type: str
    language: str
    body: str

    def uses_helper_functions(self) -> bool:
        """Check if function uses core.get_pk_* helper functions."""
        return bool(re.search(r"core\.get_pk_\w+\(", self.body))

    def has_explicit_sync_calls(self) -> List[str]:
        """Find all sync function calls in function body."""
        sync_calls = re.findall(r"(?:PERFORM\s+)?(?:app\.|)(?:fn_|)?sync_tv_(\w+)\(\)", self.body)
        return sync_calls


class SQLAnalyzer:
    """Analyze SQL files for pattern compliance."""

    def __init__(self, sql_file: Path):
        self.sql_file = sql_file
        self.content = sql_file.read_text()

    def extract_tables(self) -> List[TableDefinition]:
        """Extract all CREATE TABLE statements."""
        tables = []
        # Regex to match CREATE TABLE ... ); blocks
        pattern = r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\s*\((.*?)\);"
        matches = re.finditer(pattern, self.content, re.DOTALL | re.IGNORECASE)

        for match in matches:
            table_name = match.group(1)
            columns_block = match.group(2)

            # Parse columns
            columns = self._parse_columns(columns_block)

            # Parse constraints
            primary_keys = self._extract_primary_keys(columns_block)
            unique_cols = self._extract_unique_columns(columns_block)
            foreign_keys = self._extract_foreign_keys(columns_block)

            tables.append(
                TableDefinition(
                    name=table_name,
                    columns=columns,
                    primary_keys=primary_keys,
                    unique_columns=unique_cols,
                    foreign_keys=foreign_keys,
                )
            )

        return tables

    def extract_views(self) -> List[ViewDefinition]:
        """Extract all CREATE VIEW statements."""
        views = []

        # Find all CREATE VIEW statements
        view_pattern = r"CREATE\s+(?:OR\s+REPLACE\s+)?VIEW\s+(\w+)\s+AS\s+"
        view_matches = list(re.finditer(view_pattern, self.content, re.IGNORECASE))

        for i, match in enumerate(view_matches):
            view_name = match.group(1)
            view_start = match.start()

            # Find the end of this view (next CREATE or end of file)
            if i + 1 < len(view_matches):
                view_end = view_matches[i + 1].start()
            else:
                view_end = len(self.content)

            view_content = self.content[view_start:view_end]

            # Extract SELECT clause (from SELECT to the last FROM before semicolon)
            select_match = re.search(r"SELECT\s+(.*?);", view_content, re.DOTALL | re.IGNORECASE)
            if not select_match:
                continue

            select_clause = select_match.group(1)

            # Extract direct SELECT columns
            select_columns = self._parse_select_columns(select_clause)

            # Find JSONB column and extract fields
            jsonb_col, jsonb_fields = self._extract_jsonb_structure(select_clause)

            # Extract JOINs from the view content
            joins = self._extract_joins(view_content)

            views.append(
                ViewDefinition(
                    name=view_name,
                    select_columns=select_columns,
                    jsonb_column=jsonb_col,
                    jsonb_fields=jsonb_fields,
                    joins=joins,
                )
            )

        return views

    def extract_functions(self) -> List[FunctionDefinition]:
        """Extract all CREATE FUNCTION statements."""
        functions = []
        # Pattern for CREATE FUNCTION ... RETURNS ... AS $$ ... $$
        pattern = r"CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+((?:\w+\.)?\w+)\s*\((.*?)\)\s+RETURNS\s+(\w+(?:\[\])?)\s+(?:AS|LANGUAGE)\s+\$\$(.*?)\$\$\s*LANGUAGE\s+(\w+)"

        matches = re.finditer(pattern, self.content, re.DOTALL | re.IGNORECASE)

        for match in matches:
            full_name = match.group(1)
            schema = None
            name = full_name
            if "." in full_name:
                schema, name = full_name.split(".", 1)

            params_str = match.group(2)
            return_type = match.group(3)
            body = match.group(4)
            language = match.group(5)

            # Parse parameters
            parameters = self._parse_parameters(params_str)

            functions.append(
                FunctionDefinition(
                    name=name,
                    schema=schema,
                    parameters=parameters,
                    return_type=return_type,
                    language=language,
                    body=body,
                )
            )

        return functions

    def _parse_columns(self, columns_block: str) -> Dict[str, Dict[str, Any]]:
        """Parse column definitions from CREATE TABLE."""
        columns = {}

        # Remove comments and clean up
        clean_block = re.sub(r"--.*$", "", columns_block, flags=re.MULTILINE)
        clean_block = re.sub(r"\s+", " ", clean_block)  # Normalize whitespace

        # Split by commas
        column_defs = [col.strip() for col in clean_block.split(",") if col.strip()]

        for col_def in column_defs:
            # Skip empty or constraint definitions
            if not col_def or col_def.upper().startswith(
                ("PRIMARY KEY", "FOREIGN KEY", "CONSTRAINT", "UNIQUE")
            ):
                continue

            # Parse column definition: name type [constraints]
            parts = col_def.split()
            if len(parts) >= 2:
                col_name = parts[0]

                # Skip if not a valid column name
                if not col_name or col_name.upper() in ["PRIMARY", "FOREIGN", "CONSTRAINT"]:
                    continue

                # Extract type and constraints
                col_type = parts[1]
                constraints = " ".join(parts[2:]) if len(parts) > 2 else ""

                columns[col_name] = {"type": col_type, "constraints": constraints}

        return columns

    def _extract_primary_keys(self, columns_block: str) -> List[str]:
        """Extract primary key columns."""
        pks = []
        # Remove comments first
        clean_block = re.sub(r"--.*$", "", columns_block, flags=re.MULTILINE)
        clean_block = re.sub(r"\s+", " ", clean_block)  # Normalize whitespace

        # Split by commas and look for PRIMARY KEY
        column_defs = clean_block.split(",")
        for col_def in column_defs:
            col_def = col_def.strip()
            if "PRIMARY KEY" in col_def.upper():
                # Extract column name (first word)
                parts = col_def.split()
                if parts:
                    col_name = parts[0]
                    pks.append(col_name)
        return pks

    def _extract_unique_columns(self, columns_block: str) -> List[str]:
        """Extract UNIQUE columns."""
        unique_cols = []
        unique_pattern = r"(\w+)\s+.*?UNIQUE"
        for match in re.finditer(unique_pattern, columns_block, re.IGNORECASE):
            unique_cols.append(match.group(1))
        return unique_cols

    def _extract_foreign_keys(self, columns_block: str) -> List[Dict[str, str]]:
        """Extract foreign key constraints."""
        fks = []
        # Pattern: fk_xxx INTEGER REFERENCES table(pk_xxx)
        fk_pattern = r"(fk_\w+)\s+\w+\s+REFERENCES\s+(\w+)\s*\(\s*(pk_\w+)\s*\)"
        for match in re.finditer(fk_pattern, columns_block, re.IGNORECASE):
            fks.append(
                {
                    "column": match.group(1),
                    "references_table": match.group(2),
                    "references_column": match.group(3),
                }
            )
        return fks

    def _parse_select_columns(self, select_clause: str) -> List[str]:
        """Parse SELECT column list."""
        cols = []

        # Split by comma, but handle nested functions
        # This is a simplified approach - split on commas not inside functions
        parts = select_clause.split(",")

        for part in parts:
            part = part.strip()

            # Skip empty parts or JSONB functions
            if not part or "jsonb_build_object" in part.lower():
                continue

            # Extract column name (handle aliases)
            # Examples: "p.id", "p.id AS post_id", "jsonb_build_object(...) AS data"
            if " AS " in part.upper():
                col_part = part.split(" AS ")[0].strip()
            elif " as " in part:
                col_part = part.split(" as ")[0].strip()
            else:
                col_part = part

            # Get the last identifier (handles table.column)
            col_name = col_part.split(".")[-1].strip()

            # Skip if it's a function call or complex expression
            if "(" in col_name or " " in col_name.strip():
                continue

            cols.append(col_name)

        return cols

    def _extract_jsonb_structure(self, select_clause: str) -> tuple[Optional[str], List[str]]:
        """Extract JSONB column name and fields inside jsonb_build_object()."""
        # Find the rightmost jsonb_build_object ... AS column pattern
        # This handles cases where there are nested jsonb_build_object calls
        jsonb_pattern = r"jsonb_build_object\s*\((.*?)\)\s+(?:AS|as)\s+(\w+)"
        matches = list(re.finditer(jsonb_pattern, select_clause, re.DOTALL | re.IGNORECASE))

        if not matches:
            return None, []

        # Take the last match (rightmost) which should be the outermost one
        match = matches[-1]
        jsonb_content = match.group(1)
        jsonb_col_name = match.group(2)

        # Extract field names from quoted strings
        fields = []
        # Find all single-quoted strings
        quoted_strings = re.findall(r"'([^']*)'", jsonb_content)

        for field_name in quoted_strings:
            # Skip special values and SQL keywords
            if (
                len(field_name) > 0
                and not field_name.startswith("__")
                and field_name not in ["SELECT", "FROM", "WHERE", "INSERT", "UPDATE", "DELETE"]
                and not field_name.isspace()
            ):
                fields.append(field_name)

        # Remove duplicates while preserving order
        seen = set()
        unique_fields = []
        for field in fields:
            if field not in seen:
                seen.add(field)
                unique_fields.append(field)

        return jsonb_col_name, unique_fields

    def _extract_joins(self, from_clause: str) -> List[Dict[str, str]]:
        """Extract JOIN clauses."""
        joins = []
        join_pattern = r"JOIN\s+(\w+)(?:\s+(\w+))?\s+ON\s+(.*?)(?:JOIN|WHERE|;|$)"
        for match in re.finditer(join_pattern, from_clause, re.IGNORECASE):
            joins.append(
                {
                    "table": match.group(1),
                    "alias": match.group(2) or match.group(1),
                    "condition": match.group(3).strip(),
                }
            )
        return joins

    def _parse_parameters(self, params_str: str) -> List[Dict[str, str]]:
        """Parse function parameters."""
        if not params_str.strip():
            return []

        params = []
        for param in params_str.split(","):
            param = param.strip()
            if param:
                parts = param.split(maxsplit=1)
                if len(parts) == 2:
                    params.append({"name": parts[0], "type": parts[1]})
        return params


# Example usage:
if __name__ == "__main__":
    import sys

    if len(sys.argv) > 1:
        # Test on provided file
        sql_file = Path(sys.argv[1])
        analyzer = SQLAnalyzer(sql_file)

        tables = analyzer.extract_tables()
        views = analyzer.extract_views()
        functions = analyzer.extract_functions()

        print(f"File: {sql_file}")
        print(f"Tables: {len(tables)}")
        print(f"Views: {len(views)}")
        print(f"Functions: {len(functions)}")

        for table in tables:
            print(f"\nTable: {table.name}")
            print(f"  Trinity pattern: {table.has_trinity_pattern()}")
            print(f"  Columns: {list(table.columns.keys())}")
            print(f"  Primary keys: {table.primary_keys}")
            print(f"  Foreign keys: {table.foreign_keys}")

        for view in views:
            print(f"\nView: {view.name}")
            print(f"  Has id column: {view.has_id_column()}")
            print(f"  Has pk column: {view.has_pk_column()}")
            print(f"  JSONB exposes pk: {view.jsonb_exposes_pk()}")
            print(f"  JSONB fields: {view.jsonb_fields}")

        for func in functions:
            print(f"\nFunction: {func.name}")
            print(f"  Schema: {func.schema}")
            print(f"  Return type: {func.return_type}")
            print(f"  Uses helpers: {func.uses_helper_functions()}")
            print(f"  Sync calls: {func.has_explicit_sync_calls()}")
