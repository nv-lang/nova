//! Type checker и effect inference.
//!
//! Минимальная реализация: проверяем имена типов, выводим типы локальных
//! переменных, выводим эффекты для private функций (D28). Generic-параметры
//! проверяются как abstract names — мономорфизация делается при
//! интерпретации (treewalk не требует всего).

use crate::ast::*;
use crate::diag::{Diagnostic, Span};
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

    // D82: `external fn` whitelisted только в `std.runtime.*`. User-код
    // не должен использовать external — это keyword для документирования
    // stdlib runtime-функций, реализованных в nova_rt/*.h. Будущий
    // `extern("C")` для FFI к сторонним libs — отдельный keyword.
    let is_runtime_module = module.name.len() >= 2
        && module.name[0] == "std"
        && module.name[1] == "runtime";
    if !is_runtime_module {
        for item in &module.items {
            if let Item::Fn(fd) = item {
                if fd.is_external {
                    errors.push(Diagnostic::new(
                        format!(
                            "`external fn` is only allowed in `std.runtime.*` modules \
                             (this module is `{}`); for FFI to external C libraries \
                             a future `extern(\"C\")` keyword will be added (Q-ffi)",
                            module.name.join(".")
                        ),
                        fd.span,
                    ));
                }
            }
        }
    }

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
                // Plan 11 Ф.1-Ф.3: ad-hoc overload по типу аргумента.
                // Один method-name на одном receiver-type может иметь несколько
                // signatures, различающихся param types и/или arity. Codegen
                // (method_overloads registry) резолвит на call-site по
                // статическим типам args. Поэтому duplicate (key) разрешён
                // для методов с receiver'ом — отдельные signatures.
                //
                // Free functions (без receiver'а) — overload не разрешён
                // (нет established паттерна для resolution в bootstrap'е).
                let is_method = fd.receiver.is_some();
                if !names.insert(key.clone()) && !is_method && !fd.is_external {
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

    // Plan 15 (D72): generic bounds enforcement.
    //
    // Собираем protocol_specs (методы каждого protocol-типа) и
    // method_table (методы каждого concrete-типа). Затем ходим по
    // всем call-сайтам в bodies, для generic-вызовов с bounds
    // проверяем satisfaction concrete-аргументов.
    let bound_ctx = BoundCtx::build(module);
    bound_ctx.check_module(module, &mut errors);

    // Plan 16 (D63 forbid + D64 realtime): capability enforcement.
    //
    // Walk fn bodies + tests, отслеживая forbidden-effects стек +
    // realtime-флаг. На каждом Call-сайте — проверка intersect'а
    // callee.effects с forbidden-set; в realtime — Net/Fs/Db/Time
    // suspend-effects запрещены; в `realtime nogc` — alloc-fn'ы
    // запрещены. Установка handler'а для forbidden-эффекта внутри
    // forbid-блока — error.
    let cap_ctx = CapabilityCtx::build(module);
    cap_ctx.check_module(module, &mut errors);

    if errors.is_empty() {
        Ok(env)
    } else {
        Err(errors)
    }
}

/// Plan 15 (D72): registry для bound enforcement.
///
/// `protocol_specs`: для каждого `type Foo protocol { ... }` — список
/// required methods (TypeDeclKind::Effect; в Nova protocol/effect единая
/// форма по D62).
///
/// `fn_decls`: top-level fn-декларации (для resolve вызова по имени).
///
/// `method_table`: для каждого concrete-типа — методы (по имени), для
/// проверки "type T satisfies protocol P".
struct BoundCtx<'a> {
    /// Plan 15 D53 strict: только protocol-kind типов. Effect-kind
    /// сюда не попадает — effects не разрешены как D72 bounds.
    protocol_specs: HashMap<String, &'a [EffectMethod]>,
    /// Plan 15 D53 strict: effect-kind типы. Используется для
    /// дифференциированного error-сообщения, если их пытаются
    /// использовать как bound («`Db` is an effect, not a protocol»).
    effect_decls: HashMap<String, &'a TypeDecl>,
    fn_decls: HashMap<String, &'a FnDecl>,
    method_table: HashMap<String, HashMap<String, Vec<&'a FnDecl>>>,
}

