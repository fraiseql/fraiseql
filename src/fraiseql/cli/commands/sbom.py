"""SBOM Generation CLI Command.

Command-line interface for generating Software Bill of Materials (SBOM)
compliant with global supply chain security regulations (US EO 14028,
EU NIS2/CRA, PCI-DSS 4.0, ISO 27001, etc.).

Usage:
    fraiseql sbom generate --output fraiseql-1.5.0-sbom.json
    fraiseql sbom generate --format xml --output fraiseql-sbom.xml
    fraiseql sbom validate --input fraiseql-sbom.json
"""

import logging
from pathlib import Path
from typing import Optional

import click

try:
    import tomllib  # Python 3.11+
except ImportError:
    import tomli as tomllib  # Fallback for Python < 3.11

from fraiseql.sbom.application.sbom_generator import SBOMGenerator
from fraiseql.sbom.domain.models import Supplier
from fraiseql.sbom.infrastructure.package_scanner import PythonPackageScanner

logger = logging.getLogger(__name__)


@click.group(name="sbom")
def sbom_cli() -> None:
    """Software Bill of Materials (SBOM) management commands.

    Generate, validate, and manage SBOM files for global regulatory compliance.
    """


@sbom_cli.command(name="generate")
@click.option(
    "--output",
    "-o",
    type=click.Path(path_type=Path),
    required=True,
    help="Output file path for SBOM",
)
@click.option(
    "--format",
    "-f",
    type=click.Choice(["json", "xml"], case_sensitive=False),
    default="json",
    help="Output format (default: json)",
)
@click.option(
    "--component-name",
    type=str,
    help="Component name (auto-detected from pyproject.toml if not specified)",
)
@click.option(
    "--component-version",
    type=str,
    help="Component version (auto-detected from pyproject.toml if not specified)",
)
@click.option(
    "--supplier-name",
    type=str,
    help="Supplier/organization name",
)
@click.option(
    "--supplier-url",
    type=str,
    help="Supplier/organization website URL",
)
@click.option(
    "--supplier-contact",
    type=str,
    help="Supplier/organization contact email",
)
@click.option(
    "--author",
    "-a",
    multiple=True,
    help="Author name (can be specified multiple times)",
)
@click.option(
    "--project-root",
    type=click.Path(exists=True, path_type=Path),
    help="Project root directory (default: current directory)",
)
@click.option(
    "--include-dev",
    is_flag=True,
    help="Include development dependencies",
)
@click.option(
    "--verbose",
    "-v",
    is_flag=True,
    help="Enable verbose logging",
)
def generate_sbom(
    output: Path,
    format: str,
    component_name: Optional[str],
    component_version: Optional[str],
    supplier_name: Optional[str],
    supplier_url: Optional[str],
    supplier_contact: Optional[str],
    author: tuple[str, ...],
    project_root: Optional[Path],
    include_dev: bool,
    verbose: bool,
) -> None:
    r"""Generate Software Bill of Materials (SBOM).

    Creates a CycloneDX-format SBOM compliant with global supply chain
    security regulations (US EO 14028, EU NIS2/CRA, PCI-DSS 4.0, ISO 27001).

    Examples:
        # Generate SBOM with auto-detected metadata
        fraiseql sbom generate --output fraiseql-1.5.0-sbom.json

        # Generate SBOM with custom metadata
        fraiseql sbom generate \\
            --output sbom.json \\
            --component-name "fraiseql" \\
            --component-version "1.5.0" \\
            --supplier-name "Evolution Digitale" \\
            --supplier-url "https://fraiseql.com" \\
            --author "Lionel Hamayon"

        # Generate XML format SBOM
        fraiseql sbom generate --format xml --output sbom.xml
    """
    # Configure logging
    log_level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=log_level,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )

    click.echo("🔍 Generating Software Bill of Materials (SBOM)...")
    click.echo("")

    # Determine project root
    proj_root = project_root or Path.cwd()

    # Auto-detect component name and version from pyproject.toml
    if not component_name or not component_version:
        pyproject_path = proj_root / "pyproject.toml"
        if pyproject_path.exists():
            try:
                with pyproject_path.open("rb") as f:
                    pyproject_data = tomllib.load(f)

                project = pyproject_data.get("project", {})

                if not component_name:
                    component_name = project.get("name", "unknown")
                    click.echo(f"   Component name: {component_name} (auto-detected)")

                if not component_version:
                    component_version = project.get("version", "0.0.0")
                    click.echo(f"   Component version: {component_version} (auto-detected)")

            except Exception as e:
                click.echo(f"   ⚠️  Failed to read pyproject.toml: {e}", err=True)
                if not component_name:
                    component_name = "unknown"
                if not component_version:
                    component_version = "0.0.0"
        else:
            click.echo(f"   ⚠️  pyproject.toml not found in {proj_root}", err=True)
            if not component_name:
                component_name = "unknown"
            if not component_version:
                component_version = "0.0.0"

    # Create supplier if provided
    supplier = None
    if supplier_name:
        supplier = Supplier(
            name=supplier_name,
            url=supplier_url,
            contact=supplier_contact,
        )

    # Initialize infrastructure
    click.echo(f"   Scanning packages from: {proj_root}")
    package_scanner = PythonPackageScanner(project_root=proj_root)

    # Initialize application service
    generator = SBOMGenerator(
        metadata_repository=package_scanner,
        include_dev_dependencies=include_dev,
    )

    # Generate SBOM
    try:
        click.echo("")
        click.echo("   Scanning installed packages...")

        output_path = generator.generate_and_save(
            output_path=output,
            component_name=component_name,
            component_version=component_version,
            format=format,
            supplier=supplier,
            authors=list(author) if author else None,
        )

        click.echo("")
        click.echo("✅ SBOM generated successfully!")
        click.echo(f"   Output: {output_path}")
        click.echo(f"   Format: CycloneDX {format.upper()}")
        click.echo("   Spec Version: 1.5")
        click.echo("")
        click.echo("📄 To verify the SBOM:")
        click.echo(f"   cyclonedx validate --input-file {output_path}")
        click.echo("")
        click.echo("🔒 To sign the SBOM (recommended):")
        click.echo(f"   cosign sign-blob --yes {output_path} > {output_path}.sig")

    except Exception as e:
        click.echo("")
        click.echo(f"❌ SBOM generation failed: {e}", err=True)
        if verbose:
            logger.exception("SBOM generation error")
        raise click.Abort


