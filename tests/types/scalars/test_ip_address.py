# tests/test_ip_scalar.py

from ipaddress import IPv4Address, IPv6Address

import pytest
from graphql import GraphQLError
from graphql.language import StringValueNode

from fraiseql.types.scalars.ip_address import (
    parse_ip_address_literal,
    parse_ip_address_value,
    parse_subnet_mask_literal,
    parse_subnet_mask_value,
    serialize_ip_address_string,
    serialize_subnet_mask_string,
)

# --- IpAddressString Tests ---


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        (IPv4Address("192.168.1.1"), "192.168.1.1"),
        (IPv6Address("::1"), "::1"),
        ("192.168.1.1", "192.168.1.1"),
        ("::1", "::1"),
    ],
)
def test_serialize_ip_address_string_valid(value, expected):
    assert serialize_ip_address_string(value) == expected


@pytest.mark.parametrize(
    "value",
    [
        "invalid_ip",
        1234,
        None,
        object(),
    ],
)
def test_serialize_ip_address_string_invalid(value):
    with pytest.raises(GraphQLError):
        serialize_ip_address_string(value)


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ("192.168.1.1", IPv4Address("192.168.1.1")),
        ("::1", IPv6Address("::1")),
    ],
)
def test_parse_ip_address_value_valid(value, expected):
    assert parse_ip_address_value(value) == expected


@pytest.mark.parametrize(
    "value",
    [
        "invalid",
        123,
        None,
    ],
)
def test_parse_ip_address_value_invalid(value):
    with pytest.raises(GraphQLError):
        parse_ip_address_value(value)


def test_parse_ip_address_literal_valid():
    ast = StringValueNode(value="10.0.0.1")
    assert parse_ip_address_literal(ast) == IPv4Address("10.0.0.1")


def test_parse_ip_address_literal_invalid():
    ast = StringValueNode(value="invalid-ip")
    with pytest.raises(GraphQLError):
        parse_ip_address_literal(ast)


# --- SubnetMaskString Tests ---


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        (IPv4Address("255.255.255.0"), "255.255.255.0"),
        ("255.255.255.0", "255.255.255.0"),
    ],
)
def test_serialize_subnet_mask_string_valid(value, expected):
    assert serialize_subnet_mask_string(value) == expected


@pytest.mark.parametrize(
    "value",
    [
        "255.0.255.0",  # not a valid subnet
        "not.an.ip",
        IPv6Address("::1"),
        42,
        None,
    ],
)
def test_serialize_subnet_mask_string_invalid(value):
    with pytest.raises(GraphQLError):
        serialize_subnet_mask_string(value)


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ("255.255.255.0", IPv4Address("255.255.255.0")),
    ],
)
def test_parse_subnet_mask_value_valid(value, expected):
    assert parse_subnet_mask_value(value) == expected


@pytest.mark.parametrize(
    "value",
    [
        "255.0.255.0",
        "bad",
        42,
    ],
)
def test_parse_subnet_mask_value_invalid(value):
    with pytest.raises(GraphQLError):
        parse_subnet_mask_value(value)


def test_parse_subnet_mask_value_none():
    assert parse_subnet_mask_value(None) is None


def test_parse_subnet_mask_literal_valid():
    ast = StringValueNode(value="255.255.255.0")
    assert parse_subnet_mask_literal(ast) == IPv4Address("255.255.255.0")


def test_parse_subnet_mask_literal_invalid():
    ast = StringValueNode(value="invalid")
    with pytest.raises(GraphQLError):
        parse_subnet_mask_literal(ast)