impl<'a> BoundCtx<'a> {
    fn build(module: &'a Module) -> Self {
        let mut protocol_specs = HashMap::new();
        let mut fn_decls: HashMap<String, &FnDecl> = HashMap::new();
        let mut method_table: HashMap<String, HashMap<String, Vec<&FnDecl>>> = HashMap::new();

        let mut effect_decls: HashMap<String, &TypeDecl> = HashMap::new();
        for item in &module.items {
            match item {
                Item::Type(t) => {
                    // Plan 15 D53 strict: protocol-kind → eligible как
                    // bound (D72); effect-kind → отдельный registry для
                    // диагностики «used as bound but it's an effect».
                    match &t.kind {
                        TypeDeclKind::Protocol(methods) => {
                            protocol_specs.insert(t.name.clone(), methods.as_slice());
                        }
                        TypeDeclKind::Effect(_) => {
                            effect_decls.insert(t.name.clone(), t);
                        }
                        _ => {}
                    }
                }
                Item::Fn(f) => {
                    if let Some(recv) = &f.receiver {
                        method_table
                            .entry(recv.type_name.clone())
                            .or_default()
                            .entry(f.name.clone())
                            .or_default()
                            .push(f);
                    } else {
                        fn_decls.insert(f.name.clone(), f);
                    }
                }
                _ => {}
            }
        }

        BoundCtx { protocol_specs, effect_decls, fn_decls, method_table }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        for item in &module.items {
            match item {
                Item::Fn(f) => {
                    let mut scope: HashMap<String, TypeRef> = HashMap::new();
                    // Регистрируем параметры функции с их типами.
                    for p in &f.params {
                        scope.insert(p.name.clone(), p.ty.clone());
                    }
                    self.walk_fn_body(f, &mut scope, errors);
                }
                Item::Test(t) => {
                    // Plan 15: тесты тоже могут содержать generic-вызовы
                    // c bounds — обходим их body со свежим scope.
                    let mut scope: HashMap<String, TypeRef> = HashMap::new();
                    self.walk_block(&t.body, &mut scope, errors);
                }
                _ => {}
            }
        }
    }

    fn walk_fn_body(&self, f: &FnDecl, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        match &f.body {
            FnBody::Expr(e) => self.walk_expr(e, scope, errors),
            FnBody::Block(b) => self.walk_block(b, scope, errors),
            FnBody::External => {}
        }
    }

    fn walk_block(&self, b: &Block, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        // Сохраняем snapshot для bindings которые let'аются в этом блоке —
        // чтобы вернуть scope после блока (block-out shadowing semantics).
        let mut snapshot: Vec<(String, Option<TypeRef>)> = Vec::new();
        for s in &b.stmts {
            if let Stmt::Let(d) = s {
                if let Some(name) = pattern_simple_name(&d.pattern) {
                    snapshot.push((name.clone(), scope.get(&name).cloned()));
                }
            }
        }
        for s in &b.stmts {
            self.walk_stmt(s, scope, errors);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(t, scope, errors);
        }
        // Восстановим shadowed bindings (block-out).
        for (n, prev) in snapshot {
            match prev {
                Some(t) => { scope.insert(n, t); }
                None => { scope.remove(&n); }
            }
        }
    }

