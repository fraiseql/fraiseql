# Phase 3: Automated Verification - Build and Run Verification Script

## Objective

Build automated verification tooling that checks all examples against extracted pattern rules from Phase 2.

## Context

We have:
- Complete inventory of examples (Phase 1)
- Formalized verification rules (Phase 2)
- Golden patterns from blog_api

Now build automated verification that can:
1. Parse SQL files (tables, views, functions)
2. Analyze JSONB structures
3. Check Python type definitions
4. Match against rules from rules.yaml
5. Generate compliance reports

## Files to Modify/Create

### Create
- `.phases/verify-examples-compliance/verify.py` - Main verification script
- `.phases/verify-examples-compliance/sql_analyzer.py` - SQL parsing/analysis
- `.phases/verify-examples-compliance/jsonb_analyzer.py` - JSONB structure analysis
- `.phases/verify-examples-compliance/python_analyzer.py` - Python type checking
- `.phases/verify-examples-compliance/report_generator.py` - Generate compliance reports
- `.phases/verify-examples-compliance/test_verify.py` - Test verification logic

### Read-Only
- `examples/**/*.sql` - All SQL files to verify
- `examples/**/*.py` - All Python files to verify
- `.phases/verify-examples-compliance/rules.yaml` - Verification rules
- `.phases/verify-examples-compliance/inventory.json` - Example inventory

## Implementation Steps

### Step 1: Build SQL Parser/Analyzer

Create `sql_analyzer.py` to parse SQL and extract structure:

