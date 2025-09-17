from dataclasses import dataclass

import pytest
from graphql import GraphQLError

import fraiseql as fraise
from fraiseql.core.graphql_type import translate_query_from_type
from fraiseql.sql.where_generator import DynamicType


@pytest.mark.unit
def test_fraiseql_type_decorator() -> None:
    @fraise.type(sql_source="tb_users")
    class UserTypeDecorator:
        profile: dict
        email: str

    assert UserTypeDecorator.__gql_table__ == "tb_users"
    assert UserTypeDecorator.__gql_typename__ == "UserTypeDecorator"
    assert hasattr(UserTypeDecorator, "__gql_where_type__")
    where_type = UserTypeDecorator.__gql_where_type__
    where_instance = where_type()
    assert isinstance(where_instance, DynamicType)
    assert where_instance.to_sql() is None


def test_translate_query_from_type() -> None:
    @fraise.type(sql_source="tb_accounts")
    @dataclass
    class Account:
        id: str
        role: str

    gql_query = """
    query {
        id
        role
    }
    """
    sql = translate_query_from_type(gql_query, root_type=Account)
    sql_str = sql.as_string(None)

    assert sql_str.startswith("SELECT jsonb_build_object(")
    assert "'id', data->>'id'" in sql_str
    assert "'role', data->>'role'" in sql_str
    assert "'__typename', 'Account'" in sql_str
    assert 'FROM "tb_accounts"' in sql_str


def test_translate_query_with_nested_fields() -> None:
    @fraise.type(sql_source="tb_users")
    @dataclass
    class UserGraphqlType:
        name: str
        profile: dict  # expecting nested structure

    gql_query = """
    query {
        profile {
            age
            city
        }
    }
    """
    sql = translate_query_from_type(gql_query, root_type=UserGraphqlType)
    sql_str = sql.as_string(None)

    # Type-aware operator selection: age (numeric) uses ->, city (string) uses ->>
    assert "data->'profile'->'age'" in sql_str
    assert "data->'profile'->>'city'" in sql_str
    assert "'__typename', 'UserGraphqlType'" in sql_str


def test_translate_query_with_where_clause() -> None:
    @fraise.type(sql_source="tb_sessions")
    @dataclass
    class Session:
        id: str
        active: bool

    gql_query = """
    query {
        id
    }
    """
    where = Session.__gql_where_type__(active={"eq": True})
    sql = translate_query_from_type(gql_query, root_type=Session, where=where)
    sql_str = sql.as_string(None)

    assert 'FROM "tb_sessions"' in sql_str
    assert "WHERE" in sql_str
    assert "(data ->> 'active') = 'true'" in sql_str


def test_translate_query_invalid_graphql() -> None:
    @fraise.type(sql_source="tb_invalid")
    @dataclass
    class Broken:
        dummy: str

    bad_query = """
    query {
        invalid_syntax(
    }
    """
    with pytest.raises(GraphQLError):
        translate_query_from_type(bad_query, root_type=Broken)


def test_translate_query_deeply_nested_with_typename() -> None:
    @fraise.type(sql_source="tb_orgs")
    @dataclass
    class Org:
        id: str
        account: dict

    gql_query = """
    query {
        account {
            user {
                profile {
                    city
                    country
                }
            }
        }
    }
    """
    sql = translate_query_from_type(gql_query, root_type=Org)
    sql_str = sql.as_string(None)

    assert "data->'account'->'user'->'profile'->>'city'" in sql_str
    assert "data->'account'->'user'->'profile'->>'country'" in sql_str
    assert "'__typename', 'Org'" in sql_str
    assert 'FROM "tb_orgs"' in sql_str
