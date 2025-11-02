# Extracted from: docs/patterns/trinity_identifiers.md
# Block number: 4
from pydantic import BaseModel, validator


class ProductInput(BaseModel):
    public_id: str

    @validator("public_id")
    def validate_public_id(cls, v):
        # Ensure public ID format
        if not v.isalnum():
            raise ValueError("Public ID must be alphanumeric")
        return v.upper()
