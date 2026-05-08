//! Type checker и effect inference.
//!
//! Минимальная реализация: проверяем имена типов, выводим типы локальных
//! переменных, выводим эффекты для private функций (D28). Generic-параметры
//! проверяются как abstract names — мономорфизация делается при
//! интерпретации (treewalk не требует всего).

use crate::ast::*;
use crate::diag::Diagnostic;
use std::collections::{HashMap, HashSet};

/// Очень упрощённая система типов для bootstrap'а.
///
/// Treewalk-интерпретатор работает с динамическими значениями, поэтому
/// здесь мы выполняем минимум: проверки имён, базовая совместимость,
/// effect inference через accumulated set.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Float,
    Str,
    Bool,
    Unit,
    Never,
    /// Любой тип / неизвестный (для bootstrap'а — fallback).
    Any,
    /// Именованный тип (record, sum, effect, newtype, alias).
    /// Generics не разворачиваются — они мономорфизируются позже.
    Named(String),
    Array(Box<Ty>),
    Tuple(Vec<Ty>),
    Func {
        params: Vec<Ty>,
        ret: Box<Ty>,
        effects: Vec<String>,
    },
}

/// Результат проверки модуля — карта имён top-level → тип.
#[derive(Debug, Default)]
pub struct ModuleEnv {
    pub types: HashMap<String, TypeDecl>,
    pub fns: HashMap<String, FnDecl>,
    pub consts: HashMap<String, ConstDecl>,
}

/// Минимальная проверка модуля. Регистрирует имена и базовую структуру —
/// для bootstrap'а этого достаточно: интерпретатор ловит ошибки типов в
/// runtime через match-mismatch и method-not-found.
pub fn check_module(module: &Module) -> Result<ModuleEnv, Vec<Diagnostic>> {
    let mut env = ModuleEnv::default();
    let mut errors = Vec::new();
    let mut names: HashSet<String> = HashSet::new();

    for item in &module.items {
        match item {
            Item::Type(td) => {
                if !names.insert(td.name.clone()) {
                    errors.push(Diagnostic::new(
                        format!("duplicate top-level name `{}`", td.name),
                        td.span,
                    ));
                }
                env.types.insert(td.name.clone(), td.clone());
            }
            Item::Fn(fd) => {
                let key = match &fd.receiver {
                    Some(r) => format!("{}.{}", r.type_name, fd.name),
                    None => fd.name.clone(),
                };
                if !names.insert(key.clone()) {
                    // D82 + Plan 04: external fn разрешает overload по типу
                    // аргумента (StringBuilder.from(s) / StringBuilder.from(c)).
                    // Dispatch — special-case в codegen для built-in opaque-типов.
                    // User-defined external запрещён (whitelist `std.runtime.*`),
                    // поэтому проверка на is_external достаточна.
                    if !fd.is_external {
                        errors.push(Diagnostic::new(
                            format!("duplicate top-level name `{}`", key),
                            fd.span,
                        ));
                    }
                }
                env.fns.insert(key, fd.clone());
            }
            Item::Const(cd) => {
                if !names.insert(cd.name.clone()) {
                    errors.push(Diagnostic::new(
                        format!("duplicate top-level name `{}`", cd.name),
                        cd.span,
                    ));
                }
                env.consts.insert(cd.name.clone(), cd.clone());
            }
            Item::Let(_) | Item::Test(_) => {
                // top-level let — не используется в Nova-исходниках. test —
                // регистрируется отдельно, имя не конфликтует.
            }
        }
    }

    if errors.is_empty() {
        Ok(env)
    } else {
        Err(errors)
    }
}

