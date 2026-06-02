//! Plan 114.4.4.5 V4.1: true per-const-arg monomorphization для mixed const fns.
//!
//! V3 baseline (Plan 114.4.3 Ф.3): mixed const fn
//! `fn f(const a int, b int) -> int` compiles as regular runtime fn —
//! const param `a` purely informational (no actual specialization). Call
//! sites validated to have constexpr `a`.
//!
//! V4.1 (this pass): each unique call-site `f(LIT_A, x)` triggers emission
//! of specialized fn `f__cst_<idx>(b int) -> int` где `a` substituted с
//! literal value LIT_A в body, и `a` dropped из signature. Call site
//! rewritten to invoke specialized name с runtime args only.
//!
//! Pipeline placement: AFTER `rewrite_const_fn_calls`. Mixed fns still
//! в module; this pass specializes them.

use std::collections::HashMap;

use crate::ast::{Block, CallArg, ElseBranch, Expr, ExprKind, FnBody, FnDecl, Item, Module, Stmt, Trailing, ArrayElem, MatchArmBody};
use crate::const_fn_eval::{ConstValue, try_literal_to_value};
use crate::diag::Diagnostic;

pub fn specialize_mixed_const_fns(module: &mut Module) -> Vec<Diagnostic> {
    let mut errors: Vec<Diagnostic> = Vec::new();

    // 1. Collect mixed fn declarations.
    let mut mixed_decls: HashMap<String, FnDecl> = HashMap::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            let all_const = !fd.params.is_empty() && fd.params.iter().all(|p| p.is_const);
            let is_fully = (all_const || fd.params.is_empty()) && fd.return_is_const;
            let any_const = fd.return_is_const || fd.params.iter().any(|p| p.is_const);
            if any_const && !is_fully {
                mixed_decls.insert(fd.name.clone(), fd.clone());
            }
        }
    }
    if mixed_decls.is_empty() {
        return errors;
    }

    // 2. Walk + rewrite call sites.
    let mut specs: HashMap<(String, Vec<ConstValue>), String> = HashMap::new();
    let mut spec_counter: usize = 0;

    for item in &mut module.items {
        rewrite_in_item(item, &mixed_decls, &mut specs, &mut spec_counter, &mut errors);
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            rewrite_in_item(item, &mixed_decls, &mut specs, &mut spec_counter, &mut errors);
        }
    }

    // 3. Generate specialized fn decls.
    let mut specialized_items: Vec<Item> = Vec::new();
    for ((fn_name, const_args), spec_name) in &specs {
        let original = &mixed_decls[fn_name];
        let specialized = specialize_fn_decl(original, const_args, spec_name);
        specialized_items.push(Item::Fn(specialized));
    }
    module.items.extend(specialized_items);

    errors
}

fn specialize_fn_decl(original: &FnDecl, const_args: &[ConstValue], spec_name: &str) -> FnDecl {
    let mut spec = original.clone();
    spec.name = spec_name.to_string();

    let mut subst: HashMap<String, ConstValue> = HashMap::new();
    let mut new_params = Vec::new();
    let mut const_arg_idx = 0;
    for p in &spec.params {
        if p.is_const {
            if const_arg_idx < const_args.len() {
                subst.insert(p.name.clone(), const_args[const_arg_idx].clone());
                const_arg_idx += 1;
            }
        } else {
            let mut new_p = p.clone();
            new_p.is_const = false;
            new_params.push(new_p);
        }
    }
    spec.params = new_params;
    spec.return_is_const = false;

    match &mut spec.body {
        FnBody::Expr(e) => subst_expr(e, &subst),
        FnBody::Block(b) => subst_block(b, &subst),
        FnBody::External => {}
    }

    spec
}

fn subst_block(b: &mut Block, subst: &HashMap<String, ConstValue>) {
    for s in &mut b.stmts {
        subst_stmt(s, subst);
    }
    if let Some(t) = &mut b.trailing {
        subst_expr(t, subst);
    }
}

fn subst_stmt(s: &mut Stmt, subst: &HashMap<String, ConstValue>) {
    match s {
        Stmt::Let(d) => subst_expr(&mut d.value, subst),
        Stmt::Const(d) => subst_expr(&mut d.value, subst),
        Stmt::Expr(e) => subst_expr(e, subst),
        Stmt::Assign { target, value, .. } => {
            subst_expr(target, subst);
            subst_expr(value, subst);
        }
        Stmt::Return { value: Some(v), .. } => subst_expr(v, subst),
        Stmt::Throw { value, .. } => subst_expr(value, subst),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            subst_expr(body, subst);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            subst_expr(init, subst);
            subst_block(body, subst);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            subst_expr(expr, subst);
        }
        _ => {}
    }
}

