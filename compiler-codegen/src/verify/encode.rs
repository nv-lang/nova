//! Plan 33.1 Ф.3: Nova AST → SMT-IR encoder.
//!
//! Поддерживает straight-line код без mut/циклов (33.1 scope):
//! - Literals: int, bool, str.
//! - Variables (parameters, `result`, `old(...)`).
//! - Binary ops: +/-/*///%, ==/!=/<,<=,>,>=, &&/||, ==>/<==>.
//! - Unary: !, -.
//! - if/then/else (encoded as ite via and/or/impl).
//!
//! Не поддерживает (даёт `EncodingError`):
//! - field access (record types — uninterpreted в Ф.3, 33.2 расширит).
//! - method calls.
//! - other expressions (block, match, lambda, ...).

use std::collections::HashMap;

use crate::ast::{Expr, ExprKind, BinOp, UnOp};
use super::ir::*;

#[derive(Debug, Clone)]
pub enum EncodingError {
    /// Конструкция не поддерживается trivial-encoder'ом 33.1.
    Unsupported(String),
}

/// Plan 33.3 Ф.9: контекст encoder'а — реестр pure_view-ops модуля.
/// Ключ — pure_view name (e.g. "balance"), значение — (effect_name,
/// return_sort). Используется для конвертации `balance(id)` →
/// uninterpreted function `_view_Db_balance(id)` SMT-stide.
#[derive(Debug, Clone)]
pub struct EncodeCtx<'a> {
    pub pure_views: &'a HashMap<String, PureViewSig>,
    /// Plan 33.4 D.0.2: реестр `#pure` fn-ов модуля. Ключ — fn name.
    pub pure_fns: &'a HashMap<String, PureFnInfo>,
    /// Ф.4.2 (Plan 33.6): реестр `#trusted external fn`. Ключ — fn name.
    /// При встрече вызова → UF application; ensures инжектируются как axioms.
    pub trusted_fns: &'a HashMap<String, TrustedFnInfo>,
    /// Plan 33.3 Ф.11: типы переменных (params + let bindings).
    /// Нужны для dispatch `+` → `fp.add` vs Int `+` при FP аргументах.
    pub var_sorts: HashMap<String, SortRef>,
}

/// Signature of a `#pure` fn for SMT encoding (Plan 33.4 D.0.2).
#[derive(Debug, Clone)]
pub struct PureFnInfo {
    pub param_names: Vec<String>,
    pub param_sorts: Vec<SortRef>,
    pub return_sort: SortRef,
    /// Body expression for inlining. If present, calls to this fn in contracts
    /// are inlined (substituting args for params) rather than encoded as UF.
    /// This gives Z3 immediate ground truth without quantifier instantiation.
    pub body_expr: Option<Box<Expr>>,
}

/// SMT UF name for a pure fn: `_pure_fn_<name>`.
pub fn pure_fn_uf_name(fn_name: &str) -> String {
    format!("_pure_fn_{}", fn_name)
}

#[derive(Debug, Clone)]
pub struct PureViewSig {
    pub effect_name: String,
    pub arity: usize,
    /// Sort возвращаемого значения. Используется backend'ом для
    /// типизированной декларации UF (Z3 нужно знать range sort).
    pub return_sort: SortRef,
    /// Sorts параметров (тоже для UF declaration).
    pub param_sorts: Vec<SortRef>,
}

/// Ф.4.2 (Plan 33.6): информация о `#trusted external fn` для SMT axiom injection.
#[derive(Debug, Clone)]
pub struct TrustedFnInfo {
    pub param_names: Vec<String>,
    pub param_sorts: Vec<SortRef>,
    pub return_sort: SortRef,
    /// ensures-контракты (encoded); инжектируются как forall axiom в caller scope.
    pub ensures_exprs: Vec<crate::ast::Expr>,
}

/// SMT UF name для trusted external fn: `_trusted_<name>`.
pub fn trusted_fn_uf_name(fn_name: &str) -> String {
    format!("_trusted_{}", fn_name)
}

