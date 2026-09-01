// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plan 196 Ф.1a — CI-guard for `docs/dev/compiler-conventions.md` §0/§9/§10:
//! raw `Nova_`/`____` mangled-C-type-string DECODE (extracting semantic
//! identity — base type name via `Nova_`-prefix strip, generic type-args via
//! the `____` mono-mangle separator) is confined to functions named
//! `debt_*` — the marked, tracked, single sanctioned decode surface (172.12
//! заход 8/A4). A raw decode call OUTSIDE a `debt_*` function is a SECOND,
//! un-audited window onto type identity — exactly the §0 "two windows of
//! truth" anti-pattern this repo forbids (`ResolvedType`/D315 is supposed to
//! be the ONE canonical carrier; parsing it back out of the mangled C string
//! anywhere else means the printer and the parser can silently diverge).
//!
//! **History:** 172.12 заход 8 (2026-07-09) drove this count to 0 outside
//! `debt_*` ("A4 — потребительские `____`/`Nova_`-decode ЗАКРЫТЫ — greп-
//! инвариант вне debt-хелперов 129→0"). By the time Plan 196 recon ran
//! (2026-07-11) it had drifted back with **zero CI protection** (Plan 186's
//! D412 hex-blob/embed work plus other untracked additions) — this test IS
//! that protection, so it never silently drifts again.
//!
//! **Baseline:** `[M-196-raw-decode-allowlist]` (Plan 196 Ф.1a, exact re-audit
//! 2026-07-11) freezes the CURRENT state below. The umbrella recon's rough
//! estimate ("~12 hits in 10 functions") undercounted — this file's precise
//! re-scan found 22 sites across 16 functions; the extra 6 functions
//! (`emit_for`, `collect_pattern_inner_bindings`, `register_container_eq_mono`,
//! `infer_mono_method_ret_with_args`, `fn_field_call_sig`, `infer_expr_c_type`)
//! are pre-existing undocumented debt the rough estimate missed, not new
//! Plan-186 drift — they are included here anyway because this lint's job is
//! "0 outside debt_* from today forward", not "0 outside debt_* except for
//! debt nobody counted yet". Every later Plan-196 phase (Ф.2-Ф.6) SHRINKS
//! this map; the close-out acceptance criterion is an EMPTY map.
//!
//! Scope: `compiler-codegen/src/**/*.rs` (whole crate — today every hit
//! happens to live in `emit_c.rs`, but the invariant is crate-wide and this
//! scan is not).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/compiler-codegen → parent = <repo>.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler-codegen has a parent (repo root)")
        .to_path_buf()
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().map_or(false, |x| x == "rs") {
            out.push(p);
        }
    }
}

/// Literal substrings that constitute a "raw decode" of a mangled Nova
/// C-type name: extracting the BASE identity by stripping the `Nova_`
/// prefix, or detecting/splitting the `____` mono type-arg separator.
/// Deliberately does NOT include bare `starts_with("Nova_")`/`ends_with('*')`
/// shape predicates ("is this a heap pointer to *some* Nova struct?") —
/// those test pointer SHAPE, not decode identity; `debt_is_bare_nova_ptr` /
/// `debt_strip_nova_trim_start` etc. already own that narrower class inside
/// `debt_*`, and the shape-only predicates left in `emit_expr`/`emit_call`/
/// `infer_call_ret_c` are a separate, much larger audit this phase does not
/// attempt (would be hundreds of sites — out of Ф.1 "риск≈0" scope).
const DECODE_NEEDLES: &[&str] = &[
    ".strip_prefix(\"Nova_\")",
    ".trim_start_matches(\"Nova_\")",
    ".contains(\"____\")",
    ".find(\"____\")",
    ".split(\"____\")",
    ".split_once(\"____\")",
    ".rsplit_once(\"____\")",
];

