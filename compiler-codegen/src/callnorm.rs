//! Plan 46 (D102) Ф.2: call-args normalization pass.
//!
//! Переписывает call-site с именованными аргументами / опущенными
//! параметрами-с-дефолтами в **чистый позиционный** вызов, чтобы codegen
//! не знал про `CallArg::Named` и `Param.default`.
//!
//! ## Стратегия — двухфазный Block
//!
//! Вызов `slice(to: g(), xs: h())` где
//! `fn slice(xs []int, from int = 0, to int = xs.len())` переписывается в:
//!
//! ```text
//! {
//!     let __nova_arg_src0 = g()        // explicit args — source-order eval
//!     let __nova_arg_src1 = h()
//!     let xs   = __nova_arg_src1       // param-binding (param-order)
//!     let from = 0                     // default
//!     let to   = xs.len()              // default — видит `xs` (let выше)
//!     slice(xs, from, to)              // call в param-order
//! }
//! ```
//!
//! - **Фаза 1:** explicit args → `let __nova_arg_src<k>` в source-order
//!   (порядок side-эффектов = порядок аргументов на call-site, D102).
//! - **Фаза 2:** `let <param_name> = ...` в param-order. Имена биндингов
//!   = имена параметров → default-выражения резолвятся естественно,
//!   без substitution walk.
//! - **Call:** `callee(<param_name>...)` в param-order.
//!
//! Если binding — уже чистый позиционный по порядку без дефолтов, Call
//! не трогается (no Block overhead).

use crate::ast::*;
use crate::argbind::{bind_call_args, ArgBinding};
use crate::diag::Span;
use std::collections::HashMap;

/// Сигнатуры callee, доступные для нормализации.
struct Sigs {
    /// Free functions по имени. Только unambiguous (1 overload) —
    /// при overload нормализация пропускается (D102: overload нет, но
    /// bootstrap fn_decls может иметь несколько).
    free: HashMap<String, Vec<Param>>,
    /// Static-методы по `(type, method)`. [M-vec-new-static-arity-overload]
    /// fix: carries ALL overload signatures (was: filtered to unambiguous
    /// `v.len() == 1` and skipped otherwise) — `pick_static_params` below
    /// disambiguates by arity/bind-success at each call-site, so a genuine
    /// arity-overload (`Vec[T].new(cap int=0)` 0/1-arg vs
    /// `Vec[T].new(ptr,len,cap)` 3-arg) still gets its DEFAULT backfilled
    /// instead of unconditionally bailing out of normalization.
    static_methods: HashMap<(String, String), Vec<Vec<Param>>>,
    /// Plan 46 Ф.3: instance-методы по имени метода. Резолв `obj.method`
    /// без type-inference: если имя метода уникально (один тип, один
    /// overload) — нормализуем. Иначе — пропускаем (codegen резолвит
    /// через type-info).
    instance_by_name: HashMap<String, Vec<Param>>,
    /// [M-set-from-iter-self-new-default-arg-backfill] (Plan 196 Ф.C):
    /// enclosing-type имя текущего `Item::Fn`, для резолва bare `Self` как
    /// receiver'а (`Self.new()` внутри тела generic-static/instance метода
    /// — например `Set[T].from_iter`'s `Self.new()`). Обновляется
    /// `normalize_item` ПЕРЕД walk тела; `RefCell` — дешевле, чем протаскивать
    /// `self_type: Option<&str>` параметром через все ~30 normalize_* функций
    /// (Sigs уже общий `&Sigs` на весь module-pass, items обрабатываются
    /// строго последовательно, так что интерьерная мутация безопасна).
    self_type: std::cell::RefCell<Option<String>>,
    /// 196.5 Stage-D волна-5 (facet C, callnorm/argbind карта): every
    /// `Item::Fn`'s OWN declaration span → its params, so a call-site can be
    /// resolved to the EXACT decl the checker already picked
    /// (`resolved_callees[call.id] → decl span → by_span[span]`) instead of
    /// re-deriving a candidate through the coarse per-form name/arity
    /// heuristics below (`free`/`static_methods`/`instance_by_name` all
    /// independently re-resolve overload identity from scratch, with NO type
    /// information — `instance_by_name` in particular drops ANY method name
    /// shared by ≥2 types in the whole module, `pick_static_params` picks by
    /// arity/bind-success alone, blind to argument TYPES). Keyed by the same
    /// `f.span` the checker inserts into `resolved_callees` at
    /// `types/mod.rs` (`self.resolved_callees.borrow_mut().insert(call_id,
    /// f.span)` / `chosen.span` / `callee.span`).
    by_span: HashMap<Span, Vec<Param>>,
    /// Checker's authoritative call→declaration resolution
    /// (`ModuleEnv.resolved_callees`, populated by `check_module` BEFORE
    /// this pass runs — `types::check_module` → `normalize_module` order is
    /// fixed in every pipeline caller). `None`-keyed (missing entry) is
    /// honest: the checker had no unambiguous pick for that call-site
    /// (0 or ≥2 type-compatible overloads, or a call-shape the checker's
    /// resolver does not cover, e.g. an erased generic-mono body) — falls
    /// through to the pre-existing coarse heuristics, UNCHANGED.
    resolved_callees: HashMap<ExprId, Span>,
}

/// Plan 46 Ф.2: нормализовать все call-site в модуле.
/// Вызывается ПОСЛЕ resolve_imports_inline (нужны все сигнатуры) и
/// type-check, ПЕРЕД codegen.
///
/// `resolved_callees` — checker's `ModuleEnv.resolved_callees` (196.5
/// Stage-D волна-5, facet C): when a call-site's `ExprId` has an entry
/// here, `try_normalize_call` resolves its params from the EXACT decl the
/// checker picked instead of re-deriving a candidate through the coarse
/// per-form heuristics. Pass an empty map from a caller that has no
/// `ModuleEnv` available (pre-existing coarse behavior, unchanged).
pub fn normalize_module(module: &mut Module, resolved_callees: &HashMap<ExprId, Span>) {
    let sigs = collect_sigs(module, resolved_callees);
    for item in &mut module.items {
        normalize_item(item, &sigs);
    }
}