```python
"""SQL parsing and analysis for Trinity pattern verification."""
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass
class TableDefinition:
    """Parsed table structure."""
    name: str
    columns: dict[str, dict[str, Any]]  # column_name -> {type, constraints}
    primary_keys: list[str]
    unique_columns: list[str]
    foreign_keys: list[dict[str, str]]  # {column, references_table, references_column}

    def has_trinity_pattern(self) -> dict[str, bool]:
        """Check if table follows Trinity pattern."""
        return {
            'has_pk': any(col.startswith('pk_') for col in self.primary_keys),
            'has_id_uuid': 'id' in self.columns and 'UUID' in self.columns['id']['type'],
            'has_identifier': 'identifier' in self.columns,
            'pk_is_integer': any(
                'INTEGER' in self.columns[pk]['type']
                for pk in self.primary_keys
                if pk.startswith('pk_')
            ),
        }


@dataclass
class ViewDefinition:
    """Parsed view structure."""
    name: str
    select_columns: list[str]  # Direct SELECT columns (not in JSONB)
    jsonb_column: str | None  # Name of JSONB column (usually 'data')
    jsonb_fields: list[str]  # Fields inside jsonb_build_object()
    joins: list[dict[str, str]]  # {table, on_condition}

    def has_id_column(self) -> bool:
        """Check if view has direct 'id' column (not just in JSONB)."""
        return 'id' in self.select_columns

    def has_pk_column(self) -> bool:
        """Check if view includes pk_* column."""
        return any(col.startswith('pk_') for col in self.select_columns)

    def jsonb_exposes_pk(self) -> bool:
        """Check if JSONB contains pk_* field (security violation!)."""
        return any(field.startswith('pk_') for field in self.jsonb_fields)


@dataclass
class FunctionDefinition:
    """Parsed function structure."""
    name: str
    schema: str | None
    parameters: list[dict[str, str]]  # {name, type}
    return_type: str
    language: str
    body: str

    def uses_helper_functions(self) -> bool:
        """Check if function uses core.get_pk_* helper functions."""
        return bool(re.search(r'core\.get_pk_\w+\(', self.body))

    def has_explicit_sync_calls(self) -> list[str]:
        """Find all fn_sync_tv_* calls in function body."""
        return re.findall(r'fn_sync_tv_(\w+)\(\)', self.body)


class SQLAnalyzer:
    """Analyze SQL files for pattern compliance."""

    def __init__(self, sql_file: Path):
        self.sql_file = sql_file
        self.content = sql_file.read_text()

    def extract_tables(self) -> list[TableDefinition]:
        """Extract all CREATE TABLE statements."""
        tables = []
        # Regex to match CREATE TABLE ... ); blocks
        pattern = r'CREATE\s+TABLE\s+(\w+)\s*\((.*?)\);'
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

            tables.append(TableDefinition(
                name=table_name,
                columns=columns,
                primary_keys=primary_keys,
                unique_columns=unique_cols,
                foreign_keys=foreign_keys
            ))

        return tables

    def extract_views(self) -> list[ViewDefinition]:
        """Extract all CREATE VIEW statements."""
        views = []
        # Pattern for CREATE VIEW ... AS SELECT ...
        pattern = r'CREATE\s+(?:OR\s+REPLACE\s+)?VIEW\s+(\w+)\s+AS\s+SELECT\s+(.*?)\s+FROM'
        matches = re.finditer(pattern, self.content, re.DOTALL | re.IGNORECASE)

        for match in matches:
            view_name = match.group(1)
            select_clause = match.group(2)

            # Extract direct SELECT columns
            select_columns = self._parse_select_columns(select_clause)

            # Find JSONB column and extract fields
            jsonb_col, jsonb_fields = self._extract_jsonb_structure(select_clause)

            # Extract JOINs
            joins = self._extract_joins(self.content[match.end():])

            views.append(ViewDefinition(
                name=view_name,
                select_columns=select_columns,
                jsonb_column=jsonb_col,
                jsonb_fields=jsonb_fields,
                joins=joins
            ))

        return views

    def extract_functions(self) -> list[FunctionDefinition]:
        """Extract all CREATE FUNCTION statements."""
        functions = []
        # Pattern for CREATE FUNCTION ... RETURNS ... AS $$ ... $$
        pattern = r'CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+((?:\w+\.)?\w+)\s*\((.*?)\)\s+RETURNS\s+(\w+)(?:\[\])?\s+(?:AS|LANGUAGE)\s+\$\$(.*?)\$\$\s*LANGUAGE\s+(\w+)'

        matches = re.finditer(pattern, self.content, re.DOTALL | re.IGNORECASE)

        for match in matches:
            full_name = match.group(1)
            schema = None
            name = full_name
            if '.' in full_name:
                schema, name = full_name.split('.', 1)

            params_str = match.group(2)
            return_type = match.group(3)
            body = match.group(4)
            language = match.group(5)

            # Parse parameters
            parameters = self._parse_parameters(params_str)

            functions.append(FunctionDefinition(
                name=name,
                schema=schema,
                parameters=parameters,
                return_type=return_type,
                language=language,
                body=body
            ))

        return functions

    def _parse_columns(self, columns_block: str) -> dict[str, dict[str, Any]]:
        """Parse column definitions."""
        columns = {}
        # Simple column parsing (can be enhanced)
        lines = columns_block.split(',')
        for line in lines:
            line = line.strip()
            if not line or line.upper().startswith(('PRIMARY KEY', 'FOREIGN KEY', 'CONSTRAINT', 'UNIQUE')):
                continue

            # Extract column name and type
            parts = line.split(maxsplit=2)
            if len(parts) >= 2:
                col_name = parts[0]
                col_type = parts[1]
                constraints = line[len(col_name) + len(col_type):].strip()

                columns[col_name] = {
                    'type': col_type,
                    'constraints': constraints
                }

        return columns

    def _extract_primary_keys(self, columns_block: str) -> list[str]:
        """Extract primary key columns."""
        pks = []
        # Look for PRIMARY KEY constraint
        pk_pattern = r'(\w+)\s+\w+.*?PRIMARY\s+KEY'
        for match in re.finditer(pk_pattern, columns_block, re.IGNORECASE):
            pks.append(match.group(1))
        return pks

    def _extract_unique_columns(self, columns_block: str) -> list[str]:
        """Extract UNIQUE columns."""
        unique_cols = []
        unique_pattern = r'(\w+)\s+\w+.*?UNIQUE'
        for match in re.finditer(unique_pattern, columns_block, re.IGNORECASE):
            unique_cols.append(match.group(1))
        return unique_cols

    def _extract_foreign_keys(self, columns_block: str) -> list[dict[str, str]]:
        """Extract foreign key constraints."""
        fks = []
        # Pattern: fk_xxx INTEGER REFERENCES table(pk_xxx)
        fk_pattern = r'(fk_\w+)\s+\w+\s+REFERENCES\s+(\w+)\s*\(\s*(pk_\w+)\s*\)'
        for match in re.finditer(fk_pattern, columns_block, re.IGNORECASE):
            fks.append({
                'column': match.group(1),
                'references_table': match.group(2),
                'references_column': match.group(3)
            })
        return fks

    def _parse_select_columns(self, select_clause: str) -> list[str]:
        """Parse SELECT column list."""
        # Simple extraction of top-level columns
        cols = []
        # Split by comma (ignoring function calls with commas)
        depth = 0
        current_col = []

        for char in select_clause:
            if char == '(':
                depth += 1
            elif char == ')':
                depth -= 1
            elif char == ',' and depth == 0:
                col_name = ''.join(current_col).strip().split()[-1]  # Get last word
                cols.append(col_name)
                current_col = []
                continue

            current_col.append(char)

        # Add last column
        if current_col:
            col_name = ''.join(current_col).strip().split()[-1]
            cols.append(col_name)

        return cols

    def _extract_jsonb_structure(self, select_clause: str) -> tuple[str | None, list[str]]:
        """Extract JSONB column name and fields inside jsonb_build_object()."""
        # Find jsonb_build_object(...) AS column_name
        jsonb_pattern = r'jsonb_build_object\((.*?)\)\s+(?:AS|as)\s+(\w+)'
        match = re.search(jsonb_pattern, select_clause, re.DOTALL)

        if not match:
            return None, []

        jsonb_content = match.group(1)
        jsonb_col_name = match.group(2)

        # Extract field names (every odd element in comma-separated list)
        # Example: 'id', id, 'name', name → ['id', 'name']
        fields = []
        parts = re.split(r',\s*', jsonb_content)
        for i in range(0, len(parts), 2):
            field_name = parts[i].strip().strip("'\"")
            fields.append(field_name)

        return jsonb_col_name, fields

    def _extract_joins(self, from_clause: str) -> list[dict[str, str]]:
        """Extract JOIN clauses."""
        joins = []
        join_pattern = r'JOIN\s+(\w+)(?:\s+(\w+))?\s+ON\s+(.*?)(?:JOIN|WHERE|;|$)'
        for match in re.finditer(join_pattern, from_clause, re.IGNORECASE):
            joins.append({
                'table': match.group(1),
                'alias': match.group(2) or match.group(1),
                'condition': match.group(3).strip()
            })
        return joins

    def _parse_parameters(self, params_str: str) -> list[dict[str, str]]:
        """Parse function parameters."""
        if not params_str.strip():
            return []

        params = []
        for param in params_str.split(','):
            param = param.strip()
            if param:
                parts = param.split(maxsplit=1)
                if len(parts) == 2:
                    params.append({
                        'name': parts[0],
                        'type': parts[1]
                    })
        return params


# Example usage:
if __name__ == '__main__':
    # Test on blog_api example
    sql_file = Path('examples/blog_api/db/0_schema/01_write/011_tb_user.sql')
    analyzer = SQLAnalyzer(sql_file)

    tables = analyzer.extract_tables()
    for table in tables:
        print(f"Table: {table.name}")
        print(f"  Trinity pattern: {table.has_trinity_pattern()}")
        print(f"  Columns: {list(table.columns.keys())}")
        print(f"  Primary keys: {table.primary_keys}")
        print(f"  Foreign keys: {table.foreign_keys}")
```

