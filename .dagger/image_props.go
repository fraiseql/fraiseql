package main

// ── Delivery: the shipped image IS what it declares ───────────────────────────
//
// image.go builds the published images, and claims exactly "this Dockerfile
// builds". image_boot.go boots one against a real Postgres and requires an
// answer only a working engine can give — "the artifact WORKS". This file is the
// third question: what the artifact IS.
//
// What it links, who it runs as, which version it carries, whether its own
// HEALTHCHECK can pass, and how large it is. Every one of those was verified BY
// HAND during the v2.15.0 release-readiness pass and written into an issue —
// #1133 for the linkage, #1129 for the version label. A hand verification is a
// photograph of one moment: #1133's property ("the binary's only dynamic
// dependencies are libc, libm and libgcc_s") stops being true the first time an
// `apt-get install` returns, and nothing in eleven CI legs would say so. This
// file turns those photographs into assertions on the built artifact.
//
// ⚠ Deliberately NOT a re-reading of the Dockerfile. tools/check-deploy-versions.sh
// already greps the Dockerfile TEXT for its OCI version label, and that gate is
// the floor: it fires before anything is built, cheaply, in ShellGates. What it
// cannot tell you is whether the text produced the artifact it describes. So
// every assertion below reads the BUILT IMAGE — its config, its filesystem, its
// binary, its healthcheck — and none of them parses a Dockerfile.
//
// ⚠ Why every function here takes a `--run-id`, and why that is not copied
// reflex from image_boot.go. Dagger caches a module function call on its
// ARGUMENTS: called twice on a byte-identical context, `dagger call image-boot`
// returned in 2.1s replaying the previous run's stdout, having started no
// container (measured 2026-08-27, Phase 03). Most of what this file asserts is a
// pure function of the built image — linkage, packages, uid, labels, size — and
// replaying those would be as sound as replaying `dagger call images`. But ONE
// assertion here is a claim about an execution: the HEALTHCHECK is started,
// polled, and then required to fail once the server it checks is killed. A
// replayed execution claim is exactly the class of green this program was written
// to delete, so the whole function is argument-varied rather than split into a
// replayable half and a non-replayable half. The pure reads ride along; they cost
// seconds once the images are in the engine cache, and a second entry point would
// double the surface a future variant has to be added to.
//
// The run id is normalised by sanitizeRunID and defaults to imageBootRunIDDefault
// ("local"), shared with image_boot.go rather than duplicated: two constants with
// the same value are two things to keep in step.

import (
	"context"
	"fmt"
	"path"
	"regexp"
	"sort"
	"strings"
	"time"

	"dagger/fraiseql-ci/internal/dagger"
)

const (
	// imagePropsExpectedUID — the uid `useradd -r -u 65532` creates and `USER
	// fraiseql` selects in the runtime stage.
	//
	// Hardcoded rather than parsed out of the Dockerfile on purpose: a number
	// read from the file under test agrees with itself no matter what the built
	// image does, which is the shape of a gate that cannot fail. If the uid
	// legitimately changes, this constant changes with it — an acknowledgement.
	imagePropsExpectedUID = "65532"

	// imagePropsSizeTolerance — the fraction either side of a variant's stated
	// budget that does not fail.
	//
	// Two-sided, and 15% is wide for a reason: the runtime stage runs
	// `apt-get update && apt-get upgrade -y`, so the base layer's size moves on
	// its own whenever Debian publishes a security update, with no commit here.
	// A one-sided cap would be the more usual choice; it also accepts an image
	// that has LOST something, and "the artifact suddenly got much smaller" is a
	// thing this project would want to be told about (a stripped or missing
	// binary is the same shape as #1205's "the artifact does not exist"). The
	// budget is a tripwire that demands an acknowledgement, not a limit.
	imagePropsSizeTolerance = 0.15

	// imagePropsMarker delimits the sections of the measurement scripts below.
	// Deliberately unlikely to occur in `ldd` output, a package list, or a
	// server log.
	imagePropsMarker = "#--fraiseql-image-props--#"
)

// imagePropsAllowedSonames is #1133's property as a set: the complete list of
// shared libraries the shipped binary is allowed to depend on.
//
// The PostgreSQL driver is the pure-Rust tokio-postgres + rustls stack — pq-sys,
// libpq-sys and diesel appear nowhere in Cargo.lock — so nothing in the default
// feature set links a system library, and the Dockerfile installs no build
// dependency for one. Measured on the built image 2026-08-27: linux-vdso.so.1,
// libgcc_s.so.1, libm.so.6, libc.so.6, /lib64/ld-linux-x86-64.so.2.
//
// The loader and the vdso are not in this list; they are recognised structurally
// below (an absolute path, and the vdso's fixed name) because their names are
// architecture-dependent and carry no information about what the build linked.
var imagePropsAllowedSonames = map[string]bool{
	"libc.so.6":     true,
	"libm.so.6":     true,
	"libgcc_s.so.1": true,
}

