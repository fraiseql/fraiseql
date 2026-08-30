package main

// ── Delivery: the shipped container images ────────────────────────────────────
//
// `docker-build.yml` builds and publishes three images, and until this file it
// was the ONLY thing that built any of them — first running after a `v*` tag
// already existed. That is how the release image stayed unbuildable for weeks
// without a red check (#1205, #1107): there was nothing to be red.
//
// `dagger call image --variant=<name>` builds one of those images on a trigger
// that PRECEDES the tag.
//
// ⚠ What this function claims is exactly "this Dockerfile builds", and nothing
// more. It is deliberately not a coverage claim: the deleted `test-images` job
// built three images and asserted `docker image inspect`, i.e. that the artifact
// exists, which is why #1206 deleted it rather than repairing it. Making a built
// image MEAN something — boot it, ask it a question only a working engine can
// answer — is the next phase's job. Do not let this one grow into the answer.

import (
	"context"
	"fmt"
	"strings"

	"dagger/fraiseql-ci/internal/dagger"
)

// imageVariant is one row of `docker-build.yml`'s build matrix.
//
// This table is a second copy of that matrix, and a second copy is exactly how
// #1135 drifted, so it is not trusted: `tools/check-image-parity.py` holds the
// two to each other **bidirectionally** — a variant published there and missing
// here fails, and so does one here that is published nowhere. It also holds
// `docker-build.yml`'s two matrices (ghcr and Docker Hub) to each other, since a
// variant added to one alone ships to one registry and not the other.
//
// Keep the literal shape below simple. The parity gate reads it as source text
// (the ShellGates container has python3 and no Go toolchain), and its own red
// capability is pinned by tools/tests/image_parity_test.sh.
type imageVariant struct {
	name         string
	dockerfile   string
	buildContext string
	buildArgs    string
	// optional mirrors the matrix's `optional:`/`continue-on-error`. See Images.
	optional bool
}

var imageVariants = []imageVariant{
	{name: "fraiseql-server", dockerfile: "Dockerfile", buildContext: ".", buildArgs: "", optional: false},
	{name: "fraiseql-server-full", dockerfile: "Dockerfile", buildContext: ".", buildArgs: "CARGO_FEATURES=rest,arrow", optional: true},
	{name: "tutorial", dockerfile: "tutorial/Dockerfile", buildContext: ".", buildArgs: "", optional: true},
}

func variantNames() []string {
	names := make([]string, 0, len(imageVariants))
	for _, v := range imageVariants {
		names = append(names, v.name)
	}
	return names
}

func lookupVariant(name string) (imageVariant, error) {
	for _, v := range imageVariants {
		if v.name == name {
			return v, nil
		}
	}
	return imageVariant{}, fmt.Errorf(
		"unknown image variant %q; docker-build.yml publishes: %s",
		name, strings.Join(variantNames(), ", "))
}