impl<'a> EncodeCtx<'a> {
    /// Empty context — pure_view-ы не известны (старые тесты + bootstrap).
    pub fn empty() -> EncodeCtx<'static> {
        // Хитрый трюк: возвращаем 'static reference на пустую map.
        // Используется только для backward-compat encode_expr.
        static EMPTY_VIEWS: std::sync::OnceLock<HashMap<String, PureViewSig>> = std::sync::OnceLock::new();
        static EMPTY_FNS: std::sync::OnceLock<HashMap<String, PureFnInfo>> = std::sync::OnceLock::new();
        static EMPTY_TRUSTED: std::sync::OnceLock<HashMap<String, TrustedFnInfo>> = std::sync::OnceLock::new();
        let views = EMPTY_VIEWS.get_or_init(HashMap::new);
        let fns = EMPTY_FNS.get_or_init(HashMap::new);
        let trusted = EMPTY_TRUSTED.get_or_init(HashMap::new);
        EncodeCtx { pure_views: views, pure_fns: fns, trusted_fns: trusted, var_sorts: HashMap::new() }
    }
}

/// Helper для UF имени pure_view: `_view_<EffectName>_<OpName>`.
pub fn pure_view_uf_name(effect: &str, op: &str) -> String {
    format!("_view_{}_{}", effect, op)
}

/// Encode Nova-expr в SMT-term (без context'а — backward-compat).
pub fn encode_expr(e: &Expr) -> Result<SmtTerm, EncodingError> {
    let ctx = EncodeCtx::empty();
    encode_expr_with_ctx(e, &ctx)
}

