# FraiseQL Scripts Directory

This directory contains automation scripts, utilities, and tools for the FraiseQL development lifecycle. Scripts are organized by purpose to facilitate discovery and maintenance.

## 📂 Directory Organization

### 🛠️ Development (`development/`)
**Purpose**: Daily development workflow support
**Usage**: Local development environment setup and maintenance

```
development/
├── typecheck.sh                    # Run type checking with pyright
├── start-postgres-daemon.sh        # Start local PostgreSQL for development
├── test-db-setup.sh               # Set up test database
└── claude_mcp_server.py           # Claude MCP server for AI-assisted development
```

**When to use**:
- Setting up development environment
- Running type checks before commits
- Starting local services for development
- AI-assisted development workflows

### 🧪 Testing (`testing/`)
**Purpose**: Test automation, optimization, and maintenance
**Usage**: Test suite management and execution

```
testing/
├── add_test_markers.py             # Add pytest markers to test files
├── clean_broken_tests.py           # Remove or fix broken test files
├── migrate_tests_to_real_db.py     # Convert mock tests to real database tests
├── run-tests-in-container.sh       # Execute tests in Docker container
├── test-all-in-one-entrypoint.sh   # Comprehensive test runner
└── test-native-auth.py             # Test native authentication flows
```

**When to use**:
- Maintaining test suite quality
- Converting test patterns
- Running tests in isolated environments
- Debugging test failures

### 🚀 Deployment (`deployment/`)
**Purpose**: Build, package, and publish operations
**Usage**: Preparing and releasing software packages

```
deployment/
├── publish.sh                      # Publish package to PyPI
└── test-build.sh                   # Test package build process
```

**When to use**:
- Publishing new releases
- Validating build processes
- Package distribution

### ⚙️ Maintenance (`maintenance/`)
**Purpose**: Repository maintenance and administration
**Usage**: Repository health and structure management

```
maintenance/
├── create-testing-branch.sh        # Create testing branches with proper setup
└── setup-branch-protection.sh      # Configure GitHub branch protection rules
```

**When to use**:
- Setting up new development branches
- Configuring repository security
- Maintaining repository structure

### 🔄 CI/CD (`ci-cd/`)
**Purpose**: Continuous integration and release automation
**Usage**: Automated release processes and validation

```
ci-cd/
├── RELEASE_COMMANDS.sh             # Release command documentation
├── test-release-0.3.5.sh          # Version-specific release testing
└── verify-release-0.3.5.sh        # Release verification procedures
```

**When to use**:
- Preparing software releases
- Validating release candidates
- Automating release workflows

### ✅ Verification (`verification/`)
**Purpose**: Bug reproduction, verification, and validation
**Usage**: Specific issue validation and network testing

```
verification/
├── fraiseql_v055_network_issues_test_cases.py  # Network bug test cases
└── verify_network_fix.py                       # Network fix verification
```

**When to use**:
- Reproducing reported bugs
- Validating bug fixes
- Network-specific testing

## 🚀 Common Workflows

### Development Setup
```bash
# Set up development environment
./scripts/development/start-postgres-daemon.sh
./scripts/development/test-db-setup.sh

# Run type checks
./scripts/development/typecheck.sh
```

### Testing Workflows
```bash
# Run comprehensive test suite
./scripts/testing/test-all-in-one-entrypoint.sh

# Test in isolated container
./scripts/testing/run-tests-in-container.sh

# Clean up problematic tests
python scripts/testing/clean_broken_tests.py
```

### Release Preparation
```bash
# Validate build
./scripts/deployment/test-build.sh

# Run release-specific tests
./scripts/ci-cd/test-release-0.3.5.sh

# Publish release
./scripts/deployment/publish.sh
```

### Repository Maintenance
```bash
# Set up new testing branch
./scripts/maintenance/create-testing-branch.sh feature-name

# Configure branch protection
./scripts/maintenance/setup-branch-protection.sh
```

## 🔧 Script Development Guidelines