// Images builds every variant in the table, in order, and is what the CI leg
// calls. One function rather than a workflow-level matrix on purpose: a matrix
// there would be a FOURTH copy of the variant list, in the one file the parity
// gate cannot hold to the others without parsing its own leg.
//
// ⚠ Every variant is REQUIRED here, including the two the workflow marks
// `optional`. That is a deliberate divergence, not an oversight. `optional` in
// docker-build.yml exists so a broken best-effort image does not block the
// publish of a working one at tag time; before the tag there is no publish to
// protect, and the entire purpose of this leg is to learn early. A leg that
// tolerates a failing build is the shape that let an unreachable job pass for
// coverage for three months (#1206).
//
// The `optional` field is still carried in the table and still compared against
// the matrix by tools/check-image-parity.py, so flipping it in the workflow
// forces an acknowledgement here rather than drifting silently.
//
// Measured 2026-08-26 on fraiseql-8core: fraiseql-server 245s, fraiseql-server-full
// 260s, tutorial 12s; 8m00s for all three end to end. A re-run with a byte-identical
// context is 2s, but that is not the case a push produces: Dagger keys the build on
// the whole context digest, so ANY change to a non-ignored file — a doc, a Makefile —
// rebuilds every layer. Touching docs/architecture/overview.md, which no COPY in the
// Dockerfile names, cost a full 236s. Budget the full number per push, not the 2s.
func (m *FraiseqlCi) Images(
	ctx context.Context,
	// The context is the five paths the Dockerfile COPYs, and nothing else.
	// Dagger keys DockerBuild on the whole context digest, so a wider list made
	// every docs-only push rebuild every layer — a measured 236s for one edit to
	// a file no COPY names (#1215).
	//
	// This list is a second copy of what the build needs, and
	// tools/check-image-context.sh is what keeps it honest: add a COPY for a path
	// dropped here and the gate fails, rather than the build silently running
	// against a context missing it.
	// +ignore=["**", "!Dockerfile", "!Cargo.toml", "!Cargo.lock", "!crates/**", "!deploy/**", "!examples/**", "!tutorial/**", "target", "**/target", ".git"]
	source *dagger.Directory,
) (string, error) {
	var report strings.Builder
	built := 0

	for _, v := range imageVariants {
		out, err := m.Image(ctx, source, v.name)
		fmt.Fprintf(&report, "\n===== %s =====\n%s", v.name, out)
		if err != nil {
			fmt.Fprintf(&report, "FAILED: %v\n", err)
			return report.String(), fmt.Errorf(
				"image variant %q failed to build (%d of %d built); "+
					"docker-build.yml marks it optional=%t, but this leg requires every "+
					"published variant to build before the tag",
				v.name, built, len(imageVariants), v.optional)
		}
		built++
	}

	fmt.Fprintf(&report, "\nimages OK: %d of %d published variant(s) built (%s)\n",
		built, len(imageVariants), strings.Join(variantNames(), ", "))
	return report.String(), nil
}

// buildVariant builds one variant and returns the built container.
//
// Every variant's build-context is the repository root, matching the matrix, so
// `source` is the context for all three and `tutorial/Dockerfile` is addressed by
// path within it.
//
// The build is forced with Sync: a Dagger pipeline is lazy, and returning an
// unevaluated container would produce a function that always "succeeds" without
// building anything — a gate that cannot fail, which is the shape this whole
// program exists to remove.
//
// Shared with image_boot.go so the tier that BOOTS an image boots the same
// artifact this file builds, from one construction site. Two call sites building
// "the same" image from two copies of the build-args logic is how a gate ends up
// asserting against something the publish path never produces.
func buildVariant(ctx context.Context, source *dagger.Directory, v imageVariant) (*dagger.Container, error) {
	opts := dagger.DirectoryDockerBuildOpts{Dockerfile: v.dockerfile}
	if v.buildArgs != "" {
		name, value, ok := strings.Cut(v.buildArgs, "=")
		if !ok {
			return nil, fmt.Errorf("variant %q: malformed build-args %q, want NAME=VALUE", v.name, v.buildArgs)
		}
		opts.BuildArgs = []dagger.BuildArg{{Name: name, Value: value}}
	}

	built, err := source.DockerBuild(opts).Sync(ctx)
	if err != nil {
		return nil, fmt.Errorf("variant %q (%s) failed to build: %w", v.name, v.dockerfile, err)
	}
	return built, nil
}