/// Encode Nova-expr в SMT-term с контекстом pure_view'ов.
pub fn encode_expr_with_ctx(e: &Expr, ctx: &EncodeCtx) -> Result<SmtTerm, EncodingError> {
    match &e.kind {
        ExprKind::IntLit(n) => Ok(SmtTerm::IntLit(*n)),
        ExprKind::BoolLit(b) => Ok(SmtTerm::BoolLit(*b)),
        ExprKind::StrLit(s) => Ok(SmtTerm::StrLit(s.clone())),
        // Plan 33.3 Ф.11: float literals → FP sort.
        ExprKind::FloatLit(v) => Ok(SmtTerm::F64Lit(v.to_bits())),
        ExprKind::Ident(n) => Ok(SmtTerm::Var(n.clone())),

        // `old(e)` — magic call. Encoder подменяет на fresh var `_old_<encoded>`.
        // В pipeline это значение equated с `encode(e) at entry-state`.
        ExprKind::Call { func, args, trailing } => {
            if trailing.is_none() && args.len() == 1 {
                if let ExprKind::Ident(name) = &func.kind {
                    if name == "old" {
                        let inner = encode_expr_with_ctx(args[0].expr(), ctx)?;
                        // Name based on pretty-print для стабильности.
                        let key = format!("_old_{}", sanitize_for_var(&inner.pretty()));
                        return Ok(SmtTerm::Var(key));
                    }
                }
            }
            // Plan 33.3 Ф.9: pure_view-call → UF `_view_<Effect>_<Op>`.
            // Type-check уже проверил что эффект в сигнатуре fn (Ф.9.3
            // part 2), здесь — просто конвертация в SMT-IR.
            if trailing.is_none() {
                if let ExprKind::Ident(name) = &func.kind {
                    if let Some(sig) = ctx.pure_views.get(name) {
                        if args.len() != sig.arity {
                            return Err(EncodingError::Unsupported(format!(
                                "pure_view `{}.{}` arity mismatch: expected {}, got {}",
                                sig.effect_name, name, sig.arity, args.len(),
                            )));
                        }
                        let mut encoded_args = Vec::with_capacity(args.len());
                        for a in args {
                            encoded_args.push(encode_expr_with_ctx(a.expr(), ctx)?);
                        }
                        let uf = pure_view_uf_name(&sig.effect_name, name);
                        return Ok(SmtTerm::App(uf, encoded_args));
                    }
                }
            }
            // Plan 33.4 D.0.2: #pure fn composition.
            // If body is available, inline (substitute args for params) to give
            // Z3 ground truth without quantifier instantiation. Otherwise fall
            // back to UF application (+ forall axiom in pipeline).
            if trailing.is_none() {
                if let ExprKind::Ident(name) = &func.kind {
                    if let Some(info) = ctx.pure_fns.get(name) {
                        if args.len() != info.param_sorts.len() {
                            return Err(EncodingError::Unsupported(format!(
                                "pure fn `{}` arity mismatch: expected {}, got {}",
                                name, info.param_sorts.len(), args.len()
                            )));
                        }
                        if let Some(body_e) = &info.body_expr {
                            // Inline: encode body with params substituted by encoded args.
                            let mut term = encode_expr_with_ctx(body_e, ctx)?;
                            for (param_name, call_arg) in info.param_names.iter().zip(args.iter()) {
                                let enc_arg = encode_expr_with_ctx(call_arg.expr(), ctx)?;
                                term = term.substitute(param_name, &enc_arg);
                            }
                            return Ok(term);
                        } else {
                            // No body → UF application.
                            let mut encoded_args = Vec::with_capacity(args.len());
                            for a in args {
                                encoded_args.push(encode_expr_with_ctx(a.expr(), ctx)?);
                            }
                            let uf = pure_fn_uf_name(name);
                            return Ok(SmtTerm::App(uf, encoded_args));
                        }
                    }
                }
            }
            // Ф.4.2 (Plan 33.6): #trusted external fn → UF application.
            // ensures-аксиомы инжектируются в pipeline (не здесь — нет доступа к backend).
            if trailing.is_none() {
                if let ExprKind::Ident(name) = &func.kind {
                    if let Some(info) = ctx.trusted_fns.get(name) {
                        if args.len() != info.param_sorts.len() {
                            return Err(EncodingError::Unsupported(format!(
                                "trusted fn `{}` arity mismatch: expected {}, got {}",
                                name, info.param_sorts.len(), args.len()
                            )));
                        }
                        let mut encoded_args = Vec::with_capacity(args.len());
                        for a in args {
                            encoded_args.push(encode_expr_with_ctx(a.expr(), ctx)?);
                        }
                        let uf = trusted_fn_uf_name(name);
                        return Ok(SmtTerm::App(uf, encoded_args));
                    }
                }
            }
            // Plan 60 D117 (size-accessor uniformity): zero-arg method calls
            // `obj.len()` / `obj.cap()` / `obj.is_empty()` / `obj.byte_len()`
            // encoded идентично legacy field-access (`obj.len` etc.) — тот же
            // UF `_field_<name>_<sort>(obj)`, чтобы существующие axioms/lemmas
            // продолжали работать после Plan 60 auto-migration.
            if trailing.is_none() && args.is_empty() {
                if let ExprKind::Member { obj, name } = &func.kind {
                    if matches!(name.as_str(), "len" | "cap" | "byte_len" | "is_empty") {
                        let obj_t = encode_expr_with_ctx(obj, ctx)?;
                        let sort_hint = if name == "is_empty" { "bool" } else { "int" };
                        return Ok(SmtTerm::App(
                            format!("_field_{}_{}", name, sort_hint),
                            vec![obj_t],
                        ));
                    }
                }
            }
            Err(EncodingError::Unsupported(format!(
                "call expressions in contracts not yet supported in Plan 33.1 \
                 (Plan 33.2 composition with `#pure` functions)"
            )))
        }

        ExprKind::Binary { op, left, right } => {
            let l = encode_expr_with_ctx(left, ctx)?;
            let r = encode_expr_with_ctx(right, ctx)?;
            // Plan 33.3 Ф.11: если хотя бы один операнд — FP literal или
            // переменная FP-типа, используем fp.* операторы.
            let is_fp = is_fp_term(&l, ctx) || is_fp_term(&r, ctx);
            if is_fp {
                let fp_op = bin_op_to_fp_smt(*op)?;
                Ok(SmtTerm::App(fp_op.into(), vec![l, r]))
            } else {
                let op_str = bin_op_to_smt(*op)?;
                Ok(SmtTerm::App(op_str.into(), vec![l, r]))
            }
        }

        ExprKind::Unary { op, operand } => {
            let v = encode_expr_with_ctx(operand, ctx)?;
            let is_fp = is_fp_term(&v, ctx);
            match op {
                UnOp::Not => Ok(SmtTerm::App("not".into(), vec![v])),
                UnOp::Neg if is_fp => Ok(SmtTerm::App("fp.neg".into(), vec![v])),
                UnOp::Neg => Ok(SmtTerm::App("-".into(),
                    vec![SmtTerm::IntLit(0), v])),
            }
        }

        // `if cond { then } else { else_ }` → ite(cond, then, else)
        // через `(or (and cond then) (and (not cond) else))`.
        ExprKind::If { cond, then, else_ } => {
            let cond_term = encode_expr_with_ctx(cond, ctx)?;
            // Block must be single expression for trivial encoding.
            if !then.stmts.is_empty() { return Err(EncodingError::Unsupported(
                "if-branch with statements not supported in trivial encoder".into())); }
            let then_term = match &then.trailing {
                Some(e) => encode_expr_with_ctx(e, ctx)?,
                None => return Err(EncodingError::Unsupported(
                    "if without trailing expression".into())),
            };
            let else_term = match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => {
                    if !b.stmts.is_empty() { return Err(EncodingError::Unsupported(
                        "else-branch with statements not supported".into())); }
                    match &b.trailing {
                        Some(e) => encode_expr_with_ctx(e, ctx)?,
                        None => return Err(EncodingError::Unsupported(
                            "else-block without trailing".into())),
                    }
                }
                Some(crate::ast::ElseBranch::If(e)) => encode_expr_with_ctx(e, ctx)?,
                None => return Err(EncodingError::Unsupported(
                    "if without else not supported".into())),
            };
            // Настоящий SMT ITE — корректен для arithmetic и bool terms.
            // or+and encoding терял информацию при arithmetic (Z3 не видел
            // что ite(c, a, b) >= a когда a >= b).
            Ok(SmtTerm::App("ite".into(), vec![cond_term, then_term, else_term
            ]))
        }

        ExprKind::UnitLit => Ok(SmtTerm::Var("_unit".into())),

        // Member access (record fields) — uninterpreted UF.
        // Ф.10.1 (Plan 33.6): type-aware naming `_field_<name>_<sort>` чтобы
        // избежать sort-mismatch при использовании одного field в разных типах.
        // Sort выводится из контекста: если obj — Var с известным sort, используем
        // эвристику по naming convention (`_b` suffix → Bool, иначе Int).
        // Полная type-aware extraction требует type-checker info — V2 work.
        ExprKind::Member { obj, name } => {
            let obj_t = encode_expr_with_ctx(obj, ctx)?;
            // Простая эвристика: имя field заканчивается на `?` или начинается с `is_`
            // → Bool sort hint; иначе Int.
            let sort_hint = if name.starts_with("is_") || name.ends_with("?") {
                "bool"
            } else {
                "int"
            };
            Ok(SmtTerm::App(format!("_field_{}_{}", name, sort_hint), vec![obj_t]))
        }

        // D.1.3: forall x in lo..hi : P(x)
        ExprKind::Forall { var, range, body } => {
            let (lo, hi) = extract_range(range)?;
            let lo_t = encode_expr_with_ctx(lo, ctx)?;
            let hi_t = encode_expr_with_ctx(hi, ctx)?;
            let body_t = encode_expr_with_ctx(body, ctx)?;
            let var_s = SmtTerm::Var(var.clone());
            // range constraint: lo <= x && x < hi
            let in_range = SmtTerm::and(vec![
                SmtTerm::App("<=".into(), vec![lo_t, var_s.clone()]),
                SmtTerm::App("<".into(), vec![var_s, hi_t]),
            ]);
            // forall x: Int. in_range => body
            let implies = SmtTerm::App("=>".into(), vec![in_range, body_t]);
            // Ф.1.2 (Plan 33.5): extract trigger patterns from body.
            // Ищем App(uf, args) содержащий bound_var → передаём как trigger
            // в SmtTerm::Forall.patterns, Z3 backend использует Z3_mk_pattern.
            let patterns = collect_triggers(&implies, var);
            Ok(SmtTerm::Forall(vec![(var.clone(), SortRef::Int)], patterns, Box::new(implies)))
        }

        // D.1.3: exists x in lo..hi : P(x)
        // Кодируем как not(forall x in range: not P(x))
        ExprKind::Exists { var, range, body } => {
            let (lo, hi) = extract_range(range)?;
            let lo_t = encode_expr_with_ctx(lo, ctx)?;
            let hi_t = encode_expr_with_ctx(hi, ctx)?;
            let body_t = encode_expr_with_ctx(body, ctx)?;
            let var_s = SmtTerm::Var(var.clone());
            let in_range = SmtTerm::and(vec![
                SmtTerm::App("<=".into(), vec![lo_t, var_s.clone()]),
                SmtTerm::App("<".into(), vec![var_s, hi_t]),
            ]);
            let not_body = SmtTerm::not(body_t);
            let implies = SmtTerm::App("=>".into(), vec![in_range, not_body]);
            // Ф.1.2: triggers для двойного-отрицания exists (по body, не not_body).
            // Ищем в implies (который содержит и not_body).
            let patterns = collect_triggers(&implies, var);
            let inner = SmtTerm::Forall(vec![(var.clone(), SortRef::Int)], patterns, Box::new(implies));
            Ok(SmtTerm::not(inner))
        }

        // Path — qualified name (Module.Const, Effect.op). Encode как Var.
        ExprKind::Path(parts) => Ok(SmtTerm::Var(parts.join("::"))),

        // CharLit — unicode codepoint, encode как int literal.
        ExprKind::CharLit(n) => Ok(SmtTerm::IntLit(*n as i64)),

        // Block с единственным trailing-выражением — делегируем в trailing.
        // Если есть statements (побочные эффекты) — unsupported.
        ExprKind::Block(block) => {
            if !block.stmts.is_empty() {
                return Err(EncodingError::Unsupported(
                    "block с statements в контракте не поддерживается; \
                     используйте чистое выражение или вынесите логику в #pure fn".into()));
            }
            match &block.trailing {
                Some(e) => encode_expr_with_ctx(e, ctx),
                None => Err(EncodingError::Unsupported(
                    "пустой block в контракте не поддерживается".into())),
            }
        }

        // Tuple literal — SMT не имеет product-type по умолчанию.
        ExprKind::TupleLit(_) => Err(EncodingError::Unsupported(
            "tuple-литерал в контракте не поддерживается SMT-encoder'ом; \
             используйте отдельные переменные или #unverified".into())),

        // Match — ветвление без статической структуры; используйте if/else.
        ExprKind::Match { .. } => Err(EncodingError::Unsupported(
            "match-выражение в контракте не поддерживается; \
             используйте if/else или вынесите логику в #pure fn".into())),

        // IfLet — комбинация pattern и ветвления.
        ExprKind::IfLet { .. } => Err(EncodingError::Unsupported(
            "if let в контракте не поддерживается; используйте if/else".into())),

        // Lambda / closure — анонимные функции не кодируются в SMT.
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_) => {
            Err(EncodingError::Unsupported(
                "лямбда/closure в контракте не поддерживается; \
                 вынесите логику в именованную #pure fn".into()))
        }

        // Index (arr[i]) — массивы не поддерживаются в V1 encoder'е.
        ExprKind::Index { .. } => Err(EncodingError::Unsupported(
            "индексирование (arr[i]) в контракте не поддерживается в V1; \
             используйте #pure fn для абстракции доступа".into())),

        // RecordLit / ArrayLit — составные литералы без SMT-аналогов.
        ExprKind::RecordLit { type_name, .. } => {
            let name = type_name.as_ref()
                .map(|p| p.join("."))
                .unwrap_or_else(|| "анонимный record".into());
            Err(EncodingError::Unsupported(format!(
                "record-литерал `{}` в контракте не поддерживается; \
                 используйте #pure fn возвращающую нужное поле", name)))
        }
        ExprKind::ArrayLit(_) => Err(EncodingError::Unsupported(
            "array-литерал в контракте не поддерживается; \
             используйте forall-квантор или #pure fn".into())),

        // Try (?) / Bang (!!) / Coalesce (??) — error-propagation в контрактах бессмысленна.
        ExprKind::Try(_) => Err(EncodingError::Unsupported(
            "оператор `?` в контракте не поддерживается; \
             контракты должны быть чистыми выражениями".into())),
        ExprKind::Bang(_) => Err(EncodingError::Unsupported(
            "оператор `!!` в контракте не поддерживается; \
             контракты должны быть чистыми выражениями".into())),
        ExprKind::Coalesce(_, _) => Err(EncodingError::Unsupported(
            "оператор `??` в контракте не поддерживается; \
             используйте if/else или #pure fn".into())),

        // As (type cast) / Is (type check) — типовые операции вне SMT.
        ExprKind::As(_, ty) => Err(EncodingError::Unsupported(format!(
            "type cast `as {:?}` в контракте не поддерживается; \
             используйте #pure fn для конвертации", ty))),
        ExprKind::Is(_, ty) => Err(EncodingError::Unsupported(format!(
            "type check `is {:?}` в контракте не поддерживается; \
             используйте discriminant через #pure fn", ty))),

        // SelfAccess (@field) — нет контекста receiver'а в SMT.
        ExprKind::SelfAccess => Err(EncodingError::Unsupported(
            "`@field` (self-access) в контракте не поддерживается; \
             передавайте поля явным параметром в #pure fn".into())),

        // InterpolatedStr — строковая интерполяция вне SMT.
        ExprKind::InterpolatedStr { .. } => Err(EncodingError::Unsupported(
            "интерполированная строка в контракте не поддерживается".into())),

        // TurboFish — generic-instantiation. Delegate к base если возможно.
        ExprKind::TurboFish { base, .. } => encode_expr_with_ctx(base, ctx),

        // Range — lo..hi как выражение вне forall/exists контекста.
        ExprKind::Range { .. } => Err(EncodingError::Unsupported(
            "range-выражение в контракте разрешено только внутри forall/exists квантора".into())),

        // Loops, spawn, with, handlers — control-flow недопустим в контрактах.
        ExprKind::For { .. } | ExprKind::While { .. } | ExprKind::WhileLet { .. }
        | ExprKind::Loop { .. } | ExprKind::ParallelFor { .. } => {
            Err(EncodingError::Unsupported(
                "цикл в контракте не поддерживается; \
                 используйте forall-квантор для итерационных свойств".into()))
        }
        ExprKind::Spawn(_) | ExprKind::Supervised { .. } | ExprKind::Select { .. } => {
            Err(EncodingError::Unsupported(
                "concurrency-конструкция в контракте не поддерживается".into()))
        }
        ExprKind::With { .. } | ExprKind::HandlerLit { .. } => {
            Err(EncodingError::Unsupported(
                "with/handler в контракте не поддерживается".into()))
        }
        ExprKind::Interrupt(_) | ExprKind::Forbid { .. } | ExprKind::Realtime { .. } => {
            Err(EncodingError::Unsupported(
                "interrupt/forbid/realtime в контракте не поддерживается".into()))
        }

        // Ф.9.1 (Plan 33.6): specific catch-alls с actionable suggestion.
        ExprKind::Detach(_) => Err(EncodingError::Unsupported(
            "`detach { ... }` (concurrency primitive) в контракте не поддерживается; \
             контракты должны быть pure expressions без spawn/detach".into())),
        ExprKind::Throw(_) => Err(EncodingError::Unsupported(
            "`throw expr` (error-throw) в контракте не поддерживается; \
             используйте `requires` для предусловий вместо throw в expression".into())),
        ExprKind::MapLit { .. } => Err(EncodingError::Unsupported(
            "map-литерал `[k:v]` в контракте не поддерживается SMT-encoder'ом; \
             используйте `forall` quantifier или вынесите проверку в #pure fn".into())),
        ExprKind::TaggedTemplate { .. } => Err(EncodingError::Unsupported(
            "tagged template `tag\"...\"` в контракте не поддерживается; \
             контракт должен быть pure boolean expression".into())),

        // ExprKind exhaustive выше — wildcard был бы unreachable. Если в
        // ExprKind добавится новый variant, компилятор подсветит match
        // как non-exhaustive → нужно явно решить SMT-семантику.
    }
}

