use std::fs;

fn workspace_root() -> std::path::PathBuf {
    let mut path = std::env::current_dir().expect("Failed to get current directory");
    // Find Cargo.toml at workspace root
    loop {
        if path.join("Cargo.toml").exists()
            && path.join("Cargo.lock").exists()
            && path.join("crates").exists()
        {
            return path;
        }
        if !path.pop() {
            panic!("Could not find workspace root");
        }
    }
}

#[test]
fn test_dockerfile_exists() {
    let root = workspace_root();
    assert!(
        root.join("Dockerfile").exists(),
        "Dockerfile must exist at project root"
    );
}

#[test]
fn test_dockerignore_exists() {
    let root = workspace_root();
    assert!(
        root.join(".dockerignore").exists(),
        ".dockerignore must exist at project root"
    );
}

#[test]
fn test_deploy_docker_directory_exists() {
    let root = workspace_root();
    assert!(
        root.join("deploy/docker").exists(),
        "deploy/docker directory must exist"
    );
}

#[test]
fn test_dockerfile_multi_arch_buildkit() {
    let root = workspace_root();
    let dockerfile = fs::read_to_string(root.join("Dockerfile"))
        .expect("Failed to read Dockerfile");
    // BuildKit syntax for multi-arch builds
    assert!(
        dockerfile.contains("syntax=docker/dockerfile:1.4"),
        "BuildKit syntax required in Dockerfile"
    );
    assert!(
        dockerfile.contains("rust:"),
        "Must have Rust builder stage"
    );
}

#[test]
fn test_helm_chart_valid() {
    let root = workspace_root();
    let chart_path = root.join("deploy/kubernetes/helm/fraiseql/Chart.yaml");
    assert!(
        chart_path.exists(),
        "Helm Chart.yaml must exist"
    );

    let chart_content = fs::read_to_string(&chart_path).expect("Failed to read Chart.yaml");
    let chart: serde_yaml::Value =
        serde_yaml::from_str(&chart_content).expect("Chart.yaml must be valid YAML");

    // Validate Chart.yaml structure
    assert!(
        chart.get("apiVersion").is_some(),
        "Chart must have apiVersion"
    );
    assert!(chart.get("name").is_some(), "Chart must have name");
    assert!(chart.get("version").is_some(), "Chart must have version");
}

#[test]
fn test_kube_manifests_exist() {
    let root = workspace_root();
    let manifests_dir = root.join("deploy/kubernetes");
    assert!(
        manifests_dir.exists(),
        "deploy/kubernetes directory must exist"
    );

    assert!(
        root.join("deploy/kubernetes/deployment.yaml").exists(),
        "deployment.yaml must exist"
    );
    assert!(
        root.join("deploy/kubernetes/service.yaml").exists(),
        "service.yaml must exist"
    );
    assert!(
        root.join("deploy/kubernetes/configmap.yaml").exists(),
        "configmap.yaml must exist"
    );
}

#[test]
fn test_ci_cd_workflows_exist() {
    let root = workspace_root();
    assert!(
        root.join(".github/workflows").exists(),
        ".github/workflows directory must exist"
    );
    assert!(
        root.join(".github/workflows/build-docker.yml").exists(),
        "Docker build workflow required"
    );
    assert!(
        root.join(".github/workflows/build-sbom.yml").exists(),
        "SBOM workflow required"
    );
}

#[test]
fn test_docker_compose_exists() {
    let root = workspace_root();
    assert!(
        root.join("docker-compose.yml").exists(),
        "docker-compose.yml must exist for local development"
    );
}

// ============================================================================
// Cycle 10.2: Multi-Stage Dockerfile with Multi-Architecture Support
// ============================================================================

#[test]
fn test_dockerfile_has_build_stage() {
    let root = workspace_root();
    let dockerfile = fs::read_to_string(root.join("Dockerfile"))
        .expect("Failed to read Dockerfile");
    assert!(
        dockerfile.contains("rust:") && dockerfile.contains("builder"),
        "Must have Rust builder stage"
    );
    assert!(
        dockerfile.contains("AS builder"),
        "Builder stage must be named 'builder'"
    );
}

#[test]
fn test_dockerfile_production_stage() {
    let root = workspace_root();
    let dockerfile = fs::read_to_string(root.join("Dockerfile"))
        .expect("Failed to read Dockerfile");
    assert!(
        dockerfile.contains("FROM debian:bookworm-slim"),
        "Production stage must use debian:bookworm-slim"
    );
    assert!(
        dockerfile.contains("COPY --from=builder"),
        "Must copy from builder stage"
    );
}

#[test]
fn test_dockerfile_security_hardening() {
    let root = workspace_root();
    let dockerfile = fs::read_to_string(root.join("Dockerfile"))
        .expect("Failed to read Dockerfile");
    assert!(
        dockerfile.contains("useradd"),
        "Non-root user must be created"
    );
    assert!(
        dockerfile.contains("USER"),
        "Must switch to non-root user"
    );
    assert!(
        dockerfile.contains("HEALTHCHECK"),
        "Health check must be configured"
    );
}

#[test]
fn test_dockerfile_multi_arch_support() {
    let root = workspace_root();
    let dockerfile = fs::read_to_string(root.join("Dockerfile"))
        .expect("Failed to read Dockerfile");
    assert!(
        dockerfile.contains("ARG TARGETARCH") || dockerfile.contains("TARGETPLATFORM"),
        "Multi-arch build arguments required"
    );
}

#[test]
fn test_dockerfile_hardened_variant_exists() {
    let root = workspace_root();
    assert!(
        root.join("deploy/docker/Dockerfile.hardened").exists(),
        "Hardened Dockerfile variant must exist"
    );
}

#[test]
fn test_dockerfile_hardened_variant_content() {
    let root = workspace_root();
    let hardened = fs::read_to_string(root.join("deploy/docker/Dockerfile.hardened"))
        .expect("Failed to read hardened Dockerfile");
    // Hardened variant should use netcat instead of curl for health check
    assert!(
        hardened.contains("nc -z") || hardened.contains("netcat"),
        "Hardened variant should use netcat for health check"
    );
}

#[test]
fn test_ci_docker_build_workflow_multi_arch() {
    let root = workspace_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/build-docker.yml"))
        .expect("Failed to read build-docker.yml");
    assert!(
        workflow.contains("platforms:"),
        "Must specify platforms for multi-arch builds"
    );
    assert!(
        workflow.contains("linux/amd64"),
        "Must support amd64"
    );
    assert!(
        workflow.contains("linux/arm64"),
        "Must support arm64"
    );
}
