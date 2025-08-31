"""
FraiseQL Relay Extension - Entity Discovery Tests

Tests the automatic entity discovery functionality.
"""

import pytest
from unittest.mock import AsyncMock, MagicMock
from uuid import UUID
from typing import Dict, Any, List

try:
    from fraiseql_relay_extension.python_integration.discovery import (
        EntityDiscovery,
        discover_and_register_entities,
        create_dynamic_node_type
    )
    import fraiseql
    from fraiseql import CQRSRepository
except ImportError as e:
    pytest.skip(f"FraiseQL dependencies not available: {e}", allow_module_level=True)


# Test fixtures
@pytest.fixture
def mock_db_pool():
    """Mock database pool."""
    return AsyncMock()


@pytest.fixture
def mock_cqrs_repo():
    """Mock CQRS repository."""
    repo = AsyncMock(spec=CQRSRepository)
    return repo


@pytest.fixture
async def entity_discovery(mock_db_pool):
    """EntityDiscovery instance with mocked dependencies."""
    return EntityDiscovery(mock_db_pool)


# EntityDiscovery Tests
class TestEntityDiscovery:
    """Test EntityDiscovery functionality."""

    async def test_initialization(self, mock_db_pool):
        """Test EntityDiscovery initialization."""
        discovery = EntityDiscovery(mock_db_pool)

        assert discovery.db_pool == mock_db_pool
        assert discovery.discovered_entities == []

    async def test_discover_from_database_views(self, entity_discovery, mock_db_pool):
        """Test discovery from PostgreSQL views."""
        # Mock database connection and responses
        mock_conn = AsyncMock()
        mock_repo = AsyncMock(spec=CQRSRepository)

        # Mock pool acquire
        mock_db_pool.acquire.return_value.__aenter__.return_value = mock_conn

        # Mock views query response
        mock_views = [
            {
                'schemaname': 'public',
                'viewname': 'v_user',
                'definition': 'SELECT pk_user as id, jsonb_build_object(...) as data FROM tb_user'
            },
            {
                'schemaname': 'public',
                'viewname': 'tv_contract',
                'definition': 'SELECT pk_contract as id, data FROM materialized_contracts'
            },
            {
                'schemaname': 'public',
                'viewname': 'mv_analytics',
                'definition': 'SELECT * FROM analytics_summary'
            }
        ]

        # Mock tables query response
        mock_tables = [
            {'schemaname': 'public', 'tablename': 'tb_user'},
            {'schemaname': 'public', 'tablename': 'tb_contract'}
        ]

        # Mock repo methods
        mock_repo.execute_raw.side_effect = [mock_views, mock_tables]

        # Mock the analyze methods
        entity_discovery._analyze_view = AsyncMock(side_effect=[
            {'entity_name': 'User', 'pk_column': 'pk_user', 'v_table': 'public.v_user', 'source_table': 'public.tb_user'},
            {'entity_name': 'Contract', 'pk_column': 'pk_contract', 'tv_table': 'public.tv_contract', 'source_table': 'public.tb_contract'},
            None  # mv_analytics doesn't return entity info
        ])

        entity_discovery._analyze_table = AsyncMock(side_effect=[
            None,  # First table already processed
            None   # Second table already processed
        ])

        # Mock CQRSRepository constructor
        with pytest.MonkeyPatch().context() as m:
            m.setattr('fraiseql_relay_extension.python_integration.discovery.CQRSRepository', lambda conn: mock_repo)

            entities = await entity_discovery.discover_from_database(['public'])

        # Verify results
        assert len(entities) == 2

        user_entity = next((e for e in entities if e['entity_name'] == 'User'), None)
        assert user_entity is not None
        assert user_entity['pk_column'] == 'pk_user'
        assert user_entity['v_table'] == 'public.v_user'

        contract_entity = next((e for e in entities if e['entity_name'] == 'Contract'), None)
        assert contract_entity is not None
        assert contract_entity['pk_column'] == 'pk_contract'
        assert contract_entity['tv_table'] == 'public.tv_contract'

    def test_view_name_to_entity_name(self, entity_discovery):
        """Test view name to entity name conversion."""
        assert entity_discovery._view_name_to_entity_name('user') == 'User'
        assert entity_discovery._view_name_to_entity_name('contract_item') == 'ContractItem'
        assert entity_discovery._view_name_to_entity_name('user_preference') == 'UserPreference'

    def test_table_name_to_entity_name(self, entity_discovery):
        """Test table name to entity name conversion."""
        assert entity_discovery._table_name_to_entity_name('user') == 'User'
        assert entity_discovery._table_name_to_entity_name('contract_item') == 'ContractItem'
        assert entity_discovery._table_name_to_entity_name('user_preference') == 'UserPreference'

    def test_snake_to_pascal_case(self, entity_discovery):
        """Test snake_case to PascalCase conversion."""
        assert entity_discovery._snake_to_pascal_case('user') == 'User'
        assert entity_discovery._snake_to_pascal_case('contract_item') == 'ContractItem'
        assert entity_discovery._snake_to_pascal_case('user_preference_setting') == 'UserPreferenceSetting'
        assert entity_discovery._snake_to_pascal_case('already_pascal') == 'AlreadyPascal'

    async def test_analyze_view_v_prefix(self, entity_discovery, mock_cqrs_repo):
        """Test analyzing a view with v_ prefix."""
        view_data = {
            'viewname': 'v_user',
            'schemaname': 'public',
            'definition': 'SELECT pk_user, data FROM tb_user'
        }

        # Mock the helper methods
        entity_discovery._extract_pk_column = AsyncMock(return_value='pk_user')
        entity_discovery._find_source_table = AsyncMock(return_value='tb_user')

        result = await entity_discovery._analyze_view(mock_cqrs_repo, view_data)

        assert result is not None
        assert result['entity_name'] == 'User'
        assert result['pk_column'] == 'pk_user'
        assert result['v_table'] == 'public.v_user'
        assert result['source_table'] == 'tb_user'

    async def test_analyze_view_tv_prefix(self, entity_discovery, mock_cqrs_repo):
        """Test analyzing a view with tv_ prefix."""
        view_data = {
            'viewname': 'tv_contract',
            'schemaname': 'tenant',
            'definition': 'SELECT pk_contract, data FROM materialized_contracts'
        }

        entity_discovery._extract_pk_column = AsyncMock(return_value='pk_contract')
        entity_discovery._find_source_table = AsyncMock(return_value='tb_contract')
        entity_discovery._find_corresponding_view = AsyncMock(return_value='tenant.v_contract')

        result = await entity_discovery._analyze_view(mock_cqrs_repo, view_data)

        assert result is not None
        assert result['entity_name'] == 'Contract'
        assert result['pk_column'] == 'pk_contract'
        assert result['tv_table'] == 'tenant.tv_contract'
        assert result['v_table'] == 'tenant.v_contract'

    async def test_analyze_view_no_pk(self, entity_discovery, mock_cqrs_repo):
        """Test analyzing a view with no identifiable primary key."""
        view_data = {
            'viewname': 'v_summary',
            'schemaname': 'public',
            'definition': 'SELECT COUNT(*) FROM somewhere'
        }

        entity_discovery._extract_pk_column = AsyncMock(return_value=None)

        result = await entity_discovery._analyze_view(mock_cqrs_repo, view_data)

        assert result is None  # Should return None for views without PK

    async def test_analyze_table(self, entity_discovery, mock_cqrs_repo):
        """Test analyzing a command-side table."""
        table_data = {
            'tablename': 'tb_user',
            'schemaname': 'tenant'
        }

        # Mock the database query for UUID columns
        mock_cqrs_repo.execute_raw.return_value = [
            {'column_name': 'pk_user'}
        ]

        result = await entity_discovery._analyze_table(mock_cqrs_repo, table_data, [])

        assert result is not None
        assert result['entity_name'] == 'User'
        assert result['pk_column'] == 'pk_user'
        assert result['source_table'] == 'tenant.tb_user'

        # Verify the query was made
        mock_cqrs_repo.execute_raw.assert_called_once()
        call_args = mock_cqrs_repo.execute_raw.call_args[0]
        assert 'pk_%' in call_args[0]  # Check the SQL contains the pk_ pattern

    async def test_analyze_table_no_uuid_pk(self, entity_discovery, mock_cqrs_repo):
        """Test analyzing a table with no UUID primary key."""
        table_data = {
            'tablename': 'tb_log',
            'schemaname': 'public'
        }

        # Mock no UUID columns found
        mock_cqrs_repo.execute_raw.return_value = []

        result = await entity_discovery._analyze_table(mock_cqrs_repo, table_data, [])

        assert result is None  # Should return None for tables without UUID PK

    def test_is_potential_entity_type(self, entity_discovery):
        """Test entity type detection."""

        # Valid entity type
        @fraiseql.type
        class ValidEntity:
            id: UUID
            name: str
            email: str

        assert entity_discovery._is_potential_entity_type(ValidEntity)

        # Invalid - no id field
        @fraiseql.type
        class NoIdEntity:
            name: str
            email: str

        assert not entity_discovery._is_potential_entity_type(NoIdEntity)

        # Invalid - only id field
        @fraiseql.type
        class OnlyIdEntity:
            id: UUID

        assert not entity_discovery._is_potential_entity_type(OnlyIdEntity)

        # Invalid - built-in scalar
        class BuiltinType:
            __name__ = 'String'
            __annotations__ = {'id': UUID, 'value': str}

        assert not entity_discovery._is_potential_entity_type(BuiltinType)

    async def test_extract_pk_column_patterns(self, entity_discovery, mock_cqrs_repo):
        """Test primary key column extraction patterns."""
        # Mock successful column query
        mock_cqrs_repo.execute_raw.return_value = [
            {'column_name': 'pk_user', 'data_type': 'uuid'}
        ]

        result = await entity_discovery._extract_pk_column(mock_cqrs_repo, 'public.v_user', 'User')

        assert result == 'pk_user'

        # Test fallback when no columns found
        mock_cqrs_repo.execute_raw.return_value = []

        result = await entity_discovery._extract_pk_column(mock_cqrs_repo, 'public.v_missing', 'Missing')

        assert result == 'pk_missing'  # Fallback pattern

    async def test_find_source_table(self, entity_discovery, mock_cqrs_repo):
        """Test source table discovery."""
        # Mock table exists query
        mock_cqrs_repo.execute_raw.return_value = [{'exists': True}]

        result = await entity_discovery._find_source_table(mock_cqrs_repo, 'User')

        assert result == 'tb_user'  # First pattern should match

        # Mock no tables found
        mock_cqrs_repo.execute_raw.return_value = []

        result = await entity_discovery._find_source_table(mock_cqrs_repo, 'NonExistent')

        assert result is None

    async def test_find_corresponding_view(self, entity_discovery, mock_cqrs_repo):
        """Test finding corresponding views with different prefixes."""
        # Mock view exists
        mock_cqrs_repo.execute_raw.return_value = [{'schemaname': 'public'}]

        result = await entity_discovery._find_corresponding_view(mock_cqrs_repo, 'User', 'v_')

        assert result == 'public.v_user'

        # Mock view doesn't exist
        mock_cqrs_repo.execute_raw.return_value = []

        result = await entity_discovery._find_corresponding_view(mock_cqrs_repo, 'Missing', 'v_')

        assert result is None