fn collect_sigs(module: &Module, resolved_callees: &HashMap<ExprId, Span>) -> Sigs {
    let mut free: HashMap<String, Vec<Vec<Param>>> = HashMap::new();
    let mut static_methods: HashMap<(String, String), Vec<Vec<Param>>> = HashMap::new();
    // instance: по имени метода → список сигнатур (со всех типов).
    // Уникальное имя (1 запись) → нормализуем; иначе skip.
    let mut instance: HashMap<String, Vec<Vec<Param>>> = HashMap::new();
    let mut by_span: HashMap<Span, Vec<Param>> = HashMap::new();
    for item in &module.items {
        if let Item::Fn(f) = item {
            // 196.5 Stage-D волна-5: index EVERY decl by its own span,
            // regardless of receiver kind — the channel fast path below
            // resolves through this, bypassing the free/static/instance
            // split entirely (that split only serves the coarse fallback).
            by_span.insert(f.span, f.params.clone());
            match &f.receiver {
                None => free.entry(f.name.clone()).or_default().push(f.params.clone()),
                Some(recv) if recv.kind == ReceiverKind::Static => {
                    static_methods
                        .entry((recv.type_name.clone(), f.name.clone()))
                        .or_default()
                        .push(f.params.clone());
                }
                // Plan 46 Ф.3: instance-методы — собираем по имени.
                Some(_) => {
                    instance.entry(f.name.clone()).or_default().push(f.params.clone());
                }
            }
        }
    }
    // Берём только unambiguous (1 запись).
    let free = free.into_iter()
        .filter_map(|(k, mut v)| if v.len() == 1 { Some((k, v.remove(0))) } else { None })
        .collect();
    // [M-vec-new-static-arity-overload] fix: keep ALL overload signatures
    // (was: `filter_map` dropping any (type, method) with >1 registered
    // signature) — the ambiguity is now resolved per call-site by arity in
    // `pick_static_params`, not by refusing to normalize at all.
    let static_methods = static_methods;
    let instance_by_name = instance.into_iter()
        .filter_map(|(k, mut v)| if v.len() == 1 { Some((k, v.remove(0))) } else { None })
        .collect();
    Sigs {
        free, static_methods, instance_by_name,
        self_type: std::cell::RefCell::new(None),
        by_span,
        resolved_callees: resolved_callees.clone(),
    }
}

fn normalize_item(item: &mut Item, sigs: &Sigs) {
    match item {
        Item::Fn(f) => {
            // [M-set-from-iter-self-new-default-arg-backfill]: bare `Self`
            // resolves against the ENCLOSING type of this fn (None for a
            // free fn — `Self` isn't legal there anyway). Must be set
            // before walking params/body below.
            *sigs.self_type.borrow_mut() =
                f.receiver.as_ref().map(|r| r.type_name.clone());
            // Default-выражения параметров тоже могут содержать вызовы.
            for p in &mut f.params {
                if let Some(d) = &mut p.default {
                    normalize_expr(d, sigs);
                }
            }
            match &mut f.body {
                FnBody::Expr(e) => normalize_expr(e, sigs),
                FnBody::Block(b) => normalize_block(b, sigs),
                FnBody::External => {}
            }
        }
        Item::Test(t) => {
            // Top-level items — `Self` isn't legal outside a type method
            // body; reset so a stale enclosing-type from a PRIOR Item::Fn
            // (module.items is a flat, sequential list) can't leak in.
            *sigs.self_type.borrow_mut() = None;
            normalize_block(&mut t.body, sigs)
        }
        // Plan 57: bench setup/measure_body/teardown — все три раздела
        // обычные блоки statement'ов, требуют такой же нормализации
        // вызовов, как test body.
        Item::Bench(b) => {
            *sigs.self_type.borrow_mut() = None;
            for s in &mut b.setup {
                normalize_stmt(s, sigs);
            }
            normalize_block(&mut b.measure_body, sigs);
            for s in &mut b.teardown {
                normalize_stmt(s, sigs);
            }
        }
        Item::Const(c) => {
            *sigs.self_type.borrow_mut() = None;
            normalize_expr(&mut c.value, sigs)
        }
        Item::Let(l) => {
            *sigs.self_type.borrow_mut() = None;
            normalize_expr(&mut l.value, sigs)
        }
        Item::Type(_) => {}
        // Ф.4.1: lemma не emit'ится в runtime — нормализацию пропускаем.
        Item::Lemma(_) => {}
    }
}

fn normalize_block(b: &mut Block, sigs: &Sigs) {
    for s in &mut b.stmts {
        normalize_stmt(s, sigs);
    }
    if let Some(t) = &mut b.trailing {
        normalize_expr(t, sigs);
    }
}

