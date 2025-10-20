# FraiseQL Version Status & Roadmap

**Last Updated**: October 17, 2025
**Current Stable**: v0.11.5

---

## 📊 Version Overview

| Version | Location | Status | Stability | Purpose | For Users? |
|---------|----------|--------|-----------|---------|------------|
| **v0.11.5** | Root level | ✅ Production | Stable | Current framework | ✅ **Recommended** |
| **v1.0** | [`fraiseql/`](./fraiseql/) | 🚧 Week 1/15 | Experimental | Industrial framework | ❌ Not ready |
| **Rust Pipeline** | [`fraiseql_rs/`](./fraiseql_rs/) | ✅ Stable | Required | Core performance engine | ✅ Required |
| **v1 Prototype** | [`fraiseql-v1/`](./fraiseql-v1/) | 🚧 Week 1/8 | Experimental | Architecture exploration | ❌ Development |

---

## 🎯 Which Version Should I Use?

### **For Production Applications** → Use **v0.11.5** (Root Level)
```bash
# Recommended for all production use
pip install fraiseql
```

**Why v0.11.5?**
- ✅ **Production stable** with 50,000+ LOC
- ✅ **Battle-tested** in real applications
- ✅ **Complete feature set** (APQ, caching, monitoring)
- ✅ **Active maintenance** and bug fixes
- ✅ **Migration path** to future versions

### **For Learning/Experimentation** → Try Examples
```bash
# Explore patterns without commitment
cd examples/
ls -la  # See 20+ working examples
```

### **For Contributors** → Start with v0.11.5
- Fix bugs, add features, improve docs
- See [CONTRIBUTING.md](./CONTRIBUTING.md)

### **For Next-Gen Architecture** → v1.0 Development (fraiseql/)
- Industrial framework with Rust pipeline
- Clean architecture for enterprise scale
- Currently in active development

---

## 📈 Version Stability Definitions

### **Production Stable** 🟢
- ✅ Zero breaking changes in minor versions
- ✅ Security patches and critical bug fixes
- ✅ New features in minor versions only
- ✅ Long-term support (18+ months)

### **Maintenance Mode** 🟡
- ✅ Critical security fixes only
- ✅ No new features
- ✅ Migration guides provided
- ⚠️ Limited support timeframe

### **Experimental** 🔴
- ⚠️ Breaking changes without notice
- ⚠️ No stability guarantees
- ⚠️ Not recommended for production
- ✅ Rapid iteration and exploration

### **Showcase/Portfolio** 🎭
- 📚 Interview-quality code examples
- 📚 Demonstrates architectural patterns
- ❌ Not intended for production use
- ✅ Learning and demonstration value

---

## 🗺️ Version Roadmap

### **v0.11.5** (Current Stable)
**Status**: Production stable, actively maintained
**Timeline**: Ongoing until v1.0 release
**Support**: Full support + new features

**Planned Features**:
- Performance optimizations
- Additional caching strategies
- Enhanced monitoring
- New example applications

### **v1.0** (fraiseql/)
**Status**: Week 1 of 15 (Experimental)
**Timeline**: February 2026 (v1.0 release)
**Purpose**: Industrial framework with Rust-first architecture

**15-Week Development Phases**:
- Weeks 1-2: Documentation & architecture foundation
- Weeks 3-4: Core type system & decorators
- Weeks 5-6: CQRS repositories & command/query separation
- Weeks 7-8: GraphQL API layer
- Weeks 9-10: Rust pipeline integration & performance
- Weeks 11-12: Enterprise features (RLS, monitoring, migrations)
- Weeks 13-15: Production hardening & release preparation

**Migration**: Comprehensive migration guides from v0.11.5 will be provided

### **Rust Core** (fraiseql_rs/)
**Status**: Stable, required dependency
**Timeline**: Ongoing maintenance
**Purpose**: Performance-critical JSON transformation

### **Portfolio v1** (fraiseql-v1/)
**Status**: Week 1 of 8 (Showcase)
**Timeline**: December 2025 completion
**Purpose**: Interview demonstration project

---

## 🔄 Migration Policy

### **Breaking Changes**
- **Major versions** (v1.0, v2.0): Breaking changes allowed
- **Minor versions** (v0.12, v0.13): No breaking changes
- **Patch versions** (v0.11.6): No breaking changes

### **Deprecation Timeline**
1. **Announcement**: Feature marked deprecated in release notes
2. **Grace Period**: 2 minor versions for removal
3. **Removal**: Breaking change in next major version

### **Support Timeline**
- **Current stable**: Full support + new features
- **Previous stable**: Security fixes only (6 months)
- **Legacy versions**: No support

---

## 🚨 Version Warnings

### **Don't Use Experimental Versions in Production**
- **v1 Rebuild**: Actively developed, breaking changes daily
- **Portfolio v1**: Demonstration code, not production-ready

### **Rust Core is Required**
- All versions depend on `fraiseql_rs` for performance
- Install automatically via `pip install fraiseql`

### **Version Confusion**
- Multiple directories exist for different purposes
- Always check this document for current recommendations
- Use root-level `README.md` for stable version info

---

## 📞 Getting Help

### **For Current Stable (v0.11.5)**
- [Installation Guide](./INSTALLATION.md)
- [Quickstart](./docs/quickstart.md)
- [Examples](./examples/)

### **For Version Questions**
- Check this `VERSION_STATUS.md` first
- See [Project Structure](./PROJECT_STRUCTURE.md) for directory purposes
- Open issue if status unclear

### **For Migration Planning**
- No migration needed currently (v0.11.5 is stable)
- Watch this file for v1.0 migration guides

---

## 🔍 Version History

### **v0.11.5** (October 2025)
- ✅ Production stable release
- ✅ Complete feature set
- ✅ Performance optimizations
- ✅ Enterprise monitoring

### **v0.11.0-v0.11.4** (2025)
- 🚀 Performance improvements
- 🐛 Bug fixes
- 📚 Documentation updates

### **v0.10.x** (2024-2025)
- 🏗️ Architecture stabilization
- ⚡ Rust integration
- 📊 Monitoring features

---

*This document is updated with each release. Last updated: October 17, 2025*</content>
</xai:function_call name="read">
<parameter name="filePath">README.md