### Step 2: Build Verification Engine

Create `verify.py` main script:

```python
"""Main verification script for FraiseQL examples compliance."""
import json
import yaml
from pathlib import Path
from dataclasses import dataclass, asdict
from typing import Any

from sql_analyzer import SQLAnalyzer, TableDefinition, ViewDefinition, FunctionDefinition


@dataclass
class ViolationReport:
    """Pattern violation report."""
    rule_id: str
    rule_name: str
    severity: str  # ERROR, WARNING, INFO
    file_path: str
    line_number: int | None
    violation_type: str  # table, view, function, python_type
    entity_name: str
    description: str
    example_fix: str | None = None


@dataclass
class ComplianceReport:
    """Overall compliance report for an example."""
    example_name: str
    total_files: int
    files_checked: int
    violations: list[ViolationReport]
    compliance_score: float  # 0.0 to 1.0

    @property
    def errors(self) -> list[ViolationReport]:
        return [v for v in self.violations if v.severity == 'ERROR']

    @property
    def warnings(self) -> list[ViolationReport]:
        return [v for v in self.violations if v.severity == 'WARNING']

    @property
    def infos(self) -> list[ViolationReport]:
        return [v for v in self.violations if v.severity == 'INFO']


class PatternVerifier:
    """Verify examples against Trinity pattern rules."""

    def __init__(self, rules_path: Path):
        with open(rules_path) as f:
            self.rules = yaml.safe_load(f)

    def verify_table(self, table: TableDefinition, file_path: Path) -> list[ViolationReport]:
        """Verify table against Trinity pattern rules."""
        violations = []
        trinity = table.has_trinity_pattern()

        # Rule TR-001: Must have INTEGER pk_*
        if not trinity['has_pk'] or not trinity['pk_is_integer']:
            violations.append(ViolationReport(
                rule_id='TR-001',
                rule_name='Trinity: INTEGER Primary Key',
                severity='ERROR',
                file_path=str(file_path),
                line_number=None,
                violation_type='table',
                entity_name=table.name,
                description=f"Table {table.name} missing INTEGER pk_* primary key",
                example_fix="pk_user INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY"
            ))

        # Rule TR-002: Must have UUID id
        if not trinity['has_id_uuid']:
            violations.append(ViolationReport(
                rule_id='TR-002',
                rule_name='Trinity: UUID Public Identifier',
                severity='ERROR',
                file_path=str(file_path),
                line_number=None,
                violation_type='table',
                entity_name=table.name,
                description=f"Table {table.name} missing 'id UUID DEFAULT gen_random_uuid() UNIQUE'",
                example_fix="id UUID DEFAULT gen_random_uuid() NOT NULL UNIQUE"
            ))

        # Rule TR-003: May have identifier (info only)
        if not trinity['has_identifier']:
            violations.append(ViolationReport(
                rule_id='TR-003',
                rule_name='Trinity: TEXT Identifier',
                severity='INFO',
                file_path=str(file_path),
                line_number=None,
                violation_type='table',
                entity_name=table.name,
                description=f"Table {table.name} could benefit from 'identifier TEXT UNIQUE' for SEO-friendly slugs",
                example_fix="identifier TEXT UNIQUE  -- Human-readable slug"
            ))

        # Rule FK-001/FK-002: Foreign keys must reference pk_* (INTEGER)
        for fk in table.foreign_keys:
            if not fk['references_column'].startswith('pk_'):
                violations.append(ViolationReport(
                    rule_id='FK-001',
                    rule_name='FK: Must Reference INTEGER pk_*',
                    severity='ERROR',
                    file_path=str(file_path),
                    line_number=None,
                    violation_type='table',
                    entity_name=table.name,
                    description=f"Foreign key {fk['column']} references {fk['references_column']} (should reference pk_*)",
                    example_fix=f"{fk['column']} INTEGER REFERENCES {fk['references_table']}(pk_{fk['references_table'][3:]})"
                ))

        return violations

    def verify_view(self, view: ViewDefinition, file_path: Path) -> list[ViolationReport]:
        """Verify view against JSONB pattern rules."""
        violations = []

        # Rule VW-001: Must have direct 'id' column
        if not view.has_id_column():
            violations.append(ViolationReport(
                rule_id='VW-001',
                rule_name='View: Must Expose id Column',
                severity='ERROR',
                file_path=str(file_path),
                line_number=None,
                violation_type='view',
                entity_name=view.name,
                description=f"View {view.name} missing direct 'id' column (needed for WHERE filtering)",
                example_fix="SELECT id, jsonb_build_object(...) as data FROM ..."
            ))

        # Rule VW-003: JSONB must NOT contain pk_*
        if view.jsonb_exposes_pk():
            pk_fields = [f for f in view.jsonb_fields if f.startswith('pk_')]
            violations.append(ViolationReport(
                rule_id='VW-003',
                rule_name='JSONB: Never Expose pk_* Fields',
                severity='ERROR',
                file_path=str(file_path),
                line_number=None,
                violation_type='view',
                entity_name=view.name,
                description=f"View {view.name} exposes pk_* in JSONB: {pk_fields} (security violation!)",
                example_fix="Remove pk_* from jsonb_build_object() - keep it only as direct column if needed for JOINs"
            ))

        # Rule VW-002: Include pk_* only if referenced (warning only)
        if view.has_pk_column():
            violations.append(ViolationReport(
                rule_id='VW-002',
                rule_name='View: Include pk_* Only If Referenced',
                severity='WARNING',
                file_path=str(file_path),
                line_number=None,
                violation_type='view',
                entity_name=view.name,
                description=f"View {view.name} includes pk_* column - verify other views JOIN to it",
                example_fix="Only include pk_* if other views use it in JOIN conditions"
            ))

        return violations

    def verify_function(self, func: FunctionDefinition, file_path: Path) -> list[ViolationReport]:
        """Verify function against mutation pattern rules."""
        violations = []

        # Rule MF-001: Mutation functions must return JSONB
        if func.name.startswith('fn_') and func.return_type != 'JSONB':
            violations.append(ViolationReport(
                rule_id='MF-001',
                rule_name='Mutation: Return JSONB Structure',
                severity='ERROR',
                file_path=str(file_path),
                line_number=None,
                violation_type='function',
                entity_name=func.name,
                description=f"Mutation function {func.name} should return JSONB, got {func.return_type}",
                example_fix="RETURNS JSONB AS $$ ... RETURN jsonb_build_object('success', true, ...); $$"
            ))

        # Rule HF-002: Check variable naming conventions
        if 'DECLARE' in func.body:
            # Simple check for variable naming (can be enhanced)
            bad_vars = re.findall(r'(\w+Id|\w+_ID)\s+', func.body)  # camelCase or _ID suffix
            if bad_vars:
                violations.append(ViolationReport(
                    rule_id='HF-002',
                    rule_name='Variables: Follow Naming Convention',
                    severity='WARNING',
                    file_path=str(file_path),
                    line_number=None,
                    violation_type='function',
                    entity_name=func.name,
                    description=f"Function {func.name} has non-standard variable names: {set(bad_vars)}",
                    example_fix="Use v_<entity>_id (UUID), v_<entity>_pk (INTEGER), p_<entity>_id (parameter)"
                ))

        return violations

    def verify_sql_file(self, sql_file: Path) -> list[ViolationReport]:
        """Verify a single SQL file."""
        analyzer = SQLAnalyzer(sql_file)
        violations = []

        # Check tables
        for table in analyzer.extract_tables():
            violations.extend(self.verify_table(table, sql_file))

        # Check views
        for view in analyzer.extract_views():
            violations.extend(self.verify_view(view, sql_file))

        # Check functions
        for func in analyzer.extract_functions():
            violations.extend(self.verify_function(func, sql_file))

        return violations

    def verify_example(self, example_dir: Path) -> ComplianceReport:
        """Verify entire example directory."""
        sql_files = list(example_dir.rglob('*.sql'))
        all_violations = []

        for sql_file in sql_files:
            violations = self.verify_sql_file(sql_file)
            all_violations.extend(violations)

        # Calculate compliance score
        total_checks = len(sql_files) * 10  # Rough estimate
        error_penalty = len([v for v in all_violations if v.severity == 'ERROR']) * 5
        warning_penalty = len([v for v in all_violations if v.severity == 'WARNING']) * 1

        score = max(0.0, 1.0 - (error_penalty + warning_penalty) / max(total_checks, 1))

        return ComplianceReport(
            example_name=example_dir.name,
            total_files=len(sql_files),
            files_checked=len(sql_files),
            violations=all_violations,
            compliance_score=score
        )


# Example usage
if __name__ == '__main__':
    verifier = PatternVerifier(Path('.phases/verify-examples-compliance/rules.yaml'))

    # Verify blog_api example
    blog_api = Path('examples/blog_api')
    report = verifier.verify_example(blog_api)

    print(f"\\nCompliance Report: {report.example_name}")
    print(f"Compliance Score: {report.compliance_score:.1%}")
    print(f"\\nErrors: {len(report.errors)}")
    print(f"Warnings: {len(report.warnings)}")
    print(f"Info: {len(report.infos)}")

    if report.errors:
        print("\\n=== ERRORS ===")
        for error in report.errors[:5]:  # Show first 5
            print(f"  [{error.rule_id}] {error.entity_name}: {error.description}")
```

