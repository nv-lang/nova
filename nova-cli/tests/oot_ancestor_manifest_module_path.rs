// [M-oot-dash-module-name-e78]: a standalone `.nv` file with NO `module`
// declaration, living under a directory tree that happens to have an
// UNRELATED ancestor `nova.toml` several levels up (e.g. a leftover
// manifest from a different earlier probe sharing the same scratch tree),
// used to CC-FAIL under `nova test` with a false positive:
//
//   [E_D78_MODULE_PATH_MISMATCH] module declaration does not match file path
//
// Reported repro (owner, 2026-07-20) as "a dash/double-dash in the path
// triggers this" — that hypothesis does NOT hold: `package_name` is read
// verbatim from `[package] name` in `nova.toml` (grep confirms no
// directory-name-to-identifier synthesis exists ANYWHERE in the compiler);
// a dash-only control (clean `%TEMP%/oot-probe`, no ancestor manifest) PASSES
// same as a no-dash control. The ACTUAL trigger, isolated by bisecting the
// two conditions independently: `manifest::find_manifest` walks UP from the
// file's own directory looking for the NEAREST ancestor `nova.toml`, with NO
// awareness of which project actually invoked `nova` — so a module-less
// out-of-tree probe can land under a wholly unrelated manifest above it and
// get that manifest's `parent.target` rule wrongly enforced. Confirmed by
// reproducing with an ancestor manifest ALONE (no dash in the path at all)
// — identical `E_D78_MODULE_PATH_MISMATCH`/`cmin`-shaped failure.
//
// Root cause: `check_module_path_with_kind` (`compiler-codegen/src/manifest.rs`)
// was invoked from `test_runner.rs::codegen_to_c` (`nova test`) and from
// `nova-cli/src/main.rs` (`nova check`/`nova build`/`nova doc`) WITHOUT
// regard to `repo` — the CWD-resolved project root each of those call
// sites had ALREADY resolved for import/prelude purposes (the same `repo`
// threaded into `resolve_imports_inline*` per
// `[M-standalone-out-of-tree-interp-sb-typedef]`). D78 enforcement used a
// SECOND, inconsistent notion of "which project owns this file"
// (`find_manifest`'s blind upward filesystem walk) instead of reusing the
// first.
//
// Fix: `manifest::is_outside_repo(file, repo)` — a file resolving outside
// the invoking project's own `repo` is exempt from D78 (matches the
// existing standalone/anonymous-module contract: a module-less script
// outside the tree was ALREADY fine when no ancestor manifest existed at
// all; this closes the gap where one accidentally does). In-tree files —
// including real nested sub-packages with their OWN manifest — are
// untouched.
//
// This is a CLI end-to-end regression guard, not expressible in
// `spec_tests/conformance` (fixtures there necessarily live IN the tree,
// and this bug is specifically about an OUT-of-tree probe landing under an
// unrelated ancestor manifest). Precedent: `oot_interp_stringbuilder.rs`
// isolation pattern (own PID-tagged temp subdir per test).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nova() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nova"))
}

