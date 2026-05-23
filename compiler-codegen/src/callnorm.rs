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
    let mut free: HashMap<String, Vec<Vec<Param>>> = HashMap::new();
    let mut static_methods: HashMap<(String, String), Vec<Vec<Param>>> = HashMap::new();
    // instance: по имени метода → список сигнатур (со всех типов).
    // Уникальное имя (1 запись) → нормализуем; иначе skip.
    let mut instance: HashMap<String, Vec<Vec<Param>>> = HashMap::new();
    for item in &module.items {
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
    Sigs { free, static_methods, instance_by_name }
}

fn normalize_item(item: &mut Item, sigs: &Sigs) {
    match item {
        Item::Fn(f) => {
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
        Item::Test(t) => normalize_block(&mut t.body, sigs),
        // Plan 57: bench setup/measure_body/teardown — все три раздела
        // обычные блоки statement'ов, требуют такой же нормализации
        // вызовов, как test body.
        Item::Bench(b) => {
            for s in &mut b.setup {
                normalize_stmt(s, sigs);
            }
            normalize_block(&mut b.measure_body, sigs);
            for s in &mut b.teardown {
                normalize_stmt(s, sigs);
            }
        }
        Item::Const(c) => normalize_expr(&mut c.value, sigs),
        Item::Let(l) => normalize_expr(&mut l.value, sigs),
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
        Stmt::Assign { target, value, .. } => {
            normalize_expr(target, sigs);
            normalize_expr(value, sigs);
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { normalize_expr(v, sigs); }
        }
        Stmt::Throw { value, .. } => normalize_expr(value, sigs),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => normalize_expr(body, sigs),
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
        ExprKind::Try(x) | ExprKind::Bang(x) => normalize_expr(x, sigs),
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
        ExprKind::Supervised { body, cancel } => {
            normalize_block(body, sigs);
            if let Some(c) = cancel { normalize_expr(c, sigs); }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            normalize_block(body, sigs)
        }
        ExprKind::Throw(x) => normalize_expr(x, sigs),
        ExprKind::Interrupt(opt) => {
            if let Some(x) = opt { normalize_expr(x, sigs); }
        }
        ExprKind::Range { start, end, .. } => {
            normalize_expr(start, sigs); normalize_expr(end, sigs);
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
                if let InterpStrPart::Expr(x) = p { normalize_expr(x, sigs); }
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
        | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit => {}
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
    let params: &[Param] = match &base.kind {
        ExprKind::Ident(name) => sigs.free.get(name)?,
        ExprKind::Path(parts) if parts.len() == 2 => {
            sigs.static_methods.get(&(parts[0].clone(), parts[1].clone()))?
        }
        // Plan 46 Ф.3: instance-method `obj.method(...)`. Резолв по
        // уникальному имени метода (без type-inference). Receiver — это
        // `obj`, он НЕ входит в params (params это явные параметры
        // метода после receiver).
        ExprKind::Member { name, .. } => sigs.instance_by_name.get(name)?,
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

    // --- Строим двухфазный Block. ---
    let sp = e.span;
    let mut stmts: Vec<Stmt> = Vec::new();

    // Plan 46 Ф.3: instance-method receiver вычисляется ПЕРВЫМ
    // (source-order: receiver до аргументов). Выносим `obj` в temp,
    // func переписываем на `__nova_recv.method`. Для Ident/Path func —
    // receiver'а нет, func клонируется как есть.
    let final_func: Box<Expr> = if let ExprKind::Member { obj, name } = &func.kind {
        let recv_name = "__nova_recv";
        stmts.push(let_stmt(recv_name, (**obj).clone(), sp));
        Box::new(Expr {
            kind: ExprKind::Member {
                obj: Box::new(ident_expr(recv_name, sp)),
                name: name.clone(),
            },
            span: func.span,
        })
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
                stmts.push(let_stmt(&param.name, def, sp));
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
        span: sp,
    };

    Some(ExprKind::Block(Block {
        stmts,
        trailing: Some(Box::new(new_call)),
        span: sp,
    }))
}

/// `let <name> = <value>` statement.
fn let_stmt(name: &str, value: Expr, span: Span) -> Stmt {
    Stmt::Let(LetDecl {
        mutable: false,
        pattern: Pattern::Ident { name: name.to_string(), span },
        ty: None,
        value,
        span,
        is_ghost: false,
    })
}

/// `<name>` identifier expression.
fn ident_expr(name: &str, span: Span) -> Expr {
    Expr { kind: ExprKind::Ident(name.to_string()), span }
}