// imagePropsRequiredSonames is what must be PRESENT, and it is deliberately much
// shorter than the allowed set.
//
// A gate that parses nothing finds no forbidden entry and passes: if `ldd` were
// missing from a future base image, or its output format changed, an
// extras-only check would report a clean bill of health for zero measured
// dependencies. Requiring libc closes that.
//
// It is NOT the whole allowed set. glibc 2.34 merged libm into libc and left
// libm.so.6 as a stub, so a base-image bump can legitimately drop libm from a
// binary's DT_NEEDED. Failing on its absence would be a gate firing on a
// non-defect. The property #1133 protects is that nothing NEW appears, and that
// is the extras check.
var imagePropsRequiredSonames = []string{"libc.so.6"}

// imagePropsSizeBudgets is the stated number, per variant, in bytes: the size of
// the image's UNCOMPRESSED OCI tarball — the sum of its layers as they land on a
// host after `docker pull`, which is the number `docker images` reports. It is
// not the compressed size on the wire, and it is not `du` of the final rootfs
// (that ignores whiteouts and files replaced in a later layer).
//
// Measured 2026-08-27 on fraiseql-8core; see imagePropsSizeTolerance for why the
// band is wide and two-sided.
//
// A variant with no row here FAILS rather than passing unmeasured: a new
// published image that nobody sized is exactly what this table exists to notice.
var imagePropsSizeBudgets = map[string]int64{
	"fraiseql-server":      116_951_552, // 111.5 MiB, measured 2026-08-27
	"fraiseql-server-full": 122_482_688, // 116.8 MiB, measured 2026-08-27 (rest,arrow)
}

// ImagePropertiesAll asserts every bootable variant's properties and is what the
// CI leg calls.
//
// ⚠ The name pair here is the reverse of `Image`/`Images` and `ImageBoot`/
// `ImageBoots`: "properties" is already plural for a single image, so the
// umbrella takes the `All` suffix rather than an `s`. Same semantics as the other
// two — every published server variant is REQUIRED, including the ones
// docker-build.yml marks `optional`, for the reason Images documents: `optional`
// protects a publish, and before the tag there is no publish to protect.
func (m *FraiseqlCi) ImagePropertiesAll(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// A value that DIFFERS between runs. See the file header for why this
	// function is argument-varied even though most of what it reads is pure.
	// +optional
	// +default="local"
	runID string,
) (string, error) {
	variants := bootableVariants()
	if len(variants) == 0 {
		return "", fmt.Errorf(
			"no server image variant found in the imageVariants table — a tier that " +
				"inspects nothing passes everything")
	}

	var report strings.Builder
	checked := 0
	for _, v := range variants {
		out, err := m.ImageProperties(ctx, source, v.name, runID)
		fmt.Fprintf(&report, "\n===== %s =====\n%s", v.name, out)
		if err != nil {
			fmt.Fprintf(&report, "FAILED: %v\n", err)
			return report.String(), fmt.Errorf(
				"image variant %q does not have the properties it declares (%d of %d "+
					"checked); docker-build.yml marks it optional=%t, but this tier "+
					"requires every published server image to be what it says it is "+
					"before the tag: %w",
				v.name, checked, len(variants), v.optional, err)
		}
		checked++
	}

	names := make([]string, 0, len(variants))
	for _, v := range variants {
		names = append(names, v.name)
	}
	fmt.Fprintf(&report, "\nimage-properties OK: %d of %d server variant(s) are what they declare (%s)\n",
		checked, len(variants), strings.Join(names, ", "))
	return report.String(), nil
}

