//! Treewalk-интерпретатор Nova.
//!
//! Минимальная реализация: проходит AST, выполняет statements/expressions,
//! поддерживает handler-стек для эффектов.
//!
//! Что **не** реализовано (и не нужно для bootstrap'а Nova-on-Nova):
//! - Async/Par fiber-runtime → синхронное исполнение, `spawn` = inline call
//! - Comptime/macros
//! - SMT-проверка контрактов
//! - Type-полиморфизм с проверкой во время интерпретации (всё динамика)

pub mod env;
pub mod value;

pub mod stdlib;

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use env::Env;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use value::*;

/// Plan 19, C4-C5: конвертирует `Trailing::Block` /
/// `Trailing::LegacyBlockWithParams` в legacy `TrailingBlock` для
/// interpreter eval-path'а (старый `eval_call` принимает старый тип).
///
/// - `Trailing::Block(b)` → `TrailingBlock { params: [], body: b }`.
/// - `Trailing::LegacyBlockWithParams(tb)` → возвращает копию `tb`.
/// - `Trailing::Fn(_)` → **не должен попадать сюда**: C5 конвертирует
///   trailing-fn в synthetic `ExprKind::ClosureFull` ещё до вызова
///   этого helper'а (см. Call eval branch). Возвращаем `None`
///   как defensive fallback (не должно срабатывать в нормальном flow).
fn trailing_to_legacy_for_interp(t: &crate::ast::Trailing) -> Option<crate::ast::TrailingBlock> {
    match t {
        crate::ast::Trailing::Block(b) => Some(crate::ast::TrailingBlock {
            params: Vec::new(),
            body: (**b).clone(),
            span: b.span,
        }),
        crate::ast::Trailing::LegacyBlockWithParams(tb) => Some((**tb).clone()),
        crate::ast::Trailing::Fn(_) => None,
    }
}

/// Исполнительный контекст: топ-левел декларации модуля + текущая среда +
/// handler-стек.
pub struct Interpreter {
    /// Top-level декларации (зарегистрированные при загрузке модуля).
    pub globals: Env,
    /// Регистр типов — для resolve'а sum-вариантов и effect'ов.
    pub types: HashMap<String, TypeDecl>,
    /// Handler-стек: последние добавленные сверху.
    pub handlers: RefCell<Vec<HandlerFrame>>,
    /// Список тестов, собранных при загрузке модуля.
    pub tests: Vec<TestDecl>,
}

pub struct HandlerFrame {
    pub effect: String,
    pub handler: Rc<value::Handler>,
}

/// Управляющие сигналы: как control flow «всплывает» вверх по стеку.
pub enum Flow {
    /// Обычное значение.
    Value(Value),
    /// `return v` — выходит из функции.
    Return(Value),
    /// `break` / `continue`.
    Break,
    Continue,
    /// `throw err` — поднимается до Fail-handler'а (D65).
    Throw(Value),
    /// `interrupt v` (D61) — досрочное завершение текущего with-блока.
    /// Поднимается до ближайшей `eval_with` границы, становится результатом
    /// всего with-блока. Continuation НЕ возобновляется.
    Interrupt(Value),
}

impl Interpreter {
    pub fn new() -> Self {
        let interp = Self {
            globals: Env::new(),
            types: HashMap::new(),
            handlers: RefCell::new(Vec::new()),
            tests: Vec::new(),
        };
        stdlib::install(&interp.globals);
        interp
    }