### Step 3: Generate Compliance Reports

Create `report_generator.py`:

```python
"""Generate human-readable compliance reports."""
from pathlib import Path
from verify import ComplianceReport, ViolationReport


class ReportGenerator:
    """Generate compliance reports in various formats."""

    @staticmethod
    def generate_markdown(reports: list[ComplianceReport], output_path: Path):
        """Generate markdown compliance report."""
        with open(output_path, 'w') as f:
            f.write("# FraiseQL Examples Compliance Report\\n\\n")
            f.write(f"Generated: {datetime.now().isoformat()}\\n\\n")

            # Summary
            total_examples = len(reports)
            fully_compliant = len([r for r in reports if len(r.errors) == 0])
            avg_score = sum(r.compliance_score for r in reports) / len(reports)

            f.write("## Summary\\n\\n")
            f.write(f"- **Total Examples**: {total_examples}\\n")
            f.write(f"- **Fully Compliant**: {fully_compliant} ({fully_compliant/total_examples:.1%})\\n")
            f.write(f"- **Average Score**: {avg_score:.1%}\\n\\n")

            # Per-example reports
            f.write("## Example Reports\\n\\n")
            for report in sorted(reports, key=lambda r: r.compliance_score, reverse=True):
                f.write(f"### {report.example_name}\\n\\n")
                f.write(f"**Score**: {report.compliance_score:.1%} | ")
                f.write(f"**Errors**: {len(report.errors)} | ")
                f.write(f"**Warnings**: {len(report.warnings)}\\n\\n")

                if report.errors:
                    f.write("**Critical Issues:**\\n")
                    for error in report.errors:
                        f.write(f"- `{error.entity_name}` - {error.description}\\n")
                    f.write("\\n")
```