    fn walk_stmt(&self, s: &Stmt, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, scope, errors),
            Stmt::Let(d) => {
                self.walk_expr(&d.value, scope, errors);
                // Регистрируем simple-Ident pattern с inferred типом.
                if let Some(name) = pattern_simple_name(&d.pattern) {
                    let inferred = d.ty.clone()
                        .or_else(|| Self::infer_arg_ty(&d.value, scope));
                    if let Some(t) = inferred {
                        scope.insert(name, t);
                    }
                }
            }
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, scope, errors);
                self.walk_expr(value, scope, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, scope, errors); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, scope, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }

    fn walk_expr(&self, e: &Expr, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        // Проверяем сам call перед рекурсией в args (порядок не важен).
        self.check_call_bounds(e, scope, errors);
        match &e.kind {
            ExprKind::Call { func, args, trailing_block } => {
                self.walk_expr(func, scope, errors);
                for a in args {
                    self.walk_expr(a.expr(), scope, errors);
                }
                if let Some(tb) = trailing_block {
                    self.walk_block(&tb.body, scope, errors);
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, scope, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, scope, errors);
                self.walk_expr(right, scope, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, scope, errors),
            ExprKind::Try(inner) => self.walk_expr(inner, scope, errors),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, scope, errors);
                self.walk_expr(b, scope, errors);
            }
            ExprKind::As(e, _) => self.walk_expr(e, scope, errors),
            ExprKind::Is(e, _) => self.walk_expr(e, scope, errors),
            ExprKind::Member { obj, .. } => self.walk_expr(obj, scope, errors),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, scope, errors);
                self.walk_expr(index, scope, errors);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, scope, errors);
                self.walk_block(then, scope, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, scope, errors),
                        ElseBranch::If(e) => self.walk_expr(e, scope, errors),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee, scope, errors);
                self.walk_block(then, scope, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, scope, errors),
                        ElseBranch::If(e) => self.walk_expr(e, scope, errors),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, scope, errors);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(g, scope, errors); }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e, scope, errors),
                        MatchArmBody::Block(b) => self.walk_block(b, scope, errors),
                    }
                }
            }
            ExprKind::Block(b) => self.walk_block(b, scope, errors),
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => self.walk_expr(e, scope, errors),
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for e in elems { self.walk_expr(e, scope, errors); }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value { self.walk_expr(v, scope, errors); }
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.walk_expr(tag, scope, errors);
                for a in args { self.walk_expr(a, scope, errors); }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr(e) = p {
                        self.walk_expr(e, scope, errors);
                    }
                }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body, scope, errors),
            ExprKind::Spawn(body) => self.walk_expr(body, scope, errors),
            ExprKind::Supervised(body) | ExprKind::Detach(body) => self.walk_block(body, scope, errors),
            ExprKind::CancelScope { body, .. } => self.walk_block(body, scope, errors),
            ExprKind::Forbid { body, .. } => self.walk_block(body, scope, errors),
            ExprKind::Realtime { body, .. } => self.walk_block(body, scope, errors),
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter, scope, errors);
                self.walk_block(body, scope, errors);
            }
            ExprKind::For { iter, body, .. } => {
                self.walk_expr(iter, scope, errors);
                self.walk_block(body, scope, errors);
            }
            ExprKind::While { cond, body } => {
                self.walk_expr(cond, scope, errors);
                self.walk_block(body, scope, errors);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.walk_expr(scrutinee, scope, errors);
                self.walk_block(body, scope, errors);
            }
            ExprKind::Loop { body } => self.walk_block(body, scope, errors),
            ExprKind::Range { start, end, .. } => {
                self.walk_expr(start, scope, errors);
                self.walk_expr(end, scope, errors);
            }
            ExprKind::Throw(e) => self.walk_expr(e, scope, errors),
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt { self.walk_expr(e, scope, errors); }
            }
            ExprKind::With { body, .. } => self.walk_block(body, scope, errors),
            // Литералы / ident'ы / handler-литералы — без рекурсии в bound-проверке.
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
            | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
            | ExprKind::HandlerLit { .. } => {}
        }
    }

    /// Plan 15 Ф.3: проверить bound'ы на конкретном call-site.
    ///
    /// Если callee — top-level fn с generics+bounds, и есть turbofish
    /// type_args (или возможна простая inference из args) — проверить
    /// что concrete-T удовлетворяет bound'у.
    fn check_call_bounds(
        &self,
        e: &Expr,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let ExprKind::Call { func, args, .. } = &e.kind else { return; };
        // Распакуем turbofish, чтобы добраться до базового идентификатора.
        let (base, type_args): (&Expr, &[TypeRef]) = match &func.kind {
            ExprKind::TurboFish { base, type_args } => (base, type_args.as_slice()),
            _ => (func.as_ref(), &[][..]),
        };
        let fn_name = match &base.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return, // методы и т.п. — отдельная задача
        };
        let Some(callee) = self.fn_decls.get(&fn_name).copied() else { return; };
        // Bounds присутствуют?
        let has_bounds = callee.generics.iter().any(|g| g.bound.is_some());
        if !has_bounds { return; }
        // Сматчим concrete T. Стратегия:
        //   - turbofish — explicit type_args[i] для callee.generics[i].
        //   - иначе simple inference: для каждого param с TypeRef::Named{path:[T]}
        //     где T — generic-param, тип arg'а на той же позиции = concrete T.
        let mut bindings: HashMap<String, TypeRef> = HashMap::new();
        if !type_args.is_empty() {
            for (i, gp) in callee.generics.iter().enumerate() {
                if let Some(t) = type_args.get(i) {
                    bindings.insert(gp.name.clone(), t.clone());
                }
            }
        } else {
            // Simple inference из позиционных args.
            for (i, param) in callee.params.iter().enumerate() {
                let Some(call_arg) = args.get(i) else { continue; };
                let arg_expr = call_arg.expr();
                if let Some(t_name) = Self::param_generic_name(&param.ty, &callee.generics) {
                    if let Some(arg_ty) = Self::infer_arg_ty(arg_expr, scope) {
                        bindings.entry(t_name).or_insert(arg_ty);
                    }
                }
            }
        }
        // Для каждого bounded generic — проверить.
        for gp in &callee.generics {
            let Some(bound) = &gp.bound else { continue; };
            let Some(concrete) = bindings.get(&gp.name) else {
                // Inference не удалась — пропускаем (best-effort).
                // Strict-mode мог бы требовать explicit turbofish.
                continue;
            };
            self.check_satisfaction(
                concrete, bound, &gp.name, &fn_name, e.span, errors,
            );
        }
    }

    /// Если param's TypeRef — простой `Named{path: [T]}` где T в
    /// списке generics, вернуть имя T. Иначе None.
    fn param_generic_name(ty: &TypeRef, generics: &[GenericParam]) -> Option<String> {
        let TypeRef::Named { path, generics: g, .. } = ty else { return None; };
        if path.len() != 1 || !g.is_empty() { return None; }
        if generics.iter().any(|gp| gp.name == path[0]) {
            Some(path[0].clone())
        } else {
            None
        }
    }

    /// Минимальная inference типа argument'а — best-effort на основе
    /// синтаксической формы и текущего scope (let-bindings).
    fn infer_arg_ty(e: &Expr, scope: &HashMap<String, TypeRef>) -> Option<TypeRef> {
        match &e.kind {
            ExprKind::Ident(name) => scope.get(name).cloned(),
            ExprKind::RecordLit { type_name: Some(name), .. } => Some(TypeRef::Named {
                path: name.clone(),
                generics: Vec::new(),
                span: e.span,
            }),
            ExprKind::ArrayLit(elems) => {
                // []T — element type from first element.
                let inner = elems.iter().find_map(|el| match el {
                    ArrayElem::Item(it) | ArrayElem::Spread(it) => Self::infer_arg_ty(it, scope),
                });
                inner.map(|t| TypeRef::Array(Box::new(t), e.span))
            }
            ExprKind::IntLit(_) => Some(TypeRef::Named {
                path: vec!["int".to_string()], generics: vec![], span: e.span }),
            ExprKind::FloatLit(_) => Some(TypeRef::Named {
                path: vec!["f64".to_string()], generics: vec![], span: e.span }),
            ExprKind::BoolLit(_) => Some(TypeRef::Named {
                path: vec!["bool".to_string()], generics: vec![], span: e.span }),
            ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => Some(TypeRef::Named {
                path: vec!["str".to_string()], generics: vec![], span: e.span }),
            ExprKind::CharLit(_) => Some(TypeRef::Named {
                path: vec!["char".to_string()], generics: vec![], span: e.span }),
            _ => None,
        }
    }

    /// Plan 15 Ф.3: проверить, что concrete-тип удовлетворяет bound'у
    /// (protocol-типу). При несоответствии — R5.3 diagnostic.
    fn check_satisfaction(
        &self,
        concrete: &TypeRef,
        bound: &TypeRef,
        type_param_name: &str,
        fn_name: &str,
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        let bound_name = match bound {
            TypeRef::Named { path, .. } if path.len() == 1 => path[0].clone(),
            _ => return, // complex bounds (Hashable[K], etc.) — отдельная задача
        };
        // Plan 15 D53 strict: bound должен быть protocol-kind. Если
        // имя зарегистрировано как effect-kind — это spec violation
        // (D72: bounds require protocols). R5.3-style diagnostic.
        if let Some(eff_decl) = self.effect_decls.get(&bound_name) {
            let _ = eff_decl;
            errors.push(Diagnostic::new(
                format!(
                    "type `{}` is an effect, not a protocol — generic bounds \
                     require protocol-types (D72/D53). Hint: declare `{}` as \
                     `type {} protocol {{ ... }}` if structural-contract semantics \
                     is intended; effects are runtime-dispatched capabilities and \
                     can only appear in effect-rows `(...) {} -> ...`, not as \
                     `[T {}]` bounds.",
                    bound_name, bound_name, bound_name, bound_name, bound_name,
                ),
                span,
            ));
            return;
        }
        let concrete_name = match concrete {
            TypeRef::Named { path, .. } if path.len() == 1 => path[0].clone(),
            // Array/Tuple/Func — пока пропускаем (не обрабатываем составные T).
            _ => return,
        };
        // Built-in primitives автоматически удовлетворяют ничему — у нас
        // нет registry их методов в method_table. Skip (best-effort).
        if matches!(concrete_name.as_str(),
            "int" | "i8" | "i16" | "i32" | "i64"
            | "u8" | "u16" | "u32" | "u64"
            | "f32" | "f64" | "bool" | "char" | "byte"
            | "str" | "any") {
            return;
        }
        let Some(spec_methods) = self.protocol_specs.get(&bound_name) else {
            // Bound — не зарегистрирован ни как protocol, ни как effect.
            // Может быть type alias / record / unknown. Пока пропускаем —
            // formal check'а не делаем (best-effort permissive).
            return;
        };
        let empty: HashMap<String, Vec<&FnDecl>> = HashMap::new();
        let concrete_methods = self.method_table.get(&concrete_name).unwrap_or(&empty);
        let mut missing: Vec<String> = Vec::new();
        for required in *spec_methods {
            // Match по имени и arity. Полная sig-сверка с Self→T —
            // дальнейшая задача (Ф.5).
            let found = concrete_methods.get(&required.name).map(|fns| {
                fns.iter().any(|f| f.params.len() == required.params.len())
            }).unwrap_or(false);
            if !found {
                let sig = render_method_sig(&required.name, &required.params, &required.return_type);
                missing.push(sig);
            }
        }
        if !missing.is_empty() {
            // R5.3 структурированный AI-first diagnostic.
            let mut msg = format!(
                "type `{}` does not satisfy `{}` bound (in call to `{}[{} {}]`).\n\n  `{}` requires:\n",
                concrete_name, bound_name, fn_name, type_param_name, bound_name, bound_name);
            for required in *spec_methods {
                msg.push_str(&format!(
                    "    {}\n",
                    render_method_sig(&required.name, &required.params, &required.return_type)));
            }
            msg.push_str(&format!("\n  `{}` is missing: {}\n", concrete_name, missing.join(", ")));
            msg.push_str(&format!(
                "\n  fix: добавить недостающие методы для типа `{}`. \
                 См. spec/decisions/02-types.md#d72.",
                concrete_name));
            errors.push(Diagnostic::new(msg, span));
        }
    }
}