// ImageProperties asserts one published image variant's properties against the
// image that was built, not against the Dockerfile that describes it.
func (m *FraiseqlCi) ImageProperties(
	ctx context.Context,
	// +ignore=["target", "**/target", ".git"]
	source *dagger.Directory,
	// The variant to inspect: one of the names in docker-build.yml's matrix that
	// is built from the root Dockerfile.
	// +optional
	// +default="fraiseql-server"
	variant string,
	// A value that DIFFERS between runs. See the file header.
	// +optional
	// +default="local"
	runID string,
) (string, error) {
	v, err := lookupVariant(variant)
	if err != nil {
		return "", err
	}
	if v.dockerfile != "Dockerfile" {
		return "", fmt.Errorf(
			"variant %q is built from %s and carries none of the properties below "+
				"(no fraiseql-server binary, no PostgreSQL driver, no /health); this "+
				"tier inspects the server images only (see bootableVariants)",
			v.name, v.dockerfile)
	}

	version, err := workspaceVersion(ctx, source)
	if err != nil {
		return "", err
	}

	built, err := buildVariant(ctx, source, v)
	if err != nil {
		return "", err
	}

	r := &imagePropsReport{}
	r.printf("image-properties %s (run %s)\n", v.name, sanitizeRunID(runID))
	r.printf("workspace version: %s\n", version)

	cfg, err := readImageConfig(ctx, built)
	if err != nil {
		return r.String(), err
	}
	r.printf("\n-- image config --\n")
	r.printf("  user:        %q\n", cfg.user)
	r.printf("  workdir:     %q\n", cfg.workdir)
	r.printf("  entrypoint:  %v\n", cfg.entrypoint)
	r.printf("  cmd:         %v\n", cfg.defaultArgs)
	r.printf("  exposed:     %v\n", cfg.exposedPorts)
	r.printf("  healthcheck: shell=%t args=%v\n", cfg.hcShell, cfg.hcArgs)
	r.printf("               interval=%s timeout=%s retries=%d start-period=%s\n",
		cfg.hcInterval, cfg.hcTimeout, cfg.hcRetries, cfg.hcStartPeriod)

	binary, err := imagePropsBinaryPath(cfg.defaultArgs, cfg.workdir)
	if err != nil {
		return r.String(), err
	}

	r.printf("\n-- what the image declares --\n")
	assertVersionLabel(r, cfg.labels, version)
	assertDeclaredUser(r, cfg.user)
	assertExposedPort(r, cfg.exposedPorts)

	r.printf("\n-- what the artifact is --\n")
	probe, err := built.
		WithExec([]string{"bash", "-c", imagePropsProbeScript(binary)}).
		Stdout(ctx)
	if err != nil {
		return r.String(), fmt.Errorf("variant %q: the built image could not be inspected: %w", v.name, err)
	}
	sections := imagePropsSections(probe)
	assertRuntimeUID(r, sections["UID"], sections["USERNAME"])
	assertLinkage(r, binary, sections["LDD"])
	assertBinaryVersion(r, binary, sections["VERSION"], version)

	// Run the filesystem scan as root rather than as the image's own user. The
	// uid assertion above must run as the image declares — that is its whole
	// point — but a `find` that cannot read a directory prints nothing and looks
	// identical to a `find` that found nothing, so the scan is deliberately given
	// the strictly larger view.
	scan, err := built.
		WithUser("root").
		WithExec([]string{"bash", "-c", imagePropsScanScript()}).
		Stdout(ctx)
	if err != nil {
		return r.String(), fmt.Errorf("variant %q: the built image's filesystem could not be scanned: %w", v.name, err)
	}
	scanSections := imagePropsSections(scan)
	assertNoLibpq(r, scanSections["PKGS"], scanSections["LIBPQFILES"])

	size, err := built.
		AsTarball(dagger.ContainerAsTarballOpts{
			ForcedCompression: dagger.ImageLayerCompressionUncompressed,
		}).
		Size(ctx)
	if err != nil {
		return r.String(), fmt.Errorf("variant %q: the built image could not be sized: %w", v.name, err)
	}
	assertSizeBudget(r, v.name, int64(size))

	r.printf("\n-- what the artifact does --\n")
	hcOut, hcErr := m.runDeclaredHealthcheck(ctx, source, built, cfg)
	r.printf("%s", indent(hcOut))
	if hcErr != nil {
		return r.String(), fmt.Errorf("%s\nvariant %q: %w", r.String(), v.name, hcErr)
	}

	if len(r.failures) > 0 {
		return r.String(), fmt.Errorf(
			"%s\nvariant %q: %d propert%s of the built image contradict%s what it declares:\n  - %s",
			r.String(), v.name, len(r.failures), plural(len(r.failures), "y", "ies"),
			plural(len(r.failures), "s", ""),
			strings.Join(r.failures, "\n  - "))
	}
	r.printf("\nimage-properties OK (%s): the built image links only what #1133 allows, ships no\n", v.name)
	r.printf("libpq, runs as uid %s, carries and reports v%s, fits its size budget, and its own\n", imagePropsExpectedUID, version)
	r.printf("HEALTHCHECK passed while the server was up and failed once it was gone.\n")
	return r.String(), nil
}

// ── the report ────────────────────────────────────────────────────────────────

// imagePropsReport accumulates every failure rather than returning on the first.
//
// The alternative — fail fast — hides the second defect behind the first and
// turns "one at a time" RED proofs into a guessing game about which assertion
// actually fired. tools/check-deploy-versions.sh takes the same shape for the
// same reason.
type imagePropsReport struct {
	sb       strings.Builder
	failures []string
}

// ⚠ A Dagger module function that returns an error has its return VALUE
// discarded. Measured on this tier's first red run: `dagger call` printed one
// sentence of the error and not a single measurement — no linkage, no package
// list, no size, no server log — at exactly the moment they were wanted. A report
// that exists only on the success path is a report nobody ever reads.
//
// Writing to stdout does not fix it either: also measured, module stdout does not
// surface through `dagger call`'s default progress output. So the failure path
// below carries the WHOLE report inside the error message, which does surface.
// The stdout write stays for `--progress=plain` and for anyone attaching to the
// span, but nothing depends on it.
func (r *imagePropsReport) write(s string) {
	r.sb.WriteString(s)
	fmt.Print(s)
}

func (r *imagePropsReport) printf(format string, a ...any) { r.write(fmt.Sprintf(format, a...)) }
func (r *imagePropsReport) String() string                 { return r.sb.String() }

func (r *imagePropsReport) ok(format string, a ...any) {
	r.write(fmt.Sprintf("  ok    %s\n", fmt.Sprintf(format, a...)))
}

func (r *imagePropsReport) fail(format string, a ...any) {
	msg := fmt.Sprintf(format, a...)
	r.failures = append(r.failures, msg)
	r.write(fmt.Sprintf("  FAIL  %s\n", msg))
}

func plural(n int, one, many string) string {
	if n == 1 {
		return one
	}
	return many
}

