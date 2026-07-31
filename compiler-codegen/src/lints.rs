//! Lint-проходы по AST.
//!
//! Lint — это **warning**, не error: компилятор возвращает Diagnostic'и,
//! но компиляция продолжается. CLI решает выводить ли их (по умолчанию
//! да; `--no-lint` отключает). В отличие от parser/typecheck-error'ов,
//! lints программист может игнорировать.
//!
//! Текущие правила:
//!  - `export-fail-untyped`: `export fn ... Fail -> ...` без `[E]` —
//!    warning. Public API должен иметь typed Fail (D65 convention).

use crate::ast::{
    ArrayElem, Block, CallArg, ClosureBody, ConstDecl, ElseBranch, Expr, ExprKind, FnBody, FnDecl,
    HandlerMethodBody, Import, Item, MatchArm, MatchArmBody, Module, Pattern, ReceiverKind,
    Stmt, TypeDeclKind, TypeRef, VariantPatternKind,
};
use crate::diag::{byte_to_line_col, Applicability, Diagnostic, Span, Suggestion};
use std::collections::{HashMap, HashSet};

/// Один lint-warning.
#[derive(Debug, Clone)]
pub struct LintWarning {
    pub rule: &'static str,
    pub diag: Diagnostic,
}

// Plan 173 Ф.1 (#4, [M-172-errdefer-okdefer-dead-surface]): D189-deprecation
// lint (`lint_deprecated_defer_family` + walkers) УДАЛЁН. Это был pre-removal
// warning для okdefer/errdefer/defer|result|; после hard cutover (D189) парсер
// отвергает эти формы на месте (`[D189-removed-*]`), поэтому lint никогда не
// срабатывал — мёртвая поверхность.

/// Прогон всех lint-проверок на модуле. Возвращает список warning'ов.
pub fn lint_module(m: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    let effect_names = collect_effect_names(m);
    let protocol_names = collect_protocol_names(m);

    // Plan 90.1 Ф.5: module-level suppress for W_VIEW_EXTEND_DETACH.
    let view_extend_suppressed = m.attrs.iter()
        .any(|a| matches!(a.kind, crate::ast::ModuleAttrKind::AllowViewExtendDetach));
    for item in &m.items {
        match item {
            Item::Fn(f) => {
                check_fn(f, &mut warnings);
                check_assume_trust(f, &mut warnings);
                check_assert_static_unverified(f, &mut warnings);
                check_protocol_in_effect_position(f, &protocol_names, &effect_names, &mut warnings);
                // [M-canon-mut-param-position] (2026-07-17): W_PARAM_TYPE_POS_MUT —
                // unconditional pipeline (owner decision: NOT opt-in CONV_RULES),
                // runs on every fn signature like the other per-fn checks above.
                check_param_type_pos_mut(f, &mut warnings);
                // Plan 96.1 Ф.1: W_VIEW_PUSH_DETACH — warning при push на
                // slice-view binding (let X = arr[range]; X.push(...)).
                lint_view_push_detach(f, &mut warnings);
                // Plan 90.1 Ф.5 (D141 amendment): W_VIEW_EXTEND_DETACH — warning при вызове
                // grow-метода (append/insert/reserve) на parent-массиве
                // после создания slice-view из него.
                lint_view_extend_detach(f, view_extend_suppressed, &mut warnings);
                // Владелец 2026-07-21: `new-then-cap` — `X.new()` сразу
                // следом `.cap(n)` на том же binding (или chain-форма
                // `X.new().cap(n)`) → warning, canon = `X.new(cap: n)`.
                lint_new_then_cap(f, &mut warnings);
                // Plan 52 Ф.2: map-литерал lints (dup-key, NaN-key) —
                // требуют обхода выражений внутри тела функции.
                match &f.body {
                    FnBody::Expr(e) => walk_expr_lints(e, &mut warnings),
                    FnBody::Block(b) => walk_block_lints(b, &mut warnings),
                    FnBody::External => {}
                }
            }
            Item::Test(t) => {
                walk_block_lints(&t.body, &mut warnings);
            }
            // Plan 57: lint обходит все три раздела bench.
            Item::Bench(b) => {
                for s in &b.setup {
                    walk_stmt_lints(s, &mut warnings);
                }
                walk_block_lints(&b.measure_body, &mut warnings);
                for s in &b.teardown {
                    walk_stmt_lints(s, &mut warnings);
                }
                // Plan 57.C.7: bench-specific lint warnings внутри measure body.
                walk_bench_measure_lints(&b.measure_body, &b.name, &mut warnings);
                // Plan 57.C.7: empty measure body warning.
                if b.measure_body.stmts.is_empty() && b.measure_body.trailing.is_none() {
                    warnings.push(LintWarning {
                        rule: "bench-empty-measure",
                        diag: crate::diag::Diagnostic::new(
                            format!("bench \"{}\": empty `measure` block — no work \
                                     to measure, results will reflect только overhead",
                                b.name),
                            b.measure_body.span,
                        ),
                    });
                }
                // Group cases — same checks per case.
                for grp in &b.groups {
                    for case in &grp.cases {
                        for s in &case.setup {
                            walk_stmt_lints(s, &mut warnings);
                        }
                        walk_block_lints(&case.measure_body, &mut warnings);
                        for s in &case.teardown {
                            walk_stmt_lints(s, &mut warnings);
                        }
                        let label = format!("{}/{}/{}", b.name, grp.name, case.name);
                        walk_bench_measure_lints(&case.measure_body, &label, &mut warnings);
                        if case.measure_body.stmts.is_empty() && case.measure_body.trailing.is_none() {
                            warnings.push(LintWarning {
                                rule: "bench-empty-measure",
                                diag: crate::diag::Diagnostic::new(
                                    format!("case \"{}\": empty `measure` block", label),
                                    case.measure_body.span,
                                ),
                            });
                        }
                    }
                }
            }
            Item::Const(c) => walk_expr_lints(&c.value, &mut warnings),
            Item::Let(l) => walk_expr_lints(&l.value, &mut warnings),
            Item::Type(_) => {}
            // Plan 33.3 Ф.13: lemma — spec-only, эрейзится в codegen.
            Item::Lemma(_) => {}
        }
    }
    // Plan 62.F.bis Ф.2: structured W_PRELUDE_SHADOW warnings — extends
    // basic eprintln из 62.D bis-1 (types/mod.rs::check_module) на
    // структурированную форму с suppress-clause `module X
    // allow_prelude_shadow`. Emitted в общий warnings Vec — surfaces через
    // `cmd_check` warnings field (то же что и другие lints).
    warnings.extend(lint_prelude_shadow(m));
    // Plan 81 Ф.4: неиспользуемые импорты.
    warnings.extend(lint_unused_imports(m));
    // Plan 118 Ф.5 A22 (D216 §7): W_OPTION_DOUBLE_NESTED — nested
    // Option[Option[*T|ptr]] ambiguous под NPO codegen.
    warnings.extend(lint_option_double_nested(m));
    // Plan 110.9.4 (D188 amend): W_FFI_CANCEL_UNSAFE — non-cancel_safe FFI
    // call inside cleanup body. Stdlib types implementing Cleanup must
    // annotate native cleanup external fns с `#cancel_safe`.
    warnings.extend(lint_ffi_cancel_unsafe(m));
    // Plan 127 Ф.4: W_VALUE_RECORD_UNNECESSARY_PROMOTE — value-record local
    // detected to escape (auto-promoted to heap) when the fn signature could
    // have returned the value by-value instead. Suggests user-visible change
    // to drop the `*` from return type. Lint, not error.
    warnings.extend(lint_value_record_unnecessary_promote(m));
    warnings
}

// ============================================================================
// Plan 127 Ф.4: W_VALUE_RECORD_UNNECESSARY_PROMOTE.
//
// Когда value-record local участвует в `&v` escape (auto-promote → heap),
// fn-signature остаётся user-visible: `-> *Vec3`. Если все escape-points
// fn'a — это `return &v` (return-position only, без heap-field store /
// closure capture), пользователь мог бы изменить signature на `-> Vec3`
// и вернуть `v` напрямую — без promote'а. Lint suggests эту замену.
//
// Conservative V1 trigger: fn has promoted local AND fn return-type is
// `*<ValueRecord>`. Suppressed for compiler-synthesized FnDecls (Plan 126
// @clone / @equals / @hash bodies, identified by `Span::dummy()` spans).
// ============================================================================

fn lint_value_record_unnecessary_promote(m: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    // Run escape analysis once для query interface.
    let escape = crate::escape_analyze::analyze_module(m);
    if !escape.has_any_promoted() { return warnings; }
    // Visit each fn-item; if escape analyzer flagged any local, check signature
    // shape для unnecessary-promote heuristic.
    let visit_fn = |fd: &FnDecl, warnings: &mut Vec<LintWarning>| {
        // Suppress на synthesized fns (Plan 126 auto-derive emits FnDecls с
        // dummy spans — they are not user-controllable код).
        if fd.span.start == 0 && fd.span.end == 0 { return; }
        // External fns не имеют body, escape пройдёт мимо.
        if matches!(fd.body, FnBody::External) { return; }
        let fn_key = if let Some(recv) = &fd.receiver {
            format!("{}::{}", recv.type_name, fd.name)
        } else {
            fd.name.clone()
        };
        // Plan 127 V1 heuristic: signature returns `*X` где X — value-record.
        // Если так — by-value return альтернативой possible (suggest to user).
        let Some(rt) = &fd.return_type else { return };
        let inner_value_record_name = match rt {
            TypeRef::Pointer(inner, _) => {
                if let TypeRef::Named { path, .. } = inner.as_ref() {
                    path.last().cloned()
                } else { None }
            }
            _ => None,
        };
        let Some(vrt_name) = inner_value_record_name else { return };
        // Confirm vrt_name actually points к value-record TypeDecl в модуле.
        let mut is_value_record = false;
        let check_items = |items: &[Item], flag: &mut bool| {
            for it in items {
                if let Item::Type(td) = it {
                    if td.name == vrt_name && matches!(td.kind, crate::ast::TypeDeclKind::Record(_))
                        && td.allocation == crate::ast::AllocKind::Value
                    {
                        *flag = true;
                        break;
                    }
                }
            }
        };
        check_items(&m.items, &mut is_value_record);
        for pf in &m.peer_files {
            if is_value_record { break; }
            check_items(&pf.items_here, &mut is_value_record);
        }
        if !is_value_record { return; }
        // Final gate: escape walker flagged at least one local in this fn'a.
        // We don't know exactly which body construction triggered promote —
        // V1 conservative emit на сам fn-signature span.
        // The lint-key check uses promoted_per_fn via has_any_promoted-style
        // probe; we re-check via dedicated method.
        let mut any_promoted_here = false;
        // Plan 127 escape_analyze keys fn-ids identical to fn_key построения
        // выше. Probe through public is_promoted с пустым local — но since
        // EscapeResult API requires local name, мы используем total_count
        // proxy. Simpler: re-walk через peer-search.
        for (key, set) in escape_per_fn_iter(&escape) {
            if key == fn_key && !set.is_empty() {
                any_promoted_here = true;
                break;
            }
        }
        if !any_promoted_here { return; }
        warnings.push(LintWarning {
            rule: "W_VALUE_RECORD_UNNECESSARY_PROMOTE",
            diag: Diagnostic::new(
                format!(
                    "[W_VALUE_RECORD_UNNECESSARY_PROMOTE] fn `{}` returns `*{}` \
                     where `{}` is a value-record, and at least one local in this \
                     fn was auto-promoted to heap to satisfy escape via `&v`. \
                     Consider returning `{}` by-value (drop the `*` from the \
                     return type) so the local stays on the stack and no heap \
                     allocation is required. Auto-promote is correct but may \
                     incur unnecessary heap pressure. (Plan 127 V1 heuristic — \
                     see D228 §«escape & auto-promote».)",
                    fd.name, vrt_name, vrt_name, vrt_name
                ),
                fd.span,
            ),
        });
    };
    for item in &m.items {
        if let Item::Fn(fd) = item { visit_fn(fd, &mut warnings); }
    }
    for pf in &m.peer_files {
        for item in &pf.items_here {
            if let Item::Fn(fd) = item { visit_fn(fd, &mut warnings); }
        }
    }
    warnings
}

/// Helper: enumerate (fn_id, promoted_set) entries из EscapeResult. Wraps
/// private field access; alternative — add accessor к escape_analyze::
/// EscapeResult. V1 minimal: use total_promoted_count + per-fn probe API
/// иначе reconstruct via re-analysis.
fn escape_per_fn_iter(escape: &crate::escape_analyze::EscapeResult)
    -> Vec<(String, Vec<String>)>
{
    // V1: re-extract through public probe interface. EscapeResult is
    // small + immutable post-construction; we add a public accessor для
    // ergonomic iteration в Ф.4 closeout follow-up.
    escape.iter_promoted().map(|(k, v)| (k.clone(), v.iter().cloned().collect())).collect()
}

// ============================================================================
// Plan 110.9.4 [M-110.9.4-ffi-cancel-unsafe-lint]: W_FFI_CANCEL_UNSAFE.
//
// При invoke external fn БЕЗ `#cancel_safe` attribute inside ConsumeScope's
// `cleanup` method body — warning. Rationale: under cancel-shield deadline
// model (Plan 110.2 D188 R3), cancellation может deliver inside cleanup
// body; native fns без attestation могут блок'нуть или crash. `#cancel_safe`
// attribute attests C-side function is safe to invoke в этом scenario.
//
// Conservative: only fires when callee is plain Ident referencing external
// fn in module's own item registry. Cross-module external fns require
// import + name resolution which lint pass не выполняет.
// ============================================================================

fn lint_ffi_cancel_unsafe(m: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    use std::collections::HashMap as Map;
    // Step 1: build external fn registry: name → cancel_safe_attr.
    let mut external_fns: Map<String, bool> = Map::new();
    let mut collect = |fd: &FnDecl| {
        if fd.is_external {
            external_fns.insert(fd.name.clone(), fd.cancel_safe_attr);
        }
    };
    for item in &m.items {
        if let Item::Fn(fd) = item { collect(fd); }
    }
    for pf in &m.peer_files {
        for item in &pf.items_here {
            if let Item::Fn(fd) = item { collect(fd); }
        }
    }
    if external_fns.is_empty() { return warnings; }
    // Step 2: find cleanup methods (Item::Fn with receiver и name == "cleanup"),
    // walk их body looking for external-fn calls.
    let mut visit_fn = |fd: &FnDecl| {
        if fd.receiver.is_some() && fd.name == "cleanup" {
            match &fd.body {
                FnBody::Expr(e) => walk_expr_for_cancel_unsafe(e, &external_fns, &mut warnings),
                FnBody::Block(b) => walk_block_for_cancel_unsafe(b, &external_fns, &mut warnings),
                FnBody::External => {}
            }
        }
    };
    for item in &m.items {
        if let Item::Fn(fd) = item { visit_fn(fd); }
    }
    for pf in &m.peer_files {
        for item in &pf.items_here {
            if let Item::Fn(fd) = item { visit_fn(fd); }
        }
    }
    warnings
}

fn walk_block_for_cancel_unsafe(
    b: &Block,
    external_fns: &std::collections::HashMap<String, bool>,
    warnings: &mut Vec<LintWarning>,
) {
    for s in &b.stmts {
        walk_stmt_for_cancel_unsafe(s, external_fns, warnings);
    }
    if let Some(t) = &b.trailing {
        walk_expr_for_cancel_unsafe(t, external_fns, warnings);
    }
}

fn walk_stmt_for_cancel_unsafe(
    s: &Stmt,
    external_fns: &std::collections::HashMap<String, bool>,
    warnings: &mut Vec<LintWarning>,
) {
    match s {
        Stmt::Let(d) => walk_expr_for_cancel_unsafe(&d.value, external_fns, warnings),
        Stmt::Const(d) => walk_expr_for_cancel_unsafe(&d.value, external_fns, warnings),
        Stmt::Expr(e) => walk_expr_for_cancel_unsafe(e, external_fns, warnings),
        Stmt::Assign { target, value, .. } => {
            walk_expr_for_cancel_unsafe(target, external_fns, warnings);
            walk_expr_for_cancel_unsafe(value, external_fns, warnings);
        }
        Stmt::Return { value: Some(v), .. } => walk_expr_for_cancel_unsafe(v, external_fns, warnings),
        Stmt::Throw { value, .. } => walk_expr_for_cancel_unsafe(value, external_fns, warnings),
        Stmt::Defer { body, .. } => walk_expr_for_cancel_unsafe(body, external_fns, warnings),
        Stmt::ConsumeScope { init, body, .. } => {
            walk_expr_for_cancel_unsafe(init, external_fns, warnings);
            walk_block_for_cancel_unsafe(body, external_fns, warnings);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            walk_expr_for_cancel_unsafe(expr, external_fns, warnings);
        }
        _ => {}
    }
}

fn walk_expr_for_cancel_unsafe(
    e: &Expr,
    external_fns: &std::collections::HashMap<String, bool>,
    warnings: &mut Vec<LintWarning>,
) {
    if let ExprKind::Call { func, args, trailing } = &e.kind {
        // Detect bare-Ident callee referencing external fn без #cancel_safe.
        if let ExprKind::Ident(name) = &func.kind {
            if let Some(cancel_safe) = external_fns.get(name) {
                if !cancel_safe {
                    warnings.push(LintWarning {
                        rule: "W_FFI_CANCEL_UNSAFE",
                        diag: Diagnostic::new(
                            format!(
                                "[W_FFI_CANCEL_UNSAFE] call to external fn `{}` from \
                                 within `cleanup` body without `#cancel_safe` attribute \
                                 (Plan 110.9.4 / D188 R3). Under cancel-shield deadline \
                                 cancellation may be delivered inside cleanup; native \
                                 functions without attestation may block, deadlock, or \
                                 crash. If `{}` is verified safe to invoke under \
                                 cancel-shield (e.g., closes/frees that complete bounded \
                                 time, does not acquire shared locks), add `#cancel_safe` \
                                 to its `external fn` declaration. Otherwise wrap the \
                                 call в a separate fiber spawned before the consume \
                                 scope.",
                                name, name
                            ),
                            e.span,
                        ),
                    });
                }
            }
        }
        walk_expr_for_cancel_unsafe(func, external_fns, warnings);
        for a in args {
            match a {
                crate::ast::CallArg::Item(x) | crate::ast::CallArg::Spread(x) => {
                    walk_expr_for_cancel_unsafe(x, external_fns, warnings);
                }
                crate::ast::CallArg::Named { value, .. } => {
                    walk_expr_for_cancel_unsafe(value, external_fns, warnings);
                }
            }
        }
        if let Some(t) = trailing {
            match t {
                crate::ast::Trailing::Block(b) => walk_block_for_cancel_unsafe(b, external_fns, warnings),
                crate::ast::Trailing::Fn(sb) => match &sb.body {
                    FnBody::Expr(e) => walk_expr_for_cancel_unsafe(e, external_fns, warnings),
                    FnBody::Block(b) => walk_block_for_cancel_unsafe(b, external_fns, warnings),
                    FnBody::External => {}
                },
                crate::ast::Trailing::LegacyBlockWithParams(tb) => walk_block_for_cancel_unsafe(&tb.body, external_fns, warnings),
            }
        }
        return;
    }
    // Recurse into other expr kinds.
    match &e.kind {
        ExprKind::Unary { operand, .. } => walk_expr_for_cancel_unsafe(operand, external_fns, warnings),
        ExprKind::Binary { left, right, .. } => {
            walk_expr_for_cancel_unsafe(left, external_fns, warnings);
            walk_expr_for_cancel_unsafe(right, external_fns, warnings);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => walk_expr_for_cancel_unsafe(x, external_fns, warnings),
        ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => walk_expr_for_cancel_unsafe(x, external_fns, warnings),
        ExprKind::Coalesce(a, b) => {
            walk_expr_for_cancel_unsafe(a, external_fns, warnings);
            walk_expr_for_cancel_unsafe(b, external_fns, warnings);
        }
        ExprKind::Member { obj, .. } => walk_expr_for_cancel_unsafe(obj, external_fns, warnings),
        ExprKind::Index { obj, index } => {
            walk_expr_for_cancel_unsafe(obj, external_fns, warnings);
            walk_expr_for_cancel_unsafe(index, external_fns, warnings);
        }
        ExprKind::TurboFish { base, .. } => walk_expr_for_cancel_unsafe(base, external_fns, warnings),
        ExprKind::If { cond, then, else_ } => {
            walk_expr_for_cancel_unsafe(cond, external_fns, warnings);
            walk_block_for_cancel_unsafe(then, external_fns, warnings);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block_for_cancel_unsafe(b, external_fns, warnings),
                    ElseBranch::If(ie) => walk_expr_for_cancel_unsafe(ie, external_fns, warnings),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_for_cancel_unsafe(scrutinee, external_fns, warnings);
            for arm in arms {
                if let Some(g) = &arm.guard { walk_expr_for_cancel_unsafe(g, external_fns, warnings); }
                match &arm.body {
                    MatchArmBody::Expr(e) => walk_expr_for_cancel_unsafe(e, external_fns, warnings),
                    MatchArmBody::Block(b) => walk_block_for_cancel_unsafe(b, external_fns, warnings),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            walk_expr_for_cancel_unsafe(iter, external_fns, warnings);
            walk_block_for_cancel_unsafe(body, external_fns, warnings);
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr_for_cancel_unsafe(cond, external_fns, warnings);
            walk_block_for_cancel_unsafe(body, external_fns, warnings);
        }
        ExprKind::Loop { body, .. } => walk_block_for_cancel_unsafe(body, external_fns, warnings),
        ExprKind::Block(b) => walk_block_for_cancel_unsafe(b, external_fns, warnings),
        ExprKind::TupleLit(items) => for x in items { walk_expr_for_cancel_unsafe(x, external_fns, warnings); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el {
                ArrayElem::Item(x) | ArrayElem::Spread(x) => walk_expr_for_cancel_unsafe(x, external_fns, warnings),
            }
        },
        _ => {}
    }
}

// ============================================================================
// Plan 118 Ф.5 A22 (D216 §7): W_OPTION_DOUBLE_NESTED.
//
// `Option[Option[*T]]` / `Option[Option[ptr]]` — nested Option pattern
// под NPO codegen ambiguous: inner Option benefits from NPO (single
// pointer); outer Option falls в tagged form (inner c_ty = struct, not
// pointer). Semantically `None` vs `Some(None)` оба distinct legal
// states но user-facing semantics confusing.
//
// Warning suggests:
//  - unwrap к single Option[*T] if both-None semantics не distinguishable
//  - use `Result[*T, MyError]` for distinct "absent" vs "error" cases
// ============================================================================

/// Recursive check: returns true если TypeRef есть `Option[Option[<ptr-like>]]`.
fn typeref_is_nested_option_ptr(tr: &TypeRef) -> bool {
    if let TypeRef::Named { path, generics, .. } = tr {
        if path.last().map_or(false, |n| n == "Option") && generics.len() == 1 {
            // Inner is Option[X] — check X for pointer-like.
            if let TypeRef::Named { path: ipath, generics: igen, .. } = &generics[0] {
                if ipath.last().map_or(false, |n| n == "Option") && igen.len() == 1 {
                    // Check innermost — *T or ptr?
                    return is_pointer_like(&igen[0]);
                }
            }
        }
    }
    false
}

/// True for pointer-like TypeRef:
/// - `TypeRef::Pointer(*T)` (Plan 118)
/// - `TypeRef::Named { path: ["ptr"] }` (Plan 115)
fn is_pointer_like(tr: &TypeRef) -> bool {
    match tr {
        TypeRef::Pointer(..) => true,
        // Plan 118.5 D216 V2: Mut/Unsafe — transparent wrappers; recurse.
        TypeRef::Mut(inner, _) | TypeRef::Uninit(inner, _) => is_pointer_like(inner),
        TypeRef::Named { path, .. } => path.last().map_or(false, |n| n == "ptr"),
        _ => false,
    }
}

/// Walk TypeRef recursively, emit W_OPTION_DOUBLE_NESTED для каждого
/// nested-Option-ptr matched site.
fn walk_typeref_for_a22(tr: &TypeRef, warnings: &mut Vec<LintWarning>) {
    if typeref_is_nested_option_ptr(tr) {
        let span = match tr {
            TypeRef::Named { span, .. } => *span,
            TypeRef::Array(_, span) => *span,
            TypeRef::FixedArray(_, _, span) => *span,
            TypeRef::Tuple(_, span) => *span,
            TypeRef::Func { span, .. } => *span,
            TypeRef::Protocol { span, .. } => *span,
            TypeRef::Unit(span) => *span,
            TypeRef::Readonly(_, span) => *span,
            // Plan 118.5 D216 V2: Pointer is 2-tuple; Mut/Unsafe — wrappers.
            TypeRef::Pointer(_, span)
            | TypeRef::Mut(_, span)
            | TypeRef::Uninit(_, span)
            | TypeRef::Ref(_, span) => *span,
        };
        warnings.push(LintWarning {
            rule: "W_OPTION_DOUBLE_NESTED",
            diag: Diagnostic::new(
                "[W_OPTION_DOUBLE_NESTED] `Option[Option[*T|ptr]]` nested Option \
                 ambiguous под NPO codegen (Plan 118 D216 §7). Inner Option benefits \
                 from NPO (single-pointer layout); outer Option falls в tagged repr \
                 (inner c_ty = struct, не pointer). Semantically `None` vs \
                 `Some(None)` оба distinct legal states но difficult к distinguish. \
                 Suggest: либо collapse к `Option[*T]` (если both-None semantics \
                 не distinguishable), либо `Result[*T, E]` для distinct \
                 \"absent\" vs \"error\" cases.".to_string(),
                span,
            ),
        });
    }
    // Recurse в child TypeRef-ах.
    match tr {
        TypeRef::Named { generics, .. } => {
            for g in generics { walk_typeref_for_a22(g, warnings); }
        }
        TypeRef::Array(inner, _)
        | TypeRef::FixedArray(_, inner, _)
        | TypeRef::Readonly(inner, _)
        // Plan 118.5 D216 V2: Pointer 2-tuple + Mut/Unsafe transparent wrappers.
        | TypeRef::Pointer(inner, _)
        | TypeRef::Mut(inner, _)
        | TypeRef::Uninit(inner, _)
        | TypeRef::Ref(inner, _) => walk_typeref_for_a22(inner, warnings),
        TypeRef::Tuple(items, _) => {
            for it in items { walk_typeref_for_a22(it, warnings); }
        }
        TypeRef::Func { params, return_type, .. } => {
            for p in params { walk_typeref_for_a22(p, warnings); }
            if let Some(rt) = return_type { walk_typeref_for_a22(rt, warnings); }
        }
        TypeRef::Protocol { methods, .. } => {
            for m in methods {
                for p in &m.params { walk_typeref_for_a22(&p.ty, warnings); }
                if let Some(rt) = &m.return_type { walk_typeref_for_a22(rt, warnings); }
            }
        }
        TypeRef::Unit(_) => {}
    }
}

fn lint_option_double_nested(m: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    for item in &m.items {
        match item {
            Item::Fn(f) => {
                // Fn signature: params + return + effects.
                for p in &f.params { walk_typeref_for_a22(&p.ty, &mut warnings); }
                if let Some(rt) = &f.return_type {
                    walk_typeref_for_a22(rt, &mut warnings);
                }
                for e in &f.effects { walk_typeref_for_a22(e, &mut warnings); }
            }
            Item::Type(t) => {
                // Type decl bodies — record fields, sum variants, newtype inner.
                match &t.kind {
                    TypeDeclKind::Record(fields) => {
                        for f in fields { walk_typeref_for_a22(&f.ty, &mut warnings); }
                    }
                    TypeDeclKind::Sum(variants) => {
                        for v in variants {
                            match &v.kind {
                                crate::ast::SumVariantKind::Tuple(tys) => {
                                    for ty in tys { walk_typeref_for_a22(ty, &mut warnings); }
                                }
                                crate::ast::SumVariantKind::Record(fields) => {
                                    for f in fields { walk_typeref_for_a22(&f.ty, &mut warnings); }
                                }
                                crate::ast::SumVariantKind::Unit => {}
                            }
                        }
                    }
                    TypeDeclKind::Newtype(inner) => walk_typeref_for_a22(inner, &mut warnings),
                    TypeDeclKind::Alias(inner) => walk_typeref_for_a22(inner, &mut warnings),
                    _ => {}
                }
            }
            Item::Const(c) => {
                if let Some(ty) = &c.ty { walk_typeref_for_a22(ty, &mut warnings); }
            }
            Item::Let(l) => {
                if let Some(ty) = &l.ty { walk_typeref_for_a22(ty, &mut warnings); }
            }
            _ => {}
        }
    }
    warnings
}

// ============================================================================
// Plan 81 Ф.4: unused-import lint.
//
// Per-peer: имена, привнесённые `import`'ом этого peer-файла, должны
// использоваться в его `items_here` (per-peer isolation — Plan 42.15
// Rule C). Неиспользуемые → warning `unused-import`. По умолчанию
// warning; opt-in error через `nova.toml` — на уровне CLI (--strict).
// ============================================================================

/// Имена, которые `import` делает видимыми в peer-файле.
fn import_brought_names(imp: &Import) -> Vec<String> {
    match &imp.items {
        // Селективный `import X.{A, B as C}` — видимы final-имена.
        Some(items) => items
            .iter()
            .map(|it| it.alias.clone().unwrap_or_else(|| it.name.clone()))
            .collect(),
        // Whole-module `import X` / `import X as a` — виден module-prefix.
        None => {
            if let Some(a) = &imp.alias {
                vec![a.clone()]
            } else if let Some(last) = imp.path.last() {
                vec![last.clone()]
            } else {
                Vec::new()
            }
        }
    }
}

/// `true` если import — авто-prelude (`std.prelude*`): он неявный, его
/// нельзя пометить «unused».
fn is_prelude_import(imp: &Import) -> bool {
    matches!(imp.path.first().map(String::as_str), Some("std"))
        && imp.path.get(1).map(String::as_str) == Some("prelude")
}

fn lint_unused_imports(m: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    if m.peer_files.is_empty() {
        // Pre-resolution / single-file без populated peer_files — flat.
        check_imports_unused(&m.imports, &m.items, &mut warnings);
    } else {
        // Per-peer (Plan 42.15 Rule C — импорты изолированы по peer'ам),
        // но ТОЛЬКО ENTRY-модуля собственные co-equal peer'ы
        // (`pf.is_entry_module`) — [M-lint-phantom-prelude-unused-import]
        // (владелец 2026-07-31, репро nova-bigint/src/bigint.nv).
        //
        // `m.peer_files` после `resolve_imports_inline_ex` — это ПОЛНЫЙ
        // транзитивный import-граф, инлайненный в один `Module` для
        // cross-file type-check (каждый транзитивно затянутый модуль —
        // включая `std.collections.vec`/`hashmap`/`set`/`raw_mem` через
        // авто-prelude — пушит СВОИ peer-файлы в тот же плоский вектор,
        // см. `imports.rs::resolve_imports_inline_ex`). Без фильтра этот
        // цикл линтовал unused-import ЧУЖИХ модулей (их собственная
        // импорт-гигиена — забота ИХ ЛИНТА, не файла, который их всего
        // лишь транзитивно использует) и вешал находки на спаны entry-
        // файла: имена вроде `Vec`/`HashMap`/`RawMem`/`VecIter`/`Set`
        // (prelude-реэкспорт → `std/collections/vec/*.nv` и соседи), КОТОРЫХ
        // проверяемый файл вообще не импортирует, репортились как «unused
        // import» этого файла — 7 фантомов на `bigint.nv`, при этом файл
        // не импортирует НИ ОДНО из них (см. маркер).
        //
        // `is_entry_module` уже ровно этот фильтр в соседних lint-проходах
        // (`collect_prelude_visibility`-consumer выше, `escape_analyze.rs`)
        // — тот же идиом, применяем его и здесь.
        for pf in m.peer_files.iter().filter(|pf| pf.is_entry_module) {
            check_imports_unused(&pf.imports, &pf.items_here, &mut warnings);
        }
    }
    warnings
}

fn check_imports_unused(
    imports: &[Import],
    items: &[Item],
    warnings: &mut Vec<LintWarning>,
) {
    let mut used: HashSet<String> = HashSet::new();
    collect_used_names(items, &mut used);
    for imp in imports {
        // `export import` — re-export: имена и есть API, «используются».
        if imp.is_export || is_prelude_import(imp) {
            continue;
        }
        // Whole-module `import X` делает доступным И prefix `X`, И —
        // через Plan 35 merge — все экспортируемые имена модуля X как
        // bare-имена. Достоверно определить «не использован» нельзя без
        // резолва экспортов X → не линтуем (иначе ложные срабатывания
        // на bare-использовании). Селективный `import X.{A, B}` несёт
        // точно известный набор имён.
        if imp.items.is_none() {
            continue;
        }
        for name in import_brought_names(imp) {
            if !used.contains(&name) {
                warnings.push(LintWarning {
                    rule: "unused-import",
                    diag: crate::diag::Diagnostic::new(
                        format!(
                            "unused import `{}` — imported but never \
                             referenced in this file",
                            name,
                        ),
                        imp.span,
                    ),
                });
            }
        }
    }
}

/// Plan 81 Ф.4: собрать все имена-ссылки в items (для unused-import).
/// Plan 81 Ф.7.2: также используется codegen'ом для reachability-DCE —
/// `pub(crate)`. Полный обход AST (все expr/stmt/type-позиции).
pub(crate) fn collect_used_names(items: &[Item], out: &mut HashSet<String>) {
    for item in items {
        match item {
            Item::Fn(f) => {
                for g in &f.generics {
                    for b in &g.bounds {
                        collect_tr(b, out);
                    }
                    if let Some(d) = &g.default {
                        collect_tr(d, out);
                    }
                }
                for p in &f.params {
                    collect_tr(&p.ty, out);
                    if let Some(dv) = &p.default {
                        collect_expr(dv, out);
                    }
                }
                if let Some(rt) = &f.return_type {
                    collect_tr(rt, out);
                }
                for e in &f.effects {
                    collect_tr(e, out);
                }
                for c in &f.contracts {
                    collect_expr(&c.expr, out);
                    // Plan 159 Ф.2 (DCE soundness): an interpolated contract
                    // message `requires x > 0, "got ${x}"` desugars to an
                    // `InterpolatedStr` whose StringBuilder / Display / value→str
                    // converter selectors are injected at the violation site and
                    // never appear syntactically elsewhere. Walk it so those
                    // selectors are seeded — otherwise method-DCE prunes the
                    // converter and codegen emits `nova_str` ← `int` (CC-FAIL).
                    if let Some(me) = &c.message_expr {
                        collect_expr(me, out);
                    }
                }
                match &f.body {
                    FnBody::Expr(e) => collect_expr(e, out),
                    FnBody::Block(b) => collect_block(b, out),
                    FnBody::External => {}
                }
            }
            Item::Type(td) => {
                for g in &td.generics {
                    for b in &g.bounds {
                        collect_tr(b, out);
                    }
                    if let Some(d) = &g.default {
                        collect_tr(d, out);
                    }
                }
                match &td.kind {
                    TypeDeclKind::Record(fields) => {
                        for fld in fields {
                            collect_tr(&fld.ty, out);
                        }
                    }
                    TypeDeclKind::Sum(variants) => {
                        for v in variants {
                            match &v.kind {
                                crate::ast::SumVariantKind::Unit => {}
                                crate::ast::SumVariantKind::Tuple(tys) => {
                                    for t in tys {
                                        collect_tr(t, out);
                                    }
                                }
                                crate::ast::SumVariantKind::Record(fields) => {
                                    for fld in fields {
                                        collect_tr(&fld.ty, out);
                                    }
                                }
                            }
                        }
                    }
                    TypeDeclKind::Effect(methods) => {
                        for mth in methods {
                            for p in &mth.params {
                                collect_tr(&p.ty, out);
                            }
                            if let Some(rt) = &mth.return_type {
                                collect_tr(rt, out);
                            }
                            for e in &mth.effects {
                                collect_tr(e, out);
                            }
                        }
                    }
                    TypeDeclKind::Protocol { methods, embeds } => {
                        for mth in methods {
                            for p in &mth.params {
                                collect_tr(&p.ty, out);
                            }
                            if let Some(rt) = &mth.return_type {
                                collect_tr(rt, out);
                            }
                            for e in &mth.effects {
                                collect_tr(e, out);
                            }
                        }
                        // Plan 101.4: embedded protocols reference other named types
                        for e in embeds {
                            collect_tr(e, out);
                        }
                    }
                    // Plan 120 (D215): collect type refs from named tuple fields.
                    TypeDeclKind::NamedTuple(fields) => {
                        for f in fields {
                            collect_tr(&f.ty, out);
                        }
                    }
                    TypeDeclKind::Newtype(tr) | TypeDeclKind::Alias(tr) => {
                        collect_tr(tr, out)
                    }
                    // Plan 172.3 (D310): collect type-set member type-refs so they
                    // count as used (no false unused-import on a set's members).
                    TypeDeclKind::TypeSet(members) => {
                        for m in members {
                            collect_tr(m, out);
                        }
                    }
                    TypeDeclKind::Opaque => {}
                }
                // Plan 159 Ф.2 (DCE soundness): record `invariant <expr>,
                // "...${field}..."` clauses. The check-site interpolation of the
                // message is generated by the invariant desugar and its
                // StringBuilder / Display / value→str converter selectors never
                // appear syntactically — walk both the condition and the
                // interpolated message expr so method-DCE keeps the converter
                // (else `nova_str` ← `int` CC-FAIL at the check site). Mirrors
                // the `Item::Fn` contract handling above.
                for c in &td.invariants {
                    collect_expr(&c.expr, out);
                    if let Some(me) = &c.message_expr {
                        collect_expr(me, out);
                    }
                }
            }
            Item::Const(c) => {
                if let Some(t) = &c.ty {
                    collect_tr(t, out);
                }
                collect_expr(&c.value, out);
            }
            Item::Let(l) => collect_expr(&l.value, out),
            Item::Test(t) => collect_block(&t.body, out),
            Item::Bench(b) => {
                for s in &b.setup {
                    collect_stmt(s, out);
                }
                collect_block(&b.measure_body, out);
                for s in &b.teardown {
                    collect_stmt(s, out);
                }
                for grp in &b.groups {
                    for case in &grp.cases {
                        for s in &case.setup {
                            collect_stmt(s, out);
                        }
                        collect_block(&case.measure_body, out);
                        for s in &case.teardown {
                            collect_stmt(s, out);
                        }
                    }
                }
            }
            Item::Lemma(_) => {}
        }
    }
}

/// Собрать имена из TypeRef-дерева (все сегменты path'ей).
fn collect_tr(tr: &TypeRef, out: &mut HashSet<String>) {
    match tr {
        TypeRef::Named { path, generics, .. } => {
            for seg in path {
                out.insert(seg.clone());
            }
            for g in generics {
                collect_tr(g, out);
            }
        }
        TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
            collect_tr(inner, out)
        }
        TypeRef::Tuple(items, _) => {
            for it in items {
                collect_tr(it, out);
            }
        }
        TypeRef::Func { params, effects, return_type, .. } => {
            for p in params {
                collect_tr(p, out);
            }
            for e in effects {
                collect_tr(e, out);
            }
            if let Some(rt) = return_type {
                collect_tr(rt, out);
            }
        }
        // Plan 97 Ф.2 (D142): анонимный protocol-тип — рекурсивно
        // собираем имена из сигнатур методов (params/return). Само
        // protocol-имя анонимно — добавлять нечего.
        TypeRef::Protocol { methods, .. } => {
            for m in methods {
                for p in &m.params {
                    collect_tr(&p.ty, out);
                }
                if let Some(rt) = &m.return_type {
                    collect_tr(rt, out);
                }
                for e in &m.effects {
                    collect_tr(e, out);
                }
            }
        }
        TypeRef::Unit(_) => {}
        // D176 (Plan 108): readonly T — transparent.
        TypeRef::Readonly(inner, _) => collect_tr(inner, out),
        // Plan 118 D216 / Plan 118.5 V2: typed pointer `*T` + Mut/Unsafe
        // transparent wrappers — recurse on inner.
        TypeRef::Pointer(inner, _)
        | TypeRef::Mut(inner, _)
        | TypeRef::Uninit(inner, _)
        | TypeRef::Ref(inner, _) => collect_tr(inner, out),
    }
}

fn collect_block(b: &Block, out: &mut HashSet<String>) {
    for s in &b.stmts {
        collect_stmt(s, out);
    }
    if let Some(t) = &b.trailing {
        collect_expr(t, out);
    }
}

fn collect_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Expr(e) => collect_expr(e, out),
        Stmt::Let(d) => {
            if let Some(t) = &d.ty {
                collect_tr(t, out);
            }
            collect_expr(&d.value, out);
            // Plan 217 (D-новый, гибрид C) BUGFIX [M-217-spawn-closure-consume-
            // cleanup-undefined]: a BARE `consume x = e` (no trailing `{ … }`
            // block — unlike `Stmt::ConsumeScope` above) auto-inserts a
            // scope-exit dispatch to the resource type's `@cleanup` method via
            // the SAME synthetic `Nova_<T>_consume_cleanup` C symbol
            // (`enter_defer_scope`'s auto-cleanup prologue scan), never spelled
            // as an AST `Member`/`Call` node. Without this seed the method-DCE
            // type∧name intersection ((T, cleanup)) never fires whenever `T`'s
            // only reachable consume-binding is a bare `consume x = e` (e.g.
            // `consume stream = conn` inside a `spawn { … }` closure —
            // `examples/net/echo_server.nv`/`echo_client.nv`, TLS pair) — the
            // definition is pruned as dead while the call site (driven
            // directly by AST, independent of DCE) still links against it →
            // `undefined symbol Nova_TcpStream_consume_cleanup`. Mirrors the
            // `Stmt::ConsumeScope` seed immediately below (over-keep, never
            // over-prune: firing still requires the receiver TYPE reachable).
            if d.consume {
                out.insert("cleanup".to_string());
            }
        }
        Stmt::Const(d) => {
            if let Some(t) = &d.ty {
                collect_tr(t, out);
            }
            collect_expr(&d.value, out);
        }
        Stmt::Assign { target, value, .. } => {
            collect_expr(target, out);
            collect_expr(value, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                collect_expr(v, out);
            }
        }
        Stmt::Throw { value, .. } => collect_expr(value, out),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
        Stmt::Defer { body, .. } => {
            collect_expr(body, out)
        }
        // Plan 110 D188: collect referenced names from init expr + body block.
        Stmt::ConsumeScope { init, body, .. } => {
            collect_expr(init, out);
            // [M-187-tls-cross-pkg-consume-cleanup]: a `consume X = e { … }`
            // scope-block dispatches the resource type's `@cleanup` method on
            // every exit path via the SYNTHETIC `Nova_<T>_consume_cleanup`
            // symbol emitted by codegen (`emit_consume_entry_cleanup`) — the
            // `.cleanup(…)` selector is NEVER spelled in source, so a pure
            // syntactic walk would never seed the `cleanup` method NAME. Under
            // Plan 159 method-DCE a method fires only when its receiver-type
            // AND its name are both reachable; without this seed the type is
            // reachable but `cleanup` is not, so `(T, cleanup)` lands in
            // `dead_method_keys`, its body+fwd are dropped, and the emitted
            // dispatch call links against a MISSING definition — `undefined
            // symbol Nova_<T>_consume_cleanup` (surfaced cross-package: the
            // consume-site lived in an external package's method, e.g.
            // nova-tls `TlsStream.accept`, whose `consume stream { … }` over a
            // `std.net TcpStream` had no cleanup-name seed reachable from root).
            // Seed the name here (mirrors the Plan 209 embed-proxy / contract-
            // interpolation synthetic-selector seeds); firing still requires
            // the receiver TYPE reachable, so this only keeps `cleanup` methods
            // of consume-types actually reached (over-keep, never over-prune).
            out.insert("cleanup".to_string());
            for stmt in &body.stmts {
                collect_stmt(stmt, out);
            }
            if let Some(t) = &body.trailing {
                collect_expr(t, out);
            }
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_expr(expr, out)
        }
        Stmt::Apply { args, .. } => {
            for a in args {
                collect_expr(a, out);
            }
        }
        Stmt::Calc { steps, .. } => {
            for step in steps {
                collect_expr(&step.expr, out);
            }
        }
        // Plan 136: tuple destructuring assignment.
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs { collect_expr(e, out); }
            for e in rhs { collect_expr(e, out); }
        }
    }
}

fn collect_expr(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Ident(n) => {
            out.insert(n.clone());
        }
        ExprKind::Path(parts) => {
            for p in parts {
                out.insert(p.clone());
            }
        }
        ExprKind::TurboFish { base, type_args } => {
            collect_expr(base, out);
            for t in type_args {
                collect_tr(t, out);
            }
        }
        ExprKind::As(inner, ty) | ExprKind::Is(inner, ty) => {
            collect_expr(inner, out);
            collect_tr(ty, out);
        }
        ExprKind::Call { func, args, trailing } => {
            collect_expr(func, out);
            // Plan 159 Ф.4 / Plan 162 Ф.4: when the callee is a `Member` selector,
            // also record the method-call form `@method:<name>` so a *value-receiver*
            // method call (`expr.foo()`) can be distinguished from a bare free-function
            // call (`foo()`, which is `Ident`). Used by reachability DCE (Plan 159 Ф.1)
            // to keep method-body symbols alive. Purely additive —
            // `@method:` names never match an import path segment, so the
            // unused-import lint over-approximation is unaffected.
            if let ExprKind::Member { name, .. } = &func.kind {
                out.insert(format!("@method:{}", name));
            }
            for a in args {
                collect_expr(a.expr(), out);
            }
            if let Some(t) = trailing {
                match t {
                    crate::ast::Trailing::Block(b) => collect_block(b, out),
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                        collect_block(&tb.body, out)
                    }
                    crate::ast::Trailing::Fn(sb) => {
                        collect_fn_sig_body(sb, out)
                    }
                }
            }
        }
        ExprKind::Binary { left, right, .. } => {
            collect_expr(left, out);
            collect_expr(right, out);
            // Plan 159 Ф.1 (reachability-DCE soundness): binary operators on a
            // user/`str` type desugar to magic methods, whose selectors emit_c.rs
            // injects and which never appear syntactically:
            //   `==`/`!=`  → `Nova_T_method_equal`/`_eq` (D237 Equal / str `@eq`)
            //   `<`/`<=`/`>`/`>=` → `_compare` (Compare / str `@compare`)
            //   `+` (on str) → `Nova_str_method_concat` (D-R4, emit_c.rs ~19190)
            // Without seeding them the reachability closure prunes
            // `str.concat`/`str.eq`/`T.compare`… and codegen calls an undeclared
            // C function (implicit-int return → type-mismatch / link error).
            //
            // [M-vr-binop-wrapper-decl-order-standalone-cu] (root-cause
            // correction, was misdiagnosed as decl-order): the SAME desugar
            // hazard applies to value-record ARITHMETIC (`+`/`-`/`*`/`/`/`%`
            // on `Monotonic`/`Duration`/… — Plan 175 Ф.1b/Ф.3, emit_c.rs
            // ~29716-29738) → `Nova_T_method_plus`/`_minus`/`_times`/`_div`/
            // `_rem`, called through an unconditionally-emitted
            // `nova_vr_binop_*` wrapper. Without a literal `.plus(...)` call
            // in the CU, the reachability closure (type∧name intersection)
            // never marks the method live, DCE drops its decl+body, and the
            // wrapper calls an undeclared C function → CC-FAIL. Seed all
            // conservatively. (Harmless over-approx for the unused-import
            // lint — these are method selectors.)
            out.insert("equal".to_string());
            out.insert("eq".to_string());
            out.insert("compare".to_string());
            out.insert("concat".to_string());
            out.insert("plus".to_string());
            out.insert("minus".to_string());
            out.insert("times".to_string());
            out.insert("div".to_string());
            out.insert("rem".to_string());
        }
        ExprKind::Unary { operand, .. } => collect_expr(operand, out),
        ExprKind::Try(i) | ExprKind::Bang(i) | ExprKind::RefArg(i) => collect_expr(i, out),
        ExprKind::Coalesce(a, b) => {
            collect_expr(a, out);
            collect_expr(b, out);
        }
        // [E_COALESCE_RETURN_FALLBACK]: checker-rejected before this pass;
        // walked defensively (same shape as `Interrupt`).
        ExprKind::CoalesceReturnFallback(opt) => {
            if let Some(i) = opt { collect_expr(i, out); }
        }
        ExprKind::Member { obj, name } => {
            collect_expr(obj, out);
            // Plan 81 Ф.7.2: collect the selector name. A module-qualified
            // free-function call `mod.func()` parses as `Member{obj, name}`
            // and codegen lowers it to a call of the free function `func`,
            // so reachability-DCE must observe `func`. (For the unused-import
            // lint this is a harmless over-approximation — a name reachable
            // as `obj.name` is conservatively treated as used.)
            out.insert(name.clone());
        }
        ExprKind::Index { obj, index } => {
            collect_expr(obj, out);
            collect_expr(index, out);
            // Plan 159 Ф.1 (reachability-DCE soundness): `a[k]` (and `a[k] = v`)
            // desugars to the `Index`/`MutIndex` magic method `a.@index(k[, v])`
            // (D240). The `index` selector is injected by the desugar and never
            // appears syntactically, so seed it so the reachability closure does
            // not prune a concrete type's `@index` body. (Harmless over-approx
            // for the unused-import lint — `index` is a method selector.)
            out.insert("index".to_string());
        }
        ExprKind::If { cond, then, else_ } => {
            collect_expr(cond, out);
            collect_block(then, out);
            if let Some(eb) = else_ {
                collect_else(eb, out);
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            collect_expr(scrutinee, out);
            collect_block(then, out);
            if let Some(eb) = else_ {
                collect_else(eb, out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_expr(scrutinee, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_expr(g, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_expr(e, out),
                    MatchArmBody::Block(b) => collect_block(b, out),
                }
            }
        }
        ExprKind::Block(b) => collect_block(b, out),
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e) | ArrayElem::Spread(e) => {
                        collect_expr(e, out)
                    }
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for (k, v) in crate::ast::MapElem::cloned_pairs(elems).iter() {
                collect_expr(k, out);
                collect_expr(v, out);
            }
        }
        ExprKind::TupleLit(elems) => {
            for e in elems {
                collect_expr(e, out);
            }
        }
        ExprKind::RecordLit { type_name, fields, .. } => {
            if let Some(tn) = type_name {
                for seg in tn {
                    out.insert(seg.clone());
                }
            }
            for f in fields {
                if let Some(v) = &f.value {
                    collect_expr(v, out);
                }
            }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            collect_expr(tag, out);
            for a in args {
                collect_expr(a, out);
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let crate::ast::InterpStrPart::Expr { expr: e, spec: _ } = p {
                    collect_expr(e, out);
                }
            }
            // Plan 159 Ф.2 (reachability-DCE soundness — indirect reference):
            // string interpolation `"…${x}…"` desugars (emit_c.rs ~34560) to a
            // `StringBuilder` pipeline whose method selectors are injected by
            // codegen and never appear syntactically. The names below are the
            // *Nova* method names (what `dead_method_keys` keys on) — NOT the
            // mangled C names; the desugar lowers them to:
            //   `cap`      → Nova_StringBuilder_method_cap__nova_int
            //                 (fluent `.new().cap(n)` opener, D372 amend)
            //   `append`   → Nova_StringBuilder_method_append*
            //   `into_str` → Nova_StringBuilder_consume_into_str
            //                 (consume method → `consume_` C prefix)
            // and, per interpolated value, the Display/Debug conversion methods
            // `T.display` / `T.debug` (D229/D237) and the `str.from` /
            // `str.from_debug` / instance `T.to_str` (D410) fallbacks. Without
            // seeding these (and the `StringBuilder` receiver-type name)
            // method-level DCE prunes the StringBuilder method bodies + forward
            // decls, and codegen emits a call to an *undeclared* C function
            // (C89 implicit-`int` declaration → the observed `nova_str` ← `int`
            // type-mismatch / CC-FAIL). All are method selectors → harmless
            // over-approximation for the unused-import lint, and conservative
            // (over-keep) for DCE.
            //
            // Plan 174.1 seed-list refresh (owner review, 2026-07-08): the list
            // must track the CURRENT desugar exactly — RETRACTED names are
            // removed, not kept "for compat": `with_capacity` (→ `.new().cap(n)`,
            // D372 amend), `as_str` (→ `into_str`, Plan 91.18), `into` (D73
            // auto-derive retracted 2026-07-06; desugar's fallback is now the
            // D410 instance `to_str`). The stale `as_str` seed let method-DCE
            // prune `into_str`'s body while codegen still emitted the call →
            // implicit-int CC-FAIL in every fn-main CU whose reachable code
            // contains `${…}` interpolation (surfaced by Plan 174.1 gates).
            // NB the whole name-list is a hardcode-class liability —
            // [M-dce-seed-name-list] (P3): seeds should come from the desugar's
            // own emission facts, not a handwritten list.
            out.insert("StringBuilder".to_string());
            out.insert("append".to_string());
            out.insert("into_str".to_string());
            out.insert("cap".to_string());
            out.insert("display".to_string());
            out.insert("debug".to_string());
            out.insert("from".to_string());
            out.insert("from_debug".to_string());
            out.insert("to_str".to_string());
            // Plan 208 Ф.2 (D422, was D419/Plan 152.7.2): EVERY `${x}`/
            // `${x:?}`/`${x:SPEC}` now wraps the sink in a `FmtCtx` (bare or
            // rich) via a hand-emitted `Nova_FmtCtx_static_bare(...)`/
            // `Nova_FmtCtx_static_rich(...)` call (`emit_bare_fmtctx`/
            // `emit_format_spec_value`) before dispatching to
            // `Nova_<T>_method_display`/`_debug` — same class of
            // invisible-to-AST selector as the seeds above. Without seeding
            // these, method-DCE prunes `FmtCtx.bare`/`FmtCtx.rich` (and its
            // getter methods, reached from user `@display`/`@debug` bodies)
            // whenever the rest of the program never spells these names.
            // `display_fmt` (D419's optional hook) is GONE — no codegen path
            // emits it anymore (D422 retracts it entirely), so it is no
            // longer seeded here.
            out.insert("FmtCtx".to_string());
            out.insert("new".to_string());
            out.insert("bare".to_string());
            out.insert("rich".to_string());
            out.insert("width".to_string());
            out.insert("align".to_string());
            out.insert("fill".to_string());
            out.insert("sign".to_string());
            out.insert("kind".to_string());
            out.insert("pad".to_string());
            out.insert("alternate".to_string());
            out.insert("precision".to_string());
            out.insert("write".to_string());
            // Plan 208 Ф.4R Ш3/Ш4 (owner 2026-07-20/2026-07-21): the interp
            // fast-path (`emit_interpolated_str`/`emit_format_spec_value`,
            // compiler-codegen/src/codegen/emit_c.rs) now hand-emits DIRECT
            // calls to these `std.runtime.string_builder` free functions for
            // ALL SIX primitive kinds (int/f64/f32/char/bool/str), bare AND
            // rich-spec, Display AND Debug (Ф.4R Ш1 engine; the
            // `NOVA_FMT_LEGACY` kill-switch + the old `conv.h` chain it used
            // to fall back to are RETIRED as of Ш4) — SAME invisible-to-AST-
            // call class as the `StringBuilder`/`FmtCtx` seeds above: without
            // seeding these FREE FUNCTION names, reachability-DCE (Plan 81
            // Ф.7.2/Plan 159) prunes them whenever nothing in the rest of the
            // program spells them by name, and codegen emits a call to an
            // undeclared C function (CC-FAIL/link error) for the FIRST
            // interpolated value of that primitive kind in an otherwise
            // `*_display_spec`-free program — exactly the class of bug this
            // seed list exists to prevent (caught by the `bool_display_spec`/
            // `char_display_spec` undefined-symbol link failures on
            // `f14_legacy_workaround_still_works`/
            // `f2_static_method_str_from_bool`/`f7_char_var` during Ш4's own
            // folder-CU gate — str/char/bool were newly wired to
            // `*_display_spec` this wave and had NOT been added here yet).
            out.insert("int_display_spec".to_string());
            out.insert("f64_display_spec".to_string());
            out.insert("f32_display_spec".to_string());
            out.insert("bool_display_spec".to_string());
            out.insert("char_display_spec".to_string());
            out.insert("char_debug_display_spec".to_string());
            out.insert("str_display_spec".to_string());
            out.insert("str_debug_display_spec".to_string());
        }
        ExprKind::Lambda { body, .. } => collect_expr(body, out),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e) => collect_expr(e, out),
            ClosureBody::Block(b) => collect_block(b, out),
        },
        ExprKind::ClosureFull(sb) => collect_fn_sig_body(sb, out),
        ExprKind::Spawn(body) => collect_expr(body, out),
        ExprKind::Detach(body) | ExprKind::Blocking(body) => collect_block(body, out),
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                collect_expr(c, out);
            }
            if let Some(_dl) = deadline {
                let _dl_e = &_dl.expr;
                collect_expr(_dl_e, out);
            }
            collect_block(body, out);
        }
        ExprKind::Forbid { body, .. } => collect_block(body, out),
        ExprKind::Realtime { body, .. } => collect_block(body, out),
        ExprKind::ParallelFor { iter, body, .. } => {
            collect_expr(iter, out);
            collect_block(body, out);
            // Plan 159 Ф.1: parallel-for also drives the iteration protocol —
            // see the `For` arm below for the rationale.
            out.insert("next".to_string());
            out.insert("iter".to_string());
        }
        ExprKind::For { iter, body, .. } => {
            collect_expr(iter, out);
            collect_block(body, out);
            // Plan 159 Ф.1 (reachability-DCE soundness): `for x in it { … }`
            // desugars to the iteration protocol — codegen calls `it.iter()`
            // (when `it` is not already an iterator) and then `.next()` in a
            // loop. Those method selectors are injected by the desugar and
            // never appear syntactically, so without seeding them here the
            // reachability closure would prune `Iter.next` / `Iter.iter`
            // (e.g. `CharsIter.next`, reached only via `for c in s.chars()`)
            // and emit a call to an undeclared C function. Conservatively mark
            // both protocol names used wherever a `for` loop is reachable.
            // (Harmless over-approximation for the unused-import lint: these are
            // method selectors, not importable free names.)
            out.insert("next".to_string());
            out.insert("iter".to_string());
        }
        ExprKind::While { cond, body, .. } => {
            collect_expr(cond, out);
            collect_block(body, out);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            collect_expr(scrutinee, out);
            collect_block(body, out);
        }
        ExprKind::Loop { body, .. } => collect_block(body, out),
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    crate::ast::SelectOp::Recv { chan, .. } => {
                        collect_expr(chan, out)
                    }
                    crate::ast::SelectOp::Send { chan, value } => {
                        collect_expr(chan, out);
                        collect_expr(value, out);
                    }
                    crate::ast::SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard {
                    collect_expr(g, out);
                }
                collect_block(&arm.body, out);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { collect_expr(s, out); }
            if let Some(e) = end { collect_expr(e, out); }
        }
        ExprKind::Throw(i) => collect_expr(i, out),
        ExprKind::Interrupt(opt) => {
            if let Some(e) = opt {
                collect_expr(e, out);
            }
        }
        ExprKind::With { bindings, body } => {
            for b in bindings {
                collect_tr(&b.effect, out);
                collect_expr(&b.handler, out);
            }
            collect_block(body, out);
        }
        ExprKind::Forall { range, body, .. }
        | ExprKind::Exists { range, body, .. } => {
            collect_expr(range, out);
            collect_expr(body, out);
        }
        // Plan 97 Ф.4 (D142): protocol-литерал — collect-name walk
        // идентичен handler-литералу. Field name отличается (effect_name
        // / proto_name) — паттерн-биндинг через alias.
        ExprKind::HandlerLit { effect_name, methods }
        | ExprKind::ProtocolLit { proto_name: effect_name, methods } => {
            for seg in effect_name {
                out.insert(seg.clone());
            }
            for mth in methods {
                match &mth.body {
                    HandlerMethodBody::Expr(e) => collect_expr(e, out),
                    HandlerMethodBody::Block(b) => collect_block(b, out),
                }
            }
        }
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
        | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
        | ExprKind::HexBlobLit(_) | ExprKind::NullPtrLit
        | ExprKind::SelfAccess => {}
    }
}

fn collect_else(eb: &ElseBranch, out: &mut HashSet<String>) {
    match eb {
        ElseBranch::Block(b) => collect_block(b, out),
        ElseBranch::If(e) => collect_expr(e, out),
    }
}

fn collect_fn_sig_body(sb: &crate::ast::FnSigBody, out: &mut HashSet<String>) {
    for p in &sb.params {
        collect_tr(&p.ty, out);
    }
    for e in &sb.effects {
        collect_tr(e, out);
    }
    if let Some(rt) = &sb.return_type {
        collect_tr(rt, out);
    }
    match &sb.body {
        FnBody::Expr(e) => collect_expr(e, out),
        FnBody::Block(b) => collect_block(b, out),
        FnBody::External => {}
    }
}

/// Plan 62.F.bis Ф.2: snapshot prelude-visibility state модуля.
/// Совместно используется `types::check_module` (silent classify duplicate
/// при name-merge) и `lint_prelude_shadow` (emit structured warning).
///
/// **Pass 1**: имена объявленные прямо в `std/prelude/*.nv` peer-файлах
/// (включая `std/prelude.nv` facade себя).
/// **Pass 2**: имена re-export'нутые prelude facade через `export import
/// X.{A, B as C}` — используем alias если есть, иначе оригинальное имя.
///
/// Возвращает оба set'а отдельно — caller'ы используют по-разному.
#[derive(Debug, Default)]
pub struct PreludeVisibility {
    /// User-visible имена из prelude (peer-decls + re-exports).
    pub visible: HashSet<String>,
    /// All имена из non-entry peer items (включая codegen-only merge —
    /// items pulled для completeness, не user-visible). Subset relation:
    /// `visible ⊆ merged_from_imports`.
    pub merged_from_imports: HashSet<String>,
}

/// Вычислить prelude-visibility для модуля. Идемпотентна — multiple
/// calls возвращают тот же результат.
pub fn collect_prelude_visibility(module: &Module) -> PreludeVisibility {
    let mut visible: HashSet<String> = HashSet::new();
    let mut merged_from_imports: HashSet<String> = HashSet::new();
    // Pass 1: names declared directly in prelude peer files + collect
    // merged_from_imports set (всё что pulled из non-entry peers).
    for pf in &module.peer_files {
        if pf.is_entry_module { continue; }
        let path_str = pf.path.to_string_lossy().replace('\\', "/");
        let is_prelude_peer = path_str.contains("/std/prelude/")
            || path_str.ends_with("/std/prelude.nv");
        for it in &pf.items_here {
            let key = match it {
                Item::Type(td) => Some(td.name.clone()),
                Item::Fn(fd) => Some(match &fd.receiver {
                    Some(r) => format!("{}.{}", r.type_name, fd.name),
                    None => fd.name.clone(),
                }),
                Item::Const(cd) => Some(cd.name.clone()),
                _ => None,
            };
            if let Some(k) = key {
                merged_from_imports.insert(k.clone());
                if is_prelude_peer {
                    visible.insert(k);
                }
            }
        }
    }
    // Pass 2: names re-exported through prelude facade via selective list.
    // Re-exported alias (or original) — user-visible name; добавляем
    // в `visible`. Также добавляем в `merged_from_imports` (re-export
    // implies merge for codegen completeness).
    for pf in &module.peer_files {
        if pf.is_entry_module { continue; }
        let path_str = pf.path.to_string_lossy().replace('\\', "/");
        let is_prelude_peer = path_str.contains("/std/prelude/")
            || path_str.ends_with("/std/prelude.nv");
        if !is_prelude_peer { continue; }
        for imp in &pf.imports {
            if !imp.is_export { continue; }
            if let Some(items) = &imp.items {
                for it in items {
                    let visible_name = it.alias.clone().unwrap_or_else(|| it.name.clone());
                    visible.insert(visible_name.clone());
                    merged_from_imports.insert(visible_name);
                }
            }
            // Wildcard `export import X.*` rejected per Plan 35 R25.
        }
    }
    PreludeVisibility { visible, merged_from_imports }
}

/// Plan 62.F.bis Ф.2: lint W_PRELUDE_SHADOW — emit structured warning
/// для user-declarations что shadow'ят prelude-visible имена.
///
/// **Алгоритм:**
/// 1. Compute `PreludeVisibility` через `collect_prelude_visibility`.
/// 2. Сканируем entry's items_here (только user-declarations, не merged
///    items): для каждого top-level Type/Fn/Const проверяем conflict
///    с `visible` set.
/// 3. Если conflict — emit warning (rule: `W_PRELUDE_SHADOW`,
///    severity = warning). User-declaration wins (это уже handled в
///    types::check_module и emit_c.rs); lint лишь сигнализирует.
///
/// **Suppress:** `module X allow_prelude_shadow` clause (parser добавляет
/// `ModuleAttrKind::AllowPreludeShadow`) → возвращает empty Vec. Также
/// suppress'нут automatically для prelude self-modules (`std.prelude.*`
/// — они САМИ объявляют prelude names, не shadowing).
///
/// **Hint в сообщении:** `qualify as std.prelude.<sub>.<name>` для
/// reach'а prelude-версии, или `add allow_prelude_shadow` для suppress.
pub fn lint_prelude_shadow(module: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    // Suppress: module-level `allow_prelude_shadow` clause.
    let suppressed = module.attrs.iter()
        .any(|a| matches!(a.kind, crate::ast::ModuleAttrKind::AllowPreludeShadow));
    if suppressed {
        return warnings;
    }
    // Suppress: prelude self-modules (они declare prelude items legitimately).
    if crate::manifest::is_prelude_self_module(&module.name) {
        return warnings;
    }
    let vis = collect_prelude_visibility(module);
    if vis.visible.is_empty() {
        return warnings;
    }
    // Iterate entry's items_here (user-decls only, not merged-from-imports).
    // Если peer_files пуст (legacy single-file без resolver-merge), fall
    // back на module.items.
    let entry_items: Vec<&Item> = if module.peer_files.is_empty() {
        module.items.iter().collect()
    } else {
        module.peer_files.iter()
            .filter(|pf| pf.is_entry_module)
            .flat_map(|pf| pf.items_here.iter())
            .collect()
    };
    for item in entry_items {
        let (name, span) = match item {
            Item::Type(td) => (td.name.clone(), td.span),
            Item::Fn(fd) => {
                let key = match &fd.receiver {
                    Some(r) => format!("{}.{}", r.type_name, fd.name),
                    None => fd.name.clone(),
                };
                (key, fd.span)
            }
            Item::Const(cd) => (cd.name.clone(), cd.span),
            _ => continue,
        };
        if vis.visible.contains(&name) {
            // Структурированный warning. Лидирующий `[W_PRELUDE_SHADOW]`
            // tag в сообщении — для grep'абельности из CLI и для
            // EXPECT_COMPILE_WARNING matching в test_runner (lint rendered
            // через `diag.render` который не включает `rule` field,
            // поэтому tag нужен в самом тексте).
            let diag = Diagnostic::new(
                format!(
                    "[W_PRELUDE_SHADOW] top-level name `{}` shadows a \
                     declaration auto-imported from std.prelude (D29). \
                     User declaration wins — qualify as \
                     `std.prelude.<sub>.{}` to reach the prelude version. \
                     Suppress: add `#allow(shadow)` before `module` declaration \
                     (D174), or switch to `#no_prelude` / `#prelude(...)` (Plan 107).",
                    name, name
                ),
                span,
            );
            warnings.push(LintWarning {
                rule: "W_PRELUDE_SHADOW",
                diag,
            });
        }
    }
    warnings
}

/// Plan 52 Ф.2: рекурсивный обход блока для lint-проверок выражений.
fn walk_block_lints(b: &Block, out: &mut Vec<LintWarning>) {
    for s in &b.stmts {
        walk_stmt_lints(s, out);
    }
    if let Some(t) = &b.trailing {
        walk_expr_lints(t, out);
    }
}

/// Plan 57.C.7: bench-specific lints для measure body. Detects:
///   - Time.sleep / Time.sleep_ms (noise → unreliable measurement).
///   - Io.println / println (I/O overhead dominates measure timing).
///   - bench.opaque(<literal>) (no-op: constant folding не происходит на literals).
fn walk_bench_measure_lints(b: &Block, bench_name: &str, out: &mut Vec<LintWarning>) {
    for s in &b.stmts {
        check_bench_stmt(s, bench_name, out);
    }
    if let Some(t) = &b.trailing {
        check_bench_expr(t, bench_name, out);
    }
}

fn check_bench_stmt(s: &Stmt, bench_name: &str, out: &mut Vec<LintWarning>) {
    match s {
        Stmt::Expr(e) => check_bench_expr(e, bench_name, out),
        Stmt::Let(l) => check_bench_expr(&l.value, bench_name, out),
        Stmt::Assign { value, .. } => check_bench_expr(value, bench_name, out),
        _ => {}
    }
}

fn check_bench_expr(e: &Expr, bench_name: &str, out: &mut Vec<LintWarning>) {
    use crate::ast::{ExprKind, ElseBranch};
    match &e.kind {
        // Method call OR namespace dispatch — два вида:
        //   1. Call { func: Member { obj, name } } — obj.method() style.
        //   2. Call { func: Path([...]) } — Type.method() / Namespace.fn().
        ExprKind::Call { func, args, .. } => {
            // Plan 57.D.2: sleep-lint contextual detection.
            // Heuristic: method ∈ {sleep, sleep_ms, sleep_ns} likely refers
            // к Time effect dispatch — match regardless of obj-name.
            // Также cover Path-form (Time.sleep parsed как Path(["Time","sleep"])).
            let extract_method = |func_kind: &ExprKind| -> Option<(String, String)> {
                match func_kind {
                    ExprKind::Member { obj, name } => {
                        let obj_label = match &obj.kind {
                            ExprKind::Ident(n) => n.clone(),
                            _ => "_".to_string(),
                        };
                        Some((obj_label, name.clone()))
                    }
                    ExprKind::Path(segs) if segs.len() >= 2 => {
                        Some((segs[..segs.len()-1].join("."),
                              segs[segs.len()-1].clone()))
                    }
                    _ => None,
                }
            };
            if let Some((recv, method)) = extract_method(&func.kind) {
                let is_sleep_method = method == "sleep" || method == "sleep_ms"
                                   || method == "sleep_ns";
                if is_sleep_method {
                    out.push(LintWarning {
                        rule: "bench-sleep-in-measure",
                        diag: crate::diag::Diagnostic::new(
                            format!("bench \"{}\": `{}.{}(...)` inside `measure` block — \
                                     sleep dominates timing noise; consider exempt в bench.toml \
                                     или move в setup", bench_name, recv, method),
                            e.span,
                        ),
                    });
                }
                if recv == "Io" && (method == "println" || method == "print"
                                  || method == "eprintln") {
                    out.push(LintWarning {
                        rule: "bench-io-in-measure",
                        diag: crate::diag::Diagnostic::new(
                            format!("bench \"{}\": `Io.{}` inside `measure` block — \
                                     I/O latency dominates; results unreliable",
                                bench_name, method),
                            e.span,
                        ),
                    });
                }
                if recv == "bench" && method == "opaque" && args.len() == 1 {
                    let arg = args[0].expr();
                    if matches!(&arg.kind,
                        ExprKind::IntLit(_) | ExprKind::FloatLit(_)
                        | ExprKind::StrLit(_) | ExprKind::BoolLit(_)) {
                        out.push(LintWarning {
                            rule: "bench-opaque-literal",
                            diag: crate::diag::Diagnostic::new(
                                format!("bench \"{}\": `bench.opaque(<literal>)` — \
                                         barrier no-op на constant literals; opaque нужен только \
                                         для derived values", bench_name),
                                e.span,
                            ),
                        });
                    }
                }
            }
            // Free `println(...)` / `print(...)` / `sleep(...)` calls.
            if let ExprKind::Ident(n) = &func.kind {
                if n == "println" || n == "print" || n == "eprintln" {
                    out.push(LintWarning {
                        rule: "bench-io-in-measure",
                        diag: crate::diag::Diagnostic::new(
                            format!("bench \"{}\": `{}` inside `measure` block — \
                                     I/O latency dominates measurement", bench_name, n),
                            e.span,
                        ),
                    });
                }
                // Plan 57.D.2: bare sleep / sleep_ms / sleep_ns тоже warn —
                // могут быть resolved-to-Time-effect dispatch.
                if n == "sleep" || n == "sleep_ms" || n == "sleep_ns" {
                    out.push(LintWarning {
                        rule: "bench-sleep-in-measure",
                        diag: crate::diag::Diagnostic::new(
                            format!("bench \"{}\": `{}` inside `measure` block — \
                                     sleep dominates timing noise; move в setup или \
                                     exempt в bench.toml", bench_name, n),
                            e.span,
                        ),
                    });
                }
            }
            check_bench_expr(func, bench_name, out);
            for a in args { check_bench_expr(a.expr(), bench_name, out); }
        }
        ExprKind::If { cond, then, else_, .. } => {
            check_bench_expr(cond, bench_name, out);
            walk_bench_measure_lints(then, bench_name, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_bench_measure_lints(b, bench_name, out),
                    ElseBranch::If(if_expr) => check_bench_expr(if_expr, bench_name, out),
                }
            }
        }
        ExprKind::While { cond, body, .. } => {
            check_bench_expr(cond, bench_name, out);
            walk_bench_measure_lints(body, bench_name, out);
        }
        ExprKind::Loop { body, .. } => walk_bench_measure_lints(body, bench_name, out),
        ExprKind::For { iter, body, .. } => {
            check_bench_expr(iter, bench_name, out);
            walk_bench_measure_lints(body, bench_name, out);
        }
        _ => {}
    }
}

fn walk_stmt_lints(s: &Stmt, out: &mut Vec<LintWarning>) {
    match s {
        Stmt::Expr(e) => walk_expr_lints(e, out),
        Stmt::Let(d) => walk_expr_lints(&d.value, out),
        Stmt::Const(d) => walk_expr_lints(&d.value, out),
        Stmt::Assign { target, value, .. } => {
            walk_expr_lints(target, out);
            walk_expr_lints(value, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { walk_expr_lints(v, out); }
        }
        Stmt::Throw { value, .. } => walk_expr_lints(value, out),
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Defer { body, .. } => walk_expr_lints(body, out),
        // Plan 110 D188: lint walk through init + body block.
        Stmt::ConsumeScope { init, body, .. } => {
            walk_expr_lints(init, out);
            for stmt in &body.stmts {
                walk_stmt_lints(stmt, out);
            }
            if let Some(t) = &body.trailing {
                walk_expr_lints(t, out);
            }
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => walk_expr_lints(expr, out),
        // Plan 33.3 Ф.13: Apply/Calc — proof-statements, spec-only.
        Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
        // Plan 136: tuple destructuring assignment.
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs { walk_expr_lints(e, out); }
            for e in rhs { walk_expr_lints(e, out); }
        }
    }
}

/// Plan 96.1 Ф.1 — W_VIEW_PUSH_DETACH lint.
///
/// Detects pattern: `let X = obj[Range]; ...; X.push(...)`.
/// Warning explains, что push на slice-view с `cap == len` реаллокает
/// и детачится от parent backing'а; parent НЕ модифицируется (anti-
/// Go-append-footgun, но silent surprise).
///
/// Per-function walker maintains HashMap<binding_name, span_of_binding>
/// of slice-view bindings (RHS = `Index { obj, index: Range }`). При
/// встрече `X.push(...)` на tracked X — emit warning.
///
/// Closes `[P-plan96-lint-deferred]` from Plan 96.
fn lint_view_push_detach(f: &FnDecl, out: &mut Vec<LintWarning>) {
    let mut slice_views: std::collections::HashMap<String, crate::diag::Span> =
        std::collections::HashMap::new();
    match &f.body {
        FnBody::Expr(e) => walk_view_push_expr(e, &mut slice_views, out),
        FnBody::Block(b) => walk_view_push_block(b, &mut slice_views, out),
        FnBody::External => {}
    }
}

fn walk_view_push_block(
    b: &Block,
    slice_views: &mut std::collections::HashMap<String, crate::diag::Span>,
    out: &mut Vec<LintWarning>,
) {
    for s in &b.stmts {
        walk_view_push_stmt(s, slice_views, out);
    }
    if let Some(t) = &b.trailing {
        walk_view_push_expr(t, slice_views, out);
    }
}

fn walk_view_push_stmt(
    s: &Stmt,
    slice_views: &mut std::collections::HashMap<String, crate::diag::Span>,
    out: &mut Vec<LintWarning>,
) {
    match s {
        // Track let-binding'ов с RHS = Index{obj, index: Range}.
        Stmt::Let(d) => {
            if let ExprKind::Index { index, .. } = &d.value.kind {
                if matches!(index.kind, ExprKind::Range { .. }) {
                    // Single-name pattern: `let X = arr[a..b]`.
                    if let Pattern::Ident { name, .. } = &d.pattern {
                        slice_views.insert(name.clone(), d.value.span);
                    }
                }
            }
            walk_view_push_expr(&d.value, slice_views, out);
        }
        Stmt::Expr(e) => walk_view_push_expr(e, slice_views, out),
        Stmt::Assign { target, value, .. } => {
            walk_view_push_expr(target, slice_views, out);
            walk_view_push_expr(value, slice_views, out);
        }
        Stmt::Return { value: Some(v), .. } => walk_view_push_expr(v, slice_views, out),
        Stmt::Throw { value, .. } => walk_view_push_expr(value, slice_views, out),
        Stmt::Defer { body, .. } => {
            walk_view_push_expr(body, slice_views, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            walk_view_push_expr(expr, slice_views, out);
        }
        _ => {}
    }
}

fn walk_view_push_expr(
    e: &Expr,
    slice_views: &mut std::collections::HashMap<String, crate::diag::Span>,
    out: &mut Vec<LintWarning>,
) {
    match &e.kind {
        // Detect: X.push(...) where X is a tracked slice-view.
        ExprKind::Call { func, .. } => {
            if let ExprKind::Member { obj, name } = &func.kind {
                if name == "push" {
                    if let ExprKind::Ident(var_name) = &obj.kind {
                        if let Some(&view_span) = slice_views.get(var_name) {
                            out.push(LintWarning {
                                rule: "W_VIEW_PUSH_DETACH",
                                diag: crate::diag::Diagnostic::new(
                                    format!(
                                        "W_VIEW_PUSH_DETACH: mut view's push detaches from \
                                         parent backing; parent NOT modified. View `{}` \
                                         was created from slice expression (Plan 96 \
                                         D-cap-len, D144). Use parent directly to grow, \
                                         or convert view to independent array first.",
                                        var_name
                                    ),
                                    e.span,
                                ).with_note_at(
                                    format!("`{}` bound here from slice", var_name),
                                    view_span,
                                ),
                            });
                        }
                    }
                }
            }
            // Recurse into func and args для nested matches.
            walk_view_push_expr(func, slice_views, out);
            // Args walk: skip — push args usually don't contain new view bindings.
        }
        ExprKind::Block(b) => walk_view_push_block(b, slice_views, out),
        ExprKind::If { cond, then, else_ } => {
            walk_view_push_expr(cond, slice_views, out);
            walk_view_push_block(then, slice_views, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_view_push_block(b, slice_views, out),
                    ElseBranch::If(if_expr) => walk_view_push_expr(if_expr, slice_views, out),
                }
            }
        }
        ExprKind::For { body, .. } | ExprKind::While { body, .. } => {
            walk_view_push_block(body, slice_views, out);
        }
        // Other expression kinds — обычный обход (упрощённо).
        _ => {}
    }
}

/// Plan 90.1 Ф.5 (D141 amendment) — W_VIEW_EXTEND_DETACH lint.
///
/// Detects pattern: `let view = parent[Range]; ...; parent.append(...)`.
/// Warning: calling append / insert / reserve on a parent array
/// that has a live slice-view may trigger realloc, making the view point to
/// freed/stale memory (D-cap-len: grow detaches from parent backing, Plan 96).
///
/// Per-function walker maintains HashMap<parent_name, span_of_view_binding>
/// of parent arrays that have a slice-view binding. On `parent.append(...)` /
/// `.insert(...)` / `.reserve(...)` on tracked parent → emit warning.
///
/// Suppressed by `#allow(view_extend_detach)` at module level.
fn lint_view_extend_detach(f: &FnDecl, suppressed: bool, out: &mut Vec<LintWarning>) {
    if suppressed {
        return;
    }
    // HashMap<parent_name, view_binding_span>
    let mut view_parents: std::collections::HashMap<String, crate::diag::Span> =
        std::collections::HashMap::new();
    match &f.body {
        FnBody::Expr(e) => walk_view_extend_expr(e, &mut view_parents, out),
        FnBody::Block(b) => walk_view_extend_block(b, &mut view_parents, out),
        FnBody::External => {}
    }
}

fn walk_view_extend_block(
    b: &Block,
    view_parents: &mut std::collections::HashMap<String, crate::diag::Span>,
    out: &mut Vec<LintWarning>,
) {
    for s in &b.stmts {
        walk_view_extend_stmt(s, view_parents, out);
    }
    if let Some(t) = &b.trailing {
        walk_view_extend_expr(t, view_parents, out);
    }
}

fn walk_view_extend_stmt(
    s: &Stmt,
    view_parents: &mut std::collections::HashMap<String, crate::diag::Span>,
    out: &mut Vec<LintWarning>,
) {
    match s {
        // Track: `let view = parent[Range]` → record `parent` as having a view.
        Stmt::Let(d) => {
            if let ExprKind::Index { obj, index } = &d.value.kind {
                if matches!(index.kind, ExprKind::Range { .. }) {
                    // Single-name binding: `let view = arr[a..b]`.
                    if let Pattern::Ident { .. } = &d.pattern {
                        // Record the parent array name.
                        if let ExprKind::Ident(parent_name) = &obj.kind {
                            view_parents.insert(parent_name.clone(), d.value.span);
                        }
                    }
                }
            }
            walk_view_extend_expr(&d.value, view_parents, out);
        }
        Stmt::Expr(e) => walk_view_extend_expr(e, view_parents, out),
        Stmt::Assign { target, value, .. } => {
            walk_view_extend_expr(target, view_parents, out);
            walk_view_extend_expr(value, view_parents, out);
        }
        Stmt::Return { value: Some(v), .. } => walk_view_extend_expr(v, view_parents, out),
        Stmt::Throw { value, .. } => walk_view_extend_expr(value, view_parents, out),
        Stmt::Defer { body, .. } => {
            walk_view_extend_expr(body, view_parents, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            walk_view_extend_expr(expr, view_parents, out);
        }
        _ => {}
    }
}

/// Grow-methods that may cause realloc, invalidating existing slice views.
/// Plan 90 followup (2026-06-01): `append_zero` добавлен — extends by N
/// zero-init elements, тот же realloc-path.
fn is_grow_method(name: &str) -> bool {
    matches!(name, "append" | "insert" | "reserve" | "append_zero")
}

fn walk_view_extend_expr(
    e: &Expr,
    view_parents: &mut std::collections::HashMap<String, crate::diag::Span>,
    out: &mut Vec<LintWarning>,
) {
    match &e.kind {
        // Detect: parent.append(...) / parent.insert(...) / parent.reserve(...)
        // where `parent` is a tracked view-parent.
        ExprKind::Call { func, .. } => {
            if let ExprKind::Member { obj, name } = &func.kind {
                if is_grow_method(name) {
                    if let ExprKind::Ident(parent_name) = &obj.kind {
                        if let Some(&view_span) = view_parents.get(parent_name) {
                            out.push(LintWarning {
                                rule: "W_VIEW_EXTEND_DETACH",
                                diag: crate::diag::Diagnostic::new(
                                    format!(
                                        "W_VIEW_EXTEND_DETACH: `{parent}.{method}(...)` may \
                                         realloc and invalidate existing slice-view of `{parent}` \
                                         (D-cap-len, D141, Plan 90.1). If view is intentionally \
                                         discarded, add `#allow(view_extend_detach)` to module.",
                                        parent = parent_name,
                                        method = name,
                                    ),
                                    e.span,
                                ).with_note_at(
                                    format!("slice-view of `{}` created here", parent_name),
                                    view_span,
                                ),
                            });
                        }
                    }
                }
            }
            // Recurse into func and args for nested calls.
            walk_view_extend_expr(func, view_parents, out);
        }
        ExprKind::Block(b) => walk_view_extend_block(b, view_parents, out),
        ExprKind::If { cond, then, else_ } => {
            walk_view_extend_expr(cond, view_parents, out);
            walk_view_extend_block(then, view_parents, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_view_extend_block(b, view_parents, out),
                    ElseBranch::If(if_expr) => walk_view_extend_expr(if_expr, view_parents, out),
                }
            }
        }
        ExprKind::For { body, .. } | ExprKind::While { body, .. } => {
            walk_view_extend_block(body, view_parents, out);
        }
        _ => {}
    }
}

// ============================================================================
// `new-then-cap` lint (владелец 2026-07-21): `X.new()` (без cap-арга) сразу
// следом `.cap(n)` на том же binding — две аллокации там, где canonical
// spelling (std/src/collections/vec/core.nv:104) даёт одну:
// `X.new(cap: n)`. Две поверхностные формы:
//   - split statements: `mut v = X.new()` затем `v.cap(n)` следующим stmt;
//   - chain: `X.new().cap(n)`.
// Warning-класс (как unused-import) — не error; canon — рекомендация.
// ============================================================================

/// `true` если `Call { func: Member{name:"new", ..}, args, .. }` не содержит
/// cap-аргумента (ни позиционного, ни named `cap:`) — т.е. `.new()` /
/// `.new(0)`-подобный вызов БЕЗ явного pre-sizing.
fn is_new_call_without_cap(func: &Expr, args: &[CallArg]) -> bool {
    if let ExprKind::Member { name, .. } = &func.kind {
        if name == "new" {
            return !args.iter().any(|a| matches!(a, CallArg::Named { name, .. } if name == "cap"))
                && args.is_empty();
        }
    }
    false
}

/// Best-effort human-readable receiver description for the chain-form
/// message (`Vec[T].new().cap(n)` → "Vec[T]"). Falls back to "expression"
/// when the receiver isn't a simple path/ident (rare in practice — chain
/// form's receiver is always the type/constructor expression).
fn describe_new_receiver(func: &Expr) -> String {
    if let ExprKind::Member { obj, .. } = &func.kind {
        match &obj.kind {
            ExprKind::Ident(n) => return n.clone(),
            ExprKind::Path(segs) => return segs.join("."),
            ExprKind::TurboFish { base, .. } => return describe_new_receiver_base(base),
            _ => {}
        }
    }
    "expression".to_string()
}

fn describe_new_receiver_base(base: &Expr) -> String {
    match &base.kind {
        ExprKind::Ident(n) => n.clone(),
        ExprKind::Path(segs) => segs.join("."),
        _ => "expression".to_string(),
    }
}

fn lint_new_then_cap(f: &FnDecl, out: &mut Vec<LintWarning>) {
    match &f.body {
        FnBody::Expr(e) => walk_new_then_cap_expr(e, out),
        FnBody::Block(b) => walk_new_then_cap_block(b, out),
        FnBody::External => {}
    }
}

fn walk_new_then_cap_block(b: &Block, out: &mut Vec<LintWarning>) {
    // Track `binding -> span_of_new_call` for bindings whose RHS was a bare
    // `X.new()` (no cap). Consumed (removed) on the very next stmt if that
    // stmt is `binding.cap(n)` — только соседний stmt считается «сразу
    // следом» (D-simple: no cross-stmt reordering heuristics).
    let mut pending: Option<(String, crate::diag::Span)> = None;
    for s in &b.stmts {
        // Recurse first (nested blocks/exprs), independent of the pending
        // adjacency-tracking below.
        walk_new_then_cap_stmt(s, out);
        match s {
            Stmt::Let(d) if !d.consume => {
                let is_bare_new = matches!(
                    &d.value.kind,
                    ExprKind::Call { func, args, .. } if is_new_call_without_cap(func, args)
                );
                pending = if is_bare_new {
                    if let Pattern::Ident { name, .. } = &d.pattern {
                        Some((name.clone(), d.value.span))
                    } else {
                        None
                    }
                } else {
                    None
                };
            }
            Stmt::Expr(e) => {
                if let Some((bind_name, new_span)) = &pending {
                    if let ExprKind::Call { func, args, .. } = &e.kind {
                        if let ExprKind::Member { obj, name } = &func.kind {
                            if name == "cap"
                                && args.len() == 1
                                && matches!(&obj.kind, ExprKind::Ident(v) if v == bind_name)
                            {
                                out.push(new_then_cap_warning(bind_name, *new_span, e.span));
                            }
                        }
                    }
                }
                pending = None;
            }
            _ => {
                pending = None;
            }
        }
    }
    if let Some(t) = &b.trailing {
        // Trailing expr (no-semicolon last stmt) can ALSO be the `.cap(n)`
        // half of the split-stmt pattern — parser folds a semicolon-less
        // last statement into `trailing`, not `Stmt::Expr` (D-Block).
        if let Some((bind_name, new_span)) = &pending {
            if let ExprKind::Call { func, args, .. } = &t.kind {
                if let ExprKind::Member { obj, name } = &func.kind {
                    if name == "cap"
                        && args.len() == 1
                        && matches!(&obj.kind, ExprKind::Ident(v) if v == bind_name)
                    {
                        out.push(new_then_cap_warning(bind_name, *new_span, t.span));
                    }
                }
            }
        }
        walk_new_then_cap_expr(t, out);
    }
}

fn new_then_cap_warning(
    bind_name: &str,
    new_span: crate::diag::Span,
    cap_span: crate::diag::Span,
) -> LintWarning {
    LintWarning {
        rule: "new-then-cap",
        diag: crate::diag::Diagnostic::new(
            format!(
                "`{name}.new()` immediately followed by `{name}.cap(n)` — \
                 use `.new(cap: n)` instead: one call, one allocation \
                 (canonical spelling, see Vec docs).",
                name = bind_name,
            ),
            cap_span,
        )
        .with_note_at(format!("`{}` created here", bind_name), new_span),
    }
}

fn walk_new_then_cap_stmt(s: &Stmt, out: &mut Vec<LintWarning>) {
    match s {
        Stmt::Let(d) => walk_new_then_cap_expr(&d.value, out),
        Stmt::Expr(e) => walk_new_then_cap_expr(e, out),
        Stmt::Assign { target, value, .. } => {
            walk_new_then_cap_expr(target, out);
            walk_new_then_cap_expr(value, out);
        }
        Stmt::Return { value: Some(v), .. } => walk_new_then_cap_expr(v, out),
        Stmt::Throw { value, .. } => walk_new_then_cap_expr(value, out),
        Stmt::Defer { body, .. } => walk_new_then_cap_expr(body, out),
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            walk_new_then_cap_expr(expr, out)
        }
        _ => {}
    }
}

fn walk_new_then_cap_expr(e: &Expr, out: &mut Vec<LintWarning>) {
    match &e.kind {
        // Chain form: `X.new().cap(n)` — outer Call is `.cap(...)` whose
        // Member.obj is itself a bare `X.new()` Call.
        ExprKind::Call { func, args, .. } => {
            if let ExprKind::Member { obj, name } = &func.kind {
                if name == "cap" && args.len() == 1 {
                    if let ExprKind::Call { func: inner_func, args: inner_args, .. } = &obj.kind {
                        if is_new_call_without_cap(inner_func, inner_args) {
                            // Chain has no separate binding name — describe
                            // the receiver type/callee for the message.
                            let callee_desc = describe_new_receiver(inner_func);
                            out.push(LintWarning {
                                rule: "new-then-cap",
                                diag: crate::diag::Diagnostic::new(
                                    format!(
                                        "`{recv}.new().cap(n)` chain — use \
                                         `{recv}.new(cap: n)` instead: one call, \
                                         one allocation (canonical spelling, \
                                         see Vec docs).",
                                        recv = callee_desc,
                                    ),
                                    e.span,
                                ),
                            });
                        }
                    }
                }
            }
            walk_new_then_cap_expr(func, out);
            for a in args {
                walk_new_then_cap_expr(a.expr(), out);
            }
        }
        ExprKind::Block(b) => walk_new_then_cap_block(b, out),
        ExprKind::If { cond, then, else_ } => {
            walk_new_then_cap_expr(cond, out);
            walk_new_then_cap_block(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_new_then_cap_block(b, out),
                    ElseBranch::If(if_expr) => walk_new_then_cap_expr(if_expr, out),
                }
            }
        }
        ExprKind::For { body, .. } | ExprKind::While { body, .. } => {
            walk_new_then_cap_block(body, out);
        }
        _ => {}
    }
}

/// Plan 207 cmpxchg-lint волна B (D425 амендмент) — `W_CAS_FAILURE_STRONGER`.
///
/// Warns when `strength(failure) > strength(success)` on a LITERAL
/// `compare_exchange`/`compare_exchange_weak` call: `Relaxed < Acquire ≈ Release <
/// AcqRel < SeqCst`. Valid since C++17, but almost always an intent bug — the
/// failure path (CAS did NOT happen) usually should not demand MORE synchronization
/// than the success path. Non-literal orderings (runtime variables) are not
/// diagnosable — skipped.
///
/// Receiver-type gate: **method name only** (`compare_exchange`/
/// `compare_exchange_weak`) — deliberately NOT gated on an `Atomic*`-receiver type
/// check here. Unlike the hard-error twin
/// (`types::check_cas_ordering`), this is a pure-AST lint pass (`lint_module`'s
/// `Vec<LintWarning>` sink has no type-checker state — no `self.sig`/
/// `infer_expr_type`/`resolved_types_buf` available in this file). The method-name
/// signature is unique to the `Atomic*` family in the entire language/stdlib (no
/// other type defines `compare_exchange(expected, desired, success, failure)`), so
/// the narrower gate is unnecessary here — false-positive risk is effectively nil.
///
/// `failure ∈ {Release, AcqRel}` is EXCLUDED here (that's the hard-error's job —
/// `E_CAS_FAILURE_ORDER_INVALID` in `types.rs`; a call site failing that check would
/// otherwise not compile at all, so double-reporting a warning on it is moot).
fn lint_cas_failure_stronger(call_expr: &Expr, func: &Expr, args: &[CallArg], out: &mut Vec<LintWarning>) {
    let ExprKind::Member { name: method_name, .. } = &func.kind else { return; };
    if method_name != "compare_exchange" && method_name != "compare_exchange_weak" {
        return;
    }
    if args.len() < 2 { return; }
    // Plan 207 cmpxchg-rename: single default-param signature, not 2-arg/4-arg
    // overloads — an omitted `success`/`failure` arg is the known literal `SeqCst`
    // default (still diagnosable), not a "can't tell" skip.
    let success: &str = match cas_lint_arg(args, "success", 2) {
        Some(a) => match crate::types::mem_ordering_variant(a) { Some(v) => v, None => return },
        None => "SeqCst",
    };
    let failure: &str = match cas_lint_arg(args, "failure", 3) {
        Some(a) => match crate::types::mem_ordering_variant(a) { Some(v) => v, None => return },
        None => "SeqCst",
    };
    // Hard-error territory — не дублируем warning поверх E_CAS_FAILURE_ORDER_INVALID.
    if matches!(failure, "Release" | "AcqRel") { return; }
    if crate::types::mem_ordering_strength(failure) > crate::types::mem_ordering_strength(success) {
        out.push(LintWarning {
            rule: "W_CAS_FAILURE_STRONGER",
            diag: crate::diag::Diagnostic::new(
                format!(
                    "W_CAS_FAILURE_STRONGER: `{method}` — failure-ordering \
                     `MemOrdering.{failure}` строже success-ordering \
                     `MemOrdering.{success}` (Relaxed < Acquire≈Release < AcqRel < \
                     SeqCst); валидно с C++17, но почти всегда ошибка намерения — \
                     failure-путь (CAS не удался, значение не изменено) обычно не \
                     должен требовать БОЛЬШЕ синхронизации, чем success-путь.",
                    method = method_name, failure = failure, success = success,
                ),
                call_expr.span,
            ),
        });
    }
}

/// Plan 207 cmpxchg-lint: достать call-arg для именованного параметра `param_name`
/// на позиции `pos` (см. `types::check_cas_ordering`'s `cas_call_arg` — то же
/// правило, отдельная копия: разные файлы/сигнатуры `CallArg`-обхода, не стоит
/// городить кросс-модульный `pub(crate)` ради 8 строк).
fn cas_lint_arg<'x>(args: &'x [CallArg], param_name: &str, pos: usize) -> Option<&'x Expr> {
    for a in args {
        if let CallArg::Named { name, value } = a {
            if name == param_name { return Some(value); }
        }
    }
    match args.get(pos) {
        Some(CallArg::Named { .. }) | None => None,
        Some(a) => Some(a.expr()),
    }
}

/// Plan 52 Ф.2: рекурсивный обход выражения. На каждом `MapLit` запускает
/// map-литерал lints; рекурсивно спускается во все под-выражения.
fn walk_expr_lints(e: &Expr, out: &mut Vec<LintWarning>) {
    if let ExprKind::MapLit { elems, .. } = &e.kind {
        let pairs = crate::ast::MapElem::cloned_pairs(&elems);
        check_map_literal_lints(&pairs, out);
    }
    match &e.kind {
        ExprKind::MapLit { elems, .. } => {
                let pairs = crate::ast::MapElem::cloned_pairs(&elems);
            for (k, v) in pairs.iter() {
                walk_expr_lints(k, out);
                walk_expr_lints(v, out);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => walk_expr_lints(x, out),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for x in elems { walk_expr_lints(x, out); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { walk_expr_lints(v, out); }
            }
        }
        ExprKind::Call { func, args, trailing } => {
            // Plan 207 cmpxchg-lint волна B: W_CAS_FAILURE_STRONGER.
            lint_cas_failure_stronger(e, func, args, out);
            walk_expr_lints(func, out);
            for a in args { walk_expr_lints(a.expr(), out); }
            if let Some(t) = trailing {
                match t {
                    crate::ast::Trailing::Block(b) => walk_block_lints(b, out),
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => walk_block_lints(&tb.body, out),
                    crate::ast::Trailing::Fn(sb) => match &sb.body {
                        FnBody::Expr(x) => walk_expr_lints(x, out),
                        FnBody::Block(b) => walk_block_lints(b, out),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::TurboFish { base, .. } => walk_expr_lints(base, out),
        ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => walk_expr_lints(x, out),
        ExprKind::Coalesce(a, b) => { walk_expr_lints(a, out); walk_expr_lints(b, out); }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => walk_expr_lints(x, out),
        ExprKind::Binary { left, right, .. } => {
            walk_expr_lints(left, out); walk_expr_lints(right, out);
        }
        ExprKind::Unary { operand, .. } => walk_expr_lints(operand, out),
        ExprKind::Member { obj, .. } => walk_expr_lints(obj, out),
        ExprKind::Index { obj, index } => {
            walk_expr_lints(obj, out); walk_expr_lints(index, out);
        }
        ExprKind::If { cond, then, else_ } => {
            walk_expr_lints(cond, out);
            walk_block_lints(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block_lints(b, out),
                    ElseBranch::If(x) => walk_expr_lints(x, out),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr_lints(scrutinee, out);
            walk_block_lints(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => walk_block_lints(b, out),
                    ElseBranch::If(x) => walk_expr_lints(x, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_lints(scrutinee, out);
            for arm in arms {
                if let Some(g) = &arm.guard { walk_expr_lints(g, out); }
                match &arm.body {
                    MatchArmBody::Expr(x) => walk_expr_lints(x, out),
                    MatchArmBody::Block(b) => walk_block_lints(b, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr_lints(iter, out); walk_block_lints(body, out);
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr_lints(cond, out); walk_block_lints(body, out);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            walk_expr_lints(scrutinee, out); walk_block_lints(body, out);
        }
        ExprKind::Loop { body, .. } => walk_block_lints(body, out),
        ExprKind::Block(b) => walk_block_lints(b, out),
        ExprKind::Spawn(x) => walk_expr_lints(x, out),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => walk_block_lints(b, out),
        ExprKind::Supervised { body, cancel, deadline } => {
            walk_block_lints(body, out);
            if let Some(c) = cancel { walk_expr_lints(c, out); }
            if let Some(_dl) = deadline { walk_expr_lints(&_dl.expr, out); }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            walk_block_lints(body, out);
        }
        ExprKind::Throw(x) => walk_expr_lints(x, out),
        ExprKind::Interrupt(opt) => {
            if let Some(x) = opt { walk_expr_lints(x, out); }
        }
        // [E_COALESCE_RETURN_FALLBACK]: checker-rejected before this pass.
        ExprKind::CoalesceReturnFallback(opt) => {
            if let Some(x) = opt { walk_expr_lints(x, out); }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { walk_expr_lints(s, out); }
            if let Some(e) = end { walk_expr_lints(e, out); }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let crate::ast::InterpStrPart::Expr { expr: x, spec: _ } = p { walk_expr_lints(x, out); }
            }
        }
        ExprKind::TaggedTemplate { args, .. } => {
            for x in args { walk_expr_lints(x, out); }
        }
        ExprKind::Lambda { body, .. } => walk_expr_lints(body, out),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(x) => walk_expr_lints(x, out),
            ClosureBody::Block(b) => walk_block_lints(b, out),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Expr(x) => walk_expr_lints(x, out),
            FnBody::Block(b) => walk_block_lints(b, out),
            FnBody::External => {}
        },
        ExprKind::With { bindings, body } => {
            for b in bindings { walk_expr_lints(&b.handler, out); }
            walk_block_lints(body, out);
        }
        // Plan 97 Ф.4 (D142): protocol-литерал — lint-walk идентичен.
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(x) => walk_expr_lints(x, out),
                    HandlerMethodBody::Block(b) => walk_block_lints(b, out),
                }
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    crate::ast::SelectOp::Recv { chan, .. } => walk_expr_lints(chan, out),
                    crate::ast::SelectOp::Send { chan, value } => {
                        walk_expr_lints(chan, out); walk_expr_lints(value, out);
                    }
                    crate::ast::SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { walk_expr_lints(g, out); }
                walk_block_lints(&arm.body, out);
            }
        }
        // Plan 33.3 Ф.13: Forall/Exists — spec quantifiers.
        ExprKind::Forall { body, .. } | ExprKind::Exists { body, .. } => {
            walk_expr_lints(body, out);
        }
        // Листовые — нет под-выражений.
        ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
        | ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
        | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
        | ExprKind::HexBlobLit(_) | ExprKind::NullPtrLit => {}
    }
}

/// Plan 52 Ф.2 (D108): lint-проверки map-литерала `[k: v]`.
///
/// - **duplicate-map-key**: два ключа — одинаковые compile-time константы
///   (int/str/bool literal). Last-wins семантика, но второй entry молча
///   затирает первый — паритет с `go vet` / `tsc`. Произвольные выражения
///   (`a`, `a+1`, `f()`) не проверяются.
/// - **nan-map-key**: ключ — константа `f64.NAN` / `f32.NAN`. По IEEE 754
///   `NaN != NaN`, поэтому вставленный ключ невозможно найти обратно.
fn check_map_literal_lints(pairs: &[(Expr, Expr)], out: &mut Vec<LintWarning>) {
    // NaN-key: ключ это Path(["f64", "NAN"]) или Path(["f32", "NAN"]).
    for (k, _) in pairs.iter() {
        if let ExprKind::Path(parts) = &k.kind {
            if parts.len() == 2
                && (parts[0] == "f64" || parts[0] == "f32")
                && parts[1] == "NAN"
            {
                out.push(LintWarning {
                    rule: "nan-map-key",
                    diag: Diagnostic::new(
                        format!(
                            "warning: `{}.NAN` as map key — inserted key can never be \
                             found (IEEE 754: NaN != NaN). Consider a sentinel value or \
                             a non-float key type.",
                            parts[0]
                        ),
                        k.span,
                    ),
                });
            }
        }
    }
    // duplicate-map-key: сравниваем константные ключи попарно. Канонизируем
    // в строковый дескриптор; non-const ключи дают None и не сравниваются.
    let consts: Vec<(Option<String>, Span)> = pairs
        .iter()
        .map(|(k, _)| (const_key_descriptor(k), k.span))
        .collect();
    for i in 0..consts.len() {
        let (Some(desc_i), _) = (&consts[i].0, consts[i].1) else { continue };
        for j in (i + 1)..consts.len() {
            let (Some(desc_j), span_j) = (&consts[j].0, consts[j].1) else { continue };
            if desc_i == desc_j {
                out.push(LintWarning {
                    rule: "duplicate-map-key",
                    diag: Diagnostic::new(
                        format!(
                            "warning: duplicate key `{}` in map literal — the later \
                             entry overwrites the earlier one (last-wins)",
                            human_key(&consts[j].0, pairs, j)
                        ),
                        span_j,
                    ),
                });
                break; // один warning на дубликат — не плодим N²
            }
        }
    }
}

/// Канонический дескриптор compile-time-константного ключа для сравнения
/// дубликатов. `None` — ключ не является распознаваемой константой.
/// Дескриптор включает префикс типа, чтобы `1` (int) и `"1"` (str) не
/// считались дубликатами.
fn const_key_descriptor(k: &Expr) -> Option<String> {
    match &k.kind {
        ExprKind::IntLit(n) => Some(format!("int:{n}")),
        ExprKind::StrLit(s) => Some(format!("str:{s}")),
        ExprKind::BoolLit(b) => Some(format!("bool:{b}")),
        ExprKind::CharLit(c) => Some(format!("char:{c}")),
        // Унарный минус над int-литералом — `-1` как ключ.
        ExprKind::Unary { op: crate::ast::UnOp::Neg, operand } => {
            if let ExprKind::IntLit(n) = &operand.kind {
                Some(format!("int:{}", -n))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Человекочитаемое представление ключа для текста warning'а.
fn human_key(desc: &Option<String>, pairs: &[(Expr, Expr)], idx: usize) -> String {
    match &pairs[idx].0.kind {
        ExprKind::IntLit(n) => n.to_string(),
        ExprKind::StrLit(s) => format!("\"{s}\""),
        ExprKind::BoolLit(b) => b.to_string(),
        ExprKind::CharLit(c) => {
            char::from_u32(*c).map(|ch| format!("'{ch}'")).unwrap_or_else(|| format!("'\\u{{{c:x}}}'"))
        }
        _ => desc.clone().unwrap_or_else(|| "<key>".to_string()),
    }
}

/// Собирает имена user-defined эффектов: `type X effect { ... }`.
/// Также включает встроенные stdlib effects из prelude (D26 + D62).
fn collect_effect_names(m: &Module) -> HashSet<String> {
    let mut names: HashSet<String> = [
        "Fail", "Io", "Net", "Db", "Fs", "Time", "Random",
        "Log", "Trace", "Ask", "Alloc", "Detach", "Blocking", "Mem",
    ].iter().map(|s| s.to_string()).collect();
    for item in &m.items {
        if let Item::Type(td) = item {
            if matches!(td.kind, TypeDeclKind::Effect(_)) {
                names.insert(td.name.clone());
            }
        }
    }
    names
}

/// Собирает имена user-defined protocols: `type X protocol { ... }`.
/// Также включает встроенные prelude protocols.
///
/// Plan 15 D53 strict: после split'а `TypeDeclKind::Protocol(_)` —
/// scan'имся по нему напрямую (раньше было закомменчено потому что
/// все protocols/effects попадали в Effect-variant).
fn collect_protocol_names(m: &Module) -> HashSet<String> {
    // Plan 62.D non-opaque: `Iter` мигрирован в std/prelude/collections.nv.
    // Plan 62.E: `From`, `Into`, `Hashable`, `Display` (+ новые `Equatable`,
    // `Comparable`) мигрированы в std/prelude/protocols.nv — auto-imported
    // через R27 в каждый module, попадают в `m.items` через
    // `resolve_imports_inline` и captures'ятся for-loop'ом ниже. `TryFrom`/
    // `TryInto` deferred (Plan 56 Ф.2.7 effect-row enforcement), но они и
    // не нужны в этом lint-HashSet'е (он используется только для
    // protocol-in-effect-position warning'а на bare-name idents).
    //
    // **Остаются hardcoded:**
    //   - `Ord`, `Eq`, `ToStr` — legacy aliases (используются в
    //     nova_tests/types/generics.nv `TwoBounds[K Hashable, V Eq]`,
    //     std/encoding/json.nv comments etc.). Канонические имена per
    //     D109 — `Comparable`/`Equatable`, но `Ord`/`Eq` остаются как
    //     back-compat имена пока тесты не переписаны.
    //   - `TryFrom`, `TryInto` — deferred protocol declarations (Plan
    //     56 Ф.2.7), keep лint coverage пока formal decl не появится.
    let mut names: HashSet<String> = [
        "Ord", "Eq", "ToStr", "TryFrom", "TryInto",
    ].iter().map(|s| s.to_string()).collect();
    for item in &m.items {
        if let Item::Type(td) = item {
            if matches!(td.kind, TypeDeclKind::Protocol { .. }) {
                names.insert(td.name.clone());
            }
        }
    }
    names
}

/// Rule: `protocol-in-effect-position` — `fn f() Hashable -> ()` где
/// `Hashable` это protocol. Should be `fn f(x T Hashable) -> ()` (как
/// generic-bound на параметре, D72) или `fn f[T Hashable](x T) -> ()`.
fn check_protocol_in_effect_position(
    f: &FnDecl,
    protocols: &HashSet<String>,
    effects: &HashSet<String>,
    out: &mut Vec<LintWarning>,
) {
    for eff in &f.effects {
        if let TypeRef::Named { path, .. } = eff {
            if path.len() == 1 {
                let name = &path[0];
                if protocols.contains(name) && !effects.contains(name) {
                    out.push(LintWarning {
                        rule: "protocol-in-effect-position",
                        diag: Diagnostic::new(
                            format!(
                                "warning: `{}` is a protocol, not an effect, but appears in \
                                 effect-position (between `)` and `->`) of fn `{}` \
                                 (D62: protocols are structural type-bounds, not handler-substitutable; \
                                 use `fn {} (x T {}) -> ...` or generic-bound `[T {}]` instead)",
                                name, f.name, f.name, name, name
                            ),
                            eff.span(),
                        ),
                    });
                }
            }
        }
    }
}

fn check_fn(f: &FnDecl, out: &mut Vec<LintWarning>) {
    if !f.is_export {
        return;
    }
    // Rule: export-fail-untyped — `Fail` без [E] в public API.
    for eff in &f.effects {
        if is_fail_untyped(eff) {
            let span = eff.span();
            out.push(LintWarning {
                rule: "export-fail-untyped",
                diag: Diagnostic::new(
                    format!(
                        "warning: export fn `{}` uses `Fail` without type parameter \
                         (D65 convention: public API should specify `Fail[E]` with concrete error type; \
                         use `Fail[any]` to opt into explicit erasure)",
                        f.name
                    ),
                    span,
                ),
            });
        }
    }
}

/// [M-canon-mut-param-position] (owner decision 2026-07-17, research-mut-canon
/// follow-up): mut-параметров канон — ПРЕФИКСНАЯ форма `mut name Type`. Голая
/// постфиксная форма `name mut Type` (D6 legacy synonym, БЕЗ предшествующего
/// `ro`) — полный поведенческий синоним префиксной формы (эмпирика владельца:
/// `i mut int` реассайнится в теле идентично `mut i int`); позиция ТИПА
/// зарезервирована исключительно за view-слайсами (`[]u8` и родня — io-канон,
/// `buf mut []u8`), для прочих типов — footgun-спеллинг, под запрет.
///
/// Санкционированный D246 R2-split `ro name mut Type` (explicit `ro` L1 +
/// постфиксный `mut` L2, Plan 118.5 V3 amend) НЕ флагуется here — parser
/// (`parse_param`) не отмечает `mut_type_pos_legacy` для этой формы (см. поле
/// `Param::mut_type_pos_legacy`), так что она отфильтрована уже на входе.
///
/// Unconditional pipeline (owner-directed: NOT opt-in `CONV_RULES` — runs on
/// every fn signature check, same tier as `check_fn`/`check_assume_trust` above).
fn check_param_type_pos_mut(f: &FnDecl, out: &mut Vec<LintWarning>) {
    for p in &f.params {
        if !p.mut_type_pos_legacy {
            continue;
        }
        // Exception: view-слайсы (`[]T`) и fixed-size массивы (`[N]T`) — оба
        // легитимный io-канон-«родня» (`buf mut []u8` byte-sink, `out mut
        // [32]u8` hash-digest out-buffer — сверено по факту в std/crypto
        // sha256.nv/md5.nv/hmac.nv/jwt.nv/uuid_namespace.nv, [M-canon-mut-param-position]
        // blast-radius sweep 2026-07-17).
        if matches!(p.ty, TypeRef::Array(..) | TypeRef::FixedArray(..)) {
            continue;
        }
        let ty_str = crate::types::render_type_ref(&p.ty);
        out.push(LintWarning {
            rule: "W_PARAM_TYPE_POS_MUT",
            diag: Diagnostic::new(
                format!(
                    "warning: параметр `{}` объявлен постфиксной формой `{} mut {}` \
                     [W_PARAM_TYPE_POS_MUT] — канон mut-параметров (owner decision \
                     2026-07-17): mut ПЕРЕД именем, `mut {} {}`. Позиция ПОСЛЕ имени \
                     зарезервирована за view-слайсами (`[]u8` и родня, io-канон, \
                     `buf mut []u8`); для прочих типов постфиксная форма — запрещённый \
                     синоним префиксной (ведёт себя идентично, D6 legacy spelling).",
                    p.name, p.name, ty_str, p.name, ty_str,
                ),
                p.span,
            ),
        });
    }
}

/// Plan 33.8 Ф.3.1: `assume` вне `#trusted`-функции вводит непроверяемое
/// допущение (rule `trust-introduced`). Внутри `#trusted` функции допущение
/// разрешено молча — граница доверия объявлена явно.
fn check_assume_trust(f: &FnDecl, out: &mut Vec<LintWarning>) {
    if f.is_trusted {
        return;
    }
    let mut spans: Vec<Span> = Vec::new();
    if let FnBody::Block(b) = &f.body {
        collect_marked_spans_block(
            b,
            &|s| match s { Stmt::Assume { span, .. } => Some(*span), _ => None },
            &mut spans,
        );
    }
    for sp in spans {
        out.push(LintWarning {
            rule: "trust-introduced",
            diag: Diagnostic::new(
                format!(
                    "warning: `assume` в функции `{}` вводит непроверяемое \
                     допущение [trust-introduced]: верификатор принимает его \
                     без доказательства — ошибочное `assume` делает любой \
                     контракт «доказуемым». Пометьте функцию `#trusted`, если \
                     допущение намеренно (FFI / внешнее знание).",
                    f.name
                ),
                sp,
            ),
        });
    }
}

/// Plan 33.8 Ф.6.3: `assert_static` в V1 НЕ верифицируется SMT — модель
/// верификатора flow-insensitive (нужно знать состояние именно в точке
/// assert'а). Действует как обычный runtime-assert (debug; в release
/// стирается). Предупреждаем, чтобы не было ложной уверенности
/// «обязательство доказано статически».
fn check_assert_static_unverified(f: &FnDecl, out: &mut Vec<LintWarning>) {
    let mut spans: Vec<Span> = Vec::new();
    if let FnBody::Block(b) = &f.body {
        collect_marked_spans_block(
            b,
            &|s| match s { Stmt::AssertStatic { span, .. } => Some(*span), _ => None },
            &mut spans,
        );
    }
    for sp in spans {
        out.push(LintWarning {
            rule: "assert-static-unverified",
            diag: Diagnostic::new(
                format!(
                    "warning: `assert_static` в функции `{}` НЕ верифицируется \
                     статически в V1 [assert-static-unverified]: действует как \
                     runtime-проверка (debug), в release стирается. Полная \
                     compile-time верификация требует flow-sensitive анализа \
                     (Plan 33.8 → V2). Для гарантированной проверки выразите \
                     факт контрактом `ensures`.",
                    f.name
                ),
                sp,
            ),
        });
    }
}

/// Plan 33.8: обход тела функции — собирает span'ы statement'ов, для
/// которых `matcher` вернул Some. Рекурсивно спускается в блоки/циклы/
/// if/match. Используется lint'ами `trust-introduced` и
/// `assert-static-unverified`.
fn collect_marked_spans_block(
    b: &Block,
    matcher: &dyn Fn(&Stmt) -> Option<Span>,
    out: &mut Vec<Span>,
) {
    for s in &b.stmts {
        collect_marked_spans_stmt(s, matcher, out);
    }
    if let Some(t) = &b.trailing {
        collect_marked_spans_expr(t, matcher, out);
    }
}

fn collect_marked_spans_stmt(
    s: &Stmt,
    matcher: &dyn Fn(&Stmt) -> Option<Span>,
    out: &mut Vec<Span>,
) {
    if let Some(sp) = matcher(s) {
        out.push(sp);
    }
    match s {
        Stmt::Expr(e) => collect_marked_spans_expr(e, matcher, out),
        Stmt::Let(ld) => collect_marked_spans_expr(&ld.value, matcher, out),
        Stmt::Return { value: Some(v), .. } => collect_marked_spans_expr(v, matcher, out),
        Stmt::Throw { value, .. } => collect_marked_spans_expr(value, matcher, out),
        Stmt::Defer { body, .. } => {
            collect_marked_spans_expr(body, matcher, out)
        }
        _ => {}
    }
}

fn collect_marked_spans_expr(
    e: &Expr,
    matcher: &dyn Fn(&Stmt) -> Option<Span>,
    out: &mut Vec<Span>,
) {
    match &e.kind {
        ExprKind::Block(b) => collect_marked_spans_block(b, matcher, out),
        ExprKind::If { then, else_, .. } => {
            collect_marked_spans_block(then, matcher, out);
            match else_ {
                Some(ElseBranch::Block(b)) => collect_marked_spans_block(b, matcher, out),
                Some(ElseBranch::If(ei)) => collect_marked_spans_expr(ei, matcher, out),
                None => {}
            }
        }
        ExprKind::IfLet { then, else_, .. } => {
            collect_marked_spans_block(then, matcher, out);
            match else_ {
                Some(ElseBranch::Block(b)) => collect_marked_spans_block(b, matcher, out),
                Some(ElseBranch::If(ei)) => collect_marked_spans_expr(ei, matcher, out),
                None => {}
            }
        }
        ExprKind::While { body, .. }
        | ExprKind::WhileLet { body, .. }
        | ExprKind::Loop { body, .. }
        | ExprKind::For { body, .. }
        | ExprKind::ParallelFor { body, .. } => collect_marked_spans_block(body, matcher, out),
        ExprKind::Match { arms, .. } => {
            for arm in arms {
                match &arm.body {
                    MatchArmBody::Expr(ae) => collect_marked_spans_expr(ae, matcher, out),
                    MatchArmBody::Block(b) => collect_marked_spans_block(b, matcher, out),
                }
            }
        }
        _ => {}
    }
}

/// `Fail` без generic-параметра. Не путаем с `Fail[E]` (typed) или
/// `Fail[any]` (явная erasure — программист сознательно opt-in).
fn is_fail_untyped(ty: &TypeRef) -> bool {
    if let TypeRef::Named { path, generics, .. } = ty {
        if path.len() == 1 && path[0] == "Fail" && generics.is_empty() {
            return true;
        }
    }
    false
}

// ============================================================================
// Plan 185 — реестр конвенционных W_*-правил (`nova lint` / `nova check --lint`).
//
// Архитектура (финальная, не MVP): правило = самостоятельная единица
// { id W_*, summary, хук }. Хук — либо AST-walker по `Module` (после parse,
// БЕЗ type-check/import-resolution: правила синтаксические), либо текстовая
// эвристика по исходнику файла (для «греп»-строк карты Ф.0). Никакой привязки
// к check-пайплайну сверх точки вызова `run_conv_rules`.
//
// Точки входа:
//   - `nova lint [paths]` (nova-cli) — прогон реестра по .nv-файлам;
//   - `nova check --lint` — те же правила поверх ТОГО ЖЕ реестра.
//
// Правила, требующие семантики (типов), реализованы консервативной
// синтаксической версией и помечены `// SEMANTIC-UPGRADE:` — НЕ молча.
// ============================================================================

/// Контекст файла для реестра (вычисляется вызывающей стороной по пути).
#[derive(Debug, Clone, Copy, Default)]
pub struct ConvLintOptions {
    /// Файл принадлежит std-поверхности (правила уровня «public std»).
    pub in_std: bool,
    /// Файл внутри `std/collections/vec/` — definition-site `Vec[T]`
    /// (W_VEC_SPELLING там не действует).
    pub in_vec_module: bool,
    /// Файл — тест (`*_test.nv` / nova_tests): в тестах канон владельца —
    /// `Vec[T].of(a, b, c)` (вариадик), W_VEC_SPELLING не действует.
    pub in_test: bool,
    /// Владелец 2026-07-21: файл внутри `std/src/runtime/string/**` —
    /// реализация самого str-примитива (`@concat`/`@bytes`/etc.,
    /// std/src/runtime/string/transform.nv и соседи) — `W_STR_CONCAT_METHOD`
    /// там не действует (это canon-определение, не сайт для канонизации).
    pub in_str_runtime_impl: bool,
}

/// Одно конвенционное правило реестра.
pub struct ConvRule {
    /// Стабильный id (`W_*`) — используется в выводе и `--rule` фильтре.
    pub id: &'static str,
    /// Однострочное описание (для `nova lint --list-rules` / доков).
    pub summary: &'static str,
    /// AST-хук: модуль после parse (peer_files может быть пуст).
    pub ast: Option<fn(&Module, &ConvLintOptions, &mut Vec<LintWarning>)>,
    /// Текст-хук: сырой исходник файла (для греп-эвристик).
    pub text: Option<fn(&str, &ConvLintOptions, &mut Vec<LintWarning>)>,
}

/// Реестр правил карты Ф.0 плана 185.
pub const CONV_RULES: &[ConvRule] = &[
    ConvRule {
        id: "W_NONVARIADIC_OF",
        summary: "static `of` без вариадик-параметра — `of` зарезервирован за \
                  вариадик-коллекциями (nv-coding-style §21б)",
        ast: Some(conv_nonvariadic_of),
        text: None,
    },
    ConvRule {
        id: "W_RETIRED_PREFIX",
        summary: "префикс `as_` в имени функции/метода ретрактирован (D410): \
                  вид = голое существительное",
        ast: Some(conv_retired_prefix),
        text: None,
    },
    ConvRule {
        id: "W_ACCESSOR_PAIR",
        summary: "пара `get_x`/`set_x` — канон: методы-свойства одним именем \
                  по арности `@x()` / `mut @x(v) -> @` (D117 AMEND)",
        ast: Some(conv_accessor_pair),
        text: None,
    },
    ConvRule {
        id: "W_WITH_MUTATOR",
        summary: "`with_*` с mut-приёмником — `with_*` всегда возвращает НОВОЕ \
                  значение; мутирующее свойство = `mut @x(v) -> @` (nv-coding-style §21)",
        ast: Some(conv_with_mutator),
        text: None,
    },
    ConvRule {
        id: "W_STATIC_CONVERSION",
        summary: "статик-конверсия `T.from(x)` / `T.parse(s)` — запрещённая пятая \
                  дверь (§1а, ретракция 2026-07-09): канон `x.to_*()`",
        ast: Some(conv_static_conversion),
        text: None,
    },
    ConvRule {
        id: "W_CONSUME_NAKED_NAME",
        summary: "`consume`-receiver + голое имя-вид, конвертирующее в ДРУГОЙ тип \
                  — потребление обязано называться `@into_*()` (§1а, ось \
                  владения; голое имя зарезервировано за zero-copy видом, \
                  который receiver не потребляет)",
        ast: Some(conv_consume_naked_name),
        text: None,
    },
    ConvRule {
        id: "W_TRY_WITHOUT_SIBLING",
        summary: "`try_*` без инфаллибельного сиблинга — префикс `try_` только \
                  для пары infallible/fallible (R3 D325)",
        ast: Some(conv_try_without_sibling),
        text: None,
    },
    ConvRule {
        id: "W_SETTER_NOT_FLUENT",
        summary: "1-арный метод-свойство `mut @x(v)` не возвращает `@` — сеттер \
                  обязан быть беглым `-> @` (D117 AMEND-2)",
        ast: Some(conv_setter_not_fluent),
        text: None,
    },
    ConvRule {
        id: "W_FFI_BARE_HANDLE",
        summary: "голый `int`/`*()` хендл в extern-семействе с new/open+free/close \
                  — канон: newtype `type CFooHandle(int)` (module-conventions §4а)",
        ast: Some(conv_ffi_bare_handle),
        text: None,
    },
    ConvRule {
        id: "W_MANUAL_SLICE_COPY",
        summary: "поэлементная копия `push(x[i])` в цикле — красный флаг: \
                  `[]T`-вид среза даёт то же за O(1) (nv-coding-style §18а)",
        ast: Some(conv_manual_slice_copy),
        text: None,
    },
    ConvRule {
        id: "W_IMMUTABLE_REBUILD_SETTER",
        summary: "не-mut метод пересобирает Self всеми полями (OpenOptions-класс) \
                  — для кучевых записей канон `mut @x(v) -> @` (D117/D409)",
        ast: Some(conv_immutable_rebuild_setter),
        text: None,
    },
    ConvRule {
        id: "W_STR_CONCAT_LOOP",
        summary: "`buf = buf + x` / `buf += \"...\"` в цикле — O(N²); канон \
                  StringBuilder (perf-conventions)",
        ast: Some(conv_str_concat_loop),
        text: None,
    },
    ConvRule {
        id: "W_STR_CONCAT_METHOD",
        summary: "`.concat(...)` вызов на str-выражении — канон строковая \
                  интерполяция \"${a}${b}\" (владелец 2026-07-21, тот же \
                  D-амендмент что и E_STR_CONCAT_PLUS, spec/decisions/02-types.md)",
        ast: Some(conv_str_concat_method),
        text: None,
    },
    ConvRule {
        id: "W_RESULT_DISCARDED",
        summary: "тихое глотание Result: `ro _ = fallible()` / swallow-match \
                  `Err(_) => ()` (nv-coding-style §4)",
        ast: Some(conv_result_discarded),
        text: None,
    },
    ConvRule {
        id: "W_PARAM_NO_CONTRACT",
        summary: "index/offset/len-параметр публичной std-fn без `requires` \
                  (nv-coding-style §5, норма приёмки 2026-07-07)",
        ast: Some(conv_param_no_contract),
        text: None,
    },
    ConvRule {
        id: "W_VEC_SPELLING",
        summary: "`Vec[` вне std/collections/vec — канон `[]T` (D238/D239); \
                  легальные исключения несут маркер `[M-...]` на строке",
        ast: None,
        text: Some(conv_vec_spelling),
    },
    ConvRule {
        id: "W_RETIRED_NAME",
        summary: "ретрактированные вызовы: nth/to_bytes/to_chars/.into()/\
                  with_capacity/from_raw_parts (греп-инварианты D-блоков)",
        ast: None,
        text: Some(conv_retired_name),
    },
    ConvRule {
        id: "W_FAIL_PUBLIC_SIGNATURE",
        summary: "`Fail[...]` в публичной std-сигнатуре собственных ошибок — \
                  канон Result (R5 D325)",
        ast: None,
        text: Some(conv_fail_public_signature),
    },
    ConvRule {
        id: "W_DESTRUCTURE_SNAPSHOT",
        summary: "2+ соседних `ro`/`mut`-биндинга — полевые снапшоты одного \
                  источника — канон D411 record-деструктуризация \
                  (nv-coding-style §26)",
        ast: Some(conv_destructure_snapshot),
        text: None,
    },
    ConvRule {
        id: "W_LEADING_BINOP_CONTINUATION",
        summary: "ведущий бинарный оператор (`||`/`&&`/`+`/…) в начале \
                  продолжающей строки многострочного выражения — footgun: \
                  ведущий `||` парсится как zero-arg closure-литерал \
                  (D417-класс); канон — trailing-оператор в конце строки \
                  (nv-coding-style §27)",
        ast: None,
        text: Some(conv_leading_binop_continuation),
    },
    ConvRule {
        id: "W_REDUNDANT_OF",
        summary: "`Vec[T].of(...)` избыточен — литерал `[...]` дал бы ТОТ ЖЕ \
                  тип (nv-coding-style §28)",
        ast: Some(conv_redundant_of),
        text: None,
    },
    ConvRule {
        id: "W_NON_COMPOUND_ASSIGN",
        summary: "`x = x OP e` при существующем компаунде `x OP= e` \
                  (`+=`/`-=`/`*=`/`/=` — nv-coding-style §29)",
        ast: Some(conv_non_compound_assign),
        text: None,
    },
    ConvRule {
        id: "W_WHILE_COUNTER_FOR_RANGE",
        summary: "счётчиковый `while i < end { ...; i += 1 }` — канон \
                  `for i in start..end` (nv-coding-style §10)",
        ast: Some(conv_while_counter_for_range),
        text: None,
    },
    ConvRule {
        id: "W_COERCE_EXPLICIT_REDUNDANT",
        summary: "явный `.bytes()`/`.into_str()`/`.into_bytes()`/… (реестро-\
                  ориентированно, из видимых `#coerce fn`-деклараций) в позиции \
                  с явным ожидаемым типом ИЛИ call-аргументом на синтаксически-\
                  гарантированном значении (литерал/интерполяция/`.to_str()`-\
                  чейн) — голое значение скоэрсировалось бы в ТО ЖЕ САМОЕ через \
                  `#coerce` (D429 R6/R9, Plan 214; call-arg лейн + реестро-\
                  ориентация — владелец 2026-07-21, поглотил бывший \
                  W_REDUNDANT_BYTES_ON_LITERAL)",
        ast: Some(conv_coerce_explicit_redundant),
        text: None,
    },
    // W_MANUAL_CLAMP перечислен рядом с W_MANUAL_MIN_MAX (не порядково
    // значимо — min/max дедупит свои находки самостоятельным прогоном
    // `conv_collect_clamp_consumed_spans`, не завися от позиции в реестре;
    // см. блок-комментарий у `conv_manual_min_max`).
    ConvRule {
        id: "W_MANUAL_CLAMP",
        summary: "ручной трёхветочный `if x < lo {lo} else if x > hi {hi} else {x}` \
                  — канон `x.clamp(lo, hi)` (nv-coding-style §30, прецедент clippy \
                  manual_clamp)",
        ast: Some(conv_manual_clamp),
        text: None,
    },
    ConvRule {
        id: "W_MANUAL_MIN_MAX",
        summary: "ручной `if a > b {a} else {b}` (и зеркала `</>=/<=`, statement-форма \
                  `if x > hi {x = hi}`) — канон `a.max(b)`/`a.min(b)` (nv-coding-style \
                  §30, прецедент clippy manual_min/manual_max)",
        ast: Some(conv_manual_min_max),
        text: None,
    },
    ConvRule {
        id: "W_REDUNDANT_CONSUME_REBIND",
        summary: "`consume y = x` в теле match-арма, где `x` — уже `consume`-биндинг \
                  ИЗ ПАТТЕРНА того же арма и больше нигде не используется — бинди сразу \
                  в паттерне (владелец 2026-07-21)",
        ast: Some(conv_redundant_consume_rebind),
        text: None,
    },
    ConvRule {
        id: "W_MANUAL_CLOSE_AUTO_CLEANUP",
        summary: "хвостовой ручной finalize-вызов на `consume`-биндинге типа с \
                  `consume @cleanup` (D432) — авто-cleanup на выходе из скоупа делает \
                  вызов избыточным (владелец 2026-07-21)",
        ast: Some(conv_manual_close_auto_cleanup),
        text: None,
    },
    ConvRule {
        id: "W_REDUNDANT_CONST_TYPE_ANNOTATION",
        summary: "аннотация типа у `const`, СОВПАДАЮЩАЯ с дефолтным типом литерала-\
                  инициализатора (str/int/bool/char) — тип и так выводится (владелец \
                  2026-07-21)",
        ast: Some(conv_redundant_const_type_annotation),
        text: None,
    },
    ConvRule {
        id: "W_MANUAL_COALESCE",
        summary: "ручной `match X { Ok(v) => v, Err(_) => D }` / `{ Some(v) => v, \
                  None => D }` (identity-рука) — дрейф от канона `X ?? D` (D86 AMEND \
                  2026-07-23, [M-manual-coalesce-lint-missing])",
        ast: Some(conv_manual_coalesce),
        text: None,
    },
    ConvRule {
        id: "W_MANUAL_COLLECT",
        summary: "ручной collect `mut v = <пустой ctor>; for x in it { v.push(x) }` \
                  — дрейф от канона `mut v = it.collect()` (nv-coding-style §33, \
                  прецедент clippy manual_collect/needless_collect, \
                  [M-manual-collect-lint-missing])",
        ast: Some(conv_manual_collect),
        text: None,
    },
    ConvRule {
        id: "W_MANUAL_SLICE_TO_END",
        summary: "избыточные границы диапазона среза `recv[a..recv.len()]` / \
                  `recv[0..b]` / `recv[0..recv.len()]` — канон открытые диапазоны \
                  `recv[a..]` / `recv[..b]` / `recv[..]` (nv-coding-style §34, \
                  прецедент clippy redundant-slicing, \
                  [M-manual-slice-bounds-lint-missing])",
        ast: Some(conv_manual_slice_to_end),
        text: None,
    },
];

/// id всех правил реестра (для валидации `--rule` и `--list-rules`).
pub fn conv_rule_ids() -> Vec<&'static str> {
    CONV_RULES.iter().map(|r| r.id).collect()
}

/// Прогон реестра. `module` — None если файл не распарсился (текст-правила
/// всё равно работают). `enabled` — None = все правила, Some = только выбранные.
pub fn run_conv_rules(
    module: Option<&Module>,
    src: &str,
    opts: &ConvLintOptions,
    enabled: Option<&HashSet<String>>,
) -> Vec<LintWarning> {
    let mut out = Vec::new();
    for rule in CONV_RULES {
        if let Some(set) = enabled {
            if !set.contains(rule.id) {
                continue;
            }
        }
        if let (Some(hook), Some(m)) = (rule.ast, module) {
            hook(m, opts, &mut out);
        }
        if let Some(hook) = rule.text {
            hook(src, opts, &mut out);
        }
    }
    // Универсальная суппрессия «остаток под маркером»: находка, чья строка
    // исходника несёт `[M-...]`-маркер (ссылку на идущую работу / backlog),
    // не выводится — цель приёмки «0 находок или все остатки под маркерами».
    out.retain(|w| !conv_span_line_has_marker(src, w.diag.span.start));
    // Plan 185 Ф.N (owner decision 2026-07-17): именованное inline-подавление
    // `// nova:allow W_CODE -- причина` — единственный легальный люк под
    // будущим `--deny` (в отличие от `[M-...]`-маркера выше, который молчит
    // «пока не готово»; `nova:allow` — «читал, оставляю НАМЕРЕННО», causa
    // обязательна и грепаема).
    apply_nova_allow_suppressions(src, &mut out);
    out
}

/// `true` если строка исходника, содержащая byte-offset, несёт `[M-`-маркер.
fn conv_span_line_has_marker(src: &str, offset: usize) -> bool {
    if offset >= src.len() {
        return false;
    }
    let line_start = src[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = src[offset..].find('\n').map(|i| offset + i).unwrap_or(src.len());
    src[line_start..line_end].contains("[M-")
}

// ---------------------------------------------------------------------------
// `nova:allow` — inline-подавление находок (Plan 185 Ф.N, owner decision
// 2026-07-17). Механизм диагностики, не языковая фича — работает по сырому
// тексту исходника (как `conv_span_line_has_marker` выше), НЕ по AST-атрибуту.
//
// Синтаксис (СТРОГО, дословно):
//
//     // nova:allow W_CODE -- причина
//     <декларация/сайт находки — на СЛЕДУЮЩЕЙ строке>
//
// - Комментарий — на СВОЕЙ строке, НЕПОСРЕДСТВЕННО перед строкой находки
//   (line - 1). Гасит РОВНО перечисленные rule id, РОВНО на этой строке —
//   не файл, не блок.
// - Несколько кодов через запятую: `// nova:allow W_A, W_B -- причина`.
// - Причина ОБЯЗАТЕЛЬНА (непустой текст после `--`, trim). Пустая/
//   отсутствующая причина НЕ подавляет находку И сама становится находкой
//   `E_LINT_ALLOW_NO_REASON` (не суппрессируется ничем — иначе `nova:allow`
//   без причины был бы тихой дырой под `--deny`).
// ---------------------------------------------------------------------------

/// Один разобранный `// nova:allow` комментарий.
struct NovaAllowEntry {
    /// Строка комментария, 1-based.
    line: usize,
    rule_ids: HashSet<String>,
    has_reason: bool,
}

/// Разобрать все `// nova:allow ...` строки исходника. Строки, где после
/// `nova:allow` нет ни одного rule-id (случайное текстовое совпадение),
/// молча игнорируются — не создают ни находку, ни суппрессию.
fn parse_nova_allow_comments(src: &str) -> Vec<NovaAllowEntry> {
    let mut out = Vec::new();
    for (idx, line) in src.lines().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("//") {
            continue;
        }
        let after_slashes = trimmed.trim_start_matches('/').trim_start();
        let Some(rest) = after_slashes.strip_prefix("nova:allow") else { continue };
        let rest = rest.trim_start();
        let (ids_part, reason_part): (&str, Option<&str>) = match rest.find("--") {
            Some(p) => (rest[..p].trim(), Some(rest[p + 2..].trim())),
            None => (rest.trim(), None),
        };
        let rule_ids: HashSet<String> = ids_part
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if rule_ids.is_empty() {
            continue;
        }
        let has_reason = reason_part.is_some_and(|r| !r.is_empty());
        out.push(NovaAllowEntry { line: idx + 1, rule_ids, has_reason });
    }
    out
}

/// Byte-offset начала строки `line` (1-based).
fn conv_line_start_offset(src: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    src.match_indices('\n').nth(line - 2).map(|(i, _)| i + 1).unwrap_or(0)
}

/// Применяет `nova:allow`-суппрессию к `out` (in-place) + добавляет
/// `E_LINT_ALLOW_NO_REASON` для комментариев без причины.
fn apply_nova_allow_suppressions(src: &str, out: &mut Vec<LintWarning>) {
    let allows = parse_nova_allow_comments(src);
    if allows.is_empty() {
        return;
    }
    for a in &allows {
        if !a.has_reason {
            let offset = conv_line_start_offset(src, a.line);
            let mut ids: Vec<&String> = a.rule_ids.iter().collect();
            ids.sort();
            let ids_disp = ids.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
            out.push(LintWarning {
                rule: "E_LINT_ALLOW_NO_REASON",
                diag: Diagnostic::new(
                    format!(
                        "`nova:allow {}` без причины: канон — `// nova:allow {} -- \
                         причина` (причина обязательна — единственный легальный \
                         люк под `--deny`, грепаемый).",
                        ids_disp, ids_disp
                    ),
                    Span::new(offset, offset),
                ),
            });
        }
    }
    out.retain(|w| {
        if w.rule == "E_LINT_ALLOW_NO_REASON" {
            return true;
        }
        let (line, _) = byte_to_line_col(src, w.diag.span.start);
        !allows.iter().any(|a| {
            a.has_reason && a.line + 1 == line && a.rule_ids.contains(w.rule)
        })
    });
}

// ---------------------------------------------------------------------------
// Общие helpers реестра.
// ---------------------------------------------------------------------------

/// Все FnDecl модуля (items + peer_files).
fn conv_all_fns(m: &Module) -> Vec<&FnDecl> {
    let mut out = Vec::new();
    for item in &m.items {
        if let Item::Fn(f) = item {
            out.push(f);
        }
    }
    for pf in &m.peer_files {
        for item in &pf.items_here {
            if let Item::Fn(f) = item {
                out.push(f);
            }
        }
    }
    out
}

/// `test "name" { … }` block bodies (`Item::Test`) — SIBLING of
/// `conv_all_fns`, deliberately SEPARATE from it (owner 2026-07-21 lint
/// sweep finding): `conv_all_fns` only sees `Item::Fn`, so EVERY existing
/// `CONV_RULES` entry that walks via `conv_all_fns(m)` is BLIND to code
/// living inside `test { }` blocks — a real, pre-existing gap across the
/// WHOLE registry (most `*_test.nv` module content is exactly `test { }`
/// blocks, not `fn`s; confirmed empirically: `nova lint` over `std`/
/// `examples` found zero `W_REDUNDANT_BYTES_ON_LITERAL` hits in
/// `std/src/net/tcp_test.nv`'s `conn.write("hello net2".bytes())` call-arg
/// sites despite the shape matching). Widening the SHARED `conv_all_fns`
/// itself is out of scope here (touches all 25 pre-existing rules at once,
/// its own separate validation pass) — this wave's 4 NEW rules use this
/// sibling helper explicitly so at least the new rules' own advertised
/// scope ("std/examples") is not silently narrowed by the gap; flagged in
/// the wave's report as a legitimate follow-up for the other 25.
fn conv_all_test_bodies(m: &Module) -> Vec<&Block> {
    let mut out = Vec::new();
    for item in &m.items {
        if let Item::Test(t) = item {
            out.push(&t.body);
        }
    }
    for pf in &m.peer_files {
        for item in &pf.items_here {
            if let Item::Test(t) = item {
                out.push(&t.body);
            }
        }
    }
    out
}

/// Имена record-полей типов, ОБЪЯВЛЕННЫХ в этом модуле: тип → поля.
fn conv_module_record_fields(m: &Module) -> std::collections::HashMap<String, Vec<String>> {
    let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut collect = |items: &[Item], out: &mut std::collections::HashMap<String, Vec<String>>| {
        for item in items {
            if let Item::Type(td) = item {
                if let TypeDeclKind::Record(fields) = &td.kind {
                    out.insert(
                        td.name.clone(),
                        fields.iter().map(|f| f.name.clone()).collect(),
                    );
                }
            }
        }
    };
    collect(&m.items, &mut out);
    for pf in &m.peer_files {
        collect(&pf.items_here, &mut out);
    }
    out
}

/// `TypeRef` — голый `int`?
fn conv_ty_is_int(tr: &TypeRef) -> bool {
    matches!(tr, TypeRef::Named { path, generics, .. }
        if generics.is_empty() && path.len() == 1 && path[0] == "int")
}

/// `TypeRef` — «сырой хендл»: `int`, `ptr` или `*()`?
fn conv_ty_is_bare_handle(tr: &TypeRef) -> bool {
    match tr {
        TypeRef::Named { path, generics, .. } if generics.is_empty() && path.len() == 1 => {
            path[0] == "int" || path[0] == "ptr"
        }
        TypeRef::Pointer(inner, _) => matches!(inner.as_ref(), TypeRef::Unit(_)),
        _ => false,
    }
}

/// Последний сегмент Named-типа (для сравнения с receiver-типом).
fn conv_ty_last_name(tr: &TypeRef) -> Option<&str> {
    if let TypeRef::Named { path, .. } = tr {
        path.last().map(String::as_str)
    } else {
        None
    }
}

/// Каноническая строка-идентичность «простого места» (lvalue без побочных
/// эффектов у receiver'а): голый `ident`, `@` (self), либо цепочка полей
/// `obj.field`/`@field`/`@a.b` НАД такой же базой. `None` для всего
/// остального (Index, Call, произвольные выражения) — используется В ОБОИХ
/// направлениях: как «легальная LHS-форма» (гейт на побочные эффекты) И как
/// синтаксическое сравнение LHS/RHS-операнда (двух мест на строковое
/// равенство, W_NON_COMPOUND_ASSIGN / W_WHILE_COUNTER_FOR_RANGE).
fn conv_place_key(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(n) => Some(n.clone()),
        ExprKind::SelfAccess => Some("@".to_string()),
        ExprKind::Member { obj, name } => {
            let base = conv_place_key(obj)?;
            // `@field` (self) — БЕЗ разделительной точки (Nova-синтаксис
            // самого поля, `@x`, не `@.x`); дальше вглубь (`@a.b`) — точка.
            if base == "@" {
                Some(format!("@{name}"))
            } else {
                Some(format!("{base}.{name}"))
            }
        }
        _ => None,
    }
}

/// Обход всех expr/stmt тела функции с флагом «внутри цикла».
/// `on_stmt` / `on_expr` вызываются pre-order для каждого узла.
fn conv_walk_fn(
    f: &FnDecl,
    on_stmt: &mut dyn FnMut(&Stmt, bool),
    on_expr: &mut dyn FnMut(&Expr, bool),
) {
    match &f.body {
        FnBody::Expr(e) => conv_walk_expr(e, false, on_stmt, on_expr),
        FnBody::Block(b) => conv_walk_block(b, false, on_stmt, on_expr),
        FnBody::External => {}
    }
}

fn conv_walk_block(
    b: &Block,
    in_loop: bool,
    on_stmt: &mut dyn FnMut(&Stmt, bool),
    on_expr: &mut dyn FnMut(&Expr, bool),
) {
    for s in &b.stmts {
        conv_walk_stmt(s, in_loop, on_stmt, on_expr);
    }
    if let Some(t) = &b.trailing {
        conv_walk_expr(t, in_loop, on_stmt, on_expr);
    }
}

fn conv_walk_stmt(
    s: &Stmt,
    in_loop: bool,
    on_stmt: &mut dyn FnMut(&Stmt, bool),
    on_expr: &mut dyn FnMut(&Expr, bool),
) {
    on_stmt(s, in_loop);
    match s {
        Stmt::Let(d) => conv_walk_expr(&d.value, in_loop, on_stmt, on_expr),
        Stmt::Const(d) => conv_walk_expr(&d.value, in_loop, on_stmt, on_expr),
        Stmt::Expr(e) => conv_walk_expr(e, in_loop, on_stmt, on_expr),
        Stmt::Assign { target, value, .. } => {
            conv_walk_expr(target, in_loop, on_stmt, on_expr);
            conv_walk_expr(value, in_loop, on_stmt, on_expr);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                conv_walk_expr(e, in_loop, on_stmt, on_expr);
            }
            for e in rhs {
                conv_walk_expr(e, in_loop, on_stmt, on_expr);
            }
        }
        Stmt::Return { value: Some(v), .. } => conv_walk_expr(v, in_loop, on_stmt, on_expr),
        Stmt::Throw { value, .. } => conv_walk_expr(value, in_loop, on_stmt, on_expr),
        Stmt::Defer { body, .. } => conv_walk_expr(body, in_loop, on_stmt, on_expr),
        Stmt::ConsumeScope { init, body, .. } => {
            conv_walk_expr(init, in_loop, on_stmt, on_expr);
            conv_walk_block(body, in_loop, on_stmt, on_expr);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            conv_walk_expr(expr, in_loop, on_stmt, on_expr);
        }
        Stmt::Apply { args, .. } => {
            for a in args {
                conv_walk_expr(a, in_loop, on_stmt, on_expr);
            }
        }
        Stmt::Calc { steps, .. } => {
            for step in steps {
                conv_walk_expr(&step.expr, in_loop, on_stmt, on_expr);
            }
        }
        _ => {}
    }
}

fn conv_walk_expr(
    e: &Expr,
    in_loop: bool,
    on_stmt: &mut dyn FnMut(&Stmt, bool),
    on_expr: &mut dyn FnMut(&Expr, bool),
) {
    on_expr(e, in_loop);
    match &e.kind {
        ExprKind::Unary { operand, .. } => conv_walk_expr(operand, in_loop, on_stmt, on_expr),
        ExprKind::Binary { left, right, .. } => {
            conv_walk_expr(left, in_loop, on_stmt, on_expr);
            conv_walk_expr(right, in_loop, on_stmt, on_expr);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => conv_walk_expr(x, in_loop, on_stmt, on_expr),
        ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => {
            conv_walk_expr(x, in_loop, on_stmt, on_expr)
        }
        ExprKind::Coalesce(a, b) => {
            conv_walk_expr(a, in_loop, on_stmt, on_expr);
            conv_walk_expr(b, in_loop, on_stmt, on_expr);
        }
        ExprKind::Member { obj, .. } => conv_walk_expr(obj, in_loop, on_stmt, on_expr),
        ExprKind::Index { obj, index } => {
            conv_walk_expr(obj, in_loop, on_stmt, on_expr);
            conv_walk_expr(index, in_loop, on_stmt, on_expr);
        }
        ExprKind::TurboFish { base, .. } => conv_walk_expr(base, in_loop, on_stmt, on_expr),
        ExprKind::Call { func, args, trailing } => {
            conv_walk_expr(func, in_loop, on_stmt, on_expr);
            for a in args {
                conv_walk_expr(a.expr(), in_loop, on_stmt, on_expr);
            }
            if let Some(t) = trailing {
                match t {
                    crate::ast::Trailing::Block(b) => {
                        conv_walk_block(b, in_loop, on_stmt, on_expr)
                    }
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                        conv_walk_block(&tb.body, in_loop, on_stmt, on_expr)
                    }
                    crate::ast::Trailing::Fn(sb) => match &sb.body {
                        FnBody::Expr(e) => conv_walk_expr(e, in_loop, on_stmt, on_expr),
                        FnBody::Block(b) => conv_walk_block(b, in_loop, on_stmt, on_expr),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::If { cond, then, else_ } => {
            conv_walk_expr(cond, in_loop, on_stmt, on_expr);
            conv_walk_block(then, in_loop, on_stmt, on_expr);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => conv_walk_block(b, in_loop, on_stmt, on_expr),
                    ElseBranch::If(ie) => conv_walk_expr(ie, in_loop, on_stmt, on_expr),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            conv_walk_expr(scrutinee, in_loop, on_stmt, on_expr);
            conv_walk_block(then, in_loop, on_stmt, on_expr);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => conv_walk_block(b, in_loop, on_stmt, on_expr),
                    ElseBranch::If(ie) => conv_walk_expr(ie, in_loop, on_stmt, on_expr),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            conv_walk_expr(scrutinee, in_loop, on_stmt, on_expr);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    conv_walk_expr(g, in_loop, on_stmt, on_expr);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => conv_walk_expr(e, in_loop, on_stmt, on_expr),
                    MatchArmBody::Block(b) => conv_walk_block(b, in_loop, on_stmt, on_expr),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            conv_walk_expr(iter, in_loop, on_stmt, on_expr);
            conv_walk_block(body, true, on_stmt, on_expr);
        }
        ExprKind::While { cond, body, .. } => {
            conv_walk_expr(cond, in_loop, on_stmt, on_expr);
            conv_walk_block(body, true, on_stmt, on_expr);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            conv_walk_expr(scrutinee, in_loop, on_stmt, on_expr);
            if let Some(g) = guard {
                conv_walk_expr(g, in_loop, on_stmt, on_expr);
            }
            conv_walk_block(body, true, on_stmt, on_expr);
        }
        ExprKind::Loop { body, .. } => conv_walk_block(body, true, on_stmt, on_expr),
        ExprKind::Block(b) => conv_walk_block(b, in_loop, on_stmt, on_expr),
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => {
                        conv_walk_expr(x, in_loop, on_stmt, on_expr)
                    }
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for (k, v) in crate::ast::MapElem::cloned_pairs(elems).iter() {
                conv_walk_expr(k, in_loop, on_stmt, on_expr);
                conv_walk_expr(v, in_loop, on_stmt, on_expr);
            }
        }
        ExprKind::TupleLit(items) => {
            for x in items {
                conv_walk_expr(x, in_loop, on_stmt, on_expr);
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    conv_walk_expr(v, in_loop, on_stmt, on_expr);
                }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let crate::ast::InterpStrPart::Expr { expr, .. } = p {
                    conv_walk_expr(expr, in_loop, on_stmt, on_expr);
                }
            }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            conv_walk_expr(tag, in_loop, on_stmt, on_expr);
            for a in args {
                conv_walk_expr(a, in_loop, on_stmt, on_expr);
            }
        }
        ExprKind::Lambda { body, .. } => conv_walk_expr(body, in_loop, on_stmt, on_expr),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e) => conv_walk_expr(e, in_loop, on_stmt, on_expr),
            ClosureBody::Block(b) => conv_walk_block(b, in_loop, on_stmt, on_expr),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Expr(e) => conv_walk_expr(e, in_loop, on_stmt, on_expr),
            FnBody::Block(b) => conv_walk_block(b, in_loop, on_stmt, on_expr),
            FnBody::External => {}
        },
        ExprKind::Spawn(x) | ExprKind::Throw(x) => conv_walk_expr(x, in_loop, on_stmt, on_expr),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => {
            conv_walk_block(b, in_loop, on_stmt, on_expr)
        }
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                conv_walk_expr(c, in_loop, on_stmt, on_expr);
            }
            if let Some(dl) = deadline {
                conv_walk_expr(&dl.expr, in_loop, on_stmt, on_expr);
            }
            conv_walk_block(body, in_loop, on_stmt, on_expr);
        }
        ExprKind::With { bindings: _, body } => conv_walk_block(body, in_loop, on_stmt, on_expr),
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            conv_walk_block(body, in_loop, on_stmt, on_expr)
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &arm.guard {
                    conv_walk_expr(g, in_loop, on_stmt, on_expr);
                }
                conv_walk_block(&arm.body, in_loop, on_stmt, on_expr);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start {
                conv_walk_expr(s, in_loop, on_stmt, on_expr);
            }
            if let Some(x) = end {
                conv_walk_expr(x, in_loop, on_stmt, on_expr);
            }
        }
        ExprKind::Interrupt(Some(x)) => conv_walk_expr(x, in_loop, on_stmt, on_expr),
        _ => {}
    }
}

/// Итерация по строкам исходника: `(byte_offset_строки, вся_строка, код_без_комментария)`.
fn conv_each_code_line(src: &str, mut cb: impl FnMut(usize, &str, &str)) {
    let mut off = 0usize;
    for line in src.split_inclusive('\n') {
        let trimmed_line = line.trim_end_matches(['\n', '\r']);
        let code = match trimmed_line.find("//") {
            Some(i) => &trimmed_line[..i],
            None => trimmed_line,
        };
        cb(off, trimmed_line, code);
        off += line.len();
    }
}

// ---------------------------------------------------------------------------
// W_NONVARIADIC_OF — `.of` у невариадика (§21б nv-coding-style).
// ---------------------------------------------------------------------------

fn conv_nonvariadic_of(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        let Some(recv) = &f.receiver else { continue };
        if recv.kind != ReceiverKind::Static || f.name != "of" {
            continue;
        }
        if f.params.iter().any(|p| p.is_variadic) {
            continue;
        }
        out.push(LintWarning {
            rule: "W_NONVARIADIC_OF",
            diag: Diagnostic::new(
                format!(
                    "static `{}.of(...)` без вариадик-параметра: имя `of` \
                     зарезервировано за вариадик-коллекциями (`Vec[T].of(a, b, c)`). \
                     Тривиальная установка полей — `{}.new(...)` с дефолт-параметрами \
                     (nv-coding-style §21б).",
                    recv.type_name, recv.type_name
                ),
                f.span,
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// W_RETIRED_PREFIX — `as_`-префикс ретрактирован (D410).
// ---------------------------------------------------------------------------

fn conv_retired_prefix(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        if let Some(rest) = f.name.strip_prefix("as_") {
            if rest.is_empty() {
                continue;
            }
            out.push(LintWarning {
                rule: "W_RETIRED_PREFIX",
                diag: Diagnostic::new(
                    format!(
                        "`{}`: префикс `as_` ретрактирован (D410). Вид/линза = голое \
                         существительное (`bytes()`, `chars()`, `slice()`); копия — \
                         явный `.clone()` на месте вызова; трансформация — `to_*`.",
                        f.name
                    ),
                    f.span,
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// W_ACCESSOR_PAIR — пары `get_x`/`set_x` (D117 AMEND).
// ---------------------------------------------------------------------------

fn conv_accessor_pair(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    use std::collections::HashMap as Map;
    // (receiver-тип или "" для free fn, суффикс) → span get_-декларации.
    let mut getters: Map<(String, String), Span> = Map::new();
    let mut setters: std::collections::HashSet<(String, String)> = HashSet::new();
    for f in conv_all_fns(m) {
        let recv_key = f
            .receiver
            .as_ref()
            .map(|r| r.type_name.clone())
            .unwrap_or_default();
        if let Some(rest) = f.name.strip_prefix("get_") {
            if !rest.is_empty() {
                getters.insert((recv_key.clone(), rest.to_string()), f.span);
            }
        } else if let Some(rest) = f.name.strip_prefix("set_") {
            if !rest.is_empty() {
                setters.insert((recv_key, rest.to_string()));
            }
        }
    }
    let mut hits: Vec<(&(String, String), &Span)> = getters
        .iter()
        .filter(|(k, _)| setters.contains(k))
        .collect();
    hits.sort_by_key(|(_, s)| s.start);
    for ((recv, prop), span) in hits {
        let recv_disp = if recv.is_empty() { "<free fn>" } else { recv.as_str() };
        out.push(LintWarning {
            rule: "W_ACCESSOR_PAIR",
            diag: Diagnostic::new(
                format!(
                    "пара `get_{}`/`set_{}` на `{}`: канон — методы-свойства одним \
                     именем по арности: чтение `@{}()`, запись `mut @{}(v) -> @` \
                     (D117 AMEND, nv-coding-style «методы-свойства»).",
                    prop, prop, recv_disp, prop, prop
                ),
                *span,
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// W_WITH_MUTATOR — мутирующий `with_*` (nv-coding-style §21).
// ---------------------------------------------------------------------------

fn conv_with_mutator(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        let Some(recv) = &f.receiver else { continue };
        if recv.mutable && f.name.starts_with("with_") && f.name.len() > "with_".len() {
            // Owner decision (2026-07-17): `with_*` has TWO canonical meanings,
            // distinguished by SIGNATURE (nv-coding-style, `with_`-section):
            //   - `with_x(value)`  → returns a NEW copy (field-copy withXXX) —
            //     mut-receiver here is the bug this rule catches.
            //   - `with_x(closure)` → run the closure UNDER the resource
            //     (scope-guard / RAII: lock, run `body`, unlock — precedent:
            //     Kotlin `withLock { ... }`) — mut-receiver is REQUIRED here
            //     (lock/unlock IS the mutation), and the return value is the
            //     closure's result `R`, not "a new Self". A fn-typed parameter
            //     is the syntactic tell: skip.
            if f.params.iter().any(|p| conv_type_is_closure(&p.ty)) {
                continue;
            }
            out.push(LintWarning {
                rule: "W_WITH_MUTATOR",
                diag: Diagnostic::new(
                    format!(
                        "`{}` объявлен с mut-приёмником: `with_*` НИКОГДА не мутирует \
                         — всегда возвращает новое значение. Мутирующее беглое \
                         свойство = `mut @{}(v) -> @` (nv-coding-style §21, D117 AMEND).",
                        f.name,
                        f.name.trim_start_matches("with_")
                    ),
                    f.span,
                ),
            });
        }
    }
}

/// `true` если `ty` — fn-тип (замыкание/fn-pointer), под любым числом
/// «прозрачных» type-wrapper'ов (`*T`/`ro T`/`mut T`/`uninit T`/`ref T`).
/// Используется [`conv_with_mutator`] чтобы отличить scope-guard `with_*`
/// (принимает closure) от field-copy `with_*` (принимает значение).
fn conv_type_is_closure(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::Func { .. } => true,
        TypeRef::Pointer(inner, _)
        | TypeRef::Readonly(inner, _)
        | TypeRef::Mut(inner, _)
        | TypeRef::Uninit(inner, _)
        | TypeRef::Ref(inner, _) => conv_type_is_closure(inner),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// W_STATIC_CONVERSION — `T.from(x)` / `T.parse(s)` (§1а, ретракция 2026-07-09).
//
// SEMANTIC-UPGRADE: синтаксическая версия не различает «источник — значение»
// (запрещено, канон `x.to_*()`) от «источник — концепт» (`from_polar` легален,
// но он и не называется голым `from`). Голое `from`/`parse` с 1+ аргументом
// флагуется всегда; спорные точки — комментарий-маркер на месте.
// ---------------------------------------------------------------------------

/// CamelCase → snake_case по границам слов (nv-coding-style §1а, 2026-07-30:
/// «имя типа внутри `to_*` — snake_case по границам CamelCase»). Используется
/// только для диагностики-подсказки в [`conv_static_conversion`] — не
/// нормативный движок именования (лексикализованные исключения `datetime`/
/// `bigint`/`bigdecimal`/`bigfloat` тут не разворачиваются, это на совести
/// автора миграции).
fn conv_camel_to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push('_');
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
    out
}

fn conv_static_conversion(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        let Some(recv) = &f.receiver else { continue };
        if recv.kind != ReceiverKind::Static {
            continue;
        }
        if (f.name == "from" || f.name == "parse" || f.name == "from_str") && !f.params.is_empty() {
            out.push(LintWarning {
                rule: "W_STATIC_CONVERSION",
                diag: Diagnostic::new(
                    format!(
                        "статик-конверсия `{}.{}(...)` — запрещённая «пятая дверь» \
                         (nv-coding-style §1а, ретракция 2026-07-09): дубль `to_*`, \
                         ломает цепочки. Канон: метод на источнике `x.to_{}()` \
                         (→ Result где fallible). `from` уместен только для \
                         концепт-источника под содержательным именем (`from_polar`).",
                        recv.type_name,
                        f.name,
                        conv_camel_to_snake(&recv.type_name)
                    ),
                    f.span,
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// W_CONSUME_NAKED_NAME — `consume`-receiver + голое имя-вид, конвертирующее
// в ДРУГОЙ тип (§1а, ось владения). Зеркало W_STATIC_CONVERSION: там —
// запрещённая статик-дверь `T.from`/`T.parse`, здесь — запрещённая
// instance-дверь «голое имя потребляет receiver и возвращает другой тип»,
// когда канон требует `into_*` (§1а: «голое существительное = O(1) вид...
// НИКОГДА не потребляет receiver»; «`into_*` = потребляющий финализатор»).
//
// Исключения (все три — НЕ нарушение, отдельные оси §1а/D117):
//   - имя уже `into_*` (канон) / `to_*` (ro-источник, отдельный класс) /
//     `with_*` (D117 wither — не финализатор);
//   - возврат ТОГО ЖЕ типа, что и receiver (`Self`/`RecvType`, включая
//     обёрнутый в `Result[RecvType,...]`/`Option[RecvType]`) — это
//     трансформация/builder-шаг, не конверсия в другой тип; `-> @` на
//     `consume`-receiver'е синтаксически запрещён (E_CONSUME_RECEIVER_
//     RETURNS_AT, D8/D133), но проверяем `returns_receiver` защитно;
//   - Unit-подобный возврат (`-> ()`, отсутствие `->` вовсе, или
//     `Result[(), E]`/`Option[()]`) — НЕ конверсия в значение вообще:
//     `close`/`cleanup`/`commit`/`abort`/`release`/`unlock`/`discard`
//     (RAII-финализаторы D432 `consume @cleanup`, `MutexGuard consume
//     @unlock`, `File consume @close() -> Result[(), IoError]`) —
//     императивные глаголы без произведённого значения, другая
//     именная ось (не «вид, притворяющийся видом»); без этого исключения
//     правило захлёбывается std-практикой (`std/net/tcp.nv`,
//     `std/runtime/sync.nv`).
// ---------------------------------------------------------------------------

/// Payload-тип у `Result[T, E]`/`Option[T]` (первый generic), либо сам тип,
/// если это не Result/Option. Однослойная развёртка — этого достаточно для
/// различения «конверсия в значение» vs «финализатор без значения».
fn conv_unwrap_result_option(ty: &TypeRef) -> &TypeRef {
    if let TypeRef::Named { path, generics, .. } = ty {
        let last = path.last().map(String::as_str);
        if (last == Some("Result") || last == Some("Option")) && !generics.is_empty() {
            return &generics[0];
        }
    }
    ty
}

/// `None` (нет `->` вовсе — implicit `()`), явный `TypeRef::Unit`, либо
/// `Result[(), E]`/`Option[()]` — «финализатор без произведённого значения».
fn conv_type_is_unit_like(ty: Option<&TypeRef>) -> bool {
    match ty {
        None => true,
        Some(t) => matches!(conv_unwrap_result_option(t), TypeRef::Unit(_)),
    }
}

/// Payload (после снятия Result/Option) — ТОТ ЖЕ тип, что и receiver
/// (`Self`, либо `RecvType` по имени)?
fn conv_type_same_as_receiver(ty: &TypeRef, recv_name: &str) -> bool {
    match conv_unwrap_result_option(ty) {
        TypeRef::Named { path, .. } => match path.last().map(String::as_str) {
            Some("Self") => true,
            Some(n) => n == recv_name,
            None => false,
        },
        _ => false,
    }
}

fn conv_consume_naked_name(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        let Some(recv) = &f.receiver else { continue };
        if recv.kind != ReceiverKind::Instance || !recv.consume {
            continue;
        }
        if f.name.starts_with("into_") || f.name.starts_with("with_") || f.name.starts_with("to_")
        {
            continue;
        }
        // `-> @` (returns_receiver) mutually excludes `consume` at parse
        // time (E_CONSUME_RECEIVER_RETURNS_AT) — defensive, not reachable.
        if f.returns_receiver {
            continue;
        }
        if conv_type_is_unit_like(f.return_type.as_ref()) {
            continue;
        }
        let rt = f.return_type.as_ref().expect("checked non-unit-like above");
        if conv_type_same_as_receiver(rt, &recv.type_name) {
            continue;
        }
        out.push(LintWarning {
            rule: "W_CONSUME_NAKED_NAME",
            diag: Diagnostic::new(
                format!(
                    "`consume`-конверсия `{}.{}(...)` в другой тип обязана называться \
                     `@into_{}()` (nv-coding-style §1а, ось владения): голое имя \
                     зарезервировано за zero-copy видом, который НЕ потребляет receiver.",
                    recv.type_name, f.name, f.name
                ),
                f.span,
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// W_TRY_WITHOUT_SIBLING — `try_X` без инфаллибельного сиблинга `X` (R3 D325).
//
// Сиблинг ищется в ЭТОМ модуле (items + peers): метод — на том же
// receiver-типе, free fn — среди free fns. Чтобы не ловить false-positive
// на методах типов, чьи перегрузки живут в другом файле folder-модуля
// (per-file прогон `nova lint` не видит peers), правило для методов
// срабатывает только когда receiver-тип ОБЪЯВЛЕН в этом же модуле.
// ---------------------------------------------------------------------------

fn conv_try_without_sibling(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    let fns = conv_all_fns(m);
    // Все объявленные тип-имена — для receiver-гейта.
    let mut declared_names: HashSet<String> = HashSet::new();
    fn collect_types(items: &[Item], set: &mut HashSet<String>) {
        for it in items {
            if let Item::Type(td) = it {
                set.insert(td.name.clone());
            }
        }
    }
    collect_types(&m.items, &mut declared_names);
    for pf in &m.peer_files {
        collect_types(&pf.items_here, &mut declared_names);
    }
    // (receiver-тип или "", имя) всех fn.
    let names: HashSet<(String, &str)> = fns
        .iter()
        .map(|f| {
            (
                f.receiver.as_ref().map(|r| r.type_name.clone()).unwrap_or_default(),
                f.name.as_str(),
            )
        })
        .collect();
    for f in &fns {
        // Приватные try_-хелперы — плумбинг, R3 нормирует публичный API.
        if !f.is_export {
            continue;
        }
        let Some(rest) = f.name.strip_prefix("try_") else { continue };
        if rest.is_empty() {
            continue;
        }
        let recv_key = f
            .receiver
            .as_ref()
            .map(|r| r.type_name.clone())
            .unwrap_or_default();
        // Метод на типе, объявленном не здесь → сиблинг может жить в
        // другом peer-файле, который per-file прогон не видит. Молчим.
        if let Some(r) = &f.receiver {
            if !declared_names.contains(r.type_name.as_str()) {
                continue;
            }
        }
        if names.contains(&(recv_key.clone(), rest)) {
            continue; // сиблинг есть — пара легальна (from/try_from).
        }
        out.push(LintWarning {
            rule: "W_TRY_WITHOUT_SIBLING",
            diag: Diagnostic::new(
                format!(
                    "`{}` без инфаллибельного сиблинга `{}`: префикс `try_` — ТОЛЬКО \
                     чтобы отличить fallible-вариант одноимённого infallible \
                     (`from`/`try_from`, D77). Одиночная fallible-операция — обычное \
                     имя + `Result` (R3 D325, nv-coding-style §1).",
                    f.name, rest
                ),
                f.span,
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// W_SETTER_NOT_FLUENT — 1-арный `mut @x(v)` сеттер не `-> @` (D117 AMEND-2).
//
// Триггеры (консервативно, оба синтаксические):
//   A. имя метода совпадает с именем поля receiver-типа, объявленного здесь;
//   B. тело — единственный оператор `@<имя> = v` (запись собственного поля).
// ---------------------------------------------------------------------------

fn conv_setter_not_fluent(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    let record_fields = conv_module_record_fields(m);
    for f in conv_all_fns(m) {
        let Some(recv) = &f.receiver else { continue };
        if recv.kind != ReceiverKind::Instance || !recv.mutable {
            continue;
        }
        if f.params.len() != 1 || f.returns_receiver {
            continue;
        }
        // Возврат должен быть unit (или отсутствовать) — иначе не «сеттер без @».
        let returns_unit = match &f.return_type {
            None => true,
            Some(TypeRef::Unit(_)) => true,
            Some(_) => false,
        };
        if !returns_unit {
            continue;
        }
        let trigger_a = record_fields
            .get(&recv.type_name)
            .map_or(false, |fields| fields.iter().any(|fname| fname == &f.name));
        let trigger_b = match &f.body {
            FnBody::Block(b) if b.stmts.len() == 1 && b.trailing.is_none() => {
                matches!(&b.stmts[0], Stmt::Assign { target, .. }
                    if matches!(&target.kind, ExprKind::Member { obj, name }
                        if name == &f.name && matches!(obj.kind, ExprKind::SelfAccess)))
            }
            _ => false,
        };
        if trigger_a || trigger_b {
            out.push(LintWarning {
                rule: "W_SETTER_NOT_FLUENT",
                diag: Diagnostic::new(
                    format!(
                        "сеттер `mut @{}(v)` возвращает `()`: `-> @` у метода \
                         установки свойства — умолчание, не опция (D117 AMEND-2). \
                         Возврат приёмника автоматический (D409) и даёт цепочки \
                         `r.{}(a).{}(b)`. `-> ()` — только с обоснованием на месте.",
                        f.name, f.name, f.name
                    ),
                    f.span,
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// W_FFI_BARE_HANDLE — голый int/*() хендл в extern-семействе (§4а
// module-conventions): конструктор `*_new`/`*_open`/... возвращает сырой
// `int`/`ptr`/`*()` И в модуле есть парный `*_free`/`*_close`/... — канон
// newtype `type CFooHandle(int)` прямо в extern-сигнатурах.
// ---------------------------------------------------------------------------

const CONV_HANDLE_CTOR_SUFFIXES: &[&str] = &["_new", "_open", "_create", "_init", "_alloc"];
const CONV_HANDLE_CLOSER_SUFFIXES: &[&str] =
    &["_free", "_close", "_destroy", "_del", "_dispose", "_release", "_shutdown"];

fn conv_ffi_bare_handle(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    let fns = conv_all_fns(m);
    let externs: Vec<&&FnDecl> = fns.iter().filter(|f| f.is_external).collect();
    if externs.is_empty() {
        return;
    }
    // Префиксы ресурсов, у которых есть closer с сырым хендл-параметром.
    let mut closer_prefixes: HashSet<&str> = HashSet::new();
    for f in &externs {
        for suf in CONV_HANDLE_CLOSER_SUFFIXES {
            if let Some(prefix) = f.name.strip_suffix(suf) {
                if !prefix.is_empty()
                    && f.params.first().map_or(false, |p| conv_ty_is_bare_handle(&p.ty))
                {
                    closer_prefixes.insert(prefix);
                }
            }
        }
    }
    if closer_prefixes.is_empty() {
        return;
    }
    for f in &externs {
        for suf in CONV_HANDLE_CTOR_SUFFIXES {
            let Some(prefix) = f.name.strip_suffix(suf) else { continue };
            if prefix.is_empty() || !closer_prefixes.contains(prefix) {
                continue;
            }
            let Some(rt) = &f.return_type else { continue };
            if conv_ty_is_bare_handle(rt) {
                out.push(LintWarning {
                    rule: "W_FFI_BARE_HANDLE",
                    diag: Diagnostic::new(
                        format!(
                            "extern `{}` возвращает голый хендл (`int`/`ptr`/`*()`) при \
                             парном `{}_<close/free>`: FFI-хендл никогда не ходит по \
                             Nova-коду голым — объявите newtype `type C{}Handle(int)` \
                             прямо в extern-сигнатурах (module-conventions §4а; эталон \
                             std/encoding/compress/ffi.nv). Легальное исключение — \
                             комментарий-маркер на месте.",
                            f.name, prefix, conv_camel(prefix)
                        ),
                        f.span,
                    ),
                });
                break;
            }
        }
    }
}

/// snake_case → CamelCase (для подсказки имени newtype).
fn conv_camel(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// W_MANUAL_SLICE_COPY — `push(x[i])` в счётном цикле (§18а nv-coding-style).
//
// Эвристика с низким FP: аргумент push — индекс-выражение `<простой>[<ident>]`
// внутри цикла. Срез-вид `x[a..b]` даёт то же за O(1) без аллокации; честная
// копия владения — `.clone()` на виде, не цикл.
// ---------------------------------------------------------------------------

fn conv_manual_slice_copy(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        conv_walk_fn(
            f,
            &mut |_s, _| {},
            &mut |e, in_loop| {
                if !in_loop {
                    return;
                }
                let ExprKind::Call { func, args, .. } = &e.kind else { return };
                let ExprKind::Member { name, .. } = &func.kind else { return };
                if name != "push" || args.len() != 1 {
                    return;
                }
                let arg = args[0].expr();
                let ExprKind::Index { obj, index } = &arg.kind else { return };
                // Простой контейнер (ident / @field / поле) + ident-индекс —
                // классический счётный копи-цикл.
                let simple_obj = matches!(
                    obj.kind,
                    ExprKind::Ident(_) | ExprKind::SelfAccess | ExprKind::Member { .. }
                );
                let simple_idx = matches!(index.kind, ExprKind::Ident(_));
                if simple_obj && simple_idx {
                    out.push(LintWarning {
                        rule: "W_MANUAL_SLICE_COPY",
                        diag: Diagnostic::new(
                            "поэлементная копия `push(x[i])` в счётном цикле — красный \
                             флаг (§18а nv-coding-style): `[]T`-вид среза (D262) даёт \
                             то же за O(1) без аллокации (`x[a..b]`). Нужно владение \
                             отдельным буфером — явный `.clone()` на виде, не цикл."
                                .to_string(),
                            e.span,
                        ),
                    });
                }
            },
        );
    }
}

// ---------------------------------------------------------------------------
// W_IMMUTABLE_REBUILD_SETTER — не-mut метод возвращает Self пересборкой
// полей (OpenOptions-класс, D117/D409). Для кучевых записей поверхностная
// копия делит потроха со старым объектом — канон `mut @x(v) -> @`.
//
// SEMANTIC-UPGRADE: без типов не отличаем value-record (где with_*-копия
// легальна — копия честная) от кучевого record при receiver-типе из другого
// модуля. Синтаксический гейт: receiver-тип объявлен в ЭТОМ модуле как
// heap-record (без маркера `value`) — только тогда пересборка флагуется.
// ---------------------------------------------------------------------------

fn conv_immutable_rebuild_setter(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    // Кучевые record-типы модуля (AllocKind::Heap).
    let mut heap_records: HashSet<String> = HashSet::new();
    fn collect_heap(items: &[Item], set: &mut HashSet<String>) {
        for it in items {
            if let Item::Type(td) = it {
                if matches!(td.kind, TypeDeclKind::Record(_))
                    && td.allocation == crate::ast::AllocKind::Heap
                {
                    set.insert(td.name.clone());
                }
            }
        }
    }
    collect_heap(&m.items, &mut heap_records);
    for pf in &m.peer_files {
        collect_heap(&pf.items_here, &mut heap_records);
    }
    if heap_records.is_empty() {
        return;
    }
    for f in conv_all_fns(m) {
        let Some(recv) = &f.receiver else { continue };
        if recv.kind != ReceiverKind::Instance || recv.mutable || recv.consume {
            continue;
        }
        if f.params.is_empty() || !heap_records.contains(recv.type_name.as_str()) {
            continue;
        }
        // Возврат — receiver-тип.
        let returns_self = f
            .return_type
            .as_ref()
            .and_then(conv_ty_last_name)
            .map_or(false, |n| n == recv.type_name);
        if !returns_self {
            continue;
        }
        // Тело — RecordLit (expr-body, единственный trailing или return).
        let rec_lit: Option<&Expr> = match &f.body {
            FnBody::Expr(e) => Some(e),
            FnBody::Block(b) => match (&b.stmts[..], &b.trailing) {
                ([], Some(t)) => Some(t),
                ([Stmt::Return { value: Some(v), .. }], None) => Some(v),
                _ => None,
            },
            FnBody::External => None,
        };
        let Some(lit) = rec_lit else { continue };
        let ExprKind::RecordLit { fields, .. } = &lit.kind else { continue };
        if fields.len() < 2 {
            continue;
        }
        // ≥1 поле копирует @field (пересборка) и ≥1 поле берёт параметр.
        let copies_self = fields.iter().any(|fl| {
            fl.value.as_ref().map_or(false, |v| {
                matches!(&v.kind, ExprKind::Member { obj, .. }
                    if matches!(obj.kind, ExprKind::SelfAccess))
                    || (fl.is_spread && matches!(v.kind, ExprKind::SelfAccess))
            })
        });
        let param_names: HashSet<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
        let uses_param = fields.iter().any(|fl| {
            fl.value.as_ref().map_or(false, |v| {
                matches!(&v.kind, ExprKind::Ident(n) if param_names.contains(n.as_str()))
            })
        });
        if copies_self && uses_param {
            out.push(LintWarning {
                rule: "W_IMMUTABLE_REBUILD_SETTER",
                diag: Diagnostic::new(
                    format!(
                        "`@{}` без `mut` возвращает `{}` пересборкой полей \
                         (OpenOptions-класс): для кучевой записи поверхностная копия \
                         делит потроха со старым объектом — независимости нет, только \
                         лишняя аллокация. Канон: мутирующее беглое свойство \
                         `mut @x(v) -> @` (D117 AMEND / D409, nv-coding-style §21).",
                        f.name, recv.type_name
                    ),
                    f.span,
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// W_STR_CONCAT_LOOP — `buf = buf + x` / `buf += "..."` в цикле
// (perf-conventions): O(N²), канон — StringBuilder.
//
// SEMANTIC-UPGRADE: без типов не знаем, str ли `buf` — синтаксический гейт:
// правая часть содержит строковый литерал / интерполяцию (то есть конкатенация
// точно строковая). `n += 1` и числовые аккумуляторы не флагуются.
// ---------------------------------------------------------------------------

/// `true`, если `e` синтаксически «строкоподобно» — литерал/интерполяция,
/// `+`-конкатенация с хотя бы одним строкоподобным операндом, `.to_str()`-
/// конверсия (владелец 2026-07-21: результат всегда str, независимо от
/// ресивера — расширение W_STR_CONCAT_LOOP на очень частый реальный сайт
/// `buf += x.to_str()`/`s = s + x.to_str()`, D-амендмент про string `+`
/// spec/decisions/02-types.md), либо `.concat(...)`-вызов на уже
/// распознанном строкоподобном ресивере (chain-propagation, та же
/// эвристика используется T3 `W_STR_CONCAT_METHOD` ниже).
/// File-scope (не только `conv_str_concat_loop`): используется ТАКЖЕ
/// `conv_non_compound_assign` для дедупа с этим правилом (один и тот же
/// сайт `buf = buf + "..."` в цикле не должен получить оба warning'а —
/// канон там StringBuilder, здесь — просто `+=`).
fn conv_is_stringish(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => true,
        ExprKind::Binary { op: crate::ast::BinOp::Add, left, right } => {
            conv_is_stringish(left) || conv_is_stringish(right)
        }
        ExprKind::Call { func, .. } => {
            if let ExprKind::Member { obj, name } = &func.kind {
                match name.as_str() {
                    "to_str" => true,
                    "concat" => conv_is_stringish(obj),
                    _ => false,
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

fn conv_str_concat_loop(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        conv_walk_fn(
            f,
            &mut |s, in_loop| {
                if !in_loop {
                    return;
                }
                let Stmt::Assign { target, op, value, span } = s else { return };
                let ExprKind::Ident(tname) = &target.kind else { return };
                let flagged = match op {
                    // `buf += "..."` / `buf += "${x}"`.
                    crate::ast::AssignOp::Add => conv_is_stringish(value),
                    // `buf = buf + x` где участвует строковый литерал/интерп.
                    crate::ast::AssignOp::Assign => {
                        matches!(&value.kind,
                            ExprKind::Binary { op: crate::ast::BinOp::Add, left, right }
                                if conv_is_stringish(left) || conv_is_stringish(right)
                                    || matches!(&left.kind, ExprKind::Ident(l) if l == tname)
                                        && conv_is_stringish(right))
                            && conv_is_stringish(value)
                    }
                    _ => false,
                };
                if flagged {
                    out.push(LintWarning {
                        rule: "W_STR_CONCAT_LOOP",
                        diag: Diagnostic::new(
                            format!(
                                "конкатенация `{} = {} + ...` в цикле — O(N²) \
                                 (perf-conventions): каждая итерация копирует весь \
                                 аккумулятор. Канон: `StringBuilder.new()` + \
                                 `.append(...)` в цикле + `.into_str()` после.",
                                tname, tname
                            ),
                            *span,
                        ),
                    });
                }
            },
            &mut |_e, _| {},
        );
    }
}

// ---------------------------------------------------------------------------
// W_STR_CONCAT_METHOD (владелец 2026-07-21) — `.concat(...)` вызов на
// str-выражении: канон — string-интерполяция (`"${a}${b}"`). Изначально
// планировалось как T1/T3 половина единого lint'а `string-concat-prefer-
// interp`; T1 (бинарный `+` со str-операндами) ретрактирован в HARD ERROR
// `E_STR_CONCAT_PLUS` (types/mod.rs, is_arith-блок walk_expr) — `+` для str
// больше не существует как оператор, это не warning-уровня находка. Здесь
// остаётся T3 — метод `@concat` (std/src/runtime/string/transform.nv) НЕ
// ретрактирован, остаётся явным API, но предпочтительнее интерполяции для
// читаемости при построении строк вне цикла (в цикле — см. W_STR_CONCAT_LOOP
// выше, тот же канон StringBuilder).
//
// SEMANTIC-UPGRADE: без типов (AST-only реестр) не знаем, str ли ресивер —
// синтаксический гейт через `conv_is_stringish` (str-литерал/интерполяция/
// `.to_str()`-конверсия/уже-распознанный `.concat(...)`-chain). НЕ
// отслеживает `let`-биндинги через statement-границы (как и W_STR_CONCAT_LOOP
// не отслеживает — та же консервативная конвенция этого реестра): `ro s =
// "x"; s.concat("y")` не распознаётся (документированная граница —
// false-negative, не false-positive). Тем же путём `Vec[T].concat(...)` /
// `[]u8.concat(...)` ЕСТЕСТВЕННО не триггерят: их ресивер не матчит
// str-эвристику — отдельного явного type-based исключения не требуется.
//
// Исключение: `std/src/runtime/string/**` (реализация примитива) —
// `o.in_str_runtime_impl` (вычисляется вызывающей стороной по пути файла,
// nova-cli::conv_lint_options_for, тот же паттерн что и `in_vec_module` для
// W_VEC_SPELLING).
// ---------------------------------------------------------------------------

fn conv_str_concat_method(m: &Module, o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    if o.in_str_runtime_impl {
        return;
    }
    for f in conv_all_fns(m) {
        conv_walk_fn(
            f,
            &mut |_s, _| {},
            &mut |e, _in_loop| {
                let ExprKind::Call { func, .. } = &e.kind else { return };
                let ExprKind::Member { obj, name } = &func.kind else { return };
                if name == "concat" && conv_is_stringish(obj) {
                    out.push(LintWarning {
                        rule: "W_STR_CONCAT_METHOD",
                        diag: Diagnostic::new(
                            "`.concat(...)` на str — используйте строковую \
                             интерполяцию `\"${a}${b}\"` вместо метода \
                             (perf-conventions); в цикле — см. W_STR_CONCAT_LOOP \
                             (StringBuilder)."
                                .to_string(),
                            e.span,
                        ),
                    });
                }
            },
        );
    }
}

// ---------------------------------------------------------------------------
// W_NON_COMPOUND_ASSIGN — `x = x OP e` там, где Nova поддерживает компаунд-
// форму `x OP= e` (nv-coding-style §29). Компаунд-операторы Nova — ТОЛЬКО
// `+=`/`-=`/`*=`/`/=` (`AssignOp` — варианты `Add`/`Sub`/`Mul`/`Div`; НЕТ
// `Mod`, НЕТ битовых — парсер лексирует лишь эти четыре compound-токена,
// `%=`/`&=`/`|=`/`^=`/`<<=`/`>>=` в языке не существуют, ср. emit_c.rs
// комментарий у `checked_helper` — «НЕТ `%=` в языке»). Правило флагует
// РОВНО эти четыре бинарных оператора; `x = x % e` и битовые — молчит (нет
// компаунда, предлагать нечего).
//
// LHS ограничен «простым местом» без побочных эффектов у receiver'а — голый
// `ident` / `@field` / цепочка полей поверх них (`conv_place_key`). Index-
// места (`x[i] = x[i] + e`) НАМЕРЕННО исключены: компаунд-присваивание на
// Index-таргете (`x[i] += e`) в кодогене идёт ДРУГИМ путём, чем `x[i] = v`
// (emit_c.rs Stmt::Assign — ветки bounds-checked Vec-write / struct-value
// memcpy-write / fixed-array-write гейтятся `if *op == AssignOp::Assign`
// буквально, `+=`/`-=`/… на Index падают в общий `emit_expr(target)`
// fallback, НЕ через эти проверенные ветки) — легальность/корректность
// компаунда по индексу для нескалярных элементов не подтверждена, поэтому
// НЕ флагуем (правило консервативно — «не уверен → молчи»).
//
// НЕ дублирует W_STR_CONCAT_LOOP: тот же сайт (в цикле, `+`, строкоподобный
// RHS) там уже флагуется — канон там StringBuilder, не `+=`; здесь молчит.
// ---------------------------------------------------------------------------

/// Компаунд-эквивалент бинарного оператора — `Some` ТОЛЬКО для четырёх
/// существующих в Nova компаунд-форм (`Add`/`Sub`/`Mul`/`Div`).
// SEMANTIC-UPGRADE (2026-07-18, regression found by this rule's own sweep —
// `std/src/concurrency/retry.nv`'s `d = d * multiplier` on a `Duration`
// value-record): `*=`/`/=` are DELIBERATELY excluded here even though
// `AssignOp` has `Mul`/`Div` variants. `emit_c.rs`'s compound-assign
// operator-overload dispatch (`is_overloaded_add_ty`, ~line 27650) is
// EXPLICITLY `Add`/`Sub`-only — "`Add`/`Sub` are overloadable operators;
// `*=`/`/=` are not" — so `x *= y` / `x /= y` on ANY type overloading `*`/`/`
// (e.g. `Duration * f64` scaling) falls through to a raw C `*=`/`/=` on a
// struct, a hard CC-FAIL. We have no type info here (AST-only) to tell a
// primitive `int`/`f64` LHS from an operator-overloaded record — so, unlike
// `Add`/`Sub` (which at least partially dispatch overloads for pointer-form
// value-records, `Nova_X*`), `Mul`/`Div` compound-assign is NEVER suggested.
fn conv_binop_to_compound(op: crate::ast::BinOp) -> Option<&'static str> {
    use crate::ast::BinOp;
    match op {
        BinOp::Add => Some("+="),
        BinOp::Sub => Some("-="),
        _ => None,
    }
}

fn conv_non_compound_assign(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        conv_walk_fn(
            f,
            &mut |s, in_loop| {
                let Stmt::Assign { target, op: crate::ast::AssignOp::Assign, value, span } = s
                else {
                    return;
                };
                let Some(target_key) = conv_place_key(target) else { return };
                let ExprKind::Binary { op: bin_op, left, .. } = &value.kind else { return };
                let Some(compound) = conv_binop_to_compound(*bin_op) else { return };
                let Some(left_key) = conv_place_key(left) else { return };
                if left_key != target_key {
                    return;
                }
                // Дедуп с W_STR_CONCAT_LOOP (только эта ветка того правила
                // формально пересекается — она тоже гейтит `op == Assign`).
                if in_loop
                    && matches!(bin_op, crate::ast::BinOp::Add)
                    && conv_is_stringish(value)
                {
                    return;
                }
                let symbol = compound.trim_end_matches('=');
                out.push(LintWarning {
                    rule: "W_NON_COMPOUND_ASSIGN",
                    diag: Diagnostic::new(
                        format!(
                            "`{p} = {p} {sym} ...` повторяет LHS в RHS — Nova поддерживает \
                             компаунд `{p} {cmp} ...`: короче и не рискует рассинхроном \
                             LHS/RHS-аккумулятора при копипасте (nv-coding-style §29).",
                            p = target_key, sym = symbol, cmp = compound
                        ),
                        *span,
                    ),
                });
            },
            &mut |_e, _| {},
        );
    }
}

// ---------------------------------------------------------------------------
// W_WHILE_COUNTER_FOR_RANGE — счётчиковый `while` → `for in` (nv-coding-style
// §10, канон уже словами зафиксирован владельцем: «Итерация по диапазону —
// всегда for in, не while со счётчиком» — это МАШИННАЯ проверка того же
// правила). Паттерн:
//
//     mut i = START            // непосредственно перед while, тот же блок
//     while i < END { ...; i += 1 }     // i += 1 — ПОСЛЕДНИЙ statement тела
//
// → `for i in START..END { ... }` (`i <= END` → `START..=END`, Nova имеет
// inclusive-range, D-блок `03-syntax.md:1989` — `a..=b` нормализуется в
// `Range{start:a, end:b+1}`, семантически идентично).
//
// КОНСЕРВАТИВНО (нулевые ложные срабатывания важнее полноты) — молчит, если
// нарушено ЛЮБОЕ:
//   - `mut i = START` — НЕ непосредственно перед `while` в том же блоке
//     (`Stmt::Let` на позиции `[j]`, `while` на `[j+1]`, либо `while` —
//     `trailing`-выражение блока, если `[j]` последний statement);
//   - условие `while` — НЕ строго `i < END` / `i <= END` (сложное условие
//     `&&`/`||` и любой другой бинарный оператор — молчим, «строго»);
//   - тело `while` имеет `trailing`-выражение (yield-значение — не наш
//     статement-only паттерн) — молчим;
//   - инкремент `i += 1` / `i = i + 1` — НЕ последний statement тела;
//   - `i` присваивается ГДЕ-ТО ЕЩЁ в теле (кроме этого последнего
//     инкремента) — на ЛЮБОЙ глубине вложенности (over-conservative:
//     реассайн ВНУТРИ вложенного цикла своей ТЕНЕВОЙ переменной с тем же
//     именем тоже молчит — ложноотрицательно, но безопасно);
//   - `END` — не «простое место» (`conv_place_key`: ident/`@field`/цепочка
//     полей) — вызов/индексация переоценивались бы КАЖДУЮ итерацию в
//     `while` (может меняться), но РОВНО ОДИН раз в `for`-range — реальная
//     семантическая разница, поэтому голый Call/Index как `END` — молчим;
//   - `END`-место мутируется где-либо в теле (та же причина — for-range
//     снимает `END` ОДИН раз, `while` — каждую итерацию);
//   - `continue` встречается ГДЕ-ЛИБО в теле (any depth, включая вложенные
//     циклы — over-conservative: `continue` во ВЛОЖЕННОМ цикле относится к
//     НЕМУ, но Nova без label'ов не даёт дёшево различить «этого уровня»
//     от «того уровня», а лишний silence безопасен) — `continue` в `while`
//     прыгает МИМО инкремента (семантика сохраняется), но в `for` инкремент
//     неявный — тот же `continue` перескочил бы его ТОЖЕ одинаково, однако
//     мы сознательно занижаем recall здесь, а не рискуем;
//   - `i` используется ПОСЛЕ `while` в остатке того же блока (siblings
//     после `while` + `trailing` блока) — `for`-переменная не переживает
//     цикл (D58 scope), `while`-переменная — переживает (объявлена ДО).
//   - `while` несёт `invariants`/`decreases` (Plan 33.4 D.0.3 SMT-контракты)
//     — механическая замена потеряла бы их, не мигрируя — молчим.
// ---------------------------------------------------------------------------

/// `Stmt::Assign` — канонiчный инкремент-на-1 переменной `name`: либо
/// `name += 1`, либо `name = name + 1` (обе формы встречаются в std).
fn conv_is_increment_by_one(s: &Stmt, name: &str) -> bool {
    match s {
        Stmt::Assign { target, op: crate::ast::AssignOp::Add, value, .. } => {
            matches!(&target.kind, ExprKind::Ident(n) if n == name)
                && matches!(value.kind, ExprKind::IntLit(1))
        }
        Stmt::Assign { target, op: crate::ast::AssignOp::Assign, value, .. } => {
            matches!(&target.kind, ExprKind::Ident(n) if n == name)
                && matches!(&value.kind,
                    ExprKind::Binary { op: crate::ast::BinOp::Add, left, right }
                        if matches!(&left.kind, ExprKind::Ident(n) if n == name)
                            && matches!(right.kind, ExprKind::IntLit(1)))
        }
        _ => false,
    }
}

/// `true`, если где-то в `stmts` (любая глубина — includes вложенные
/// блоки/циклы/ветвления) есть `Stmt::Assign`/`Stmt::TupleAssign`,
/// присваивающий имени `name`. Используется, чтобы убедиться, что счётчик
/// `i` не мутируется НИГДЕ, кроме проверенного последнего инкремента
/// (который вызывающая сторона исключает из среза `stmts`).
fn conv_body_reassigns_ident(stmts: &[Stmt], name: &str) -> bool {
    let mut found = false;
    for s in stmts {
        conv_walk_stmt(
            s,
            false,
            &mut |st, _| match st {
                Stmt::Assign { target, .. } => {
                    if matches!(&target.kind, ExprKind::Ident(n) if n == name) {
                        found = true;
                    }
                }
                Stmt::TupleAssign { lhs, .. } => {
                    if lhs.iter().any(|e| matches!(&e.kind, ExprKind::Ident(n) if n == name)) {
                        found = true;
                    }
                }
                _ => {}
            },
            &mut |_, _| {},
        );
    }
    found
}

/// `true`, если где-то в `stmts` есть `Stmt::Assign`, чей target имеет тот
/// же `conv_place_key`, что `key` — используется для «END не мутируется».
fn conv_body_reassigns_place(stmts: &[Stmt], key: &str) -> bool {
    let mut found = false;
    for s in stmts {
        conv_walk_stmt(
            s,
            false,
            &mut |st, _| {
                if let Stmt::Assign { target, .. } = st {
                    if conv_place_key(target).as_deref() == Some(key) {
                        found = true;
                    }
                }
            },
            &mut |_, _| {},
        );
    }
    found
}

/// `true`, если где-то в `stmts` (любая глубина) есть `continue`.
/// Over-conservative по дизайну — см. блок-комментарий правила выше
/// («continue во вложенном цикле» тоже гасит находку).
fn conv_body_has_continue(stmts: &[Stmt]) -> bool {
    let mut found = false;
    for s in stmts {
        conv_walk_stmt(
            s,
            false,
            &mut |st, _| {
                if matches!(st, Stmt::Continue(_)) {
                    found = true;
                }
            },
            &mut |_, _| {},
        );
    }
    found
}

/// `true`, если `name` встречается как `Ident` где-либо в `stmts`.
fn conv_stmts_reference_ident(stmts: &[Stmt], name: &str) -> bool {
    let mut found = false;
    for s in stmts {
        conv_walk_stmt(
            s,
            false,
            &mut |_, _| {},
            &mut |e, _| {
                if matches!(&e.kind, ExprKind::Ident(n) if n == name) {
                    found = true;
                }
            },
        );
    }
    found
}

/// `true`, если `name` встречается как `Ident` где-либо в `e`.
fn conv_expr_references_ident(e: &Expr, name: &str) -> bool {
    let mut found = false;
    conv_walk_expr(
        e,
        false,
        &mut |_, _| {},
        &mut |ex, _| {
            if matches!(&ex.kind, ExprKind::Ident(n) if n == name) {
                found = true;
            }
        },
    );
    found
}

/// `END` — голый int-литерал (опционально унарный `-`)? Литерал стабилен по
/// конструкции (нечего мутировать) — не нуждается в `conv_body_reassigns_place`.
/// Возвращает текстовое представление для сообщения (`"3"`, `"-1"`).
fn conv_end_int_literal(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::IntLit(n) => Some(n.to_string()),
        ExprKind::Unary { op: crate::ast::UnOp::Neg, operand } => {
            if let ExprKind::IntLit(n) = operand.kind {
                Some(format!("-{n}"))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Проверяет один кандидат (`mut name = start` непосредственно перед
/// `while`) по всем критериям правила; `Some(warning)` — все критерии
/// выполнены, `None` — молчим (см. блок-комментарий правила).
fn conv_check_while_counter(
    name: &str,
    counter_ty: &Option<TypeRef>,
    while_expr: &Expr,
    after_stmts: &[Stmt],
    after_trailing: Option<&Expr>,
) -> Option<LintWarning> {
    let ExprKind::While { cond, body, invariants, decreases } = &while_expr.kind else {
        return None;
    };
    if !invariants.is_empty() || decreases.is_some() {
        return None;
    }
    let ExprKind::Binary { op, left, right } = &cond.kind else { return None };
    let inclusive = match op {
        crate::ast::BinOp::Lt => false,
        crate::ast::BinOp::Le => true,
        _ => return None,
    };
    if !matches!(&left.kind, ExprKind::Ident(n) if n == name) {
        return None;
    }
    // END — либо простое место (ident/`@field`/цепочка полей, проверяется
    // ниже на не-мутацию), либо int-литерал (стабилен по конструкции —
    // литерал нечего мутировать, `while c < 3` — обычный случай, не только
    // именованная граница).
    let end_is_literal = conv_end_int_literal(right).is_some();
    let end_key = if end_is_literal {
        conv_end_int_literal(right).unwrap()
    } else {
        conv_place_key(right)?
    };

    if body.trailing.is_some() || body.stmts.is_empty() {
        return None;
    }
    let last = body.stmts.last().unwrap();
    if !conv_is_increment_by_one(last, name) {
        return None;
    }
    let rest = &body.stmts[..body.stmts.len() - 1];
    if conv_body_reassigns_ident(rest, name) {
        return None;
    }
    if !end_is_literal && conv_body_reassigns_place(&body.stmts, &end_key) {
        return None;
    }
    if conv_body_has_continue(&body.stmts) {
        return None;
    }
    if conv_stmts_reference_ident(after_stmts, name) {
        return None;
    }
    if let Some(t) = after_trailing {
        if conv_expr_references_ident(t, name) {
            return None;
        }
    }

    let range_op = if inclusive { "..=" } else { ".." };
    let cmp = if inclusive { "<=" } else { "<" };
    // Plan 87 `for x TYPE in iter` — если у счётчика была явная аннотация
    // типа (`mut y i32 = ...`), она ОБЯЗАНА перейти на for-переменную:
    // `for y in a..b` инферит тип из границ диапазона (обычно голый `int`),
    // МОЛЧА расширяя/сужая относительно исходного `i32`/`u8`/… — реальный
    // регресс (найден этой же волной на std/time/civil/tz.nv: `mut y i32 =
    // 2007`-счётчик, переданный в `fn(y i32)`, без аннотации на for стал
    // `int` → E_IMPLICIT_NARROWING на вызове). Суффикс с типом в
    // предложении — не декоративный, это ОБЯЗАТЕЛЬНАЯ часть корректной
    // замены при explicit-типе.
    let ty_suffix = counter_ty
        .as_ref()
        .and_then(conv_ty_last_name)
        .map(|t| format!(" {t}"))
        .unwrap_or_default();
    Some(LintWarning {
        rule: "W_WHILE_COUNTER_FOR_RANGE",
        diag: Diagnostic::new(
            format!(
                "счётчиковый `while {name} {cmp} {end_key}` (`mut {name}{ty_suffix} = ...` перед \
                 циклом, `{name} += 1` последним statement'ом тела) — канон `for {name}{ty_suffix} \
                 in <start>{range_op}{end_key} {{ ... }}` (nv-coding-style §10): исключает \
                 off-by-one/забытый инкремент, `i` не переживает цикл.{ty_note}",
                ty_note = if ty_suffix.is_empty() {
                    ""
                } else {
                    " ВАЖНО: явная аннотация типа счётчика ОБЯЗАНА перейти на for-переменную \
                     (Plan 87 `for x TYPE in iter`) — иначе for-range инферит тип из границ \
                     диапазона, молча расширяя/сужая относительно исходного типа."
                }
            ),
            while_expr.span,
        ),
    })
}

/// Сканирует `stmts` (+ опциональный `trailing` блока) на пары `mut i =
/// start` непосредственно перед `while`. `while` может быть либо
/// `Stmt::Expr` на позиции `[j+1]`, либо самим `trailing` блока (если
/// `Stmt::Let` — последний statement).
fn conv_scan_stmts_for_while_counter(stmts: &[Stmt], trailing: Option<&Expr>, out: &mut Vec<LintWarning>) {
    let n = stmts.len();
    for i in 0..n {
        let Stmt::Let(d) = &stmts[i] else { continue };
        if !d.mutable || d.is_ghost || d.consume {
            continue;
        }
        let Pattern::Ident { name, .. } = &d.pattern else { continue };

        let (while_expr, after_stmts, after_trailing): (&Expr, &[Stmt], Option<&Expr>) =
            if i + 1 < n {
                let Stmt::Expr(e) = &stmts[i + 1] else { continue };
                if !matches!(e.kind, ExprKind::While { .. }) {
                    continue;
                }
                (e, &stmts[i + 2..], trailing)
            } else if let Some(t) = trailing {
                if !matches!(t.kind, ExprKind::While { .. }) {
                    continue;
                }
                (t, &[], None)
            } else {
                continue;
            };

        if let Some(w) =
            conv_check_while_counter(name, &d.ty, while_expr, after_stmts, after_trailing)
        {
            out.push(w);
        }
    }
}

fn conv_while_counter_for_range(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        match &f.body {
            FnBody::Block(b) => conv_walk_block_for_while_counter(b, out),
            FnBody::Expr(_) | FnBody::External => {}
        }
    }
}

fn conv_walk_block_for_while_counter(b: &Block, out: &mut Vec<LintWarning>) {
    conv_scan_stmts_for_while_counter(&b.stmts, b.trailing.as_deref(), out);
    for s in &b.stmts {
        conv_walk_stmt_for_while_counter(s, out);
    }
    if let Some(t) = &b.trailing {
        conv_walk_expr_for_while_counter(t, out);
    }
}

fn conv_walk_stmt_for_while_counter(s: &Stmt, out: &mut Vec<LintWarning>) {
    match s {
        Stmt::Let(d) => conv_walk_expr_for_while_counter(&d.value, out),
        Stmt::Const(d) => conv_walk_expr_for_while_counter(&d.value, out),
        Stmt::Expr(e) => conv_walk_expr_for_while_counter(e, out),
        Stmt::Assign { target, value, .. } => {
            conv_walk_expr_for_while_counter(target, out);
            conv_walk_expr_for_while_counter(value, out);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                conv_walk_expr_for_while_counter(e, out);
            }
            for e in rhs {
                conv_walk_expr_for_while_counter(e, out);
            }
        }
        Stmt::Return { value: Some(v), .. } => conv_walk_expr_for_while_counter(v, out),
        Stmt::Throw { value, .. } => conv_walk_expr_for_while_counter(value, out),
        Stmt::Defer { body, .. } => conv_walk_expr_for_while_counter(body, out),
        Stmt::ConsumeScope { init, body, .. } => {
            conv_walk_expr_for_while_counter(init, out);
            conv_walk_block_for_while_counter(body, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            conv_walk_expr_for_while_counter(expr, out);
        }
        _ => {}
    }
}

fn conv_walk_expr_for_while_counter(e: &Expr, out: &mut Vec<LintWarning>) {
    match &e.kind {
        ExprKind::If { cond, then, else_ } => {
            conv_walk_expr_for_while_counter(cond, out);
            conv_walk_block_for_while_counter(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => conv_walk_block_for_while_counter(b, out),
                    ElseBranch::If(ie) => conv_walk_expr_for_while_counter(ie, out),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            conv_walk_expr_for_while_counter(scrutinee, out);
            conv_walk_block_for_while_counter(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => conv_walk_block_for_while_counter(b, out),
                    ElseBranch::If(ie) => conv_walk_expr_for_while_counter(ie, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            conv_walk_expr_for_while_counter(scrutinee, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    conv_walk_expr_for_while_counter(g, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => conv_walk_expr_for_while_counter(e, out),
                    MatchArmBody::Block(b) => conv_walk_block_for_while_counter(b, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            conv_walk_expr_for_while_counter(iter, out);
            conv_walk_block_for_while_counter(body, out);
        }
        ExprKind::While { cond, body, .. } => {
            conv_walk_expr_for_while_counter(cond, out);
            conv_walk_block_for_while_counter(body, out);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            conv_walk_expr_for_while_counter(scrutinee, out);
            if let Some(g) = guard {
                conv_walk_expr_for_while_counter(g, out);
            }
            conv_walk_block_for_while_counter(body, out);
        }
        ExprKind::Loop { body, .. } => conv_walk_block_for_while_counter(body, out),
        ExprKind::Block(b) => conv_walk_block_for_while_counter(b, out),
        ExprKind::Call { func, args, trailing } => {
            conv_walk_expr_for_while_counter(func, out);
            for a in args {
                conv_walk_expr_for_while_counter(a.expr(), out);
            }
            if let Some(t) = trailing {
                match t {
                    crate::ast::Trailing::Block(b) => conv_walk_block_for_while_counter(b, out),
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                        conv_walk_block_for_while_counter(&tb.body, out)
                    }
                    crate::ast::Trailing::Fn(sb) => match &sb.body {
                        FnBody::Expr(e) => conv_walk_expr_for_while_counter(e, out),
                        FnBody::Block(b) => conv_walk_block_for_while_counter(b, out),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Expr(e) => conv_walk_expr_for_while_counter(e, out),
            FnBody::Block(b) => conv_walk_block_for_while_counter(b, out),
            FnBody::External => {}
        },
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e) => conv_walk_expr_for_while_counter(e, out),
            ClosureBody::Block(b) => conv_walk_block_for_while_counter(b, out),
        },
        ExprKind::Lambda { body, .. } => conv_walk_expr_for_while_counter(body, out),
        ExprKind::Spawn(x) | ExprKind::Throw(x) => conv_walk_expr_for_while_counter(x, out),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => conv_walk_block_for_while_counter(b, out),
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                conv_walk_expr_for_while_counter(c, out);
            }
            if let Some(dl) = deadline {
                conv_walk_expr_for_while_counter(&dl.expr, out);
            }
            conv_walk_block_for_while_counter(body, out);
        }
        ExprKind::With { bindings: _, body } => conv_walk_block_for_while_counter(body, out),
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            conv_walk_block_for_while_counter(body, out)
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                if let Some(g) = &arm.guard {
                    conv_walk_expr_for_while_counter(g, out);
                }
                conv_walk_block_for_while_counter(&arm.body, out);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// W_MANUAL_CLAMP / W_MANUAL_MIN_MAX — ручной if/else вместо @clamp/@max/@min
// (Plan 185, заказ владельца 2026-07-20, стилевой линт семейства). Прецедент
// — clippy `manual_clamp`/`manual_min`/`manual_max`/`comparison_chain`.
//
// SEMANTIC-UPGRADE: без типов не знаем, есть ли у операндов метод
// `@clamp`/`@max`/`@min` вовсе (нужен Comparable/Ints-бланкет) — гейт
// консервативен синтаксически: обе ветви возвращают РОВНО операнды
// сравнения (или int/float-литерал, буквально совпадающий с операндом
// сравнения) — значит побочных эффектов нет (`conv_operand_key` признаёт
// только «простое место»/литерал), переупорядочивание вычислений безопасно.
//
// Самоссылочный сайт: `@min`/`@max`/`@clamp` (std/runtime/defaults.nv,
// std/prelude/protocols.nv Ints-бланкет, std/time/duration/core.nv) САМИ
// реализованы РОВНО этим if/else-паттерном (`export fn int @min(other int)
// -> int => if @ < other { @ } else { other }` и т.п.) — предложение
// «замени на `.max(...)`» внутри тела `@max` было бы рекурсией на себя.
// ОБА правила молчат внутри любой fn с ИМЕНЕМ буквально `min`/`max`/
// `clamp` — узко по имени, НЕ по receiver'у (гасит и свободные функции-
// реализации: `spec_tests/conformance/standalone/
// f11_corpus_06_pattern_regression.nv` содержит `fn max(a int, b int) ->
// int => if a > b { a } else { b }` — пиновая регрессия ИМЕННО этой формы,
// легитимно остаётся нетронутой). W_MANUAL_MIN_MAX ТОЖЕ гасится внутри
// `clamp`-именованных fn — тело `@clamp` содержит внутренний `if`,
// который САМ по себе валидный 2-операндный min/max-шейп (не рекурсия НА
// `@clamp`, но: канон-референсная реализация вне периметра волны +
// generic `fn[T Ints] T @clamp` резолвит `.min()`/`.max()` на T не
// проверено — см. комментарий у `conv_manual_min_max` ниже).
//
// W_MANUAL_CLAMP НАМЕРЕННО не покрывает вложенные вызовы-цепочки
// `x.min(hi).max(lo)` / `x.max(lo).min(hi)` (упомянутые как альтернативная
// форма в задании) — они РАСХОДЯТСЯ с `@clamp` на инвертированном диапазоне
// (`lo > hi`): `@clamp` определён как `if @ < lo { lo } else if @ > hi {
// hi } else { @ }` — при `lo > hi` для `@ < lo` возвращает `lo`, тогда как
// ОБЕ цепочки `x.max(lo).min(hi)`/`x.min(hi).max(lo)` при том же `lo > hi`
// для `x < lo` вернули бы `hi` (проверено алгебраически: `max(x,lo)=lo`
// т.к. `x<lo`, затем `min(lo,hi)=hi` т.к. `lo>hi`, — разное значение).
// Синтаксический линт не может исключить `lo > hi` во время сборки, значит
// подсказка была бы ПОВЕДЕНЧЕСКИ рискованной в этом крайнем случае; корпус
// (std/spec_tests/examples) не содержит ни одного реального сайта такой
// цепочки — сужение критериев без потери охвата (никого не подавляем).
// ---------------------------------------------------------------------------

/// Операнд, безопасный для линтов семейства min/max/clamp: «простое место»
/// (`conv_place_key` — ident/`@field`/цепочка полей, без побочных эффектов у
/// receiver'а) ИЛИ голый int/float-литерал (опционально унарный `-`).
/// Литералы НЕ префиксируются — идентификатор Nova не может состоять
/// только из цифр, коллизий с `conv_place_key` нет; строка одновременно
/// служит ключом равенства И читаемым текстом для сообщения диагностики.
fn conv_operand_key(e: &Expr) -> Option<String> {
    if let Some(k) = conv_place_key(e) {
        return Some(k);
    }
    match &e.kind {
        ExprKind::IntLit(n) => Some(n.to_string()),
        ExprKind::FloatLit(f) => Some(conv_format_float(*f)),
        ExprKind::Unary { op: crate::ast::UnOp::Neg, operand } => match &operand.kind {
            ExprKind::IntLit(n) => Some(format!("-{n}")),
            ExprKind::FloatLit(f) => Some(format!("-{}", conv_format_float(*f))),
            _ => None,
        },
        _ => None,
    }
}

/// `f64` → короткая однозначная текстовая форма для сообщения (`0.0`, не
/// `0`) — иллюстративная, не обязана байт-в-байт воспроизводить исходный
/// литерал.
fn conv_format_float(f: f64) -> String {
    if f.is_finite() && f == f.trunc() {
        format!("{f:.1}")
    } else {
        f.to_string()
    }
}

/// Единственное значение блока — trailing-выражение ИЛИ единственный
/// `return`-statement (то же расширение формы, что и в
/// `lint_value_record_unnecessary_promote::rec_lit` выше в этом файле).
fn conv_block_single_value(b: &Block) -> Option<&Expr> {
    match (&b.stmts[..], &b.trailing) {
        ([], Some(t)) => Some(t),
        ([Stmt::Return { value: Some(v), .. }], None) => Some(v),
        _ => None,
    }
}

/// Символ бинарного сравнения для текста диагностики.
fn conv_cmp_symbol(op: crate::ast::BinOp) -> &'static str {
    use crate::ast::BinOp;
    match op {
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        _ => "?",
    }
}

/// Дано `L OP R` (одна из четырёх форм сравнения) и значение, которое ветвь
/// возвращает при истинном условии (`v_true`) и при ложном (`v_false`) —
/// ОБА непременно равны (по `conv_operand_key`) либо `l_key`, либо `r_key`.
/// Возвращает `"max"`/`"min"`, если пара однозначно соответствует канону;
/// `None` — вырожденный случай (ни один операнд не совпал / оба совпали).
fn conv_minmax_method(
    op: crate::ast::BinOp,
    l_key: &str,
    r_key: &str,
    v_true: &str,
    v_false: &str,
) -> Option<&'static str> {
    use crate::ast::BinOp;
    let is_gt = matches!(op, BinOp::Gt | BinOp::Ge);
    let is_lt = matches!(op, BinOp::Lt | BinOp::Le);
    if !is_gt && !is_lt {
        return None;
    }
    if v_true == l_key && v_false == r_key {
        Some(if is_gt { "max" } else { "min" })
    } else if v_true == r_key && v_false == l_key {
        Some(if is_gt { "min" } else { "max" })
    } else {
        None
    }
}

/// Одна попытка сматчить `if` под W_MANUAL_MIN_MAX — либо expr-форма
/// (`if a > b { a } else { b }`), либо statement-форма без `else`
/// (`if x > hi { x = hi }`, ограничение на месте одной границей).
fn conv_manual_min_max_check(e: &Expr) -> Option<LintWarning> {
    let ExprKind::If { cond, then, else_ } = &e.kind else { return None };
    let ExprKind::Binary { op, left, right } = &cond.kind else { return None };
    if !matches!(
        op,
        crate::ast::BinOp::Lt | crate::ast::BinOp::Le | crate::ast::BinOp::Gt | crate::ast::BinOp::Ge
    ) {
        return None;
    }
    let l_key = conv_operand_key(left)?;
    let r_key = conv_operand_key(right)?;
    if l_key == r_key {
        return None; // вырожденное сравнение места с самим собой
    }
    match else_ {
        Some(ElseBranch::Block(else_block)) => {
            let then_key = conv_operand_key(conv_block_single_value(then)?)?;
            let else_key = conv_operand_key(conv_block_single_value(else_block)?)?;
            let method = conv_minmax_method(*op, &l_key, &r_key, &then_key, &else_key)?;
            Some(LintWarning {
                rule: "W_MANUAL_MIN_MAX",
                diag: Diagnostic::new(
                    format!(
                        "ручной `if {l} {cmp} {r} {{ ... }} else {{ ... }}` вычисляет \
                         {word} двух операндов — канон `{l}.{method}({r})` (nv-coding-style \
                         §30, прецедент clippy manual_min/manual_max).",
                        l = l_key,
                        cmp = conv_cmp_symbol(*op),
                        r = r_key,
                        word = if method == "max" { "максимум" } else { "минимум" },
                        method = method,
                    ),
                    e.span,
                ),
            })
        }
        // `else if` — потенциальный трёхветочный W_MANUAL_CLAMP, не наш
        // двухоперандный случай; тот линт проверяет ту же ноду отдельно.
        Some(ElseBranch::If(_)) => None,
        None => {
            if then.trailing.is_some() {
                return None;
            }
            let [Stmt::Assign { target, op: crate::ast::AssignOp::Assign, value, .. }] = &then.stmts[..]
            else {
                return None;
            };
            let target_key = conv_place_key(target)?;
            if target_key != l_key && target_key != r_key {
                return None; // цель присваивания — не операнд сравнения
            }
            let value_key = conv_operand_key(value)?;
            let method = conv_minmax_method(*op, &l_key, &r_key, &value_key, &target_key)?;
            Some(LintWarning {
                rule: "W_MANUAL_MIN_MAX",
                diag: Diagnostic::new(
                    format!(
                        "ручной `if {l} {cmp} {r} {{ {t} = ... }}` ограничивает `{t}` на \
                         месте одной границей — канон `{t} = {t}.{method}({v})` \
                         (nv-coding-style §30).",
                        l = l_key,
                        cmp = conv_cmp_symbol(*op),
                        r = r_key,
                        t = target_key,
                        method = method,
                        v = value_key,
                    ),
                    e.span,
                ),
            })
        }
    }
}

fn conv_manual_min_max(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    // Дедуп с W_MANUAL_CLAMP: внутренний `if` трёхветочного clamp-паттерна
    // (вторая проверка — `else if`/вложенный `if`) сам по себе валидный
    // 2-операндный min/max-шейп; синтаксический walker посещает его ОТДЕЛЬНЫМ
    // узлом (pre-order обход заходит и в `ElseBranch::If`, и во вложенный
    // блок). Если это ИМЕННО тот внутренний `if`, который W_MANUAL_CLAMP уже
    // разобрал как половину валидного трёхветочного паттерна — не дублируем
    // тем же сайтом половинчатой min/max-подсказкой (clamp покрывает ОБЕ
    // границы разом).
    //
    // ВАЖНО: сравнение по СОДержИМому span'а внешнего `if`-узла НЕ работает
    // здесь — `parser::parse_if` считает span if-выражения как `start(if
    // keyword)..end(then-блока)`, `else`-цепочка в него НЕ включена (см.
    // `parse_if`), значит span внешнего clamp-узла НЕ содержит span
    // вложенного `if` физически (они формально НЕ во вложенности, хотя
    // AST-узел вложен). Поэтому дедуп — по ТОЧНОМУ совпадению span'а
    // ВНУТРЕННЕГО узла (`conv_manual_clamp_check` возвращает его как часть
    // своего результата) — этот span у обоих правил вычисляется на ОДНОМ и
    // том же распарсенном узле, совпадает байт-в-байт.
    let consumed = conv_collect_clamp_consumed_spans(m);
    for f in conv_all_fns(m) {
        // `min`/`max` — прямая рекурсия (см. блок-комментарий выше).
        // `clamp` — ТОЖЕ молчит: тело `@clamp` (Ints-бланкет
        // protocols.nv, f32/f64 defaults.nv) содержит ВНУТРЕННИЙ `if @ >
        // hi { hi } else { @ }`, который сам по себе валидный
        // 2-операндный min/max-шейп (это НЕ рекурсия на `@clamp` — вызов
        // `.min()`/`.max()` из тела `@clamp` не циклится), но: (а) это
        // канон-РЕФЕРЕНСНАЯ реализация, от которой всё остальное
        // отталкивается — трогать её этой стилевой волной не входит в
        // периметр; (б) `fn[T Ints] T @clamp` — generic, а `.min()`/
        // `.max()` определены КОНКРЕТНО per-type (defaults.nv), НЕ через
        // Ints-бланкет — резолвится ли вызов на T внутри generic-тела БЕЗ
        // отдельного bound-требования, не проверено (не наша забота
        // проверять — избегаем риска). Тот же гейт закрывает
        // `spec_tests/conformance/method_with_args_ok.nv::Bounded4_1
        // @clamp` — файл пинует ИМЕННО этот if/else-шейп для теста
        // ro-caching кодогена (докстринг «4 reads ro fields — cache
        // emitted»), а не только W_MANUAL_CLAMP-совпадающие сайты.
        if f.name == "min" || f.name == "max" || f.name == "clamp" {
            continue;
        }
        conv_walk_fn(
            f,
            &mut |_s, _in_loop| {},
            &mut |e, _in_loop| {
                if consumed.contains(&(e.span.start, e.span.end)) {
                    return;
                }
                if let Some(w) = conv_manual_min_max_check(e) {
                    out.push(w);
                }
            },
        );
    }
}

/// Множество span'ов «внутренних» if-узлов, уже разобранных
/// `conv_manual_clamp_check` как вторая половина валидного трёхветочного
/// clamp-паттерна где-то в модуле — используется ТОЛЬКО для дедупа
/// W_MANUAL_MIN_MAX (см. блок-комментарий там). Не зависит от порядка
/// правил в `CONV_RULES` (реестра) — прогоняется отдельно.
fn conv_collect_clamp_consumed_spans(m: &Module) -> HashSet<(usize, usize)> {
    let mut consumed = HashSet::new();
    for f in conv_all_fns(m) {
        if f.name == "clamp" {
            continue;
        }
        conv_walk_fn(
            f,
            &mut |_s, _in_loop| {},
            &mut |e, _in_loop| {
                if let Some((_w, inner_span)) = conv_manual_clamp_check(e) {
                    consumed.insert((inner_span.start, inner_span.end));
                }
            },
        );
    }
    consumed
}

/// Внутренний `if`, скрытый ЛИБО за сахаром `else if ...` (`ElseBranch::If`),
/// ЛИБО за буквальным вложенным блоком `else { if ... }` (семантически то
/// же самое — прецедент `spec_tests/conformance/method_with_args_ok.nv`).
/// Обе формы равноценны для W_MANUAL_CLAMP.
fn conv_nested_if(else_: &ElseBranch) -> Option<&Expr> {
    match else_ {
        ElseBranch::If(inner) => Some(inner),
        ElseBranch::Block(b) => {
            let v = conv_block_single_value(b)?;
            if matches!(v.kind, ExprKind::If { .. }) {
                Some(v)
            } else {
                None
            }
        }
    }
}

/// Матч трёхветочного clamp-паттерна на ОДНОМ `if`-узле — байт-в-байт та же
/// форма, что и каноническая реализация `@clamp` (`if @ < lo { lo } else if
/// @ > hi { hi } else { @ } }`), с точностью до имён операндов/направления
/// (`>`-проверка может идти первой). Возвращает находку И span вложенного
/// `if`-узла (вторая проверка) — используется `conv_collect_clamp_consumed_
/// spans` для дедупа с W_MANUAL_MIN_MAX (см. там).
fn conv_manual_clamp_check(e: &Expr) -> Option<(LintWarning, Span)> {
    let ExprKind::If { cond: cond1, then: then1, else_: else1 } = &e.kind else { return None };
    let ExprKind::Binary { op: op1, left: l1, right: r1 } = &cond1.kind else { return None };
    if !matches!(
        op1,
        crate::ast::BinOp::Lt | crate::ast::BinOp::Le | crate::ast::BinOp::Gt | crate::ast::BinOp::Ge
    ) {
        return None;
    }
    let x_key = conv_operand_key(l1)?;
    let b1_key = conv_operand_key(r1)?;
    if x_key == b1_key {
        return None;
    }
    let v1_key = conv_operand_key(conv_block_single_value(then1)?)?;
    if v1_key != b1_key {
        return None;
    }

    let inner = conv_nested_if(else1.as_ref()?)?;
    let ExprKind::If { cond: cond2, then: then2, else_: else2 } = &inner.kind else { return None };
    let ExprKind::Binary { op: op2, left: l2, right: r2 } = &cond2.kind else { return None };
    if !matches!(
        op2,
        crate::ast::BinOp::Lt | crate::ast::BinOp::Le | crate::ast::BinOp::Gt | crate::ast::BinOp::Ge
    ) {
        return None;
    }
    if conv_operand_key(l2)? != x_key {
        return None; // сравнивается не тот же операнд, что снаружи
    }
    let b2_key = conv_operand_key(r2)?;
    let v2_key = conv_operand_key(conv_block_single_value(then2)?)?;
    if v2_key != b2_key {
        return None;
    }

    let ElseBranch::Block(final_block) = else2.as_ref()? else { return None };
    let v3_key = conv_operand_key(conv_block_single_value(final_block)?)?;
    if v3_key != x_key {
        return None; // финальная ветвь обязана вернуть исходный операнд
    }

    let is_lo1 = matches!(op1, crate::ast::BinOp::Lt | crate::ast::BinOp::Le);
    let is_lo2 = matches!(op2, crate::ast::BinOp::Lt | crate::ast::BinOp::Le);
    if is_lo1 == is_lo2 || b1_key == b2_key {
        return None; // обе проверки в одну сторону / одна и та же граница
    }
    let (lo_key, hi_key) = if is_lo1 { (b1_key, b2_key) } else { (b2_key, b1_key) };

    Some((
        LintWarning {
            rule: "W_MANUAL_CLAMP",
            diag: Diagnostic::new(
                format!(
                    "ручной трёхветочный if/else-if ограничивает `{x}` диапазоном \
                     `[{lo}, {hi}]` — канон `{x}.clamp({lo}, {hi})` (nv-coding-style §30, \
                     прецедент clippy manual_clamp). Направление границ у clamp'а легко \
                     перепутать вручную — machine-applicable подсказка ловит именно этот \
                     класс багов.",
                    x = x_key,
                    lo = lo_key,
                    hi = hi_key,
                ),
                e.span,
            ),
        },
        inner.span,
    ))
}

fn conv_manual_clamp(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        if f.name == "clamp" {
            continue; // само-ссылочный сайт — см. блок-комментарий выше
        }
        conv_walk_fn(
            f,
            &mut |_s, _in_loop| {},
            &mut |e, _in_loop| {
                if let Some((w, _inner_span)) = conv_manual_clamp_check(e) {
                    out.push(w);
                }
            },
        );
    }
}

// ---------------------------------------------------------------------------
// W_RESULT_DISCARDED — тихое глотание Result (nv-coding-style §4):
//   а) `ro _ = <вызов>` — discard-биндинг результата вызова;
//   б) swallow-match: арм `Err(_) => ()` / `Err(_) => {}` без обработки.
//
// SEMANTIC-UPGRADE: голый вызов-statement с отброшенным Result требует
// типов (не знаем, Result ли возврат) — не покрыт синтаксической версией.
// ---------------------------------------------------------------------------

fn conv_result_discarded(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    // SEMANTIC-UPGRADE: без типов не знаем, Result ли возврат — гейтим
    // discard-биндинг на callee-имена, семантика которых почти наверняка
    // Result (ошибку close/flush/write глотать нельзя — ENOSPC-класс).
    // Option-дропы (`ro _ = map.remove(k)` / `stack.pop()`) легальны.
    const RESULT_CALLEES: &[&str] = &[
        "close", "flush", "write", "write_all", "send", "shutdown", "commit",
        "sync", "wait",
    ];
    fn callee_name(e: &Expr) -> Option<&str> {
        match &e.kind {
            ExprKind::Call { func, .. } => match &func.kind {
                ExprKind::Member { name, .. } => Some(name.as_str()),
                ExprKind::Ident(n) => Some(n.as_str()),
                ExprKind::Path(p) => p.last().map(String::as_str),
                _ => None,
            },
            ExprKind::Try(x) | ExprKind::Bang(x) => callee_name(x),
            _ => None,
        }
    }
    fn is_call_like(e: &Expr) -> bool {
        callee_name(e).map_or(false, |n| RESULT_CALLEES.contains(&n))
    }
    for f in conv_all_fns(m) {
        // Один проход, находки в локальный буфер (уникальный доступ к out
        // нельзя делить между двумя замыканиями walker'а).
        let mut found: Vec<(&'static str, String, Span)> = Vec::new();
        {
            let mut stmt_hits: Vec<Span> = Vec::new();
            let mut arm_hits: Vec<Span> = Vec::new();
            conv_walk_fn(
                f,
                &mut |s, _| {
                    if let Stmt::Let(d) = s {
                        if matches!(d.pattern, Pattern::Wildcard(_)) && is_call_like(&d.value) {
                            stmt_hits.push(d.span);
                        }
                    }
                },
                &mut |e, _| {
                    let ExprKind::Match { arms, .. } = &e.kind else { return };
                    for arm in arms {
                        let is_err_wildcard = matches!(&arm.pattern,
                            Pattern::Variant { path, kind, .. }
                                if path.last().map(String::as_str) == Some("Err")
                                    && matches!(kind,
                                        crate::ast::VariantPatternKind::Tuple { patterns, .. }
                                            if patterns.iter().all(|p| matches!(p, Pattern::Wildcard(_)))));
                        if !is_err_wildcard {
                            continue;
                        }
                        let body_empty = match &arm.body {
                            MatchArmBody::Expr(x) => matches!(x.kind, ExprKind::UnitLit),
                            MatchArmBody::Block(b) => b.stmts.is_empty() && b.trailing.is_none(),
                        };
                        if body_empty {
                            arm_hits.push(arm.span);
                        }
                    }
                },
            );
            for sp in stmt_hits {
                found.push((
                    "discard",
                    "`ro _ = <вызов>` — discard-биндинг глотает результат (в т.ч. \
                     возможную ошибку Result) молча (nv-coding-style §4). Ошибку \
                     обработайте (`?` / `!!` / match) или задокументируйте намеренный \
                     дроп комментарием на месте."
                        .to_string(),
                    sp,
                ));
            }
            for sp in arm_hits {
                found.push((
                    "swallow",
                    "swallow-match: арм `Err(_) => ()` глотает ошибку молча \
                     (nv-coding-style §4). Обработайте (лог/проброс/`?`) или \
                     задокументируйте намеренное игнорирование."
                        .to_string(),
                    sp,
                ));
            }
        }
        for (_kind, msg, sp) in found {
            out.push(LintWarning {
                rule: "W_RESULT_DISCARDED",
                diag: Diagnostic::new(msg, sp),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// W_MANUAL_COALESCE (Ф.3, [M-manual-coalesce-lint-missing], D86 AMEND
// 2026-07-23) — ручной `match X { Ok(v) => v, Err(_) => D }` / `{ Some(v) =>
// v, None => D }` (identity-рука: рука успеха возвращает РОВНО тот
// идентификатор, что связан в паттерне) — дрейф от канона `X ?? D`.
//
// НЕ ловит: `Ok(_) => false` (разный идентификатор/wildcard в руке —
// `.is_ok()`/`.is_err()`), `Ok(v) => f(v)` (рука не идентична паттерну —
// `.map(f) ?? d`), разные имена в паттерне и руке, guard'ы на любом арме.
// Критично (владелец): в корпусе 189 НЕ-identity сайтов против 69 целевых —
// эти исключения обязательны, иначе линт бесполезен (шум).
// ---------------------------------------------------------------------------

/// Успех-арм `Ok(v) => v` / `Some(v) => v` — рука в точности идентификатор,
/// связанный в паттерне (identity). Возвращает `(is_result, bound_name)`.
fn conv_coalesce_identity_arm(arm: &MatchArm) -> Option<(bool, &str)> {
    let Pattern::Variant { path, kind, .. } = &arm.pattern else { return None };
    let is_result = match path.last().map(String::as_str) {
        Some("Ok") => true,
        Some("Some") => false,
        _ => return None,
    };
    let VariantPatternKind::Tuple { patterns, rest: false } = kind else { return None };
    let [Pattern::Ident { name, is_mut: false, is_consume: false, .. }] = &patterns[..] else {
        return None;
    };
    let body_expr: &Expr = match &arm.body {
        MatchArmBody::Expr(be) => be,
        MatchArmBody::Block(b) if b.stmts.is_empty() => b.trailing.as_ref()?,
        MatchArmBody::Block(_) => return None,
    };
    match &body_expr.kind {
        ExprKind::Ident(n) if n == name.as_str() => Some((is_result, name.as_str())),
        _ => None,
    }
}

/// Fallback-арм классифицируется по ФОРМЕ тела (та же таксономия, что D86 §
/// «fallback может быть»): значение-подобный (включая `panic`/`throw` —
/// обычные ВЫРАЖЕНИЯ, `??` их поддерживает без ретракции) vs `return ...`
/// (парсер эмитит его как `Block{stmts:[Stmt::Return],trailing:None}` —
/// `parse_match`, D19 exception для control-flow arm bodies).
enum CoalesceFbShape<'a> {
    /// Значение / `panic(...)` / `throw ...` — канон `X ?? D`, не задет
    /// ретракцией (D86 form остаётся легальной).
    Value,
    /// `return <expr>` (или голый `return`) — Ф.2-таблица применяется.
    Return(Option<&'a Expr>),
}

fn conv_coalesce_fb_shape(arm: &MatchArm) -> CoalesceFbShape<'_> {
    if let MatchArmBody::Block(b) = &arm.body {
        if b.trailing.is_none() {
            if let [Stmt::Return { value, .. }] = b.stmts.as_slice() {
                return CoalesceFbShape::Return(value.as_ref());
            }
        }
    }
    CoalesceFbShape::Value
}

/// `return Err(e)` где `e` — РОВНО тот идентификатор, что связан в
/// fallback-паттерне `Err(e)` — доказывает (синтаксически, без инференса),
/// что тип ошибки не меняется: verbatim passthrough.
fn conv_coalesce_is_err_passthrough(ret_expr: &Expr, bound_name: &str) -> bool {
    if let ExprKind::Call { func, args, trailing: None } = &ret_expr.kind {
        if matches!(&func.kind, ExprKind::Ident(n) if n == "Err") {
            if let [CallArg::Item(inner)] = &args[..] {
                return matches!(&inner.kind, ExprKind::Ident(n) if n == bound_name);
            }
        }
    }
    false
}

/// Ф.3-адаптер decision-функции Ф.2 (`crate::types::coalesce_return_fallback_
/// advice`) — у линта НЕТ type-checker'а (`ConvRule.ast`-хуки чисто
/// синтаксические), поэтому carrier/E-тип не ИНФЕРИРУЮТСЯ, а СИНТЕЗИРУЮТСЯ
/// из того, что видно в самом AST без инференса:
/// - carrier (`Option`/`Result`) — читается напрямую из success-арма
///   паттерна (`Ok`⟹Result, `Some`⟹Option) — 100% надёжно, это не эвристика.
/// - return-тип функции — DECLARED `FnDecl.return_type` (синтаксис, не
///   инференс — обычный parsed `TypeRef`).
/// - E-тип операнда (нужен ТОЛЬКО чтобы отличить Ф.2 row2 "тот же E" от
///   row4 "E меняется") — эвристика: `return Err(e)` verbatim passthrough
///   ⟹ E совпадает с F return-типа (клонируем F-generic оттуда, гарантируя
///   `typeref_equal` true); иначе — синтетический sentinel-тип, который НЕ
///   МОЖЕТ структурно совпасть ни с одним реальным именем (гарантирует ветку
///   `MapErr`). Это НЕ подмена типов — это МИНИМАЛЬНЫЙ носитель, достаточный
///   для существующей decision-функции, честно документированный.
fn conv_coalesce_advice_for_return(
    is_result: bool,
    fb_bound_err_name: Option<&str>,
    ret_expr: Option<&Expr>,
    f: &FnDecl,
) -> crate::types::CoalesceReturnAdvice {
    let dummy_span = crate::diag::Span::new(0, 0);
    let ret_ty = f.return_type.clone();
    let op_ty = if is_result {
        let is_passthrough = match (fb_bound_err_name, ret_expr) {
            (Some(name), Some(re)) => conv_coalesce_is_err_passthrough(re, name),
            _ => false,
        };
        let err_ty = if is_passthrough {
            ret_ty.as_ref().and_then(|rt| match rt {
                TypeRef::Named { generics, .. } => generics.get(1).cloned(),
                _ => None,
            })
        } else {
            None
        }
        .unwrap_or_else(|| TypeRef::Named {
            path: vec!["__coalesce_lint_distinct_err_sentinel__".to_string()],
            generics: vec![],
            span: dummy_span,
        });
        TypeRef::Named {
            path: vec!["Result".to_string()],
            generics: vec![TypeRef::Unit(dummy_span), err_ty],
            span: dummy_span,
        }
    } else {
        TypeRef::Named { path: vec!["Option".to_string()], generics: vec![], span: dummy_span }
    };
    crate::types::coalesce_return_fallback_advice(
        Some(&op_ty), ret_ty.as_ref(), &HashMap::new())
}

fn conv_manual_coalesce(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        let mut hits: Vec<(Span, String, Option<crate::diag::Suggestion>)> = Vec::new();
        conv_walk_fn(f, &mut |_, _| {}, &mut |e, _| {
            let ExprKind::Match { arms, .. } = &e.kind else { return };
            if arms.len() != 2 {
                return;
            }
            let Some(ident_idx) = arms.iter().position(|a| conv_coalesce_identity_arm(a).is_some())
            else {
                return;
            };
            let fb_idx = 1 - ident_idx;
            let (is_result, _bound) = conv_coalesce_identity_arm(&arms[ident_idx]).unwrap();
            let fb_arm = &arms[fb_idx];
            if fb_arm.guard.is_some() || arms[ident_idx].guard.is_some() {
                return; // guard меняет семантику — не чистый coalesce
            }
            // Fallback-арм обязан быть ИМЕННО другой половиной суммы —
            // иначе это не coalesce-форма вовсе (обе руки Ok, например).
            let fb_bound_err_name: Option<&str> = match &fb_arm.pattern {
                Pattern::Variant { path, kind, .. } => {
                    let variant = path.last().map(String::as_str);
                    let expected = if is_result { "Err" } else { "None" };
                    if variant != Some(expected) {
                        return;
                    }
                    if is_result {
                        match kind {
                            VariantPatternKind::Tuple { patterns, rest: false } => {
                                match &patterns[..] {
                                    [Pattern::Ident { name, .. }] => Some(name.as_str()),
                                    [Pattern::Wildcard(_)] => None,
                                    _ => return,
                                }
                            }
                            _ => return,
                        }
                    } else {
                        None
                    }
                }
                _ => return,
            };
            // `??`'s desugar discards the caught `Err`-payload entirely
            // (`Err(_) => fallback` — no binding reaches the fallback side).
            // If the fallback body FREELY references the bound error name
            // (e.g. `Err(e) => { log(e); D }`), rewriting to `X ?? D` would
            // silently drop that reference — NOT an equivalent rewrite, so
            // suppress (stay silent) rather than suggest a wrong canon. This
            // only applies to the Value bucket: the Return-class bucket's
            // `.map_err(fn(e E) -> F => ...)` bridge CAN reference `e` (it's
            // the closure's own parameter), so no analogous restriction there.
            if let Some(name) = fb_bound_err_name {
                let references_bound_name = match &fb_arm.body {
                    MatchArmBody::Expr(be) => {
                        let mut free = HashSet::new();
                        crate::types::capture_scan_expr(be, &mut HashSet::new(), &mut free);
                        free.contains(name)
                    }
                    MatchArmBody::Block(b) => {
                        let mut free = HashSet::new();
                        crate::types::capture_scan_block(b, &mut HashSet::new(), &mut free);
                        free.contains(name)
                    }
                };
                if references_bound_name && matches!(conv_coalesce_fb_shape(fb_arm), CoalesceFbShape::Value) {
                    return;
                }
            }
            let (note, suggestion) = match conv_coalesce_fb_shape(fb_arm) {
                CoalesceFbShape::Value => (
                    "значение-fallback (в т.ч. `panic`/`throw` — обычные выражения, \
                     эту форму D86-ретракция не затрагивает) — канон `X ?? D`."
                        .to_string(),
                    None, // embed произвольного текста X/D — вне AST-only lint'а без source-map
                ),
                CoalesceFbShape::Return(ret_expr) => {
                    let advice =
                        conv_coalesce_advice_for_return(is_result, fb_bound_err_name, ret_expr, f);
                    // МОЛЧАНИЕ (владелец, Ф.3): `glob.nv`-класс (обёртки для
                    // проброса нет — `NoBridgeKnown`) и генерик/невыведенный
                    // случай (`Unknown`) НЕ дрейф от канона `??` — `??` сам
                    // не смог бы это выразить (Ф.2 отверг бы тем же путём),
                    // значит `match` здесь — единственная законная форма, а
                    // не «ручной эквивалент». Не толкаем к неверной переписке.
                    if matches!(
                        advice,
                        crate::types::CoalesceReturnAdvice::NoBridgeKnown
                            | crate::types::CoalesceReturnAdvice::Unknown
                    ) {
                        return;
                    }
                    let (note, suggestion) = crate::types::coalesce_advice_render(&advice, e.span);
                    // `MapErr`'s Suggestion embeds `source_err_display` — for
                    // the checker (Ф.2) that's a REAL inferred type; here
                    // (Ф.3, syntax-only heuristic) it's the internal sentinel
                    // placeholder (`conv_coalesce_advice_for_return` doc) when
                    // not a verbatim passthrough — showing that name to the
                    // programmer would be confusing/wrong. Drop the
                    // Suggestion in that one case; the note (target error
                    // type, real) stays informative.
                    let suggestion = if matches!(
                        advice,
                        crate::types::CoalesceReturnAdvice::MapErr { .. }
                    ) {
                        None
                    } else {
                        suggestion
                    };
                    (note, suggestion)
                }
            };
            hits.push((
                e.span,
                format!(
                    "ручной `match X {{ {ok}(v) => v, {err} => D }}` — дрейф от канона \
                     `X ?? D` (D86; амендмент 2026-07-07 ретрактировал `unwrap_or`-\
                     близнецов именно в пользу `?? v`). {note}",
                    ok = if is_result { "Ok" } else { "Some" },
                    err = if is_result { "Err(_)" } else { "None" },
                    note = note,
                ),
                suggestion,
            ));
        });
        for (span, msg, suggestion) in hits {
            let mut diag = Diagnostic::new(msg, span);
            if let Some(s) = suggestion {
                diag = diag.with_suggestion(s);
            }
            out.push(LintWarning { rule: "W_MANUAL_COALESCE", diag });
        }
    }
}

// ---------------------------------------------------------------------------
// W_MANUAL_COLLECT ([M-manual-collect-lint-missing], Пункт 22 / Plan 200) —
// ручной collect: `mut v = <пустой ctor>; for x in <iter> { v.push(x) }` —
// дрейф от канона `mut v = <iter>.collect()`.
//
// Условие БЕЗ ложных срабатываний (строго синтаксически):
//  (1) `v` объявлена ПУСТЫМ конструктором коллекции (`[]T.new()` /
//      `Vec[T].new()` / пустой литерал `[]`) НЕПОСРЕДСТВЕННО перед циклом —
//      for обязан быть СЛЕДУЮЩИМ statement того же блока, поэтому «v не
//      используется между объявлением и циклом» выполняется тривиально;
//  (2) тело цикла — РОВНО `v.push(<loop_var>)`: один statement (либо
//      trailing-выражение), receiver — голый ident `v`, аргумент push —
//      ГОЛАЯ loop-переменная (не `f(x)`, не под `if`, не несколько);
//  (3) loop-pattern — простой ident (не destructure).
//
// Только identity-collect. Расширения (`push(f(x))` → `.map(f).collect()`;
// `if c { push(x) }` → `.filter(c).collect()`) — НЕ в этой версии (владелец
// 2026-07-24). Прецедент clippy `manual_collect`/`needless_collect`.
// ---------------------------------------------------------------------------

/// `e` — ПУСТОЙ конструктор коллекции: `[]T.new()` / `Vec[T].new()` (ровно
/// 0 аргументов) либо пустой литерал `[]`. Ненулевой `.new(cap)` или
/// непустой литерал — НЕ пустой ctor (преаллокация/инициализация меняет
/// намерение), не матчим.
fn conv_is_empty_collection_ctor(e: &Expr) -> bool {
    match &e.kind {
        // Пустой литерал `[]` (парсер даёт `ArrayLit(vec![])`).
        ExprKind::ArrayLit(elems) => elems.is_empty(),
        // `[]T.new()` / `Vec[T].new()` — 0-арг static-ctor.
        ExprKind::Call { func, args, trailing: None } => {
            if !args.is_empty() {
                return false;
            }
            let ExprKind::Member { obj, name } = &func.kind else { return false };
            if name != "new" {
                return false;
            }
            match &obj.kind {
                // `[]T.new()` → `Path(["__array", <T>])` (D38 slice-sugar).
                ExprKind::Path(p) => p.first().map(String::as_str) == Some("__array"),
                // `Vec[T].new()` → `TurboFish{ base: Ident("Vec")/Path..Vec, [T] }`.
                ExprKind::TurboFish { base, type_args } => {
                    type_args.len() == 1
                        && match &base.kind {
                            ExprKind::Ident(n) => n == "Vec",
                            ExprKind::Path(p) => p.last().map(String::as_str) == Some("Vec"),
                            _ => false,
                        }
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// Тело for-цикла — РОВНО одно выражение (один `Stmt::Expr` без trailing,
/// либо пустые stmts + trailing-выражение). Иначе `None` (несколько
/// statement'ов / пусто).
fn conv_for_body_single_expr(b: &Block) -> Option<&Expr> {
    match (b.stmts.as_slice(), &b.trailing) {
        ([Stmt::Expr(e)], None) => Some(e),
        ([], Some(t)) => Some(t.as_ref()),
        _ => None,
    }
}

/// `expr` — РОВНО `v.push(loop_var)`: receiver = голый ident `v`, единственный
/// аргумент — голый ident `loop_var`, без trailing-блока.
fn conv_is_identity_push(expr: &Expr, v: &str, loop_var: &str) -> bool {
    let ExprKind::Call { func, args, trailing: None } = &expr.kind else { return false };
    let ExprKind::Member { obj, name } = &func.kind else { return false };
    if name != "push" {
        return false;
    }
    if !matches!(&obj.kind, ExprKind::Ident(n) if n == v) {
        return false;
    }
    match &args[..] {
        [CallArg::Item(a)] => matches!(&a.kind, ExprKind::Ident(n) if n == loop_var),
        _ => false,
    }
}

/// Непосредственные блоки, которыми ВЛАДЕЕТ узел-выражение (без рекурсии).
/// Рекурсия обхода делегируется `conv_walk_*` — здесь только извлечение, так
/// что ни один вложенный блок не пропущен и не посещён дважды.
fn conv_blocks_of_expr(e: &Expr) -> Vec<&Block> {
    let mut v: Vec<&Block> = Vec::new();
    match &e.kind {
        ExprKind::Block(b) => v.push(b),
        ExprKind::If { then, else_, .. } | ExprKind::IfLet { then, else_, .. } => {
            v.push(then);
            if let Some(ElseBranch::Block(b)) = else_ {
                v.push(b);
            }
        }
        ExprKind::For { body, .. }
        | ExprKind::ParallelFor { body, .. }
        | ExprKind::While { body, .. }
        | ExprKind::WhileLet { body, .. }
        | ExprKind::Loop { body, .. }
        | ExprKind::Supervised { body, .. }
        | ExprKind::Detach(body)
        | ExprKind::Blocking(body)
        | ExprKind::With { body, .. }
        | ExprKind::Forbid { body, .. }
        | ExprKind::Realtime { body, .. } => v.push(body),
        ExprKind::Match { arms, .. } => {
            for a in arms {
                if let MatchArmBody::Block(b) = &a.body {
                    v.push(b);
                }
            }
        }
        ExprKind::Select { arms } => {
            for a in arms {
                v.push(&a.body);
            }
        }
        ExprKind::ClosureLight { body: ClosureBody::Block(b), .. } => v.push(b),
        ExprKind::ClosureFull(sb) => {
            if let FnBody::Block(b) = &sb.body {
                v.push(b);
            }
        }
        _ => {}
    }
    v
}

/// Каноническая `<iter>.collect()`-переписка: рендер итератора через
/// `print_expr` + (а) обёртка в скобки, если верхний уровень итератора —
/// низкоприоритетная форма (Range/Binary/…) и без скобок приклеился бы к
/// `.collect()` неверно; (б) честная `Applicability`. `print_expr` НЕ
/// расставляет скобки вокруг вложенных Range/closure/if/match и рендерит
/// `TurboFish` лоссовым плейсхолдером `[..]` → при их наличии где-либо внутри
/// понижаем до `MaybeIncorrect` (правка правдоподобна, но требует ревью).
fn conv_collect_canon_iter(iter: &Expr) -> (String, Applicability) {
    let src = crate::ast::pretty::print_expr(iter);
    let needs_wrap = matches!(
        &iter.kind,
        ExprKind::Range { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Unary { .. }
            | ExprKind::As(..)
            | ExprKind::Is(..)
            | ExprKind::Coalesce(..)
            | ExprKind::Lambda { .. }
            | ExprKind::ClosureLight { .. }
            | ExprKind::ClosureFull(..)
            | ExprKind::If { .. }
            | ExprKind::IfLet { .. }
            | ExprKind::Match { .. }
    );
    let wrapped = if needs_wrap { format!("({src})") } else { src };
    let mut faithful = true;
    conv_walk_expr(iter, false, &mut |_, _| {}, &mut |x, _| {
        if matches!(
            &x.kind,
            ExprKind::Range { .. }
                | ExprKind::TurboFish { .. }
                | ExprKind::As(..)
                | ExprKind::Is(..)
                | ExprKind::Coalesce(..)
                | ExprKind::Lambda { .. }
                | ExprKind::ClosureLight { .. }
                | ExprKind::ClosureFull(..)
                | ExprKind::If { .. }
                | ExprKind::IfLet { .. }
                | ExprKind::Match { .. }
        ) {
            faithful = false;
        }
    });
    let app = if faithful {
        Applicability::MachineApplicable
    } else {
        Applicability::MaybeIncorrect
    };
    (wrapped, app)
}

/// Скан ОДНОЙ последовательности statement'ов на паттерн ручного collect
/// (соседние `mut v = <пустой ctor>` + `for x in it { v.push(x) }`). Работает
/// in-place (`out` — владеющий вектор находок), поэтому вызывается прямо из
/// обходных замыканий — HRTB-ссылки `&Block` живут ровно на время вызова, не
/// утекают (в отличие от накопления `&Block` в долгоживущий вектор).
fn conv_scan_block_for_collect(b: &Block, out: &mut Vec<LintWarning>) {
    let stmts = &b.stmts;
    for i in 0..stmts.len() {
        let Stmt::Let(d) = &stmts[i] else { continue };
        if d.is_ghost || d.consume || !d.mutable {
            continue; // push требует mut-биндинга
        }
        let Pattern::Ident { name, .. } = &d.pattern else { continue };
        if !conv_is_empty_collection_ctor(&d.value) {
            continue;
        }
        // Следующий statement того же блока обязан быть for-циклом.
        let Some(Stmt::Expr(for_expr)) = stmts.get(i + 1) else { continue };
        let ExprKind::For { pattern, iter, body, .. } = &for_expr.kind else { continue };
        let Pattern::Ident { name: loop_var, .. } = pattern else { continue };
        let Some(body_expr) = conv_for_body_single_expr(body) else { continue };
        if !conv_is_identity_push(body_expr, name, loop_var) {
            continue;
        }
        let region = d.span.merge(for_expr.span);
        let (iter_src, applicability) = conv_collect_canon_iter(iter);
        let canon = format!("mut {name} = {iter_src}.collect()");
        out.push(LintWarning {
            rule: "W_MANUAL_COLLECT",
            diag: Diagnostic::new(
                format!(
                    "ручной collect: `mut {name} = <пустой ctor>` + \
                     `for {loop_var} in <iter> {{ {name}.push({loop_var}) }}` — \
                     дрейф от канона `{canon}` (nv-coding-style §33, прецедент \
                     clippy manual_collect/needless_collect). `.collect()` \
                     материализует итератор одним выражением.",
                ),
                region,
            )
            .with_suggestion(Suggestion {
                message: format!("канон: `{canon}`"),
                span: region,
                replacement: canon,
                applicability,
            }),
        });
    }
}

fn conv_manual_collect(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    // Каждый блок сканируется РОВНО один раз: топ-блоки тел — явно; вложенные
    // блоки — через владеющий их узел (on_expr / on_stmt). Два прохода
    // `conv_walk_*` на функцию (expr-владельцы / stmt-владельцы `ConsumeScope`)
    // — раздельно, чтобы не держать два `&mut out`-замыкания одновременно.
    for f in conv_all_fns(m) {
        if let FnBody::Block(b) = &f.body {
            conv_scan_block_for_collect(b, out);
        }
        conv_walk_fn(f, &mut |_, _| {}, &mut |e, _| {
            for nb in conv_blocks_of_expr(e) {
                conv_scan_block_for_collect(nb, out);
            }
        });
        conv_walk_fn(
            f,
            &mut |s, _| {
                if let Stmt::ConsumeScope { body, .. } = s {
                    conv_scan_block_for_collect(body, out);
                }
            },
            &mut |_, _| {},
        );
    }
    for tb in conv_all_test_bodies(m) {
        conv_scan_block_for_collect(tb, out);
        conv_walk_block(tb, false, &mut |_, _| {}, &mut |e, _| {
            for nb in conv_blocks_of_expr(e) {
                conv_scan_block_for_collect(nb, out);
            }
        });
        conv_walk_block(
            tb,
            false,
            &mut |s, _| {
                if let Stmt::ConsumeScope { body, .. } = s {
                    conv_scan_block_for_collect(body, out);
                }
            },
            &mut |_, _| {},
        );
    }
}

// ---------------------------------------------------------------------------
// W_MANUAL_SLICE_TO_END ([M-manual-slice-bounds-lint-missing], Пункт 22) —
// избыточные границы диапазона среза. Три редукции (ТОЛЬКО exclusive `..`):
//  (1) recv[a..recv.len()] / recv[a..recv.byte_len()] -> recv[a..]
//  (2) recv[0..b]                                     -> recv[..b]
//  (3) recv[0..recv.len()]                            -> recv[..]
//
// Условие БЕЗ ложных срабатываний: end — ГОЛЫЙ 0-арг вызов `.len()`/
// `.byte_len()` на ТОМ ЖЕ receiver-выражении, что и срез (структурное
// равенство «чистого места» — ident/`@`/поле/индекс БЕЗ вызовов: вызов в
// receiver'е нельзя дважды вычислять безопасно, не матчим). Для (2) start —
// литерал `0`. Тип знать не нужно — матчим по факту вызова len-метода на том
// же receiver. Fix-it машинный (удаление/замена по точным AST-span'ам).
// Прецедент clippy redundant-slicing.
// ---------------------------------------------------------------------------

/// Каноническая строка «чистого места» — детерминированного выражения БЕЗ
/// вызовов/побочных эффектов: ident / `@` / Path / цепочка полей / индекс с
/// чистым индексом. `None` для всего, что содержит вызов или произвольное
/// выражение — тогда «тот же receiver» синтаксически доказать нельзя.
fn conv_pure_place_key(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(n) => Some(format!("i:{n}")),
        ExprKind::SelfAccess => Some("@".to_string()),
        ExprKind::Path(p) => Some(format!("p:{}", p.join("."))),
        ExprKind::Member { obj, name } => {
            let base = conv_pure_place_key(obj)?;
            Some(format!("{base}.{name}"))
        }
        ExprKind::Index { obj, index } => {
            let base = conv_pure_place_key(obj)?;
            let idx = conv_pure_index_key(index)?;
            Some(format!("{base}[{idx}]"))
        }
        _ => None,
    }
}

/// Индекс, допустимый внутри «чистого места»: int-литерал или чистое место.
fn conv_pure_index_key(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::IntLit(n) => Some(format!("n:{n}")),
        ExprKind::Ident(_)
        | ExprKind::SelfAccess
        | ExprKind::Path(_)
        | ExprKind::Member { .. }
        | ExprKind::Index { .. } => conv_pure_place_key(e),
        _ => None,
    }
}

/// Если `e` — голый 0-арг вызов `.len()`/`.byte_len()`, вернуть его receiver.
fn conv_len_call_receiver(e: &Expr) -> Option<&Expr> {
    let ExprKind::Call { func, args, trailing: None } = &e.kind else { return None };
    if !args.is_empty() {
        return None;
    }
    let ExprKind::Member { obj, name } = &func.kind else { return None };
    if name == "len" || name == "byte_len" {
        Some(obj)
    } else {
        None
    }
}

fn conv_manual_slice_to_end(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        conv_walk_fn(f, &mut |_, _| {}, &mut |e, _| {
            let ExprKind::Index { obj, index } = &e.kind else { return };
            let ExprKind::Range { start, end, inclusive: false } = &index.kind else { return };
            // end — голый len/byte_len на ТОМ ЖЕ receiver, что и obj среза?
            let end_len_same = match end.as_deref() {
                Some(en) => match (conv_len_call_receiver(en), conv_pure_place_key(obj)) {
                    (Some(recv), Some(obj_key)) => conv_pure_place_key(recv) == Some(obj_key),
                    _ => false,
                },
                None => false,
            };
            // start — литерал `0`?
            let start_zero = match start.as_deref() {
                Some(s) => matches!(&s.kind, ExprKind::IntLit(0)),
                None => false,
            };
            let (form, sugg_span, sugg_repl): (&str, Span, String) =
                if end_len_same && start_zero {
                    // (3) recv[0..recv.len()] -> recv[..]
                    ("recv[0..recv.len()] → recv[..]", index.span, "..".to_string())
                } else if end_len_same {
                    // (1) recv[a..recv.len()] -> recv[a..]
                    (
                        "recv[a..recv.len()] → recv[a..]",
                        end.as_ref().unwrap().span,
                        String::new(),
                    )
                } else if start_zero {
                    // (2) recv[0..b] -> recv[..b]
                    (
                        "recv[0..b] → recv[..b]",
                        start.as_ref().unwrap().span,
                        String::new(),
                    )
                } else {
                    return;
                };
            out.push(LintWarning {
                rule: "W_MANUAL_SLICE_TO_END",
                diag: Diagnostic::new(
                    format!(
                        "избыточная граница диапазона среза ({form}) — канон открытый \
                         диапазон (spec 02-types.md, nv-coding-style §34, прецедент \
                         clippy redundant-slicing). Голый `len()`/`byte_len()` как end \
                         и литерал `0` как start подразумеваются автоматически.",
                    ),
                    e.span,
                )
                .with_suggestion(Suggestion {
                    message: "убрать избыточную границу диапазона".to_string(),
                    span: sugg_span,
                    replacement: sugg_repl,
                    applicability: Applicability::MachineApplicable,
                }),
            });
        });
    }
}

// ---------------------------------------------------------------------------
// W_COERCE_EXPLICIT_REDUNDANT — explicit call to a `#coerce`-registered
// method (Plan 214, D429 R9) where a bare value would coerce to the exact
// same result at this position. Generalized 2026-07-21 (owner correction on
// [M-bytes-literal-callarg-coerce-codegen-gap]'s lint follow-up — "не
// закладываться на str: линт должен ловить ЛЮБЫЕ избыточные явные конверсии,
// покрытые авто-коэрсией", pairs read from the registry, not hardcoded)
// to (a) cover the call-arg position — absorbing the former standalone
// `W_REDUNDANT_BYTES_ON_LITERAL` rule as this rule's call-arg lane instead
// of a second rule — and (b) build its method/shape table from the
// `#coerce fn` declarations ACTUALLY VISIBLE in the linted module (own
// `Item::Fn`s + same-folder peers, `conv_all_fns`'s scope) instead of a
// fixed 3-entry list, falling back to that list only when the scan finds
// nothing (a consumer file outside the declaring type's folder-module —
// `nova lint` has no cross-module import resolution, see the SEMANTIC-
// UPGRADE note below).
// ---------------------------------------------------------------------------

/// Plan 214 (D429 R9): the O-shape a seed `#coerce` method's zero-arg call
/// must land on for the call to be flagged redundant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoerceSeedShape {
    Str,
    BytesSlice,
}

/// The 3 std seed `#coerce` pairs (Plan 214 Ф.3), used as a FALLBACK when
/// the AST scan (`scan_coerce_methods`) finds no `#coerce fn` decl in the
/// linted module's own scope (e.g. a consumer file in a different folder-
/// module that only imports `str`/`StringBuilder`/`WriteBuffer` — `nova
/// lint` cannot see the declaring file's AST, see the SEMANTIC-UPGRADE note
/// on `conv_coerce_explicit_redundant`). Kept as literal data (not a
/// hardcoded RULE) — same role a registry miss/cold-cache fallback would
/// play, not a second source of truth: whenever the scan finds a REAL decl
/// for one of these names it fully supersedes the matching fallback entry.
const COERCE_SEED_METHODS: &[(&str, CoerceSeedShape)] = &[
    ("bytes", CoerceSeedShape::BytesSlice),
    ("into_bytes", CoerceSeedShape::BytesSlice),
    ("into_str", CoerceSeedShape::Str),
];

fn coerce_seed_shape_of(method: &str) -> Option<CoerceSeedShape> {
    COERCE_SEED_METHODS.iter().find(|(n, _)| *n == method).map(|(_, s)| *s)
}

fn coerce_ty_matches_shape(ty: &TypeRef, shape: CoerceSeedShape) -> bool {
    match shape {
        CoerceSeedShape::Str => matches!(ty,
            TypeRef::Named { path, generics, .. }
                if generics.is_empty() && path.len() == 1 && path[0] == "str"),
        CoerceSeedShape::BytesSlice => match ty {
            TypeRef::Array(inner, _) => matches!(&**inner,
                TypeRef::Named { path, generics, .. }
                    if generics.is_empty() && path.len() == 1 && path[0] == "u8"),
            TypeRef::Named { path, generics, .. }
                if path.len() == 1 && path[0] == "Vec" && generics.len() == 1 =>
            {
                matches!(&generics[0], TypeRef::Named { path, generics, .. }
                    if generics.is_empty() && path.len() == 1 && path[0] == "u8")
            }
            _ => false,
        },
    }
}

/// Registry-driven method-name set for THIS module: every zero-arg,
/// receiver-form `#coerce fn` visible via `conv_all_fns(m)` (own items +
/// same-folder peers — the same scope `MapLitCtx`/`collect_coerce_pairs`
/// scans for the checker's real registry, minus cross-module import
/// resolution `nova lint` structurally cannot do, see module doc above).
/// Best-effort: does NOT replicate D429's R1-R14 validation (a malformed
/// `#coerce fn` — wrong arity, generic, etc. — is a checker error elsewhere;
/// this scan only needs the method NAME to widen the lint's coverage, never
/// to accept/reject the program). Returns method name → O-shape when the O
/// type matches one of the two known shapes (Str / BytesSlice) this lint
/// currently knows how to compare against an expected-type position; a
/// `#coerce` pair returning some OTHER shape is invisible to this lint (safe
/// false-negative — no advice is worse than wrong advice).
fn scan_coerce_methods(m: &Module) -> HashMap<String, CoerceSeedShape> {
    let mut found = HashMap::new();
    for f in conv_all_fns(m) {
        if !f.coerce_attr || f.receiver.is_none() || !f.params.is_empty() {
            continue;
        }
        let Some(ret) = &f.return_type else { continue };
        // Strip a `ro`-wrapper (view lane) — shape comparison is on the
        // bare underlying type either way (finalize lane never wraps `ro`).
        let bare_ret = match ret {
            TypeRef::Readonly(inner, _) => inner.as_ref(),
            other => other,
        };
        let shape = if coerce_ty_matches_shape(bare_ret, CoerceSeedShape::Str) {
            Some(CoerceSeedShape::Str)
        } else if coerce_ty_matches_shape(bare_ret, CoerceSeedShape::BytesSlice) {
            Some(CoerceSeedShape::BytesSlice)
        } else {
            None
        };
        if let Some(shape) = shape {
            found.insert(f.name.clone(), shape);
        }
    }
    found
}

fn conv_coerce_explicit_redundant(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    // SEMANTIC-UPGRADE: `nova lint` is a syntactic, per-file, no-import-
    // resolution pass (see module doc at the top of this file) — it cannot
    // resolve a receiver's real type, so it cannot consult the ACTUAL
    // `#coerce` registry (`types/mod.rs::collect_coerce_pairs`, built from
    // the checker's module-merged AST) for arbitrary USER-declared pairs
    // (D429 R8: `#coerce` covers all types, not just std) living OUTSIDE the
    // linted module's own folder. `scan_coerce_methods` above narrows this
    // gap (reads REAL `#coerce fn` decls in scope instead of a fixed list),
    // but the fallback list still stands in for the same-shape decl living
    // in an out-of-folder module. This is a CONSERVATIVE approximation:
    // flags a bare zero-arg call to a known-shape `#coerce` method name
    // sitting at a SYNTACTICALLY explicit-type position (`let`/`const`
    // annotation, a fn's arrow-body / `return` matching its OWN declared
    // return type, OR — since 2026-07-21 — a call-arg, per D429 R9's own
    // note that call-arg needs no target-type check: if the call already
    // type-checks WITH the explicit `.method()` present, D429 R6 guarantees
    // the bare value would coerce identically at that same slot). Risk this
    // shares with the pre-existing non-call-arg lanes (documented there
    // already, not new): an UNRELATED type spelling the same method name
    // with the exact same shape, or (R16) a call-arg landing on a generic
    // catch-all overload where the coercion is genuinely NOT inserted — both
    // presumed rare enough in practice for the 3 seed pairs' well-known
    // names; a full semantic version (reading the real registry with import
    // resolution) belongs in a type-aware pass run inside the build
    // pipeline, a legitimate follow-up, not a silent gap.
    let dynamic = scan_coerce_methods(m);
    let shape_of = |method: &str| -> Option<CoerceSeedShape> {
        dynamic.get(method).copied().or_else(|| coerce_seed_shape_of(method))
    };

    // Bare `x.method()` — zero-arg method call on a LEAF receiver (plain
    // `Ident` or a str literal) ONLY. Found empirically (running this rule
    // over std): a fluent-chain receiver (`StringBuilder.new(...).append(y)
    // .into_str()`) also matches the method-name+use-site-type shape, but
    // stripping `.into_str()` there would NOT actually become coercible —
    // the AST-rewrite this lint's advice relies on
    // (`MapLitAnnotator::try_coerce_leaf`, types/mod.rs) is ITSELF leaf-only
    // (mirrors the pre-existing D55 `try_wrap_leaf` scope), so a bare
    // `StringBuilder.new(...).append(y)` left at a `str`-typed position would
    // NOT get rewritten back to `.into_str()` — CC-FAIL (`Nova_StringBuilder*`
    // where `nova_str` expected). Restricting to the SAME leaf shapes the
    // rewrite handles keeps this lint's advice always safe to apply.
    fn bare_seed_call(e: &Expr) -> Option<(&str, Span)> {
        let ExprKind::Call { func, args, .. } = &e.kind else { return None };
        if !args.is_empty() {
            return None;
        }
        let ExprKind::Member { obj, name } = &func.kind else { return None };
        if !matches!(&obj.kind, ExprKind::Ident(_) | ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. }) {
            return None;
        }
        Some((name.as_str(), e.span))
    }

    fn check_value(
        value: &Expr,
        expected: &TypeRef,
        shape_of: &dyn Fn(&str) -> Option<CoerceSeedShape>,
        out: &mut Vec<LintWarning>,
    ) {
        let Some((method, span)) = bare_seed_call(value) else { return };
        let Some(shape) = shape_of(method) else { return };
        if !coerce_ty_matches_shape(expected, shape) {
            return;
        }
        out.push(LintWarning {
            rule: "W_COERCE_EXPLICIT_REDUNDANT",
            diag: Diagnostic::new(
                format!(
                    "явный вызов `.{method}()` в позиции с явным ожидаемым типом — \
                     голое значение скоэрсировалось бы в ТО ЖЕ САМОЕ через `#coerce` \
                     (D429 R9). Уберите явный вызов — действует `#coerce`."
                ),
                span,
            ),
        });
    }

    // Call-arg position (D429 R9 + R6, absorbed 2026-07-21 from the former
    // standalone `W_REDUNDANT_BYTES_ON_LITERAL`): a `.method()` call sitting
    // as a call-ARGUMENT, on a receiver whose type is SYNTACTICALLY
    // guaranteed regardless of scope (`nova lint` cannot resolve a plain
    // `Ident`'s type here, unlike the let/return lanes above where the
    // POSITION'S own annotation supplies the expected type) — a literal, an
    // interpolation, OR a chain whose OUTERMOST call is `.to_str()` (owner
    // brief 2026-07-21: "литерал/интерполяция/чейн, оканчивающийся
    // `.to_str()`" — `.to_str()` is a near-universal Display-family method
    // returning `str` unconditionally across std, so a chain ending in it is
    // exactly as syntactically-str-guaranteed as a literal, no receiver-type
    // resolution needed). No target-type check needed here (unlike
    // `check_value` above): D429 R9's own note establishes call-arg
    // soundness without it — if `x.method()` type-checks as this call's
    // argument, R6 guarantees a bare `x` would coerce identically at that
    // same slot. A bare `Ident` receiver is intentionally EXCLUDED (owner's
    // literal-only brief for this lane, mirrors the retired rule's own
    // `no_warning_bytes_on_variable_call_arg` test) — only the THREE
    // syntactically-str-guaranteed shapes above ever fire here.
    fn is_to_str_chain(e: &Expr) -> bool {
        let ExprKind::Call { func, args, trailing } = &e.kind else { return false };
        if !args.is_empty() || trailing.is_some() {
            return false;
        }
        matches!(&func.kind, ExprKind::Member { name, .. } if name == "to_str")
    }

    fn check_call_arg(
        e: &Expr,
        shape_of: &dyn Fn(&str) -> Option<CoerceSeedShape>,
        out: &mut Vec<LintWarning>,
    ) {
        let ExprKind::Call { args, .. } = &e.kind else { return };
        for a in args {
            // Spread (`...expr`) changes arity/semantics — out of scope
            // (mirrors `conv_redundant_of`'s positional-only caution).
            if a.is_spread() {
                continue;
            }
            let arg_expr = a.expr();
            let ExprKind::Call { func, args: inner_args, trailing } = &arg_expr.kind else { continue };
            if !inner_args.is_empty() || trailing.is_some() {
                continue;
            }
            let ExprKind::Member { obj, name } = &func.kind else { continue };
            let is_str_guaranteed_receiver = matches!(
                obj.kind,
                ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. }
            ) || is_to_str_chain(obj);
            if !is_str_guaranteed_receiver {
                continue;
            }
            let Some(_shape) = shape_of(name) else { continue };
            out.push(LintWarning {
                rule: "W_COERCE_EXPLICIT_REDUNDANT",
                diag: Diagnostic::new(
                    format!(
                        "явный вызов `.{name}()` call-аргументом на синтаксически-\
                         гарантированном значении — голое значение скоэрсировалось бы \
                         в ТО ЖЕ САМОЕ через `#coerce` (D429 R6/R9). Уберите явный вызов."
                    ),
                    arg_expr.span,
                ),
            });
        }
    }

    for f in conv_all_fns(m) {
        match (&f.body, &f.return_type) {
            (FnBody::Expr(e), Some(ret)) => check_value(e, ret, &shape_of, out),
            // Block body's OWN trailing expr (no `return` keyword — implicit
            // return, the pervasive std idiom: `fn f() -> str { ...;
            // buf.into_str() }`). Mirrors `MapLitAnnotator::walk_fn_body_block`
            // (types/mod.rs) — same "outermost block only" scope (a nested
            // if/match arm's own trailing is only a return in tail position,
            // not tracked here either, consistent with that rewrite-side fix).
            (FnBody::Block(b), Some(ret)) => {
                if let Some(t) = &b.trailing {
                    check_value(t, ret, &shape_of, out);
                }
            }
            _ => {}
        }
        // `conv_walk_fn` takes TWO independent `&mut dyn FnMut` closures
        // (stmt + expr) — both need mutable access to the same accumulator,
        // which two DISTINCT closure captures can't share directly (each
        // would need its own unique `&mut out`, rejected by the borrow
        // checker even though the two closures are only ever called
        // sequentially, never concurrently). A `RefCell` sidesteps this:
        // both closures borrow it mutably only for the duration of their own
        // call, never overlapping in practice, so runtime borrow-checking
        // never panics; drained into the real `out` once the walk finishes.
        let local = std::cell::RefCell::new(Vec::new());
        conv_walk_fn(
            f,
            &mut |s, _| match s {
                Stmt::Let(d) => {
                    if let Some(ty) = &d.ty {
                        check_value(&d.value, ty, &shape_of, &mut local.borrow_mut());
                    }
                }
                Stmt::Const(d) => {
                    if let Some(ty) = &d.ty {
                        check_value(&d.value, ty, &shape_of, &mut local.borrow_mut());
                    }
                }
                Stmt::Return { value: Some(v), .. } => {
                    if let Some(ret) = &f.return_type {
                        check_value(v, ret, &shape_of, &mut local.borrow_mut());
                    }
                }
                _ => {}
            },
            &mut |e, _in_loop| check_call_arg(e, &shape_of, &mut local.borrow_mut()),
        );
        out.extend(local.into_inner());
    }
    // `test { }` block bodies — see `conv_all_test_bodies` doc (sibling of
    // `conv_all_fns`, covers a real gap the shared helper leaves open). Only
    // the call-arg lane applies here (no fn return-type/let-annot context
    // needed beyond what `check_call_arg` already handles standalone) — a
    // `test { }` block has no declared return type of its own, and its own
    // `let`/`const` positions ARE walked by `check_call_arg`'s sibling
    // statement-walk callback below via the SAME `conv_walk_block`, so this
    // mirrors the fn-body treatment exactly minus the two contexts a test
    // block structurally lacks.
    for tb in conv_all_test_bodies(m) {
        let local = std::cell::RefCell::new(Vec::new());
        conv_walk_block(
            tb,
            false,
            &mut |s, _| {
                if let Stmt::Let(d) = s {
                    if let Some(ty) = &d.ty {
                        check_value(&d.value, ty, &shape_of, &mut local.borrow_mut());
                    }
                } else if let Stmt::Const(d) = s {
                    if let Some(ty) = &d.ty {
                        check_value(&d.value, ty, &shape_of, &mut local.borrow_mut());
                    }
                }
            },
            &mut |e, _in_loop| check_call_arg(e, &shape_of, &mut local.borrow_mut()),
        );
        out.extend(local.into_inner());
    }
}

// ---------------------------------------------------------------------------
// W_REDUNDANT_CONSUME_REBIND — `consume y = x` as (one of) the arm block's
// own statements, where `x` is a name bound `consume` DIRECTLY by the
// immediately enclosing match arm's own pattern (`Ok(consume x)`) and never
// used again anywhere else in that arm's body (owner 2026-07-21). Advice:
// bind straight to the final name in the pattern (`Ok(consume y)`), dropping
// the rebind statement — a real corpus instance of exactly this double-
// rebind exists today (`std/src/fs/d323_open_options_test.nv`:
// `Ok(consume f0) => { consume f = f0; … }`).
// ---------------------------------------------------------------------------

fn conv_redundant_consume_rebind(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    fn variant_name(p: &Pattern) -> &str {
        match p {
            Pattern::Variant { path, .. } => path.last().map(String::as_str).unwrap_or("pattern"),
            _ => "pattern",
        }
    }
    fn consume_idents(p: &Pattern, acc: &mut Vec<String>) {
        match p {
            Pattern::Ident { name, is_consume: true, .. } => acc.push(name.clone()),
            Pattern::Variant { kind: VariantPatternKind::Tuple { patterns, .. }, .. } => {
                for sp in patterns {
                    consume_idents(sp, acc);
                }
            }
            Pattern::Tuple(pats, _) => {
                for sp in pats {
                    consume_idents(sp, acc);
                }
            }
            Pattern::Record { fields, .. } => {
                for rf in fields {
                    if let Some(sp) = &rf.pattern {
                        consume_idents(sp, acc);
                    }
                }
            }
            Pattern::Binding { inner, .. } => consume_idents(inner, acc),
            Pattern::Or { alternatives, .. } => {
                for a in alternatives {
                    consume_idents(a, acc);
                }
            }
            _ => {}
        }
    }
    fn ident_count(b: &Block, name: &str) -> usize {
        let mut n = 0usize;
        conv_walk_block(b, false, &mut |_, _| {}, &mut |e, _| {
            if let ExprKind::Ident(id) = &e.kind {
                if id == name {
                    n += 1;
                }
            }
        });
        n
    }

    fn check_expr(e: &Expr, out: &mut Vec<LintWarning>) {
        let ExprKind::Match { arms, .. } = &e.kind else { return };
        for arm in arms {
            let MatchArmBody::Block(body) = &arm.body else { continue };
            let mut names = Vec::new();
            consume_idents(&arm.pattern, &mut names);
            if names.is_empty() {
                continue;
            }
            let vname = variant_name(&arm.pattern);
            for old_name in &names {
                for s in &body.stmts {
                    let Stmt::Let(d) = s else { continue };
                    if !d.consume {
                        continue;
                    }
                    let Pattern::Ident { name: new_name, .. } = &d.pattern else {
                        continue;
                    };
                    let ExprKind::Ident(rhs) = &d.value.kind else { continue };
                    if rhs != old_name {
                        continue;
                    }
                    if ident_count(body, old_name) > 1 {
                        continue;
                    }
                    out.push(LintWarning {
                        rule: "W_REDUNDANT_CONSUME_REBIND",
                        diag: Diagnostic::new(
                            format!(
                                "`consume {new_name} = {old_name}` избыточен — \
                                 `{old_name}` уже `consume`-биндинг из паттерна \
                                 арма (`{vname}(consume {old_name})`), нигде \
                                 больше не используется. Бинди сразу: \
                                 `{vname}(consume {new_name})`."
                            ),
                            d.span,
                        ),
                    });
                }
            }
        }
    }

    for f in conv_all_fns(m) {
        conv_walk_fn(f, &mut |_s, _| {}, &mut |e, _in_loop| check_expr(e, out));
    }
    for tb in conv_all_test_bodies(m) {
        conv_walk_block(tb, false, &mut |_s, _| {}, &mut |e, _in_loop| check_expr(e, out));
    }
}

// ---------------------------------------------------------------------------
// W_MANUAL_CLOSE_AUTO_CLEANUP — a tail-position, zero-arg finalizer call
// (`x.close()` and siblings) on a `consume`-bound local whose EXPLICIT type
// annotation is a std type carrying `consume @cleanup` (D432 auto-cleanup:
// TcpStream/MutexGuard/ReadGuard/WriteGuard/Permit) — the scope-exit
// prologue already invokes `@cleanup` on every exit path (success/throw/
// panic/cancel), so a manual tail call duplicates it (owner 2026-07-21).
//
// SEMANTIC-UPGRADE: mirrors codegen's REAL `auto_cleanup_types`/
// `consume_cleanup_types` (emit_c.rs, scanned from `f.name == "cleanup" &&
// recv.consume` program-wide) with a hardcoded conservative seed of the five
// CURRENT std types declaring `consume @cleanup` (verified 2026-07-21:
// `net/tcp.nv` TcpStream, `runtime/sync.nv` MutexGuard/ReadGuard/
// WriteGuard/Permit) — a per-file syntactic pass can't see cross-file
// declarations, same conservative-seed precedent as
// `conv_coerce_explicit_redundant` above. Deliberately does NOT include
// `File` (fs.nv): documented BY DESIGN as a must-`@close` type WITHOUT
// auto-cleanup (`@close` returns `Result`, auto-cleanup can't surface the
// error) — excluded correctly, not an oversight.
//
// Scope restricted to the LAST statement of (a) a fn's own top-level body
// block and (b) a match-arm block — NOT if/while/for/loop-nested blocks
// (owner's brief: "только ХВОСТОВОЙ вызов"; a documented scope limit, not a
// silent gap — a full recursive block-walker is a legitimate follow-up).
// ---------------------------------------------------------------------------

const CONV_AUTO_CLEANUP_SEED_TYPES: &[&str] =
    &["TcpStream", "MutexGuard", "ReadGuard", "WriteGuard", "Permit"];

fn conv_manual_close_auto_cleanup(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    fn tail_call(block: &Block) -> Option<&Expr> {
        if let Some(t) = &block.trailing {
            return Some(t.as_ref());
        }
        match block.stmts.last() {
            Some(Stmt::Expr(e)) => Some(e),
            Some(Stmt::Let(d))
                if matches!(&d.pattern, Pattern::Ident { name, .. } if name == "_") =>
            {
                Some(&d.value)
            }
            _ => None,
        }
    }

    fn check_block(block: &Block, out: &mut Vec<LintWarning>) {
        // Consume-bindings, explicitly typed as a seed auto-cleanup type,
        // introduced directly in THIS block's own statement list.
        let mut seed_bindings: Vec<&str> = Vec::new();
        for s in &block.stmts {
            let Stmt::Let(d) = s else { continue };
            if !d.consume {
                continue;
            }
            let Pattern::Ident { name, .. } = &d.pattern else { continue };
            let Some(ty) = &d.ty else { continue };
            let Some(tn) = conv_ty_last_name(ty) else { continue };
            if CONV_AUTO_CLEANUP_SEED_TYPES.contains(&tn) {
                seed_bindings.push(name.as_str());
            }
        }
        if seed_bindings.is_empty() {
            return;
        }
        let Some(tail) = tail_call(block) else { return };
        let ExprKind::Call { func, args, trailing } = &tail.kind else { return };
        if !args.is_empty() || trailing.is_some() {
            return;
        }
        let ExprKind::Member { obj, name: method } = &func.kind else { return };
        let ExprKind::Ident(recv) = &obj.kind else { return };
        if !seed_bindings.contains(&recv.as_str()) {
            return;
        }
        out.push(LintWarning {
            rule: "W_MANUAL_CLOSE_AUTO_CLEANUP",
            diag: Diagnostic::new(
                format!(
                    "`{recv}.{method}()` хвостовым вызовом скоупа избыточен: авто-\
                     `@cleanup` (D432) уже закрывает `{recv}` на выходе из скоупа \
                     (успех/throw/panic/cancel). Уберите вызов."
                ),
                tail.span,
            ),
        });
    }

    fn check_expr(e: &Expr, out: &mut Vec<LintWarning>) {
        let ExprKind::Match { arms, .. } = &e.kind else { return };
        for arm in arms {
            if let MatchArmBody::Block(b) = &arm.body {
                check_block(b, out);
            }
        }
    }

    for f in conv_all_fns(m) {
        if let FnBody::Block(b) = &f.body {
            check_block(b, out);
        }
        conv_walk_fn(f, &mut |_s, _| {}, &mut |e, _in_loop| check_expr(e, out));
    }
    for tb in conv_all_test_bodies(m) {
        check_block(tb, out);
        conv_walk_block(tb, false, &mut |_s, _| {}, &mut |e, _in_loop| check_expr(e, out));
    }
}

// ---------------------------------------------------------------------------
// W_REDUNDANT_CONST_TYPE_ANNOTATION — `const`, module- or scope-level, whose
// explicit type annotation MATCHES the bare-literal-default type of its own
// initializer (`str`→str, `int`→int, `bool`→bool, `char`→char) — the type is
// inferred either way, so the annotation adds nothing (owner 2026-07-21,
// verified live: `const MESSAGE = "hello"` already infers `str`). Reuses the
// SAME default-type helpers `W_REDUNDANT_OF` already relies on
// (`conv_ty_literal_default_name` / `conv_expr_is_bare_literal_of`) — single
// source of truth for "what type does a bare literal default to", no new
// literal-default logic duplicated here.
//
// Deliberately NARROW: only `const` — module-level `Item::Const` AND
// scope-level `Stmt::Const` (Plan 114.4 Ф.2, SAME `ConstDecl` shape) — `let`/
// `ro`/`mut` locals are OUT of scope (owner: their annotation is often
// documentary, not a canon violation). Any annotation that does NOT match
// the bare-literal default (`[]u8`, `u32`, a narrower/wider numeric width,
// …) is left alone — those are load-bearing (coercion/narrowing), never
// flagged.
// ---------------------------------------------------------------------------

// [M-p67-path-call-const-receiver-method-ice] guard (found the HARD way
// this wave, 2026-07-21): a SCREAMING_SNAKE_CASE const name used later as
// `NAME.method(...)` parses as a 2-segment `ExprKind::Path(["NAME",
// "method"])`, not `Member{obj: Ident("NAME"), ..}` (parser's Path-vs-Member
// split is CASE-based, not type-based — see
// `examples/flagship/aggregator/regressions/const_receiver_generic_ext_ice/
// const_receiver_generic_ext_ice.nv`'s own doc comment for the full
// pre-existing root-cause writeup). Removing an EXPLICIT type from the
// `const` declaration can make the checker lose the annotation this Path
// shape needs and re-trigger the "[P67-LEGACY] Path call return type
// unknown" codegen ICE — `nova check` stays green (accept-path is fine),
// only `nova build`/`nova test` (real codegen) explodes. Caught empirically
// via `examples/mini_aggregator.nv`'s `BUDGET_MS.to_millis()` (CC-FAIL
// reproduced, reverted) DURING this wave's canonization sweep — this rule
// must not recommend the same footgun again. Conservative: skips flagging
// ANY const whose name is later used as a call-receiver (`NAME.method(...)`,
// `Member` OR `Path` shape) ANYWHERE in the module, even though only the
// Path-shape (SCREAMING_SNAKE_CASE + generic-extension method) is the
// actual trigger — false-negative-safe over false-positive-unsafe.
fn conv_collect_call_receiver_names(m: &Module) -> HashSet<String> {
    fn add_from_expr(e: &Expr, out: &mut HashSet<String>) {
        match &e.kind {
            ExprKind::Path(segs) if segs.len() >= 2 => {
                out.insert(segs[0].clone());
            }
            ExprKind::Call { func, .. } => {
                if let ExprKind::Member { obj, .. } = &func.kind {
                    if let ExprKind::Ident(n) = &obj.kind {
                        out.insert(n.clone());
                    }
                }
            }
            _ => {}
        }
    }
    let mut names = HashSet::new();
    for f in conv_all_fns(m) {
        conv_walk_fn(f, &mut |_, _| {}, &mut |e, _| add_from_expr(e, &mut names));
    }
    for tb in conv_all_test_bodies(m) {
        conv_walk_block(tb, false, &mut |_, _| {}, &mut |e, _| add_from_expr(e, &mut names));
    }
    names
}

fn conv_check_const_redundant_annotation(
    d: &ConstDecl,
    call_receiver_names: &HashSet<String>,
    out: &mut Vec<LintWarning>,
) {
    let Some(ty) = &d.ty else { return };
    let Some(default_name) = conv_ty_literal_default_name(ty) else { return };
    if !conv_expr_is_bare_literal_of(&d.value, default_name) {
        return;
    }
    if call_receiver_names.contains(&d.name) {
        return;
    }
    out.push(LintWarning {
        rule: "W_REDUNDANT_CONST_TYPE_ANNOTATION",
        diag: Diagnostic::new(
            format!(
                "аннотация `{name} {t}` избыточна — тип `{t}` и так выводится из \
                 литерала-инициализатора. Уберите аннотацию (оставляйте только когда \
                 она направляет коэрсию/сужение — например `[]u8`/`u32`).",
                name = d.name,
                t = default_name
            ),
            d.span,
        ),
    });
}

fn conv_redundant_const_type_annotation(
    m: &Module,
    _o: &ConvLintOptions,
    out: &mut Vec<LintWarning>,
) {
    let call_receiver_names = conv_collect_call_receiver_names(m);
    for item in &m.items {
        if let Item::Const(d) = item {
            conv_check_const_redundant_annotation(d, &call_receiver_names, out);
        }
    }
    for pf in &m.peer_files {
        for item in &pf.items_here {
            if let Item::Const(d) = item {
                conv_check_const_redundant_annotation(d, &call_receiver_names, out);
            }
        }
    }
    fn check_stmt(s: &Stmt, names: &HashSet<String>, out: &mut Vec<LintWarning>) {
        if let Stmt::Const(d) = s {
            conv_check_const_redundant_annotation(d, names, out);
        }
    }
    for f in conv_all_fns(m) {
        conv_walk_fn(f, &mut |s, _| check_stmt(s, &call_receiver_names, out), &mut |_, _| {});
    }
    for tb in conv_all_test_bodies(m) {
        conv_walk_block(
            tb,
            false,
            &mut |s, _| check_stmt(s, &call_receiver_names, out),
            &mut |_, _| {},
        );
    }
}

// ---------------------------------------------------------------------------
// W_PARAM_NO_CONTRACT — index/offset/len-параметр публичной std-fn без
// `requires` (nv-coding-style §5): каждый такой параметр ОБЯЗАН нести
// контракт — норма приёмки (согласовано 2026-07-07). Только в std (in_std).
// ---------------------------------------------------------------------------

const CONV_CONTRACT_PARAM_NAMES: &[&str] = &[
    "i", "idx", "pos", "offset", "off", "len", "n", "cap", "count", "start", "end", "limit",
];

fn conv_param_no_contract(m: &Module, o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    if !o.in_std {
        return;
    }
    for f in conv_all_fns(m) {
        if !f.is_export || f.is_external || matches!(f.body, FnBody::External) {
            continue;
        }
        // Fn, возвращающая Option/Result, обрабатывает граничные значения
        // самим типом возврата (`@get(i) -> Option[T]`: отрицательный индекс
        // → None) — контракт там опционален, не норма приёмки.
        if f.return_type.as_ref().and_then(conv_ty_last_name)
            .map_or(false, |n| n == "Option" || n == "Result")
        {
            continue;
        }
        // Имена, упомянутые в requires-контрактах.
        let mut required: HashSet<String> = HashSet::new();
        for c in &f.contracts {
            if c.kind == crate::ast::ContractKind::Requires {
                collect_expr(&c.expr, &mut required);
            }
        }
        for p in &f.params {
            if p.is_variadic || !conv_ty_is_int(&p.ty) {
                continue;
            }
            if !CONV_CONTRACT_PARAM_NAMES.contains(&p.name.as_str()) {
                continue;
            }
            if !required.contains(&p.name) {
                out.push(LintWarning {
                    rule: "W_PARAM_NO_CONTRACT",
                    diag: Diagnostic::new(
                        format!(
                            "index/offset/len-параметр `{}` публичной std-fn `{}` без \
                             `requires`: каждый такой параметр обязан нести контракт \
                             (nv-coding-style §5, норма приёмки 2026-07-07). Доказанный \
                             `requires` — zero-cost (Z3 элидирует на литеральных \
                             аргументах).",
                            p.name, f.name
                        ),
                        p.span,
                    ),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// W_VEC_SPELLING (текст) — `Vec[` вне std/collections/vec (D238/D239):
// за пределами definition-site пишется `[]T`. Строки с маркером `[M-...]`
// (известные compiler-gap исключения) не флагуются.
// ---------------------------------------------------------------------------

fn conv_vec_spelling(src: &str, o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    // Definition-site и тесты: в тестах канон — `Vec[T].of(a, b, c)`
    // (решение владельца; вариадик-конструктор требует номинала).
    if o.in_vec_module || o.in_test {
        return;
    }
    conv_each_code_line(src, |off, full_line, code| {
        if full_line.contains("[M-") {
            return; // легальное исключение с маркером на строке
        }
        let mut from = 0usize;
        while let Some(i) = code[from..].find("Vec[") {
            let at = from + i;
            // Не часть более длинного идентификатора (MyVec[...]).
            let prev_ok = at == 0
                || !code.as_bytes()[at - 1].is_ascii_alphanumeric()
                    && code.as_bytes()[at - 1] != b'_';
            if prev_ok {
                out.push(LintWarning {
                    rule: "W_VEC_SPELLING",
                    diag: Diagnostic::new(
                        "`Vec[...]` вне std/collections/vec — definition-site-only \
                         спеллинг (D238/D239): за пределами vec-модуля пишите `[]T`. \
                         Известные compiler-gap исключения — с комментарием-маркером \
                         `[M-...]` на строке."
                            .to_string(),
                        Span::new(off + at, off + at + 4),
                    ),
                });
            }
            from = at + 4;
        }
    });
}

// ---------------------------------------------------------------------------
// W_RETIRED_NAME (текст) — вызовы ретрактированных API (греп-инварианты
// `= 0` из D-блоков): nth / to_bytes / to_chars / .into() / with_capacity /
// from_raw_parts.
// ---------------------------------------------------------------------------

const CONV_RETIRED_PATTERNS: &[(&str, &str)] = &[
    (".nth(", "`nth` ретрактирован — `.iter().skip(n)` / индекс `[n]`"),
    (".to_bytes(", "`to_bytes` ретрактирован (D410) — `bytes().clone()`"),
    (".to_chars(", "`to_chars` ретрактирован (D410) — `chars().collect()`"),
    (".into()", "голый `.into()` ретрактирован (D73 retraction) — явный `to_*`/`into_*`"),
    (".with_capacity(", "`with_capacity` ретрактирован (D372 amend + 200 П4) — `.new(cap: n)`"),
    (".from_raw_parts(", "`from_raw_parts` ретрактирован — типизированные конструкторы"),
];

fn conv_retired_name(src: &str, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    conv_each_code_line(src, |off, _full, code| {
        for (pat, note) in CONV_RETIRED_PATTERNS {
            let mut from = 0usize;
            while let Some(i) = code[from..].find(pat) {
                let at = from + i;
                out.push(LintWarning {
                    rule: "W_RETIRED_NAME",
                    diag: Diagnostic::new(
                        format!("ретрактированный вызов: {}.", note),
                        Span::new(off + at, off + at + pat.len()),
                    ),
                });
                from = at + pat.len();
            }
        }
    });
}

// ---------------------------------------------------------------------------
// W_FAIL_PUBLIC_SIGNATURE (текст, только std) — `Fail[...]` в публичной
// std-сигнатуре собственных ошибок (R5 D325): канон `Result[T, XError]`,
// throw = `!!` на Result-форме. Дополняет conformance-guard.
// ---------------------------------------------------------------------------

fn conv_fail_public_signature(src: &str, o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    if !o.in_std {
        return;
    }
    conv_each_code_line(src, |off, full_line, code| {
        if full_line.contains("[M-") {
            return;
        }
        let t = code.trim_start();
        if !t.starts_with("export fn ") {
            return;
        }
        if let Some(i) = code.find("Fail[") {
            // Generic re-throw `Fail[E]` (одна заглавная буква = тип-параметр)
            // — легальный комбинатор-паттерн (retry/execute), не собственная
            // ошибка std. R5 — про конкретные XError-типы.
            let inner: String = code[i + 5..]
                .chars()
                .take_while(|c| *c != ']')
                .collect();
            let inner = inner.trim();
            if inner.len() == 1 && inner.chars().all(|c| c.is_ascii_uppercase()) {
                return;
            }
            out.push(LintWarning {
                rule: "W_FAIL_PUBLIC_SIGNATURE",
                diag: Diagnostic::new(
                    "`Fail[...]` в публичной std-сигнатуре: собственные ошибки std \
                     наружу — `Result[T, XError]` (R5 D325); `Fail`-эффект наружу не \
                     отдаём (throw = `!!` на Result-форме)."
                        .to_string(),
                    Span::new(off + i, off + i + 5),
                ),
            });
        }
    });
}

// ---------------------------------------------------------------------------
// W_DESTRUCTURE_SNAPSHOT — 2+ подряд идущих `ro`/`mut`-биндинга вида
// `x = <тот же ident>.x` (nv-coding-style §26, D411 канон): стилевой дрейф —
// источник читается по частям вручную там, где D411 record-деструктуризация
// (`ro {status, headers, ..} = resp`) даёт то же за один биндинг.
//
// Консервативно:
//  - только точное имя-поле (биндинг-паттерн `Ident`, значение — `Member`
//    БЕЗ обёртки `Call`, т.е. поле, не метод — `resp.status()` не матчит,
//    т.к. это `Call{func: Member}`, а не голый `Member`);
//  - shorthand-совпадение: имя биндинга == имя поля (`ro rx = p.x` не матчит —
//    это переименование, не снапшот-дрейф);
//  - источник — простой идентификатор (`obj` = `Ident`), не выражение;
//  - оба биндинга в паре — ОДНОЙ мутабельности (`ro`+`ro` либо `mut`+`mut`):
//    смешанная пара не сворачивается в один D411-биндинг (деструктуризация
//    даёт всем полям одну мутабельность), сообщать о ней как о том же
//    дрейфе было бы вводящей в заблуждение рекомендацией;
//  - строго СОСЕДНИЕ `Stmt::Let` в одном `Block` (никакой другой statement
//    между ними — иначе группа разрывается).
// ---------------------------------------------------------------------------

/// Возвращает `(источник, поле)`, если `d` — биндинг-снапшот поля вида
/// `x = <ident>.x` (имя биндинга == имя поля).
fn conv_field_snapshot(d: &crate::ast::LetDecl) -> Option<(&str, &str)> {
    let Pattern::Ident { name, .. } = &d.pattern else { return None };
    let ExprKind::Member { obj, name: field } = &d.value.kind else { return None };
    let ExprKind::Ident(src) = &obj.kind else { return None };
    if field != name {
        return None;
    }
    Some((src.as_str(), field.as_str()))
}

/// Сканирует непосредственные stmt'ы одного блока на серии из 2+ соседних
/// field-snapshot биндингов одного источника и одной мутабельности.
fn conv_scan_stmts_for_destructure(stmts: &[Stmt], out: &mut Vec<LintWarning>) {
    let mut i = 0;
    while i < stmts.len() {
        let Stmt::Let(d0) = &stmts[i] else { i += 1; continue };
        let Some((src0, _)) = conv_field_snapshot(d0) else { i += 1; continue };
        let mut j = i + 1;
        while j < stmts.len() {
            let Stmt::Let(dj) = &stmts[j] else { break };
            let Some((srcj, _)) = conv_field_snapshot(dj) else { break };
            if srcj != src0 || dj.mutable != d0.mutable {
                break;
            }
            j += 1;
        }
        let run_len = j - i;
        if run_len >= 2 {
            let last = match &stmts[j - 1] {
                Stmt::Let(dl) => dl.span,
                _ => d0.span,
            };
            let kw = if d0.mutable { "mut" } else { "ro" };
            out.push(LintWarning {
                rule: "W_DESTRUCTURE_SNAPSHOT",
                diag: Diagnostic::new(
                    format!(
                        "{} подряд идущих `{}`-биндинга снимают отдельные поля с \
                         одного источника `{}` (`x = {}.x`) — стилевой дрейф \
                         (nv-coding-style §26). Канон — D411 record-деструктуризация \
                         одним биндингом: `{} {{ .., .. }} = {}`.",
                        run_len, kw, src0, src0, kw, src0
                    ),
                    Span::new(d0.span.start, last.end),
                ),
            });
            i = j;
            continue;
        }
        i += 1;
    }
}

fn conv_walk_block_for_destructure(b: &Block, out: &mut Vec<LintWarning>) {
    conv_scan_stmts_for_destructure(&b.stmts, out);
    for s in &b.stmts {
        conv_walk_stmt_for_destructure(s, out);
    }
    if let Some(t) = &b.trailing {
        conv_walk_expr_for_destructure(t, out);
    }
}

fn conv_walk_stmt_for_destructure(s: &Stmt, out: &mut Vec<LintWarning>) {
    match s {
        Stmt::Let(d) => conv_walk_expr_for_destructure(&d.value, out),
        Stmt::Const(d) => conv_walk_expr_for_destructure(&d.value, out),
        Stmt::Expr(e) => conv_walk_expr_for_destructure(e, out),
        Stmt::Assign { target, value, .. } => {
            conv_walk_expr_for_destructure(target, out);
            conv_walk_expr_for_destructure(value, out);
        }
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs {
                conv_walk_expr_for_destructure(e, out);
            }
            for e in rhs {
                conv_walk_expr_for_destructure(e, out);
            }
        }
        Stmt::Return { value: Some(v), .. } => conv_walk_expr_for_destructure(v, out),
        Stmt::Throw { value, .. } => conv_walk_expr_for_destructure(value, out),
        Stmt::Defer { body, .. } => conv_walk_expr_for_destructure(body, out),
        Stmt::ConsumeScope { init, body, .. } => {
            conv_walk_expr_for_destructure(init, out);
            conv_walk_block_for_destructure(body, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            conv_walk_expr_for_destructure(expr, out);
        }
        _ => {}
    }
}

fn conv_walk_expr_for_destructure(e: &Expr, out: &mut Vec<LintWarning>) {
    match &e.kind {
        ExprKind::If { cond, then, else_ } => {
            conv_walk_expr_for_destructure(cond, out);
            conv_walk_block_for_destructure(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => conv_walk_block_for_destructure(b, out),
                    ElseBranch::If(ie) => conv_walk_expr_for_destructure(ie, out),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            conv_walk_expr_for_destructure(scrutinee, out);
            conv_walk_block_for_destructure(then, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => conv_walk_block_for_destructure(b, out),
                    ElseBranch::If(ie) => conv_walk_expr_for_destructure(ie, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            conv_walk_expr_for_destructure(scrutinee, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    conv_walk_expr_for_destructure(g, out);
                }
                match &arm.body {
                    MatchArmBody::Expr(e) => conv_walk_expr_for_destructure(e, out),
                    MatchArmBody::Block(b) => conv_walk_block_for_destructure(b, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            conv_walk_expr_for_destructure(iter, out);
            conv_walk_block_for_destructure(body, out);
        }
        ExprKind::While { cond, body, .. } => {
            conv_walk_expr_for_destructure(cond, out);
            conv_walk_block_for_destructure(body, out);
        }
        ExprKind::WhileLet { scrutinee, guard, body, .. } => {
            conv_walk_expr_for_destructure(scrutinee, out);
            if let Some(g) = guard {
                conv_walk_expr_for_destructure(g, out);
            }
            conv_walk_block_for_destructure(body, out);
        }
        ExprKind::Loop { body, .. } => conv_walk_block_for_destructure(body, out),
        ExprKind::Block(b) => conv_walk_block_for_destructure(b, out),
        ExprKind::Call { func, args, trailing } => {
            conv_walk_expr_for_destructure(func, out);
            for a in args {
                conv_walk_expr_for_destructure(a.expr(), out);
            }
            if let Some(t) = trailing {
                match t {
                    crate::ast::Trailing::Block(b) => conv_walk_block_for_destructure(b, out),
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                        conv_walk_block_for_destructure(&tb.body, out)
                    }
                    crate::ast::Trailing::Fn(sb) => match &sb.body {
                        FnBody::Expr(e) => conv_walk_expr_for_destructure(e, out),
                        FnBody::Block(b) => conv_walk_block_for_destructure(b, out),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Expr(e) => conv_walk_expr_for_destructure(e, out),
            FnBody::Block(b) => conv_walk_block_for_destructure(b, out),
            FnBody::External => {}
        },
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e) => conv_walk_expr_for_destructure(e, out),
            ClosureBody::Block(b) => conv_walk_block_for_destructure(b, out),
        },
        ExprKind::Lambda { body, .. } => conv_walk_expr_for_destructure(body, out),
        ExprKind::Spawn(x) | ExprKind::Throw(x) => conv_walk_expr_for_destructure(x, out),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => conv_walk_block_for_destructure(b, out),
        ExprKind::Supervised { body, cancel, deadline } => {
            if let Some(c) = cancel {
                conv_walk_expr_for_destructure(c, out);
            }
            if let Some(dl) = deadline {
                conv_walk_expr_for_destructure(&dl.expr, out);
            }
            conv_walk_block_for_destructure(body, out);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            conv_walk_block_for_destructure(body, out)
        }
        _ => {}
    }
}

fn conv_destructure_snapshot(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        match &f.body {
            FnBody::Expr(_) => {}
            FnBody::Block(b) => conv_walk_block_for_destructure(b, out),
            FnBody::External => {}
        }
    }
}

// ---------------------------------------------------------------------------
// W_LEADING_BINOP_CONTINUATION (текст) — ведущий бинарный оператор в начале
// continuation-строки многострочного выражения.
//
// Причина (D417-класс, `E_CLOSURE_SCALAR_RETURN`, types/mod.rs
// `check_closure_scalar_return`): парсер НЕ продолжает выражение через
// ведущий `||`/`&&`/… на СЛЕДУЮЩЕЙ строке — если предыдущая строка уже сама
// по себе полна (statement-sequence внутри `{ }`-блока), она завершает
// statement, а строка с ведущим `||` парсится как ОТДЕЛЬНЫЙ zero-arg
// closure-литерал (discarded statement). Для `||` это тихо даёт always-true
// в скалярном возврате (реальный баг needs_quoting/is_ascii_ident_char,
// 2026-07-10). Этот линт ловит ПРИЧИНУ (стиль) раньше и во ВСЕХ контекстах
// (не только скалярный return, который покрывает только
// `E_CLOSURE_SCALAR_RETURN`).
//
// Эвристика (греп-уровня, не полный парс): строка-продолжение начинается
// (после отступа) с бинарного оператора, И ближайшая предыдущая непустая
// код-строка выглядит как самостоятельно завершённый statement.
//
// Калибровка против ложных срабатываний (§7.3, эмпирически на std +
// nova_tests, 2026-07-10) вскрыла НЕСКОЛЬКО реальных safe-паттернов — каждый
// закрыт отдельным гейтом:
//
//   1. **Ограниченный parse_expr()-вызов безопасен ВСЕГДА** (`if`/`while`/
//      `for`/`match`-условие ДО открывающей `{`, arrow-body `fn ... =>
//      EXPR`, содержимое `(...)`/`[...]`-списка) — Pratt-парсер не
//      завершает выражение на границе строки там, где НЕТ statement-
//      sequence. Подтверждено: `std/data/semver.nv` (arrow-body), `std/
//      path/glob.nv` (if-условие без скобок), `nova_tests/map_literals/
//      positive_siphash_smoke.nv` (`-1: "neg",` внутри многострочного
//      map-литерала `[...]`) — все реально компилируются и проходят тесты.
//      Гейты: `brace_depth >= 1` (cumulative-счётчик ТОЛЬКО `{`/`}` —
//      arrow-body верхнего уровня модуля имеет brace_depth == 0);
//      `par_depth == 0` (cumulative-счётчик ТОЛЬКО `(`/`)`/`[`/`]` — внутри
//      открытого списка/вызова всегда 0 не бывает); `awaiting_brace`-
//      состояние (от строки-начала statement'а с `if`/`while`/`for`/
//      `match`/`else` до появления `{`).
//   2. **`} else {` (и любой close+reopen НА ОДНОЙ строке) — НЕ «предыдущий
//      statement завершён»**: net-delta скобок этой строки — 0 (совпадает с
//      «завершённый statement»), но реально строка ЗАКРЫВАЕТ один блок и
//      ОТКРЫВАЕТ новый — следующая строка ПЕРВЫЙ statement НОВОГО блока
//      (напр. `-1` как отрицательный литерал — не continuation).
//      Подтверждено: `nova_tests/plan106/guard_ok_basic.nv`,
//      `nova_tests/concurrency/blocking_test.nv` (`} else { \n -1 \n }`).
//      Гейт: `last_brace_open` — последний `{`/`}`-символ ПРЕДЫДУЩЕЙ
//      строки; если это `{` (не `}`) — строка «похожа на завершённую» НЕ
//      считается (даже при net-delta == 0).
//   3. **Унарный префикс — не бинарный оператор.** `-`/`+`/`*` в Nova имеют
//      ПОВСЕДНЕВНЫЕ унарные прочтения на месте начала statement'а:
//      отрицательный/положительный числовой литерал (`-1`, частый trailing-
//      результат fn/if-ветки) и разыменование сырого указателя (`*p = v`,
//      Plan 118 unsafe/raw-pointers — очень частый паттерн в
//      `nova_tests/plan118*`/`plan147`). В отличие от `||`/`&&`/сравнений,
//      у этих трёх токенов НЕТ надёжного «это бинарное продолжение»
//      прочтения без полного парса — **исключены** из набора детектируемых
//      операторов (иначе шквал ложных находок на легитимный код).
//   4. **`calc { ... }`-блок (Plan 33.5, equational reasoning) КАНОНИЧЕСКИ
//      использует ведущие `==`/`<`/… НА КАЖДОЙ строке** (`x * 2;` \n `== x *
//      2;`) — это НЕ баг, это спроектированный синтаксис доказательного
//      блока. Подтверждено: `nova_tests/contracts/calc_basic_positive.nv`.
//      Гейт: весь `calc { ... }`-блок (по глубине `{}` от точки открытия до
//      возврата на тот же уровень) — вне проверки целиком.
//   5. **Backtick tagged-template литералы (`` html`...` ``) — их
//      МНОГОСТРОЧНОЕ содержимое НЕ код** (HTML/произвольный текст внутри),
//      может начинаться с `<...>`. Подтверждено:
//      `nova_tests/types/alias_tagged.nv` (`html\`<html>...\``). Гейт:
//      backtick-состояние (`` ` ``…`` ` ``) трактуется как непрозрачная
//      строка, персистентная между строками (аналог блок-комментария).
//
// Известное ограничение: НЕ ловит случай, когда предыдущий statement — это
// блок, закрывающийся ОДНОЙ строкой `}` (this последняя строка сама по себе
// НЕ «завершённый statement» в терминах этой эвристики) — осознанный
// компромисс простоты (мотивирующий пример ловится точно: `(a && b)` \n
// `|| (c && d)`).
// ---------------------------------------------------------------------------

/// Бинарные операторы-кандидаты. Длинные формы — ПЕРЕД короткими (порядок
/// имеет значение для `starts_with`-матчинга). `+`/`-`/`*` сознательно
/// ИСКЛЮЧЕНЫ — см. п.3 в комментарии выше (унарный литерал-знак / raw-
/// pointer deref — слишком частые легитимные прочтения).
const CONV_LEADING_BINOPS: &[&str] = &[
    "||", "&&", "==", "!=", "<=", ">=", "<<", ">>", "/", "%", "<", ">",
];

/// Управляющие слова, открывающие condition-span, длящийся ДО ближайшей `{`
/// (Nova `if`/`while`/`for`/`match`/`else` не требуют скобок вокруг условия).
const CONV_COND_KEYWORDS: &[&str] = &["if", "while", "for", "match", "else"];

/// Если `trimmed` начинается с одного из `CONV_LEADING_BINOPS` (и это не
/// стрелка `->`/`=>` и не хвост блок-комментария `*/`) — возвращает токен.
fn conv_match_leading_binop(trimmed: &str) -> Option<&'static str> {
    if trimmed.starts_with("->") || trimmed.starts_with("=>") || trimmed.starts_with("*/") {
        return None;
    }
    for op in CONV_LEADING_BINOPS {
        if trimmed.starts_with(op) {
            return Some(op);
        }
    }
    None
}

/// `true`, если строка-начало statement'а — управляющее слово
/// (`if`/`while`/`for`/`match`/`else`), за которым следует word-boundary
/// (пробел / конец строки / `(`), т.е. это не префикс идентификатора
/// (`ifoo`).
fn conv_starts_cond_keyword(trimmed: &str) -> bool {
    for kw in CONV_COND_KEYWORDS {
        if let Some(rest) = trimmed.strip_prefix(kw) {
            if rest.is_empty() || !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                return true;
            }
        }
    }
    false
}

/// `true`, если строка-начало statement'а — `calc` (proof-блок, Plan 33.5),
/// за которым следует word-boundary.
fn conv_starts_calc_keyword(trimmed: &str) -> bool {
    if let Some(rest) = trimmed.strip_prefix("calc") {
        return rest.is_empty() || !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_');
    }
    false
}

/// Результат построчного скана: net-delta ВСЕХ скобок (круглые+квадратные+
/// фигурные), net-delta ТОЛЬКО фигурных, последний фигурный символ строки
/// (`Some(true)` = `{`, `Some(false)` = `}`, `None` = фигурных не было),
/// есть ли в строке код вне комментария/строки/backtick-литерала.
struct ConvLineScan {
    delta: i32,
    brace_delta: i32,
    last_brace_open: Option<bool>,
    has_code: bool,
}

/// Сканирует ОДНУ строку исходника, обновляя межстрочное состояние
/// `in_block_comment` и `in_backtick` (backtick tagged-template литералы —
/// `` `...` `` — многострочны, содержимое непрозрачно, п.5 выше).
fn conv_scan_line_delta(line: &str, in_block_comment: &mut bool, in_backtick: &mut bool) -> ConvLineScan {
    let mut delta = 0i32;
    let mut brace_delta = 0i32;
    let mut last_brace_open: Option<bool> = None;
    let mut has_code = false;
    let mut in_string = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if *in_backtick {
            if ch == '\\' {
                chars.next();
                continue;
            }
            if ch == '`' {
                *in_backtick = false;
            }
            continue;
        }
        if *in_block_comment {
            if ch == '*' && line[idx..].starts_with("*/") {
                *in_block_comment = false;
                chars.next();
            }
            continue;
        }
        if in_string {
            if ch == '\\' {
                chars.next();
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '/' && line[idx..].starts_with("//") {
            break;
        }
        if ch == '/' && line[idx..].starts_with("/*") {
            *in_block_comment = true;
            chars.next();
            continue;
        }
        if ch == '`' {
            *in_backtick = true;
            has_code = true;
            continue;
        }
        if ch == '"' {
            in_string = true;
            has_code = true;
            continue;
        }
        if ch.is_whitespace() {
            continue;
        }
        has_code = true;
        match ch {
            '(' | '[' | '{' => delta += 1,
            ')' | ']' | '}' => delta -= 1,
            _ => {}
        }
        if ch == '{' {
            brace_delta += 1;
            last_brace_open = Some(true);
        } else if ch == '}' {
            brace_delta -= 1;
            last_brace_open = Some(false);
        }
    }
    ConvLineScan { delta, brace_delta, last_brace_open, has_code }
}

fn conv_leading_binop_continuation(src: &str, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    let mut off = 0usize;
    let mut in_block_comment = false;
    let mut in_backtick = false;
    let mut prev_delta: Option<i32> = None;
    let mut prev_brace_delta: i32 = 0;
    let mut prev_last_brace_open: Option<bool> = None;
    let mut prev_is_decl = false;
    let mut brace_depth: i32 = 0;
    let mut par_depth: i32 = 0;
    let mut awaiting_brace = false;
    let mut calc_base_depth: Option<i32> = None;

    for raw_line in src.split_inclusive('\n') {
        let line_off = off;
        off += raw_line.len();
        let line = raw_line.trim_end_matches(['\n', '\r']);

        let was_in_block_comment = in_block_comment;
        let scan = conv_scan_line_delta(line, &mut in_block_comment, &mut in_backtick);
        let ConvLineScan { delta, brace_delta, last_brace_open, has_code } = scan;
        let par_delta = delta - brace_delta;

        if !has_code {
            continue;
        }

        if !was_in_block_comment {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("//") && !trimmed.starts_with("/*") {
                let in_calc_block = calc_base_depth.is_some_and(|d| brace_depth >= d);
                if !awaiting_brace && !in_calc_block && brace_depth >= 1 && par_depth == 0 {
                    if let Some(op) = conv_match_leading_binop(trimmed) {
                        // `||` уникален: это ЕДИНСТВЕННЫЙ оператор набора,
                        // у которого ЕСТЬ легитимное альтернативное чтение
                        // как ПЕРВЫЙ токен statement'а — zero-arg closure-
                        // литерал, возвращаемый как trailing-значение блока
                        // (`fn() -> fn() -> int { ... \n || { ... } }`,
                        // прецедент калибровки
                        // `nova_tests/syntax/closure_mut_capture_escape.nv:18`
                        // — реальный, ПРАВИЛЬНЫЙ код). Когда предыдущая
                        // строка — ДЕКЛАРАЦИЯ (`mut`/`ro`/`const`), нет связной
                        // истории «continuation одного выражения» (декларация
                        // не производит операнд для объединения) — не
                        // флагуем именно `||` в этом сочетании. Другие
                        // операторы (`&&`/сравнения/…) НЕ имеют такого
                        // легитимного альтернативного чтения — для них
                        // декларация-предшественник ПРОДОЛЖАЕТ считаться
                        // подозрительной (реальный риск: `ro valid =
                        // check_a(x)` \n `&& check_b(x)` — забытый trailing).
                        let prev_looks_complete = prev_delta == Some(0)
                            && prev_last_brace_open != Some(true)
                            && !(op == "||" && prev_is_decl);
                        if prev_looks_complete {
                            let indent = line.len() - trimmed.len();
                            let start = line_off + indent;
                            out.push(LintWarning {
                                rule: "W_LEADING_BINOP_CONTINUATION",
                                diag: Diagnostic::new(
                                    format!(
                                        "ведущий бинарный оператор `{op}` в начале continuation-строки: \
                                         предыдущая строка сбалансирована по скобкам и похожа на \
                                         завершённый statement — парсер, скорее всего, НЕ продолжит его \
                                         этим оператором (ведущий `||` парсится как отдельный zero-arg \
                                         closure-литерал → discarded statement → возможен always-true, \
                                         D417-класс). Канон — TRAILING-форма: перенесите `{op}` в конец \
                                         ПРЕДЫДУЩЕЙ строки (nv-coding-style §27)."
                                    ),
                                    Span::new(start, start + op.len()),
                                ),
                            });
                        }
                    }
                }
                // Обновление awaiting_brace: строка-начало statement'а
                // начинается с управляющего слова — входим в condition-span,
                // длящийся до появления `{` (на этой ЖЕ или последующей
                // строке). «Начало statement'а» здесь ШИРЕ, чем гейт
                // предупреждения выше: сюда попадает И «предыдущий statement
                // сбалансирован» (prev_delta == Some(0), не после close+
                // reopen), И «предыдущая строка только что ОТКРЫЛА новый
                // блок» (prev_brace_delta > 0) — т.е. текущая строка ПЕРВЫЙ
                // statement внутри только что открытого блока (напр.
                // вложенный `if` сразу после `while cond {` / `match x {
                // Arm => {` — без этого расширения `if`-заголовок первого
                // statement'а блока не распознаётся, ложное срабатывание на
                // его continuation-строках, см. std/path/glob.nv-прецедент).
                let at_fresh_stmt = (prev_delta == Some(0) && prev_last_brace_open != Some(true))
                    || prev_brace_delta > 0;
                if !awaiting_brace && at_fresh_stmt && conv_starts_cond_keyword(trimmed) {
                    awaiting_brace = true;
                }
                if brace_delta > 0 {
                    awaiting_brace = false;
                }
                // `calc { ... }` (п.4): весь блок — вне проверки, до
                // возврата brace_depth на уровень, на котором calc открылся.
                // Требуем инлайновую `{` на ТОЙ ЖЕ строке (`calc {`,
                // единственная наблюдаемая форма) — `calc` без `{` на этой
                // же строке не распознаётся (редкий/гипотетический случай,
                // осознанный компромисс простоты).
                if calc_base_depth.is_none()
                    && at_fresh_stmt
                    && conv_starts_calc_keyword(trimmed)
                    && brace_delta > 0
                {
                    calc_base_depth = Some(brace_depth + brace_delta);
                }
            }
        }

        brace_depth += brace_delta;
        if calc_base_depth.is_some_and(|d| brace_depth < d) {
            calc_base_depth = None;
        }
        par_depth += par_delta;
        prev_is_decl = !was_in_block_comment && {
            let t = line.trim_start();
            t.starts_with("mut ") || t.starts_with("ro ") || t.starts_with("const ")
        };
        prev_delta = Some(delta);
        prev_brace_delta = brace_delta;
        prev_last_brace_open = last_brace_open;
    }
}

// ---------------------------------------------------------------------------
// W_REDUNDANT_OF — `Vec[T].of(...)`, когда литерал `[...]` дал бы ТОТ ЖЕ тип
// (nv-coding-style §28: канон конструирования коллекций, · согласовано
// 2026-07-10). `.of` оправдан только когда несёт информацию, которой нет в
// литерале (фиксация ширины `u32`/`i64`/…, `None`-элементы, пустая граница
// API); в остальных случаях — избыточная упаковка.
//
// SEMANTIC-UPGRADE: без типов не можем в общем случае доказать «литерал дал
// бы тот же элемент-тип» для произвольного выражения-аргумента. V1 —
// КОНСЕРВАТИВНЫЙ подкласс: `T` буквально совпадает с default-типом, который
// литерал даёт голому примитивному литералу (`int`/`str`/`bool`/`char`), И
// КАЖДЫЙ аргумент — однозначный литерал ИМЕННО этого типа (не идентификатор,
// не `None`, не вызов, без сужения). Это исключает ложные срабатывания на
// `Vec[u32].of(1,2,3)` (сужение — generics.is_empty()/имя не совпадёт),
// `Vec[Option[int]].of(None)` (generics непусты — не наш default-набор),
// `Vec[T].of()` (пусто — args.is_empty() гейт), `Vec[[]u8].of(x.bytes())`
// (elem-тип — Array, не Named; аргумент не литерал) и любой non-literal
// аргумент (переменная/вызов/property — тип не выводится синтаксически).
// Остальные истинно-избыточные случаи (не примитивный литерал) — вне V1,
// маркер расширения на будущее.
// ---------------------------------------------------------------------------

fn conv_redundant_of(m: &Module, _o: &ConvLintOptions, out: &mut Vec<LintWarning>) {
    for f in conv_all_fns(m) {
        conv_walk_fn(
            f,
            &mut |_s, _| {},
            &mut |e, _in_loop| {
                let ExprKind::Call { func, args, trailing } = &e.kind else { return };
                if trailing.is_some() || args.is_empty() {
                    return;
                }
                let ExprKind::Member { obj, name } = &func.kind else { return };
                if name != "of" {
                    return;
                }
                let ExprKind::TurboFish { base, type_args } = &obj.kind else { return };
                let is_vec = match &base.kind {
                    ExprKind::Ident(n) => n == "Vec",
                    ExprKind::Path(p) => p.last().map(String::as_str) == Some("Vec"),
                    _ => false,
                };
                if !is_vec || type_args.len() != 1 {
                    return;
                }
                // Только позиционные Item-аргументы — spread/именованные меняют
                // семантику (не эквивалент простого литерала), вне V1.
                if !args.iter().all(|a| matches!(a, CallArg::Item(_))) {
                    return;
                }
                let Some(default_name) = conv_ty_literal_default_name(&type_args[0]) else {
                    return;
                };
                let all_match_default = args.iter().all(|a| {
                    conv_expr_is_bare_literal_of(a.expr(), default_name)
                });
                if !all_match_default {
                    return;
                }
                out.push(LintWarning {
                    rule: "W_REDUNDANT_OF",
                    diag: Diagnostic::new(
                        format!(
                            "`Vec[{t}].of(...)` избыточен: аргументы — голые `{t}`-литералы, \
                             для которых литерал `[...]` дал бы ТОТ ЖЕ тип `Vec[{t}]` \
                             (nv-coding-style §28). Канон — `[...]`. `.of` оправдан только \
                             когда фиксирует тип, которого литерал не даёт (сужение ширины \
                             `u32`/`i64`/…, `None`-элементы, пустая граница API).",
                            t = default_name
                        ),
                        e.span,
                    ),
                });
            },
        );
    }
}

/// Имя default-типа, который литерал `[...]` даёт голому примитивному
/// литералу этого вида (`int`/`str`/`bool`/`char`) — только если `T` в
/// `Vec[T]` буквально этот бесгенериковый именованный тип (без сужения).
fn conv_ty_literal_default_name(tr: &TypeRef) -> Option<&'static str> {
    if let TypeRef::Named { path, generics, .. } = tr {
        if generics.is_empty() && path.len() == 1 {
            return match path[0].as_str() {
                "int" => Some("int"),
                "str" => Some("str"),
                "bool" => Some("bool"),
                "char" => Some("char"),
                _ => None,
            };
        }
    }
    None
}

/// `e` — однозначный голый литерал вида `kind` (`int`/`str`/`bool`/`char`),
/// без сужения/оборачивания. Для `int` допускает унарный `-`/`+` перед
/// `IntLit` (частый случай отрицательных констант), это остаётся тем же
/// default-типом литерала.
fn conv_expr_is_bare_literal_of(e: &Expr, kind: &str) -> bool {
    match kind {
        "int" => match &e.kind {
            ExprKind::IntLit(_) => true,
            ExprKind::Unary { op, operand } => {
                matches!(op, crate::ast::UnOp::Neg)
                    && matches!(operand.kind, ExprKind::IntLit(_))
            }
            _ => false,
        },
        "str" => matches!(e.kind, ExprKind::StrLit(_)),
        "bool" => matches!(e.kind, ExprKind::BoolLit(_)),
        "char" => matches!(e.kind, ExprKind::CharLit(_)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::Parser;

    fn parse(src: &str) -> Module {
        let toks = lex(src).unwrap();
        let mut p = Parser::new(toks);
        p.parse_module().unwrap()
    }

    #[test]
    fn warns_on_export_fail_untyped() {
        let m = parse("module foo\nexport fn parse(s str) Fail -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].rule, "export-fail-untyped");
    }

    #[test]
    fn no_warning_on_export_fail_typed() {
        let m = parse("module foo\nexport fn parse(s str) Fail[ParseError] -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    // [M-canon-mut-param-position] (2026-07-17): W_PARAM_TYPE_POS_MUT.

    #[test]
    fn warns_on_bare_postfix_mut_non_slice() {
        // `i mut int` — bare postfix legacy synonym of `mut i int`, non-slice
        // type — must warn.
        let m = parse("module foo\nfn bump(i mut int) {\n    i = i + 1\n}\n");
        let ws = lint_module(&m);
        assert!(
            ws.iter().any(|w| w.rule == "W_PARAM_TYPE_POS_MUT"),
            "ожидался W_PARAM_TYPE_POS_MUT, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_postfix_mut_slice() {
        // `buf mut []u8` — postfix mut on a SLICE type is the io-канон
        // exception (`buf mut []u8`) — must NOT warn.
        let m = parse("module foo\nfn fill(buf mut []u8) {\n    buf[0] = 1\n}\n");
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "W_PARAM_TYPE_POS_MUT"),
            "не должен fire на slice-типе, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_postfix_mut_fixed_array() {
        // `out mut [32]u8` — fixed-size byte array (hash-digest out-buffer,
        // real std/crypto pattern: sha256/md5/hmac/jwt/uuid_namespace) is
        // "родня" of the `[]T` slice exception — must NOT warn.
        let m = parse("module foo\nfn hash(out mut [32]u8) {\n    out[0] = 1 as u8\n}\n");
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "W_PARAM_TYPE_POS_MUT"),
            "не должен fire на fixed-size array, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_r2_split_ro_mut() {
        // `ro i mut int` — sanctioned D246 R2-split (explicit `ro` L1 +
        // postfix `mut` L2) — must NOT warn (exempt by construction: parser
        // never sets `mut_type_pos_legacy` when `ro` was explicit).
        let m = parse("module foo\nfn touch(ro i mut int) {\n    i = i + 1\n}\n");
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "W_PARAM_TYPE_POS_MUT"),
            "не должен fire на R2-split `ro x mut T`, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_canonical_prefix_mut() {
        // `mut i int` — canonical prefix form — must NOT warn.
        let m = parse("module foo\nfn bump(mut i int) {\n    i = i + 1\n}\n");
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "W_PARAM_TYPE_POS_MUT"),
            "не должен fire на канонической префиксной форме, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Plan 81 Ф.4: unused-import lint.

    #[test]
    fn warns_on_unused_selective_import() {
        let m = parse(
            "module foo\nimport bar.{Unused}\nfn run() -> int => 0\n",
        );
        let ws = lint_module(&m);
        assert!(ws.iter().any(|w| w.rule == "unused-import"));
    }

    #[test]
    fn no_warning_on_used_selective_import() {
        let m = parse(
            "module foo\nimport bar.{Helper}\nfn run() -> int => Helper()\n",
        );
        let ws = lint_module(&m);
        assert!(!ws.iter().any(|w| w.rule == "unused-import"));
    }

    #[test]
    fn no_warning_on_whole_module_import() {
        // Whole-module import не линтуется как unused-import — lint pass
        // не выполняет резолв имён. Ошибка E_IMPORT_GLOB эмитируется
        // type-checker'ом (Plan 163 Ф.2), не lint pass'ом.
        let m = parse("module foo\nimport bar\nfn run() -> int => 0\n");
        let ws = lint_module(&m);
        assert!(!ws.iter().any(|w| w.rule == "unused-import"));
    }

    #[test]
    fn used_import_in_type_position_not_flagged() {
        // Импортированный тип, использованный только в сигнатуре.
        let m = parse(
            "module foo\nimport bar.{Widget}\n\
             fn run(w Widget) -> int => 0\n",
        );
        let ws = lint_module(&m);
        assert!(!ws.iter().any(|w| w.rule == "unused-import"));
    }

    // [M-lint-phantom-prelude-unused-import] (владелец 2026-07-31, репро
    // nova-bigint/src/bigint.nv): `lint_unused_imports` итерировал ВЕСЬ
    // `m.peer_files` (после `resolve_imports_inline_ex` — это полный
    // транзитивный import-граф, инлайненный в один `Module`), а не только
    // entry-модуля собственные co-equal peer'ы (`pf.is_entry_module`) —
    // unused-import ЧУЖОГО транзитивно затянутого модуля (auto-prelude →
    // `std.collections.vec`/`hashmap`/`raw_mem`/…) вешался на проверяемый
    // файл как «фантомная» находка об имени, которое файл вообще не
    // импортирует.

    #[test]
    fn phantom_prelude_unused_import_not_flagged_on_foreign_peer() {
        // Синтетический repro-shape: entry-модуль `foo` без своих импортов
        // + "чужой" peer (module_name `std.collections.vec`, is_entry_module
        // = false) с ЕГО СОБСТВЕННЫМ неиспользуемым импортом — как если бы
        // резолвер инлайнил транзитивно затянутый `std/collections/vec/
        // core.nv` (auto-prelude → Vec) в общий `peer_files`. Импорт-гигиена
        // чужого модуля — не находка ЭТОГО файла.
        let entry_m = parse("module foo\nfn run() -> int => 0\n");
        let foreign_m = parse("module bar\nimport baz.{Unused}\nfn helper() -> int => 0\n");

        let mut m = entry_m;
        let entry_peer = crate::ast::PeerFile {
            path: std::path::PathBuf::from("/synthetic/entry.nv"),
            file_id: crate::diag::FileId::from(0_u32),
            imports: m.imports.clone(),
            items_here: m.items.clone(),
            imported_item_names: HashSet::new(),
            is_entry_module: true,
            module_name: m.name.clone(),
        };
        let foreign_peer = crate::ast::PeerFile {
            path: std::path::PathBuf::from("/synthetic/std/collections/vec/core.nv"),
            file_id: crate::diag::FileId::from(7_u32),
            imports: foreign_m.imports.clone(),
            items_here: foreign_m.items.clone(),
            imported_item_names: HashSet::new(),
            is_entry_module: false,
            module_name: vec!["std".to_string(), "collections".to_string(), "vec".to_string()],
        };
        m.peer_files = vec![entry_peer, foreign_peer];

        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "unused-import"),
            "foreign (non-entry) peer's own unused import must NOT surface as \
             a phantom finding on the checked file, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unused_import_still_fires_for_entry_own_peer_group() {
        // Companion-негатив: фикс не должен ослепить линт к РЕАЛЬНОМУ
        // unused-import в СОБСТВЕННОЙ peer-группе entry-модуля — исключаются
        // только чужие (non-entry) peer'ы.
        let entry_m = parse("module foo\nimport bar.{Unused}\nfn run() -> int => 0\n");
        let foreign_m = parse("module baz\nimport qux.{AlsoUnused}\nfn helper() -> int => 0\n");

        let mut m = entry_m;
        let entry_peer = crate::ast::PeerFile {
            path: std::path::PathBuf::from("/synthetic/entry.nv"),
            file_id: crate::diag::FileId::from(0_u32),
            imports: m.imports.clone(),
            items_here: m.items.clone(),
            imported_item_names: HashSet::new(),
            is_entry_module: true,
            module_name: m.name.clone(),
        };
        let foreign_peer = crate::ast::PeerFile {
            path: std::path::PathBuf::from("/synthetic/std/somewhere.nv"),
            file_id: crate::diag::FileId::from(9_u32),
            imports: foreign_m.imports.clone(),
            items_here: foreign_m.items.clone(),
            imported_item_names: HashSet::new(),
            is_entry_module: false,
            module_name: vec!["std".to_string(), "somewhere".to_string()],
        };
        m.peer_files = vec![entry_peer, foreign_peer];

        let ws = lint_module(&m);
        let hits: Vec<&LintWarning> = ws.iter().filter(|w| w.rule == "unused-import").collect();
        assert_eq!(
            hits.len(), 1,
            "exactly one unused-import (entry's own `Unused`), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
        assert!(
            hits[0].diag.message.contains("Unused") && !hits[0].diag.message.contains("AlsoUnused"),
            "should name entry's own unused import, not the foreign peer's, got: {}",
            hits[0].diag.message
        );
    }

    // Plan 33.8 Ф.3.3: `assume` вне `#trusted` → lint `trust-introduced`.
    #[test]
    fn warns_on_assume_outside_trusted() {
        let m = parse(
            "module foo\nfn risky(x int) -> int {\n    assume x >= 0\n    x + 1\n}\n",
        );
        let ws = lint_module(&m);
        assert!(
            ws.iter().any(|w| w.rule == "trust-introduced"),
            "ожидался trust-introduced warning, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Plan 33.8 Ф.3.3: `assume` внутри `#trusted` — без warning.
    #[test]
    fn no_warning_on_assume_in_trusted() {
        let m = parse(
            "module foo\n#trusted\nfn ffi(x int) -> int {\n    assume x >= 0\n    x + 1\n}\n",
        );
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "trust-introduced"),
            "trust-introduced не должен эмититься внутри #trusted"
        );
    }

    // Plan 33.8 Ф.6.3: `assert_static` → lint `assert-static-unverified`.
    #[test]
    fn warns_on_assert_static() {
        let m = parse(
            "module foo\nfn step(x int) -> int {\n    assert_static x >= 0\n    x + 1\n}\n",
        );
        let ws = lint_module(&m);
        assert!(
            ws.iter().any(|w| w.rule == "assert-static-unverified"),
            "ожидался assert-static-unverified, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_assert_static_warning_without_it() {
        let m = parse("module foo\nfn plain(x int) -> int {\n    x + 1\n}\n");
        let ws = lint_module(&m);
        assert!(!ws.iter().any(|w| w.rule == "assert-static-unverified"));
    }

    #[test]
    fn no_warning_on_export_fail_any() {
        // Fail[any] — explicit erasure, программист opt-in
        let m = parse("module foo\nexport fn dump() Fail[any] -> () => ()\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    #[test]
    fn no_warning_on_private_fail() {
        // Private fn — Fail без E это inference placeholder, OK
        let m = parse("module foo\nfn parse(s str) Fail -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    #[test]
    fn warns_on_protocol_in_effect_position() {
        // `Eq` — protocol (hardcoded back-compat alias в
        // `collect_protocol_names`); в effect-position → warning.
        // Раньше тест использовал `Hashable` — после Plan 62.E он
        // мигрирован в prelude и распознаётся только после import-merge,
        // которого bare `parse` не делает (stale test, чинится здесь).
        let m = parse("module foo\nfn process(x int) Eq -> int => x\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].rule, "protocol-in-effect-position");
    }

    #[test]
    fn no_warning_on_effect_in_effect_position() {
        // Db — effect, OK в effect-position.
        let m = parse("module foo\nfn lookup(id int) Db -> int => id\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    /// Plan 62.F.bis Ф.2: tests для `lint_prelude_shadow` + suppress.
    ///
    /// Конструкция test-fixture: парсим entry-module + вручную создаём
    /// fake prelude PeerFile с одним type `Foo`, имитируя ситуацию когда
    /// resolver merge'нул prelude items. Без peer_files visibility-логика
    /// не активирует (single-file legacy path).
    mod prelude_shadow {
        use super::*;
        use crate::ast::{Item, PeerFile, TypeDecl, TypeDeclKind};
        use crate::diag::{FileId, Span};
        use std::collections::HashSet;
        use std::path::PathBuf;

        /// Создаёт fake prelude peer file со списком top-level type-имён.
        fn fake_prelude_peer(name_decls: &[&str]) -> PeerFile {
            // Plan 123 baseline-fix (2026-06-02): Default::default() spread.
            let items: Vec<Item> = name_decls.iter().map(|n| Item::Type(TypeDecl {
                is_export: true,
                name: (*n).to_string(),
                kind: TypeDeclKind::Record(Vec::new()),
                ..Default::default()
            })).collect();
            PeerFile {
                path: PathBuf::from("/synthetic/std/prelude/core.nv"),
                file_id: FileId::from(42_u32),
                imports: Vec::new(),
                items_here: items,
                imported_item_names: HashSet::new(),
                is_entry_module: false,
                module_name: vec!["std".into(), "prelude".into(), "core".into()],
            }
        }

        fn add_fake_prelude(m: &mut Module, names: &[&str]) {
            // Ensure entry's own peer_file существует — иначе fallback
            // на module.items (legacy single-file).
            let entry_peer = PeerFile {
                path: PathBuf::from("/synthetic/entry.nv"),
                file_id: FileId::from(0_u32),
                imports: m.imports.clone(),
                items_here: m.items.clone(),
                imported_item_names: HashSet::new(),
                is_entry_module: true,
                module_name: m.name.clone(),
            };
            m.peer_files = vec![entry_peer, fake_prelude_peer(names)];
        }

        #[test]
        fn warns_on_user_type_shadowing_prelude_option() {
            let mut m = parse("module myapp\ntype Option { foo int }\n");
            add_fake_prelude(&mut m, &["Option"]);
            let ws = lint_prelude_shadow(&m);
            assert_eq!(ws.len(), 1, "expected one W_PRELUDE_SHADOW");
            assert_eq!(ws[0].rule, "W_PRELUDE_SHADOW");
            assert!(ws[0].diag.message.contains("`Option`"),
                "message should mention shadowed name: {}", ws[0].diag.message);
        }

        #[test]
        fn no_warning_when_no_shadow() {
            let mut m = parse("module myapp\ntype MyType { x int }\n");
            add_fake_prelude(&mut m, &["Option", "Result"]);
            let ws = lint_prelude_shadow(&m);
            assert!(ws.is_empty(), "no shadow → no warning, got {:?}", ws);
        }

        #[test]
        fn suppress_via_allow_prelude_shadow_clause() {
            // D174 (Plan 107): `#allow(shadow)` attribute before `module` → ModuleAttrKind.
            let mut m = parse("#allow(shadow)\nmodule myapp\ntype Option { foo int }\n");
            add_fake_prelude(&mut m, &["Option"]);
            let ws = lint_prelude_shadow(&m);
            assert!(ws.is_empty(), "suppress should silence W_PRELUDE_SHADOW, got {:?}", ws);
        }

        #[test]
        fn no_prelude_does_not_suppress_explicit_shadow_lint() {
            // `#no_prelude` (D174, Plan 107) отключает auto-import — visibility set пуст,
            // shadowing невозможен → no warning естественно.
            let mut m = parse("#no_prelude\nmodule myapp\ntype Option { foo int }\n");
            // НЕ добавляем fake prelude peer'ы — `no_prelude` исключает их.
            let ws = lint_prelude_shadow(&m);
            assert!(ws.is_empty(), "no_prelude → no prelude visibility, no warning");
        }

        #[test]
        fn const_shadowing_emits_warning() {
            let mut m = parse("module myapp\nconst PRELUDE_VERSION int = 99\n");
            add_fake_prelude(&mut m, &["PRELUDE_VERSION"]);
            let ws = lint_prelude_shadow(&m);
            assert_eq!(ws.len(), 1);
            assert!(ws[0].diag.message.contains("`PRELUDE_VERSION`"));
        }

        #[test]
        fn prelude_self_module_skipped() {
            // Prelude sub-modules legitimately declare prelude names —
            // не должны получать W_PRELUDE_SHADOW для себя.
            let mut m = parse("module std.prelude.core\ntype Option { x int }\n");
            // Даже если бы peer_file сказал что Option visible — should skip.
            add_fake_prelude(&mut m, &["Option"]);
            let ws = lint_prelude_shadow(&m);
            assert!(ws.is_empty(), "prelude self-module must be skipped");
        }
    }

    // Owner decision (2026-07-17): W_WITH_MUTATOR closure-param exception —
    // scope-guard `with_*(body fn() -> R)` must stay silent; field-copy
    // `with_*(v T)` must still warn.

    #[test]
    fn no_warning_on_with_mutator_closure_param() {
        let src = "module foo\n\
             type Mutex { mut locked bool }\n\
             export fn Mutex mut @with_lock[R](body fn() -> R) -> R {\n\
                 @locked = true\n\
                 ro r = body()\n\
                 @locked = false\n\
                 r\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_WITH_MUTATOR"),
            "scope-guard with_* (closure param) must NOT warn, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn warns_on_with_mutator_value_param() {
        let src = "module foo\n\
             type Widget { mut label str }\n\
             export fn Widget mut @with_label(v str) -> () {\n\
                 @label = v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_WITH_MUTATOR"),
            "value-param with_* (field-copy shape) must still warn, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Plan 185 Ф.N (owner decision 2026-07-17): `// nova:allow W_CODE --
    // причина` inline suppression mechanism.

    #[test]
    fn nova_allow_with_reason_suppresses_finding() {
        let src = "module foo\n\
             type Widget { ro kind int }\n\
             // nova:allow W_STATIC_CONVERSION -- test reason\n\
             export fn Widget.from(x int) -> Widget => { kind: x }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_STATIC_CONVERSION"),
            "nova:allow with a reason must suppress the finding, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
        assert!(
            !ws.iter().any(|w| w.rule == "E_LINT_ALLOW_NO_REASON"),
            "well-formed nova:allow must not itself be a finding, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn nova_allow_without_reason_does_not_suppress_and_errors() {
        let src = "module foo\n\
             type Widget { ro kind int }\n\
             // nova:allow W_STATIC_CONVERSION\n\
             export fn Widget.from(x int) -> Widget => { kind: x }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_STATIC_CONVERSION"),
            "no-reason nova:allow must NOT suppress, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
        assert!(
            ws.iter().any(|w| w.rule == "E_LINT_ALLOW_NO_REASON"),
            "no-reason nova:allow must itself be a finding, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn nova_allow_wrong_rule_id_does_not_suppress_other_rule() {
        // Комментарий гасит W_PARAM_NO_CONTRACT, но не W_STATIC_CONVERSION —
        // находка другого правила на той же строке должна остаться.
        let src = "module foo\n\
             type Widget { ro kind int }\n\
             // nova:allow W_PARAM_NO_CONTRACT -- unrelated reason\n\
             export fn Widget.from(x int) -> Widget => { kind: x }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_STATIC_CONVERSION"),
            "allow of a DIFFERENT rule id must not suppress this finding, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn nova_allow_not_on_line_immediately_before_does_not_suppress() {
        // Пустая строка между `nova:allow` и декларацией — контракт «на
        // строке ПЕРЕД» не соблюдён, находка должна остаться.
        let src = "module foo\n\
             type Widget { ro kind int }\n\
             // nova:allow W_STATIC_CONVERSION -- test reason\n\
             \n\
             export fn Widget.from(x int) -> Widget => { kind: x }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_STATIC_CONVERSION"),
            "nova:allow must only suppress the very next line, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // [M-from-str-static-conversion-lint-gap] (2026-07-30): детектор был
    // слеп к `from_str`-морфологии (матчил только буквальные `from`/`parse`).

    #[test]
    fn warns_on_static_from_str() {
        // pos: `Type.from_str(s str)` — та же «пятая дверь», что и голый
        // `from`, просто другим именем — канон `str @to_path()`.
        let src = "module foo\n\
             type Path { ro bytes []u8 }\n\
             export fn Path.from_str(s str) -> Path => { bytes: s.bytes() }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_STATIC_CONVERSION"),
            "ожидался W_STATIC_CONVERSION на `Path.from_str`, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_from_polar_concept_source() {
        // neg: `from_polar` — источник НЕ значение-ресивер, а концепт (пара
        // r/theta) — легальная дверь по §1а, НЕ должна расширяться слепо.
        let src = "module foo\n\
             type Complex { ro re f64, ro im f64 }\n\
             export fn Complex.from_polar(r f64, theta f64) -> Complex => \
             { re: r, im: theta }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_STATIC_CONVERSION"),
            "не должен fire на `from_polar` (концепт-источник), получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_from_raw_parts_concept_source() {
        // neg: `from_raw_parts`-класс (сырой хендл/указатель — концепт, не
        // значение-ресивер) — легальная дверь, вторая явная граница §1а.
        let src = "module foo\n\
             type Buf { ro ptr *() }\n\
             export fn Buf.from_raw_parts(p *(), len int) -> Buf => { ptr: p }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_STATIC_CONVERSION"),
            "не должен fire на `from_raw_parts` (концепт-источник), получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // W_CONSUME_NAKED_NAME — `consume`-receiver + голое имя-вид,
    // конвертирующее в другой тип (§1а, ось владения; зеркало
    // W_STATIC_CONVERSION).

    #[test]
    fn warns_on_consume_naked_name_into_other_type() {
        let src = "module foo\n\
             type Response { ro body []u8 }\n\
             type HttpError { ro msg str }\n\
             export fn Response consume @bytes() -> Result[[]u8, HttpError] => Ok(@body)\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_CONSUME_NAKED_NAME"),
            "consume + голое имя, конвертирующее в другой тип, должно флагаться, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn does_not_warn_on_consume_into_prefix() {
        let src = "module foo\n\
             type Response { ro body []u8 }\n\
             type HttpError { ro msg str }\n\
             export fn Response consume @into_bytes() -> Result[[]u8, HttpError] => Ok(@body)\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_CONSUME_NAKED_NAME"),
            "канон into_* не должен флагаться, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn does_not_warn_on_consume_with_mutator() {
        let src = "module foo\n\
             type Body { ro limit int }\n\
             export fn Body consume @with_limit(n int) -> Body => { limit: n }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_CONSUME_NAKED_NAME"),
            "consume @with_* (D117 wither, не финализатор) не должен флагаться, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn does_not_warn_on_non_consume_bare_view() {
        let src = "module foo\n\
             type Str2 { ro body []u8 }\n\
             export fn Str2 @bytes() -> []u8 => @body\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_CONSUME_NAKED_NAME"),
            "не-consume голый вид не должен флагаться, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn does_not_warn_on_consume_same_type_return() {
        let src = "module foo\n\
             type Body { ro limit int }\n\
             export fn Body consume @clamp(n int) -> Body => { limit: n }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_CONSUME_NAKED_NAME"),
            "возврат ТОГО ЖЕ типа (не финализатор в другой тип) не должен флагаться, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn does_not_warn_on_consume_unit_finalizer() {
        let src = "module foo\n\
             type Conn { ro fd int }\n\
             export fn Conn consume @close() -> () => ()\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_CONSUME_NAKED_NAME"),
            "Unit-возврат (RAII-финализатор, не конверсия в значение) не должен флагаться, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn does_not_warn_on_consume_result_unit_finalizer() {
        let src = "module foo\n\
             type Fd { ro n int }\n\
             type IoError { ro msg str }\n\
             export fn Fd consume @close() -> Result[(), IoError] => Ok(())\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_CONSUME_NAKED_NAME"),
            "`Result[(), E]`-финализатор (close-класс) не должен флагаться, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn nova_allow_suppresses_consume_naked_name() {
        let src = "module foo\n\
             type Response { ro body []u8 }\n\
             type HttpError { ro msg str }\n\
             // nova:allow W_CONSUME_NAKED_NAME -- test reason\n\
             export fn Response consume @bytes() -> Result[[]u8, HttpError] => Ok(@body)\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_CONSUME_NAKED_NAME"),
            "nova:allow должен гасить находку, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Plan 185 (заказ владельца 2026-07-17): W_NON_COMPOUND_ASSIGN.

    #[test]
    fn warns_on_non_compound_add() {
        let src = "module foo\nfn run() -> () {\n    mut x = 0\n    x = x + 1\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_NON_COMPOUND_ASSIGN"),
            "expected W_NON_COMPOUND_ASSIGN, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn warns_on_non_compound_self_field() {
        let src = "module foo\n\
             type Counter { mut count int }\n\
             export fn Counter mut @bump() -> () {\n    @count = @count + 1\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_NON_COMPOUND_ASSIGN"),
            "expected W_NON_COMPOUND_ASSIGN on `@count = @count + 1`, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_non_compound_when_lhs_rhs_differ() {
        // `x = y + 1` — RHS не читает `x` первым операндом, компаунд `x += 1`
        // изменил бы СЕМАНТИКУ (не эквивалентная замена) — молчим.
        let src = "module foo\nfn run() -> () {\n    mut x = 0\n    ro y = 1\n    x = y + 1\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_NON_COMPOUND_ASSIGN"),
            "must NOT fire when LHS != RHS-left operand, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_non_compound_for_operator_without_compound_form() {
        // `%` не имеет компаунд-формы в Nova (`AssignOp` без `Mod`) — молчим.
        let src = "module foo\nfn run() -> () {\n    mut x = 5\n    x = x % 2\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_NON_COMPOUND_ASSIGN"),
            "must NOT fire for an operator without a compound form, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_non_compound_index_target() {
        // Index-места намеренно исключены (компаунд по индексу — другой,
        // непроверенный путь кодогена) — молчим.
        let src = "module foo\nfn run() -> () {\n    mut arr = [1, 2, 3]\n    arr[0] = arr[0] + 1\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_NON_COMPOUND_ASSIGN"),
            "must NOT fire on an Index target, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_non_compound_dedup_with_str_concat_loop() {
        // Тот же сайт, что и W_STR_CONCAT_LOOP (в цикле, `+`, строкоподобный
        // RHS) — не дублируем.
        let src = "module foo\nfn run() -> () {\n    mut buf = \"\"\n    \
             for i in 0..3 {\n        buf = buf + \"x\"\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_STR_CONCAT_LOOP"),
            "sanity: expected W_STR_CONCAT_LOOP on this site, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
        assert!(
            !ws.iter().any(|w| w.rule == "W_NON_COMPOUND_ASSIGN"),
            "must NOT duplicate W_STR_CONCAT_LOOP on the same site, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn warns_on_non_compound_str_concat_outside_loop() {
        // Тот же shape, но ВНЕ цикла — W_STR_CONCAT_LOOP не применяется
        // (требует `in_loop`), W_NON_COMPOUND_ASSIGN — применяется.
        let src = "module foo\nfn run() -> () {\n    mut buf = \"a\"\n    buf = buf + \"x\"\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_STR_CONCAT_LOOP"),
            "sanity: W_STR_CONCAT_LOOP must NOT fire outside a loop, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
        assert!(
            ws.iter().any(|w| w.rule == "W_NON_COMPOUND_ASSIGN"),
            "expected W_NON_COMPOUND_ASSIGN outside a loop, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Владелец 2026-07-21: `conv_is_stringish` расширен на `.to_str()`/
    // `.concat(...)`-chain (T1 [бинарный `+`] ретрактирован в HARD ERROR
    // E_STR_CONCAT_PLUS, types/mod.rs — тесты там же, spec_tests/conformance/
    // neg fixture); W_STR_CONCAT_LOOP (T2) и новый W_STR_CONCAT_METHOD (T3)
    // остаются lint-warning.

    #[test]
    fn warns_on_str_concat_loop_with_to_str_operand() {
        // Расширение: `s += i.to_str()` в цикле — раньше НЕ ловилось
        // (`conv_is_stringish` не распознавал `.to_str()`-вызов), теперь ловится.
        let src = "module foo\nfn run() -> str {\n    mut s = \"\"\n    \
             for i in 0..10 {\n        s += i.to_str()\n    }\n    s\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_STR_CONCAT_LOOP"),
            "ожидался W_STR_CONCAT_LOOP на `s += i.to_str()` в цикле, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_str_concat_loop_int_accumulator() {
        // Обратная совместимость: числовой аккумулятор в цикле — не str,
        // не должен fire (не должно быть ложных срабатываний от `.to_str()`
        // расширения на несвязанные `+=`).
        let src = "module foo\nfn run() -> int {\n    mut acc = 0\n    \
             for i in 0..10 {\n        acc += i\n    }\n    acc\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_STR_CONCAT_LOOP"),
            "не должен fire на int += int, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Владелец 2026-07-21: W_STR_CONCAT_METHOD (T3) — `.concat(...)` на str.

    #[test]
    fn warns_on_str_dot_concat_method_call() {
        let src = "module foo\nfn run() -> str {\n    \"hello\".concat(\" world\")\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_STR_CONCAT_METHOD"),
            "ожидался W_STR_CONCAT_METHOD на str.concat(...), получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn warns_on_str_concat_via_to_str_receiver() {
        // Ресивер — `.to_str()`-конверсия (тоже точно str).
        let src = "module foo\nfn run(n int) -> str {\n    n.to_str().concat(\"!\")\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_STR_CONCAT_METHOD"),
            "ожидался W_STR_CONCAT_METHOD, ресивер .to_str(), получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn warns_twice_on_str_concat_chain() {
        // Chain-propagation: оба `.concat(...)` в цепочке распознаются
        // (внешний ресивер = внутренний concat-call на str-литерале).
        let src = "module foo\nfn run() -> str {\n    \"a\".concat(\"b\").concat(\"c\")\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hits = ws.iter().filter(|w| w.rule == "W_STR_CONCAT_METHOD").count();
        assert!(
            hits >= 2,
            "ожидались 2 срабатывания (chain), получено {}: {:?}",
            hits,
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_vec_concat_negative() {
        // Негатив: `.concat(...)` на Vec — ресивер не str-литерал/`.to_str()`/
        // `.concat()`-chain — эвристика естественно молчит.
        let src = "module foo\nfn run() -> Vec[int] {\n    \
             ro v = Vec[int].new()\n    ro w = Vec[int].new()\n    v.concat(w)\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_STR_CONCAT_METHOD"),
            "не должен fire на Vec.concat, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_bytes_concat_negative() {
        // Негатив: `.concat(...)` на `[]u8` — та же логика.
        let src = "module foo\nfn run() -> []u8 {\n    \
             mut a []u8 = []u8.new()\n    mut b []u8 = []u8.new()\n    a.concat(b)\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_STR_CONCAT_METHOD"),
            "не должен fire на []u8.concat, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_str_concat_method_in_runtime_string_impl() {
        // `in_str_runtime_impl` — исключение для реализации примитива
        // (std/src/runtime/string/**, вычисляется по пути вызывающей
        // стороной, nova-cli::conv_lint_options_for).
        let src = "module std.runtime.string.transform\n\
             fn str @concat_twice(a str, b str) -> str {\n    a.concat(b)\n}\n";
        let m = parse(src);
        let opts = ConvLintOptions { in_str_runtime_impl: true, ..ConvLintOptions::default() };
        let ws = run_conv_rules(Some(&m), src, &opts, None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_STR_CONCAT_METHOD"),
            "должен молчать при in_str_runtime_impl, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Plan 185 (заказ владельца 2026-07-17): W_WHILE_COUNTER_FOR_RANGE.

    #[test]
    fn warns_on_basic_while_counter() {
        let src = "module foo\nfn run() -> () {\n    mut i = 0\n    \
             while i < 10 {\n        i = i + 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE"),
            "expected W_WHILE_COUNTER_FOR_RANGE, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn warns_on_inclusive_while_counter_with_compound_increment() {
        let src = "module foo\nfn run() -> () {\n    mut i = 0\n    \
             while i <= 10 {\n        i += 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            ws.iter().any(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE"),
            "expected W_WHILE_COUNTER_FOR_RANGE (inclusive), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_while_counter_when_continue_in_body() {
        // `continue` перепрыгнул бы инкремент — семантика при замене на
        // `for` иная — молчим.
        let src = "module foo\nfn run() -> () {\n    mut i = 0\n    \
             while i < 10 {\n        if i == 5 { continue }\n        i = i + 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE"),
            "must NOT fire when body contains `continue`, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_while_counter_when_used_after_loop() {
        // `i` используется ПОСЛЕ while (trailing-значение блока) — `for`-
        // переменная не пережила бы цикл — молчим.
        let src = "module foo\nfn run() -> int {\n    mut i = 0\n    \
             while i < 10 {\n        i = i + 1\n    }\n    i\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE"),
            "must NOT fire when `i` is used after the loop, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_while_counter_when_reassigned_elsewhere_in_body() {
        // `i` присваивается ЕЩЁ РАЗ внутри тела (не только последний
        // инкремент) — молчим.
        let src = "module foo\nfn run() -> () {\n    mut i = 0\n    \
             while i < 10 {\n        if i == 3 { i = 100 }\n        i = i + 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE"),
            "must NOT fire when `i` is reassigned elsewhere in the body, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_while_counter_when_end_mutated_in_body() {
        // `END` (`n`) мутируется в теле — `for`-range снял бы `n` ОДИН раз,
        // `while` — перечитывает каждую итерацию — реальная разница, молчим.
        let src = "module foo\nfn run() -> () {\n    mut n = 10\n    mut i = 0\n    \
             while i < n {\n        n = n - 1\n        i = i + 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE"),
            "must NOT fire when END is mutated in the body, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_while_counter_when_end_is_a_call() {
        // `END` — вызов (`xs.len()`), переоценивается бы КАЖДУЮ итерацию
        // `while`, но РОВНО ОДИН раз в `for`-range — молчим.
        let src = "module foo\nfn run(xs []int) -> () {\n    mut i = 0\n    \
             while i < xs.len() {\n        i = i + 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE"),
            "must NOT fire when END is a call, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_while_counter_when_increment_not_last_stmt() {
        let src = "module foo\nfn run() -> () {\n    mut i = 0\n    \
             while i < 10 {\n        i = i + 1\n        log(i)\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE"),
            "must NOT fire when the increment is not the last statement, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn while_counter_nested_pos_case_flags_c_and_b_not_pos() {
        // Реальный std-паттерн (string_builder.nv `@pad_in_place`): внешний
        // `c`-while и внутренний `b`-while — оба счётчиковые, подпадают под
        // критерии; `pos` растёт СКВОЗЬ оба уровня (не `mut pos = ...`
        // непосредственно перед while — между ним и любым while всегда есть
        // другой `mut`-let) — под критерии НЕ подпадает, не флагуется.
        let src = "module foo\nfn run(fill_len int) -> () {\n    \
             mut pos = 0\n    mut c = 0\n    while c < 3 {\n        \
             mut b = 0\n        while b < fill_len {\n            \
             pos = pos + 1\n            b = b + 1\n        }\n        \
             c = c + 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hits: Vec<_> = ws.iter().filter(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE").collect();
        assert_eq!(
            hits.len(),
            2,
            "expected exactly 2 hits (c-loop + b-loop), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Regression (2026-07-18, found by this rule's own sweep on
    // std/time/civil/tz.nv): an explicit counter type annotation
    // (`mut y i32 = ...`) MUST be called out in the suggestion — `for y in
    // a..b` infers the element type from the range bounds (bare int
    // literals default to `int`), silently widening away the original
    // `i32` and breaking a narrower-param call downstream
    // (E_IMPLICIT_NARROWING). The message must mention the type so anyone
    // applying the suggestion writes `for y i32 in a..b` (Plan 87).

    #[test]
    fn while_counter_message_carries_explicit_counter_type() {
        let src = "module foo\nfn run() -> () {\n    mut y i32 = 0\n    \
             while y <= 10 {\n        y += 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = ws
            .iter()
            .find(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE")
            .unwrap_or_else(|| {
                panic!(
                    "expected W_WHILE_COUNTER_FOR_RANGE, got: {:?}",
                    ws.iter().map(|w| w.rule).collect::<Vec<_>>()
                )
            });
        assert!(
            hit.diag.message.contains("i32"),
            "suggestion must mention the explicit counter type `i32`, got: {}",
            hit.diag.message
        );
    }

    #[test]
    fn while_counter_message_silent_on_type_when_none_declared() {
        // Sanity: no explicit type on the counter → no spurious type-carry
        // note (keeps the common-case message clean).
        let src = "module foo\nfn run() -> () {\n    mut i = 0\n    \
             while i < 10 {\n        i += 1\n    }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = ws
            .iter()
            .find(|w| w.rule == "W_WHILE_COUNTER_FOR_RANGE")
            .expect("expected W_WHILE_COUNTER_FOR_RANGE");
        assert!(
            !hit.diag.message.contains("ВАЖНО"),
            "must NOT emit the type-carry note when no explicit type was declared, got: {}",
            hit.diag.message
        );
    }

    // Regression (2026-07-18, found by this rule's own sweep on
    // std/concurrency/retry.nv): `d = d * multiplier` on a `Duration`
    // value-record must NOT be suggested as `d *= multiplier` — Nova's
    // compound-assign codegen dispatches operator overloads ONLY for
    // `Add`/`Sub` (`emit_c.rs` — "`Add`/`Sub` are overloadable operators;
    // `*=`/`/=` are not"); `*=`/`/=` on an operator-overloaded type falls
    // through to a raw C `*=`/`/=` on a struct — a hard CC-FAIL. Since this
    // rule has no type info, `Mul`/`Div` compound-assign is never suggested
    // at all (conservative — see `conv_binop_to_compound`).

    #[test]
    fn no_warning_on_non_compound_mul_or_div() {
        let src = "module foo\nfn run() -> () {\n    mut d = 1.0\n    d = d * 2.0\n    \
             mut e = 8.0\n    e = e / 2.0\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            !ws.iter().any(|w| w.rule == "W_NON_COMPOUND_ASSIGN"),
            "must NOT suggest `*=`/`/=` (compound-assign never overload-dispatches Mul/Div, \
             CC-FAIL risk on operator-overloaded types), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Plan 185 (заказ владельца 2026-07-20): W_MANUAL_MIN_MAX / W_MANUAL_CLAMP.

    fn min_max_rule_hits<'a>(ws: &'a [LintWarning], rule: &str) -> Vec<&'a LintWarning> {
        ws.iter().filter(|w| w.rule == rule).collect()
    }

    #[test]
    fn manual_min_max_gt_then_left() {
        // if a > b { a } else { b } -> a.max(b)
        let src = "module foo\nfn run(a int, b int) -> int => if a > b { a } else { b }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("a.max(b)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn manual_min_max_gt_then_right_is_min() {
        // if a > b { b } else { a } -> a.min(b)
        let src = "module foo\nfn run(a int, b int) -> int => if a > b { b } else { a }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("a.min(b)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn manual_min_max_lt_then_left_is_min() {
        // if a < b { a } else { b } -> a.min(b)
        let src = "module foo\nfn run(a int, b int) -> int => if a < b { a } else { b }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("a.min(b)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn manual_min_max_lt_then_right_is_max() {
        // if a < b { b } else { a } -> a.max(b)
        let src = "module foo\nfn run(a int, b int) -> int => if a < b { b } else { a }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("a.max(b)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn manual_min_max_ge_and_le_mirrors() {
        let src = "module foo\n\
             fn m1(a int, b int) -> int => if a >= b { a } else { b }\n\
             fn m2(a int, b int) -> int => if a <= b { a } else { b }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 2, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit.iter().any(|w| w.diag.message.contains("a.max(b)")));
        assert!(hit.iter().any(|w| w.diag.message.contains("a.min(b)")));
    }

    #[test]
    fn manual_min_max_return_form() {
        // Block form with a single `return` statement (not bare trailing expr).
        let src = "module foo\nfn run(a int, b int) -> int {\n    if a > b { return a }\n    b\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        // Здесь else-ветви нет вовсе (это if-statement с одиночным
        // return в then, а не if/else-выражение) — statement-форма (без
        // else) не матчит, т.к. тело — `return`, не присваивание. Значит
        // находки НЕ ожидается; тест фиксирует, что мы не ломаем на этой
        // форме (не паникуем на несовпадающем then-теле).
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert!(hit.is_empty(), "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_min_max_literal_bound() {
        // if n > 0 { n } else { 0 } -> n.max(0) (io/mem.nv-style idiom).
        let src = "module foo\nfn run(n int) -> int => if n > 0 { n } else { 0 }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("n.max(0)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn manual_min_max_statement_form_cap_at_hi() {
        // if x > hi { x = hi } -> x = x.min(hi) (одна верхняя граница).
        let src = "module foo\nfn run(hi int) -> int {\n    mut x = 0\n    \
             if x > hi { x = hi }\n    x\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("x = x.min(hi)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn manual_min_max_statement_form_mirrored_running_max() {
        // if deadline_ms > current_ms { current_ms = deadline_ms } (running-max
        // tracking, testing/handlers.nv-style — target на ПРАВОЙ стороне cond).
        let src = "module foo\nfn run(deadline_ms int) -> int {\n    mut current_ms = 0\n    \
             if deadline_ms > current_ms { current_ms = deadline_ms }\n    current_ms\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(
            hit[0].diag.message.contains("current_ms = current_ms.max(deadline_ms)"),
            "got: {}",
            hit[0].diag.message
        );
    }

    #[test]
    fn manual_min_max_statement_form_mirrored_running_min() {
        // if x < lo { lo = x } (running-min tracking, statistics.nv-style).
        let src = "module foo\nfn run(x int) -> int {\n    mut lo = 0\n    \
             if x < lo { lo = x }\n    lo\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(
            hit[0].diag.message.contains("lo = lo.min(x)"),
            "got: {}",
            hit[0].diag.message
        );
    }

    #[test]
    fn no_warning_manual_min_max_different_operands() {
        // Ветви возвращают операнды, НЕ участвующие в сравнении — не min/max.
        let src = "module foo\nfn run(a int, b int, c int) -> int => if a > b { a } else { c }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_manual_min_max_side_effect_branch() {
        // Ветвь — вызов (потенциальный побочный эффект), не голое место/литерал.
        let src = "module foo\nfn run(a int, b int) -> int => if a > b { compute() } else { b }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_manual_min_max_unrelated_assign_target() {
        // Statement-форма: цель присваивания — НЕ операнд сравнения.
        let src = "module foo\nfn run(a int, b int) -> int {\n    mut other = 0\n    \
             if a > b { other = b }\n    other\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Регрессия-предохранитель: `@min`/`@max` (std/runtime/defaults.nv) сами
    // реализованы РОВНО этим if/else-паттерном — предложение заменить на
    // `.max(...)`/`.min(...)` внутри тела `@max` было бы рекурсией на себя.
    // Гейт — по ИМЕНИ функции (`min`/`max`), не по receiver'у: свободная
    // функция `fn max(...)` (как `spec_tests/conformance/standalone/
    // f11_corpus_06_pattern_regression.nv`) тоже обязана молчать.
    #[test]
    fn no_warning_manual_min_max_self_referential_definition() {
        let src = "module foo\n\
             export fn int @min(other int) -> int => if @ < other { @ } else { other }\n\
             export fn int @max(other int) -> int => if @ > other { @ } else { other }\n\
             fn max(a int, b int) -> int => if a > b { a } else { b }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX").is_empty(),
            "must NOT fire inside the very definition of @min/@max/free `max`, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn manual_clamp_canonical_lo_first() {
        // if x < lo { lo } else if x > hi { hi } else { x } -> x.clamp(lo, hi)
        // (буквальная форма @clamp, std/prelude/protocols.nv — здесь имя fn
        // намеренно НЕ `clamp`, чтобы не задеть само-ссылочный гейт).
        let src = "module foo\n\
             fn bound(x int, lo int, hi int) -> int =>\n    \
             if x < lo { lo } else if x > hi { hi } else { x }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_CLAMP");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("x.clamp(lo, hi)"), "got: {}", hit[0].diag.message);
        // Дедуп: внутренний `if x > hi { hi } else { x }` НЕ должен ТАКЖЕ
        // всплыть как отдельный W_MANUAL_MIN_MAX на том же сайте.
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX").is_empty(),
            "clamp finding must suppress the nested half-pattern min/max finding, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn manual_clamp_hi_first_order() {
        // if x > hi { hi } else if x < lo { lo } else { x } -> x.clamp(lo, hi)
        // (проверка-на-верхнюю-границу идёт первой — зеркальный порядок).
        let src = "module foo\n\
             fn bound(x int, lo int, hi int) -> int =>\n    \
             if x > hi { hi } else if x < lo { lo } else { x }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_CLAMP");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("x.clamp(lo, hi)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn manual_clamp_nested_block_form() {
        // else { if x > hi { hi } else { x } } — буквально вложенный if,
        // не `else if`-сахар (прецедент method_with_args_ok.nv).
        let src = "module foo\n\
             fn bound(x int, lo int, hi int) -> int {\n    \
             if x < lo { lo } else { if x > hi { hi } else { x } }\n}\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_CLAMP");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("x.clamp(lo, hi)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn no_warning_manual_clamp_three_way_comparator() {
        // cmp()-стиль -1/0/1 (semver.nv/period.nv-класс) — ветви возвращают
        // литералы, НЕ равные границам сравнения — не clamp.
        let src = "module foo\n\
             fn cmp3(a int, b int) -> int =>\n    \
             if a < b { -1 } else if a > b { 1 } else { 0 }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_CLAMP").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_manual_clamp_final_else_not_original() {
        let src = "module foo\n\
             fn bound(x int, lo int, hi int) -> int =>\n    \
             if x < lo { lo } else if x > hi { hi } else { lo }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_CLAMP").is_empty(),
            "final else must return the original unclamped operand, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_manual_clamp_same_direction_checks() {
        // Обе проверки — нижняя граница (не lo/hi пара) — не clamp-форма.
        let src = "module foo\n\
             fn bound(x int, lo int, lo2 int) -> int =>\n    \
             if x < lo { lo } else if x < lo2 { lo2 } else { x }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_CLAMP").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Регрессия-предохранитель: `@clamp` (std/prelude/protocols.nv Ints-
    // бланкет, std/runtime/defaults.nv f32/f64) сам реализован РОВНО этим
    // трёхветочным паттерном — предложение заменить на `.clamp(...)` внутри
    // тела `@clamp` было бы рекурсией на себя.
    #[test]
    fn no_warning_manual_clamp_self_referential_definition() {
        let src = "module foo\n\
             export fn f64 @clamp(lo f64, hi f64) -> f64 =>\n    \
             if @ < lo { lo } else if @ > hi { hi } else { @ }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_CLAMP").is_empty(),
            "must NOT fire inside the very definition of @clamp, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Regression (найдено на живом std/prelude/protocols.nv +
    // spec_tests/conformance/method_with_args_ok.nv при разкраснении волны
    // 2026-07-20): тело `@clamp` (`if @ < lo { lo } else if @ > hi { hi }
    // else { @ }`) содержит ВНУТРЕННИЙ `if @ > hi { hi } else { @ }` —
    // сам по себе валидный 2-операндный min/max-шейп. W_MANUAL_CLAMP это
    // тело гасит (self-ref по имени `clamp`), но W_MANUAL_MIN_MAX должен
    // ТОЖЕ молчать внутри `clamp`-именованных fn — иначе ложно предлагает
    // `.min()`/`.max()` внутри канон-референсной реализации `@clamp`
    // (generic `fn[T Ints] T @clamp` — резолвит ли `.min()`/`.max()` на T
    // без отдельного bound-требования не проверено) и внутри
    // `method_with_args_ok.nv::Bounded4_1 @clamp`, которая пинует ИМЕННО
    // этот if/else-шейп для теста ro-caching кодогена.
    #[test]
    fn no_warning_manual_min_max_inside_clamp_definition() {
        let src = "module foo\n\
             export fn f64 @clamp(lo f64, hi f64) -> f64 =>\n    \
             if @ < lo { lo } else if @ > hi { hi } else { @ }\n\
             type Bounded4_1 { ro lo int, ro hi int }\n\
             fn Bounded4_1 @clamp(x int) -> int {\n    \
             if x < @lo { @lo } else { if x > @hi { @hi } else { x } }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_MIN_MAX").is_empty(),
            "must NOT fire inside the body of ANY `clamp`-named fn (canon @clamp \
             reference impl AND user-defined @clamp pinning a codegen shape), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Владелец 2026-07-21: `new-then-cap` lint.

    #[test]
    fn warns_on_split_stmt_new_then_cap() {
        let m = parse(
            "module foo\n\
             fn run() {\n    \
             mut v = Vec[int].new()\n    \
             v.cap(16)\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            ws.iter().any(|w| w.rule == "new-then-cap"),
            "ожидался new-then-cap, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn warns_on_chain_new_then_cap() {
        let m = parse(
            "module foo\n\
             fn run() {\n    \
             mut v = Vec[int].new().cap(16)\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            ws.iter().any(|w| w.rule == "new-then-cap"),
            "ожидался new-then-cap (chain), получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_when_cap_passed_at_new() {
        let m = parse(
            "module foo\n\
             fn run() {\n    \
             mut v = Vec[int].new(cap: 16)\n    \
             v.push(1)\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "new-then-cap"),
            "не должен fire когда cap уже передан в new(), получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_standalone_cap_setter() {
        // `v.cap(n)` без предшествующего bare `.new()` на том же binding
        // (legit setter — shrink-to-fit / room-for-N, vec/core.nv:242-243)
        // — must NOT warn.
        let m = parse(
            "module foo\n\
             fn run(v mut Vec[int]) {\n    \
             ro extra = 4\n    \
             v.cap(v.len() + extra)\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "new-then-cap"),
            "не должен fire на самостоятельном .cap()-сеттере, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_when_cap_call_on_different_binding() {
        // `v` был создан bare-`new()`, но следующий stmt зовёт `.cap()` на
        // ДРУГОМ binding'е (`w`, обычный параметр) — не тот же паттерн,
        // adjacency-tracking по имени НЕ должен ложно смэтчить `v`.
        let m = parse(
            "module foo\n\
             fn run(w mut Vec[int]) {\n    \
             mut v = Vec[int].new()\n    \
             w.cap(16)\n    \
             v.push(1)\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "new-then-cap"),
            "не должен fire когда .cap() зовётся на ДРУГОМ binding'е, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_when_stmt_between_new_and_cap() {
        // Стейтмент МЕЖДУ new и cap разрывает «сразу следом» — не warn
        // (D-simple: только строго соседний stmt).
        let m = parse(
            "module foo\n\
             fn run() {\n    \
             mut v = Vec[int].new()\n    \
             ro n = 16\n    \
             v.cap(n)\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "new-then-cap"),
            "не должен fire когда между new и cap есть другой stmt, получено: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // ─── W_COERCE_EXPLICIT_REDUNDANT — call-arg lane (absorbed 2026-07-21
    // from the former standalone `W_REDUNDANT_BYTES_ON_LITERAL`) ──────────

    #[test]
    fn redundant_bytes_on_literal_call_arg() {
        // Real corpus shape: examples/tls/echo_server.nv
        // `stream.write_all("echo_ok\n".bytes())`.
        let src = "module foo\n\
             fn run(stream mut TcpStream) -> () {\n    \
             stream.write_all(\"echo_ok\\n\".bytes())\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_COERCE_EXPLICIT_REDUNDANT");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn redundant_bytes_on_interpolation_call_arg() {
        // Interpolated string receiver — always `str`-typed regardless of
        // content (mirrors StrLit treatment), owner brief 2026-07-21.
        let src = "module foo\n\
             fn run(stream mut TcpStream, msg str) -> () {\n    \
             stream.write_all(\"echo: ${msg}\".bytes())\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_COERCE_EXPLICIT_REDUNDANT");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn redundant_bytes_on_to_str_chain_call_arg() {
        // Owner brief 2026-07-21: a chain ending in `.to_str()` is also
        // syntactically str-guaranteed — real corpus shape (fmt/duration):
        // `f.write(@nanos.to_str().bytes())`.
        let src = "module foo\n\
             fn run(f mut FmtCtx, nanos i64) -> () {\n    \
             f.write(nanos.to_str().bytes())\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_COERCE_EXPLICIT_REDUNDANT");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn no_warning_bytes_on_variable_call_arg() {
        // `.bytes()` on an IDENT (not a literal/interp/`.to_str()`-chain) —
        // call-arg lane's explicit literal-only scope (owner brief), must
        // stay silent (even though the general D429 `#coerce` mechanism
        // would ALSO cover a variable receiver at a KNOWN-type position).
        let src = "module foo\n\
             fn run(stream mut TcpStream, s str) -> () {\n    \
             stream.write_all(s.bytes())\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_COERCE_EXPLICIT_REDUNDANT").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_double_warning_bytes_on_literal_let_position_not_call_arg() {
        // let/return positions are the SAME rule's own non-call-arg lane —
        // must fire exactly ONCE (no double-warning on one site).
        let src = "module foo\n\
             fn run() -> () {\n    \
             ro b []u8 = \"hi\".bytes()\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_COERCE_EXPLICIT_REDUNDANT");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn coerce_registry_scan_picks_up_local_coerce_fn_not_in_seed_list() {
        // Registry-driven extension (owner correction 2026-07-21): a
        // #coerce fn NOT in the hardcoded 3-seed fallback list (here: a
        // made-up `@view_bytes()` name) is still caught via the AST scan
        // (`scan_coerce_methods`) when its declaration is visible in the
        // SAME module — proves the lint reads pairs from what's actually
        // declared, not solely the static list. Exercised via the `let`-
        // annotation lane (accepts an Ident receiver, unlike the call-arg
        // lane's literal-only scope — see the sibling call-arg-specific
        // test below for that narrower lane).
        let src = "module foo\n\
             type Blob { ro data []u8 }\n\
             #coerce\n\
             fn Blob @view_bytes() -> ro []u8 => @data\n\
             fn run(b Blob) -> () {\n    \
             ro v []u8 = b.view_bytes()\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_COERCE_EXPLICIT_REDUNDANT");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn no_warning_call_arg_on_bare_ident_even_with_local_registry_hit() {
        // `b.view_bytes()` receiver `b` is an Ident (variable) at a CALL-ARG
        // position — that lane's literal-only scope excludes an Ident
        // regardless of registry membership (mirrors
        // `no_warning_bytes_on_variable_call_arg`, exercised through the
        // dynamic-registry path instead of the static-fallback path).
        let src = "module foo\n\
             type Blob { ro data []u8 }\n\
             #coerce\n\
             fn Blob @view_bytes() -> ro []u8 => @data\n\
             fn run(sink mut TcpStream, b Blob) -> () {\n    \
             sink.write_all(b.view_bytes())\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_COERCE_EXPLICIT_REDUNDANT").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // ─── W_REDUNDANT_CONSUME_REBIND ────────────────────────────────────

    #[test]
    fn redundant_consume_rebind_real_corpus_shape() {
        // Real corpus shape: std/src/fs/d323_open_options_test.nv:26
        // `Ok(consume f0) => { consume f = f0; ro _ = f.write("bb".bytes()); ro _ = f.close() }`.
        let src = "module foo\n\
             fn run(r Result[File, IoError]) -> () {\n    \
             match r {\n        \
             Ok(consume f0) => {\n            \
             consume f = f0\n            \
             ro _ = f.write(\"bb\".bytes())\n        \
             }\n        \
             Err(_) => ()\n    \
             }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_REDUNDANT_CONSUME_REBIND");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("Ok(consume f)"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn no_warning_consume_rebind_when_old_name_still_used() {
        let src = "module foo\n\
             fn run(r Result[File, IoError]) -> () {\n    \
             match r {\n        \
             Ok(consume f0) => {\n            \
             consume f = f0\n            \
             ro _ = f0.metadata()\n        \
             }\n        \
             Err(_) => ()\n    \
             }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_REDUNDANT_CONSUME_REBIND").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_consume_rebind_when_pattern_not_consume() {
        // Plain `Ok(f0)` — pattern binding is NOT `consume`, so `consume f
        // = f0` is not the double-rebind anti-pattern this rule targets.
        let src = "module foo\n\
             fn run(r Result[File, IoError]) -> () {\n    \
             match r {\n        \
             Ok(f0) => {\n            \
             consume f = f0\n        \
             }\n        \
             Err(_) => ()\n    \
             }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_REDUNDANT_CONSUME_REBIND").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // ─── W_MANUAL_CLOSE_AUTO_CLEANUP ────────────────────────────────────

    #[test]
    fn manual_close_auto_cleanup_tail_call_fires() {
        let src = "module foo\n\
             fn run(t TcpStream) -> () {\n    \
             consume conn TcpStream = t\n    \
             conn.close()\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_MANUAL_CLOSE_AUTO_CLEANUP");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn no_warning_manual_close_non_seed_type() {
        // `File` deliberately excluded — `@close` is fallible, no
        // auto-`@cleanup` exists for it (fs.nv doc, D133).
        let src = "module foo\n\
             fn run(t File) -> () {\n    \
             consume f File = t\n    \
             ro _ = f.close()\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_CLOSE_AUTO_CLEANUP").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_manual_close_not_tail_position() {
        // Early close with logic AFTER — legal, per owner's brief.
        let src = "module foo\n\
             fn run(t TcpStream) -> () {\n    \
             consume conn TcpStream = t\n    \
             conn.close()\n    \
             ro _ = 1\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_CLOSE_AUTO_CLEANUP").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_manual_close_untyped_consume() {
        // No explicit type annotation — conservative gap by design (can't
        // syntactically know the binding's type without cross-file lookup).
        let src = "module foo\n\
             fn run(t TcpStream) -> () {\n    \
             consume conn = t\n    \
             conn.close()\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_MANUAL_CLOSE_AUTO_CLEANUP").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // ─── W_REDUNDANT_CONST_TYPE_ANNOTATION ─────────────────────────────

    #[test]
    fn redundant_const_type_annotation_module_level() {
        // Real corpus shape: examples/tls/echo_client.nv:28
        // `const MESSAGE str = "hello from nova, over tls"`.
        let src = "module foo\nconst MESSAGE str = \"hello\"\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_REDUNDANT_CONST_TYPE_ANNOTATION");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("MESSAGE"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn redundant_const_type_annotation_scope_level() {
        let src = "module foo\n\
             fn run() -> int {\n    \
             const N int = 5\n    \
             N\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = min_max_rule_hits(&ws, "W_REDUNDANT_CONST_TYPE_ANNOTATION");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn no_warning_const_annotation_drives_coercion() {
        // `[]u8` — NOT the literal's own default type (`str`) — the
        // annotation is load-bearing (D55/D429 coercion), never flagged.
        let src = "module foo\nconst BUF []u8 = \"hello\"\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_REDUNDANT_CONST_TYPE_ANNOTATION").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_const_annotation_narrows_int_width() {
        let src = "module foo\nconst N u32 = 5\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_REDUNDANT_CONST_TYPE_ANNOTATION").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_const_no_annotation() {
        let src = "module foo\nconst MESSAGE = \"hello\"\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_REDUNDANT_CONST_TYPE_ANNOTATION").is_empty(),
            "got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_const_p67_path_call_receiver_guard() {
        // [M-p67-path-call-const-receiver-method-ice] guard: `BUDGET_MS` is
        // later used as `BUDGET_MS.to_millis()` (2-segment Path-call
        // receiver, real CC-FAIL reproduced this wave via
        // examples/mini_aggregator.nv) — must NOT recommend dropping the
        // annotation even though it matches the literal's own default type.
        let src = "module foo\n\
             const BUDGET_MS int = 120\n\
             fn f() -> int {\n    \
             BUDGET_MS.to_millis()\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            min_max_rule_hits(&ws, "W_REDUNDANT_CONST_TYPE_ANNOTATION").is_empty(),
            "must stay silent on a const later used as a call-receiver (P67-family risk), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // Ф.3 (owner order 2026-07-23, [M-manual-coalesce-lint-missing]):
    // W_MANUAL_COALESCE — identity-match drift from `X ?? D` canon (D86).

    fn coalesce_rule_hits<'a>(ws: &'a [LintWarning], rule: &str) -> Vec<&'a LintWarning> {
        ws.iter().filter(|w| w.rule == rule).collect()
    }

    #[test]
    fn manual_coalesce_pos_value_fallback_option() {
        let src = "module foo\n\
             fn find(n int) -> Option[int] => if n > 0 { Some(n) } else { None }\n\
             fn run(n int) -> int {\n\
                 match find(n) {\n\
                     Some(v) => v,\n\
                     None => 0,\n\
                 }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = coalesce_rule_hits(&ws, "W_MANUAL_COALESCE");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert!(hit[0].diag.message.contains("X ?? D"), "got: {}", hit[0].diag.message);
    }

    #[test]
    fn manual_coalesce_pos_value_fallback_result() {
        let src = "module foo\n\
             type E enum Bad\n\
             fn find(n int) -> Result[int, E] => if n > 0 { Ok(n) } else { Err(E.Bad) }\n\
             fn run(n int) -> int {\n\
                 match find(n) {\n\
                     Ok(v) => v,\n\
                     Err(_) => -1,\n\
                 }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = coalesce_rule_hits(&ws, "W_MANUAL_COALESCE");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_coalesce_pos_return_same_carrier() {
        let src = "module foo\n\
             fn find(n int) -> Option[int] => if n > 0 { Some(n) } else { None }\n\
             fn run(n int) -> Option[int] {\n\
                 ro x = match find(n) {\n\
                     Some(v) => v,\n\
                     None => return None,\n\
                 }\n\
                 Some(x + 1)\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = coalesce_rule_hits(&ws, "W_MANUAL_COALESCE");
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        let suggestion = hit[0].diag.suggestion.as_ref().expect("expected a `?`-Suggestion");
        assert_eq!(suggestion.replacement, "?", "got: {:?}", suggestion);
    }

    #[test]
    fn manual_coalesce_neg_is_ok_wildcard_not_identity() {
        // `Ok(_) => false` — wildcard pattern, not identity — this is
        // `.is_ok()`/`.is_err()`, NOT a coalesce drift. Must NOT fire.
        let src = "module foo\n\
             type E enum Bad\n\
             fn find(n int) -> Result[int, E] => if n > 0 { Ok(n) } else { Err(E.Bad) }\n\
             fn run(n int) -> bool {\n\
                 match find(n) {\n\
                     Ok(_) => true,\n\
                     Err(_) => false,\n\
                 }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            coalesce_rule_hits(&ws, "W_MANUAL_COALESCE").is_empty(),
            "must NOT fire on Ok(_) => bool (is_ok/is_err shape), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn manual_coalesce_neg_map_shape_not_identity() {
        // `Some(v) => v + 1` — arm is not the bare bound identifier — this
        // is `.map(f) ?? d`, NOT identity. Must NOT fire.
        let src = "module foo\n\
             fn find(n int) -> Option[int] => if n > 0 { Some(n) } else { None }\n\
             fn run(n int) -> int {\n\
                 match find(n) {\n\
                     Some(v) => v + 1,\n\
                     None => 0,\n\
                 }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            coalesce_rule_hits(&ws, "W_MANUAL_COALESCE").is_empty(),
            "must NOT fire on Some(v) => v + 1 (map shape), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn manual_coalesce_neg_different_names_not_identity() {
        // Success arm's body uses a DIFFERENT identifier than the pattern
        // binds — not identity. Must NOT fire.
        let src = "module foo\n\
             fn find(n int) -> Option[int] => if n > 0 { Some(n) } else { None }\n\
             fn run(n int) -> int {\n\
                 match find(n) {\n\
                     Some(v) => n,\n\
                     None => 0,\n\
                 }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            coalesce_rule_hits(&ws, "W_MANUAL_COALESCE").is_empty(),
            "must NOT fire when arm body uses a different name than the pattern binds, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn manual_coalesce_neg_glob_class_no_bridge_silent() {
        // `glob.nv`-class: fn returns `bool` (no Option/Result bridge) —
        // identity shape present, but `??` couldn't express this either
        // (Ф.2 would reject the same way) — explicit `match` is the ONLY
        // legal form here, so the lint must stay SILENT (owner: "молчание"),
        // not fire-with-a-note.
        let src = "module foo\n\
             fn find(n int) -> Option[int] => if n > 0 { Some(n) } else { None }\n\
             fn run(n int) -> bool {\n\
                 (match find(n) {\n\
                     Some(v) => v,\n\
                     None => return false,\n\
                 }) > 0\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert!(
            coalesce_rule_hits(&ws, "W_MANUAL_COALESCE").is_empty(),
            "must be SILENT on glob.nv-class (no Option/Result bridge), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn manual_coalesce_neg_fallback_references_bound_err_silent() {
        // Real-world regression (found migrating examples/flagship/aggregator
        // during Ф.4): `Err(e) => { st.close(); return Err(Wrap(e.to_str())) }`
        // — a Value-shape fallback (multi-stmt block, not bare `return`) that
        // FREELY references the bound error `e`. `??`'s desugar discards
        // `Err`'s payload entirely (`Err(_) => fallback`) — rewriting to
        // `X ?? D` would silently drop the reference to `e`, NOT an
        // equivalent rewrite. Must stay SILENT, not suggest a wrong canon.
        let src = "module foo\n\
             type E enum Bad\n\
             fn find(n int) -> Result[int, E] => if n > 0 { Ok(n) } else { Err(E.Bad) }\n\
             fn close() -> () => ()\n\
             fn run(n int) -> int {\n\
                 match find(n) {\n\
                     Ok(v) => v,\n\
                     Err(e) => {\n\
                         close()\n\
                         panic(\"boom\")\n\
                     },\n\
                 }\n\
             }\n";
        // Sibling fixture: same shape, but the fallback DOES reference the
        // bound error `e` (via `describe(e)`) — must suppress.
        let src2 = "module foo\n\
             type E enum Bad\n\
             fn find(n int) -> Result[int, E] => if n > 0 { Ok(n) } else { Err(E.Bad) }\n\
             fn describe(e E) -> str => \"err\"\n\
             fn run(n int) -> str {\n\
                 match find(n) {\n\
                     Ok(v) => v.to_str(),\n\
                     Err(e) => describe(e),\n\
                 }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        // Sanity: the minimal repro above (no reference to `e`) SHOULD still
        // fire (it's an ordinary value-fallback) — confirms the suppression
        // below is specifically about the `e`-reference, not a general
        // regression in Value-shape detection.
        assert_eq!(
            coalesce_rule_hits(&ws, "W_MANUAL_COALESCE").len(),
            1,
            "sanity: non-referencing Value-shape fallback must still fire, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );

        let m2 = parse(src2);
        let ws2 = run_conv_rules(Some(&m2), src2, &ConvLintOptions::default(), None);
        assert!(
            coalesce_rule_hits(&ws2, "W_MANUAL_COALESCE").is_empty(),
            "must be SILENT when the Value-shape fallback references the bound \
             error name (`??` cannot express that), got: {:?}",
            ws2.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    // ── Пункт 22 / Plan 200: W_MANUAL_COLLECT ([M-manual-collect-lint-missing])

    fn collect_hits(ws: &[LintWarning]) -> usize {
        coalesce_rule_hits(ws, "W_MANUAL_COLLECT").len()
    }

    #[test]
    fn manual_collect_pos_slice_sugar_new() {
        let src = "module foo\n\
             fn run(items []int) -> []int {\n\
                 mut v = []int.new()\n\
                 for x in items {\n\
                     v.push(x)\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        let hit = coalesce_rule_hits(&ws, "W_MANUAL_COLLECT");
        assert!(hit[0].diag.message.contains(".collect()"), "got: {}", hit[0].diag.message);
        let s = hit[0].diag.suggestion.as_ref().expect("suggestion");
        assert_eq!(s.replacement, "mut v = items.collect()", "got: {:?}", s);
        assert_eq!(s.applicability, Applicability::MachineApplicable);
    }

    #[test]
    fn manual_collect_pos_vec_turbofish_new() {
        let src = "module foo\n\
             fn run(items []int) -> []int {\n\
                 mut v = Vec[int].new()\n\
                 for x in items {\n\
                     v.push(x)\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_pos_empty_literal() {
        let src = "module foo\n\
             fn run(items []int) -> []int {\n\
                 mut v []int = []\n\
                 for x in items {\n\
                     v.push(x)\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_neg_push_mapped_value() {
        // `v.push(dbl(x))` — не identity (канон `.map(dbl).collect()`, вне V1).
        let src = "module foo\n\
             fn dbl(x int) -> int => x + x\n\
             fn run(items []int) -> []int {\n\
                 mut v = []int.new()\n\
                 for x in items {\n\
                     v.push(dbl(x))\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_neg_push_under_condition() {
        // `if x > 0 { v.push(x) }` — под условием (канон `.filter(...).collect()`).
        let src = "module foo\n\
             fn run(items []int) -> []int {\n\
                 mut v = []int.new()\n\
                 for x in items {\n\
                     if x > 0 {\n\
                         v.push(x)\n\
                     }\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_neg_push_wrong_receiver() {
        // push в `sink` (не свежий пустой ctor `v`) — не ручной collect.
        let src = "module foo\n\
             fn run(items []int, sink []int) -> () {\n\
                 mut v = []int.new()\n\
                 for x in items {\n\
                     sink.push(x)\n\
                 }\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_neg_non_empty_ctor() {
        // `mut v = [1, 2]` — НЕ пустой ctor.
        let src = "module foo\n\
             fn run(items []int) -> []int {\n\
                 mut v = [1, 2]\n\
                 for x in items {\n\
                     v.push(x)\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_neg_preallocated_new() {
        // `[]int.new(4)` — преаллокация, не «пустой» ctor.
        let src = "module foo\n\
             fn run(items []int) -> []int {\n\
                 mut v = []int.new(4)\n\
                 for x in items {\n\
                     v.push(x)\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_neg_multiple_body_stmts() {
        // Тело цикла — 2 statement'а, не РОВНО push.
        let src = "module foo\n\
             fn note(x int) -> () => ()\n\
             fn run(items []int) -> []int {\n\
                 mut v = []int.new()\n\
                 for x in items {\n\
                     note(x)\n\
                     v.push(x)\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_neg_stmt_between_decl_and_loop() {
        // Между объявлением и циклом есть statement — не «непосредственно перед».
        let src = "module foo\n\
             fn note() -> () => ()\n\
             fn run(items []int) -> []int {\n\
                 mut v = []int.new()\n\
                 note()\n\
                 for x in items {\n\
                     v.push(x)\n\
                 }\n\
                 v\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_collect_pos_inside_nested_block() {
        // Паттерн внутри вложенного `if`-блока — обход должен доставать.
        let src = "module foo\n\
             fn run(items []int, flag bool) -> []int {\n\
                 mut out []int = []\n\
                 if flag {\n\
                     mut v = []int.new()\n\
                     for x in items {\n\
                         v.push(x)\n\
                     }\n\
                     out = v\n\
                 }\n\
                 out\n\
             }\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(collect_hits(&ws), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    // ── Пункт 22 / Plan 200: W_MANUAL_SLICE_TO_END ([M-manual-slice-bounds-lint-missing])

    fn slice_hits(ws: &[LintWarning]) -> Vec<&LintWarning> {
        coalesce_rule_hits(ws, "W_MANUAL_SLICE_TO_END")
    }

    #[test]
    fn manual_slice_pos_end_len() {
        let src = "module foo\n\
             fn run(v []int, a int) -> []int => v[a..v.len()]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = slice_hits(&ws);
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        let s = hit[0].diag.suggestion.as_ref().expect("suggestion");
        assert_eq!(s.replacement, "", "reduction-1 deletes the end operand");
        assert_eq!(s.applicability, Applicability::MachineApplicable);
    }

    #[test]
    fn manual_slice_pos_end_byte_len_str() {
        let src = "module foo\n\
             fn run(s str, a int) -> str => s[a..s.byte_len()]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(slice_hits(&ws).len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_slice_pos_start_zero() {
        let src = "module foo\n\
             fn run(v []int, b int) -> []int => v[0..b]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = slice_hits(&ws);
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert_eq!(hit[0].diag.suggestion.as_ref().unwrap().replacement, "");
    }

    #[test]
    fn manual_slice_pos_both_zero_and_len() {
        let src = "module foo\n\
             fn run(v []int) -> []int => v[0..v.len()]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        let hit = slice_hits(&ws);
        assert_eq!(hit.len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
        assert_eq!(hit[0].diag.suggestion.as_ref().unwrap().replacement, "..", "reduction-3 → `[..]`");
    }

    #[test]
    fn manual_slice_pos_field_receiver() {
        let src = "module foo\n\
             type Buf { mut data []int }\n\
             fn Buf @tail(a int) -> []int => @data[a..@data.len()]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(slice_hits(&ws).len(), 1, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_slice_neg_different_receiver() {
        let src = "module foo\n\
             fn run(x []int, y []int, a int) -> []int => x[a..y.len()]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(slice_hits(&ws).len(), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_slice_neg_arithmetic_end() {
        let src = "module foo\n\
             fn run(x []int, a int) -> []int => x[a..x.len() - 1]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(slice_hits(&ws).len(), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_slice_neg_plain_bounds() {
        let src = "module foo\n\
             fn run(x []int, a int, b int) -> []int => x[a..b]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(slice_hits(&ws).len(), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }

    #[test]
    fn manual_slice_neg_inclusive_range() {
        // Инклюзивный `..=` НЕ трогаем (редукция для len была бы OOB).
        let src = "module foo\n\
             fn run(x []int, b int) -> []int => x[0..=b]\n";
        let m = parse(src);
        let ws = run_conv_rules(Some(&m), src, &ConvLintOptions::default(), None);
        assert_eq!(slice_hits(&ws).len(), 0, "got: {:?}", ws.iter().map(|w| w.rule).collect::<Vec<_>>());
    }
}

// ============================================================================
// Plan 110.9.4 — W_FFI_CANCEL_UNSAFE unit tests.
// ============================================================================

#[cfg(test)]
mod cancel_unsafe_tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::Parser;

    fn parse(src: &str) -> Module {
        let toks = lex(src).unwrap();
        let mut p = Parser::new(toks);
        p.parse_module().unwrap()
    }

    #[test]
    fn warns_on_external_fn_without_cancel_safe_in_on_exit() {
        let m = parse(
            "module foo\n\
             extern \"nova\" fn native_close(h int) -> int\n\
             type Conn { ro h int }\n\
             fn Conn consume @cleanup(_outcome ScopeOutcome) -> () {\n\
                 ro _r = native_close(@h)\n\
                 return ()\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            ws.iter().any(|w| w.rule == "W_FFI_CANCEL_UNSAFE"),
            "expected W_FFI_CANCEL_UNSAFE warning, got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_external_fn_with_cancel_safe_in_on_exit() {
        let m = parse(
            "module foo\n\
             #cancel_safe\n\
             extern \"nova\" fn native_close(h int) -> int\n\
             type Conn { ro h int }\n\
             fn Conn consume @cleanup(_outcome ScopeOutcome) -> () {\n\
                 ro _r = native_close(@h)\n\
                 return ()\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "W_FFI_CANCEL_UNSAFE"),
            "no W_FFI_CANCEL_UNSAFE expected (cancel_safe attestation), got: {:?}",
            ws.iter().map(|w| w.rule).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_external_fn_call_outside_on_exit() {
        let m = parse(
            "module foo\n\
             extern \"nova\" fn native_close(h int) -> int\n\
             fn regular_fn(h int) -> int => native_close(h)\n",
        );
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "W_FFI_CANCEL_UNSAFE"),
            "external fn call outside cleanup must be silent"
        );
    }

    #[test]
    fn no_warning_on_plain_nova_fn_in_on_exit() {
        let m = parse(
            "module foo\n\
             fn plain_close(h int) -> int => h\n\
             type Conn { ro h int }\n\
             fn Conn consume @cleanup(_outcome ScopeOutcome) -> () {\n\
                 ro _r = plain_close(@h)\n\
                 return ()\n\
             }\n",
        );
        let ws = lint_module(&m);
        assert!(
            !ws.iter().any(|w| w.rule == "W_FFI_CANCEL_UNSAFE"),
            "plain Nova fn call from cleanup must be silent (not FFI)"
        );
    }
}
