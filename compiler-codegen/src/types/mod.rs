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
                    errors.push(Diagnostic::new(
                        format!("duplicate top-level name `{}`", key),
                        fd.span,
                    ));
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