func indent(s string) string {
	var b strings.Builder
	for _, line := range strings.Split(strings.TrimRight(s, "\n"), "\n") {
		b.WriteString("  ")
		b.WriteString(line)
		b.WriteString("\n")
	}
	return b.String()
}

// ── the workspace version ─────────────────────────────────────────────────────

// workspaceVersionRE is anchored at the start of a line and takes the FIRST
// match, which in Cargo.toml is `[workspace.package]`'s.
//
// ⚠ Unanchored, the first `version =` in the file is a dependency pin — the same
// trap tools/check-deploy-versions.sh documents.
var workspaceVersionRE = regexp.MustCompile(`(?m)^version = "([^"]+)"`)

func workspaceVersion(ctx context.Context, source *dagger.Directory) (string, error) {
	contents, err := source.File("Cargo.toml").Contents(ctx)
	if err != nil {
		return "", fmt.Errorf("could not read the workspace Cargo.toml: %w", err)
	}
	m := workspaceVersionRE.FindStringSubmatch(contents)
	if m == nil {
		return "", fmt.Errorf(
			"Cargo.toml has no line-anchored `version = \"...\"`, so there is no " +
				"workspace version to hold the image to")
	}
	return m[1], nil
}

// ── the image's declared configuration ────────────────────────────────────────

type imageConfig struct {
	labels        map[string]string
	user          string
	workdir       string
	entrypoint    []string
	defaultArgs   []string
	exposedPorts  []int
	hcArgs        []string
	hcShell       bool
	hcInterval    string
	hcTimeout     string
	hcStartPeriod string
	hcRetries     int
}

func readImageConfig(ctx context.Context, built *dagger.Container) (imageConfig, error) {
	var cfg imageConfig

	labels, err := built.Labels(ctx)
	if err != nil {
		return cfg, fmt.Errorf("the built image's labels could not be read: %w", err)
	}
	cfg.labels = make(map[string]string, len(labels))
	for _, l := range labels {
		name, err := l.Name(ctx)
		if err != nil {
			return cfg, fmt.Errorf("a label name could not be read: %w", err)
		}
		value, err := l.Value(ctx)
		if err != nil {
			return cfg, fmt.Errorf("the value of label %q could not be read: %w", name, err)
		}
		cfg.labels[name] = value
	}

	if cfg.user, err = built.User(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's USER could not be read: %w", err)
	}
	if cfg.workdir, err = built.Workdir(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's WORKDIR could not be read: %w", err)
	}
	if cfg.entrypoint, err = built.Entrypoint(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's ENTRYPOINT could not be read: %w", err)
	}
	if cfg.defaultArgs, err = built.DefaultArgs(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's CMD could not be read: %w", err)
	}

	ports, err := built.ExposedPorts(ctx)
	if err != nil {
		return cfg, fmt.Errorf("the built image's EXPOSE could not be read: %w", err)
	}
	for _, p := range ports {
		n, err := p.Port(ctx)
		if err != nil {
			return cfg, fmt.Errorf("an exposed port could not be read: %w", err)
		}
		cfg.exposedPorts = append(cfg.exposedPorts, n)
	}
	sort.Ints(cfg.exposedPorts)

	hc := built.DockerHealthcheck()
	if cfg.hcArgs, err = hc.Args(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's HEALTHCHECK could not be read: %w", err)
	}
	if cfg.hcShell, err = hc.Shell(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's HEALTHCHECK form could not be read: %w", err)
	}
	if cfg.hcInterval, err = hc.Interval(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's HEALTHCHECK interval could not be read: %w", err)
	}
	if cfg.hcTimeout, err = hc.Timeout(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's HEALTHCHECK timeout could not be read: %w", err)
	}
	if cfg.hcStartPeriod, err = hc.StartPeriod(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's HEALTHCHECK start-period could not be read: %w", err)
	}
	if cfg.hcRetries, err = hc.Retries(ctx); err != nil {
		return cfg, fmt.Errorf("the built image's HEALTHCHECK retries could not be read: %w", err)
	}
	return cfg, nil
}

// imagePropsBinaryPath resolves the binary the image's own CMD names.
//
// ⚠ Derived, never re-typed. The point of the version assertion below is that a
// shared CARGO_TARGET_DIR once left the WRONG binary behind a `Finished in
// 0.23s` line; asking a hardcoded `/app/fraiseql-server` for its version while
// the image's CMD names something else would reproduce that hole one layer up.
func imagePropsBinaryPath(defaultArgs []string, workdir string) (string, error) {
	if len(defaultArgs) == 0 {
		return "", fmt.Errorf(
			"the built image declares no CMD, so there is no binary to interrogate; " +
				"a container started from it runs nothing")
	}
	arg0 := defaultArgs[0]
	if strings.HasPrefix(arg0, "/") {
		return path.Clean(arg0), nil
	}
	if workdir == "" {
		workdir = "/"
	}
	return path.Join(workdir, arg0), nil
}

// ── assertions on what the image declares ─────────────────────────────────────

const ociVersionLabel = "org.opencontainers.image.version"