/// Plan 15: extract simple identifier-name из Pattern. Используется
/// для регистрации let-bindings в scope (только Pattern::Ident; complex
/// patterns — tuple/variant — пропускаются).
fn pattern_simple_name(p: &Pattern) -> Option<String> {
    match p {
        Pattern::Ident { name, .. } => Some(name.clone()),
        _ => None,
    }
}

// ============================================================================
// Plan 16 (D63 forbid + D64 realtime): capability enforcement.
// ============================================================================

/// Plan 16: набор "suspend"-эффектов которые нельзя использовать внутри
/// `realtime { ... }` блоков (D64). Эти эффекты по семантике могут
/// приостановить fiber'а в production-runtime'е.
fn realtime_suspend_effect(name: &str) -> bool {
    matches!(name, "Net" | "Fs" | "Db" | "Time" | "Blocking")
}

/// Plan 16: hardcoded whitelist callee-name'ов, которые **аллоцируют**
/// в managed heap (и потому запрещены в `realtime nogc { ... }`).
/// Идентификация по mangled C-name pattern + по высокоуровневым
/// `Type.method` (e.g. `[]int.new`, `StringBuilder.new`).
///
/// **Не покрывается** этим whitelist'ом:
/// - User-defined record-конструкторы `Foo.new()` если они alloc'ят
///   через nova_alloc — codegen всегда heap-боксит record-литералы,
///   так что фактически любой record-литерал «аллоцирующий». Но
///   detection требует bigger inference. Conservative — флагуем
///   только статические fabric-методы.
/// - `str.from(non-str)` если требует concat'а — пока считаем
///   все `str.from`-вызовы "alloc'ирующими".
fn nogc_blacklisted_call(callee_path: &[String]) -> bool {
    if callee_path.len() != 2 { return false; }
    let ty = callee_path[0].as_str();
    let m = callee_path[1].as_str();
    // Array constructors: `[]T.new` / `[]T.with_capacity`.
    if ty.starts_with("[]") && matches!(m, "new" | "with_capacity") { return true; }
    // Builder/buffer constructors.
    if matches!(ty, "StringBuilder" | "WriteBuffer" | "ReadBuffer")
        && matches!(m, "new" | "with_capacity" | "from") { return true; }
    // Channel constructor.
    if ty == "Channel" && matches!(m, "new" | "with_capacity") { return true; }
    // Map/Set/Vec/Deque etc.
    if matches!(ty, "HashMap" | "Set" | "Vec" | "Deque" | "LinkedList" | "Lru" | "BloomFilter")
        && matches!(m, "new" | "with_capacity") { return true; }
    // str.from: format/conversion может alloc'ать.
    if ty == "str" && m == "from" { return true; }
    false
}