/// Ф.1.2 (Plan 33.5): собирает trigger patterns для квантора над `bound_var`.
///
/// Алгоритм:
/// 1. Обходим body рекурсивно, собираем **все** App(name, args) где:
///    - name не является логическим оператором (=>, and, or, not, =, !=, <, <=, >, >=).
///    - хотя бы один arg содержит `bound_var` (прямо или косвенно).
/// 2. Возвращаем их как `Vec<Vec<SmtTerm>>` — один pattern per найденный App.
///    Z3 попробует каждый pattern независимо; достаточно матча одного.
/// 3. Если triggers не найдены — возвращаем пустой вектор (no-hint,
///    Z3 использует heuristic instantiation).
///
/// Паритет с Dafny: Dafny автоматически выводит triggers; Verus требует
/// явных `#[trigger]`. Nova автовыводит как Dafny, но без SAT fallback.
pub fn collect_triggers(body: &SmtTerm, bound_var: &str) -> Vec<Vec<SmtTerm>> {
    let mut found: Vec<SmtTerm> = Vec::new();
    collect_trigger_apps(body, bound_var, &mut found);
    if found.is_empty() {
        return vec![];
    }
    // Ф.9.7 (Plan 33.6): trigger ranking — score by (depth, ops count),
    // меньше = лучше (avoid matching loops). Top-3.
    let mut scored: Vec<(usize, SmtTerm)> = found.into_iter()
        .map(|t| (trigger_score(&t), t))
        .collect();
    scored.sort_by_key(|(s, _)| *s);
    scored.truncate(3);
    scored.into_iter().map(|(_, t)| vec![t]).collect()
}

