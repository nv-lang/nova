//! Plan 46 (D102): argument binding — сопоставление call-site аргументов
//! (позиционных + именованных) с параметрами callee.
//!
//! **Единая логика** для двух потребителей:
//! - `types::check_module` — diagnostics (Ф.1): unknown name, double-bind,
//!   missing required, positional-after-named.
//! - `codegen::emit_c` — раскладка named → positional + вставка defaults
//!   (Ф.2).
//!
//! Это не упрощение под bootstrap — это правильная архитектура: одна
//! функция сопоставления, два потребителя, нет дублирования правил D102.

use crate::ast::{CallArg, Param};
use crate::diag::Span;

/// Чем связан один параметр callee после binding'а.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgBinding {
    /// Связан с `args[idx]` — позиционный аргумент.
    Positional(usize),
    /// Связан с `args[idx]` — именованный аргумент (`name: expr`).
    Named(usize),
    /// Параметр опущен на call-site — использовать `Param.default`.
    Default,
    /// Variadic-параметр: собирает `args[indices]` (позиционные/spread,
    /// оставшиеся после regular-параметров). Пустой Vec = пустой пакет.
    Variadic(Vec<usize>),
}

/// Ошибка binding'а. Несёт `Span` для diagnostic'а.
#[derive(Debug, Clone)]
pub enum BindError {
    /// Позиционный аргумент после именованного — D102 запрещает.
    PositionalAfterNamed { span: Span },
    /// Именованный аргумент с именем, которого нет среди параметров.
    UnknownParam { name: String, span: Span },
    /// Параметр связан дважды (позиционно И по имени).
    DuplicateParam { name: String, span: Span },
    /// Обязательный параметр (без default) не передан.
    MissingRequired { name: String },
    /// Именованный аргумент для variadic-параметра — D102 запрещает.
    NamedForVariadic { name: String, span: Span },
    /// Позиционных аргументов больше чем параметров (не variadic callee).
    TooManyPositional { expected: usize, got: usize, span: Span },
    /// `...spread` в не-variadic позиции.
    SpreadInNonVariadic { span: Span },
}

impl BindError {
    pub fn message(&self) -> String {
        match self {
            BindError::PositionalAfterNamed { .. } =>
                "позиционный аргумент не может идти после именованного (D102)".to_string(),
            BindError::UnknownParam { name, .. } =>
                format!("именованный аргумент `{}` — нет такого параметра", name),
            BindError::DuplicateParam { name, .. } =>
                format!("параметр `{}` связан дважды (позиционно и по имени)", name),
            BindError::MissingRequired { name } =>
                format!("обязательный параметр `{}` не передан", name),
            BindError::NamedForVariadic { name, .. } =>
                format!("именованный аргумент `{}` недопустим для variadic-параметра (D102)", name),
            BindError::TooManyPositional { expected, got, .. } =>
                format!("слишком много позиционных аргументов: ожидалось {}, передано {}", expected, got),
            BindError::SpreadInNonVariadic { .. } =>
                "`...spread` допустим только для variadic-параметра".to_string(),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            BindError::PositionalAfterNamed { span }
            | BindError::UnknownParam { span, .. }
            | BindError::DuplicateParam { span, .. }
            | BindError::NamedForVariadic { span, .. }
            | BindError::TooManyPositional { span, .. }
            | BindError::SpreadInNonVariadic { span } => *span,
            BindError::MissingRequired { .. } => Span::dummy(),
        }
    }
}

