//! `fraiseql sbom` - Software Bill of Materials generator
//!
//! Generates SBOM in CycloneDX JSON or SPDX format by parsing
//! Cargo.lock for Rust dependencies and fraiseql.toml for project metadata.

use std::{fmt, fs, path::Path, str::FromStr};

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::info;

/// Output format for SBOM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbomFormat {
    /// CycloneDX JSON format (default)
    CycloneDx,
    /// SPDX JSON format
    Spdx,
}

impl fmt::Display for SbomFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CycloneDx => write!(f, "cyclonedx"),
            Self::Spdx => write!(f, "spdx"),
        }
    }
}

impl FromStr for SbomFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cyclonedx" | "cdx" => Ok(Self::CycloneDx),
            "spdx" => Ok(Self::Spdx),
            other => Err(format!("Unknown SBOM format: {other}. Choose: cyclonedx, spdx")),
        }
    }
}

/// Parsed Cargo.lock package entry
#[derive(Debug, Deserialize)]
struct CargoLockPackage {
    name:    String,
    version: String,
    source:  Option<String>,
}

/// Parsed Cargo.lock file
#[derive(Debug, Deserialize)]
struct CargoLock {
    package: Vec<CargoLockPackage>,
}

/// Run the SBOM command
pub fn run(format: SbomFormat, output: Option<&str>) -> Result<()> {
    info!("Generating SBOM in {format} format");

    // Load project metadata from fraiseql.toml (optional)
    let (project_name, project_version) = load_project_metadata();

    // Parse Cargo.lock
    let packages = parse_cargo_lock()?;

    // Generate SBOM
    let sbom = match format {
        SbomFormat::CycloneDx => generate_cyclonedx(&project_name, &project_version, &packages)?,
        SbomFormat::Spdx => generate_spdx(&project_name, &project_version, &packages)?,
    };

    // Output
    match output {
        Some(path) => {
            fs::write(path, &sbom).context(format!("Failed to write SBOM to {path}"))?;
            println!("SBOM written to {path}");
        },
        None => {
            println!("{sbom}");
        },
    }

    Ok(())
}

fn load_project_metadata() -> (String, String) {
    let toml_path = Path::new("fraiseql.toml");
    if toml_path.exists() {
        if let Ok(content) = fs::read_to_string(toml_path) {
            if let Ok(parsed) = toml::from_str::<toml::Value>(&content) {
                let name = parsed
                    .get("project")
                    .and_then(|p| p.get("name"))
                    .and_then(toml::Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let version = parsed
                    .get("project")
                    .and_then(|p| p.get("version"))
                    .and_then(toml::Value::as_str)
                    .unwrap_or("0.0.0")
                    .to_string();
                return (name, version);
            }
        }
    }
    ("unknown".to_string(), "0.0.0".to_string())
}

fn parse_cargo_lock() -> Result<Vec<CargoLockPackage>> {
    // Search for Cargo.lock in current dir or parent dirs
    let lock_path = find_cargo_lock()?;

    let content = fs::read_to_string(&lock_path)
        .context(format!("Failed to read {}", lock_path.display()))?;

    parse_cargo_lock_content(&content)
}

fn parse_cargo_lock_content(content: &str) -> Result<Vec<CargoLockPackage>> {
    let lock: CargoLock = toml::from_str(content).context("Failed to parse Cargo.lock")?;
    Ok(lock.package)
}

fn find_cargo_lock() -> Result<std::path::PathBuf> {
    let mut dir = std::env::current_dir().context("Failed to get current directory")?;

    loop {
        let candidate = dir.join("Cargo.lock");
        if candidate.exists() {
            return Ok(candidate);
        }

        if !dir.pop() {
            break;
        }
    }

    anyhow::bail!(
        "Cargo.lock not found. Run from a Rust project directory or a subdirectory of one."
    )
}

fn generate_cyclonedx(
    project_name: &str,
    project_version: &str,
    packages: &[CargoLockPackage],
) -> Result<String> {
    let components: Vec<serde_json::Value> = packages
        .iter()
        .map(|pkg| {
            let mut component = serde_json::json!({
                "type": "library",
                "name": pkg.name,
                "version": pkg.version,
                "purl": format!("pkg:cargo/{}@{}", pkg.name, pkg.version),
            });

            if let Some(source) = &pkg.source {
                if source.contains("registry") {
                    component["externalReferences"] = serde_json::json!([{
                        "type": "distribution",
                        "url": format!("https://crates.io/crates/{}", pkg.name),
                    }]);
                }
            }

            component
        })
        .collect();

    let sbom = serde_json::json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "name": project_name,
                "version": project_version,
            },
            "tools": [{
                "vendor": "FraiseQL",
                "name": "fraiseql-cli",
                "version": env!("CARGO_PKG_VERSION"),
            }],
        },
        "components": components,
    });

    serde_json::to_string_pretty(&sbom).context("Failed to serialize CycloneDX SBOM")
}