func assertVersionLabel(r *imagePropsReport, labels map[string]string, version string) {
	label, ok := labels[ociVersionLabel]
	switch {
	case !ok:
		r.fail("the built image carries no %s label", ociVersionLabel)
	case label != version:
		r.fail("the built image's %s is %q, the workspace is %q", ociVersionLabel, label, version)
	default:
		r.ok("%s = %s, and the workspace agrees", ociVersionLabel, label)
	}
}

// assertDeclaredUser checks only that a USER is declared at all. Whether the
// declared user is root is the RUNTIME uid assertion's job — a name in the config
// says nothing about the uid behind it — so this deliberately does not judge the
// value, and says so rather than printing a bare "ok" next to USER root.
func assertDeclaredUser(r *imagePropsReport, user string) {
	if strings.TrimSpace(user) == "" {
		r.fail(
			"the built image's config declares no USER, so every runtime that honours " +
				"it — docker run, Kubernetes without a securityContext — starts this " +
				"process as root")
		return
	}
	r.ok("the image config declares USER %q (the uid assertion below judges it)", user)
}

func assertExposedPort(r *imagePropsReport, ports []int) {
	for _, p := range ports {
		if p == imageBootPort {
			r.ok("the image EXPOSEs %d", imageBootPort)
			return
		}
	}
	r.fail(
		"the built image EXPOSEs %v, which does not include %d — the port its own "+
			"HEALTHCHECK and the deployment manifests reach it on",
		ports, imageBootPort)
}

// ── assertions on what the artifact is ────────────────────────────────────────

func assertRuntimeUID(r *imagePropsReport, uid, username string) {
	uid = strings.TrimSpace(uid)
	username = strings.TrimSpace(username)
	switch {
	case uid == "":
		r.fail("the built image did not report a uid for the process it starts")
	case uid == "0":
		r.fail("the process in the built image runs as uid 0 (root)")
	case uid != imagePropsExpectedUID:
		r.fail("the process in the built image runs as uid %s, expected %s", uid, imagePropsExpectedUID)
	default:
		r.ok("the process runs as uid %s (%s), not root", uid, username)
	}
}

// assertLinkage holds the built binary to #1133: libc, libm and libgcc_s, plus
// the loader and the vdso, and nothing else.
func assertLinkage(r *imagePropsReport, binary, ldd string) {
	found := map[string]bool{}
	var extras []string
	var structural []string

	for _, line := range strings.Split(ldd, "\n") {
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		soname := fields[0]
		switch {
		case strings.HasPrefix(soname, "/"):
			// The dynamic loader, named by absolute path and architecture-dependent.
			structural = append(structural, soname)
		case soname == "linux-vdso.so.1", strings.HasPrefix(soname, "linux-vdso"), strings.HasPrefix(soname, "linux-gate"):
			structural = append(structural, soname)
		case !strings.Contains(soname, ".so"):
			// Not a soname line at all — `ldd`'s "not a dynamic executable", an
			// error message, anything else. Reported, and caught by the required
			// check below, which is what makes an unparseable output fail rather
			// than pass with an empty set.
			continue
		default:
			found[soname] = true
			if !imagePropsAllowedSonames[soname] {
				extras = append(extras, soname)
			}
		}
	}

	sort.Strings(extras)
	names := make([]string, 0, len(found))
	for n := range found {
		names = append(names, n)
	}
	sort.Strings(names)

	if len(names) == 0 {
		r.fail(
			"nothing in `ldd %s` parsed as a shared-library dependency, so the "+
				"linkage assertion measured nothing; raw output:\n        %s",
			binary, strings.ReplaceAll(strings.TrimSpace(ldd), "\n", "\n        "))
		return
	}

	for _, required := range imagePropsRequiredSonames {
		if !found[required] {
			r.fail(
				"`ldd %s` does not list %s; the output did not parse as a dynamic "+
					"executable's dependencies (found: %s)",
				binary, required, strings.Join(names, ", "))
		}
	}

	if len(extras) > 0 {
		r.fail(
			"%s links %s, which #1133 says nothing in this workspace needs — the "+
				"allowed set is %s. A new system library in the image means a build "+
				"dependency came back; add it to imagePropsAllowedSonames only with "+
				"the reason it is now linked.",
			binary, strings.Join(extras, ", "), strings.Join(sortedKeys(imagePropsAllowedSonames), ", "))
		return
	}

	r.ok("%s links exactly %s (+ %s)", binary, strings.Join(names, ", "), strings.Join(structural, ", "))
}

func assertBinaryVersion(r *imagePropsReport, binary, out, version string) {
	out = strings.TrimSpace(out)
	if out == "" {
		r.fail("`%s --version` printed nothing", binary)
		return
	}
	fields := strings.Fields(out)
	reported := fields[len(fields)-1]
	if reported != version {
		r.fail(
			"the binary in the image reports %q, the workspace declares %q "+
				"(full output: %q). The OCI label is not evidence for this: a shared "+
				"CARGO_TARGET_DIR once left the wrong binary in target/release/ behind "+
				"a `Finished in 0.23s` line.",
			reported, version, out)
		return
	}
	r.ok("the binary reports %q, and the workspace agrees", out)
}