/// Plan 16: registry для capability enforcement.
struct CapabilityCtx<'a> {
    /// Top-level free fn-декларации (для resolve вызова по имени).
    fn_decls: HashMap<String, &'a FnDecl>,
    /// Plan 15 reuse: type → method_name → fn-decls.
    method_table: HashMap<String, HashMap<String, Vec<&'a FnDecl>>>,
    /// Effect-type name registry (для distinguish'а effect-call vs ordinary).
    effect_decls: HashMap<String, &'a TypeDecl>,
}

/// Plan 16: capability state передаётся через walk как mutable.
/// Push/pop при входе/выходе из forbid/realtime блоков.
#[derive(Default, Clone)]
struct CapState {
    /// Stack forbidden-effects-set'ов от вложенных `forbid` блоков.
    /// Effect разрешён если он не в **union'е** этих set'ов.
    /// (Forbid внутри forbid — union, см. D63.)
    forbidden_stack: Vec<HashSet<String>>,
    /// True если мы внутри `realtime { ... }` (или `realtime nogc`).
    /// Suspend-effects (Net/Fs/Db/Time/Blocking) запрещены.
    realtime_active: bool,
    /// True если мы внутри `realtime nogc { ... }`. Дополнительно к
    /// realtime_active запрещены alloc-вызовы.
    realtime_nogc: bool,
    /// Stack handlers, установленных через `with X = ... { ... }`.
    /// Используется для D63 forbid-handler-ban: `with X` внутри
    /// `forbid X` — compile error.
    with_handler_stack: Vec<String>,
}

impl CapState {
    /// Union forbidden-set'ов всех уровней стека.
    fn union_forbidden(&self) -> HashSet<String> {
        let mut out = HashSet::new();
        for s in &self.forbidden_stack { out.extend(s.iter().cloned()); }
        out
    }
}