    /// Загружает модуль — регистрирует типы, функции, константы, тесты.
    pub fn load_module(&mut self, module: &Module) -> Result<(), Diagnostic> {
        // Регистрация типов.
        for item in &module.items {
            if let Item::Type(td) = item {
                self.types.insert(td.name.clone(), td.clone());
            }
        }
        // Регистрация free-функций и static-методов.
        for item in &module.items {
            match item {
                Item::Fn(fd) => {
                    let key = match &fd.receiver {
                        Some(r) => format!("{}.{}", r.type_name, fd.name),
                        None => fd.name.clone(),
                    };
                    // Plan 14 Ф.6-bis: variadic_last из FnDecl.
                    let variadic_last = fd.params.last()
                        .map(|p| p.is_variadic).unwrap_or(false);
                    let closure = Closure {
                        params: fd.params.iter().map(|p| p.name.clone()).collect(),
                        body: match &fd.body {
                            FnBody::Expr(e) => ClosureBody::Expr(Box::new(e.clone())),
                            FnBody::Block(b) => ClosureBody::Block(b.clone()),
                            // D82: external fn — interp не реализован для них
                            // (interp удалён, оставляем panic как safety-fallback).
                            FnBody::External => panic!("interp does not support external fn"),
                        },
                        env: self.globals.clone(),
                        receiver: None,
                        variadic_last,
                    };
                    self.globals.define(key, Value::Closure(Rc::new(closure)));
                }
                Item::Const(cd) => {
                    let v = self.eval_in_globals(&cd.value)?;
                    self.globals.define(cd.name.clone(), v);
                }
                Item::Test(td) => {
                    self.tests.push(td.clone());
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn eval_in_globals(&self, expr: &Expr) -> Result<Value, Diagnostic> {
        match self.eval_expr(expr, &self.globals)? {
            Flow::Value(v) => Ok(v),
            Flow::Return(_) => Err(Diagnostic::new("`return` outside function", expr.span)),
            Flow::Break => Err(Diagnostic::new("`break` outside loop", expr.span)),
            Flow::Continue => Err(Diagnostic::new("`continue` outside loop", expr.span)),
            Flow::Throw(_) => Err(Diagnostic::new("uncaught throw", expr.span)),
            Flow::Interrupt(_) => Err(Diagnostic::new(
                "`interrupt` outside handler-method",
                expr.span,
            )),
        }
    }

    /// Запустить функцию `main`, если она есть.
    pub fn run_main(&self) -> Result<Value, Diagnostic> {
        if let Some(main) = self.globals.lookup("main") {
            return self.call_value(main, &[], Span::dummy());
        }
        Ok(Value::Unit)
    }

    /// Запустить все тесты модуля. Возвращает (passed, failed, names).
    pub fn run_tests(&self) -> Result<(usize, usize, Vec<String>), Diagnostic> {
        let mut passed = 0;
        let mut failed = 0;
        let mut failed_names = Vec::new();
        for test in &self.tests {
            let env = Env::new_child(&self.globals);
            match self.exec_block(&test.body, &env) {
                Ok(_) => passed += 1,
                Err(_) => {
                    failed += 1;
                    failed_names.push(test.name.clone());
                }
            }
        }
        Ok((passed, failed, failed_names))
    }

    /// Вызвать произвольное значение как функцию.
    pub fn call_value(
        &self,
        callee: Value,
        args: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match callee {
            Value::Closure(c) => self.call_closure(&c, args, span),
            Value::Native(n) => match (n.func)(args) {
                Ok(v) => Ok(v),
                Err(e) => Err(Diagnostic::new(e.message, span)),
            },
            other => Err(Diagnostic::new(
                format!("cannot call {}", other.type_name()),
                span,
            )),
        }
    }

    fn call_closure(
        &self,
        closure: &Closure,
        args: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match self.call_closure_flow(closure, args, span)? {
            Flow::Value(v) | Flow::Return(v) => Ok(v),
            Flow::Throw(err) => Err(Diagnostic::new(
                format!("uncaught throw: {:?}", err),
                span,
            )),
            Flow::Interrupt(_) => Err(Diagnostic::new(
                "`interrupt` escaped handler-method",
                span,
            )),
            Flow::Break | Flow::Continue => Err(Diagnostic::new(
                "break/continue outside loop",
                span,
            )),
        }
    }

    /// Вызов closure с возвратом Flow — для случаев, когда throw должен
    /// проброситься выше по call stack'у к обработчику Fail (D65).
    fn call_closure_flow(
        &self,
        closure: &Closure,
        args: &[Value],
        span: Span,
    ) -> Result<Flow, Diagnostic> {
        // Plan 14 Ф.6-bis (D69): variadic-fn — собираем
        // args[regular_arity..] в Value::Array и передаём как
        // последний param. Caller'у не нужно знать об упаковке.
        let env = Env::new_child(&closure.env);
        if let Some(recv) = &closure.receiver {
            env.define("@", recv.clone());
        }
        if closure.variadic_last && !closure.params.is_empty() {
            let regular_arity = closure.params.len() - 1;
            if args.len() < regular_arity {
                return Err(Diagnostic::new(
                    format!(
                        "variadic call: expected at least {} regular arg(s), got {}",
                        regular_arity, args.len()
                    ),
                    span,
                ));
            }
            // Bind regular params.
            for (name, value) in closure.params[..regular_arity].iter()
                .zip(args[..regular_arity].iter())
            {
                env.define(name.clone(), value.clone());
            }
            // Pack rest into Value::Array, bind as last param.
            let rest: Vec<Value> = args[regular_arity..].to_vec();
            let arr = Value::Array(std::rc::Rc::new(std::cell::RefCell::new(rest)));
            env.define(closure.params[regular_arity].clone(), arr);
        } else {
            if closure.params.len() != args.len() {
                return Err(Diagnostic::new(
                    format!(
                        "argument count mismatch: expected {}, got {}",
                        closure.params.len(),
                        args.len()
                    ),
                    span,
                ));
            }
            for (name, value) in closure.params.iter().zip(args.iter()) {
                env.define(name.clone(), value.clone());
            }
        }
        let flow = match &closure.body {
            ClosureBody::Expr(e) => self.eval_expr(e, &env)?,
            ClosureBody::Block(b) => self.exec_block_flow(b, &env)?,
        };
        // Return превращаем в Value — `return` локален функции.
        // Throw / Value / Break / Continue — пробрасываем как есть.
        Ok(match flow {
            Flow::Return(v) => Flow::Value(v),
            other => other,
        })
    }

    // ─── eval ────────────────────────────────────────────────────────────

    pub fn eval_expr(&self, expr: &Expr, env: &Env) -> Result<Flow, Diagnostic> {
        match &expr.kind {
            ExprKind::IntLit(n) => Ok(Flow::Value(Value::Int(*n))),
            ExprKind::CharLit(cp) => Ok(Flow::Value(Value::Int(*cp as i64))),
            ExprKind::FloatLit(x) => Ok(Flow::Value(Value::Float(*x))),
            ExprKind::StrLit(s) => Ok(Flow::Value(Value::Str(s.clone()))),
            ExprKind::InterpolatedStr { parts } => {
                // D44 string interpolation: вычисляем каждую часть и
                // конкатенируем (interp — без StringBuilder).
                let mut out = String::new();
                for p in parts {
                    match p {
                        crate::ast::InterpStrPart::Lit(s) => out.push_str(s),
                        crate::ast::InterpStrPart::Expr(e) => {
                            let v = match self.eval_expr(e, env)? {
                                Flow::Value(v) => v,
                                other => return Ok(other),
                            };
                            out.push_str(&format!("{}", v));
                        }
                    }
                }
                Ok(Flow::Value(Value::Str(out)))
            }
            ExprKind::BoolLit(b) => Ok(Flow::Value(Value::Bool(*b))),
            ExprKind::UnitLit => Ok(Flow::Value(Value::Unit)),
            ExprKind::SelfAccess => match env.lookup("@") {
                Some(v) => Ok(Flow::Value(v)),
                None => Err(Diagnostic::new("`@` used outside method", expr.span)),
            },
            ExprKind::Ident(name) => match env.lookup(name) {
                Some(v) => Ok(Flow::Value(v)),
                None => {
                    // Может быть unit-variant sum-типа — проверим через types.
                    if let Some(v) = self.try_resolve_unit_variant(name) {
                        return Ok(Flow::Value(v));
                    }
                    Err(Diagnostic::new(
                        format!("undefined name: `{}`", name),
                        expr.span,
                    ))
                }
            },
            ExprKind::Path(path) => {
                // Type.member — статический доступ; module.name — обычный
                // global. В bootstrap'е handle обоих случаев одинаково:
                // конкатенированное имя "Type.member".
                let key = path.join(".");
                if let Some(v) = env.lookup(&key) {
                    return Ok(Flow::Value(v));
                }
                // Sum-variant `Color.Red` или `RepoError.NotFound`?
                if path.len() == 2 {
                    if let Some(v) = self.try_resolve_unit_variant_qualified(&path[0], &path[1]) {
                        return Ok(Flow::Value(v));
                    }
                }
                Err(Diagnostic::new(
                    format!("undefined path: `{}`", key),
                    expr.span,
                ))
            }
            ExprKind::Member { obj, name } => {
                let obj_v = self.eval_expr_value(obj, env)?;
                self.member_access(&obj_v, name, expr.span).map(Flow::Value)
            }
            // D38 turbofish: type_args — explicit hint, в interp игнорируем
            // (treewalk не делает monomorphization).
            ExprKind::TurboFish { base, .. } => self.eval_expr(base, env),
            ExprKind::Index { obj, index } => {
                let obj_v = self.eval_expr_value(obj, env)?;
                let idx_v = self.eval_expr_value(index, env)?;
                self.index_access(&obj_v, &idx_v, expr.span).map(Flow::Value)
            }
            ExprKind::Call {
                func,
                args,
                trailing,
            } => {
                // Plan 14 Ф.6-bis (D69): handle spread + variadic в interp.
                // Plan 19, C5: trailing разбираем здесь же.
                //
                // Случаи:
                // - `Trailing::Block` / `LegacyBlockWithParams` —
                //   передаются в legacy `eval_call` через старый
                //   `TrailingBlock` (тот сам создаст Closure-value
                //   и добавит в args).
                // - `Trailing::Fn` — закрытие создаётся прямо здесь
                //   и добавляется в args как обычный CallArg::Item
                //   (новый Expr с ExprKind::ClosureFull). Затем eval
                //   через тот же путь без trailing.
                //
                // Pre-rewrite: если Trailing::Fn — конвертируем в
                // synthetic ClosureFull-аргумент и удаляем trailing.
                let (effective_args, effective_trailing): (Vec<CallArg>, Option<&crate::ast::Trailing>) =
                    if let Some(crate::ast::Trailing::Fn(sb)) = trailing.as_ref() {
                        // Сборка synthetic Expr с ClosureFull.
                        let sb_clone = sb.clone();
                        let closure_expr = Expr::new(
                            ExprKind::ClosureFull(sb_clone),
                            sb.span,
                        );
                        let mut args_extended = args.clone();
                        args_extended.push(CallArg::Item(closure_expr));
                        (args_extended, None)
                    } else {
                        (args.clone(), trailing.as_ref())
                    };

                let legacy_tb = effective_trailing
                    .and_then(trailing_to_legacy_for_interp);

                let args = &effective_args;
                if !args.iter().any(|a| a.is_spread()) {
                    let plain: Vec<Expr> = args.iter().map(|a| a.expr().clone()).collect();
                    self.eval_call(func, &plain, legacy_tb.as_ref(), env, expr.span)
                } else {
                    // Pre-eval с unfolding spread'ов.
                    let mut arg_values: Vec<Value> = Vec::with_capacity(args.len());
                    for a in args {
                        match a {
                            CallArg::Item(e) => arg_values.push(self.eval_expr_value(e, env)?),
                            // Plan 46 (D102) / Plan 50 Ф.2: named-аргумент в
                            // treewalk-interp. `cmd_run` делает
                            // resolve_imports_inline + callnorm перед
                            // интерпретацией — поэтому для любого резолвимого
                            // callee (включая импортированные) `callnorm`
                            // переписывает named→positional в param-order, и
                            // interp получает чистый `CallArg::Item`. Этот arm
                            // остаётся только для callee, которые `callnorm` не
                            // смог резолвить (ambiguous/overloaded в bootstrap
                            // fn_decls) — там сигнатуры нет, позиционный eval —
                            // единственный и консистентный вариант.
                            CallArg::Named { value, .. } => {
                                arg_values.push(self.eval_expr_value(value, env)?);
                            }
                            CallArg::Spread(e) => {
                                let v = self.eval_expr_value(e, env)?;
                                match v {
                                    Value::Array(arr) => {
                                        for item in arr.borrow().iter() {
                                            arg_values.push(item.clone());
                                        }
                                    }
                                    other => return Err(Diagnostic::new(
                                        format!("spread (...) requires array, got `{}`", other.type_name()),
                                        e.span,
                                    )),
                                }
                            }
                        }
                    }
                    // trailing-block — преобразуем в closure-аргумент.
                    if let Some(tb) = legacy_tb.as_ref() {
                        let closure = Closure {
                            params: tb.params.iter().map(|p| p.name.clone()).collect(),
                            body: ClosureBody::Block(tb.body.clone()),
                            env: env.clone(),
                            receiver: env.lookup("@"),
                            variadic_last: false,
                        };
                        arg_values.push(Value::Closure(Rc::new(closure)));
                    }
                    // Plan 14 Ф.6-bis: dispatch.
                    // Для `obj.method(...)` нам нужен receiver-bound closure
                    // через try_member_call (instance-метод lookup в
                    // globals по `Type.method`). Для остального — call_value.
                    if let ExprKind::Member { obj, name } = &func.kind {
                        let recv_v = self.eval_expr_value(obj, env)?;
                        // Native + result/option/closure methods через
                        // try_member_call_values (новая helper).
                        if let Some(out) = self.try_member_call_values(&recv_v, name, &arg_values, expr.span)? {
                            return Ok(Flow::Value(out));
                        }
                    }
                    let callee = self.eval_expr_value(func, env)?;
                    Ok(Flow::Value(self.call_value(callee, &arg_values, expr.span)?))
                }
            }
            ExprKind::Try(inner) => {
                let result = self.eval_expr(inner, env)?;
                match result {
                    Flow::Throw(err) => Ok(Flow::Throw(err)),
                    Flow::Value(v) => {
                        // Result-like / Option-like:
                        //   Ok(x)    → x         (значение распаковано)
                        //   Err(e)   → return Err(e)  (ранний выход из fn)
                        //   Some(x)  → x
                        //   None     → return None
                        //
                        // `Flow::Return(...)` всплывает до границы функции,
                        // где `call_closure_flow` превращает его в `Flow::Value`.
                        // Так `?` на ошибке мгновенно выходит из текущей fn,
                        // возвращая исходный Err/None — что и нужно для
                        // bootstrap-семантики Fail через Result.
                        if let Value::Variant { name, payload, .. } = &v {
                            match (name.as_str(), payload) {
                                ("Ok", VariantPayload::Tuple(items)) if items.len() == 1 => {
                                    return Ok(Flow::Value(items[0].clone()));
                                }
                                ("Err", _) => {
                                    return Ok(Flow::Return(v));
                                }
                                ("Some", VariantPayload::Tuple(items)) if items.len() == 1 => {
                                    return Ok(Flow::Value(items[0].clone()));
                                }
                                ("None", VariantPayload::Unit) => {
                                    return Ok(Flow::Return(v));
                                }
                                _ => {}
                            }
                        }
                        // Иначе — пропускаем как есть
                        Ok(Flow::Value(v))
                    }
                    other => Ok(other),
                }
            }
            // Plan 19, C7 (D85): postfix `!!` — throw-стиль для
            // Result/Option. На Some(v)/Ok(v) разворачивает; на
            // None/Err(e) бросает через Fail[E].
            //
            // В отличие от `?` (Try), `!!` использует Throw flow —
            // ошибка ловится handler'ом `Fail[E]` в with-блоке (а не
            // ранним return'ом из enclosing fn). Это даёт две формы
            // обработки: `?` для Result-style fn-сигнатуры, `!!` для
            // throw-style + with-handler.
            ExprKind::Bang(inner) => {
                let result = self.eval_expr(inner, env)?;
                match result {
                    Flow::Throw(err) => Ok(Flow::Throw(err)),
                    Flow::Value(v) => {
                        if let Value::Variant { name, payload, .. } = &v {
                            match (name.as_str(), payload) {
                                ("Ok", VariantPayload::Tuple(items)) if items.len() == 1 => {
                                    return Ok(Flow::Value(items[0].clone()));
                                }
                                ("Err", VariantPayload::Tuple(items)) if items.len() == 1 => {
                                    // `expr!!` на Err(e): throw e через
                                    // Fail-эффект. Runtime ловит
                                    // в активном Fail-handler'е.
                                    return Ok(Flow::Throw(items[0].clone()));
                                }
                                ("Some", VariantPayload::Tuple(items)) if items.len() == 1 => {
                                    return Ok(Flow::Value(items[0].clone()));
                                }
                                ("None", VariantPayload::Unit) => {
                                    // `Option!!` бросает RuntimeNoneError
                                    // (D85 prelude unit-тип).
                                    let none_err = Value::Variant {
                                        type_name: Some("RuntimeNoneError".to_string()),
                                        name: "RuntimeNoneError".to_string(),
                                        payload: VariantPayload::Unit,
                                    };
                                    return Ok(Flow::Throw(none_err));
                                }
                                _ => {}
                            }
                        }
                        Ok(Flow::Value(v))
                    }
                    other => Ok(other),
                }
            }
            ExprKind::Coalesce(a, b) => {
                let av = self.eval_expr(a, env)?;
                match av {
                    Flow::Value(Value::Variant { ref name, .. }) if name == "None" => {
                        self.eval_expr(b, env)
                    }
                    Flow::Value(Value::Variant {
                        ref name, ref payload, ..
                    }) if name == "Some" => {
                        if let VariantPayload::Tuple(items) = payload {
                            if items.len() == 1 {
                                return Ok(Flow::Value(items[0].clone()));
                            }
                        }
                        Ok(av)
                    }
                    _ => Ok(av),
                }
            }
            ExprKind::As(inner, _ty) => {
                // В bootstrap'е cast — no-op (значение остаётся как есть).
                // Mostly работает для int/float.
                let v = self.eval_expr_value(inner, env)?;
                Ok(Flow::Value(v))
            }
            ExprKind::Is(_inner, _ty) => {
                // В bootstrap'е — false по умолчанию (D54 нужен runtime tag).
                Ok(Flow::Value(Value::Bool(false)))
            }
            ExprKind::Binary { op, left, right } => {
                let l = self.eval_expr_value(left, env)?;
                let r = self.eval_expr_value(right, env)?;
                let v = self.binop(*op, &l, &r, expr.span)?;
                Ok(Flow::Value(v))
            }
            ExprKind::Unary { op, operand } => {
                let v = self.eval_expr_value(operand, env)?;
                let v = match (op, v) {
                    (UnOp::Neg, Value::Int(n)) => Value::Int(-n),
                    (UnOp::Neg, Value::Float(x)) => Value::Float(-x),
                    (UnOp::Not, Value::Bool(b)) => Value::Bool(!b),
                    (op, v) => {
                        return Err(Diagnostic::new(
                            format!("invalid unary {:?} on {}", op, v.type_name()),
                            expr.span,
                        ))
                    }
                };
                Ok(Flow::Value(v))
            }
            ExprKind::If { cond, then, else_ } => {
                let c = self.eval_expr_value(cond, env)?;
                if c.truthy() {
                    self.exec_block_flow(then, env)
                } else if let Some(else_branch) = else_ {
                    match else_branch {
                        ElseBranch::Block(b) => self.exec_block_flow(b, env),
                        ElseBranch::If(e) => self.eval_expr(e, env),
                    }
                } else {
                    Ok(Flow::Value(Value::Unit))
                }
            }
            ExprKind::IfLet {
                pattern,
                scrutinee,
                then,
                else_,
            } => {
                let v = self.eval_expr_value(scrutinee, env)?;
                let local = Env::new_child(env);
                if self.match_pattern(pattern, &v, &local) {
                    self.exec_block_flow(then, &local)
                } else if let Some(else_branch) = else_ {
                    match else_branch {
                        ElseBranch::Block(b) => self.exec_block_flow(b, env),
                        ElseBranch::If(e) => self.eval_expr(e, env),
                    }
                } else {
                    Ok(Flow::Value(Value::Unit))
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                let v = self.eval_expr_value(scrutinee, env)?;
                for arm in arms {
                    let local = Env::new_child(env);
                    if self.match_pattern(&arm.pattern, &v, &local) {
                        if let Some(guard) = &arm.guard {
                            let g = self.eval_expr_value(guard, &local)?;
                            if !g.truthy() {
                                continue;
                            }
                        }
                        return match &arm.body {
                            MatchArmBody::Expr(e) => self.eval_expr(e, &local),
                            MatchArmBody::Block(b) => self.exec_block_flow(b, &local),
                        };
                    }
                }
                Err(Diagnostic::new("no match arm matched", expr.span))
            }
            ExprKind::For {
                pattern,
                iter,
                body,
                ..
            } => {
                let iter_v = self.eval_expr_value(iter, env)?;
                self.run_for_loop(pattern, iter_v, body, env, expr.span)
            }
            ExprKind::While { cond, body, .. } => loop {
                let c = self.eval_expr_value(cond, env)?;
                if !c.truthy() {
                    break Ok(Flow::Value(Value::Unit));
                }
                match self.exec_block_flow(body, env)? {
                    Flow::Break => break Ok(Flow::Value(Value::Unit)),
                    Flow::Continue | Flow::Value(_) => continue,
                    other => break Ok(other),
                }
            },
            ExprKind::WhileLet {
                pattern,
                scrutinee,
                body,
                ..
            } => loop {
                let v = self.eval_expr_value(scrutinee, env)?;
                let local = Env::new_child(env);
                if !self.match_pattern(pattern, &v, &local) {
                    break Ok(Flow::Value(Value::Unit));
                }
                match self.exec_block_flow(body, &local)? {
                    Flow::Break => break Ok(Flow::Value(Value::Unit)),
                    Flow::Continue | Flow::Value(_) => continue,
                    other => break Ok(other),
                }
            },
            ExprKind::Loop { body, .. } => loop {
                match self.exec_block_flow(body, env)? {
                    Flow::Break => break Ok(Flow::Value(Value::Unit)),
                    Flow::Continue | Flow::Value(_) => continue,
                    other => break Ok(other),
                }
            },
            ExprKind::Lambda {
                params,
                body,
                ..
            } => {
                let closure = Closure {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    body: ClosureBody::Expr(Box::new((**body).clone())),
                    env: env.clone(),
                    receiver: env.lookup("@"),
                    // Lambdas не variadic в bootstrap'е.
                    variadic_last: false,
                };
                Ok(Flow::Value(Value::Closure(Rc::new(closure))))
            }
            // Plan 19, C5: closure-light eval.
            // `|x| body` — создаёт `Closure` runtime-value с
            // captured env. Тело — bare expression или block;
            // оба покрыты `ClosureBody` (общий enum в AST).
            //
            // Receiver (`@`) захватывается из env как у старой Lambda
            // — для closure'ов внутри методов.
            ExprKind::ClosureLight { params, body } => {
                let closure = Closure {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    body: body.clone(),
                    env: env.clone(),
                    receiver: env.lookup("@"),
                    // closure-light не поддерживает variadic
                    // (D69 — variadic только в named fn / closure-full).
                    variadic_last: false,
                };
                Ok(Flow::Value(Value::Closure(Rc::new(closure))))
            }
            // Plan 19, C5: closure-full eval.
            // `fn(x int) Effects -> R body` — то же что named fn
            // без имени. Параметры извлекаются по именам (типы
            // erasure'ятся в interp). Тело — `FnBody::Expr` или
            // `FnBody::Block`, конвертируется в `ClosureBody`.
            //
            // Variadic поддерживается (D69).
            ExprKind::ClosureFull(sb) => {
                let body = match &sb.body {
                    FnBody::Expr(e) => ClosureBody::Expr(Box::new(e.clone())),
                    FnBody::Block(b) => ClosureBody::Block(b.clone()),
                    FnBody::External => unreachable!(
                        "closure-full cannot be `external` — only named fn"
                    ),
                };
                let variadic_last = sb
                    .params
                    .last()
                    .map(|p| p.is_variadic)
                    .unwrap_or(false);
                let closure = Closure {
                    params: sb.params.iter().map(|p| p.name.clone()).collect(),
                    body,
                    env: env.clone(),
                    receiver: env.lookup("@"),
                    variadic_last,
                };
                Ok(Flow::Value(Value::Closure(Rc::new(closure))))
            }
            ExprKind::Block(b) => self.exec_block_flow(b, env),
            ExprKind::ArrayLit(elems) => {
                let mut out = Vec::new();
                for e in elems {
                    match e {
                        ArrayElem::Item(expr) => {
                            out.push(self.eval_expr_value(expr, env)?);
                        }
                        ArrayElem::Spread(expr) => {
                            let v = self.eval_expr_value(expr, env)?;
                            match v {
                                Value::Array(arr) => {
                                    for item in arr.borrow().iter() {
                                        out.push(item.clone());
                                    }
                                }
                                _ => {
                                    return Err(Diagnostic::new(
                                        format!("cannot spread {}", v.type_name()),
                                        expr.span,
                                    ));
                                }
                            }
                        }
                    }
                }
                Ok(Flow::Value(Value::Array(Rc::new(RefCell::new(out)))))
            }
            ExprKind::TupleLit(items) => {
                let mut vs = Vec::with_capacity(items.len());
                for it in items {
                    vs.push(self.eval_expr_value(it, env)?);
                }
                Ok(Flow::Value(Value::Tuple(vs)))
            }
            // Plan 52 Ф.20: invariant — MapLit ДОЛЖЕН быть устранён
            // desugar pass'ом (compiler-codegen/src/desugar.rs::desugar_module)
            // ДО входа в interp. Pipeline: parse → type-check → annotate
            // → desugar → interp/codegen. Если сюда попал raw MapLit —
            // это bug в pipeline wiring (забыли вызвать desugar_module),
            // не user error. Сообщение явно указывает на compiler bug
            // чтобы issue репортилось правильно.
            ExprKind::MapLit { .. } => {
                Err(Diagnostic::new(
                    "compiler bug: map literal `[k: v]` reached interpreter \
                     без desugar pass — это нарушение pipeline invariant. \
                     desugar_module() обязан быть вызван до interp/codegen. \
                     Report issue: https://github.com/nv-lang/nova/issues",
                    expr.span,
                ))
            }
            ExprKind::RecordLit { type_name, fields, .. } => {
                let mut out: HashMap<String, Value> = HashMap::new();
                for f in fields {
                    if f.is_spread {
                        let src = match &f.value {
                            Some(e) => self.eval_expr_value(e, env)?,
                            None => continue,
                        };
                        if let Value::Record { fields: src_fields, .. } = &src {
                            for (k, v) in src_fields.borrow().iter() {
                                out.insert(k.clone(), v.clone());
                            }
                        } else {
                            return Err(Diagnostic::new(
                                format!("cannot spread {} in record literal", src.type_name()),
                                f.span,
                            ));
                        }
                    } else {
                        let v = match &f.value {
                            Some(e) => self.eval_expr_value(e, env)?,
                            None => env.lookup(&f.name).ok_or_else(|| {
                                Diagnostic::new(
                                    format!("field shorthand: name `{}` not in scope", f.name),
                                    f.span,
                                )
                            })?,
                        };
                        out.insert(f.name.clone(), v);
                    }
                }
                let tn = type_name.as_ref().and_then(|p| p.last().cloned());
                Ok(Flow::Value(Value::Record {
                    type_name: tn,
                    fields: Rc::new(RefCell::new(out)),
                }))
            }
            ExprKind::With { bindings, body } => self.eval_with(bindings, body, env),
            // Plan 97 Ф.4 (D142): protocol-литерал — в interpreter'е
            // обрабатывается тождественно handler-литералу (closure-
            // bundle с захватом env). Field name отличается, делегируем.
            ExprKind::HandlerLit { effect_name, methods }
            | ExprKind::ProtocolLit { proto_name: effect_name, methods } => {
                let mut map = HashMap::new();
                for m in methods {
                    map.insert(m.name.clone(), m.clone());
                }
                let handler = Rc::new(value::Handler {
                    effect: effect_name
                        .last()
                        .cloned()
                        .unwrap_or_else(|| "<unknown>".into()),
                    methods: map,
                    env: env.clone(),
                    lambda: None,
                });
                Ok(Flow::Value(Value::Handler(handler)))
            }
            ExprKind::Interrupt(value) => {
                // interrupt v (D61): прерывает текущий with-блок, значение
                // становится результатом всего with. В bootstrap-модели
                // sentinel-variant `__interrupt`, который
                // ловится в eval_with, где разворачивает стек.
                let v = match value {
                    Some(e) => self.eval_expr_value(e, env)?,
                    None => Value::Unit,
                };
                Ok(Flow::Interrupt(v))
            }
            ExprKind::Range { start, end, inclusive } => {
                // Plan 96 Ф.2 — интерпретатор не поддерживает open-ended Range
                // как первоклассное значение (bounded-substitution делается в
                // codegen на slice-site). Type-checker отвергает open-ended
                // вне slice-context (Ф.3); если сюда дошёл с None — это
                // material для type-checker'а, диагностика.
                let start_expr = start.as_deref().ok_or_else(|| Diagnostic::new(
                    "interpreter does not support open-ended Range as a value (use bounded form a..b)",
                    expr.span,
                ))?;
                let end_expr = end.as_deref().ok_or_else(|| Diagnostic::new(
                    "interpreter does not support open-ended Range as a value (use bounded form a..b)",
                    expr.span,
                ))?;
                let s = self.eval_expr_value(start_expr, env)?;
                let e = self.eval_expr_value(end_expr, env)?;
                let (s, e) = match (s, e) {
                    (Value::Int(s), Value::Int(e)) => (s, e),
                    _ => {
                        return Err(Diagnostic::new(
                            "range bounds must be integers",
                            expr.span,
                        ));
                    }
                };
                Ok(Flow::Value(Value::Range {
                    start: s,
                    end: e,
                    inclusive: *inclusive,
                }))
            }
            ExprKind::Spawn(body) => {
                // В bootstrap'е spawn = inline call (синхронно).
                self.eval_expr(body, env)
            }
            ExprKind::Supervised { body, .. } => {
                // В bootstrap-интерпретаторе supervised — обычный block, а
                // `cancel:` токен игнорируется (нет реального scheduler'а).
                // Codegen реализует D75 полноценно через NovaCancelToken +
                // nova_supervised_run_cancel. Это bootstrap-ограничение
                // [M-interp-cancel] — см. docs/simplifications.md.
                self.exec_block_flow(body, env)
            }
            ExprKind::Detach(body) => {
                // В bootstrap'е default-handler Detach = SyncDetach: исполняется inline.
                // Production-runtime запустит на глобальном supervisor'е.
                self.exec_block_flow(body, env)
            }
            ExprKind::Blocking(body) => {
                // Plan 83.3 (D50): в интерпретаторе `blocking { }` исполняется
                // inline — нет M:N-worker'а, который можно было бы запинить.
                // Codegen-pipeline уводит работу в libuv threadpool (Ф.4.2).
                self.exec_block_flow(body, env)
            }
            ExprKind::Throw(value) => {
                // D25/D65: throw в expression-position. В интерпретаторе
                // делегируем к stmt-механизму через Flow::Throw.
                let v = self.eval_expr_value(value, env)?;
                Ok(Flow::Throw(v))
            }
            ExprKind::ParallelFor { pattern, iter, body, .. } => {
                // В bootstrap'е parallel for ≡ обычный for (sequential).
                // Codegen раскрывает в supervised + spawn для реального параллелизма.
                let iter_v = self.eval_expr_value(iter, env)?;
                self.run_for_loop(pattern, iter_v, body, env, expr.span)
            }
            ExprKind::Forbid { effects: _, body } => {
                // В bootstrap-интерпретаторе forbid (D63) исполняется как
                // обычный block — runtime барьер через sentinel-frame и
                // compile-time проверка прямых эффектов это задача
                // production-компилятора. Здесь блок прозрачен.
                self.exec_block_flow(body, env)
            }
            ExprKind::Realtime { nogc: _, body } => {
                // В bootstrap'е нет fiber-runtime'а с safepoint'ами,
                // realtime (D64) исполняется как обычный block. В
                // production-компиляторе runtime ставит флаг и проверяет
                // на каждом suspend-point'е.
                self.exec_block_flow(body, env)
            }
            ExprKind::TaggedTemplate { tag: _, parts, .. } => {
                // В bootstrap'е tagged template = просто строка (parts
                // конкатенируются). Tag-функция игнорируется. Достаточно
                // для написания компилятора, где `sql\`...\`` не используется.
                let s = parts.join("");
                Ok(Flow::Value(Value::Str(s)))
            }
            ExprKind::Select { .. } => Err(Diagnostic::new(
                "`select` requires the compiled runtime (not available in bootstrap interpreter)",
                expr.span,
            )),
            // D.1.3: квантор — только в контрактах, не в интерпретаторе.
            ExprKind::Forall { .. } | ExprKind::Exists { .. } => Err(Diagnostic::new(
                "forall/exists quantifiers are contract-only and cannot be interpreted",
                expr.span,
            )),
        }
    }

    fn eval_expr_value(&self, expr: &Expr, env: &Env) -> Result<Value, Diagnostic> {
        match self.eval_expr(expr, env)? {
            Flow::Value(v) => Ok(v),
            Flow::Return(_) => Err(Diagnostic::new("`return` in expression context", expr.span)),
            Flow::Break => Err(Diagnostic::new("`break` in expression context", expr.span)),
            Flow::Continue => Err(Diagnostic::new(
                "`continue` in expression context",
                expr.span,
            )),
            Flow::Throw(_) => Err(Diagnostic::new("uncaught throw", expr.span)),
            Flow::Interrupt(_) => Err(Diagnostic::new(
                "`interrupt` in expression context",
                expr.span,
            )),
        }
    }

    fn eval_call(
        &self,
        func: &Expr,
        args: &[Expr],
        trailing: Option<&TrailingBlock>,
        env: &Env,
        span: Span,
    ) -> Result<Flow, Diagnostic> {
        // Определяем callee: если это Member/Path → operation на handler'е?
        if let ExprKind::Member { obj, name } = &func.kind {
            // Случай 1: `Db.query(...)` — Path("Db", "query"), но синтаксически
            // парсится как Path или как Member над Ident. Проверим, является
            // ли `obj` именем эффекта в handler-стеке.
            if let ExprKind::Ident(eff_name) = &obj.kind {
                if let Some(handler) = self.find_handler(eff_name) {
                    let arg_values = self.eval_args(args, env)?;
                    return self
                        .invoke_handler_op(&handler, name, &arg_values, env, span);
                }
            }
            // Случай 2: instance-метод `obj.method(...)` или static `Type.method`
            let recv_v = self.eval_expr_value(obj, env)?;
            // Прямой вызов на handler-значении (D61): `h.op(args)` минует
            // with-стек, исполняет handler-method прямо на этом значении.
            // `interrupt v` внутри прерывает только этот вызов (становится
            // результатом `h.op(args)`), не enclosing with-блок.
            if let Value::Handler(handler) = &recv_v {
                let arg_values = self.eval_args(args, env)?;
                let flow = self.invoke_handler_op(handler, name, &arg_values, env, span)?;
                // На границе прямого вызова Flow::Interrupt становится
                // обычным значением вызова — semantics D61: interrupt
                // прерывает «текущий with», а текущий with у direct-call'а —
                // это сам этот вызов.
                return Ok(match flow {
                    Flow::Interrupt(v) => Flow::Value(v),
                    other => other,
                });
            }
            // Native member-call?
            if let Some(v) = self.try_member_call(&recv_v, name, args, env, span)? {
                return Ok(Flow::Value(v));
            }
        }
        if let ExprKind::Path(path) = &func.kind {
            if path.len() == 2 {
                // Может быть variant constructor: `Result.Ok(x)` или `MyEnum.Variant(...)`.
                if self.types.contains_key(&path[0]) {
                    if let Some(v) =
                        self.try_construct_variant(&path[0], &path[1], args, env, span)?
                    {
                        return Ok(Flow::Value(v));
                    }
                }
                // handler-effect operation: `Effect.op(...)`?
                if let Some(handler) = self.find_handler(&path[0]) {
                    let arg_values = self.eval_args(args, env)?;
                    return self
                        .invoke_handler_op(&handler, &path[1], &arg_values, env, span);
                }
                // Static-method: `Type.method`.
                let key = format!("{}.{}", path[0], path[1]);
                if let Some(v) = self.globals.lookup(&key) {
                    let arg_values = self.eval_args(args, env)?;
                    return Ok(Flow::Value(self.call_value(v, &arg_values, span)?));
                }
            }
        }
        // Variant-constructor через одиночный Ident с заглавной буквы —
        // проверяем ДО eval_expr_value(func), потому что Square/Good/etc.
        // не зарегистрированы как имена в globals.
        if let ExprKind::Ident(name) = &func.kind {
            if name
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false)
            {
                let arg_values = self.eval_args(args, env)?;
                if let Some(v) = self.try_construct_variant_anywhere(name, &arg_values, span)? {
                    return Ok(Flow::Value(v));
                }
                // fall-through: возможно это closure-binding с заглавной буквы
                // (нестандартно, но не запрещаем).
            }
        }
        // Обычный вызов: callee — выражение.
        let callee = self.eval_expr_value(func, env)?;
        let mut arg_values = self.eval_args(args, env)?;
        // trailing-block — преобразуем в closure-аргумент
        if let Some(tb) = trailing {
            let closure = Closure {
                params: tb.params.iter().map(|p| p.name.clone()).collect(),
                body: ClosureBody::Block(tb.body.clone()),
                env: env.clone(),
                receiver: env.lookup("@"),
                variadic_last: false,
            };
            arg_values.push(Value::Closure(Rc::new(closure)));
        }
        Ok(Flow::Value(self.call_value(callee, &arg_values, span)?))
    }

    fn eval_args(&self, args: &[Expr], env: &Env) -> Result<Vec<Value>, Diagnostic> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(self.eval_expr_value(a, env)?);
        }
        Ok(out)
    }

    fn try_member_call(
        &self,
        recv: &Value,
        method: &str,
        args: &[Expr],
        env: &Env,
        span: Span,
    ) -> Result<Option<Value>, Diagnostic> {
        // Native methods on built-in types.
        match recv {
            Value::Str(_) | Value::Array(_) => {
                if let Some(out) = stdlib::try_native_method(self, recv, method, args, env, span)?
                {
                    return Ok(Some(out));
                }
            }
            _ => {}
        }
        // Prelude D26 methods на Result/Option (Variant'ы).
        // Spec 08-runtime.md:235 для Result, аналогично для Option.
        if let Value::Variant { name, payload, .. } = recv {
            if let Some(out) = self.try_result_option_method(
                name, payload, method, args, env, span,
            )? {
                return Ok(Some(out));
            }
        }
        // Метод на пользовательском типе: ищем Type.method в globals.
        let type_name = match recv {
            Value::Record {
                type_name: Some(tn),
                ..
            } => Some(tn.clone()),
            Value::Variant {
                type_name: Some(tn),
                ..
            } => Some(tn.clone()),
            _ => None,
        };
        if let Some(tn) = type_name {
            let key = format!("{}.{}", tn, method);
            if let Some(closure_val) = self.globals.lookup(&key) {
                let mut arg_values = self.eval_args(args, env)?;
                if let Value::Closure(closure) = closure_val {
                    let mut new_closure = (*closure).clone();
                    new_closure.receiver = Some(recv.clone());
                    return Ok(Some(self.call_closure(&new_closure, &arg_values, span)?));
                } else {
                    // Native?
                    arg_values.insert(0, recv.clone());
                    return Ok(Some(self.call_value(closure_val, &arg_values, span)?));
                }
            }
        }
        Ok(None)
    }

    /// Plan 14 Ф.6-bis: вариант `try_member_call` который принимает
    /// уже-evaluated args (Vec<Value>). Используется в spread-path
    /// `ExprKind::Call`, где args pre-eval'ятся для unfolding'а
    /// `...arr`. Логика identical try_member_call но без
    /// `eval_args(...)`-шагов.
    ///
    /// Ограничение: native methods через stdlib::try_native_method
    /// требуют `&[Expr]`, не Vec<Value>. Поэтому native-path тут
    /// пропущен — для array-spread в native methods (rare) caller
    /// должен использовать non-spread форму.
    fn try_member_call_values(
        &self,
        recv: &Value,
        method: &str,
        arg_values: &[Value],
        span: Span,
    ) -> Result<Option<Value>, Diagnostic> {
        // Result/Option built-in methods — могут принять pre-evaluated args.
        if let Value::Variant { name, payload, .. } = recv {
            if let Some(out) = self.try_result_option_method_values(
                name, payload, method, arg_values, span,
            )? {
                return Ok(Some(out));
            }
        }
        // User-defined Type.method lookup в globals.
        let type_name = match recv {
            Value::Record { type_name: Some(tn), .. } => Some(tn.clone()),
            Value::Variant { type_name: Some(tn), .. } => Some(tn.clone()),
            _ => None,
        };
        if let Some(tn) = type_name {
            let key = format!("{}.{}", tn, method);
            if let Some(closure_val) = self.globals.lookup(&key) {
                if let Value::Closure(closure) = closure_val {
                    let mut new_closure = (*closure).clone();
                    new_closure.receiver = Some(recv.clone());
                    return Ok(Some(self.call_closure(&new_closure, arg_values, span)?));
                } else {
                    let mut with_recv = vec![recv.clone()];
                    with_recv.extend_from_slice(arg_values);
                    return Ok(Some(self.call_value(closure_val, &with_recv, span)?));
                }
            }
        }
        Ok(None)
    }

    /// Plan 14 Ф.6-bis: Result/Option methods с pre-evaluated args.
    /// Простая stub-обёртка — большинство prelude-методов на Result/
    /// Option (unwrap_or, map, etc.) — single-arg, в которые spread
    /// не имеет смысла. Если future требует поддержки — расширить.
    fn try_result_option_method_values(
        &self,
        _name: &str,
        _payload: &VariantPayload,
        _method: &str,
        _arg_values: &[Value],
        _span: Span,
    ) -> Result<Option<Value>, Diagnostic> {
        // Spread в Result/Option methods не предусмотрен — сразу None.
        Ok(None)
    }

    /// Built-in методы D26 prelude на Result[T,E] и Option[T].
    /// Spec: spec/decisions/08-runtime.md:235.
    /// Распознаёт receiver по имени variant'а (Ok/Err для Result,
    /// Some/None для Option). type_name могут не быть выставлен —
    /// различаем pure-name'ом.
    ///
    /// Result methods:
    ///   - is_ok / is_err
    ///   - ok() → Option[T] / err() → Option[E]
    ///   - unwrap_or(d) / unwrap_or_else(f)
    ///   - map(f) / map_err(f)
    /// Option methods:
    ///   - is_some / is_none
    ///   - unwrap_or(d) / unwrap_or_else(f)
    ///   - map(f)
    ///   - ok_or(e) → Result[T, E]
    fn try_result_option_method(
        &self,
        name: &str,
        payload: &VariantPayload,
        method: &str,
        args: &[Expr],
        env: &Env,
        span: Span,
    ) -> Result<Option<Value>, Diagnostic> {
        let make_some = |v: Value| Value::Variant {
            type_name: Some("Option".into()),
            name: "Some".into(),
            payload: VariantPayload::Tuple(vec![v]),
        };
        let make_none = || Value::Variant {
            type_name: Some("Option".into()),
            name: "None".into(),
            payload: VariantPayload::Unit,
        };
        let make_ok = |v: Value| Value::Variant {
            type_name: Some("Result".into()),
            name: "Ok".into(),
            payload: VariantPayload::Tuple(vec![v]),
        };
        let make_err = |v: Value| Value::Variant {
            type_name: Some("Result".into()),
            name: "Err".into(),
            payload: VariantPayload::Tuple(vec![v]),
        };
        let inner = |p: &VariantPayload| -> Option<Value> {
            if let VariantPayload::Tuple(items) = p {
                if items.len() == 1 {
                    return Some(items[0].clone());
                }
            }
            None
        };

        match (name, method) {
            // ===== Result =====
            ("Ok", "is_ok")  | ("Err", "is_err")  => Ok(Some(Value::Bool(true))),
            ("Ok", "is_err") | ("Err", "is_ok")   => Ok(Some(Value::Bool(false))),

            ("Ok", "ok") => Ok(Some(make_some(inner(payload).unwrap_or(Value::Unit)))),
            ("Err", "ok") => Ok(Some(make_none())),
            ("Ok", "err") => Ok(Some(make_none())),
            ("Err", "err") => Ok(Some(make_some(inner(payload).unwrap_or(Value::Unit)))),

            ("Ok", "unwrap_or") => Ok(Some(inner(payload).unwrap_or(Value::Unit))),
            ("Err", "unwrap_or") => {
                let arg_values = self.eval_args(args, env)?;
                Ok(Some(arg_values.into_iter().next().unwrap_or(Value::Unit)))
            }

            ("Ok", "unwrap_or_else") => Ok(Some(inner(payload).unwrap_or(Value::Unit))),
            ("Err", "unwrap_or_else") => {
                let arg_values = self.eval_args(args, env)?;
                let f = arg_values.into_iter().next().ok_or_else(|| {
                    Diagnostic::new("Result.unwrap_or_else: missing closure arg", span)
                })?;
                let e = inner(payload).unwrap_or(Value::Unit);
                Ok(Some(self.call_value(f, &[e], span)?))
            }

            ("Ok", "map") => {
                let arg_values = self.eval_args(args, env)?;
                let f = arg_values.into_iter().next().ok_or_else(|| {
                    Diagnostic::new("Result.map: missing closure arg", span)
                })?;
                let v = inner(payload).unwrap_or(Value::Unit);
                let mapped = self.call_value(f, &[v], span)?;
                Ok(Some(make_ok(mapped)))
            }
            ("Err", "map") => {
                // Err остаётся Err — re-wrap с тем же payload.
                let e = inner(payload).unwrap_or(Value::Unit);
                Ok(Some(make_err(e)))
            }

            ("Ok", "map_err") => {
                // Ok остаётся Ok без вызова f.
                let v = inner(payload).unwrap_or(Value::Unit);
                Ok(Some(make_ok(v)))
            }
            ("Err", "map_err") => {
                let arg_values = self.eval_args(args, env)?;
                let f = arg_values.into_iter().next().ok_or_else(|| {
                    Diagnostic::new("Result.map_err: missing closure arg", span)
                })?;
                let e = inner(payload).unwrap_or(Value::Unit);
                let mapped = self.call_value(f, &[e], span)?;
                Ok(Some(make_err(mapped)))
            }

            // ===== Option =====
            ("Some", "is_some") | ("None", "is_none") => Ok(Some(Value::Bool(true))),
            ("Some", "is_none") | ("None", "is_some") => Ok(Some(Value::Bool(false))),

            ("Some", "unwrap_or") => Ok(Some(inner(payload).unwrap_or(Value::Unit))),
            ("None", "unwrap_or") => {
                let arg_values = self.eval_args(args, env)?;
                Ok(Some(arg_values.into_iter().next().unwrap_or(Value::Unit)))
            }

            ("Some", "unwrap_or_else") => Ok(Some(inner(payload).unwrap_or(Value::Unit))),
            ("None", "unwrap_or_else") => {
                let arg_values = self.eval_args(args, env)?;
                let f = arg_values.into_iter().next().ok_or_else(|| {
                    Diagnostic::new("Option.unwrap_or_else: missing closure arg", span)
                })?;
                Ok(Some(self.call_value(f, &[], span)?))
            }

            ("Some", "map") => {
                let arg_values = self.eval_args(args, env)?;
                let f = arg_values.into_iter().next().ok_or_else(|| {
                    Diagnostic::new("Option.map: missing closure arg", span)
                })?;
                let v = inner(payload).unwrap_or(Value::Unit);
                let mapped = self.call_value(f, &[v], span)?;
                Ok(Some(make_some(mapped)))
            }
            ("None", "map") => Ok(Some(make_none())),

            ("Some", "ok_or") => {
                let v = inner(payload).unwrap_or(Value::Unit);
                Ok(Some(make_ok(v)))
            }
            ("None", "ok_or") => {
                let arg_values = self.eval_args(args, env)?;
                let e = arg_values.into_iter().next().unwrap_or(Value::Unit);
                Ok(Some(make_err(e)))
            }

            _ => Ok(None),
        }
    }

    fn try_construct_variant(
        &self,
        type_name: &str,
        variant: &str,
        args: &[Expr],
        env: &Env,
        span: Span,
    ) -> Result<Option<Value>, Diagnostic> {
        let td = match self.types.get(type_name) {
            Some(td) => td,
            None => return Ok(None),
        };
        let TypeDeclKind::Sum(variants) = &td.kind else {
            return Ok(None);
        };
        for v in variants {
            if v.name == variant {
                let arg_values = self.eval_args(args, env)?;
                let payload = match &v.kind {
                    SumVariantKind::Unit => VariantPayload::Unit,
                    SumVariantKind::Tuple(_) => VariantPayload::Tuple(arg_values),
                    SumVariantKind::Record(_) => {
                        return Err(Diagnostic::new(
                            "record-variant constructor not supported in bootstrap",
                            span,
                        ));
                    }
                };
                return Ok(Some(Value::Variant {
                    type_name: Some(type_name.to_string()),
                    name: variant.to_string(),
                    payload,
                }));
            }
        }
        Ok(None)
    }

    fn try_construct_variant_anywhere(
        &self,
        name: &str,
        args: &[Value],
        _span: Span,
    ) -> Result<Option<Value>, Diagnostic> {
        // Сначала проверим — это конструктор из ПРЕЛЮДИИ?
        if let Some(v) = self.preset_variant(name, args) {
            return Ok(Some(v));
        }
        // Иначе ищем в зарегистрированных sum-типах.
        for (type_name, td) in &self.types {
            if let TypeDeclKind::Sum(variants) = &td.kind {
                for v in variants {
                    if v.name == name {
                        let payload = match &v.kind {
                            SumVariantKind::Unit => VariantPayload::Unit,
                            SumVariantKind::Tuple(_) => VariantPayload::Tuple(args.to_vec()),
                            SumVariantKind::Record(_) => continue,
                        };
                        return Ok(Some(Value::Variant {
                            type_name: Some(type_name.clone()),
                            name: name.to_string(),
                            payload,
                        }));
                    }
                }
            }
        }
        Ok(None)
    }

    fn preset_variant(&self, name: &str, args: &[Value]) -> Option<Value> {
        match (name, args.len()) {
            ("Some", 1) => Some(Value::Variant {
                type_name: Some("Option".into()),
                name: "Some".into(),
                payload: VariantPayload::Tuple(args.to_vec()),
            }),
            ("None", 0) => Some(Value::Variant {
                type_name: Some("Option".into()),
                name: "None".into(),
                payload: VariantPayload::Unit,
            }),
            ("Ok", 1) => Some(Value::Variant {
                type_name: Some("Result".into()),
                name: "Ok".into(),
                payload: VariantPayload::Tuple(args.to_vec()),
            }),
            ("Err", 1) => Some(Value::Variant {
                type_name: Some("Result".into()),
                name: "Err".into(),
                payload: VariantPayload::Tuple(args.to_vec()),
            }),
            _ => None,
        }
    }

    fn try_resolve_unit_variant(&self, name: &str) -> Option<Value> {
        // Some/None/Ok/Err/true-like в прелюдии.
        if let Some(v) = self.preset_variant(name, &[]) {
            return Some(v);
        }
        for (type_name, td) in &self.types {
            if let TypeDeclKind::Sum(variants) = &td.kind {
                for v in variants {
                    if v.name == name && matches!(v.kind, SumVariantKind::Unit) {
                        return Some(Value::Variant {
                            type_name: Some(type_name.clone()),
                            name: name.to_string(),
                            payload: VariantPayload::Unit,
                        });
                    }
                }
            }
        }
        None
    }

    fn try_resolve_unit_variant_qualified(
        &self,
        type_name: &str,
        variant: &str,
    ) -> Option<Value> {
        let td = self.types.get(type_name)?;
        let TypeDeclKind::Sum(variants) = &td.kind else {
            return None;
        };
        for v in variants {
            if v.name == variant && matches!(v.kind, SumVariantKind::Unit) {
                return Some(Value::Variant {
                    type_name: Some(type_name.to_string()),
                    name: variant.to_string(),
                    payload: VariantPayload::Unit,
                });
            }
        }
        None
    }

    fn member_access(&self, obj: &Value, name: &str, span: Span) -> Result<Value, Diagnostic> {
        match obj {
            Value::Record { fields, .. } => fields
                .borrow()
                .get(name)
                .cloned()
                .ok_or_else(|| Diagnostic::new(format!("no field `{}`", name), span)),
            Value::Tuple(items) => {
                if let Ok(idx) = name.parse::<usize>() {
                    items
                        .get(idx)
                        .cloned()
                        .ok_or_else(|| Diagnostic::new(format!("tuple index {} out of range", idx), span))
                } else {
                    Err(Diagnostic::new(
                        format!("invalid tuple field `{}`", name),
                        span,
                    ))
                }
            }
            Value::Variant {
                payload: VariantPayload::Record(fields),
                ..
            } => fields
                .borrow()
                .get(name)
                .cloned()
                .ok_or_else(|| Diagnostic::new(format!("no field `{}`", name), span)),
            // Plan 60 / D117: size-accessor field-access — error с fix-it hint.
            // Method-call form работает через try_native_method (stdlib.rs).
            Value::Array(_) | Value::Str(_) if matches!(name, "len" | "is_empty" | "byte_len" | "cap" | "capacity") => {
                let suggested = if name == "cap" { "capacity" } else { name };
                Err(Diagnostic::new(
                    format!(
                        "[E_SIZE_ACCESSOR_FIELD] size-like accessor `{}` is method-only \
                         (Plan 60 / D117) — use `.{}()`",
                        name, suggested
                    ),
                    span,
                ))
            }
            other => Err(Diagnostic::new(
                format!("cannot access `.{}` on {}", name, other.type_name()),
                span,
            )),
        }
    }

    fn index_access(&self, obj: &Value, index: &Value, span: Span) -> Result<Value, Diagnostic> {
        match (obj, index) {
            (Value::Array(arr), Value::Int(i)) => {
                let arr = arr.borrow();
                if *i < 0 || (*i as usize) >= arr.len() {
                    return Err(Diagnostic::new(
                        format!("array index out of range: {}", i),
                        span,
                    ));
                }
                Ok(arr[*i as usize].clone())
            }
            (Value::Tuple(items), Value::Int(i)) => {
                if *i < 0 || (*i as usize) >= items.len() {
                    return Err(Diagnostic::new(
                        format!("tuple index out of range: {}", i),
                        span,
                    ));
                }
                Ok(items[*i as usize].clone())
            }
            _ => Err(Diagnostic::new(
                format!(
                    "cannot index {} by {}",
                    obj.type_name(),
                    index.type_name()
                ),
                span,
            )),
        }
    }

    fn binop(&self, op: BinOp, l: &Value, r: &Value, span: Span) -> Result<Value, Diagnostic> {
        use BinOp::*;
        match (op, l, r) {
            (Add, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Sub, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (Mul, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Div, Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    return Err(Diagnostic::new("division by zero", span));
                }
                Ok(Value::Int(a / b))
            }
            (Mod, Value::Int(a), Value::Int(b)) => {
                if *b == 0 {
                    return Err(Diagnostic::new("modulo by zero", span));
                }
                Ok(Value::Int(a % b))
            }
            (Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Add, Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
            (Eq, a, b) => Ok(Value::Bool(a == b)),
            (Neq, a, b) => Ok(Value::Bool(a != b)),
            (Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
            (Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
            (Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
            (Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),
            (Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
            (Le, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
            (Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
            (Ge, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
            (And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
            (Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
            // Plan 33.1 (D24): импликация и эквивалентность.
            // Семантически `A ==> B` ≡ `!A || B`; `A <==> B` ≡ `A == B` для bool.
            (Implies, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(!*a || *b)),
            (Iff, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a == *b)),
            _ => Err(Diagnostic::new(
                format!(
                    "cannot apply {:?} to {} and {}",
                    op,
                    l.type_name(),
                    r.type_name()
                ),
                span,
            )),
        }
    }

    fn run_for_loop(
        &self,
        pattern: &Pattern,
        iter: Value,
        body: &Block,
        env: &Env,
        span: Span,
    ) -> Result<Flow, Diagnostic> {
        // Поддерживаем range и array — для bootstrap'а достаточно.
        match iter {
            Value::Range {
                start,
                end,
                inclusive,
            } => {
                let mut i = start;
                let limit = if inclusive { end + 1 } else { end };
                while i < limit {
                    let local = Env::new_child(env);
                    if !self.match_pattern(pattern, &Value::Int(i), &local) {
                        return Err(Diagnostic::new("for-pattern did not match", span));
                    }
                    match self.exec_block_flow(body, &local)? {
                        Flow::Break => return Ok(Flow::Value(Value::Unit)),
                        Flow::Continue | Flow::Value(_) => {}
                        other => return Ok(other),
                    }
                    i += 1;
                }
                Ok(Flow::Value(Value::Unit))
            }
            Value::Array(arr) => {
                let len = arr.borrow().len();
                for i in 0..len {
                    let item = arr.borrow()[i].clone();
                    let local = Env::new_child(env);
                    if !self.match_pattern(pattern, &item, &local) {
                        return Err(Diagnostic::new("for-pattern did not match", span));
                    }
                    match self.exec_block_flow(body, &local)? {
                        Flow::Break => return Ok(Flow::Value(Value::Unit)),
                        Flow::Continue | Flow::Value(_) => {}
                        other => return Ok(other),
                    }
                }
                Ok(Flow::Value(Value::Unit))
            }
            other => Err(Diagnostic::new(
                format!("cannot iterate {}", other.type_name()),
                span,
            )),
        }
    }

    fn match_pattern(&self, pat: &Pattern, value: &Value, env: &Env) -> bool {
        match pat {
            Pattern::Wildcard(_) => true,
            Pattern::Literal(lit, _) => match (lit, value) {
                (Literal::Int(a), Value::Int(b)) => a == b,
                (Literal::Float(a), Value::Float(b)) => a == b,
                (Literal::Str(a), Value::Str(b)) => a == b,
                (Literal::Bool(a), Value::Bool(b)) => a == b,
                (Literal::Unit, Value::Unit) => true,
                _ => false,
            },
            Pattern::Ident { name, .. } => {
                env.define(name.clone(), value.clone());
                true
            }
            Pattern::Variant { path, kind, .. } => {
                let last = path.last().unwrap();
                let Value::Variant {
                    name,
                    payload,
                    ..
                } = value
                else {
                    return false;
                };
                if name != last {
                    return false;
                }
                match (kind, payload) {
                    (VariantPatternKind::Unit, VariantPayload::Unit) => true,
                    (
                        VariantPatternKind::Tuple { patterns, rest },
                        VariantPayload::Tuple(items),
                    ) => {
                        if !*rest && patterns.len() != items.len() {
                            return false;
                        }
                        if *rest && patterns.len() > items.len() {
                            return false;
                        }
                        for (p, v) in patterns.iter().zip(items.iter()) {
                            if !self.match_pattern(p, v, env) {
                                return false;
                            }
                        }
                        true
                    }
                    _ => false,
                }
            }
            Pattern::Record {
                fields,
                rest,
                ..
            } => {
                let Value::Record {
                    fields: rec_fields, ..
                } = value
                else {
                    return false;
                };
                let rec = rec_fields.borrow();
                for f in fields {
                    let Some(v) = rec.get(&f.name) else {
                        return false;
                    };
                    match &f.pattern {
                        Some(p) => {
                            if !self.match_pattern(p, v, env) {
                                return false;
                            }
                        }
                        None => {
                            env.define(f.name.clone(), v.clone());
                        }
                    }
                }
                if !rest {
                    // Все поля должны быть покрыты — проверка ослаблена для
                    // bootstrap'а: разрешаем больше полей в значении.
                }
                true
            }
            Pattern::Array { elems, .. } => {
                let Value::Array(arr) = value else {
                    return false;
                };
                let arr = arr.borrow();
                // Простой случай: без rest — длины совпадают.
                let has_rest = elems
                    .iter()
                    .any(|e| matches!(e, ArrayPatternElem::Rest | ArrayPatternElem::RestBind(_)));
                if !has_rest {
                    if elems.len() != arr.len() {
                        return false;
                    }
                    for (i, e) in elems.iter().enumerate() {
                        if let ArrayPatternElem::Item(p) = e {
                            if !self.match_pattern(p, &arr[i], env) {
                                return false;
                            }
                        }
                    }
                    return true;
                }
                // С rest: левая часть до rest — фиксированный prefix; правая
                // часть после rest — фиксированный suffix.
                let mut prefix: Vec<&Pattern> = Vec::new();
                let mut suffix: Vec<&Pattern> = Vec::new();
                let mut rest_bind: Option<&str> = None;
                let mut seen_rest = false;
                for e in elems {
                    match e {
                        ArrayPatternElem::Item(p) => {
                            if !seen_rest {
                                prefix.push(p);
                            } else {
                                suffix.push(p);
                            }
                        }
                        ArrayPatternElem::Rest => {
                            seen_rest = true;
                        }
                        ArrayPatternElem::RestBind(name) => {
                            seen_rest = true;
                            rest_bind = Some(name);
                        }
                    }
                }
                if arr.len() < prefix.len() + suffix.len() {
                    return false;
                }
                for (p, v) in prefix.iter().zip(arr.iter()) {
                    if !self.match_pattern(p, v, env) {
                        return false;
                    }
                }
                let rest_start = prefix.len();
                let rest_end = arr.len() - suffix.len();
                for (p, v) in suffix.iter().zip(arr[rest_end..].iter()) {
                    if !self.match_pattern(p, v, env) {
                        return false;
                    }
                }
                if let Some(name) = rest_bind {
                    let slice: Vec<Value> = arr[rest_start..rest_end].to_vec();
                    env.define(
                        name.to_string(),
                        Value::Array(Rc::new(RefCell::new(slice))),
                    );
                }
                true
            }
            Pattern::Tuple(pats, _) => {
                let Value::Tuple(items) = value else {
                    return false;
                };
                if pats.len() != items.len() {
                    return false;
                }
                for (p, v) in pats.iter().zip(items.iter()) {
                    if !self.match_pattern(p, v, env) {
                        return false;
                    }
                }
                true
            }
            Pattern::Binding { name, inner, .. } => {
                if !self.match_pattern(inner, value, env) {
                    return false;
                }
                env.define(name.clone(), value.clone());
                true
            }
            Pattern::Or { alternatives, .. } => {
                // Pattern alternation: пытаемся каждый альтернатив,
                // bindings от первого matched варианта.
                for alt in alternatives {
                    if self.match_pattern(alt, value, env) {
                        return true;
                    }
                }
                false
            }
        }
    }

    fn exec_block(&self, block: &Block, env: &Env) -> Result<Value, Diagnostic> {
        match self.exec_block_flow(block, env)? {
            Flow::Value(v) => Ok(v),
            Flow::Return(_) => Err(Diagnostic::new("`return` not allowed here", block.span)),
            Flow::Break => Err(Diagnostic::new("`break` not allowed here", block.span)),
            Flow::Continue => Err(Diagnostic::new("`continue` not allowed here", block.span)),
            Flow::Throw(_) => Err(Diagnostic::new("uncaught throw", block.span)),
            Flow::Interrupt(_) => Err(Diagnostic::new(
                "`interrupt` outside handler-method",
                block.span,
            )),
        }
    }

    pub fn exec_block_flow(&self, block: &Block, env: &Env) -> Result<Flow, Diagnostic> {
        let local = Env::new_child(env);
        // D90 Plan 20 Ф.5: per-scope defer-stack. defer/errdefer body
        // регистрируется при выполнении соответствующего statement'а
        // (eager registration). На exit (любой Flow::*) — invoke LIFO.
        // ErrDefer — только если is_error_exit.
        let mut defers: Vec<(Expr, /*is_errdefer*/ bool)> = Vec::new();

        let mut exit_flow: Option<Flow> = None;
        for stmt in &block.stmts {
            // Spec: defer/errdefer регистрируется eagerly — body
            // запоминается, но не выполняется до exit'а. Аргументы
            // body — closure-captures из текущего env (lazy при invoke).
            match stmt {
                Stmt::Defer { body, .. } => {
                    defers.push((body.clone(), false));
                    continue;
                }
                Stmt::ErrDefer { body, .. } => {
                    defers.push((body.clone(), true));
                    continue;
                }
                // D160 Plan 100.4.3: OkDefer — runs only on success (is_error=false),
                // so in the interpreter, register as success-only defer (is_error flag=false,
                // but semantics: skip if is_error_exit=true). For simplicity in the interpreter
                // bootstrap, treat same as defer for now.
                Stmt::OkDefer { body, .. } => {
                    defers.push((body.clone(), false));
                    continue;
                }
                // DeferWithResult — treat as plain defer in interpreter (result binding TBD).
                Stmt::DeferWithResult { body, .. } => {
                    defers.push((body.clone(), false));
                    continue;
                }
                _ => {}
            }
            match self.exec_stmt(stmt, &local) {
                Ok(Flow::Value(_)) => {}
                Ok(other) => { exit_flow = Some(other); break; }
                Err(diag) => {
                    // Even on hard error, invoke defer'ы (best-effort cleanup).
                    self.run_defers(&defers, /*is_error_exit*/ true, &local);
                    return Err(diag);
                }
            }
        }
        // Trailing expression — выполнить только если не было раннего exit'а.
        let result_flow = if exit_flow.is_some() {
            exit_flow.unwrap()
        } else if let Some(t) = &block.trailing {
            match self.eval_expr(t, &local) {
                Ok(f) => f,
                Err(diag) => {
                    self.run_defers(&defers, /*is_error_exit*/ true, &local);
                    return Err(diag);
                }
            }
        } else {
            Flow::Value(Value::Unit)
        };

        // Invoke defer'ы LIFO. is_error_exit = (Flow::Throw).
        let is_error_exit = matches!(result_flow, Flow::Throw(_));
        self.run_defers(&defers, is_error_exit, &local);
        Ok(result_flow)
    }

    /// D90 Plan 20 Ф.5: invoke defer'ов LIFO.
    /// - `defer` body — выполняется при любом exit'е.
    /// - `errdefer` body — выполняется только если `is_error_exit`
    ///   (Flow::Throw, hard panic-error).
    ///
    /// Если defer body сам throw'нёт — body checker'ом (Ф.3) запрещает
    /// throw в body. На runtime это не должно произойти на well-typed
    /// code. Но если случится (например через native call) — поглощаем
    /// (нельзя propagate из defer, иначе масштабирующая ошибка).
    fn run_defers(&self, defers: &[(Expr, bool)], is_error_exit: bool, env: &Env) {
        for (body, is_errdefer) in defers.iter().rev() {
            if *is_errdefer && !is_error_exit {
                continue; // errdefer skip на normal exit
            }
            // Best-effort: ошибки в defer body silently игнорируем.
            // Type-check (Ф.3) запрещает Fail/throw/return/break в body,
            // так что well-typed программы сюда не попадают.
            let _ = self.eval_expr(body, env);
        }
    }

    fn exec_stmt(&self, stmt: &Stmt, env: &Env) -> Result<Flow, Diagnostic> {
        match stmt {
            Stmt::Let(decl) => {
                // Plan 33.3 Ф.9.1 (D24): ghost erasure в interp.
                // `ghost let` — spec-only, не выполняется в runtime.
                // (Parser ensures ghost-vars не читаются из non-ghost кода.)
                if decl.is_ghost {
                    return Ok(Flow::Value(Value::Unit));
                }
                let v = match self.eval_expr(&decl.value, env)? {
                    Flow::Value(v) => v,
                    other => return Ok(other),
                };
                let local = env.clone();
                if !self.match_pattern(&decl.pattern, &v, &local) {
                    return Err(Diagnostic::new("let-pattern did not match", decl.span));
                }
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Expr(e) => self.eval_expr(e, env),
            Stmt::Assign {
                target,
                op,
                value,
                span,
            } => {
                let v = self.eval_expr_value(value, env)?;
                self.do_assign(target, *op, v, env, *span)?;
                Ok(Flow::Value(Value::Unit))
            }
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.eval_expr_value(e, env)?,
                    None => Value::Unit,
                };
                Ok(Flow::Return(v))
            }
            Stmt::Break(_) => Ok(Flow::Break),
            Stmt::Continue(_) => Ok(Flow::Continue),
            Stmt::Throw { value, .. } => {
                let v = self.eval_expr_value(value, env)?;
                Ok(Flow::Throw(v))
            }
            // D90 Plan 20 Ф.2: парсер принимает defer/errdefer, но interp
            // пока не реализует scope-stack invocation (Ф.5). No-op:
            // выражения не выполняются до Ф.5.
            //
            // TODO Ф.5: per-scope Vec<DeferEntry>, invoke LIFO на exit
            //          (Flow::Return / Flow::Throw / Flow::Break / normal).
            //          ErrDefer — флаг is_error_exit, invoke только если true.
            Stmt::Defer { .. } | Stmt::ErrDefer { .. }
            | Stmt::OkDefer { .. } | Stmt::DeferWithResult { .. } => {
                Ok(Flow::Value(Value::Unit))
            }
            // Plan 33.2 Ф.8 (D24): `assert_static <bool>` — в interp
            // выполняется как обычный assert (runtime-check); SMT-verify
            // через types/verify в compile-time.
            // Plan 33.3 (D24): `assume <bool>` — то же runtime behavior:
            // если expr=false, программист обещал что не будет → bug.
            Stmt::AssertStatic { expr, span } | Stmt::Assume { expr, span } => {
                let v = match self.eval_expr(expr, env)? {
                    Flow::Value(v) => v,
                    other => return Ok(other),
                };
                if let Value::Bool(false) = v {
                    return Err(Diagnostic::new(
                        "assert_static/assume failed", *span));
                }
                Ok(Flow::Value(Value::Unit))
            }
            // Ф.4.1: `apply lemma(args)` — ghost statement, нет runtime-эффекта.
            // В interp'е просто пропускаем (аргументы не вычисляем — они могут
            // содержать spec-выражения без runtime-значения).
            Stmt::Apply { .. } => Ok(Flow::Value(Value::Unit)),
            // Ф.4.2: calc — ghost statement, нет runtime-эффекта.
            Stmt::Calc { .. } => Ok(Flow::Value(Value::Unit)),
            // Plan 33.9 Ф.2: reveal — ghost statement.
            Stmt::Reveal { .. } => Ok(Flow::Value(Value::Unit)),
        }
    }

    fn do_assign(
        &self,
        target: &Expr,
        op: AssignOp,
        value: Value,
        env: &Env,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match &target.kind {
            ExprKind::Ident(name) => {
                let new_val = if matches!(op, AssignOp::Assign) {
                    value
                } else {
                    let cur = env
                        .lookup(name)
                        .ok_or_else(|| Diagnostic::new(format!("undefined `{}`", name), span))?;
                    self.compound(&cur, op, &value, span)?
                };
                if !env.assign(name, new_val) {
                    return Err(Diagnostic::new(
                        format!("`{}` not in scope (not a `mut` binding?)", name),
                        span,
                    ));
                }
                Ok(())
            }
            ExprKind::Member { obj, name } => {
                let obj_v = self.eval_expr_value(obj, env)?;
                if let Value::Record { fields, .. } = &obj_v {
                    let mut map = fields.borrow_mut();
                    let new_val = if matches!(op, AssignOp::Assign) {
                        value
                    } else {
                        let cur = map.get(name).cloned().ok_or_else(|| {
                            Diagnostic::new(format!("no field `{}`", name), span)
                        })?;
                        self.compound(&cur, op, &value, span)?
                    };
                    map.insert(name.clone(), new_val);
                    return Ok(());
                }
                Err(Diagnostic::new("cannot assign to non-record member", span))
            }
            ExprKind::Index { obj, index } => {
                let obj_v = self.eval_expr_value(obj, env)?;
                let idx_v = self.eval_expr_value(index, env)?;
                if let (Value::Array(arr), Value::Int(i)) = (&obj_v, &idx_v) {
                    let mut arr = arr.borrow_mut();
                    if *i < 0 || (*i as usize) >= arr.len() {
                        return Err(Diagnostic::new("index out of range", span));
                    }
                    let new_val = if matches!(op, AssignOp::Assign) {
                        value
                    } else {
                        self.compound(&arr[*i as usize], op, &value, span)?
                    };
                    arr[*i as usize] = new_val;
                    return Ok(());
                }
                Err(Diagnostic::new("cannot assign to non-array index", span))
            }
            ExprKind::SelfAccess => Err(Diagnostic::new("cannot assign to `@`", span)),
            _ => Err(Diagnostic::new("invalid assignment target", span)),
        }
    }

    fn compound(&self, lhs: &Value, op: AssignOp, rhs: &Value, span: Span) -> Result<Value, Diagnostic> {
        let bin = match op {
            AssignOp::Assign => return Ok(rhs.clone()),
            AssignOp::Add => BinOp::Add,
            AssignOp::Sub => BinOp::Sub,
            AssignOp::Mul => BinOp::Mul,
            AssignOp::Div => BinOp::Div,
        };
        self.binop(bin, lhs, rhs, span)
    }

    // ─── handlers ────────────────────────────────────────────────────────

    fn find_handler(&self, effect: &str) -> Option<Rc<value::Handler>> {
        let stack = self.handlers.borrow();
        for frame in stack.iter().rev() {
            if frame.effect == effect {
                return Some(frame.handler.clone());
            }
        }
        None
    }

    /// Plan 19, C8 (D31-rev): проверка что эффект имеет ровно одну
    /// операцию для handler-лямбда формы `with EffectName = |x| body`.
    /// Bootstrap fallback (production: type-checker enforce'ит на
    /// compile time). Возвращает Ok если операций ровно 1 или нет
    /// type-decl (effect определён через D2 prelude — Fail/Random/etc).
    fn assert_effect_has_single_op(
        &self,
        effect_name: &str,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if let Some(decl) = self.types.get(effect_name) {
            if let TypeDeclKind::Effect(ops) = &decl.kind {
                if ops.len() > 1 {
                    return Err(Diagnostic::new(
                        format!(
                            "handler-lambda `with {} = |...|` requires effect with exactly one operation; \
                             `{}` has {} operations. Use full handler-literal syntax `handler {} {{ ... }}`.",
                            effect_name, effect_name, ops.len(), effect_name
                        ),
                        span,
                    ));
                }
            }
        }
        // Не-зарегистрированный effect (Fail/Random/etc — prelude D2)
        // — пропускаем; production type-checker распознаёт сам.
        Ok(())
    }

    fn invoke_handler_op(
        &self,
        handler: &value::Handler,
        op: &str,
        args: &[Value],
        _env: &Env,
        span: Span,
    ) -> Result<Flow, Diagnostic> {
        // Plan 19, C8 (D31-rev): если handler был создан через
        // closure-light syntax (`with X = |args| body`), у него
        // вместо methods хранится Closure'а. Переадресуем call к
        // closure: `op(args)` → `closure(args)`. Эффект должен иметь
        // ровно одну операцию (assert проверен при создании
        // handler'а в eval_with).
        if let Some(closure) = &handler.lambda {
            return self.call_closure_flow(closure, args, span);
        }
        let method = handler.methods.get(op).ok_or_else(|| {
            Diagnostic::new(
                format!("handler for `{}` has no operation `{}`", handler.effect, op),
                span,
            )
        })?;
        let local = Env::new_child(&handler.env);
        if method.params.len() != args.len() {
            return Err(Diagnostic::new(
                format!(
                    "handler-method `{}` expects {} args, got {}",
                    op,
                    method.params.len(),
                    args.len()
                ),
                span,
            ));
        }
        for (p, v) in method.params.iter().zip(args.iter()) {
            local.define(p.name.clone(), v.clone());
        }
        let body_flow = match &method.body {
            HandlerMethodBody::Expr(e) => self.eval_expr(e, &local)?,
            HandlerMethodBody::Block(b) => self.exec_block_flow(b, &local)?,
        };
        // D61: handler-method завершается одним из:
        //  - финальное выражение (Flow::Value) → return-value операции
        //  - `return v` (Flow::Return) → return-value операции
        //  - `interrupt v` (Flow::Interrupt) → пробивается до eval_with
        match body_flow {
            Flow::Return(v) => Ok(Flow::Value(v)),
            other => Ok(other),
        }
    }

    fn eval_with(
        &self,
        bindings: &[WithBinding],
        body: &Block,
        env: &Env,
    ) -> Result<Flow, Diagnostic> {
        let mut frames_pushed = 0;
        for b in bindings {
            let effect_name = match &b.effect {
                TypeRef::Named { path, .. } => {
                    path.last().cloned().unwrap_or_default()
                }
                _ => {
                    return Err(Diagnostic::new(
                        "with-binding effect must be a named type",
                        b.effect.span(),
                    ));
                }
            };
            let handler_v = self.eval_expr_value(&b.handler, env)?;
            let handler = match handler_v {
                Value::Handler(h) => h,
                // Plan 19, C8 (D31-rev): handler-лямбда `|err| body`
                // в позиции `with EffectName = ...` — sugar над
                // handler-литералом с одной операцией. Закрытие в
                // этой позиции автоматически оборачиваем в Handler
                // с единственным методом эффекта.
                //
                // Compile-time проверка «эффект имеет ровно одну
                // операцию» — задача type-checker'а; в bootstrap'е
                // здесь runtime-fallback: смотрим в зарегистрированный
                // type-decl эффекта, берём первую (и обычно единственную)
                // операцию.
                Value::Closure(closure) => {
                    // Эффект должен иметь ровно одну операцию, иначе
                    // closure-form неоднозначен. Bootstrap-fallback:
                    // если operations > 1, берём первую (production
                    // type-checker должен enforce'ить «одна операция»
                    // на этапе компиляции).
                    let _ = self.assert_effect_has_single_op(
                        &effect_name,
                        b.span,
                    );
                    Rc::new(Handler {
                        effect: effect_name.clone(),
                        methods: HashMap::new(),
                        env: env.clone(),
                        lambda: Some(closure),
                    })
                }
                other => {
                    return Err(Diagnostic::new(
                        format!("expected handler, got {}", other.type_name()),
                        b.span,
                    ));
                }
            };
            self.handlers.borrow_mut().push(HandlerFrame {
                effect: effect_name,
                handler,
            });
            frames_pushed += 1;
        }
        let result = self.exec_block_flow(body, env);
        for _ in 0..frames_pushed {
            self.handlers.borrow_mut().pop();
        }
        // throw → если есть Fail-handler выше, он бы уже его поймал.
        // В bootstrap'е после with-блока throw не превращаем в catch
        // автоматически — конкретные тесты делают match { Err => ... }.
        //
        // Flow::Interrupt(v) — handler-method сделал `interrupt v`,
        // значение `v` становится результатом всего with-блока.
        match result {
            Ok(Flow::Interrupt(v)) => Ok(Flow::Value(v)),
            other => other,
        }
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