fn combined_output(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Builds an isolated temp tree:
///   <base>/<tag>-anc-pkg/          <- ancestor `nova.toml` (UNRELATED package,
///                                     stands in for a leftover/foreign manifest)
///     nova.toml
///     sub/
///       probe.nv                  <- NO `module` declaration (anonymous
///                                     standalone script), one dir under the
///                                     ancestor manifest — same shape as the
///                                     reported repro's `scratchpad/ootv/probe.nv`
///                                     under `scratchpad/nova.toml`.
///
/// `dash` selects whether the ancestor package directory name itself
/// contains a dash (`anc-pkg-1` style) — kept as a parameter so the suite
/// covers both the owner's literal hypothesis (dash in path) AND the
/// isolated real trigger (ancestor manifest, no dash) side by side.
fn isolated_ancestor_manifest_probe(tag: &str, dash_in_path: bool) -> (PathBuf, PathBuf) {
    let mut anc = std::env::temp_dir();
    let pkg_dir_name = if dash_in_path {
        format!("nova_oot_ancestor_{}_{}-pkg", std::process::id(), tag)
    } else {
        format!("nova_oot_ancestor_{}_{}_pkg", std::process::id(), tag)
    };
    anc.push(pkg_dir_name);
    let sub = anc.join("sub");
    fs::create_dir_all(&sub).expect("mkdir ancestor+sub temp dir");
    fs::write(
        anc.join("nova.toml"),
        "[package]\nname = \"unrelated_leftover_pkg\"\nversion = \"0.1.0\"\n\n[lib]\nsrc = \".\"\n",
    )
    .expect("write ancestor nova.toml");
    let file = sub.join("probe.nv");
    // No `module` line at all — anonymous standalone script, exactly the
    // reported repro shape (`test { ... }` with no leading `module X`).
    let src = "test \"probe\" {\n    \
        ro a = 1.5\n    \
        assert(\"${a}\" == \"1.5\")\n\
    }\n";
    fs::write(&file, src).expect("write probe.nv");
    (anc, file)
}

fn run_nova_test_and_assert_pass(dir: PathBuf, file: PathBuf, case: &str) {
    let out = nova()
        .arg("test")
        .arg(&file)
        .output()
        .expect("spawn `nova test` on out-of-tree probe");
    let combined = combined_output(&out);
    // DELETE EXACTLY WHAT THE PROBE CREATED. Until 2026-08-23 this line read
    // `dir.parent()`, and `dir` is `<system temp>/nova_oot_ancestor_<pid>_<tag>_pkg`
    // -- so the parent is the SYSTEM TEMP ROOT, and the test asked to delete
    // all of it. `.ok()` swallowed the outcome, so nothing ever said so.
    //
    // On Windows the call fails (files in use) and the suite is green, which
    // is why it lived. On the CI runner it partially SUCCEEDS: the two tests
    // in this file run in parallel threads, and whichever finishes first
    // deletes the other's `probe.nv` out from under a running `nova test`.
    // That is the CI-only red the gate reported -- `out.status.success()` on
    // line 121, in whichever of the two lost the race.
    debug_assert!(
        dir.file_name()
            .map(|n| n.to_string_lossy().starts_with("nova_oot_ancestor_"))
            .unwrap_or(false),
        "cleanup must target the probe's own directory, not an ancestor: {dir:?}"
    );
    fs::remove_dir_all(&dir).ok();

    assert!(
        !combined.contains("E_D78_MODULE_PATH_MISMATCH"),
        "[{case}] must NOT false-positive D78 for a module-less out-of-tree \
         probe just because SOME unrelated ancestor `nova.toml` exists \
         further up the filesystem; got:\n{combined}"
    );
    assert!(
        out.status.success(),
        "[{case}] `nova test` on a module-less out-of-tree probe (with an \
         unrelated ancestor manifest above it) must PASS; status={:?}\n{combined}",
        out.status
    );
    assert!(
        combined.contains("PASS"),
        "[{case}] expected a PASS outcome line; got:\n{combined}"
    );
}

/// Literal owner repro shape: ancestor package directory name contains a
/// dash (`...-pkg`).
#[test]
fn nova_test_oot_probe_under_dashed_ancestor_manifest_passes() {
    let (dir, file) = isolated_ancestor_manifest_probe("dash", true);
    run_nova_test_and_assert_pass(dir, file, "dash-in-path");
}

/// Isolation control: SAME ancestor-manifest shape, but with NO dash
/// anywhere in the constructed path — proves the trigger is the ancestor
/// manifest, not the dash (the owner's hypothesis didn't hold; documented
/// in the module doc-comment above).
#[test]
fn nova_test_oot_probe_under_plain_ancestor_manifest_passes() {
    let (dir, file) = isolated_ancestor_manifest_probe("plain", false);
    run_nova_test_and_assert_pass(dir, file, "no-dash-control");
}