impl<'a> CapabilityCtx<'a> {
    fn build(module: &'a Module) -> Self {
        let mut fn_decls: HashMap<String, &FnDecl> = HashMap::new();
        let mut method_table: HashMap<String, HashMap<String, Vec<&FnDecl>>> = HashMap::new();
        let mut effect_decls: HashMap<String, &TypeDecl> = HashMap::new();
        for item in &module.items {
            match item {
                Item::Type(t) => {
                    if matches!(t.kind, TypeDeclKind::Effect(_)) {
                        effect_decls.insert(t.name.clone(), t);
                    }
                }
                Item::Fn(f) => {
                    if let Some(recv) = &f.receiver {
                        method_table
                            .entry(recv.type_name.clone())
                            .or_default()
                            .entry(f.name.clone())
                            .or_default()
                            .push(f);
                    } else {
                        fn_decls.insert(f.name.clone(), f);
                    }
                }
                _ => {}
            }
        }
        CapabilityCtx { fn_decls, method_table, effect_decls }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        for item in &module.items {
            match item {
                Item::Fn(f) => {
                    let mut state = CapState::default();
                    // Plan 16 Ф.5: @realtime атрибут оборачивает body
                    // в realtime[+nogc] контекст.
                    match f.realtime_attr {
                        RealtimeAttr::None => {}
                        RealtimeAttr::Realtime => state.realtime_active = true,
                        RealtimeAttr::RealtimeNogc => {
                            state.realtime_active = true;
                            state.realtime_nogc = true;
                        }
                    }
                    self.walk_fn_body(f, &mut state, errors);
                }
                Item::Test(t) => {
                    let mut state = CapState::default();
                    self.walk_block(&t.body, &mut state, errors);
                }
                _ => {}
            }
        }
    }

    fn walk_fn_body(&self, f: &FnDecl, state: &mut CapState, errors: &mut Vec<Diagnostic>) {
        match &f.body {
            FnBody::Expr(e) => self.walk_expr(e, state, errors),
            FnBody::Block(b) => self.walk_block(b, state, errors),
            FnBody::External => {}
        }
    }

    fn walk_block(&self, b: &Block, state: &mut CapState, errors: &mut Vec<Diagnostic>) {
        for s in &b.stmts {
            self.walk_stmt(s, state, errors);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(t, state, errors);
        }
    }