/// D28 effect inference для private fn.
///
/// Walk модуль mutably: для каждой private (`!is_export`) fn,
/// если её тело использует `throw`, и в effect-row нет ни одного
/// `Fail`/`Fail[E]`/`Fail[any]` — добавляем `Fail` (placeholder).
///
/// Это упрощённая реализация D28 для bootstrap'а:
/// - Полная version выводила бы конкретный E из type-of(throw expr).
///   Bootstrap не имеет точного типизатора, поэтому выводит просто
///   `Fail` (placeholder, по D65 — inference placeholder).
/// - Для public fn ничего не делаем (D62: явная декларация обязательна).
/// - Транзитивная inference (callee имеет Fail → caller тоже) не
///   реализована; программист должен явно импортировать.
///
/// Эффекты типа Db/Net/Time/etc. **не** добавляются автоматически —
/// они resource-capability и должны быть видны в сигнатуре, программист
/// объявляет явно. Только Fail имеет особый placeholder-режим.
pub fn infer_effects(module: &mut Module) {
    for item in &mut module.items {
        if let Item::Fn(f) = item {
            if f.is_export {
                continue;
            }
            if has_throw_in_fn(f) && !has_fail_effect(&f.effects) {
                let span = f.span;
                f.effects.push(TypeRef::Named {
                    path: vec!["Fail".to_string()],
                    generics: vec![],
                    span,
                });
            }
        }
    }
}

/// Есть ли хотя бы один `Fail`/`Fail[...]` в effect-row.
fn has_fail_effect(effects: &[TypeRef]) -> bool {
    effects.iter().any(|e| {
        matches!(e, TypeRef::Named { path, .. } if path.len() == 1 && path[0] == "Fail")
    })
}

/// Содержит ли тело fn выражение `throw` (рекурсивно).
fn has_throw_in_fn(f: &FnDecl) -> bool {
    match &f.body {
        FnBody::Expr(e) => has_throw_in_expr(e),
        FnBody::Block(b) => has_throw_in_block(b),
        // D82: external fn — тела нет; throw'ы декларируются через
        // Fail[E] effect-аннотацию в сигнатуре, не в теле.
        FnBody::External => false,
    }
}

fn has_throw_in_block(b: &Block) -> bool {
    for s in &b.stmts {
        if has_throw_in_stmt(s) {
            return true;
        }
    }
    if let Some(t) = &b.trailing {
        if has_throw_in_expr(t) {
            return true;
        }
    }
    false
}

fn has_throw_in_stmt(s: &Stmt) -> bool {
    match s {
        Stmt::Expr(e) => has_throw_in_expr(e),
        Stmt::Let(decl) => has_throw_in_expr(&decl.value),
        Stmt::Assign { target, value, .. } =>
            has_throw_in_expr(target) || has_throw_in_expr(value),
        Stmt::Return { value, .. } => value.as_ref().map_or(false, has_throw_in_expr),
        Stmt::Throw { value, .. } => {
            // Statement-level throw: явный сигнал, что Fail нужен.
            let _ = value;
            true
        }
        Stmt::Break(_) | Stmt::Continue(_) => false,
    }
}