/// Ф.9.7: score = depth * 10 + ops count. Меньше = better trigger.
fn trigger_score(t: &SmtTerm) -> usize {
    fn depth(t: &SmtTerm) -> usize {
        match t {
            SmtTerm::App(_, args) => 1 + args.iter().map(depth).max().unwrap_or(0),
            SmtTerm::Forall(_, _, body) => 1 + depth(body),
            _ => 0,
        }
    }
    fn ops(t: &SmtTerm) -> usize {
        match t {
            SmtTerm::App(_, args) => 1 + args.iter().map(ops).sum::<usize>(),
            SmtTerm::Forall(_, _, body) => 1 + ops(body),
            _ => 0,
        }
    }
    depth(t) * 10 + ops(t)
}

/// Рекурсивный walker для collect_triggers.
fn collect_trigger_apps(t: &SmtTerm, bound_var: &str, out: &mut Vec<SmtTerm>) {
    match t {
        SmtTerm::App(name, args) => {
            let is_logic_op = matches!(name.as_str(),
                "=>" | "and" | "or" | "not" | "=" | "!=" | "<" | "<=" | ">" | ">=" | "ite");
            if !is_logic_op && args.iter().any(|a| term_contains_var(a, bound_var)) {
                // Хороший trigger: UF или arithmetic fn содержащий bound var.
                out.push(t.clone());
                // Не рекурсируем в args — inner triggers less precise.
                return;
            }
            // Для логических операторов рекурсируем в аргументы.
            for a in args {
                collect_trigger_apps(a, bound_var, out);
            }
        }
        SmtTerm::Forall(_, _, inner) => collect_trigger_apps(inner, bound_var, out),
        _ => {}
    }
}