### Script Organization Principles
1. **Purpose-based grouping**: Scripts grouped by primary function
2. **Clear naming**: Script names indicate functionality
3. **Executable permissions**: Shell scripts marked executable
4. **Documentation**: Each script includes usage comments
5. **Error handling**: Scripts fail fast with meaningful messages

### Adding New Scripts
1. **Identify category**: Use the purpose-based directory structure
2. **Follow naming conventions**: Use clear, descriptive names
3. **Add documentation**: Include usage instructions and examples
4. **Set permissions**: Make shell scripts executable (`chmod +x`)
5. **Test thoroughly**: Validate script works in clean environment

### Script Naming Conventions
```bash
# Good script names
setup-development-environment.sh    # Clear purpose
test-authentication-flows.py       # Specific functionality
verify-database-migrations.sh      # Action and target clear

# Avoid
script.sh                          # Too generic
test.py                           # Ambiguous
util.sh                           # Unclear purpose
```

### Cross-Platform Compatibility
- **Shell scripts**: Use `#!/bin/bash` for Linux/macOS compatibility
- **Python scripts**: Use `#!/usr/bin/env python3` shebang
- **Path handling**: Use relative paths from repository root
- **Dependencies**: Document required tools and versions

## 🛡️ Security Considerations

### Safe Script Practices
1. **Input validation**: Validate all user inputs
2. **Path safety**: Avoid path traversal vulnerabilities
3. **Credential handling**: Never hardcode credentials
4. **Temporary files**: Clean up temporary files
5. **Permission checks**: Verify required permissions

### Environment Variable Usage
```bash
# Good: Use environment variables for configuration
DATABASE_URL=${DATABASE_URL:-"postgresql://localhost:5432/fraiseql_test"}

# Good: Validate required variables
if [ -z "$REQUIRED_VAR" ]; then
    echo "Error: REQUIRED_VAR must be set"
    exit 1
fi
```

## 📋 Script Dependencies

### Common Requirements
- **Python 3.8+**: For Python scripts
- **Bash 4.0+**: For shell scripts
- **PostgreSQL**: For database-related scripts
- **Docker**: For containerized testing
- **Git**: For version control operations

### Development Dependencies
See `pyproject.toml` for complete Python dependencies.

### System Dependencies
- `curl` - For HTTP operations
- `jq` - For JSON processing
- `grep`, `sed`, `awk` - For text processing

## 🧹 Maintenance

### Regular Maintenance Tasks
1. **Review unused scripts**: Remove obsolete automation
2. **Update documentation**: Keep README current
3. **Validate dependencies**: Ensure scripts work with current tools
4. **Performance optimization**: Improve slow-running scripts
5. **Security updates**: Review and update security practices

### Script Lifecycle Management
- **Creation**: Follow organization and naming guidelines
- **Evolution**: Update documentation when functionality changes
- **Deprecation**: Mark deprecated scripts clearly
- **Removal**: Archive or remove unused scripts

### Version-Specific Scripts
Scripts tied to specific versions (like release testing) should:
- Include version in filename
- Document version requirements
- Be archived after version EOL

## 🔍 Troubleshooting

### Common Issues
| Problem | Likely Cause | Solution |
|---------|--------------|----------|
| Permission denied | Script not executable | `chmod +x script.sh` |
| Command not found | Missing dependencies | Check system requirements |
| Database connection fails | PostgreSQL not running | Start PostgreSQL service |
| Path not found | Wrong working directory | Run from repository root |

### Getting Help
1. **Script documentation**: Check script header comments
2. **Usage patterns**: See workflow examples above
3. **Dependencies**: Verify all requirements installed
4. **Logs**: Check script output for error details

---

## 🎯 Quick Reference

**Need to set up development?** → `scripts/development/`
**Running tests?** → `scripts/testing/`
**Preparing release?** → `scripts/deployment/` + `scripts/ci-cd/`
**Repository maintenance?** → `scripts/maintenance/`
**Bug verification?** → `scripts/verification/`

---

*This scripts organization evolves with FraiseQL development needs. When adding new scripts, prioritize developer productivity and maintainability.*