fn subst_expr(e: &mut Expr, subst: &HashMap<String, ConstValue>) {
    if let ExprKind::Ident(name) = &e.kind {
        if let Some(v) = subst.get(name) {
            let span = e.span;
            *e = v.to_literal_expr(span);
            return;
        }
    }
    match &mut e.kind {
        ExprKind::Unary { operand, .. } => subst_expr(operand, subst),
        ExprKind::Binary { left, right, .. } => {
            subst_expr(left, subst);
            subst_expr(right, subst);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => subst_expr(x, subst),
        ExprKind::Call { func, args, trailing } => {
            subst_expr(func, subst);
            for a in args {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => subst_expr(x, subst),
                    CallArg::Named { value, .. } => subst_expr(value, subst),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => subst_block(b, subst),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => subst_expr(e, subst),
                        FnBody::Block(b) => subst_block(b, subst),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => subst_block(&mut tb.body, subst),
                }
            }
        }
        ExprKind::Member { obj, .. } => subst_expr(obj, subst),
        ExprKind::Index { obj, index } => {
            subst_expr(obj, subst);
            subst_expr(index, subst);
        }
        ExprKind::TurboFish { base, .. } => subst_expr(base, subst),
        ExprKind::Try(x) | ExprKind::Bang(x) => subst_expr(x, subst),
        ExprKind::Coalesce(a, b) => { subst_expr(a, subst); subst_expr(b, subst); }
        ExprKind::TupleLit(items) => for x in items { subst_expr(x, subst); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el {
                ArrayElem::Item(x) | ArrayElem::Spread(x) => subst_expr(x, subst),
            }
        },
        ExprKind::RecordLit { fields, .. } => for f in fields {
            if let Some(v) = &mut f.value { subst_expr(v, subst); }
        },
        ExprKind::If { cond, then, else_ } => {
            subst_expr(cond, subst);
            subst_block(then, subst);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => subst_block(b, subst),
                    ElseBranch::If(ie) => subst_expr(ie, subst),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            subst_expr(scrutinee, subst);
            for arm in arms {
                if let Some(g) = &mut arm.guard { subst_expr(g, subst); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => subst_expr(e, subst),
                    MatchArmBody::Block(b) => subst_block(b, subst),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            subst_expr(iter, subst);
            subst_block(body, subst);
        }
        ExprKind::While { cond, body, .. } => {
            subst_expr(cond, subst);
            subst_block(body, subst);
        }
        ExprKind::Loop { body, .. } => subst_block(body, subst),
        ExprKind::Block(b) => subst_block(b, subst),
        _ => {}
    }
}

fn rewrite_in_item(
    item: &mut Item,
    mixed_decls: &HashMap<String, FnDecl>,
    specs: &mut HashMap<(String, Vec<ConstValue>), String>,
    spec_counter: &mut usize,
    errors: &mut Vec<Diagnostic>,
) {
    match item {
        Item::Fn(fd) => {
            // Skip rewriting inside mixed fn bodies themselves — they get
            // specialized via clone. Original mixed fn body unchanged (used
            // as template для specialization).
            if mixed_decls.contains_key(&fd.name) { return; }
            for p in &mut fd.params {
                if let Some(def) = &mut p.default {
                    rewrite_in_expr(def, mixed_decls, specs, spec_counter, errors);
                }
            }
            match &mut fd.body {
                FnBody::Expr(e) => rewrite_in_expr(e, mixed_decls, specs, spec_counter, errors),
                FnBody::Block(b) => rewrite_in_block(b, mixed_decls, specs, spec_counter, errors),
                FnBody::External => {}
            }
        }
        Item::Const(c) => rewrite_in_expr(&mut c.value, mixed_decls, specs, spec_counter, errors),
        Item::Let(l) => rewrite_in_expr(&mut l.value, mixed_decls, specs, spec_counter, errors),
        Item::Test(t) => rewrite_in_block(&mut t.body, mixed_decls, specs, spec_counter, errors),
        Item::Bench(b) => {
            for s in &mut b.setup { rewrite_in_stmt(s, mixed_decls, specs, spec_counter, errors); }
            rewrite_in_block(&mut b.measure_body, mixed_decls, specs, spec_counter, errors);
            for s in &mut b.teardown { rewrite_in_stmt(s, mixed_decls, specs, spec_counter, errors); }
        }
        Item::Type(td) => {
            for ac in &mut td.assoc_consts {
                rewrite_in_expr(&mut ac.value, mixed_decls, specs, spec_counter, errors);
            }
        }
        Item::Lemma(_) => {}
    }
}

fn rewrite_in_block(
    b: &mut Block,
    mixed_decls: &HashMap<String, FnDecl>,
    specs: &mut HashMap<(String, Vec<ConstValue>), String>,
    spec_counter: &mut usize,
    errors: &mut Vec<Diagnostic>,
) {
    for s in &mut b.stmts {
        rewrite_in_stmt(s, mixed_decls, specs, spec_counter, errors);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_in_expr(t, mixed_decls, specs, spec_counter, errors);
    }
}

fn rewrite_in_stmt(
    s: &mut Stmt,
    mixed_decls: &HashMap<String, FnDecl>,
    specs: &mut HashMap<(String, Vec<ConstValue>), String>,
    spec_counter: &mut usize,
    errors: &mut Vec<Diagnostic>,
) {
    match s {
        Stmt::Let(d) => rewrite_in_expr(&mut d.value, mixed_decls, specs, spec_counter, errors),
        Stmt::Const(d) => rewrite_in_expr(&mut d.value, mixed_decls, specs, spec_counter, errors),
        Stmt::Expr(e) => rewrite_in_expr(e, mixed_decls, specs, spec_counter, errors),
        Stmt::Assign { target, value, .. } => {
            rewrite_in_expr(target, mixed_decls, specs, spec_counter, errors);
            rewrite_in_expr(value, mixed_decls, specs, spec_counter, errors);
        }
        Stmt::Return { value: Some(v), .. } => rewrite_in_expr(v, mixed_decls, specs, spec_counter, errors),
        Stmt::Throw { value, .. } => rewrite_in_expr(value, mixed_decls, specs, spec_counter, errors),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            rewrite_in_expr(body, mixed_decls, specs, spec_counter, errors);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            rewrite_in_expr(init, mixed_decls, specs, spec_counter, errors);
            rewrite_in_block(body, mixed_decls, specs, spec_counter, errors);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            rewrite_in_expr(expr, mixed_decls, specs, spec_counter, errors);
        }
        _ => {}
    }
}

fn rewrite_in_expr(
    e: &mut Expr,
    mixed_decls: &HashMap<String, FnDecl>,
    specs: &mut HashMap<(String, Vec<ConstValue>), String>,
    spec_counter: &mut usize,
    errors: &mut Vec<Diagnostic>,
) {
    // Recurse children first.
    match &mut e.kind {
        ExprKind::Unary { operand, .. } => rewrite_in_expr(operand, mixed_decls, specs, spec_counter, errors),
        ExprKind::Binary { left, right, .. } => {
            rewrite_in_expr(left, mixed_decls, specs, spec_counter, errors);
            rewrite_in_expr(right, mixed_decls, specs, spec_counter, errors);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => rewrite_in_expr(x, mixed_decls, specs, spec_counter, errors),
        ExprKind::Member { obj, .. } => rewrite_in_expr(obj, mixed_decls, specs, spec_counter, errors),
        ExprKind::Index { obj, index } => {
            rewrite_in_expr(obj, mixed_decls, specs, spec_counter, errors);
            rewrite_in_expr(index, mixed_decls, specs, spec_counter, errors);
        }
        ExprKind::TurboFish { base, .. } => rewrite_in_expr(base, mixed_decls, specs, spec_counter, errors),
        ExprKind::Try(x) | ExprKind::Bang(x) => rewrite_in_expr(x, mixed_decls, specs, spec_counter, errors),
        ExprKind::Coalesce(a, b) => { rewrite_in_expr(a, mixed_decls, specs, spec_counter, errors); rewrite_in_expr(b, mixed_decls, specs, spec_counter, errors); }
        ExprKind::TupleLit(items) => for x in items { rewrite_in_expr(x, mixed_decls, specs, spec_counter, errors); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el {
                ArrayElem::Item(x) | ArrayElem::Spread(x) => rewrite_in_expr(x, mixed_decls, specs, spec_counter, errors),
            }
        },
        ExprKind::RecordLit { fields, .. } => for f in fields {
            if let Some(v) = &mut f.value { rewrite_in_expr(v, mixed_decls, specs, spec_counter, errors); }
        },
        ExprKind::If { cond, then, else_ } => {
            rewrite_in_expr(cond, mixed_decls, specs, spec_counter, errors);
            rewrite_in_block(then, mixed_decls, specs, spec_counter, errors);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_in_block(b, mixed_decls, specs, spec_counter, errors),
                    ElseBranch::If(ie) => rewrite_in_expr(ie, mixed_decls, specs, spec_counter, errors),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_in_expr(scrutinee, mixed_decls, specs, spec_counter, errors);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_in_expr(g, mixed_decls, specs, spec_counter, errors); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => rewrite_in_expr(e, mixed_decls, specs, spec_counter, errors),
                    MatchArmBody::Block(b) => rewrite_in_block(b, mixed_decls, specs, spec_counter, errors),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            rewrite_in_expr(iter, mixed_decls, specs, spec_counter, errors);
            rewrite_in_block(body, mixed_decls, specs, spec_counter, errors);
        }
        ExprKind::While { cond, body, .. } => {
            rewrite_in_expr(cond, mixed_decls, specs, spec_counter, errors);
            rewrite_in_block(body, mixed_decls, specs, spec_counter, errors);
        }
        ExprKind::Loop { body, .. } => rewrite_in_block(body, mixed_decls, specs, spec_counter, errors),
        ExprKind::Block(b) => rewrite_in_block(b, mixed_decls, specs, spec_counter, errors),
        ExprKind::Call { func, args, trailing } => {
            rewrite_in_expr(func, mixed_decls, specs, spec_counter, errors);
            for a in args {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => rewrite_in_expr(x, mixed_decls, specs, spec_counter, errors),
                    CallArg::Named { value, .. } => rewrite_in_expr(value, mixed_decls, specs, spec_counter, errors),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => rewrite_in_block(b, mixed_decls, specs, spec_counter, errors),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => rewrite_in_expr(e, mixed_decls, specs, spec_counter, errors),
                        FnBody::Block(b) => rewrite_in_block(b, mixed_decls, specs, spec_counter, errors),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => rewrite_in_block(&mut tb.body, mixed_decls, specs, spec_counter, errors),
                }
            }
        }
        _ => {}
    }

    // Now check if THIS expression is Call to mixed fn — perform specialization.
    let mut do_rewrite: Option<(String, Vec<ConstValue>, Vec<CallArg>)> = None;
    if let ExprKind::Call { func, args, trailing: None } = &e.kind {
        if let ExprKind::Ident(name) = &func.kind {
            if let Some(fd) = mixed_decls.get(name) {
                let mut const_arg_vals: Vec<ConstValue> = Vec::new();
                let mut runtime_args: Vec<CallArg> = Vec::new();
                let mut all_resolved = args.len() == fd.params.len();
                if all_resolved {
                    for (p, a) in fd.params.iter().zip(args.iter()) {
                        match a {
                            CallArg::Item(ae) => {
                                if p.is_const {
                                    if let Some(v) = try_literal_to_value(ae) {
                                        const_arg_vals.push(v);
                                    } else {
                                        all_resolved = false;
                                        break;
                                    }
                                } else {
                                    runtime_args.push(a.clone());
                                }
                            }
                            _ => {
                                all_resolved = false;
                                break;
                            }
                        }
                    }
                }
                if all_resolved {
                    do_rewrite = Some((name.clone(), const_arg_vals, runtime_args));
                }
            }
        }
    }
    if let Some((fn_name, const_args, runtime_args)) = do_rewrite {
        let key = (fn_name.clone(), const_args);
        let counter_val = &mut *spec_counter;
        let spec_name = specs.entry(key).or_insert_with(|| {
            let n = format!("{}__cst_{}", fn_name, counter_val);
            *counter_val += 1;
            n
        }).clone();
        if let ExprKind::Call { func, args, .. } = &mut e.kind {
            if let ExprKind::Ident(n) = &mut func.kind {
                *n = spec_name;
            }
            *args = runtime_args;
        }
        let _ = errors;
    }
}