/// Ф.1.2: проверяет, содержит ли term переменную `var_name`.
pub fn term_contains_var(t: &SmtTerm, var_name: &str) -> bool {
    match t {
        SmtTerm::Var(n) => n == var_name,
        SmtTerm::App(_, args) => args.iter().any(|a| term_contains_var(a, var_name)),
        SmtTerm::Forall(_, _, body) => term_contains_var(body, var_name),
        _ => false,
    }
}

/// D.1.3: извлечь lo и hi из Range-выражения.
fn extract_range(e: &Expr) -> Result<(&Expr, &Expr), EncodingError> {
    match &e.kind {
        ExprKind::Range { start, end, .. } => Ok((start, end)),
        _ => Err(EncodingError::Unsupported(
            "quantifier range must be lo..hi expression".into())),
    }
}

/// Plan 33.3 Ф.11: определяем по SmtTerm — является ли он FP типом.
/// Проверяем: F64Lit/F32Lit literals, или Var чей sort FP в ctx.
fn is_fp_term(t: &SmtTerm, ctx: &EncodeCtx) -> bool {
    match t {
        SmtTerm::F32Lit(_) | SmtTerm::F64Lit(_) => true,
        SmtTerm::Var(name) => matches!(
            ctx.var_sorts.get(name),
            Some(SortRef::F32) | Some(SortRef::F64)
        ),
        // App returns FP если первый аргумент FP (для arithmetic chains).
        SmtTerm::App(op, args) if matches!(op.as_str(), "fp.add" | "fp.sub" | "fp.mul" | "fp.div" | "fp.abs" | "fp.neg" | "fp.sqrt") => {
            !args.is_empty()
        }
        _ => false,
    }
}