// Image builds one published image variant from `source`.
func (m *FraiseqlCi) Image(
	ctx context.Context,
	// The context is the five paths the Dockerfile COPYs, and nothing else.
	// Dagger keys DockerBuild on the whole context digest, so a wider list made
	// every docs-only push rebuild every layer — a measured 236s for one edit to
	// a file no COPY names (#1215).
	//
	// This list is a second copy of what the build needs, and
	// tools/check-image-context.sh is what keeps it honest: add a COPY for a path
	// dropped here and the gate fails, rather than the build silently running
	// against a context missing it.
	// +ignore=["**", "!Dockerfile", "!Cargo.toml", "!Cargo.lock", "!crates/**", "!deploy/**", "!examples/**", "!tutorial/**", "target", "**/target", ".git"]
	source *dagger.Directory,
	// The variant to build: one of the names in docker-build.yml's matrix.
	variant string,
) (string, error) {
	v, err := lookupVariant(variant)
	if err != nil {
		return "", err
	}

	built, err := buildVariant(ctx, source, v)
	if err != nil {
		return "", err
	}

	// Read the entrypoint back out of the built image. This is NOT an assertion
	// that the image works — see the header — it is here so the report names what
	// was produced rather than only that something was.
	cmd, err := built.DefaultArgs(ctx)
	if err != nil {
		return "", fmt.Errorf("variant %q: built, but its config could not be read: %w", v.name, err)
	}

	var report strings.Builder
	fmt.Fprintf(&report, "built %s\n", v.name)
	fmt.Fprintf(&report, "  dockerfile: %s\n", v.dockerfile)
	fmt.Fprintf(&report, "  context:    %s\n", v.buildContext)
	if v.buildArgs != "" {
		fmt.Fprintf(&report, "  build-args: %s\n", v.buildArgs)
	}
	fmt.Fprintf(&report, "  cmd:        %s\n", strings.Join(cmd, " "))
	return report.String(), nil
}

// ImageTarball builds one variant and returns it as a docker-format image
// archive, for a consumer that needs the artifact OUTSIDE the Dagger engine.
//
// It exists because of a measured limit, not a preference. Phase 05 deploys the
// chart into a real Kubernetes cluster, and a kubelet cannot run inside a Dagger
// exec on this engine: the exec's cgroup has an EMPTY `cgroup.controllers`,
// because the engine container's own cgroup root delegates nothing through
// `cgroup.subtree_control`. k3s exits at startup with "failed to find cpu cgroup
// (v2)" and there is no in-container fix — delegation has to come from the
// parent. So the cluster runs under host docker (which does get delegation) and
// this function is how it gets the artifact.
//
// ⚠ The point is that `tools/chart-deploy-test.sh` deploys THE IMAGE THIS
// REPOSITORY BUILDS, from `buildVariant` — the same construction site as
// `images`, `image-boots` and `image-properties`. A `docker build` in that shell
// script would have been shorter and would have been a FOURTH copy of the build
// arguments, which is the drift `tools/check-image-parity.py` exists to prevent.
//
// DockerMediaTypes rather than the OCI default: the consumer is `docker load`.
func (m *FraiseqlCi) ImageTarball(
	ctx context.Context,
	// The context is the five paths the Dockerfile COPYs, and nothing else.
	// Dagger keys DockerBuild on the whole context digest, so a wider list made
	// every docs-only push rebuild every layer — a measured 236s for one edit to
	// a file no COPY names (#1215).
	//
	// This list is a second copy of what the build needs, and
	// tools/check-image-context.sh is what keeps it honest: add a COPY for a path
	// dropped here and the gate fails, rather than the build silently running
	// against a context missing it.
	// +ignore=["**", "!Dockerfile", "!Cargo.toml", "!Cargo.lock", "!crates/**", "!deploy/**", "!examples/**", "!tutorial/**", "target", "**/target", ".git"]
	source *dagger.Directory,
	// The variant to export: one of the names in docker-build.yml's matrix.
	// +optional
	// +default="fraiseql-server"
	variant string,
) (*dagger.File, error) {
	v, err := lookupVariant(variant)
	if err != nil {
		return nil, err
	}
	built, err := buildVariant(ctx, source, v)
	if err != nil {
		return nil, err
	}
	return built.AsTarball(dagger.ContainerAsTarballOpts{
		MediaTypes: dagger.ImageMediaTypesDockerMediaTypes,
	}), nil
}