func assertNoLibpq(r *imagePropsReport, pkgs, files string) {
	var installed []string
	for _, line := range strings.Split(pkgs, "\n") {
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}
		// dpkg's abbreviated status: "ii" is installed. "rc" is removed with its
		// config left behind, which ships nothing and is not this gate's business.
		if !strings.HasPrefix(fields[0], "ii") {
			continue
		}
		name := strings.SplitN(fields[1], ":", 2)[0] // strip the :amd64 arch suffix
		if strings.HasPrefix(name, "libpq") {
			installed = append(installed, fields[1])
		}
	}

	var found []string
	for _, line := range strings.Split(files, "\n") {
		if line = strings.TrimSpace(line); line != "" {
			found = append(found, line)
		}
	}

	if len(installed) == 0 {
		r.ok("no libpq package is installed")
	} else {
		r.fail(
			"the built image installs %s. Nothing in the workspace can link libpq — "+
				"pq-sys, libpq-sys and diesel appear nowhere in Cargo.lock — so it is "+
				"scanned, patched and reported on for a library the process never "+
				"opens (#1133).",
			strings.Join(installed, ", "))
	}

	if len(found) == 0 {
		r.ok("no libpq shared object is present on the rootfs")
	} else {
		r.fail("the built image's rootfs carries %s", strings.Join(found, ", "))
	}
}

func assertSizeBudget(r *imagePropsReport, variant string, size int64) {
	budget, ok := imagePropsSizeBudgets[variant]
	if !ok || budget <= 0 {
		r.fail(
			"variant %q has no size budget in imagePropsSizeBudgets; it measured %s, "+
				"which is the number to record. An unsized published image is one "+
				"nobody has looked at.",
			variant, humanBytes(size))
		return
	}

	low := int64(float64(budget) * (1 - imagePropsSizeTolerance))
	high := int64(float64(budget) * (1 + imagePropsSizeTolerance))
	switch {
	case size > high:
		r.fail(
			"the image is %s, which is %s over the %s budget (+%.1f%%, tolerance ±%.0f%%). "+
				"Something was added to the runtime stage; if it belongs there, raise "+
				"imagePropsSizeBudgets[%q] deliberately.",
			humanBytes(size), humanBytes(size-budget), humanBytes(budget),
			100*float64(size-budget)/float64(budget), 100*imagePropsSizeTolerance, variant)
	case size < low:
		r.fail(
			"the image is %s, which is %s under the %s budget (-%.1f%%, tolerance ±%.0f%%). "+
				"An artifact that suddenly lost this much is the same shape as one that "+
				"lost its binary; confirm it still ships everything, then lower "+
				"imagePropsSizeBudgets[%q].",
			humanBytes(size), humanBytes(budget-size), humanBytes(budget),
			100*float64(budget-size)/float64(budget), 100*imagePropsSizeTolerance, variant)
	default:
		r.ok("the image is %s, within ±%.0f%% of the %s budget (%+.1f%%)",
			humanBytes(size), 100*imagePropsSizeTolerance, humanBytes(budget),
			100*float64(size-budget)/float64(budget))
	}
}

func sortedKeys(m map[string]bool) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

func humanBytes(n int64) string {
	const mib = 1024 * 1024
	return fmt.Sprintf("%.1f MiB", float64(n)/mib)
}

// ── the HEALTHCHECK, executed ─────────────────────────────────────────────────