@sbom_cli.command(name="validate")
@click.option(
    "--input",
    "-i",
    "input_file",
    type=click.Path(exists=True, path_type=Path),
    required=True,
    help="SBOM file to validate",
)
@click.option(
    "--verbose",
    "-v",
    is_flag=True,
    help="Enable verbose output",
)
def validate_sbom(input_file: Path, verbose: bool) -> None:
    """Validate an SBOM file against CycloneDX schema.

    Checks that the SBOM file is valid and complete according to
    CycloneDX 1.5 specification.

    Examples:
        # Validate SBOM file
        fraiseql sbom validate --input fraiseql-1.5.0-sbom.json
    """
    click.echo(f"🔍 Validating SBOM: {input_file}")
    click.echo("")

    try:
        import json

        # Read SBOM file
        with input_file.open() as f:
            sbom_data = json.load(f)

        # Basic validation checks
        required_fields = ["bomFormat", "specVersion", "serialNumber", "version"]
        missing_fields = [field for field in required_fields if field not in sbom_data]

        if missing_fields:
            click.echo(
                f"❌ Validation failed: Missing required fields: {missing_fields}",
                err=True,
            )
            raise click.Abort

        # Check format
        if sbom_data.get("bomFormat") != "CycloneDX":
            click.echo(
                f"❌ Validation failed: Invalid bomFormat: {sbom_data.get('bomFormat')}",
                err=True,
            )
            raise click.Abort

        # Check spec version
        spec_version = sbom_data.get("specVersion")
        if spec_version not in ["1.4", "1.5", "1.6"]:
            click.echo(
                f"⚠️  Warning: Spec version {spec_version} may not be supported",
                err=True,
            )

        # Check components
        components = sbom_data.get("components", [])
        if not components:
            click.echo("⚠️  Warning: SBOM contains no components", err=True)

        # Count components by type
        component_types: dict[str, int] = {}
        copyleft_count = 0

        for comp in components:
            comp_type = comp.get("type", "unknown")
            component_types[comp_type] = component_types.get(comp_type, 0) + 1

            # Check for copyleft licenses
            licenses = comp.get("licenses", [])
            for lic in licenses:
                license_info = lic.get("license", {})
                license_id = license_info.get("id", "")
                if "GPL" in license_id and "LGPL" not in license_id:
                    copyleft_count += 1

        click.echo("✅ SBOM is valid!")
        click.echo("")
        click.echo(f"   Serial Number: {sbom_data.get('serialNumber')}")
        click.echo(f"   Spec Version: {spec_version}")
        click.echo(f"   Total Components: {len(components)}")

        if component_types:
            click.echo("   Component Types:")
            for comp_type, count in sorted(component_types.items()):
                click.echo(f"      - {comp_type}: {count}")

        if copyleft_count > 0:
            click.echo("")
            click.echo(
                f"⚠️  Warning: Found {copyleft_count} components with copyleft licenses (GPL)"
            )
            click.echo("   Copyleft licenses may restrict federal use. Review carefully.")

        if verbose:
            click.echo("")
            click.echo("📊 Detailed Component List:")
            for comp in components[:10]:  # Show first 10
                name = comp.get("name", "unknown")
                version = comp.get("version", "unknown")
                click.echo(f"   - {name}@{version}")
            if len(components) > 10:
                click.echo(f"   ... and {len(components) - 10} more")

    except json.JSONDecodeError as e:
        click.echo(f"❌ Invalid JSON: {e}", err=True)
        raise click.Abort
    except Exception as e:
        click.echo(f"❌ Validation failed: {e}", err=True)
        raise click.Abort


# Register command group
__all__ = ["sbom_cli"]