/// If `line` is a function-header line (any indent — impl methods, free fns,
/// nested local fns), returns the fn name. Matches `[pub[(crate)]] [async]
/// fn <name>` after left-trim. Closures (`|x| ...`) are NOT `fn` in Rust, so
/// they never reset the tracker — a raw decode written inside a closure
/// body is correctly attributed to the enclosing NAMED function.
/// Name of the function a header line declares, or `None` if the line is not
/// a header.
///
/// [221.1 #861] Visibility forms are stripped GENERICALLY, not from a list of
/// three. The previous version knew only `pub(crate) `, `pub ` and `async `,
/// and a header it does not recognise is worse than one it rejects: the
/// scanner keeps `current_fn` from the LAST header it did recognise, so every
/// finding inside the unrecognised function is reported under a neighbour
/// whose body ended long before. Measured 2026-09-01: the finding at
/// `emit_c/variant_ctor_channel.rs:54` was attributed to
/// `set_resolved_variant_ctors` (lines 17-22) while it actually sits in
/// `pub(super) fn channel_variant_ctx` (line 34). One of the twenty-four
/// entries in the current report, and it is the one that would have sent the
/// repair to the wrong function.
fn fn_header_name(line: &str) -> Option<String> {
    let t = line.trim_start();
    // `pub`, `pub(crate)`, `pub(super)`, `pub(in some::path)` -- take the
    // parenthesised part as a unit rather than enumerating the spellings.
    let t = if let Some(rest) = t.strip_prefix("pub") {
        let rest = if rest.starts_with('(') {
            match rest.find(") ") {
                Some(i) => &rest[i + 2..],
                None => return None,   // `pub(` with no close: not a header
            }
        } else {
            rest.strip_prefix(' ').unwrap_or(rest)
        };
        rest
    } else {
        t
    };
    let t = t.strip_prefix("default ").unwrap_or(t);
    let t = t.strip_prefix("const ").unwrap_or(t);
    let t = t.strip_prefix("async ").unwrap_or(t);
    let t = t.strip_prefix("unsafe ").unwrap_or(t);
    // `extern "C" fn`, `extern "system" fn`, ...
    let t = if let Some(rest) = t.strip_prefix("extern ") {
        match rest.strip_prefix('"').and_then(|r| r.find('"').map(|i| &r[i + 1..])) {
            Some(after) => after.strip_prefix(' ').unwrap_or(after),
            None => rest,
        }
    } else {
        t
    };
    let t = t.strip_prefix("fn ")?;
    let name: String = t
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

struct Hit {
    file: String,
    line: usize,
    fn_name: String,
    snippet: String,
}

/// Whole-crate scan: every raw-decode `DECODE_NEEDLES` hit, tagged with its
/// enclosing (last-seen-header) function name. Skips comment-only lines
/// (`// ...` after trim) — quoting the pattern in a doc-comment is not a
/// real decode site (172.12's audit hit exactly this false-positive class).
fn scan() -> Vec<Hit> {
    let root = repo_root().join("compiler-codegen").join("src");
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no .rs found under {:?}", root);

    let mut hits = Vec::new();
    for f in &files {
        let Ok(src) = fs::read_to_string(f) else { continue };
        let rel = f
            .strip_prefix(repo_root())
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        let mut current_fn = String::from("<module-level>");
        for (i, line) in src.lines().enumerate() {
            if let Some(name) = fn_header_name(line) {
                current_fn = name;
            }
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if DECODE_NEEDLES.iter().any(|n| line.contains(n)) {
                hits.push(Hit {
                    file: rel.clone(),
                    line: i + 1,
                    fn_name: current_fn.clone(),
                    snippet: trimmed.trim_end().to_string(),
                });
            }
        }
    }
    hits
}

/// `[M-196-raw-decode-allowlist]` baseline: fn_name -> expected count of
/// raw-decode sites OUTSIDE `debt_*` (audited 2026-07-11, `emit_c.rs` only —
/// today's only offender file). Ф.2-Ф.6 shrink this to `{}` (the umbrella
/// acceptance criterion, `docs/plans/196-one-truth-closeout.md`).
fn baseline_allowlist() -> BTreeMap<&'static str, usize> {
    [
        ("emit_protocol_box_typedef", 2),
        ("emit_value_record_type", 1),
        ("emit_record_type", 1),
        ("emit_sum_type", 1),
        ("emit_generic_type_instance", 1),
        ("emit_expr_with_target_type", 1),
        ("emit_expr", 3),
        ("emit_call", 3),
        ("emit_for", 1),
        ("collect_pattern_inner_bindings", 1),
        ("register_container_eq_mono", 1),
        ("infer_mono_method_ret_with_args", 1),
        ("register_novaopt_decl", 1),
        ("fn_field_call_sig", 1),
        ("infer_call_ret_c", 1),
        // `infer_expr_c_type`'s own legacy arms (Channels 6c-6z) are ALREADY
        // tracked by a separate, pre-existing marker
        // (`[M-172.1-lifted-legacy-arms]`, Plan 196 Ф.1d/Ф.2-6 close it by
        // deleting the arms outright) — kept in THIS map too because the
        // lint is purely mechanical (file:line-based), not marker-aware.
        ("infer_expr_c_type", 2),
    ]
    .into_iter()
    .collect()
}