/// Plan 33.3 Ф.11: BinOp → FP SMT operator.
fn bin_op_to_fp_smt(op: BinOp) -> Result<&'static str, EncodingError> {
    Ok(match op {
        BinOp::Add => "fp.add",
        BinOp::Sub => "fp.sub",
        BinOp::Mul => "fp.mul",
        BinOp::Div => "fp.div",
        BinOp::Eq  => "fp.eq",
        BinOp::Neq => "!=",  // fp.neq через not(fp.eq) — Z3 не имеет fp.neq напрямую
        BinOp::Lt  => "fp.lt",
        BinOp::Le  => "fp.leq",
        BinOp::Gt  => "fp.gt",
        BinOp::Ge  => "fp.geq",
        // Logical ops — всегда Bool, не FP-specific.
        BinOp::And => "and",
        BinOp::Or  => "or",
        BinOp::Implies => "=>",
        BinOp::Iff => "<=>",
        _ => return Err(EncodingError::Unsupported(
            format!("FP binary op {:?} not supported in SMT encoding", op))),
    })
}

fn bin_op_to_smt(op: BinOp) -> Result<&'static str, EncodingError> {
    Ok(match op {
        BinOp::Add => "+", BinOp::Sub => "-",
        BinOp::Mul => "*", BinOp::Div => "/", BinOp::Mod => "%",
        BinOp::Eq => "=", BinOp::Neq => "!=",
        BinOp::Lt => "<", BinOp::Le => "<=",
        BinOp::Gt => ">", BinOp::Ge => ">=",
        BinOp::And => "and", BinOp::Or => "or",
        BinOp::Implies => "=>",
        BinOp::Iff => "=",
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
            return Err(EncodingError::Unsupported(
                "bitwise operators in contracts not supported in Plan 33.1".into()));
        }
    })
}

