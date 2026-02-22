"""Tests for attribute-based access control (ABAC) in FraiseQL Python.

Demonstrates fine-grained access control based on user and resource attributes.
"""

import pytest
from typing import Annotated

import fraiseql
from fraiseql.scalars import ID


class TestClearanceLevelBasedAccess:
    """Test access control based on clearance levels."""

    def test_clearance_level_access(self):
        """Test field access based on clearance level."""

        @fraiseql.type
        class ClassifiedDocument:
            """Document with clearance-based access."""

            id: ID
            publicContent: str

            @fraiseql.authorize(
                rule="hasAttribute($context, 'clearance_level', 1)"
            )
            secretContent: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ClassifiedDocument")
        assert type_info is not None

    def test_type_level_clearance_requirement(self):
        """Test entire type requiring clearance level."""

        @fraiseql.authorize(
            rule="hasAttribute($context, 'clearance_level', 3)",
            description="Only users with top secret clearance",
        )
        @fraiseql.type
        class TopSecretData:
            """Type accessible only with top secret clearance."""

            id: ID
            content: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("TopSecretData")
        assert type_info is not None

    def test_multiple_clearance_levels(self):
        """Test multiple data classification levels."""

        @fraiseql.type
        class ClassifiedContent:
            """Content with multiple classification levels."""

            unclassified: str

            @fraiseql.authorize(rule="hasAttribute($context, 'clearance_level', 1)")
            confidential: str

            @fraiseql.authorize(rule="hasAttribute($context, 'clearance_level', 2)")
            secret: str

            @fraiseql.authorize(rule="hasAttribute($context, 'clearance_level', 3)")
            topSecret: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ClassifiedContent")
        assert type_info is not None


class TestDepartmentBasedAccess:
    """Test access control based on department."""

    def test_department_access(self):
        """Test field access restricted by department."""

        @fraiseql.type
        class FinancialRecord:
            """Financial record with department access."""

            accountNumber: str

            @fraiseql.authorize(
                rule="hasAttribute($context, 'department', 'finance') OR hasRole($context, 'auditor')"
            )
            balance: float

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("FinancialRecord")
        assert type_info is not None

    def test_multiple_department_access(self):
        """Test access by multiple departments."""

        @fraiseql.type
        class HRRecord:
            """HR record accessible by multiple departments."""

            employeeId: ID

            @fraiseql.authorize(
                rule="hasAttribute($context, 'department', 'hr') OR hasAttribute($context, 'department', 'finance')"
            )
            compensation: float

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("HRRecord")
        assert type_info is not None


class TestTimeBasedAccessControl:
    """Test access control based on time windows."""

    def test_time_based_access(self):
        """Test field access restricted by time window."""

        @fraiseql.type
        class TimeSensitiveData:
            """Data accessible only during specific time window."""

            id: ID

            @fraiseql.authorize(
                rule="currentTime() >= $field.validFrom AND currentTime() <= $field.validUntil"
            )
            temporaryContent: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("TimeSensitiveData")
        assert type_info is not None


class TestGeographicRestrictions:
    """Test access control based on geographic location."""

    def test_geographic_restriction(self):
        """Test field access based on geographic location."""

        @fraiseql.type
        class RegionalData:
            """Data restricted by region."""

            regionCode: str

            @fraiseql.authorize(
                rule="userLocation() IN ['NA', 'EU', 'APAC'] AND $context.region == $field.region"
            )
            regionSpecificData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("RegionalData")
        assert type_info is not None

    def test_gdpr_compliance(self):
        """Test GDPR-compliant data access."""

        @fraiseql.type
        class PersonalData:
            """Personal data with GDPR restrictions."""

            id: ID

            @fraiseql.authorize(
                rule="($context.country IN ['DE', 'FR', 'IT'] AND hasAttribute($context, 'gdpr_compliant', true)) "
                "OR hasRole($context, 'data_processor')",
                description="GDPR-compliant access to EU personal data",
            )
            euPersonalData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("PersonalData")
        assert type_info is not None


