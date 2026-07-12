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
    /// Static-методы по `(type, method)`.
    static_methods: HashMap<(String, String), Vec<Param>>,
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
}

/// Plan 46 Ф.2: нормализовать все call-site в модуле.
/// Вызывается ПОСЛЕ resolve_imports_inline (нужны все сигнатуры) и
/// type-check, ПЕРЕД codegen.
pub fn normalize_module(module: &mut Module) {
    let sigs = collect_sigs(module);
    for item in &mut module.items {
        normalize_item(item, &sigs);
    }
}

fn collect_sigs(module: &Module) -> Sigs {
    if std::env::var_os("NOVA_DEBUG_196_3").is_some() {
        eprintln!("[DEBUG-196.3] collect_sigs ENTRY module.items.len()={} module.peer_files.len()={}",
            module.items.len(), module.peer_files.len());
        for pf in &module.peer_files {
            eprintln!("[DEBUG-196.3]   peer path={:?} is_entry_module={} items_here.len()={} module_name={:?}",
                pf.path, pf.is_entry_module, pf.items_here.len(), pf.module_name);
        }
    }
    let mut free: HashMap<String, Vec<Vec<Param>>> = HashMap::new();
    let mut static_methods: HashMap<(String, String), Vec<Vec<Param>>> = HashMap::new();
    // instance: по имени метода → список сигнатур (со всех типов).
    // Уникальное имя (1 запись) → нормализуем; иначе skip.
    let mut instance: HashMap<String, Vec<Vec<Param>>> = HashMap::new();
    let mut collect_items = |items: &[Item],
                              free: &mut HashMap<String, Vec<Vec<Param>>>,
                              static_methods: &mut HashMap<(String, String), Vec<Vec<Param>>>,
                              instance: &mut HashMap<String, Vec<Vec<Param>>>| {
        for item in items {
            if let Item::Fn(f) = item {
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
    };
    collect_items(&module.items, &mut free, &mut static_methods, &mut instance);
    // [M-static-generic-ctor-block-wrap-ice] (Plan 196.3): `module.items`
    // covers ONLY the compiled module's own declarations (entry file +
    // its folder-module co-equal siblings) — a std-lib ctor like
    // `Queue[T].new(cap int = 0)`, declared in `std/collections/queue.nv`
    // and reached via `import`, lives in `module.peer_files`, NOT
    // `module.items`. Without this, `static_methods`/`instance_by_name`
    // NEVER contain an entry for ANY imported generic-static ctor
    // (Vec/HashMap/Set/Queue's `new`), so `try_normalize_call`'s
    // `is_static_generic_recv` branch silently bails via `?` for every
    // SUCH call site — default-arg backfill never ran (mono'd C ctor got
    // called with too few args), and bare/named `.new()` calls fell
    // through codegen's fallback C-type inference straight into the
    // `[P67-LEGACY] Ident \`Queue\`/\`Vec\`/... not in var_types` panic
    // (full-CU ICE, D372-amend2). Mirror the SAME peer_files scan pattern
    // already used elsewhere for exactly this kind of cross-module gap
    // (e.g. emit_c.rs generic_type_templates registration). Filtered to
    // `is_entry_module == false` (imported peers only) — `true` peers are
    // the compiled module's OWN files, already fully covered by the
    // `module.items` pass above; scanning them again would double-push
    // into these `Vec<Vec<Param>>` accumulators and corrupt the
    // uniqueness filter below (a legitimately-unique sig would count as
    // 2 overloads and get dropped as "ambiguous").
    for pf in &module.peer_files {
        if pf.is_entry_module { continue; }
        collect_items(&pf.items_here, &mut free, &mut static_methods, &mut instance);
    }
    // Берём только unambiguous (1 запись).
    let free = free.into_iter()
        .filter_map(|(k, mut v)| if v.len() == 1 { Some((k, v.remove(0))) } else { None })
        .collect();
    let static_methods = static_methods.into_iter()
        .filter_map(|(k, mut v)| if v.len() == 1 { Some((k, v.remove(0))) } else { None })
        .collect();
    let instance_by_name = instance.into_iter()
        .filter_map(|(k, mut v)| if v.len() == 1 { Some((k, v.remove(0))) } else { None })
        .collect();
    Sigs { free, static_methods, instance_by_name, self_type: std::cell::RefCell::new(None) }
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
        ExprKind::Supervised { body, cancel, deadline } => {
            normalize_block(body, sigs);
            if let Some(c) = cancel { normalize_expr(c, sigs); }
            if let Some(_dl) = deadline { normalize_expr(&mut _dl.expr, sigs); }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            normalize_block(body, sigs)
        }
        ExprKind::Throw(x) => normalize_expr(x, sigs),
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

/// Попытаться нормализовать `Call`. Возвращает `Some(new_kind)` если
/// переписали (в Block-expr), `None` если оставили как есть.
fn try_normalize_call(e: &Expr, sigs: &Sigs) -> Option<ExprKind> {
    let ExprKind::Call { func, args, trailing } = &e.kind else { return None; };

    // Резолвим callee params.
    let base: &Expr = match &func.kind {
        ExprKind::TurboFish { base, .. } => base,
        _ => func.as_ref(),
    };
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
    let params: &[Param] = match &base.kind {
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
            let type_name = if parts[0] == "Self" {
                sigs.self_type.borrow().clone()?
            } else {
                parts[0].clone()
            };
            sigs.static_methods.get(&(type_name, parts[1].clone()))?
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
            if std::env::var_os("NOVA_DEBUG_196_3").is_some() {
                eprintln!("[DEBUG-196.3] Member-arm static_key={:?} method={} static_methods.len()={} has_key={}",
                    static_key, name, sigs.static_methods.len(),
                    static_key.as_ref().map(|tn| sigs.static_methods.contains_key(&(tn.clone(), name.clone()))).unwrap_or(false));
            }
            match static_key.and_then(|tn| sigs.static_methods.get(&(tn, name.clone()))) {
                Some(p) => { is_static_generic_recv = true; p }
                None => sigs.instance_by_name.get(name)?,
            }
        }
        _ => return None, // сложный func — codegen сам.
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

    // [M-static-generic-ctor-block-wrap-ice] (Plan 196.3, D102/D372-fold
    // follow-up): a static-generic-receiver call (`is_static_generic_recv`
    // — `Type[Args].method(...)` / `[]T.method(...)`) with EXACTLY ONE
    // effective param needs no temp-hoisting Block at all — there is only
    // ONE argument position, so no L-to-R evaluation-order hazard between
    // MULTIPLE explicit args (the reason the Block/temp machinery below
    // exists) can arise. Block-wrapping such a call broke downstream
    // codegen: the synthesized inner `Call` carries `ExprId::UNSET`, and
    // nesting it inside a `Block` hides the `Member{obj: TurboFish{..}}`
    // static-generic-ctor shape from the D109 C-type-inference fast path
    // (`infer_call_ret_c`, emit_c.rs) — inference fell through to a
    // generic Ident fallback and PANICKED (`[P67-LEGACY] Ident
    // \`Queue\`/\`Vec\`/\`Set\`/\`HashMap\` not in var_types`), a full-CU
    // ICE on EVERY bare/named `.new()` call across all four generic-
    // static D372-amend2 canonical-cap-ctor types. Undetectable until
    // Plan 196.3: a SEPARATE bug (D102 false-positive on the two
    // non-generic-static types of the six, `StringBuilder`/`WriteBuffer`)
    // always aborted compilation before codegen ever reached this shape,
    // so the gap was never exercised end-to-end. Rewrite directly to a
    // flat `Call` with the ORIGINAL (unchanged) func expr and exactly one
    // resolved arg — structurally IDENTICAL to the already-working
    // un-normalized explicit-positional call shape (`Vec[T].new(1024)`,
    // which bypasses `try_normalize_call` entirely since it needs no
    // backfill and `needs_norm` is false). Scoped to `is_static_generic_recv`
    // ONLY — ordinary instance-method / free-fn single-default-param
    // normalization keeps the general Block path below unchanged (zero
    // regression risk to the already-passing corpus).
    if is_static_generic_recv && effective_params.len() == 1 {
        if std::env::var_os("NOVA_DEBUG_196_3").is_some() {
            eprintln!("[DEBUG-196.3] fast-path HIT for id={:?}", e.id);
        }
        let resolved_arg: CallArg = match &bindings[0] {
            ArgBinding::Positional(ai) | ArgBinding::Named(ai) => {
                CallArg::Item(args[*ai].expr().clone())
            }
            ArgBinding::Default => {
                let def = effective_params[0].default.clone()
                    .expect("Default binding requires param.default");
                CallArg::Item(def)
            }
            // Variadic: not a canonical single-param ctor shape — leave
            // untouched, codegen resolves it directly (matches the
            // `!needs_norm` early-return behavior for pure-positional
            // calls elsewhere in this fn).
            ArgBinding::Variadic(_) => return None,
        };
        return Some(ExprKind::Call {
            func: func.clone(),
            args: vec![resolved_arg],
            trailing: trailing.clone(),
        });
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
        } else {
        let recv_name = "__nova_recv";
        stmts.push(let_stmt(recv_name, (**obj).clone(), sp));
        Box::new(Expr {
            kind: ExprKind::Member {
                obj: Box::new(ident_expr(recv_name, sp)),
                name: name.clone(),
            },
            span: func.span, id: crate::ast::ExprId::UNSET,
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

    // Фаза 2: param-binding в PARAM-order. Имя биндинга = имя параметра
    // → default-выражения видят предшествующие параметры естественно.
    let mut call_args: Vec<CallArg> = Vec::new();
    for (pi, binding) in bindings.iter().enumerate() {
        let param = &effective_params[pi];
        match binding {
            ArgBinding::Positional(ai) | ArgBinding::Named(ai) => {
                let tname = src_temp.get(ai).cloned()
                    .expect("explicit arg temp must exist");
                stmts.push(let_stmt(
                    &param.name,
                    ident_expr(&tname, sp),
                    sp,
                ));
                call_args.push(CallArg::Item(ident_expr(&param.name, sp)));
            }
            ArgBinding::Default => {
                let def = param.default.clone()
                    .expect("Default binding requires param.default");
                // Plan 172.1 [M-172.1-default-arg-typed]: thread the param's DECLARED type into
                // the desugared `let` so a context-typed default literal/expr coerces to the
                // param type instead of defaulting to signed `nova_int` — `fn f(x uint = 0x80 >> 1)`
                // keeps an UNSIGNED operand (logical shift), not a signed collapse (int-collapse, D412).
                stmts.push(let_stmt_typed(&param.name, def, Some(param.ty.clone()), sp));
                call_args.push(CallArg::Item(ident_expr(&param.name, sp)));
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
    let new_call = Expr {
        kind: ExprKind::Call {
            func: final_func,
            args: call_args,
            trailing: trailing.clone(),
        },
        span: sp, id: crate::ast::ExprId::UNSET,
    };

    Some(ExprKind::Block(Block {
        stmts,
        trailing: Some(Box::new(new_call)),
        span: sp, is_unsafe: false
    }))
}

/// `let <name> = <value>` statement.
fn let_stmt(name: &str, value: Expr, span: Span) -> Stmt {
    Stmt::Let(LetDecl {
        mutable: false,
        pattern: Pattern::Ident { name: name.to_string(), span, is_mut: false },
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
        pattern: Pattern::Ident { name: name.to_string(), span, is_mut: false },
        ty,
        value,
        span,
        is_ghost: false,
        consume: false,
    })
}

/// `<name>` identifier expression.
fn ident_expr(name: &str, span: Span) -> Expr {
    Expr { kind: ExprKind::Ident(name.to_string()), span, id: crate::ast::ExprId::UNSET }
}
