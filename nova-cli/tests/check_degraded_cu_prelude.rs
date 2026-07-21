// [M-104.10-diag-pipeline-correctness] (CLI counterpart): `nova check FILE`
// on a standalone `.nv` file with NO ancestor `nova.toml` (neither from the
// process CWD nor from the file's own path) used to silently skip import
// resolution ENTIRELY — no prelude merge, no sig-table — while still
// type-checking the module as if complete. Every prelude symbol
// (`println`, `Vec`, …) then false-reddened as "undefined identifier" for
// perfectly valid Nova code. This is the exact false-red class the sibling
// `nova-lsp` fix (`[M-104.10-degraded-cu-red]`, `nova-lsp/src/compiler.rs`)
// already eliminated on the IDE side, left unfixed on the CLI side.
//
// Worse: `resolve_std_path` (compiler-codegen/src/manifest.rs) explicitly
// supports a `NOVA_STD_PATH` env override for exactly this out-of-project
// scenario, but the old CWD-only `find_repo_root()` gate in
// `check_one_file` (nova-cli/src/main.rs) never even reached the call that
// would honor it — the override was silently ignored.
//
// Root cause: `check_one_file`'s import-resolution block gated the ENTIRE
// `resolve_imports_inline_ex`/`collect_all_signatures` attempt behind a
// bare `find_repo_root()` (CWD-anchored `nova.toml` walk) succeeding, with
// no fallback — unlike `embed_resolve` a few lines below in the SAME
// function, which already used the path-anchored `find_repo_root_from`.
//
// Fix: best-effort repo anchor for the import-resolution block, mirroring
// the LSP's fallback chain — CWD-anchored `nova.toml` -> entry-file-anchored
// `nova.toml` (`find_repo_root_from`, reusing the same helper `embed_resolve`
// already calls) -> the entry file's own directory, so `NOVA_STD_PATH`
// (absolute) or a sibling `std/` still resolves for a genuinely standalone
// probe. `resolve_imports_inline_ex` is a no-op-safe call when the resulting
// `stdlib_dir` does not exist (prelude auto-import guard in `imports.rs`),
// so this can only improve resolution, never regress it.
//
// This is a CLI end-to-end regression guard — not expressible in
// `spec_tests/conformance` (fixtures there necessarily live IN the tree,
// and this bug is specifically about an out-of-tree file). Precedent:
// `oot_interp_stringbuilder.rs` / `oot_ancestor_manifest_module_path.rs`
// isolation pattern (own PID-tagged temp subdir per test). Uses `nova
// check` (not `nova build`/`nova test`), so no C toolchain / GC lib is
// needed — matches `interp_unsupported.rs`'s lighter invocation.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Real stdlib next to this crate (`nova-cli/../std`) — used via
/// `NOVA_STD_PATH` so the out-of-tree probe below can resolve `std.prelude`
/// with no `nova.toml` anywhere in its ancestry.
fn real_std_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("nova-cli has a parent dir")
        .join("std")
}

fn nova() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nova"));
    cmd.env("NOVA_STD_PATH", real_std_path());
    cmd
}

fn combined_output(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Isolated out-of-tree temp dir with NO ancestor `nova.toml` at all (own
/// PID-tagged subdir directly under the system temp root — never inside the
/// Nova repo, and never shared with another test's fixture, matching the
/// folder-module-is-one-module isolation rule).
fn isolated_oot_dir(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nova_degraded_cu_check_{}_{}", std::process::id(), tag));
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    dir
}

/// POS: a standalone out-of-tree file using ONLY prelude symbols
/// (`println`) must PASS `nova check` when `NOVA_STD_PATH` points at a real
/// stdlib — no false "undefined identifier" on `println`.
#[test]
fn nova_check_oot_prelude_symbol_resolves_via_nova_std_path() {
    let dir = isolated_oot_dir("pos");
    let file = dir.join("main.nv");
    fs::write(
        &file,
        "module m\n\nfn go() -> () {\n    println(\"hi\")\n}\n",
    )
    .expect("write temp .nv");

    let out = nova().arg("check").arg(&file).output().expect("spawn `nova check`");
    let _ = fs::remove_dir_all(&dir);
    let combined = combined_output(&out);

    assert!(
        out.status.success(),
        "`nova check` on an out-of-tree file using only prelude symbols must \
         PASS when NOVA_STD_PATH resolves a real stdlib (degraded-CU parity \
         with nova-lsp's [M-104.10-degraded-cu-red]); status={:?}\n{combined}",
        out.status
    );
    assert!(
        !combined.to_lowercase().contains("undefined identifier"),
        "must not false-red a valid prelude call as an undefined identifier; \
         got:\n{combined}"
    );
}

/// NEG: the fix must not swallow a genuine error — an out-of-tree file with
/// a real undefined symbol (NOT a prelude one) must still fail `nova check`
/// with that error, even with `NOVA_STD_PATH` resolving the stdlib.
#[test]
fn nova_check_oot_genuine_undefined_symbol_still_reported() {
    let dir = isolated_oot_dir("neg");
    let file = dir.join("main.nv");
    fs::write(
        &file,
        "module m\n\nfn bad() -> int => undefined_symbol_xyz\n",
    )
    .expect("write temp .nv");

    let out = nova().arg("check").arg(&file).output().expect("spawn `nova check`");
    let _ = fs::remove_dir_all(&dir);
    let combined = combined_output(&out);

    assert!(
        !out.status.success(),
        "a genuine undefined-symbol error must still fail `nova check`; \
         got:\n{combined}"
    );
    assert!(
        combined.contains("undefined_symbol_xyz"),
        "the genuine error must still name the real undefined symbol; \
         got:\n{combined}"
    );
}
