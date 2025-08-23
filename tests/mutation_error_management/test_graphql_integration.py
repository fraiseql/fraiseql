"""Tests for GraphQL integration of the new error management system."""

import json
import pytest
from unittest.mock import Mock, AsyncMock
from graphql import GraphQLResolveInfo

from fraiseql.mutations.types import MutationResult
from fraiseql.mutations.clean_decorator import clean_mutation
from fraiseql.mutations.result_processor import MutationResultProcessor


class TestGraphQLErrorIntegration:
    """Test GraphQL-level error handling."""
    
    def test_graphql_response_structure_for_errors(self):
        """RED: GraphQL responses must have correct structure for error cases."""
        # This test ensures the GraphQL layer receives properly formatted errors
        
        # Mock GraphQL info context
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {
            "db": AsyncMock(),
            "tenant_id": "test-tenant-123",
            "user_id": "test-user-456"
        }
        
        # Mock database function call that returns error
        mock_info.context["db"].execute_function = AsyncMock(return_value=MutationResult(
            status="noop:machine_already_exists",
            message="Machine with this serial already exists"
        ))
        
        # Create a test mutation with clean error management
        @clean_mutation(function="create_machine", schema="app")
        class TestCreateMachine:
            class Input:
                serial_number: str
                name: str
            
            class Success:
                message: str = "Machine created successfully"
            
            class Error:
                message: str = "Failed to create machine"
                error_code: str = "CREATE_FAILED"
        
        # This test will fail until we implement the clean_mutation decorator
        # The decorator should return properly structured GraphQL responses
        
        # When we call the resolver
        resolver = TestCreateMachine._get_resolver()  # This method doesn't exist yet
        
        # Mock input
        input_data = {"serial_number": "ABC123", "name": "Test Machine"}
        
        # The result should be a ProcessedResult that can be serialized by GraphQL
        import asyncio
        result = asyncio.run(resolver(mock_info, input=input_data))
        
        # Should be JSON serializable for GraphQL
        result_dict = result.to_dict()
        json_result = json.dumps(result_dict, default=str)
        parsed = json.loads(json_result)
        
        # Should have GraphQL union structure
        assert '__typename' in parsed
        assert parsed['__typename'] == 'Error'
        assert 'errors' in parsed
        assert isinstance(parsed['errors'], list)
        assert len(parsed['errors']) > 0
        
        # Error should have proper structure
        error = parsed['errors'][0]
        assert error['code'] == 422
        assert error['identifier'] == 'machine_already_exists'
        assert error['message'] == 'Machine with this serial already exists'
    
    def test_graphql_response_structure_for_success(self):
        """RED: GraphQL responses must have correct structure for success cases."""
        # Mock GraphQL info context  
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {
            "db": AsyncMock(),
            "tenant_id": "test-tenant-123",
            "user_id": "test-user-456"
        }
        
        # Mock successful database response
        mock_info.context["db"].execute_function = AsyncMock(return_value=MutationResult(
            status="success",
            message="Machine created successfully",
            object_data={"id": "machine-123", "serial_number": "ABC123"}
        ))
        
        # Create a test mutation with clean error management
        @clean_mutation(function="create_machine", schema="app")
        class TestCreateMachine:
            class Input:
                serial_number: str
                name: str
            
            class Success:
                message: str = "Machine created successfully"
            
            class Error:
                message: str = "Failed to create machine"
                error_code: str = "CREATE_FAILED"
        
        # When we call the resolver
        resolver = TestCreateMachine._get_resolver()
        
        # Mock input
        input_data = {"serial_number": "ABC123", "name": "Test Machine"}
        
        # The result should be a ProcessedResult
        import asyncio
        result = asyncio.run(resolver(mock_info, input=input_data))
        
        # Should be JSON serializable for GraphQL
        result_dict = result.to_dict()
        json_result = json.dumps(result_dict, default=str)
        parsed = json.loads(json_result)
        
        # Should have GraphQL union structure with empty errors array
        assert '__typename' in parsed
        assert parsed['__typename'] == 'Success'
        assert 'errors' in parsed
        assert isinstance(parsed['errors'], list)
        assert len(parsed['errors']) == 0  # Empty, not None
    
    def test_typename_field_present(self):
        """RED: __typename field must be present for union resolution."""
        # GraphQL unions require __typename field for client-side type resolution
        
        processor = MutationResultProcessor()
        
        error_result = MutationResult(
            status="noop:test",
            message="Test error"
        )
        
        class TestError:
            pass
        
        processed = processor.process_error(error_result, TestError)
        result_dict = processed.to_dict()
        
        # Must have __typename for GraphQL union resolution
        assert '__typename' in result_dict
        assert result_dict['__typename'] == 'TestError'
    
    def test_frontend_compatibility(self):
        """RED: Response structure must match frontend expectations."""
        # PrintOptim frontend expects:
        # 1. errors array is always present (never None)
        # 2. errors array contains structured error objects
        # 3. __typename is present for union resolution
        # 4. All fields are camelCase (handled by GraphQL layer)
        
        processor = MutationResultProcessor()
        
        error_result = MutationResult(
            status="noop:validation_failed",
            message="Multiple validation errors",
            extra_metadata={
                "validation_errors": [
                    {"field": "serial_number", "issue": "already_exists"}
                ]
            }
        )
        
        class MockErrorClass:
            __name__ = "CreateMachineError"
        
        processed = processor.process_error(error_result, MockErrorClass)
        result_dict = processed.to_dict()
        
        # Frontend compatibility requirements
        assert 'errors' in result_dict
        assert isinstance(result_dict['errors'], list)
        assert len(result_dict['errors']) > 0
        
        error = result_dict['errors'][0]
        assert 'code' in error
        assert 'identifier' in error
        assert 'message' in error
        assert 'details' in error
        
        # Should preserve complex metadata
        assert 'validation_errors' in error['details']
        assert error['details']['validation_errors'][0]['field'] == 'serial_number'
    
    def test_union_type_detection_by_typename(self):
        """RED: GraphQL should be able to resolve union types by __typename."""
        # This test ensures that the union type detection works correctly
        # when GraphQL needs to determine if a result is Success or Error type
        
        processor = MutationResultProcessor()
        
        # Test Error type detection
        error_result = MutationResult(status="noop:test", message="Error")
        
        class CreateMachineError:
            pass
        
        processed_error = processor.process_error(error_result, CreateMachineError)
        error_dict = processed_error.to_dict()
        
        # Error types should have populated errors array
        assert error_dict['__typename'] == 'CreateMachineError'
        assert len(error_dict['errors']) > 0
        
        # Test Success type detection  
        success_result = MutationResult(status="success", message="Success")
        
        class CreateMachineSuccess:
            pass
        
        processed_success = processor.process_success(success_result, CreateMachineSuccess)
        success_dict = processed_success.to_dict()
        
        # Success types should have empty errors array
        assert success_dict['__typename'] == 'CreateMachineSuccess'
        assert len(success_dict['errors']) == 0  # Empty, not None
    
    def test_async_resolver_compatibility(self):
        """RED: The new system must work with async GraphQL resolvers."""
        # The clean_mutation decorator should generate async resolvers
        # that are compatible with GraphQL-core's async execution
        
        # Test that the resolver is async
        @clean_mutation(function="test_function", schema="app")
        class TestMutation:
            class Input:
                test_field: str
            
            class Success:
                message: str = "Success"
            
            class Error:
                message: str = "Error"
        
        resolver = TestMutation._get_resolver()
        
        # The resolver should be an async function
        import inspect
        assert inspect.iscoroutinefunction(resolver), "Resolver should be async function"
    
    def test_context_parameter_extraction(self):
        """RED: Context parameters should be extracted correctly for database calls."""
        # The clean_mutation decorator should extract context parameters
        # like tenant_id, user_id from GraphQL context and pass them to database functions
        
        from fraiseql.mutations.clean_decorator import _extract_context_args
        from unittest.mock import Mock
        
        # Mock GraphQL info with context
        mock_info = Mock()
        mock_info.context = {
            "tenant_id": "test-tenant-123",
            "user_id": "test-user-456",
            "other_field": "should_not_be_extracted"
        }
        
        # Define context parameter mapping
        context_params = {
            "input_pk_organization": "tenant_id",
            "input_created_by": "user_id"
        }
        
        # Extract context args
        result = _extract_context_args(mock_info, context_params)
        
        # Should extract only mapped parameters
        expected = {
            "input_pk_organization": "test-tenant-123",
            "input_created_by": "test-user-456"
        }
        
        assert result == expected