/// Plan 46 (D102): сопоставить call-site аргументы с параметрами callee.
///
/// Возвращает `Vec<ArgBinding>` длиной `params.len()` — для каждого
/// параметра, чем он связан. Порядок результата = порядок параметров
/// (call-order для codegen).
///
/// Алгоритм:
/// 1. Split args: позиционный префикс (Item/Spread) + именованный
///    суффикс (Named). Позиционный после именованного → ошибка.
/// 2. Regular-параметры (все кроме variadic) связываются: сначала
///    позиционно по индексу, потом по имени, потом default.
/// 3. Variadic-параметр (если есть, всегда последний) собирает
///    оставшиеся позиционные args.
/// 4. Проверки: unknown named, double-bind, named-for-variadic,
///    missing required, too-many-positional.
pub fn bind_call_args(
    params: &[Param],
    args: &[CallArg],
) -> Result<Vec<ArgBinding>, BindError> {
    // --- Шаг 1: split на позиционный префикс + именованный суффикс. ---
    let mut positional: Vec<usize> = Vec::new();
    let mut named: Vec<usize> = Vec::new();
    let mut seen_named = false;
    for (i, a) in args.iter().enumerate() {
        match a {
            CallArg::Named { .. } => {
                seen_named = true;
                named.push(i);
            }
            CallArg::Item(_) | CallArg::Spread(_) => {
                if seen_named {
                    return Err(BindError::PositionalAfterNamed {
                        span: a.expr().span,
                    });
                }
                positional.push(i);
            }
        }
    }

    // --- variadic detection: только последний параметр может быть. ---
    let has_variadic = params.last().map_or(false, |p| p.is_variadic);
    let regular_count = if has_variadic { params.len() - 1 } else { params.len() };

    // --- Early unknown-named check: каждый именованный аргумент обязан
    // соответствовать имени какого-то параметра. Проверяем ДО missing-
    // required — unknown name более специфичная ошибка (D102 правило 5).
    for &ni in &named {
        let arg_name = args[ni].arg_name().unwrap_or("");
        if !params.iter().any(|p| p.name == arg_name) {
            return Err(BindError::UnknownParam {
                name: arg_name.to_string(),
                span: args[ni].expr().span,
            });
        }
    }

    // --- Шаг 2: bind regular-параметры. ---
    let mut bindings: Vec<ArgBinding> = Vec::with_capacity(params.len());
    // Отслеживаем какие named args уже использованы (для unknown detection).
    let mut named_used: Vec<bool> = vec![false; named.len()];

    for (pi, param) in params.iter().take(regular_count).enumerate() {
        if pi < positional.len() {
            // Позиционно связан. Проверим что не дублируется по имени.
            if let Some(ni_pos) = named.iter().position(|&ni| {
                args[ni].arg_name() == Some(param.name.as_str())
            }) {
                named_used[ni_pos] = true; // mark — чтобы не упало unknown
                return Err(BindError::DuplicateParam {
                    name: param.name.clone(),
                    span: args[named[ni_pos]].expr().span,
                });
            }
            bindings.push(ArgBinding::Positional(positional[pi]));
        } else {
            // Не хватило позиционных — ищем named по имени.
            if let Some(ni_pos) = named.iter().position(|&ni| {
                args[ni].arg_name() == Some(param.name.as_str())
            }) {
                named_used[ni_pos] = true;
                bindings.push(ArgBinding::Named(named[ni_pos]));
            } else if param.default.is_some() {
                bindings.push(ArgBinding::Default);
            } else {
                return Err(BindError::MissingRequired {
                    name: param.name.clone(),
                });
            }
        }
    }

    // --- Шаг 3: variadic-параметр собирает оставшиеся позиционные. ---
    if has_variadic {
        let variadic_param = &params[params.len() - 1];
        // Named для variadic — запрещено (D102).
        if let Some(ni_pos) = named.iter().position(|&ni| {
            args[ni].arg_name() == Some(variadic_param.name.as_str())
        }) {
            return Err(BindError::NamedForVariadic {
                name: variadic_param.name.clone(),
                span: args[named[ni_pos]].expr().span,
            });
        }
        // Оставшиеся позиционные (после regular_count) → в variadic.
        let var_indices: Vec<usize> = if positional.len() > regular_count {
            positional[regular_count..].to_vec()
        } else {
            Vec::new()
        };
        bindings.push(ArgBinding::Variadic(var_indices));
    } else {
        // Не variadic: лишние позиционные — ошибка.
        if positional.len() > regular_count {
            let extra_span = args[positional[regular_count]].expr().span;
            return Err(BindError::TooManyPositional {
                expected: regular_count,
                got: positional.len(),
                span: extra_span,
            });
        }
        // Spread в не-variadic вызов — ошибка (нет variadic-параметра).
        for &pi in &positional {
            if args[pi].is_spread() {
                return Err(BindError::SpreadInNonVariadic {
                    span: args[pi].expr().span,
                });
            }
        }
    }

    // --- Шаг 4: unknown named — любой named, не привязанный к параметру. ---
    for (ni_pos, &arg_idx) in named.iter().enumerate() {
        if !named_used[ni_pos] {
            let name = args[arg_idx].arg_name().unwrap_or("").to_string();
            return Err(BindError::UnknownParam {
                name,
                span: args[arg_idx].expr().span,
            });
        }
    }

    Ok(bindings)
}