#[test]
fn no_raw_type_decode_outside_debt_helpers() {
    let hits = scan();
    let mut by_fn: BTreeMap<String, Vec<&Hit>> = BTreeMap::new();
    for h in &hits {
        if h.fn_name.starts_with("debt_") {
            continue; // sanctioned: the marked, tracked decode surface
        }
        by_fn.entry(h.fn_name.clone()).or_default().push(h);
    }

    let baseline = baseline_allowlist();
    let mut new_violations = Vec::new();
    let mut shrink_needed = Vec::new();

    for (fn_name, hs) in &by_fn {
        let expected = baseline.get(fn_name.as_str()).copied().unwrap_or(0);
        if hs.len() > expected {
            let sites: Vec<String> = hs
                .iter()
                .map(|h| format!("{}:{}: {}", h.file, h.line, h.snippet))
                .collect();
            new_violations.push(format!(
                "fn `{}`: {} raw-decode site(s) outside debt_* (baseline allows {}):\n    {}",
                fn_name,
                hs.len(),
                expected,
                sites.join("\n    ")
            ));
        } else if hs.len() < expected {
            shrink_needed.push(format!(
                "fn `{}`: baseline allows {}, only {} found now — SHRINK the \
                 allowlist in baseline_allowlist() (this is PROGRESS, not a bug)",
                fn_name,
                expected,
                hs.len()
            ));
        }
    }
    for (&fn_name, &expected) in &baseline {
        if !by_fn.contains_key(fn_name) && expected > 0 {
            shrink_needed.push(format!(
                "fn `{}`: baseline allows {}, 0 found now — function fully clean, \
                 REMOVE its allowlist entry from baseline_allowlist()",
                fn_name, expected
            ));
        }
    }

    assert!(
        new_violations.is_empty(),
        "docs/dev/compiler-conventions.md §0/§9/§10 [M-196-raw-decode-allowlist]: raw \
         Nova_/____ type-decode found OUTSIDE debt_* helpers, beyond the frozen \
         baseline — this is a NEW second-window-of-truth site. Wrap the decode in \
         a dedicated `debt_*`-prefixed helper (existing pattern, ~50 examples in \
         emit_c.rs) or, better, extend the RT-native `resolved_type_to_c` \
         (`ResolvedType`, D315) path instead of parsing the mangled C string:\n{}",
        new_violations.join("\n")
    );
    assert!(
        shrink_needed.is_empty(),
        "[M-196-raw-decode-allowlist] is stale (docs/dev/compiler-conventions.md §9 — \
         each Plan 196 phase SHRINKS the allowlist as arms are removed/lifted into \
         debt_* helpers): update baseline_allowlist() in \
         compiler-codegen/tests/no_raw_type_decode.rs to match reality:\n{}",
        shrink_needed.join("\n")
    );

    // Non-vacuity: the scanner MUST find the large, real, sanctioned debt_*
    // surface — else it silently stopped seeing lines/files (walk/regex
    // regression) and every assertion above would vacuously pass.
    let debt_hits = hits.iter().filter(|h| h.fn_name.starts_with("debt_")).count();
    assert!(
        debt_hits >= 40,
        "non-vacuity: expected >=40 raw-decode sites inside debt_* helpers (the \
         sanctioned surface, ~56 as of 172.12 A4), got {} — scanner likely stopped \
         seeing lines/files",
        debt_hits
    );
}
