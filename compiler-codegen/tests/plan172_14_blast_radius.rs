// Plan 172.14 §7 de-risk: real (not estimated) blast-radius measurement for
// Ф.1 (auto by-ref for large read-only value-typed params). Methodology
// requires "detect-режим + blast-radius + карта сайтов" BEFORE any codegen
// change (§7 / §2.1 no-band-aid). This is a pure READ-ONLY analysis pass —
// no codegen/checker behavior is touched by this file.
//
// Scans std/, nova_tests/, spec_tests/ (relative to the workspace root, one
// level up from this crate) for every non-receiver fn/method parameter whose
// resolved type is a stack-passed value (value-record / NamedTuple / Tuple /
// FixedArray — never a heap-record/sum, which already lower to an 8-byte
// pointer per `type_size_or_align_resolved`'s boxing short-circuit), and
// classifies it against the owner-approved thresholds (172.14 plan header):
// SysV classic ≤16 bytes stays by-value; a candidate for auto by-ref is a
// param that is (a) NOT `mut`/`consume`/`variadic`/`const` (those already
// have their own ABI story — Р10 in-out ptr, ownership-move, n/a, comptime),
// and (b) resolves to a KNOWN size > 16 bytes.
//
// Per-file parsing (no cross-file peer-module resolution) is a deliberate
// V1 simplification for this recon tool: a value-type declared in a SIBLING
// file of the same folder-module resolves to `None` (tallied separately,
// NOT silently dropped) rather than a wrong guess. Run with:
//   cargo test --test plan172_14_blast_radius -- --ignored --nocapture
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nova_codegen::ast::{AllocKind, FnDecl, Item, Module, TypeDecl, TypeDeclKind, TypeRef};
use nova_codegen::const_fn_eval::{build_type_decl_registry, type_size_or_align_resolved};

fn collect_nv_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_nv_files(&path, out);
        } else if path.extension().map(|e| e == "nv").unwrap_or(false) {
            out.push(path);
        }
    }
}

fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "int" | "uint" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
            | "f32" | "f64" | "bool" | "char" | "str"
    )
}

/// True if `ty` COULD be a stack-passed value (never a heap-record/sum,
/// which the boxing short-circuit in `type_size_or_align_resolved` already
/// treats as an 8-byte pointer regardless of this check — those simply
/// report `size <= 16` below and fall out of the candidate bucket
/// naturally). Used only to separate "not a candidate by construction"
/// (primitive scalar / heap handle) from "candidate, size unknown"
/// (generic type-param / cross-file value-type) in the report.
fn is_scalar_or_pointerish(ty: &TypeRef, type_params: &std::collections::HashSet<String>) -> bool {
    match ty {
        TypeRef::Named { path, generics, .. } if generics.is_empty() && path.len() == 1 => {
            is_primitive(&path[0]) || type_params.contains(&path[0])
        }
        TypeRef::Pointer(_, _) => true,
        TypeRef::Array(_, _) => false, // []T = slice, 16B (ptr+len) — already <=16, not boxed
        _ => false,
    }
}

#[derive(Default)]
struct Tally {
    total_params_checked: usize,
    skipped_mut: usize,
    skipped_consume: usize,
    skipped_variadic: usize,
    skipped_const: usize,
    skipped_scalar_or_ptr: usize,
    size_known_le16: usize,
    size_known_gt16_candidate: usize,
    size_unknown_generic: usize,
    size_unknown_other: usize,
    value_record_decls: usize,
    named_tuple_decls: usize,
    candidate_sites: Vec<String>,
}

fn classify_fn(
    fd: &FnDecl,
    registry: &HashMap<String, TypeDecl>,
    file: &Path,
    tally: &mut Tally,
) {
    let type_params: std::collections::HashSet<String> =
        fd.generics.iter().map(|g| g.name.clone()).collect();
    for p in &fd.params {
        tally.total_params_checked += 1;
        if p.is_mut { tally.skipped_mut += 1; continue; }
        if p.consume { tally.skipped_consume += 1; continue; }
        if p.is_variadic { tally.skipped_variadic += 1; continue; }
        if p.is_const { tally.skipped_const += 1; continue; }
        if is_scalar_or_pointerish(&p.ty, &type_params) {
            tally.skipped_scalar_or_ptr += 1;
            continue;
        }
        let is_generic_dependent = type_ref_uses_type_param(&p.ty, &type_params);
        match type_size_or_align_resolved(&p.ty, false, registry) {
            Some(n) if n <= 16 => tally.size_known_le16 += 1,
            Some(n) => {
                tally.size_known_gt16_candidate += 1;
                tally.candidate_sites.push(format!(
                    "{}: fn {} param `{}` ({}B)",
                    file.display(),
                    fn_display_name(fd),
                    p.name,
                    n
                ));
            }
            None if is_generic_dependent => tally.size_unknown_generic += 1,
            None => tally.size_unknown_other += 1,
        }
    }
}