// runDeclaredHealthcheck starts the image on its OWN CMD, runs the image's OWN
// HEALTHCHECK against it, and then kills the server and requires the healthcheck
// to fail.
//
// ⚠ Three states, not one. "The healthcheck passed" alone is satisfied by
// `HEALTHCHECK CMD true`, which is the artifact-level version of asserting
// `{ __typename }`. So the command is required to FAIL before the server exists,
// PASS while it is serving, and FAIL again once it is killed — mutate the world,
// re-ask, require the answer to change (Rule 3).
//
// ⚠ Nothing here re-types the Dockerfile. The command, its per-attempt timeout,
// its start-period, interval and retry count all come out of the built image's
// config, so a healthcheck that was edited, or dropped, is inspected as it
// actually shipped.
//
// ⚠ And nothing here supplies the bind address. DATABASE_URL and
// FRAISEQL_SCHEMA_PATH are operator-supplied everywhere this image is deployed,
// so supplying them is faithful; the port the server listens on is not, and a
// gate that sets it is a gate that agrees with itself about the one thing the
// HEALTHCHECK hardcodes.
func (m *FraiseqlCi) runDeclaredHealthcheck(
	ctx context.Context,
	source *dagger.Directory,
	built *dagger.Container,
	cfg imageConfig,
) (string, error) {
	if len(cfg.hcArgs) == 0 {
		return "", fmt.Errorf(
			"the built image declares no HEALTHCHECK, so every orchestrator that " +
				"waits on one — `depends_on: condition: service_healthy`, a " +
				"Kubernetes probe defaulted from the image — has nothing to wait for")
	}

	invocation, err := imagePropsHealthcheckInvocation(cfg.hcArgs, cfg.hcShell)
	if err != nil {
		return "", err
	}

	perAttempt := dockerDurationSeconds(cfg.hcTimeout, 5)
	deadline := dockerDurationSeconds(cfg.hcStartPeriod, 0) +
		dockerDurationSeconds(cfg.hcInterval, 30)*maxInt(cfg.hcRetries, 1)
	deadline = maxInt(deadline, 30)

	postgres := m.imageBootPgService()
	script := imagePropsHealthcheckScript(healthcheckScriptArgs{
		workdir:    cfg.workdir,
		command:    shellJoin(cfg.defaultArgs),
		invocation: invocation,
		perAttempt: perAttempt,
		deadline:   deadline,
	})

	out, err := built.
		WithFile("/schema.compiled.json", source.File("docker/e2e/schema.compiled.json")).
		WithServiceBinding(imageBootPgBindHost, postgres).
		WithEnvVariable("DATABASE_URL", fmt.Sprintf(
			"postgresql://%s:%s@%s:5432/%s", pgUser, pgPassword, imageBootPgBindHost, pgDatabase)).
		WithEnvVariable("FRAISEQL_SCHEMA_PATH", "/schema.compiled.json").
		WithEnvVariable("FRAISEQL_ENV", "development").
		WithExec([]string{"bash", "-c", script}).
		Stdout(ctx)
	if err != nil {
		return "", fmt.Errorf("the built image's HEALTHCHECK could not be executed: %w", err)
	}

	sections := imagePropsSections(out)
	r := &imagePropsReport{}
	r.printf("healthcheck: %s\n", invocation)
	r.printf("  per attempt %ds, healthy-by deadline %ds (start-period + interval x retries)\n",
		perAttempt, deadline)

	assertHealthcheckStates(r, sections)
	r.printf("\nserver log (tail):\n%s\n", indent(strings.TrimSpace(sections["SERVERLOG"])))

	if len(r.failures) > 0 {
		return r.String(), fmt.Errorf(
			"the image's own HEALTHCHECK does not do what the image claims:\n  - %s",
			strings.Join(r.failures, "\n  - "))
	}
	return r.String(), nil
}

func assertHealthcheckStates(r *imagePropsReport, sections map[string]string) {
	before := strings.TrimSpace(sections["BEFORE"])
	up := strings.TrimSpace(sections["UP"])
	down := strings.TrimSpace(sections["DOWN"])

	// 1. Before the server exists the healthcheck must fail. A healthcheck that
	//    passes here is not checking this server.
	if fieldValue(before, "exit") == "0" {
		r.fail(
			"the HEALTHCHECK succeeded BEFORE the server was started, so it is not " +
				"checking the server: a container running this image would report " +
				"healthy while serving nothing")
	} else {
		r.ok("the HEALTHCHECK fails before the server is started (exit %s)", fieldValue(before, "exit"))
	}

	// 2. With the server up it must pass, inside the deadline the image's own
	//    interval and retry count define.
	switch fieldValue(up, "healthy") {
	case "yes":
		r.ok("the HEALTHCHECK passed %ss after the image started on its own CMD", fieldValue(up, "after"))
	default:
		r.fail(
			"the HEALTHCHECK never passed. A container started from this image on its "+
				"own CMD is marked UNHEALTHY, and anything waiting on it — a compose "+
				"`condition: service_healthy`, a Kubernetes readiness probe defaulted "+
				"from the image — never starts. Detail: %s",
			collapse(up))
	}

	// 3. Killed, it must fail again. Without this the tier passes on a
	//    healthcheck that cannot fail.
	if fieldValue(down, "unhealthy") == "yes" {
		r.ok("the HEALTHCHECK failed again once the server was killed")
	} else {
		r.fail(
			"the HEALTHCHECK still passed after the server was killed, so it does not "+
				"report on this server at all. Detail: %s", collapse(down))
	}
}

// fieldValue reads `key=value` out of a measurement section.
func fieldValue(section, key string) string {
	for _, line := range strings.Split(section, "\n") {
		for _, field := range strings.Fields(line) {
			if name, value, ok := strings.Cut(field, "="); ok && name == key {
				return value
			}
		}
	}
	return ""
}

