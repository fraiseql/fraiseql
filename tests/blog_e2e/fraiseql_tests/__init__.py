"""Enhanced FraiseQL Testing Module

This module contains the GREEN phase implementation of the enhanced FraiseQL
pattern that eliminates MutationResultBase inheritance while adding
comprehensive error array support.
"""

from .enhanced_mutation import (
    FraiseQLError,
    FraiseQLMutation, 
    MutationResultBase,  # Backward compatibility
    map_database_result_to_graphql,
    create_validation_summary_from_errors,
    CreateAuthorInput,
    Author,
    CreateAuthorSuccess,
    CreateAuthorError,
    CreateAuthorEnhanced,
    create_sample_success_result,
    create_sample_error_result,
    demonstrate_clean_pattern
)

__all__ = [
    # Core enhanced types
    "FraiseQLError",
    "FraiseQLMutation",
    
    # Backward compatibility
    "MutationResultBase",
    
    # Mapping functions
    "map_database_result_to_graphql", 
    "create_validation_summary_from_errors",
    
    # Sample types
    "CreateAuthorInput",
    "Author", 
    "CreateAuthorSuccess",
    "CreateAuthorError",
    "CreateAuthorEnhanced",
    
    # Utility functions
    "create_sample_success_result",
    "create_sample_error_result",
    "demonstrate_clean_pattern"
]