    fn walk_stmt(&self, s: &Stmt, state: &mut CapState, errors: &mut Vec<Diagnostic>) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, state, errors),
            Stmt::Let(d) => self.walk_expr(&d.value, state, errors),
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, state, errors);
                self.walk_expr(value, state, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, state, errors); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, state, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }

    fn walk_expr(&self, e: &Expr, state: &mut CapState, errors: &mut Vec<Diagnostic>) {
        // Сначала проверяем сам узел (call-bound checks), потом
        // погружаемся внутрь с обновлённым state'ом для блочных
        // конструкций (forbid/realtime/with).
        self.check_capabilities_at(e, state, errors);
        match &e.kind {
            ExprKind::Forbid { effects, body } => {
                // Push forbidden-set, walk, pop.
                let names: HashSet<String> = effects.iter()
                    .filter_map(|t| match t {
                        TypeRef::Named { path, .. } if path.len() == 1 => Some(path[0].clone()),
                        _ => None,
                    })
                    .collect();
                state.forbidden_stack.push(names);
                self.walk_block(body, state, errors);
                state.forbidden_stack.pop();
            }
            ExprKind::Realtime { nogc, body } => {
                let prev_active = state.realtime_active;
                let prev_nogc = state.realtime_nogc;
                state.realtime_active = true;
                state.realtime_nogc = state.realtime_nogc || *nogc;
                self.walk_block(body, state, errors);
                state.realtime_active = prev_active;
                state.realtime_nogc = prev_nogc;
            }
            ExprKind::With { bindings, body } => {
                // Plan 16 D63: установка handler'а для forbidden-эффекта
                // внутри forbid-блока — compile error.
                //
                // WithBinding.effect: TypeRef. Для названия эффекта
                // берём последний segment Named-path (e.g. `std.io.Net`
                // → "Net"). Non-Named TypeRefs (Array/Tuple/Func/etc.) —
                // невалидны для эффект-handler'ов, пропускаем.
                let pushed: Vec<String> = bindings.iter()
                    .filter_map(|b| match &b.effect {
                        TypeRef::Named { path, .. } if !path.is_empty() => path.last().cloned(),
                        _ => None,
                    })
                    .collect();
                let forbidden = state.union_forbidden();
                for n in &pushed {
                    if forbidden.contains(n) {
                        errors.push(Diagnostic::new(
                            format!(
                                "cannot install handler for `{}` inside `forbid {}` block (D63): \
                                 forbid is impenetrable — code in body cannot escape sandbox \
                                 via `with X = …`.",
                                n, n
                            ),
                            e.span,
                        ));
                    }
                    state.with_handler_stack.push(n.clone());
                }
                self.walk_block(body, state, errors);
                for _ in &pushed { state.with_handler_stack.pop(); }
            }
            ExprKind::Call { func, args, trailing_block } => {
                self.walk_expr(func, state, errors);
                for a in args { self.walk_expr(a.expr(), state, errors); }
                if let Some(tb) = trailing_block {
                    self.walk_block(&tb.body, state, errors);
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, state, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, state, errors);
                self.walk_expr(right, state, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, state, errors),
            ExprKind::Try(inner) => self.walk_expr(inner, state, errors),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, state, errors);
                self.walk_expr(b, state, errors);
            }
            ExprKind::As(e, _) => self.walk_expr(e, state, errors),
            ExprKind::Is(e, _) => self.walk_expr(e, state, errors),
            ExprKind::Member { obj, .. } => self.walk_expr(obj, state, errors),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, state, errors);
                self.walk_expr(index, state, errors);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, state, errors);
                self.walk_block(then, state, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, state, errors),
                        ElseBranch::If(e) => self.walk_expr(e, state, errors),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee, state, errors);
                self.walk_block(then, state, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, state, errors),
                        ElseBranch::If(e) => self.walk_expr(e, state, errors),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, state, errors);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(g, state, errors); }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e, state, errors),
                        MatchArmBody::Block(b) => self.walk_block(b, state, errors),
                    }
                }
            }
            ExprKind::Block(b) => self.walk_block(b, state, errors),
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => self.walk_expr(e, state, errors),
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for e in elems { self.walk_expr(e, state, errors); }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value { self.walk_expr(v, state, errors); }
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.walk_expr(tag, state, errors);
                for a in args { self.walk_expr(a, state, errors); }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr(e) = p {
                        self.walk_expr(e, state, errors);
                    }
                }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body, state, errors),
            ExprKind::Spawn(body) => self.walk_expr(body, state, errors),
            ExprKind::Supervised(body) | ExprKind::Detach(body) => self.walk_block(body, state, errors),
            ExprKind::CancelScope { body, .. } => self.walk_block(body, state, errors),
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter, state, errors);
                self.walk_block(body, state, errors);
            }
            ExprKind::For { iter, body, .. } => {
                self.walk_expr(iter, state, errors);
                self.walk_block(body, state, errors);
            }
            ExprKind::While { cond, body } => {
                self.walk_expr(cond, state, errors);
                self.walk_block(body, state, errors);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.walk_expr(scrutinee, state, errors);
                self.walk_block(body, state, errors);
            }
            ExprKind::Loop { body } => self.walk_block(body, state, errors),
            ExprKind::Range { start, end, .. } => {
                self.walk_expr(start, state, errors);
                self.walk_expr(end, state, errors);
            }
            ExprKind::Throw(e) => self.walk_expr(e, state, errors),
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt { self.walk_expr(e, state, errors); }
            }
            // Литералы / ident'ы / handler-литералы — без рекурсии.
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
            | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
            | ExprKind::HandlerLit { .. } => {}
        }
    }

    /// Plan 16 Ф.2-Ф.4: проверка capability-rules на конкретном узле.
    /// Сейчас — только для Call'ов; forbid/realtime/with управляют
    /// state'ом, не вызывая check'ов на собственном узле.
    fn check_capabilities_at(&self, e: &Expr, state: &CapState, errors: &mut Vec<Diagnostic>) {
        let ExprKind::Call { func, .. } = &e.kind else { return; };
        // Path-form: `Type.method`, `Effect.op` или `[]T.method`.
        // Для `[]T.method()` парсер строит Member{obj: Path(["__array", T]), name}.
        let path: Vec<String> = match &func.kind {
            ExprKind::Path(parts) => parts.clone(),
            ExprKind::Member { obj, name } => {
                match &obj.kind {
                    ExprKind::Ident(n) => vec![n.clone(), name.clone()],
                    // `[]T.method`: Path(["__array","T"]) → ["[]T", method].
                    ExprKind::Path(parts) if parts.len() == 2 && parts[0] == "__array" => {
                        vec![format!("[]{}", parts[1]), name.clone()]
                    }
                    ExprKind::Path(parts) => {
                        let mut v = parts.clone();
                        v.push(name.clone());
                        v
                    }
                    _ => return, // dynamic member-call; не resolve'им
                }
            }
            ExprKind::Ident(n) => vec![n.clone()],
            _ => return,
        };
        // 1. Effect-op call: `Effect.op(...)` где Effect — registered effect-type.
        if path.len() == 2 {
            let head = &path[0];
            if self.effect_decls.contains_key(head) {
                self.check_forbid_intersection(head, state, e.span, errors);
                if state.realtime_active && realtime_suspend_effect(head) {
                    errors.push(Diagnostic::new(
                        format!(
                            "cannot use suspend-effect `{}` inside `realtime` block (D64): \
                             {}.{} may suspend the fiber. Hint: extract the effectful work \
                             out of `realtime` block, or use non-blocking alternative \
                             (e.g. `Channel.try_recv` instead of `Channel.recv`).",
                            head, head, &path[1]
                        ),
                        e.span,
                    ));
                }
            }
        }
        // 2. Free-fn call: lookup callee.effects.
        if path.len() == 1 {
            if let Some(callee) = self.fn_decls.get(&path[0]) {
                self.check_callee_effects(callee, &path[0], state, e.span, errors);
            }
        }
        // 3. Method call: `Type.method` или `obj.method` — lookup в method_table.
        // (Только receiver-Path формы; instance-method через obj.method
        // требует type-инференции, отложен.)
        if path.len() == 2 {
            if let Some(methods) = self.method_table.get(&path[0]) {
                if let Some(fns) = methods.get(&path[1]) {
                    for callee in fns {
                        self.check_callee_effects(callee, &format!("{}.{}", path[0], path[1]), state, e.span, errors);
                    }
                }
            }
        }
        // 4. Plan 16 Ф.4: nogc alloc-fn check.
        if state.realtime_nogc && nogc_blacklisted_call(&path) {
            errors.push(Diagnostic::new(
                format!(
                    "cannot allocate inside `realtime nogc` block (D64): `{}` allocates \
                     on managed heap. Hint: use `region {{ ... }}` for arena-allocations, \
                     or move the allocation outside the `realtime nogc` block.",
                    path.join(".")
                ),
                e.span,
            ));
        }
    }

    /// Plan 16 Ф.2: проверка пересечения callee.effects с union forbidden-стека.
    fn check_callee_effects(
        &self,
        callee: &FnDecl,
        callee_label: &str,
        state: &CapState,
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        // Pure — всегда OK.
        if callee.effects.is_empty() && state.forbidden_stack.is_empty() && !state.realtime_active {
            return;
        }
        let forbidden = state.union_forbidden();
        for eff in &callee.effects {
            let TypeRef::Named { path, .. } = eff else { continue; };
            if path.is_empty() { continue; }
            let name = &path[0];
            // Forbid check.
            if forbidden.contains(name) {
                errors.push(Diagnostic::new(
                    format!(
                        "function `{}` requires effect `{}`, forbidden by enclosing \
                         `forbid {}` block (D63). Hint: pure code inside `forbid` is OK; \
                         to use `{}`, restructure to compute effect-free results inside \
                         and apply effects outside the sandbox.",
                        callee_label, name, name, name
                    ),
                    span,
                ));
            }
            // Realtime check.
            if state.realtime_active && realtime_suspend_effect(name) {
                errors.push(Diagnostic::new(
                    format!(
                        "function `{}` requires suspend-effect `{}`, cannot be called \
                         inside `realtime` block (D64). Hint: realtime guarantees \
                         no fiber-suspension; effects {} block.",
                        callee_label, name,
                        "Net/Fs/Db/Time/Blocking suspend the fiber and are forbidden inside realtime"
                    ),
                    span,
                ));
            }
        }
    }

    /// Plan 16 D63: единичная проверка effect'a против forbidden-стека.
    fn check_forbid_intersection(
        &self,
        eff_name: &str,
        state: &CapState,
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        let forbidden = state.union_forbidden();
        if forbidden.contains(eff_name) {
            errors.push(Diagnostic::new(
                format!(
                    "use of effect `{}` is forbidden by enclosing `forbid {}` block (D63).",
                    eff_name, eff_name
                ),
                span,
            ));
        }
    }
}

