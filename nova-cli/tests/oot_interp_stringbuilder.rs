// [M-standalone-out-of-tree-interp-sb-typedef]: a standalone `.nv` file
// living OUTSIDE the project tree (no `nova.toml` ancestor of its own — a
// `%TEMP%` probe file is the canonical case) with string interpolation
// (`"${expr}"`) used to CC-FAIL under `nova test`:
//
//   error: must use 'struct' tag to refer to type 'Nova_StringBuilder'
//   error: initializing 'nova_str' with an expression of incompatible type 'int'
//
// Root cause: `codegen_to_c` (compiler-codegen/src/test_runner.rs) resolved
// the project root ITSELF by walking up from the `.nv` FILE's own directory
// (`find_repo_root_from(path)`). For an out-of-tree file that walk finds no
// `nova.toml` and returns `None`, which silently skipped the ENTIRE
// cross-file import block — including the implicit `std.prelude`
// auto-import. `StringBuilder` (interpolation's lowering target, Plan
// 109/D179 — a Nova-defined type reached only via `std.prelude`) then never
// entered the module, so its C typedef and method bodies were never
// emitted, while `emit_interpolated_str` still unconditionally synthesized
// raw C calls to them.
//
// `nova build` never had this bug: `cmd_build` already threads its own
// CWD-resolved `repo`/`stdlib_dir` (from `find_repo_root()`, walking up from
// the process's CWD — always inside the project when `nova` is invoked from
// within it, regardless of where the TARGET file lives) through to
// `resolve_imports_inline`/`resolve_embeds` unconditionally. The fix makes
// `nova test`/`nova test-build` use the exact same already-resolved
// repo/stdlib_dir instead of re-deriving one from the target file's path.
//
// This is a CLI end-to-end regression guard — not expressible in
// `spec_tests/conformance` (fixtures there necessarily live IN the tree).
// Precedent: `nova-cli/tests/lint_deny.rs` isolation pattern. Requires the
// full C toolchain (this actually invokes `nova test`, unlike the lighter
// `nova check`/`nova run` used by `interp_unsupported.rs`), so it needs a
// GC lib/include reachable the same way any other `nova test` invocation
// does (default repo-relative `vcpkg_installed`, or `NOVA_GC_LIB_DIR`/
// `NOVA_GC_INCLUDE_DIR` env override for isolated worktrees without their
// own `vcpkg_installed`, see `nova-cli/src/main.rs::env_path_override`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nova() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nova"))
}

/// An isolated file under `std::env::temp_dir()` — outside any `nova.toml`
/// tree by construction (system temp is never inside the Nova repo).
/// Isolation (own subdir per test) mirrors `lint_deny.rs`/
/// `interp_unsupported.rs`: Nova treats a directory as one folder-module of
/// co-equal files, so a shared temp dir would fold unrelated fixtures into
/// one module and collide.
fn isolated_oot_nv(tag: &str) -> (PathBuf, PathBuf) {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nova_oot_interp_sb_{}_{}", std::process::id(), tag));
    fs::create_dir_all(&dir).expect("mkdir temp dir");
    let file = dir.join("main.nv");
    // int / f64 / f32 / str interpolation — f64 alone is enough to trigger
    // the StringBuilder path (any interpolated non-`str` arg needs it via
    // `Display`/`nova_*_to_str`), the other three widen the regression net
    // per the fix's gate (int/f64/f32/str all route through the same
    // `emit_interpolated_str` StringBuilder synthesis).
    let src = "module t\n\n\
        test \"t\" {\n    \
            ro i = 42\n    \
            ro f64v = 1.5\n    \
            ro f32v f32 = 2.5\n    \
            ro s = \"hi\"\n    \
            assert(\"${i}\" == \"42\")\n    \
            assert(\"${f64v}\" == \"1.5\")\n    \
            assert(\"${f32v}\" == \"2.5\")\n    \
            assert(\"${s}\" == \"hi\")\n\
        }\n";
    fs::write(&file, src).expect("write temp .nv");
    (dir, file)
}

fn combined_output(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// `nova test <out-of-tree-file>` must PASS: string interpolation over
/// int/f64/f32/str must codegen + compile + link + run successfully even
/// when the target `.nv` has no `nova.toml` ancestor of its own.
#[test]
fn nova_test_out_of_tree_interpolation_passes() {
    let (dir, file) = isolated_oot_nv("main");
    let out = nova()
        .arg("test")
        .arg(&file)
        .output()
        .expect("spawn `nova test` on out-of-tree file");
    let _ = fs::remove_dir_all(&dir);
    let combined = combined_output(&out);

    assert!(
        out.status.success(),
        "`nova test` on an out-of-tree file with string interpolation must \
         PASS (StringBuilder must resolve via implicit std.prelude import \
         same as in-tree); status={:?}\n{combined}",
        out.status
    );
    assert!(
        !combined.contains("Nova_StringBuilder"),
        "must not regress into the CC-FAIL this test guards against \
         (missing Nova_StringBuilder typedef/body for out-of-tree files); \
         got:\n{combined}"
    );
    assert!(
        combined.contains("PASS"),
        "expected a PASS outcome line; got:\n{combined}"
    );
}
