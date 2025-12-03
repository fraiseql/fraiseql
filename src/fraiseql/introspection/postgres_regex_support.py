"""PostgreSQL Regex Pattern Support for Database Introspection

This module extends PostgresIntrospector with regex pattern matching capabilities
for more powerful and flexible database object filtering.

Author: Purvansh Joshi (Open Source Contribution)
Date: 2025-12-03
Issue: fraiseql/fraiseql#149
"""

from typing import Optional, List, Pattern
import re


class RegexPatternMatcher:
    """Handles regex pattern matching for PostgreSQL object names.
    
    This class provides utilities to compile and validate regex patterns
    for use with PostgreSQL's regex operator (~).
    """
    
    def __init__(self, pattern: str, case_sensitive: bool = True):
        """Initialize regex pattern matcher.
        
        Args:
            pattern: The regex pattern string
            case_sensitive: Whether matching should be case-sensitive
            
        Raises:
            ValueError: If pattern is invalid
        """
        self.pattern = pattern
        self.case_sensitive = case_sensitive
        self._validate_pattern()
        self._compiled_pattern = self._compile_pattern()
    
    def _validate_pattern(self) -> None:
        """Validate regex pattern syntax.
        
        Raises:
            ValueError: If pattern has invalid syntax
        """
        try:
            flags = 0 if self.case_sensitive else re.IGNORECASE
            re.compile(self.pattern, flags)
        except re.error as e:
            raise ValueError(f"Invalid regex pattern: {str(e)}")
    
    def _compile_pattern(self) -> Pattern:
        """Compile the regex pattern with appropriate flags.
        
        Returns:
            Compiled regex pattern object
        """
        flags = 0 if self.case_sensitive else re.IGNORECASE
        return re.compile(self.pattern, flags)
    
    def matches(self, text: str) -> bool:
        """Check if text matches the pattern.
        
        Args:
            text: Text to match against pattern
            
        Returns:
            True if pattern matches, False otherwise
        """
        return bool(self._compiled_pattern.search(text))
    
    def to_postgres_regex(self) -> str:
        """Convert to PostgreSQL regex operator syntax.
        
        Returns:
            SQL snippet for PostgreSQL regex operator (~)
        """
        if self.case_sensitive:
            return f"~ '{self.pattern}'"
        else:
            return f"~* '{self.pattern}'"


class PostgresIntrospectorRegexExt:
    """Extension to PostgresIntrospector adding regex pattern support.
    
    This class provides methods to extend the existing PostgresIntrospector
    with regex pattern matching for discovering database objects.
    """
    
    @staticmethod
    def discover_views_by_regex(
        connection,
        pattern: str,
        schema: str = 'public',
        case_sensitive: bool = True
    ) -> List[dict]:
        """Discover views matching a regex pattern.
        
        Args:
            connection: Database connection object
            pattern: Regex pattern to match view names
            schema: Schema to search in (default: 'public')
            case_sensitive: Whether matching is case-sensitive
            
        Returns:
            List of matching view information dictionaries
            
        Raises:
            ValueError: If pattern is invalid
        """
        matcher = RegexPatternMatcher(pattern, case_sensitive)
        
        query = f"""
        SELECT 
            table_name as name,
            table_schema as schema
        FROM information_schema.views
        WHERE table_schema = %s
        AND table_name {matcher.to_postgres_regex()}
        ORDER BY table_name;
        """
        
        cursor = connection.cursor()
        cursor.execute(query, (schema, pattern))
        return cursor.fetchall()
    
    @staticmethod
    def discover_functions_by_regex(
        connection,
        pattern: str,
        schema: str = 'public',
        case_sensitive: bool = True
    ) -> List[dict]:
        """Discover functions matching a regex pattern.
        
        Args:
            connection: Database connection object
            pattern: Regex pattern to match function names
            schema: Schema to search in (default: 'public')
            case_sensitive: Whether matching is case-sensitive
            
        Returns:
            List of matching function information dictionaries
            
        Raises:
            ValueError: If pattern is invalid
        """
        matcher = RegexPatternMatcher(pattern, case_sensitive)
        
        query = f"""
        SELECT 
            routine_name as name,
            routine_schema as schema,
            routine_type
        FROM information_schema.routines
        WHERE routine_schema = %s
        AND routine_name {matcher.to_postgres_regex()}
        ORDER BY routine_name;
        """
        
        cursor = connection.cursor()
        cursor.execute(query, (schema, pattern))
        return cursor.fetchall()


# Integration point for PostgresIntrospector
def extend_postgres_introspector(introspector_class):
    """Decorator to extend PostgresIntrospector with regex support.
    
    Usage:
        @extend_postgres_introspector
        class PostgresIntrospector:
            # existing implementation
    """
    def wrapper(*args, **kwargs):
        # Add regex methods to introspector
        introspector_class.discover_views_by_regex = PostgresIntrospectorRegexExt.discover_views_by_regex
        introspector_class.discover_functions_by_regex = PostgresIntrospectorRegexExt.discover_functions_by_regex
        return introspector_class(*args, **kwargs)
    return wrapper
