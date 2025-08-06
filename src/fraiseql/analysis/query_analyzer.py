"""Advanced query analysis for execution mode selection."""

from dataclasses import dataclass
from enum import Enum
from typing import Dict, List, Optional

from graphql import (
    DocumentNode,
    FieldNode,
    GraphQLSchema,
    OperationDefinitionNode,
    SelectionSetNode,
    parse,
    validate,
)

from fraiseql.cache.view_metadata import ViewMetadataCache
from fraiseql.utils.casing import to_snake_case


class PassthroughEligibility(Enum):
    """Query eligibility for passthrough execution."""

    ELIGIBLE = "eligible"
    NOT_ELIGIBLE = "not_eligible"
    MAYBE = "maybe"  # Requires runtime checks


@dataclass
class PassthroughAnalysis:
    """Analysis result for passthrough eligibility."""

    eligible: bool
    eligibility: PassthroughEligibility
    reason: Optional[str] = None
    view_name: Optional[str] = None
    field_paths: Optional[Dict[str, str]] = None  # GraphQL -> JSONB path
    where_conditions: Optional[Dict[str, any]] = None
    complexity_score: int = 0
    has_custom_resolvers: bool = False
    has_computed_fields: bool = False
    max_depth: int = 0


class QueryAnalyzer:
    """Comprehensive query analyzer for execution mode selection."""

    def __init__(self, schema: GraphQLSchema):
        """Initialize analyzer with GraphQL schema.

        Args:
            schema: GraphQL schema for type information
        """
        self.schema = schema
        self.view_metadata = ViewMetadataCache()
        self._resolver_cache = {}
        self._init_resolver_analysis()

    def analyze_for_passthrough(
        self, query: str, variables: Optional[Dict[str, any]] = None
    ) -> PassthroughAnalysis:
        """Analyze query for JSON passthrough eligibility.

        Args:
            query: GraphQL query string
            variables: Query variables

        Returns:
            PassthroughAnalysis with eligibility details
        """
        try:
            # Parse query
            document = parse(query)

            # Validate against schema
            errors = validate(self.schema, document)
            if errors:
                return PassthroughAnalysis(
                    eligible=False,
                    eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                    reason="Query validation failed",
                )

            # Extract operation
            operation = self._get_operation(document)
            if not operation:
                return PassthroughAnalysis(
                    eligible=False,
                    eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                    reason="No operation found",
                )

            # Check operation type
            if operation.operation != "query":
                return PassthroughAnalysis(
                    eligible=False,
                    eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                    reason=f"Operation type '{operation.operation}' not supported",
                )

            # Analyze structure
            structure_check = self._check_structure_complexity(operation)
            if not structure_check.eligible:
                return structure_check

            # Check for custom resolvers
            resolver_check = self._check_custom_resolvers(operation)
            if not resolver_check.eligible:
                return resolver_check

            # Analyze field mappings
            field_analysis = self._analyze_field_mappings(operation)
            if not field_analysis.eligible:
                return field_analysis

            # Build final analysis
            return PassthroughAnalysis(
                eligible=True,
                eligibility=PassthroughEligibility.ELIGIBLE,
                view_name=field_analysis.view_name,
                field_paths=field_analysis.field_paths,
                where_conditions=self._extract_where_conditions(operation, variables),
                complexity_score=structure_check.complexity_score,
                max_depth=structure_check.max_depth,
            )

        except Exception as e:
            return PassthroughAnalysis(
                eligible=False,
                eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                reason=f"Analysis error: {e!s}",
            )

    def _init_resolver_analysis(self):
        """Initialize resolver analysis by examining schema."""
        # Analyze which fields have custom resolvers
        query_type = self.schema.type_map.get("Query")
        if not query_type:
            return

        for field_name, field in query_type.fields.items():
            if field.resolve:
                # Check if it's a simple resolver
                self._resolver_cache[field_name] = self._is_simple_resolver(field.resolve)

    def _is_simple_resolver(self, resolver) -> bool:
        """Check if a resolver is simple enough for passthrough."""
        # Get the original function
        original = resolver
        while hasattr(original, "__wrapped__"):
            original = original.__wrapped__

        # Check function characteristics
        # This is a heuristic - in production would need deeper analysis
        if hasattr(original, "__code__"):
            code = original.__code__
            # Simple resolvers typically have few instructions
            if code.co_code.__len__() > 100:  # Arbitrary threshold
                return False

            # Check for complex patterns in code names
            names = code.co_names
            complex_patterns = ["filter", "map", "reduce", "join", "aggregate"]
            if any(pattern in names for pattern in complex_patterns):
                return False

        return True

    def _get_operation(self, document: DocumentNode) -> Optional[OperationDefinitionNode]:
        """Extract operation from document."""
        for definition in document.definitions:
            if isinstance(definition, OperationDefinitionNode):
                return definition
        return None

    def _check_structure_complexity(
        self, operation: OperationDefinitionNode
    ) -> PassthroughAnalysis:
        """Check if query structure is simple enough."""
        # Check number of root fields
        selections = operation.selection_set.selections
        if len(selections) != 1:
            return PassthroughAnalysis(
                eligible=False,
                eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                reason="Multiple root fields not supported",
            )

        # Calculate complexity
        complexity = ComplexityCalculator()
        score = complexity.calculate(operation.selection_set)

        # Check depth
        depth = self._calculate_depth(operation.selection_set)

        if score > 100:  # Configurable threshold
            return PassthroughAnalysis(
                eligible=False,
                eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                reason=f"Query too complex (score: {score})",
                complexity_score=score,
                max_depth=depth,
            )

        if depth > 3:  # Configurable threshold
            return PassthroughAnalysis(
                eligible=False,
                eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                reason=f"Query too deep (depth: {depth})",
                complexity_score=score,
                max_depth=depth,
            )

        return PassthroughAnalysis(
            eligible=True,
            eligibility=PassthroughEligibility.ELIGIBLE,
            complexity_score=score,
            max_depth=depth,
        )

    def _check_custom_resolvers(self, operation: OperationDefinitionNode) -> PassthroughAnalysis:
        """Check if query uses custom resolvers."""
        # Get root field
        root_field = operation.selection_set.selections[0]
        if not isinstance(root_field, FieldNode):
            return PassthroughAnalysis(
                eligible=False,
                eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                reason="Invalid root selection",
            )

        field_name = root_field.name.value

        # Check resolver cache
        if field_name in self._resolver_cache and not self._resolver_cache[field_name]:
            return PassthroughAnalysis(
                eligible=False,
                eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                reason=f"Field '{field_name}' has complex resolver",
                has_custom_resolvers=True,
            )

        # Check nested fields for custom resolvers
        if root_field.selection_set:
            custom_fields = self._find_custom_resolver_fields(root_field.selection_set)
            if custom_fields:
                return PassthroughAnalysis(
                    eligible=False,
                    eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                    reason=f"Custom resolvers on fields: {', '.join(custom_fields)}",
                    has_custom_resolvers=True,
                )

        return PassthroughAnalysis(eligible=True, eligibility=PassthroughEligibility.ELIGIBLE)

    def _analyze_field_mappings(self, operation: OperationDefinitionNode) -> PassthroughAnalysis:
        """Analyze field mappings for JSONB extraction."""
        root_field = operation.selection_set.selections[0]
        field_name = root_field.name.value

        # Get return type
        query_type = self.schema.type_map.get("Query")
        if not query_type or field_name not in query_type.fields:
            return PassthroughAnalysis(
                eligible=False,
                eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                reason=f"Field '{field_name}' not found in schema",
            )

        field_def = query_type.fields[field_name]
        return_type = field_def.type

        # Unwrap list/non-null types
        while hasattr(return_type, "of_type"):
            return_type = return_type.of_type

        type_name = return_type.name

        # Map to view name
        view_name = self._get_view_name(type_name)
        if not view_name:
            return PassthroughAnalysis(
                eligible=False,
                eligibility=PassthroughEligibility.NOT_ELIGIBLE,
                reason=f"No view mapping for type '{type_name}'",
            )

        # Build field paths
        field_paths = {}
        if root_field.selection_set:
            field_paths = self._build_field_paths(root_field.selection_set)

        return PassthroughAnalysis(
            eligible=True,
            eligibility=PassthroughEligibility.ELIGIBLE,
            view_name=view_name,
            field_paths=field_paths,
        )

    def _get_view_name(self, type_name: str) -> Optional[str]:
        """Get database view name for GraphQL type."""
        # Convert to snake_case and add _view suffix
        snake_name = to_snake_case(type_name)
        return f"{snake_name}_view"

    def _build_field_paths(
        self, selection_set: SelectionSetNode, prefix: str = ""
    ) -> Dict[str, str]:
        """Build mapping of GraphQL fields to JSONB paths."""
        field_paths = {}

        for selection in selection_set.selections:
            if isinstance(selection, FieldNode):
                field_name = selection.name.value
                snake_field = to_snake_case(field_name)

                # Build JSONB path
                if prefix:
                    jsonb_path = f"{prefix}->{snake_field}"
                else:
                    jsonb_path = snake_field

                field_paths[field_name] = jsonb_path

                # Recurse for nested selections
                if selection.selection_set:
                    nested_paths = self._build_field_paths(selection.selection_set, jsonb_path)
                    field_paths.update(nested_paths)

        return field_paths

    def _extract_where_conditions(
        self, operation: OperationDefinitionNode, variables: Optional[Dict[str, any]]
    ) -> Dict[str, any]:
        """Extract WHERE conditions from query arguments."""
        conditions = {}

        root_field = operation.selection_set.selections[0]
        if not isinstance(root_field, FieldNode):
            return conditions

        # Process arguments
        for arg in root_field.arguments:
            arg_name = arg.name.value

            # Handle different argument value types
            if hasattr(arg.value, "name"):  # Variable
                var_name = arg.value.name.value
                if variables and var_name in variables:
                    conditions[arg_name] = variables[var_name]
            elif hasattr(arg.value, "value"):  # Literal
                conditions[arg_name] = arg.value.value

        return conditions

    def _find_custom_resolver_fields(self, selection_set: SelectionSetNode) -> List[str]:
        """Find fields with custom resolvers in selection set."""
        custom_fields = []

        for selection in selection_set.selections:
            if isinstance(selection, FieldNode):
                # Check if field has custom resolver
                # This would need to look up the field in the schema
                # and check its resolver
                pass

        return custom_fields

    def _calculate_depth(self, selection_set: SelectionSetNode, current_depth: int = 0) -> int:
        """Calculate maximum query depth."""
        if not selection_set:
            return current_depth

        max_depth = current_depth

        for selection in selection_set.selections:
            if isinstance(selection, FieldNode) and selection.selection_set:
                depth = self._calculate_depth(selection.selection_set, current_depth + 1)
                max_depth = max(max_depth, depth)

        return max_depth

    def get_field_mapping(self, type_name: str, field_name: str) -> Optional[str]:
        """Get database field mapping for GraphQL field.

        Args:
            type_name: GraphQL type name
            field_name: GraphQL field name

        Returns:
            Database column/JSONB path if found
        """
        # Check metadata cache
        view_name = self._get_view_name(type_name)
        if not view_name:
            return None

        # Get from view metadata
        return self.view_metadata.get_jsonb_paths(view_name, field_name)


class ComplexityCalculator:
    """Calculates query complexity score."""

    def calculate(self, selection_set: SelectionSetNode) -> int:
        """Calculate complexity score for selection set."""
        score = 0

        for selection in selection_set.selections:
            if isinstance(selection, FieldNode):
                # Base score for field
                score += 1

                # Add score for nested selections
                if selection.selection_set:
                    score += self.calculate(selection.selection_set) * 2

                # Add score for arguments
                score += len(selection.arguments) * 2

        return score