fn has_throw_in_expr(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Throw(_) => true,
        ExprKind::Try(inner) => has_throw_in_expr(inner),
        ExprKind::Binary { left, right, .. } =>
            has_throw_in_expr(left) || has_throw_in_expr(right),
        ExprKind::Unary { operand, .. } => has_throw_in_expr(operand),
        ExprKind::Call { func, args, .. } =>
            has_throw_in_expr(func) || args.iter().any(has_throw_in_expr),
        ExprKind::Member { obj, .. } => has_throw_in_expr(obj),
        ExprKind::Index { obj, index } =>
            has_throw_in_expr(obj) || has_throw_in_expr(index),
        ExprKind::If { cond, then, else_, .. } => {
            if has_throw_in_expr(cond) || has_throw_in_block(then) { return true; }
            match else_ {
                Some(ElseBranch::Block(b)) => has_throw_in_block(b),
                Some(ElseBranch::If(e)) => has_throw_in_expr(e),
                None => false,
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            if has_throw_in_expr(scrutinee) || has_throw_in_block(then) { return true; }
            match else_ {
                Some(ElseBranch::Block(b)) => has_throw_in_block(b),
                Some(ElseBranch::If(e)) => has_throw_in_expr(e),
                None => false,
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            if has_throw_in_expr(scrutinee) { return true; }
            arms.iter().any(|arm| match &arm.body {
                MatchArmBody::Expr(e) => has_throw_in_expr(e),
                MatchArmBody::Block(b) => has_throw_in_block(b),
            })
        }
        ExprKind::While { cond, body } => has_throw_in_expr(cond) || has_throw_in_block(body),
        ExprKind::WhileLet { scrutinee, body, .. } =>
            has_throw_in_expr(scrutinee) || has_throw_in_block(body),
        ExprKind::For { iter, body, .. } => has_throw_in_expr(iter) || has_throw_in_block(body),
        ExprKind::Loop { body } => has_throw_in_block(body),
        ExprKind::Block(b) => has_throw_in_block(b),
        ExprKind::Lambda { .. } => false,
            // Lambda has its own scope; throw inside lambda — её эффекты, не текущей fn.
        ExprKind::Range { start, end, .. } =>
            has_throw_in_expr(start) || has_throw_in_expr(end),
        ExprKind::TupleLit(elems) => elems.iter().any(has_throw_in_expr),
        ExprKind::ArrayLit(elems) => elems.iter().any(|el| match el {
            ArrayElem::Item(e) => has_throw_in_expr(e),
            ArrayElem::Spread(e) => has_throw_in_expr(e),
        }),
        ExprKind::RecordLit { fields, .. } =>
            fields.iter().any(|f| f.value.as_ref().map_or(false, has_throw_in_expr)),
        ExprKind::With { bindings, body } => {
            if bindings.iter().any(|b| has_throw_in_expr(&b.handler)) { return true; }
            has_throw_in_block(body)
        }
        ExprKind::Spawn(e) => has_throw_in_expr(e),
        ExprKind::Supervised(b) => has_throw_in_block(b),
        ExprKind::ParallelFor { iter, body, .. } =>
            has_throw_in_expr(iter) || has_throw_in_block(body),
        ExprKind::TurboFish { base, .. } => has_throw_in_expr(base),
        _ => false,
    }
}

/// Преобразует `TypeRef` AST в `Ty` для базовой проверки.
pub fn ty_of_ref(tr: &TypeRef) -> Ty {
    match tr {
        TypeRef::Named { path, .. } => match path.last().map(|s| s.as_str()) {
            Some("int") | Some("i8") | Some("i16") | Some("i32") | Some("i64") => Ty::Int,
            Some("u8") | Some("u16") | Some("u32") | Some("u64") => Ty::Int,
            Some("f32") | Some("f64") => Ty::Float,
            Some("str") => Ty::Str,
            Some("bool") => Ty::Bool,
            Some("byte") => Ty::Int,
            Some("Never") => Ty::Never,
            Some(name) => Ty::Named(name.to_string()),
            None => Ty::Any,
        },
        TypeRef::Array(inner, _) => Ty::Array(Box::new(ty_of_ref(inner))),
        TypeRef::FixedArray(_, inner, _) => Ty::Array(Box::new(ty_of_ref(inner))),
        TypeRef::Tuple(elems, _) => Ty::Tuple(elems.iter().map(ty_of_ref).collect()),
        TypeRef::Func {
            params,
            return_type,
            effects,
            ..
        } => Ty::Func {
            params: params.iter().map(ty_of_ref).collect(),
            ret: Box::new(
                return_type
                    .as_ref()
                    .map(|t| ty_of_ref(t))
                    .unwrap_or(Ty::Unit),
            ),
            effects: effects
                .iter()
                .filter_map(|e| match e {
                    TypeRef::Named { path, .. } => path.last().cloned(),
                    _ => None,
                })
                .collect(),
        },
        TypeRef::Unit(_) => Ty::Unit,
    }
}

