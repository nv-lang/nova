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

mod stdlib;

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
use env::Env;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use value::*;

/// Исполнительный контекст: топ-левел декларации модуля + текущая среда +
/// handler-стек.
pub struct Interpreter {
    /// Top-level декларации (зарегистрированные при загрузке модуля).
    pub globals: Env,
    /// Регистр типов — для resolve'а sum-вариантов и protocol'ов.
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
    /// `throw err` — поднимается до Throws-handler'а.
    Throw(Value),
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
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
                    let closure = Closure {
                        params: fd.params.iter().map(|p| p.name.clone()).collect(),
                        body: match &fd.body {
                            FnBody::Expr(e) => ClosureBody::Expr(e.clone()),
                            FnBody::Block(b) => ClosureBody::Block(b.clone()),
                        },
                        env: self.globals.clone(),
                        receiver: None,
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
            Flow::Break | Flow::Continue => Err(Diagnostic::new(
                "break/continue outside loop",
                span,
            )),
        }
    }

    /// Вызов closure с возвратом Flow — для случаев, когда throw должен
    /// проброситься выше по call stack'у к обработчику Throws.
    fn call_closure_flow(
        &self,
        closure: &Closure,
        args: &[Value],
        span: Span,
    ) -> Result<Flow, Diagnostic> {
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
        let env = Env::new_child(&closure.env);
        if let Some(recv) = &closure.receiver {
            env.define("@", recv.clone());
        }
        for (name, value) in closure.params.iter().zip(args.iter()) {
            env.define(name.clone(), value.clone());
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
            ExprKind::FloatLit(x) => Ok(Flow::Value(Value::Float(*x))),
            ExprKind::StrLit(s) => Ok(Flow::Value(Value::Str(s.clone()))),
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
            ExprKind::Index { obj, index } => {
                let obj_v = self.eval_expr_value(obj, env)?;
                let idx_v = self.eval_expr_value(index, env)?;
                self.index_access(&obj_v, &idx_v, expr.span).map(Flow::Value)
            }
            ExprKind::Call {
                func,
                args,
                trailing_block,
            } => self.eval_call(func, args, trailing_block.as_ref(), env, expr.span),
            ExprKind::Try(inner) => {
                let result = self.eval_expr(inner, env)?;
                match result {
                    Flow::Throw(err) => Ok(Flow::Throw(err)),
                    Flow::Value(v) => {
                        // Если Result-like Variant: Ok(x) → x, Err(e) → throw e
                        if let Value::Variant { name, payload, .. } = &v {
                            match (name.as_str(), payload) {
                                ("Ok", VariantPayload::Tuple(items)) if items.len() == 1 => {
                                    return Ok(Flow::Value(items[0].clone()));
                                }
                                ("Err", VariantPayload::Tuple(items)) if items.len() == 1 => {
                                    return Ok(Flow::Throw(items[0].clone()));
                                }
                                ("Some", VariantPayload::Tuple(items)) if items.len() == 1 => {
                                    return Ok(Flow::Value(items[0].clone()));
                                }
                                ("None", VariantPayload::Unit) => {
                                    return Ok(Flow::Throw(Value::Unit));
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
            } => {
                let iter_v = self.eval_expr_value(iter, env)?;
                self.run_for_loop(pattern, iter_v, body, env, expr.span)
            }
            ExprKind::While { cond, body } => loop {
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
            ExprKind::Loop { body } => loop {
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
                    body: ClosureBody::Expr((**body).clone()),
                    env: env.clone(),
                    receiver: env.lookup("@"),
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
            ExprKind::RecordLit { type_name, fields } => {
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
            ExprKind::HandlerLit {
                effect_name,
                methods,
            } => {
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
                });
                Ok(Flow::Value(Value::Handler(handler)))
            }
            ExprKind::Resume(args) => {
                // resume — спец-форма: возвращает Value, исходящий обратно
                // в место вызова операции. В bootstrap'е one-shot resumption
                // реализован через простую модель: операция возвращает
                // значение, переданное в resume. Если resume не вызван —
                // handler возвращает значение за весь with-блок (его
                // return-value).
                let v = match args.len() {
                    0 => Value::Unit,
                    1 => self.eval_expr_value(&args[0], env)?,
                    _ => {
                        return Err(Diagnostic::new(
                            "resume takes 0 or 1 argument",
                            expr.span,
                        ));
                    }
                };
                // Возвращаем значение через спец-сигнал: обёртываем в
                // Variant("__resume", payload), interp ловит наверху.
                Ok(Flow::Value(Value::Variant {
                    type_name: Some("__resume".into()),
                    name: "__resume".into(),
                    payload: VariantPayload::Tuple(vec![v]),
                }))
            }
            ExprKind::Range { start, end, inclusive } => {
                let s = self.eval_expr_value(start, env)?;
                let e = self.eval_expr_value(end, env)?;
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
            ExprKind::TaggedTemplate { tag: _, parts, .. } => {
                // В bootstrap'е tagged template = просто строка (parts
                // конкатенируются). Tag-функция игнорируется. Достаточно
                // для написания компилятора, где `sql\`...\`` не используется.
                let s = parts.join("");
                Ok(Flow::Value(Value::Str(s)))
            }
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
            };
            arg_values.push(Value::Closure(Rc::new(closure)));
        }
        // Variant-constructor через одиночный path/ident?
        if let ExprKind::Ident(name) = &func.kind {
            if name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                // Look for variant in any registered sum type.
                if let Some(v) = self.try_construct_variant_anywhere(name, &arg_values, span)? {
                    return Ok(Flow::Value(v));
                }
            }
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
            Value::Array(arr) if name == "len" => Ok(Value::Int(arr.borrow().len() as i64)),
            Value::Str(s) if name == "len" => Ok(Value::Int(s.chars().count() as i64)),
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
        }
    }

    fn exec_block(&self, block: &Block, env: &Env) -> Result<Value, Diagnostic> {
        match self.exec_block_flow(block, env)? {
            Flow::Value(v) => Ok(v),
            Flow::Return(_) => Err(Diagnostic::new("`return` not allowed here", block.span)),
            Flow::Break => Err(Diagnostic::new("`break` not allowed here", block.span)),
            Flow::Continue => Err(Diagnostic::new("`continue` not allowed here", block.span)),
            Flow::Throw(_) => Err(Diagnostic::new("uncaught throw", block.span)),
        }
    }

    pub fn exec_block_flow(&self, block: &Block, env: &Env) -> Result<Flow, Diagnostic> {
        let local = Env::new_child(env);
        for stmt in &block.stmts {
            match self.exec_stmt(stmt, &local)? {
                Flow::Value(_) => {}
                other => return Ok(other),
            }
        }
        if let Some(t) = &block.trailing {
            return self.eval_expr(t, &local);
        }
        Ok(Flow::Value(Value::Unit))
    }

    fn exec_stmt(&self, stmt: &Stmt, env: &Env) -> Result<Flow, Diagnostic> {
        match stmt {
            Stmt::Let(decl) => {
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

    fn invoke_handler_op(
        &self,
        handler: &value::Handler,
        op: &str,
        args: &[Value],
        _env: &Env,
        span: Span,
    ) -> Result<Flow, Diagnostic> {
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
        // Распаковываем resume-сигнал: если последнее значение — __resume,
        // возвращаем его payload[0] как результат операции.
        match body_flow {
            Flow::Value(Value::Variant {
                name,
                payload: VariantPayload::Tuple(items),
                ..
            }) if name == "__resume" => {
                let inner = items.into_iter().next().unwrap_or(Value::Unit);
                Ok(Flow::Value(inner))
            }
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
            let handler_v = self.eval_expr_value(&b.handler, env)?;
            let handler = match handler_v {
                Value::Handler(h) => h,
                other => {
                    return Err(Diagnostic::new(
                        format!("expected handler, got {}", other.type_name()),
                        b.span,
                    ));
                }
            };
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
        // throw → если есть Throws-handler выше, он бы уже его поймал.
        // В bootstrap'е после with-блока throw не превращаем в catch
        // автоматически — конкретные тесты делают match { Err => ... }.
        result
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