func collapse(s string) string {
	return strings.Join(strings.Fields(s), " ")
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// dockerDurationSeconds parses the "30s" / "1m30s" form the image config uses.
func dockerDurationSeconds(s string, fallback int) int {
	d, err := time.ParseDuration(strings.TrimSpace(s))
	if err != nil || d <= 0 {
		return fallback
	}
	return int(d.Seconds())
}

// imagePropsHealthcheckInvocation turns the image's healthcheck config back into
// something a shell can run, in whichever of Docker's two forms it was written.
func imagePropsHealthcheckInvocation(args []string, shell bool) (string, error) {
	if len(args) == 0 {
		return "", fmt.Errorf("the built image declares no HEALTHCHECK command")
	}
	if shell {
		// Shell form: Docker runs the single command string through /bin/sh -c.
		return "sh -c " + shellQuote(strings.Join(args, " ")), nil
	}
	return shellJoin(args), nil
}

func shellJoin(args []string) string {
	quoted := make([]string, 0, len(args))
	for _, a := range args {
		quoted = append(quoted, shellQuote(a))
	}
	return strings.Join(quoted, " ")
}

func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

// ── the measurement scripts ───────────────────────────────────────────────────
//
// ⚠ These scripts MEASURE; the Go above JUDGES. That is the opposite of
// image_boot.go's script, which asserts inline, and the difference is
// deliberate: this tier's failures have to name a delta ("the image is 40.2 MiB
// over the 108.6 MiB budget", "it links libcurl.so.4"), and a shell that exits 1
// at the first surprise reports one line and hides the rest. Every command below
// therefore swallows its own failure, and every judgement lives in an assert*
// function that can be read next to the property it protects.

func imagePropsProbeScript(binary string) string {
	return strings.NewReplacer(
		"@M@", imagePropsMarker,
		"@BIN@", shellQuote(binary),
	).Replace(`
set -u
echo "@M@UID"
id -u 2>&1 || true
echo "@M@USERNAME"
id -un 2>&1 || true
echo "@M@LDD"
ldd @BIN@ 2>&1 || true
echo "@M@VERSION"
@BIN@ --version 2>&1 || true
echo "@M@END"
`)
}

func imagePropsScanScript() string {
	return strings.NewReplacer("@M@", imagePropsMarker).Replace(`
set -u
echo "@M@PKGS"
dpkg-query -W -f='${db:Status-Abbrev} ${Package}\n' 2>/dev/null || true
echo "@M@LIBPQFILES"
find / -xdev -name 'libpq*' 2>/dev/null || true
echo "@M@END"
`)
}

type healthcheckScriptArgs struct {
	workdir    string
	command    string
	invocation string
	perAttempt int
	deadline   int
}

func imagePropsHealthcheckScript(a healthcheckScriptArgs) string {
	workdir := a.workdir
	if workdir == "" {
		workdir = "/"
	}
	return strings.NewReplacer(
		"@M@", imagePropsMarker,
		"@WORKDIR@", shellQuote(workdir),
		"@CMD@", a.command,
		"@HC@", a.invocation,
		"@PERATTEMPT@", fmt.Sprintf("%d", a.perAttempt),
		"@DEADLINE@", fmt.Sprintf("%d", a.deadline),
	).Replace(`
set -u

cd @WORKDIR@ || { echo "cannot cd to the image's WORKDIR"; exit 1; }

HC_LOG=/tmp/fraiseql-healthcheck.log
SRV_LOG=/tmp/fraiseql-server.log
: >"$SRV_LOG"

# The image's own HEALTHCHECK, run under the per-attempt timeout the image itself
# declares. Docker gives each attempt exactly that long; a check that answers
# correctly but too late is a failed check, and a gate that waits longer than the
# artifact does would call it healthy.
run_healthcheck() { timeout @PERATTEMPT@ @HC@ ; }

# curl draws a carriage-return progress meter on stderr, so a byte-capped head of
# this log is a screenful of dashes with the actual error scrolled off the end.
hc_tail() {
  tr '\r' '\n' <"$HC_LOG" \
    | grep -vE '^[[:space:]]*$|^[[:space:]]*%|Dload|--:--:--' \
    | tail -n 3
}

echo "@M@BEFORE"
run_healthcheck >"$HC_LOG" 2>&1
echo "exit=$?"
hc_tail

echo "@M@START"
# The image's own CMD, verbatim, with no Args and no entrypoint override. The
# whole point is the command a "docker run" with no arguments would execute.
@CMD@ >"$SRV_LOG" 2>&1 &
SRV_PID=$!
echo "pid=$SRV_PID"

echo "@M@UP"
healthy=no
waited=0
for i in $(seq 1 @DEADLINE@); do
  waited=$i
  if ! kill -0 "$SRV_PID" 2>/dev/null; then
    echo "exited=yes the process the image starts is gone after ${i}s"
    break
  fi
  if run_healthcheck >"$HC_LOG" 2>&1; then healthy=yes; break; fi
  sleep 1
done
echo "healthy=$healthy after=$waited deadline=@DEADLINE@"
hc_tail

echo "@M@DOWN"
# The discriminator. Everything above is satisfied by a HEALTHCHECK that cannot
# fail; this is not.
kill "$SRV_PID" 2>/dev/null
for i in $(seq 1 15); do kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
kill -9 "$SRV_PID" 2>/dev/null || true
wait "$SRV_PID" 2>/dev/null
unhealthy=no
for i in $(seq 1 30); do
  if run_healthcheck >"$HC_LOG" 2>&1; then sleep 1; else unhealthy=yes; break; fi
done
echo "unhealthy=$unhealthy"
hc_tail

echo "@M@SERVERLOG"
tail -n 40 "$SRV_LOG" 2>/dev/null || true
echo "@M@END"
`)
}

// imagePropsSections splits a measurement script's stdout on its marker lines.
func imagePropsSections(out string) map[string]string {
	sections := map[string]string{}
	name := ""
	var body strings.Builder
	flush := func() {
		if name != "" {
			sections[name] = body.String()
		}
		body.Reset()
	}
	for _, line := range strings.Split(out, "\n") {
		if rest, ok := strings.CutPrefix(strings.TrimSpace(line), imagePropsMarker); ok {
			flush()
			name = rest
			continue
		}
		body.WriteString(line)
		body.WriteString("\n")
	}
	flush()
	return sections
}
