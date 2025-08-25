import uuid

import pytest

from fraiseql.types.scalars.id_scalar import ID


@pytest.mark.unit
class TestIDClass:
    """Test suite for ID class."""

    def test_init_with_uuid(self) -> None:
        """Test initializing ID with UUID object."""
        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")
        id_obj = ID(test_uuid)
        assert id_obj.uuid == test_uuid
        assert str(id_obj) == "12345678-1234-5678-1234-567812345678"

    def test_init_with_uuid_string(self) -> None:
        """Test initializing ID with UUID string."""
        uuid_str = "12345678-1234-5678-1234-567812345678"
        id_obj = ID(uuid_str)
        assert str(id_obj) == uuid_str
        assert isinstance(id_obj.uuid, uuid.UUID)

    def test_init_with_invalid_uuid_string(self) -> None:
        """Test initializing ID with invalid UUID string raises error."""
        with pytest.raises(TypeError, match="Invalid UUID string"):
            ID("not-a-uuid")

    def test_init_with_invalid_type(self) -> None:
        """Test initializing ID with invalid type raises error."""
        with pytest.raises(TypeError, match="ID must be initialized with a UUID or str"):
            ID(123)

    def test_coerce_from_id(self) -> None:
        """Test coercing ID from another ID instance."""
        id1 = ID("12345678-1234-5678-1234-567812345678")
        id2 = ID.coerce(id1)
        assert id1 == id2
        assert id1 is id2  # Should return same instance

    def test_coerce_from_uuid(self) -> None:
        """Test coercing ID from UUID."""
        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")
        id_obj = ID.coerce(test_uuid)
        assert id_obj.uuid == test_uuid

    def test_coerce_from_string(self) -> None:
        """Test coercing ID from string."""
        uuid_str = "12345678-1234-5678-1234-567812345678"
        id_obj = ID.coerce(uuid_str)
        assert str(id_obj) == uuid_str

    def test_coerce_invalid_type(self) -> None:
        """Test coercing from invalid type raises error."""
        with pytest.raises(TypeError, match="Cannot coerce int to ID"):
            ID.coerce(123)

    def test_equality_with_id(self) -> None:
        """Test equality between ID instances."""
        id1 = ID("12345678-1234-5678-1234-567812345678")
        id2 = ID("12345678-1234-5678-1234-567812345678")
        id3 = ID("87654321-4321-8765-4321-876543218765")

        assert id1 == id2
        assert id1 != id3

    def test_equality_with_uuid(self) -> None:
        """Test equality between ID and UUID."""
        test_uuid = uuid.UUID("12345678-1234-5678-1234-567812345678")
        id_obj = ID(test_uuid)

        assert id_obj == test_uuid

    def test_equality_with_other_types(self) -> None:
        """Test equality with non-ID types returns NotImplemented."""
        id_obj = ID("12345678-1234-5678-1234-567812345678")
        assert id_obj.__eq__("12345678-1234-5678-1234-567812345678") is NotImplemented
        assert id_obj.__eq__(123) is NotImplemented

    def test_hash(self) -> None:
        """Test ID hashing."""
        id1 = ID("12345678-1234-5678-1234-567812345678")
        id2 = ID("12345678-1234-5678-1234-567812345678")

        assert hash(id1) == hash(id2)

        # Should be usable in sets and dicts
        id_set = {id1, id2}
        assert len(id_set) == 1

    def test_repr(self) -> None:
        """Test ID repr."""
        id_obj = ID("12345678-1234-5678-1234-567812345678")
        assert repr(id_obj) == "ID('12345678-1234-5678-1234-567812345678')"

    def test_str(self) -> None:
        """Test ID string representation."""
        id_obj = ID("12345678-1234-5678-1234-567812345678")
        assert str(id_obj) == "12345678-1234-5678-1234-567812345678"
