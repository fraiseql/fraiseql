package main

// ── Release Pre-Tag Validation (Phase 06, minimal scope) ──────────────────────
//
// Local pre-tag validation for the crates.io release, runnable the same way
// locally (`dagger call publish-dry-run`) and — optionally, later — on the runner.
//
// The actual publish + image build/push stay GitHub-native (release.yml /
// docker-build.yml, both still live on `v*` tags): releases are infrequent, so
// their hosted-CI cost is negligible and re-implementing a working crates.io
// publisher is not worth the risk. What this module adds is the ability to catch,
// BEFORE a tag goes out, the two failure classes a release run would otherwise
// discover the expensive way:
//   - "tag shipped but a crate failed to publish" (the v2.3.0 / v2.3.1 regression)
//     → PublishDryRun walks the same topological order as release.yml.
//   - an accidental breaking public-API change → SemverNamed / SemverWorkspace.
//
// See .phases/dagger-adoption/parity-notes.md for the ported-vs-GitHub-native split.

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// legacyPublishOrder is the topological crate publish order proven by release.yml
// (it shipped all 16 publishable crates at v2.3.2). It is the canonical order the
// dry-run loop iterates; PublishOrderSelftest validates it against the live
// dependency graph, so a newly-added crate or a new cross-crate edge that would
// break publish order is caught here rather than mid-release.
var legacyPublishOrder = []string{
	// Tier 1: leaf crates (depend on none of the others).
	"fraiseql-error", "fraiseql-auth", "fraiseql-webhooks", "fraiseql-wire",
	// Tier 2: depend on tier 1.
	"fraiseql-db", "fraiseql-storage",
	// Tier 3.
	"fraiseql-federation",
	// Tier 4.
	"fraiseql-core",
	// Tier 4.5.
	"fraiseql-codegen",
	// Tier 5.
	"fraiseql-arrow", "fraiseql-secrets", "fraiseql-observers", "fraiseql-functions",
	// Tier 6: top-level binaries + umbrella.
	"fraiseql-server", "fraiseql-cli", "fraiseql",
}

// PublishOrder returns the canonical crates.io publish order, one crate per line —
// the order PublishDryRun (and the legacy release.yml) publish in.
func (m *FraiseqlCi) PublishOrder() string {
	return strings.Join(legacyPublishOrder, "\n")
}

// cargoMetadata is the subset of `cargo metadata --no-deps` output the selftest reads.
type cargoMetadata struct {
	Packages []cargoPackage `json:"packages"`
}

type cargoPackage struct {
	Name string `json:"name"`
	// Publish: null/absent → publishable; [] → publish=false; ["registry"] → restricted.
	Publish      *[]string         `json:"publish"`
	Dependencies []cargoDependency `json:"dependencies"`
}

type cargoDependency struct {
	Name string `json:"name"`
	// Kind: null → normal, "dev", or "build".
	Kind *string `json:"kind"`
}

func (p cargoPackage) publishable() bool {
	return p.Publish == nil || len(*p.Publish) > 0
}

// PublishOrderSelftest validates legacyPublishOrder against the live workspace
// dependency graph (`cargo metadata --no-deps`):
//
//  1. Set equality — the publishable crates the workspace actually has match the
//     embedded order exactly. A newly-added publishable crate, or a removed one,
//     fails here with the diff.
//  2. Topological validity — every crate is listed after all of its in-workspace
//     (non-dev) dependencies. A new cross-crate edge that violates publish order
//     fails here.
//
// This is the Phase-06 CLEANUP gate ("publish order matches release.yml"): the
// embedded order stays canonical (it is what release.yml proved), and this guards
// it against drift.
func (m *FraiseqlCi) PublishOrderSelftest(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	out, err := m.rustBase().
		WithMountedDirectory("/src", source).
		WithWorkdir("/src").
		WithExec([]string{"cargo", "metadata", "--no-deps", "--format-version", "1"}).
		Stdout(ctx)
	if err != nil {
		return "", fmt.Errorf("cargo metadata failed: %w", err)
	}

	var meta cargoMetadata
	if err := json.Unmarshal([]byte(out), &meta); err != nil {
		return "", fmt.Errorf("parsing cargo metadata JSON: %w", err)
	}

	pos := make(map[string]int, len(legacyPublishOrder))
	for i, name := range legacyPublishOrder {
		pos[name] = i
	}

	// (1) Set equality: live publishable crates vs the embedded order.
	livePublishable := make(map[string]bool)
	for _, pkg := range meta.Packages {
		if pkg.publishable() {
			livePublishable[pkg.Name] = true
		}
	}
	var missing, extra []string
	for name := range livePublishable {
		if _, ok := pos[name]; !ok {
			missing = append(missing, name)
		}
	}
	for _, name := range legacyPublishOrder {
		if !livePublishable[name] {
			extra = append(extra, name)
		}
	}
	sort.Strings(missing)
	sort.Strings(extra)
	if len(missing) > 0 || len(extra) > 0 {
		return "", fmt.Errorf(
			"publish-order drift vs `cargo metadata`:\n"+
				"  missing from legacyPublishOrder (add them in dependency order): %v\n"+
				"  listed but no longer a publishable workspace crate (remove them): %v",
			missing, extra)
	}

	// (2) Topological validity: each crate after its in-workspace non-dev deps.
	var violations []string
	for _, pkg := range meta.Packages {
		if !pkg.publishable() {
			continue
		}
		for _, dep := range pkg.Dependencies {
			if dep.Kind != nil && *dep.Kind == "dev" {
				continue // dev-deps do not constrain publish order
			}
			if !livePublishable[dep.Name] {
				continue // external or non-publishable dependency
			}
			if pos[dep.Name] >= pos[pkg.Name] {
				violations = append(violations, fmt.Sprintf(
					"%s is ordered before its dependency %s", pkg.Name, dep.Name))
			}
		}
	}
	sort.Strings(violations)
	if len(violations) > 0 {
		return "", fmt.Errorf("publish order is not topologically valid:\n  %s",
			strings.Join(violations, "\n  "))
	}

	return fmt.Sprintf("publish-order OK: %d publishable crates, valid topological order\n  %s\n",
		len(legacyPublishOrder), strings.Join(legacyPublishOrder, " → ")), nil
}
