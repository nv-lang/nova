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
    ArrayElem, Block, ClosureBody, ElseBranch, Expr, ExprKind, FnBody, FnDecl,
    HandlerMethodBody, Item, MatchArmBody, Module, Stmt, TypeDeclKind, TypeRef,
};
use crate::diag::{Diagnostic, Span};
use std::collections::HashSet;

/// Один lint-warning.
#[derive(Debug, Clone)]
pub struct LintWarning {
    pub rule: &'static str,
    pub diag: Diagnostic,
}

/// Прогон всех lint-проверок на модуле. Возвращает список warning'ов.
pub fn lint_module(m: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    let effect_names = collect_effect_names(m);
    let protocol_names = collect_protocol_names(m);
    for item in &m.items {
        match item {
            Item::Fn(f) => {
                check_fn(f, &mut warnings);
                check_assume_trust(f, &mut warnings);
                check_assert_static_unverified(f, &mut warnings);
                check_protocol_in_effect_position(f, &protocol_names, &effect_names, &mut warnings);
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
    warnings
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
                     Suppress: add `allow_prelude_shadow` clause to module \
                     declaration, or switch to `no_prelude` / \
                     `partial_prelude(...)` (Plan 62.F).",
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
        Stmt::Assign { target, value, .. } => {
            walk_expr_lints(target, out);
            walk_expr_lints(value, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { walk_expr_lints(v, out); }
        }
        Stmt::Throw { value, .. } => walk_expr_lints(value, out),
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => walk_expr_lints(body, out),
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => walk_expr_lints(expr, out),
        // Plan 33.3 Ф.13: Apply/Calc — proof-statements, spec-only.
        Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
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
        ExprKind::Try(x) | ExprKind::Bang(x) => walk_expr_lints(x, out),
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
        ExprKind::Detach(b) => walk_block_lints(b, out),
        ExprKind::Supervised { body, cancel } => {
            walk_block_lints(body, out);
            if let Some(c) = cancel { walk_expr_lints(c, out); }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            walk_block_lints(body, out);
        }
        ExprKind::Throw(x) => walk_expr_lints(x, out),
        ExprKind::Interrupt(opt) => {
            if let Some(x) = opt { walk_expr_lints(x, out); }
        }
        ExprKind::Range { start, end, .. } => {
            walk_expr_lints(start, out); walk_expr_lints(end, out);
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let crate::ast::InterpStrPart::Expr(x) = p { walk_expr_lints(x, out); }
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
        ExprKind::HandlerLit { methods, .. } => {
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
        | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit => {}
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
            if matches!(td.kind, TypeDeclKind::Protocol(_)) {
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
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
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
        // Hashable — встроенный protocol; в effect-position warning.
        let m = parse("module foo\nfn process(x int) Hashable -> int => x\n");
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
            let items: Vec<Item> = name_decls.iter().map(|n| Item::Type(TypeDecl {
                doc: None,
                doc_attrs: Vec::new(),
                is_export: true,
                name: (*n).to_string(),
                generics: Vec::new(),
                kind: TypeDeclKind::Record(Vec::new()),
                span: Span::dummy(),
                attrs: Vec::new(),
                invariants: Vec::new(),
                axioms: Vec::new(),
            })).collect();
            PeerFile {
                path: PathBuf::from("/synthetic/std/prelude/core.nv"),
                file_id: FileId::from(42_u32),
                imports: Vec::new(),
                items_here: items,
                imported_item_names: HashSet::new(),
                is_entry_module: false,
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
            // Module clause `allow_prelude_shadow` парсится → ModuleAttrKind.
            let mut m = parse("module myapp allow_prelude_shadow\ntype Option { foo int }\n");
            add_fake_prelude(&mut m, &["Option"]);
            let ws = lint_prelude_shadow(&m);
            assert!(ws.is_empty(), "suppress should silence W_PRELUDE_SHADOW, got {:?}", ws);
        }

        #[test]
        fn no_prelude_does_not_suppress_explicit_shadow_lint() {
            // `no_prelude` отключает auto-import — visibility set пуст,
            // shadowing невозможен → no warning естественно.
            let mut m = parse("module myapp no_prelude\ntype Option { foo int }\n");
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
}