/// Make valid SMT-IR var name from pretty-printed term.
fn sanitize_for_var(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Span;

    fn span() -> Span { Span::new(0, 0) }

    fn ident(n: &str) -> Expr {
        Expr::new(ExprKind::Ident(n.into()), span())
    }

    fn int(n: i64) -> Expr { Expr::new(ExprKind::IntLit(n), span()) }

    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::new(ExprKind::Binary { op, left: Box::new(l), right: Box::new(r) }, span())
    }

    #[test]
    fn encode_simple_eq() {
        // x == 5
        let e = bin(BinOp::Eq, ident("x"), int(5));
        let t = encode_expr(&e).unwrap();
        assert_eq!(t,
            SmtTerm::App("=".into(),
                vec![SmtTerm::Var("x".into()), SmtTerm::IntLit(5)]));
    }

    #[test]
    fn encode_arith() {
        // x + 1
        let e = bin(BinOp::Add, ident("x"), int(1));
        let t = encode_expr(&e).unwrap();
        assert_eq!(t,
            SmtTerm::App("+".into(),
                vec![SmtTerm::Var("x".into()), SmtTerm::IntLit(1)]));
    }

    #[test]
    fn encode_implication() {
        // x > 0 ==> x >= 1
        let e = bin(BinOp::Implies,
            bin(BinOp::Gt, ident("x"), int(0)),
            bin(BinOp::Ge, ident("x"), int(1)));
        let t = encode_expr(&e).unwrap();
        // Just check op is "=>" — структура была.
        match t {
            SmtTerm::App(op, _) => assert_eq!(op, "=>"),
            _ => panic!(),
        }
    }
}
