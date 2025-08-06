"""Example mutations demonstrating context parameter usage."""

from uuid import UUID

from fraiseql.mutations import mutation


class CreateLocationInput:
    """Input for creating a new location."""

    name: str
    address: str
    latitude: float
    longitude: float

    def to_dict(self):
        return {
            "name": self.name,
            "address": self.address,
            "latitude": self.latitude,
            "longitude": self.longitude,
        }


class CreateLocationSuccess:
    """Success response for location creation."""

    def __init__(self, location_id: UUID, message: str = "Location created successfully"):
        self.location_id = location_id
        self.message = message


class CreateLocationError:
    """Error response for location creation."""

    def __init__(self, message: str, code: str):
        self.message = message
        self.code = code


@mutation(
    function="create_location",
    schema="app",
    context_params={
        "tenant_id": "input_pk_organization",  # Maps context["tenant_id"] to first parameter
        "user": "input_created_by",  # Maps context["user"].user_id to second parameter
    },
)
class CreateLocation:
    """Create a new location with tenant isolation and user tracking."""

    input: CreateLocationInput
    success: CreateLocationSuccess
    failure: CreateLocationError


class UpdateLocationInput:
    """Input for updating a location."""

    id: UUID
    name: str | None = None
    address: str | None = None
    latitude: float | None = None
    longitude: float | None = None

    def to_dict(self):
        data = {"id": str(self.id)}
        if self.name is not None:
            data["name"] = self.name
        if self.address is not None:
            data["address"] = self.address
        if self.latitude is not None:
            data["latitude"] = self.latitude
        if self.longitude is not None:
            data["longitude"] = self.longitude
        return data


class UpdateLocationSuccess:
    """Success response for location update."""

    def __init__(
        self,
        location_id: UUID,
        updated_fields: list[str],
        message: str = "Location updated successfully",
    ):
        self.location_id = location_id
        self.updated_fields = updated_fields
        self.message = message


class UpdateLocationError:
    """Error response for location update."""

    def __init__(self, message: str, code: str):
        self.message = message
        self.code = code


@mutation(
    function="update_location",
    schema="app",
    context_params={"tenant_id": "input_pk_organization", "user": "input_updated_by"},
)
class UpdateLocation:
    """Update an existing location with tenant isolation."""

    input: UpdateLocationInput
    success: UpdateLocationSuccess
    failure: UpdateLocationError


class DeleteLocationInput:
    """Input for deleting a location."""

    id: UUID

    def to_dict(self):
        return {"id": str(self.id)}


class DeleteLocationSuccess:
    """Success response for location deletion."""

    def __init__(self, location_id: UUID, message: str = "Location deleted successfully"):
        self.location_id = location_id
        self.message = message


class DeleteLocationError:
    """Error response for location deletion."""

    def __init__(self, message: str, code: str):
        self.message = message
        self.code = code


@mutation(
    function="delete_location",
    schema="app",
    context_params={"tenant_id": "input_pk_organization", "user": "input_deleted_by"},
)
class DeleteLocation:
    """Delete a location with tenant isolation and audit trail."""

    input: DeleteLocationInput
    success: DeleteLocationSuccess
    failure: DeleteLocationError


# Example of mutation without context parameters (legacy style)
class CreateCategoryInput:
    """Input for creating a category (legacy single-parameter style)."""

    name: str
    description: str
    tenant_id: UUID  # Included in business data (not ideal)
    created_by: UUID

    def to_dict(self):
        return {
            "name": self.name,
            "description": self.description,
            "tenant_id": str(self.tenant_id),
            "created_by": str(self.created_by),
        }


class CreateCategorySuccess:
    """Success response for category creation."""

    def __init__(self, category_id: UUID, message: str = "Category created successfully"):
        self.category_id = category_id
        self.message = message


class CreateCategoryError:
    """Error response for category creation."""

    def __init__(self, message: str, code: str):
        self.message = message
        self.code = code


@mutation(
    function="create_category",
    schema="app",
    # No context_params - uses legacy single JSONB parameter
)
class CreateCategory:
    """Create a category using legacy single-parameter style."""

    input: CreateCategoryInput
    success: CreateCategorySuccess
    failure: CreateCategoryError