## Verification Commands

### Run Verification on Single Example
```bash
cd /home/lionel/code/fraiseql
python .phases/verify-examples-compliance/verify.py examples/blog_api/
```

### Run Verification on All Examples
```bash
for dir in examples/*/; do
  python .phases/verify-examples-compliance/verify.py "$dir"
done > verification-report.txt
```

### Test SQL Parser
```bash
python .phases/verify-examples-compliance/sql_analyzer.py \
  examples/blog_api/db/0_schema/01_write/011_tb_user.sql
```

## Expected Output

### Console Output
```
Verifying examples/blog_api/...
  ✅ tb_user (3 rules passed)
  ✅ v_user (3 rules passed)
  ⚠️  v_post (1 warning: pk_post included)
  ❌ fn_update_user (1 error: missing RETURNS JSONB)

Compliance Score: 85.0%
Errors: 1 | Warnings: 1 | Info: 0
```

### compliance-report.md
Detailed markdown report with:
- Summary statistics
- Per-example compliance scores
- Violation details with fixes
- Trend analysis

## Acceptance Criteria

- [ ] SQL analyzer parses all examples without errors
- [ ] Verification script runs on all examples (30+)
- [ ] Blog API example scores 100% (golden reference)
- [ ] Reports generated in markdown and JSON
- [ ] All rule violations include fix suggestions
- [ ] Ready for Phase 4 (Manual Review)

## DO NOT

- ❌ Do NOT auto-fix violations (document only)
- ❌ Do NOT fail on warnings (errors only block)
- ❌ Do NOT skip difficult-to-parse files (log and continue)
- ❌ Do NOT assume patterns (verify from actual code)