fn normalize_stmt(s: &mut Stmt, sigs: &Sigs) {
    match s {
        Stmt::Expr(e) => normalize_expr(e, sigs),
        Stmt::Let(d) => normalize_expr(&mut d.value, sigs),
        Stmt::Const(d) => normalize_expr(&mut d.value, sigs),
        Stmt::Assign { target, value, .. } => {
            normalize_expr(target, sigs);
            normalize_expr(value, sigs);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { normalize_expr(v, sigs); }
        }
        Stmt::Throw { value, .. } => normalize_expr(value, sigs),
        Stmt::Defer { body, .. } => normalize_expr(body, sigs),
        // Plan 110 D188: consume X = init() { body } — walk init expr +
        // body block (stmts + trailing).
        Stmt::ConsumeScope { init, body, .. } => {
            normalize_expr(init, sigs);
            for stmt in &mut body.stmts {
                normalize_stmt(stmt, sigs);
            }
            if let Some(t) = &mut body.trailing {
                normalize_expr(t, sigs);
            }
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => normalize_expr(expr, sigs),
        Stmt::Break(_) | Stmt::Continue(_) => {}
        // Ф.4.1: apply — ghost, аргументы нормализуем.
        Stmt::Apply { args, .. } => {
            for a in args { normalize_expr(a, sigs); }
        }
        // Ф.4.2: calc — ghost, выражения шагов нормализуем.
        Stmt::Calc { steps, .. } => {
            for step in steps { normalize_expr(&mut step.expr, sigs); }
        }
        // Plan 33.9: reveal — ghost, no exprs to normalize.
        Stmt::Reveal { .. } => {}
        // Plan 136: tuple destructuring assignment — walk all lhs + rhs exprs.
        Stmt::TupleAssign { lhs, rhs, .. } => {
            for e in lhs { normalize_expr(e, sigs); }
            for e in rhs { normalize_expr(e, sigs); }
        }
    }
}

/// Рекурсивный walk по Expr: сначала нормализуем под-выражения, потом
/// сам Call (bottom-up — вложенные Call'ы уже нормализованы).
fn normalize_expr(e: &mut Expr, sigs: &Sigs) {
    // 1. Рекурсия в под-выражения.
    walk_children(e, sigs);
    // 2. Если это Call — попробовать нормализовать.
    if let ExprKind::Call { .. } = &e.kind {
        if let Some(new_kind) = try_normalize_call(e, sigs) {
            e.kind = new_kind;
        }
    }
}

/// Рекурсия в дочерние выражения (без обработки самого Call).
fn walk_children(e: &mut Expr, sigs: &Sigs) {
    match &mut e.kind {
        ExprKind::Call { func, args, trailing } => {
            normalize_expr(func, sigs);
            for a in args.iter_mut() {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => normalize_expr(x, sigs),
                    CallArg::Named { value, .. } => normalize_expr(value, sigs),
                }
            }
            if let Some(t) = trailing {
                normalize_trailing(t, sigs);
            }
        }
        ExprKind::TurboFish { base, .. } => normalize_expr(base, sigs),
        ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => normalize_expr(x, sigs),
        ExprKind::Coalesce(a, b) => { normalize_expr(a, sigs); normalize_expr(b, sigs); }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => normalize_expr(x, sigs),
        ExprKind::Binary { left, right, .. } => {
            normalize_expr(left, sigs); normalize_expr(right, sigs);
        }
        ExprKind::Unary { operand, .. } => normalize_expr(operand, sigs),
        ExprKind::Member { obj, .. } => normalize_expr(obj, sigs),
        ExprKind::Index { obj, index } => {
            normalize_expr(obj, sigs); normalize_expr(index, sigs);
        }
        ExprKind::If { cond, then, else_ } => {
            normalize_expr(cond, sigs);
            normalize_block(then, sigs);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => normalize_block(b, sigs),
                    ElseBranch::If(x) => normalize_expr(x, sigs),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            normalize_expr(scrutinee, sigs);
            normalize_block(then, sigs);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => normalize_block(b, sigs),
                    ElseBranch::If(x) => normalize_expr(x, sigs),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            normalize_expr(scrutinee, sigs);
            for arm in arms.iter_mut() {
                if let Some(g) = &mut arm.guard { normalize_expr(g, sigs); }
                match &mut arm.body {
                    MatchArmBody::Expr(x) => normalize_expr(x, sigs),
                    MatchArmBody::Block(b) => normalize_block(b, sigs),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            normalize_expr(iter, sigs); normalize_block(body, sigs);
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            normalize_expr(iter, sigs); normalize_block(body, sigs);
        }
        ExprKind::While { cond, body, .. } => {
            normalize_expr(cond, sigs); normalize_block(body, sigs);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            normalize_expr(scrutinee, sigs); normalize_block(body, sigs);
        }
        ExprKind::Loop { body, .. } => normalize_block(body, sigs),
        ExprKind::Block(b) => normalize_block(b, sigs),
        ExprKind::Spawn(x) => normalize_expr(x, sigs),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => normalize_block(b, sigs),
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            normalize_block(body, sigs);
            if let Some(c) = cancel { normalize_expr(c, sigs); }
            if let Some(_dl) = deadline { normalize_expr(&mut _dl.expr, sigs); }
            if let Some(oh) = on_timeout { normalize_expr(oh, sigs); }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            normalize_block(body, sigs)
        }
        ExprKind::Throw(x) => normalize_expr(x, sigs),
        ExprKind::CoalesceReturnFallback(opt) => {
            if let Some(x) = opt { normalize_expr(x, sigs); }
        }
        ExprKind::Interrupt(opt) => {
            if let Some(x) = opt { normalize_expr(x, sigs); }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { normalize_expr(s, sigs); }
            if let Some(e) = end { normalize_expr(e, sigs); }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems.iter_mut() {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => normalize_expr(x, sigs),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for me in elems.iter_mut() {
                match me {
                    crate::ast::MapElem::Pair(k, v) => {
                        normalize_expr(k, sigs);
                        normalize_expr(v, sigs);
                    }
                    crate::ast::MapElem::Spread(e) => normalize_expr(e, sigs),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for x in elems.iter_mut() { normalize_expr(x, sigs); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields.iter_mut() {
                if let Some(v) = &mut f.value { normalize_expr(v, sigs); }
            }
        }
        ExprKind::InterpolatedStr { parts } => {
            for p in parts.iter_mut() {
                if let InterpStrPart::Expr { expr: x, spec: _ } = p { normalize_expr(x, sigs); }
            }
        }
        ExprKind::TaggedTemplate { args, .. } => {
            for x in args.iter_mut() { normalize_expr(x, sigs); }
        }
        ExprKind::Lambda { body, .. } => normalize_expr(body, sigs),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(x) => normalize_expr(x, sigs),
            ClosureBody::Block(b) => normalize_block(b, sigs),
        },
        ExprKind::ClosureFull(sb) => match &mut sb.body {
            FnBody::Expr(x) => normalize_expr(x, sigs),
            FnBody::Block(b) => normalize_block(b, sigs),
            FnBody::External => {}
        },
        ExprKind::With { bindings, body } => {
            for b in bindings.iter_mut() { normalize_expr(&mut b.handler, sigs); }
            normalize_block(body, sigs);
        }
        // Plan 97 Ф.4 (D142): protocol-литерал — call-нормализация
        // тел методов идентична handler-литералу.
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods.iter_mut() {
                match &mut m.body {
                    HandlerMethodBody::Expr(x) => normalize_expr(x, sigs),
                    HandlerMethodBody::Block(b) => normalize_block(b, sigs),
                }
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms.iter_mut() {
                match &mut arm.op {
                    SelectOp::Recv { chan, .. } => normalize_expr(chan, sigs),
                    SelectOp::Send { chan, value } => {
                        normalize_expr(chan, sigs); normalize_expr(value, sigs);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &mut arm.guard { normalize_expr(g, sigs); }
                normalize_block(&mut arm.body, sigs);
            }
        }
        // Листовые — нет под-выражений.
        ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
        | ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
        | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
        | ExprKind::HexBlobLit(_) | ExprKind::NullPtrLit => {}
        // D.1.3: квантор — только в контрактах, не в runtime-коде.
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            normalize_expr(range, sigs);
            normalize_expr(body, sigs);
        }
    }
}

fn normalize_trailing(t: &mut Trailing, sigs: &Sigs) {
    match t {
        Trailing::Block(b) => normalize_block(b, sigs),
        Trailing::LegacyBlockWithParams(tb) => normalize_block(&mut tb.body, sigs),
        Trailing::Fn(sb) => match &mut sb.body {
            FnBody::Expr(x) => normalize_expr(x, sigs),
            FnBody::Block(b) => normalize_block(b, sigs),
            FnBody::External => {}
        },
    }
}

/// [M-vec-new-static-arity-overload] fix: pick the ONE overload signature
/// (from `candidates`, all `(type, method)` overloads collected by
/// `collect_sigs`) that this call-site's `args` can bind against —
/// disambiguates arity-overloaded static ctors (e.g. `Vec[T].new(cap
/// int=0)` 0/1-arg vs `Vec[T].new(ptr,len,cap)` 3-arg) so default-arg
/// backfill still fires for the correct candidate instead of bailing out
/// whenever a static method has more than one registered signature.
/// Fast path (`candidates.len() == 1`) is BYTE-IDENTICAL to the prior
/// unconditional lookup. `None` (no candidate binds, or ≥2 candidates
/// bind — genuinely ambiguous) leaves the call untouched, same as before
/// — codegen resolves it via full type-info (arity + param-C-type +
/// checker's `resolved_callees`).
fn pick_static_params<'a>(
    candidates: &'a [Vec<Param>],
    args: &[CallArg],
    trailing_present: bool,
) -> Option<&'a [Param]> {
    if candidates.len() == 1 {
        return Some(&candidates[0]);
    }
    let mut matched: Option<&'a [Param]> = None;
    for cand in candidates {
        let effective: &[Param] = if trailing_present && !cand.is_empty() {
            &cand[..cand.len() - 1]
        } else {
            cand
        };
        if bind_call_args(effective, args).is_ok() {
            if matched.is_some() {
                return None; // ≥2 candidates bind — genuinely ambiguous, skip.
            }
            matched = Some(cand.as_slice());
        }
    }
    matched
}

/// Попытаться нормализовать `Call`. Возвращает `Some(new_kind)` если
/// переписали (в Block-expr), `None` если оставили как есть.
fn try_normalize_call(e: &Expr, sigs: &Sigs) -> Option<ExprKind> {
    let ExprKind::Call { func, args, trailing } = &e.kind else { return None; };

    // Резолвим callee params.
    let base: &Expr = match &func.kind {
        ExprKind::TurboFish { base, .. } => base,
        _ => func.as_ref(),
    };
    // [M-196-method-turbofish-block-rewrite-ice] (Plan 196 Facet C, wave
    // continuation): an EXPLICIT method-level-generic turbofish call —
    // `obj.method[U,...](...)` (Plan 91.1 `M-91.1-method-turbofish-dispatch`)
    // — parses to a TOP-level `TurboFish{base: Member{obj, name}}`: the
    // structural MIRROR-IMAGE of the generic-static-ctor shape handled below
    // (`Member{obj: TurboFish{base: Ident(Type)}}`, TurboFish one level
    // INSIDE `obj`). The unwrap just above strips this top-level TurboFish so
    // `base.kind` is a plain `Member`, and the heuristics below (in
    // particular `sigs.instance_by_name`) then happily resolve+rewrite it
    // like an ordinary instance call, discarding no information syntactically
    // — but codegen's method-turbofish return/mono resolution for THIS
    // receiver shape (`infer_call_ret_c` → `resolve_instance_call_subst`,
    // Plan 91.1/172.1, outside this pass's zone) infers each arg's C type
    // EAGERLY (`infer_expr_c_type`) to bind the method's own generic
    // parameter(s), and does so BEFORE this pass's synthesized two-phase
    // Block (fresh `let <param> = ...` locals, fresh `ExprId::UNSET` Ident
    // refs) has been emitted — so `var_types` has no entry yet for the
    // synthesized name and the checker never annotated it either (it never
    // existed pre-rewrite) → `[P67-LEGACY] Ident '<param>' not in var_types`
    // ICE (compiler-conventions.md §0), not a wrong answer. Repro: `type
    // Box[T]{slot T} fn Box[T] @wrap[U](tag U, note str = "n") -> str` called
    // `b.wrap[str]("hi")` (0-arg omitted default `note`) — crashes; the SAME
    // call WITHOUT the explicit turbofish (`b.wrap("hi")`, U inferred
    // bidirectionally, D119) desugars and runs correctly (proven by
    // `spec_tests/conformance/m196_facetc_instance_collision_and_method_
    // generic_default.nv`), so the gap is narrowly this AST shape, not
    // default-arg backfill in general. Until codegen's
    // method-turbofish dispatch threads a channel this pass's synthesized
    // locals can satisfy (owned by the frozen `infer_call_ret_c`/
    // `resolve_instance_call_subst` zone, not this one), leave calls in this
    // shape UNTOUCHED — same "not covered" bucket as the bottom `_ => return
    // None` (codegen sees the original args raw: an omitted default becomes
    // a plain arity mismatch — a diagnosed compile error, never a crash).
    if matches!(func.kind, ExprKind::TurboFish { .. }) && matches!(base.kind, ExprKind::Member { .. }) {
        return None;
    }
    // [M-vec-new-cap-default-arg-backfill] fix: a GENERIC static-ctor call —
    // `Type[Args].method(...)` or the D38/D239 slice-sugar `[]T.method(...)`
    // — parses to `Member{obj, name}` too (the `TurboFish`/`__array`-Path
    // marker lives one level down in `obj`, NOT at `func.kind` directly —
    // the unwrap above only strips a TOP-level TurboFish, e.g. a free-fn
    // turbofish call `f[T](...)`). Previously such calls fell straight into
    // the instance-method lookup below (`sigs.instance_by_name.get(name)`),
    // which almost never has an entry for a ctor name like "new" →
    // `try_normalize_call` silently gave up and NO default-arg backfill
    // happened for these call-sites. `is_static_generic_recv` records
    // whether `base` is one of these two static-generic shapes so the
    // Block-building below (Plan 46 Ф.3 receiver-hoist) does NOT try to
    // evaluate the TYPE expression `obj` as a VALUE — that hoist is only
    // correct for genuine instance receivers.
    let mut is_static_generic_recv = false;
    // 196.5 Stage-D волна-5 (facet C point-fix): if the CHECKER already
    // resolved this exact call-site to one declaration (`resolved_callees`,
    // populated by `check_module` BEFORE this pass), read its params
    // straight from `by_span` — a strict superset of every per-form
    // heuristic below (`free`/`static_methods`/`instance_by_name`), all of
    // which re-derive candidate identity from scratch with LESS
    // information than the checker already had (arity/bind-success only,
    // no argument types) and — for `instance_by_name` — drop the call
    // entirely whenever ≥2 types in the module share the method name. When
    // present this is BYTE-IDENTICAL to a correct heuristic pick (same
    // decl) and a STRICT FIX for the cases the heuristics silently skip
    // (overloaded free-fn, cross-type instance-method name collision,
    // arity-ambiguous static overload). Absent (checker had no unambiguous
    // pick, or `e.id` unset) → falls through to the unchanged heuristics.
    let channel_params: Option<&[Param]> = if e.id.is_set() {
        sigs.resolved_callees
            .get(&e.id)
            .and_then(|sp| sigs.by_span.get(sp))
            .map(|v| v.as_slice())
    } else {
        None
    };
    let params: &[Param] = if let Some(p) = channel_params {
        // Receiver-hoist shape flag (Ф.3 below) is purely SYNTACTIC — it
        // depends on whether `obj` is a type-expression (TurboFish /
        // `[]T`-sugar) or a genuine value receiver, independent of which
        // params-source resolved this call. Mirrors the shape test the
        // heuristic `Member` arm performs below, computed unconditionally
        // here since the channel fast path skips that arm entirely.
        if let ExprKind::Member { obj, .. } = &base.kind {
            is_static_generic_recv = matches!(&obj.kind, ExprKind::TurboFish { .. })
                || matches!(
                    &obj.kind,
                    ExprKind::Path(bparts) if bparts.len() == 2 && bparts[0] == "__array"
                );
        }
        p
    } else {
        match &base.kind {
        ExprKind::Ident(name) => sigs.free.get(name)?,
        // [M-set-from-iter-self-new-default-arg-backfill] (Plan 196 Ф.C):
        // `Self.new()` written INSIDE a generic-static/instance method body
        // (e.g. `Set[T].from_iter`'s `mut s = Self.new()`) parses to this
        // SAME `Path(parts.len()==2)` shape as an ordinary `Type.method()`
        // static call (confirmed empirically — a capitalized-leading dotted
        // 2-segment chain collapses to `Path`, never `Member{obj: Ident}`,
        // so `Self` is just `parts[0]` here). The bug: `parts[0]` was used
        // LITERALLY as the type-name lookup key, but `"Self"` is never a
        // real entry in `static_methods` (keyed by the true declared type,
        // e.g. "Set") → the `?` on the `.get()` bailed the WHOLE function,
        // silently skipping default-arg backfill for every `Self.method()`
        // 0-arg/omitted-default call site. Fix: resolve `Self` against
        // `sigs.self_type` (the ENCLOSING type of the fn being walked, set
        // by `normalize_item` from `f.receiver.type_name`) before the
        // `static_methods` probe — same table, same non-generic-static path
        // used for every other `Type.method()` call, just with `Self`
        // substituted to its real name first.
        ExprKind::Path(parts) if parts.len() == 2 => {
            // [merge Ф.C + П5]: Self-резолв (enclosing type) И overload-aware
            // дизамбигуация — обе способности нужны.
            let type_name = if parts[0] == "Self" {
                sigs.self_type.borrow().clone()?
            } else {
                parts[0].clone()
            };
            let cands = sigs.static_methods.get(&(type_name, parts[1].clone()))?;
            pick_static_params(cands, args, trailing.is_some())?
        }
        // `obj` may itself be the SAME two type-position shapes handled by
        // the `Path`/`TurboFish` arms above — a generic receiver written
        // `Type[Args].method(...)` parses to `Member{obj: TurboFish{base:
        // Ident(Type), ..}, name}` (the TurboFish nests one level INSIDE
        // `obj` here, so the unwrap at the top of this fn does not see it),
        // and the D38/D239 slice-sugar `[]T.method(...)` parses to
        // `Member{obj: Path(["__array", elem]), name}` (`[]T` ≡ `Vec[T]`,
        // same "Vec" key the checker already normalizes to elsewhere —
        // `types/mod.rs`, `path[0].starts_with("[]") => "Vec"`). Derive the
        // SAME `(type_name, method_name)` key the `Path` arm above uses and
        // probe the SAME `sigs.static_methods` table before falling back to
        // instance-by-name — preserves every existing (non-generic-static)
        // case unchanged.
        ExprKind::Member { obj, name } => {
            let static_key = match &obj.kind {
                ExprKind::TurboFish { base, .. } => match &base.kind {
                    ExprKind::Ident(n) => Some(n.clone()),
                    ExprKind::Path(parts) if parts.len() == 1 => Some(parts[0].clone()),
                    _ => None,
                },
                ExprKind::Path(parts) if parts.len() == 2 && parts[0] == "__array" => {
                    Some("Vec".to_string())
                }
                _ => None,
            };
            match static_key
                .and_then(|tn| sigs.static_methods.get(&(tn, name.clone())))
                .and_then(|cands| pick_static_params(cands, args, trailing.is_some()))
            {
                Some(p) => { is_static_generic_recv = true; p }
                None => sigs.instance_by_name.get(name)?,
            }
        }
        _ => return None, // сложный func — codegen сам.
        }
    };

    // Trailing связывает последний param — bind против params без него.
    let trailing_present = trailing.is_some();
    let effective_params: &[Param] = if trailing_present && !params.is_empty() {
        &params[..params.len() - 1]
    } else {
        params
    };

    // Binding. Ошибка → не трогаем (type-checker Ф.1 уже дал diagnostic).
    let bindings = bind_call_args(effective_params, args).ok()?;

    // Нужна ли нормализация? Только если есть Named-аргументы или
    // Default-биндинги. Чистый позиционный (включая variadic-хвост) —
    // оставляем как есть, codegen обработает.
    let needs_norm = bindings.iter().any(|b| {
        matches!(b, ArgBinding::Named(_) | ArgBinding::Default)
    }) || args.iter().any(|a| matches!(a, CallArg::Named { .. }));
    if !needs_norm {
        return None;
    }

    // --- Строим двухфазный Block. ---
    let sp = e.span;
    let mut stmts: Vec<Stmt> = Vec::new();

    // Plan 46 Ф.3: instance-method receiver вычисляется ПЕРВЫМ
    // (source-order: receiver до аргументов). Выносим `obj` в temp,
    // func переписываем на `__nova_recv.method`. Для Ident/Path func —
    // receiver'а нет, func клонируется как есть.
    // [M-vec-new-cap-default-arg-backfill]: a static-generic receiver
    // (`is_static_generic_recv`) is a TYPE expression (`TurboFish`/
    // `Path(["__array", elem])`), not a VALUE — hoisting it into
    // `let __nova_recv = <type-expr>` is meaningless (and codegen has no
    // C type for it, [E_UNKNOWN_TYPE_METHOD]). Treat it like the Ident/Path
    // static-call arms: `func` carries no runtime receiver, clone as-is.
    let final_func: Box<Expr> = if let ExprKind::Member { obj, name } = &func.kind {
        if is_static_generic_recv {
            func.clone()
        } else if is_addressable_receiver(obj) {
            // Plan 248 (wave 3, D447 fallout — real bug, not atomic-specific):
            // `obj` is a simple, side-effect-free lvalue (bare Ident/`@field`/
            // a pure projection chain of these) — re-embed it directly instead
            // of hoisting into `let __nova_recv = obj`. Hoisting COPIES the
            // receiver's VALUE; for a value-record (D226) type with a `mut`
            // receiver method, that copy silently drops the mutation from the
            // caller's original binding the moment this desugar fires (only
            // for named/default-arg calls) — e.g. `c.compare_exchange(a, b)`
            // (2 defaulted trailing params) mutated a throwaway `__nova_recv`
            // copy, leaving the caller's own `c` untouched (found on
            // `AtomicInt`/`AtomicI64`/`AtomicBool`, the first D226 value-
            // records in the corpus with a `mut`-receiver method that ALSO
            // has default params — masked forever on the old pointer-newtype
            // shape, where copying the receiver only ever copied a harmless
            // alias pointer, not the pointee). A bare-lvalue READ has no
            // evaluation-order hazard against the sibling argument temps
            // below (unlike a `Call`/other side-effecting receiver, which
            // still needs the hoist for correct source-order — see the `else`
            // branch), so reusing it as-is is safe.
            Box::new(Expr {
                kind: ExprKind::Member {
                    obj: obj.clone(),
                    name: name.clone(),
                },
                span: func.span, id: crate::ast::ExprId::UNSET, debug_only: false,
            })
        } else {
        let recv_name = "__nova_recv";
        stmts.push(let_stmt(recv_name, (**obj).clone(), sp));
        Box::new(Expr {
            kind: ExprKind::Member {
                obj: Box::new(ident_expr(recv_name, sp)),
                name: name.clone(),
            },
            span: func.span, id: crate::ast::ExprId::UNSET, debug_only: false,
        })
        }
    } else {
        func.clone()
    };

    // Фаза 1: explicit args → temps в SOURCE-order.
    // src_temp[arg_index] = имя temp-переменной для args[arg_index].
    let mut src_temp: HashMap<usize, String> = HashMap::new();
    for (ai, a) in args.iter().enumerate() {
        // Только Item/Named попадают в explicit-temps; Spread редок с
        // named и в этом пути не комбинируется (bind дал бы Variadic).
        let value_expr = match a {
            CallArg::Item(x) | CallArg::Named { value: x, .. } => x.clone(),
            CallArg::Spread(_) => continue, // variadic-путь, см. ниже
        };
        let tname = format!("__nova_arg_src{}", ai);
        stmts.push(let_stmt(&tname, value_expr, sp));
        src_temp.insert(ai, tname);
    }

    // ── Реестр №799, ВТОРАЯ ПОЛОВИНА (первая — обходчик интерполяции,
    //    `emit_c.rs`, 2026-08-29). Имя биндинга = имя параметра — приём, ради
    //    которого фаза 2 и устроена так: default-выражения видят предыдущие
    //    параметры без substitution walk. НО если в ТОМ ЖЕ блоке фаза 1 читает
    //    переменную вызывающего С ТЕМ ЖЕ ИМЕНЕМ, чтение оказывается ВЫШЕ
    //    объявления одноимённой локали, и внутри `spawn` это стоит захвата:
    //    решение «захват или локаль» принимается ПЛОСКО (`refs - bound`), имя
    //    попадает в `bound` — и поля в контексте фибры не появляется вовсе.
    //    Замер 2026-08-31, `docs/plans/repro/p799/`: пара файлов, различающихся
    //    ОДНИМ именем переменной, — с коллизией CC-FAIL `use of undeclared
    //    identifier 'policy'`, без неё сборка и `ok 3`.
    //
    //    ПОЧЕМУ ПЕРЕИМЕНОВАНИЕ, А НЕ ПРАВКА ЗАХВАТОВ: собрать захват мало —
    //    переписыватель обращений тоже плоский, и тогда употребление ПОСЛЕ
    //    `let policy = …` тоже стало бы `_c->policy`, то есть громкий CC-FAIL
    //    сменился бы тихо неверным значением. Переименование снимает коллизию,
    //    на которой спотыкаются ОБА механизма, и не трогает ни один из них.
    //
    //    КОГДА: только если фаза 1 вообще что-то положила. Нет явных
    //    аргументов — нет и чтений до объявления, и вызов «только с
    //    умолчаниями» остаётся байт-в-байт прежним.
    let rename: HashMap<String, String> = if src_temp.is_empty() {
        HashMap::new()
    } else {
        effective_params
            .iter()
            .take(bindings.len())
            .map(|prm| (prm.name.clone(), format!("__nova_bind_{}", prm.name)))
            .collect()
    };
    let bind_name = |prm: &Param| -> String {
        rename.get(&prm.name).cloned().unwrap_or_else(|| prm.name.clone())
    };

    // Фаза 2: param-binding в PARAM-order. Имя биндинга = имя параметра
    // → default-выражения видят предшествующие параметры естественно.
    let mut call_args: Vec<CallArg> = Vec::new();
    for (pi, binding) in bindings.iter().enumerate() {
        let param = &effective_params[pi];
        match binding {
            ArgBinding::Positional(ai) | ArgBinding::Named(ai) => {
                let tname = src_temp.get(ai).cloned()
                    .expect("explicit arg temp must exist");
                let bn = bind_name(param);
                stmts.push(let_stmt(
                    &bn,
                    ident_expr(&tname, sp),
                    sp,
                ));
                call_args.push(CallArg::Item(ident_expr(&bn, sp)));
            }
            ArgBinding::Default => {
                let def = param.default.clone()
                    .expect("Default binding requires param.default");
                // Plan 172.1 [M-172.1-default-arg-typed]: thread the param's DECLARED type into
                // the desugared `let` so a context-typed default literal/expr coerces to the
                // param type instead of defaulting to signed `nova_int` — `fn f(x uint = 0x80 >> 1)`
                // keeps an UNSIGNED operand (logical shift), not a signed collapse (int-collapse, D412).
                // Default-выражение вправе ссылаться на ПРЕДЫДУЩИЕ параметры по
                // имени (`fn slice(xs []int, to int = xs.len())`). Раз локали
                // переименованы — переименовываем и эти ссылки, ВНУТРИ одного
                // синтетического блока: это не substitution walk по программе,
                // которого D102 избегает, а правка скопированного выражения.
                let mut def = def;
                if !rename.is_empty() {
                    rename_idents(&mut def, &rename);
                }
                let bn = bind_name(param);
                stmts.push(let_stmt_typed(&bn, def, Some(param.ty.clone()), sp));
                call_args.push(CallArg::Item(ident_expr(&bn, sp)));
            }
            ArgBinding::Variadic(indices) => {
                // Variadic-хвост: передаём исходные args[indices] напрямую
                // (Item/Spread). Eval-order сохранён — это последние
                // позиционные, идут после regular в source.
                for &idx in indices {
                    call_args.push(args[idx].clone());
                }
            }
        }
    }

    // Финальный call в param-order. Trailing сохраняется. func —
    // переписанный (receiver вынесен в temp для Member, иначе как есть).
    // [fix №338]: id = e.id (ОРИГИНАЛЬНЫЙ id вызова, не UNSET). Чекер уже
    // аннотировал ИМЕННО этот call-site (resolved_callees/resolved_types
    // ключуются по e.id — `e` это тот самый Call, который мы здесь
    // переписываем в двухфазный Block, `e.kind` заменяется на Block, но
    // САМ `e.id` наверху остаётся). Синтезированный `new_call` — тот же
    // логический call (тот же callee, та же резолюция), просто с
    // аргументами в param-order — так что переиспользование e.id корректно
    // отражает факт «это тот же вызов» и открывает codegen's Channel 1/2
    // (infer_expr_c_type: resolved_callees→fn_ret_by_span,
    // resolved_types→resolved_type_to_c) для block.trailing внутри
    // emit_block_expr. С UNSET id оба канала гейтятся на `expr.id.is_set()`
    // и молча пропускались → block_ty падал в legacy `infer_call_ret_c`,
    // чья Ident-ветка (B10f) НАРОЧНО отвергает "void*" (générique erasure
    // для fn-типов) и не эмитит альтернативы — `block_ty` оставался ""
    // → `_nv_tmp_N;` без типа, `_nv_tmp_N = ()(...)` пустой каст, CC-FAIL
    // "use of undeclared identifier '_nv_tmp_N'". Репро: любой вызов
    // функции с default-параметром, возвращающей функциональный тип
    // (`fn(...) -> ...`), из-за чего требуется default-arg backfill
    // (омитнутый арг / именованный дефолт / форвард переменной — все три
    // формы требуют этот Block). Прямой (недесугаренный) вызов с тем же
    // сигнатурным профилем не сломан — у него `expr.id` уже ИЗНАЧАЛЬНО SET.
    let new_call = Expr {
        kind: ExprKind::Call {
            func: final_func,
            args: call_args,
            trailing: trailing.clone(),
        },
        span: sp, id: e.id, debug_only: false,
    };

    Some(ExprKind::Block(Block {
        stmts,
        trailing: Some(Box::new(new_call)),
        span: sp, is_unsafe: false
    }))
}

/// Plan 248 (wave 3, D447 fallout): classify a method-call receiver `Expr`
/// as an addressable lvalue — mirrors `emit_c.rs`'s `is_lvalue_receiver`
/// (same predicate, same reasoning, different crate module: codegen's own
/// receiver-ABI adaptation already established that a bare Ident/`@field`/
/// pure-projection-chain receiver is safe to reference directly, without an
/// intermediate temp, because reading it has no evaluation-order hazard).
/// Used here to decide whether the default/named-arg call-normalization
/// Block may skip hoisting the receiver into `let __nova_recv = obj` — for
/// non-addressable (rvalue) receivers the hoist is still required (source-
/// order correctness against the sibling argument temps).
fn is_addressable_receiver(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Ident(_) | ExprKind::SelfAccess => true,
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } =>
            is_addressable_receiver(obj),
        ExprKind::TurboFish { base, .. } => is_addressable_receiver(base),
        _ => false,
    }
}

/// `let <name> = <value>` statement.
fn let_stmt(name: &str, value: Expr, span: Span) -> Stmt {
    Stmt::Let(LetDecl {
        mutable: false,
        pattern: Pattern::Ident { name: name.to_string(), span, is_mut: false, is_consume: false },
        ty: None,
        value,
        span,
        is_ghost: false,
        consume: false,
    })
}

/// `let <name> <ty> = <value>` statement — typed variant (Plan 172.1 default-arg coercion).
fn let_stmt_typed(name: &str, value: Expr, ty: Option<crate::ast::TypeRef>, span: Span) -> Stmt {
    Stmt::Let(LetDecl {
        mutable: false,
        pattern: Pattern::Ident { name: name.to_string(), span, is_mut: false, is_consume: false },
        ty,
        value,
        span,
        is_ghost: false,
        consume: false,
    })
}

/// `<name>` identifier expression.
/// Переименовать идентификаторы по карте `map` (старое имя → новое) во всём
/// дереве выражения `e`. Зеркалит `walk_children` (см. выше): та же
/// структура ветвей `ExprKind`, тот же порядок под-выражений — вместо
/// `normalize_expr` на каждом под-выражении вызывается `rename_idents`.
/// Единственное отличие от `walk_children` по СМЫСЛУ (не по форме обхода):
/// здесь есть содержательное действие на самом узле `Ident` — переименование,
/// если `map` содержит текущее имя.
fn rename_idents(e: &mut Expr, map: &std::collections::HashMap<String, String>) {
    // Сам узел — Ident: переименовываем, если имя есть в карте.
    if let ExprKind::Ident(name) = &mut e.kind {
        if let Some(new_name) = map.get(name.as_str()) {
            *name = new_name.clone();
        }
    }
    match &mut e.kind {
        ExprKind::Call { func, args, trailing } => {
            rename_idents(func, map);
            for a in args.iter_mut() {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => rename_idents(x, map),
                    CallArg::Named { value, .. } => rename_idents(value, map),
                }
            }
            // Trailing инлайнится прямо здесь (не отдельной функцией) —
            // задание просит РОВНО две функции; зеркалит `normalize_trailing`
            // (callnorm.rs:456) один в один по видам Trailing.
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => rename_idents_block(b, map),
                    Trailing::LegacyBlockWithParams(tb) => rename_idents_block(&mut tb.body, map),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(x) => rename_idents(x, map),
                        FnBody::Block(b) => rename_idents_block(b, map),
                        FnBody::External => {}
                    },
                }
            }
        }
        ExprKind::TurboFish { base, .. } => rename_idents(base, map),
        ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => rename_idents(x, map),
        ExprKind::Coalesce(a, b) => { rename_idents(a, map); rename_idents(b, map); }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => rename_idents(x, map),
        ExprKind::Binary { left, right, .. } => {
            rename_idents(left, map); rename_idents(right, map);
        }
        ExprKind::Unary { operand, .. } => rename_idents(operand, map),
        ExprKind::Member { obj, .. } => rename_idents(obj, map),
        ExprKind::Index { obj, index } => {
            rename_idents(obj, map); rename_idents(index, map);
        }
        ExprKind::If { cond, then, else_ } => {
            rename_idents(cond, map);
            rename_idents_block(then, map);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rename_idents_block(b, map),
                    ElseBranch::If(x) => rename_idents(x, map),
                }
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            rename_idents(scrutinee, map);
            rename_idents_block(then, map);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rename_idents_block(b, map),
                    ElseBranch::If(x) => rename_idents(x, map),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rename_idents(scrutinee, map);
            for arm in arms.iter_mut() {
                if let Some(g) = &mut arm.guard { rename_idents(g, map); }
                match &mut arm.body {
                    MatchArmBody::Expr(x) => rename_idents(x, map),
                    MatchArmBody::Block(b) => rename_idents_block(b, map),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            rename_idents(iter, map); rename_idents_block(body, map);
        }
        ExprKind::ParallelFor { iter, body, .. } => {
            rename_idents(iter, map); rename_idents_block(body, map);
        }
        ExprKind::While { cond, body, .. } => {
            rename_idents(cond, map); rename_idents_block(body, map);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            rename_idents(scrutinee, map); rename_idents_block(body, map);
        }
        ExprKind::Loop { body, .. } => rename_idents_block(body, map),
        ExprKind::Block(b) => rename_idents_block(b, map),
        ExprKind::Spawn(x) => rename_idents(x, map),
        ExprKind::Detach(b) | ExprKind::Blocking(b) => rename_idents_block(b, map),
        ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
            rename_idents_block(body, map);
            if let Some(c) = cancel { rename_idents(c, map); }
            if let Some(dl) = deadline { rename_idents(&mut dl.expr, map); }
            if let Some(oh) = on_timeout { rename_idents(oh, map); }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            rename_idents_block(body, map)
        }
        ExprKind::Throw(x) => rename_idents(x, map),
        ExprKind::CoalesceReturnFallback(opt) => {
            if let Some(x) = opt { rename_idents(x, map); }
        }
        ExprKind::Interrupt(opt) => {
            if let Some(x) = opt { rename_idents(x, map); }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { rename_idents(s, map); }
            if let Some(e) = end { rename_idents(e, map); }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems.iter_mut() {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => rename_idents(x, map),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for me in elems.iter_mut() {
                match me {
                    crate::ast::MapElem::Pair(k, v) => {
                        rename_idents(k, map);
                        rename_idents(v, map);
                    }
                    crate::ast::MapElem::Spread(e) => rename_idents(e, map),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for x in elems.iter_mut() { rename_idents(x, map); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields.iter_mut() {
                if let Some(v) = &mut f.value { rename_idents(v, map); }
            }
        }
        // Строковая интерполяция: обязательная ветвь (по этой же ветви в
        // проекте уже дважды был дефект из-за пропуска обхода).
        ExprKind::InterpolatedStr { parts } => {
            for p in parts.iter_mut() {
                if let InterpStrPart::Expr { expr: x, spec: _ } = p { rename_idents(x, map); }
            }
        }
        ExprKind::TaggedTemplate { args, .. } => {
            for x in args.iter_mut() { rename_idents(x, map); }
        }
        ExprKind::Lambda { body, .. } => rename_idents(body, map),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(x) => rename_idents(x, map),
            ClosureBody::Block(b) => rename_idents_block(b, map),
        },
        ExprKind::ClosureFull(sb) => match &mut sb.body {
            FnBody::Expr(x) => rename_idents(x, map),
            FnBody::Block(b) => rename_idents_block(b, map),
            FnBody::External => {}
        },
        ExprKind::With { bindings, body } => {
            for b in bindings.iter_mut() { rename_idents(&mut b.handler, map); }
            rename_idents_block(body, map);
        }
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods.iter_mut() {
                match &mut m.body {
                    HandlerMethodBody::Expr(x) => rename_idents(x, map),
                    HandlerMethodBody::Block(b) => rename_idents_block(b, map),
                }
            }
        }
        ExprKind::Select { arms } => {
            for arm in arms.iter_mut() {
                match &mut arm.op {
                    SelectOp::Recv { chan, .. } => rename_idents(chan, map),
                    SelectOp::Send { chan, value } => {
                        rename_idents(chan, map); rename_idents(value, map);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &mut arm.guard { rename_idents(g, map); }
                rename_idents_block(&mut arm.body, map);
            }
        }
        // Листовые — нет под-выражений. `Ident` уже обработан ВЫШЕ (само
        // переименование смотрит на узел до этого match); здесь для него —
        // как и для остальных перечисленных — действительно нечего обходить.
        ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
        | ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
        | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
        | ExprKind::HexBlobLit(_) | ExprKind::NullPtrLit => {}
        // D.1.3: квантор — только в контрактах, не в runtime-коде.
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            rename_idents(range, map);
            rename_idents(body, map);
        }
    }
}

/// Обход блока (`stmts` + опциональный `trailing`) и его операторов —
/// объединяет связку `normalize_block`/`normalize_stmt` (callnorm.rs:212/221)
/// в одну функцию (задание просит РОВНО две функции всего): те же виды
/// `Stmt`, те же поля, `rename_idents` вместо `normalize_expr` на каждом
/// под-выражении, рекурсивный `rename_idents_block` на каждом вложенном
/// блоке.
fn rename_idents_block(b: &mut Block, map: &std::collections::HashMap<String, String>) {
    for s in &mut b.stmts {
        match s {
            Stmt::Expr(e) => rename_idents(e, map),
            Stmt::Let(d) => rename_idents(&mut d.value, map),
            Stmt::Const(d) => rename_idents(&mut d.value, map),
            Stmt::Assign { target, value, .. } => {
                rename_idents(target, map);
                rename_idents(value, map);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { rename_idents(v, map); }
            }
            Stmt::Throw { value, .. } => rename_idents(value, map),
            Stmt::Defer { body, .. } => rename_idents(body, map),
            // Plan 110 D188: consume X = init() { body } — walk init expr +
            // body block. `body` — уже `Block`, рекурсия той же функцией
            // (byte-эквивалентно ручному инлайн-циклу образца по stmts +
            // trailing).
            Stmt::ConsumeScope { init, body, .. } => {
                rename_idents(init, map);
                rename_idents_block(body, map);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => rename_idents(expr, map),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            // Ф.4.1: apply — ghost, аргументы переименовываем.
            Stmt::Apply { args, .. } => {
                for a in args { rename_idents(a, map); }
            }
            // Ф.4.2: calc — ghost, выражения шагов переименовываем.
            Stmt::Calc { steps, .. } => {
                for step in steps { rename_idents(&mut step.expr, map); }
            }
            // Plan 33.9: reveal — ghost, нет выражений для переименования.
            Stmt::Reveal { .. } => {}
            // Plan 136: tuple destructuring assignment — обходим все lhs + rhs.
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs { rename_idents(e, map); }
                for e in rhs { rename_idents(e, map); }
            }
        }
    }
    if let Some(t) = &mut b.trailing {
        rename_idents(t, map);
    }
}

fn ident_expr(name: &str, span: Span) -> Expr {
    Expr { kind: ExprKind::Ident(name.to_string()), span, id: crate::ast::ExprId::UNSET, debug_only: false }
}