fn generate_spdx(
    project_name: &str,
    project_version: &str,
    packages: &[CargoLockPackage],
) -> Result<String> {
    let spdx_packages: Vec<serde_json::Value> = packages
        .iter()
        .enumerate()
        .map(|(i, pkg)| {
            serde_json::json!({
                "SPDXID": format!("SPDXRef-Package-{}", i + 1),
                "name": pkg.name,
                "versionInfo": pkg.version,
                "downloadLocation": pkg.source.as_deref().unwrap_or("NOASSERTION"),
                "filesAnalyzed": false,
                "externalRefs": [{
                    "referenceCategory": "PACKAGE-MANAGER",
                    "referenceType": "purl",
                    "referenceLocator": format!("pkg:cargo/{}@{}", pkg.name, pkg.version),
                }],
            })
        })
        .collect();

    let relationships: Vec<serde_json::Value> = packages
        .iter()
        .enumerate()
        .map(|(i, _)| {
            serde_json::json!({
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relatedSpdxElement": format!("SPDXRef-Package-{}", i + 1),
                "relationshipType": "DESCRIBES",
            })
        })
        .collect();

    let sbom = serde_json::json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": format!("{project_name}-{project_version}"),
        "documentNamespace": format!("https://spdx.org/spdxdocs/{project_name}-{project_version}"),
        "creationInfo": {
            "created": chrono_now_utc(),
            "creators": [
                format!("Tool: fraiseql-cli-{}", env!("CARGO_PKG_VERSION")),
            ],
        },
        "packages": spdx_packages,
        "relationships": relationships,
    });

    serde_json::to_string_pretty(&sbom).context("Failed to serialize SPDX SBOM")
}

/// Get current UTC timestamp in ISO 8601 format without external chrono dependency
fn chrono_now_utc() -> String {
    // Use std::time to get a basic timestamp
    let now = std::time::SystemTime::now();
    let duration = now.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();

    // Convert to date components (simplified)
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    let (year, month, day) = days_to_date(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since Unix epoch to (year, month, day)
const fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[allow(clippy::unwrap_used)]  // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sbom_format_from_str() {
        assert_eq!(SbomFormat::from_str("cyclonedx").unwrap(), SbomFormat::CycloneDx);
        assert_eq!(SbomFormat::from_str("cdx").unwrap(), SbomFormat::CycloneDx);
        assert_eq!(SbomFormat::from_str("spdx").unwrap(), SbomFormat::Spdx);
        assert!(SbomFormat::from_str("csv").is_err());
    }

    #[test]
    fn test_generate_cyclonedx() {
        let packages = vec![
            CargoLockPackage {
                name:    "serde".to_string(),
                version: "1.0.200".to_string(),
                source:  Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            },
            CargoLockPackage {
                name:    "tokio".to_string(),
                version: "1.42.0".to_string(),
                source:  Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
            },
        ];

        let result = generate_cyclonedx("test-app", "1.0.0", &packages).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["bomFormat"], "CycloneDX");
        assert_eq!(parsed["specVersion"], "1.5");
        assert_eq!(parsed["metadata"]["component"]["name"], "test-app");
        assert_eq!(parsed["components"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["components"][0]["name"], "serde");
        assert!(
            parsed["components"][0]["purl"]
                .as_str()
                .unwrap()
                .contains("pkg:cargo/serde@1.0.200")
        );
    }

    #[test]
    fn test_generate_spdx() {
        let packages = vec![CargoLockPackage {
            name:    "anyhow".to_string(),
            version: "1.0.0".to_string(),
            source:  Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
        }];

        let result = generate_spdx("test-app", "0.1.0", &packages).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["spdxVersion"], "SPDX-2.3");
        assert_eq!(parsed["packages"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["packages"][0]["name"], "anyhow");
    }

    #[test]
    fn test_find_cargo_lock() {
        // Use CARGO_MANIFEST_DIR to avoid cwd race conditions under parallel test execution
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let cargo_lock = workspace_root.join("Cargo.lock");
        assert!(cargo_lock.exists(), "Should find Cargo.lock in workspace root");
    }

    #[test]
    fn test_parse_cargo_lock() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let cargo_lock = workspace_root.join("Cargo.lock");
        let content = std::fs::read_to_string(&cargo_lock).unwrap();
        let packages = parse_cargo_lock_content(&content).unwrap();
        assert!(!packages.is_empty(), "Cargo.lock should contain packages");

        // Should contain known dependencies
        let has_serde = packages.iter().any(|p| p.name == "serde");
        assert!(has_serde, "Should contain serde dependency");
    }

    #[test]
    fn test_days_to_date_epoch() {
        let (y, m, d) = days_to_date(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_date_known() {
        // 2024-01-01 = 19723 days since epoch
        let (y, m, d) = days_to_date(19_723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    #[test]
    fn test_chrono_now_utc_format() {
        let ts = chrono_now_utc();
        // Should match ISO 8601 format
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
        assert_eq!(ts.len(), 20); // "YYYY-MM-DDTHH:MM:SSZ"
    }
}