class TestDynamicNodeType:
    """Test dynamic node type creation."""

    def test_create_dynamic_node_type(self):
        """Test creating a dynamic node type."""
        NodeType = create_dynamic_node_type('DynamicUser')

        assert NodeType.__name__ == 'DynamicUser'
        assert NodeType.__qualname__ == 'DynamicUser'

        # Test instance creation and from_dict
        instance = NodeType()
        assert instance.__typename == 'DynamicUser'

        # Test from_dict method
        test_data = {
            'id': '550e8400-e29b-41d4-a716-446655440000',
            'name': 'Test User',
            'email': 'test@example.com'
        }

        instance = NodeType.from_dict(test_data)
        assert hasattr(instance, 'id')
        assert hasattr(instance, 'name')
        assert hasattr(instance, 'email')
        assert instance.name == 'Test User'
        assert instance.email == 'test@example.com'


class TestDiscoveryIntegration:
    """Test full discovery integration scenarios."""

    async def test_discover_and_register_entities(self, mock_db_pool):
        """Test the convenience function for discovery and registration."""
        # Mock RelayIntegration
        mock_relay = AsyncMock()
        mock_relay.db_pool = mock_db_pool

        # Mock EntityDiscovery
        mock_discovery = AsyncMock()
        mock_discovery.discover_from_database.return_value = [
            {
                'entity_name': 'User',
                'pk_column': 'pk_user',
                'v_table': 'v_user',
                'source_table': 'tb_user'
            },
            {
                'entity_name': 'Post',
                'pk_column': 'pk_post',
                'v_table': 'v_post',
                'source_table': 'tb_post'
            }
        ]

        with pytest.MonkeyPatch().context() as m:
            m.setattr('fraiseql_relay_extension.python_integration.discovery.EntityDiscovery', lambda pool: mock_discovery)
            m.setattr('fraiseql_relay_extension.python_integration.discovery.create_dynamic_node_type', lambda name: type(name, (), {}))

            result = await discover_and_register_entities(mock_relay, ['public'])

        # Verify discovery was called
        mock_discovery.discover_from_database.assert_called_once_with(['public'])

        # Verify entities were registered
        assert mock_relay.register_entity_type.call_count == 2

        # Check registration calls
        calls = mock_relay.register_entity_type.call_args_list

        # First call should be for User
        user_call = calls[0][1]  # kwargs
        assert user_call['entity_name'] == 'User'
        assert user_call['pk_column'] == 'pk_user'

        # Second call should be for Post
        post_call = calls[1][1]  # kwargs
        assert post_call['entity_name'] == 'Post'
        assert post_call['pk_column'] == 'pk_post'

        assert result == 2  # Should return number of registered entities

    async def test_discover_and_register_with_errors(self, mock_db_pool):
        """Test discovery with some registration errors."""
        mock_relay = AsyncMock()
        mock_relay.db_pool = mock_db_pool

        # Mock registration failure for second entity
        mock_relay.register_entity_type.side_effect = [
            None,  # First registration succeeds
            Exception("Registration failed")  # Second fails
        ]

        mock_discovery = AsyncMock()
        mock_discovery.discover_from_database.return_value = [
            {'entity_name': 'User', 'pk_column': 'pk_user', 'v_table': 'v_user', 'source_table': 'tb_user'},
            {'entity_name': 'Post', 'pk_column': 'pk_post', 'v_table': 'v_post', 'source_table': 'tb_post'}
        ]

        with pytest.MonkeyPatch().context() as m:
            m.setattr('fraiseql_relay_extension.python_integration.discovery.EntityDiscovery', lambda pool: mock_discovery)
            m.setattr('fraiseql_relay_extension.python_integration.discovery.create_dynamic_node_type', lambda name: type(name, (), {}))

            result = await discover_and_register_entities(mock_relay, ['public'])

        # Should return 1 (only successful registrations)
        assert result == 1

        # Both registration attempts should have been made
        assert mock_relay.register_entity_type.call_count == 2


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