/// Render method signature `name(p1 T1, p2 T2) -> Ret` — для diagnostic'а.
fn render_method_sig(name: &str, params: &[Param], ret: &Option<TypeRef>) -> String {
    let p_strs: Vec<String> = params.iter().map(|p| {
        format!("{} {}", p.name, render_type_ref(&p.ty))
    }).collect();
    let r = ret.as_ref().map(|t| format!(" -> {}", render_type_ref(t))).unwrap_or_default();
    format!("{}({}){}", name, p_strs.join(", "), r)
}

fn render_type_ref(t: &TypeRef) -> String {
    match t {
        TypeRef::Named { path, generics, .. } => {
            if generics.is_empty() {
                path.join(".")
            } else {
                let g: Vec<String> = generics.iter().map(render_type_ref).collect();
                format!("{}[{}]", path.join("."), g.join(", "))
            }
        }
        TypeRef::Array(inner, _) => format!("[]{}", render_type_ref(inner)),
        TypeRef::FixedArray(n, inner, _) => format!("[{}]{}", n, render_type_ref(inner)),
        TypeRef::Tuple(items, _) => {
            let s: Vec<String> = items.iter().map(render_type_ref).collect();
            format!("({})", s.join(", "))
        }
        TypeRef::Func { params, return_type, .. } => {
            let p: Vec<String> = params.iter().map(render_type_ref).collect();
            let r = return_type.as_ref().map(|t| format!(" -> {}", render_type_ref(t))).unwrap_or_default();
            format!("fn({}){}", p.join(", "), r)
        }
        TypeRef::Unit(_) => "()".to_string(),
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
            has_throw_in_expr(func) || args.iter().any(|a| has_throw_in_expr(a.expr())),
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