fn type_ref_uses_type_param(ty: &TypeRef, type_params: &std::collections::HashSet<String>) -> bool {
    match ty {
        TypeRef::Named { path, generics, .. } => {
            (path.len() == 1 && type_params.contains(&path[0]))
                || generics.iter().any(|g| type_ref_uses_type_param(g, type_params))
        }
        TypeRef::Array(inner, _) => type_ref_uses_type_param(inner, type_params),
        TypeRef::FixedArray(_, inner, _) => type_ref_uses_type_param(inner, type_params),
        TypeRef::Tuple(elems, _) => elems.iter().any(|e| type_ref_uses_type_param(e, type_params)),
        TypeRef::Readonly(inner, _) | TypeRef::Mut(inner, _) | TypeRef::Uninit(inner, _) => {
            type_ref_uses_type_param(inner, type_params)
        }
        _ => false,
    }
}

fn fn_display_name(fd: &FnDecl) -> String {
    if let Some(recv) = &fd.receiver {
        format!("{}::{}", recv.type_name, fd.name)
    } else {
        fd.name.clone()
    }
}

fn walk_items(items: &[Item], registry: &HashMap<String, TypeDecl>, file: &Path, tally: &mut Tally) {
    for item in items {
        match item {
            Item::Fn(fd) => classify_fn(fd, registry, file, tally),
            Item::Type(td) => {
                match &td.kind {
                    TypeDeclKind::Record(_) if td.allocation == AllocKind::Value => {
                        tally.value_record_decls += 1;
                    }
                    TypeDeclKind::NamedTuple(_) => tally.named_tuple_decls += 1,
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn scan_module(module: &Module, registry: &HashMap<String, TypeDecl>, file: &Path, tally: &mut Tally) {
    walk_items(&module.items, registry, file, tally);
    for pf in &module.peer_files {
        walk_items(&pf.items_here, registry, file, tally);
    }
}

#[test]
#[ignore] // recon tool, run explicitly: cargo test --test plan172_14_blast_radius -- --ignored --nocapture
fn plan172_14_blast_radius_report() {
    // The parser recurses on deeply-nested expr trees; the default test-thread
    // stack (smaller than main's) overflows on some large real-world .nv
    // files. Run the whole scan on a thread with a generous stack instead.
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(run_report)
        .unwrap()
        .join()
        .unwrap();
}

fn run_report() {
    let roots = ["../std", "../nova_tests", "../spec_tests"];
    let mut files = Vec::new();
    for r in roots {
        collect_nv_files(Path::new(r), &mut files);
    }
    files.sort();

    // Two-pass: (1) parse every file once, build a WHOLE-CORPUS type-decl
    // registry (name-keyed — accepts cross-file name collisions as a known
    // limitation of this recon tool; real modules ARE namespaced, this is a
    // flat approximation) so cross-file value-type params resolve instead of
    // falling into "unknown"; (2) re-walk every parsed module's fns against
    // the merged registry.
    let mut parsed: Vec<(PathBuf, Module)> = Vec::new();
    let mut parse_failures = 0usize;
    for file in &files {
        let Ok(src) = std::fs::read_to_string(file) else { continue; };
        match nova_codegen::parser::parse(&src) {
            Ok(module) => parsed.push((file.clone(), module)),
            Err(_) => parse_failures += 1,
        }
    }
    let mut global_registry: HashMap<String, TypeDecl> = HashMap::new();
    for (_, module) in &parsed {
        global_registry.extend(build_type_decl_registry(module));
    }

    let mut tally = Tally::default();
    for (file, module) in &parsed {
        scan_module(module, &global_registry, file, &mut tally);
    }

    println!("\n===== Plan 172.14 Ф.1 blast-radius (real measurement, 2026-07-10) =====");
    println!("files scanned:              {}", files.len());
    println!("parse failures (skipped):   {}", parse_failures);
    println!("value-record decls:         {}", tally.value_record_decls);
    println!("named-tuple decls:          {}", tally.named_tuple_decls);
    println!("---");
    println!("total non-receiver params checked: {}", tally.total_params_checked);
    println!("  skipped (mut, Р10 in-out ptr):    {}", tally.skipped_mut);
    println!("  skipped (consume):                {}", tally.skipped_consume);
    println!("  skipped (variadic):                {}", tally.skipped_variadic);
    println!("  skipped (const, comptime):          {}", tally.skipped_const);
    println!("  skipped (scalar/pointer/typaram):    {}", tally.skipped_scalar_or_ptr);
    println!("  size known, <=16B (SysV by-value):   {}", tally.size_known_le16);
    println!("  size known, >16B — Ф.1 CANDIDATE:    {}", tally.size_known_gt16_candidate);
    println!("  size unknown (generic type-param):   {}", tally.size_unknown_generic);
    println!("  size unknown (cross-file/other):     {}", tally.size_unknown_other);
    println!("---");
    println!("Ф.1 candidate sites ({}):", tally.candidate_sites.len());
    for site in &tally.candidate_sites {
        println!("  {}", site);
    }
}