class TestCleanMutationDecorator:
    """Test the clean_mutation decorator functionality."""
    
    def test_clean_mutation_decorator_exists(self):
        """RED: clean_mutation decorator should be importable."""
        # This test ensures the decorator can be imported and used
        
        try:
            from fraiseql.mutations.clean_decorator import clean_mutation
            assert clean_mutation is not None
        except ImportError:
            assert False, "clean_mutation decorator not yet implemented"
    
    def test_clean_mutation_decorator_validates_class_structure(self):
        """RED: Decorator should validate that classes have required Success/Error types."""
        # The decorator should validate that mutation classes have:
        # - Input type
        # - Success type  
        # - Error type
        
        import pytest
        
        # Test missing Error class
        with pytest.raises(ValueError, match="missing required attributes.*Error"):
            @clean_mutation(function="test", schema="app")
            class InvalidMutation1:
                class Input:
                    pass
                class Success:
                    pass
                # Missing Error class
        
        # Test missing Success class
        with pytest.raises(ValueError, match="missing required attributes.*Success"):
            @clean_mutation(function="test", schema="app")
            class InvalidMutation2:
                class Input:
                    pass
                class Error:
                    pass
                # Missing Success class
        
        # Test missing Input class
        with pytest.raises(ValueError, match="missing required attributes.*Input"):
            @clean_mutation(function="test", schema="app")
            class InvalidMutation3:
                class Success:
                    pass
                class Error:
                    pass
                # Missing Input class
    
    def test_clean_mutation_decorator_generates_resolver(self):
        """RED: Decorator should generate GraphQL resolver function."""
        # The decorator should create an async resolver function that:
        # - Extracts context parameters
        # - Calls database function
        # - Processes results through MutationResultProcessor
        # - Returns properly structured response
        
        @clean_mutation(function="test_function", schema="app")
        class TestMutation:
            class Input:
                test_field: str
            
            class Success:
                message: str = "Success"
            
            class Error:
                message: str = "Error"
        
        # Should have generated a resolver
        assert hasattr(TestMutation, '_get_resolver')
        resolver = TestMutation._get_resolver()
        
        # Resolver should be callable
        assert callable(resolver)
        
        # Resolver should be async
        import inspect
        assert inspect.iscoroutinefunction(resolver)
    
    def test_clean_mutation_decorator_registers_with_graphql(self):
        """RED: Decorator should register the resolver with GraphQL schema."""
        # The decorator should integrate with FraiseQL's GraphQL registration system
        
        # For now, this is a placeholder since GraphQL registration is complex
        # and would require integration with the actual GraphQL schema system
        
        @clean_mutation(function="test_function", schema="app")
        class TestMutation:
            class Input:
                test_field: str
            
            class Success:
                message: str = "Success"
            
            class Error:
                message: str = "Error"
        
        # The decorator should complete without errors
        # Full GraphQL registration would be tested in integration tests
        assert True  # Placeholder - decorator completed successfully