class TestProjectBasedAccess:
    """Test access control based on project membership."""

    def test_project_member_access(self):
        """Test field access restricted to project members."""

        @fraiseql.type
        class ProjectData:
            """Project data accessible only by members."""

            projectId: str

            @fraiseql.authorize(
                rule="isMember($context.userId, $field.projectId) OR hasRole($context, 'admin')"
            )
            projectSecrets: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ProjectData")
        assert type_info is not None


class TestCombinedAttributes:
    """Test access control with multiple combined attributes."""

    def test_combined_attributes(self):
        """Test access control with multiple combined attributes."""

        @fraiseql.type
        class HighlyRestrictedData:
            """Data with multiple combined attribute requirements."""

            id: ID

            @fraiseql.authorize(
                rule="hasRole($context, 'executive') AND "
                "hasAttribute($context, 'clearance_level', 3) AND "
                "hasAttribute($context, 'department', 'c_suite') AND "
                "$context.country == 'US'",
                description="C-suite executives in US only",
            )
            multiAttributeProtectedData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("HighlyRestrictedData")
        assert type_info is not None


class TestABACPatterns:
    """Test common ABAC patterns."""

    def test_pii_protection_pattern(self):
        """Test personally identifiable information protection."""

        @fraiseql.type
        class Customer:
            """Customer with PII protection."""

            id: ID
            name: str

            @fraiseql.authorize(
                rule="hasAttribute($context, 'pii_access', true) OR hasRole($context, 'data_manager')"
            )
            email: str

            @fraiseql.authorize(
                rule="hasAttribute($context, 'pii_access', true) OR hasRole($context, 'data_manager')"
            )
            phoneNumber: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("Customer")
        assert type_info is not None

    def test_financial_data_pattern(self):
        """Test financial data protection pattern."""

        @fraiseql.type
        class FinancialData:
            """Financial data with attribute-based protection."""

            id: ID

            @fraiseql.authorize(
                rule="hasAttribute($context, 'clearance_level', 2) AND hasAttribute($context, 'department', 'finance')"
            )
            transactionAmount: float

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("FinancialData")
        assert type_info is not None

    def test_medical_records_pattern(self):
        """Test medical records protection pattern."""

        @fraiseql.type
        class MedicalRecord:
            """Medical record with attribute-based access."""

            patientId: ID

            @fraiseql.authorize(
                rule="hasAttribute($context, 'role', 'physician') OR hasAttribute($context, 'role', 'nurse')"
            )
            diagnosis: str

            @fraiseql.authorize(
                rule="hasAttribute($context, 'role', 'pharmacist')"
            )
            medications: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("MedicalRecord")
        assert type_info is not None


class TestABACQueries:
    """Test ABAC on queries."""

    def test_query_with_attribute_access(self):
        """Test query with attribute-based access control."""

        @fraiseql.type
        class PersonalData:
            """Personal data type."""

            id: ID
            email: str

        @fraiseql.authorize(
            rule="hasAttribute($context, 'department', 'hr') OR hasAttribute($context, 'department', 'finance')"
        )
        @fraiseql.query
        def employeeRecords() -> list[PersonalData]:
            """Access employee records (HR and Finance only)."""
            pass

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        query_info = registry.get_query("employeeRecords")
        assert query_info is not None


class TestDataClassificationLevels:
    """Test data with multiple classification levels."""

    def test_progressive_classification(self):
        """Test progressive data classification levels."""

        @fraiseql.type
        class ProgressiveData:
            """Data with progressive classification."""

            publicData: str

            @fraiseql.authorize(rule="hasAttribute($context, 'clearance_level', 1)")
            internalData: str

            @fraiseql.authorize(rule="hasAttribute($context, 'clearance_level', 2)")
            confidentialData: str

            @fraiseql.authorize(rule="hasAttribute($context, 'clearance_level', 3)")
            secretData: str

        from fraiseql.registry import SchemaRegistry

        registry = SchemaRegistry()
        type_info = registry.get_type("ProgressiveData")
        assert type_info is not None
