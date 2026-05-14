//! Type checker и effect inference.
//!
//! Минимальная реализация: проверяем имена типов, выводим типы локальных
//! переменных, выводим эффекты для private функций (D28). Generic-параметры
//! проверяются как abstract names — мономорфизация делается при
//! интерпретации (treewalk не требует всего).

use crate::ast::*;
use crate::diag::{Diagnostic, FileId, MAIN_FILE_ID, Span};
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
///
/// **D84 overloading:** `fns` хранит **Vec** для каждого имени, потому
/// что одно имя может иметь несколько перегрузок (методы с одним именем
/// на одном receiver-type, free-functions с разными signatures, разные
/// `From[X]`). Резолв на call-site по argument-types — ответственность
/// codegen / bound-checker.
#[derive(Debug, Default)]
pub struct ModuleEnv {
    pub types: HashMap<String, TypeDecl>,
    pub fns: HashMap<String, Vec<FnDecl>>,
    pub consts: HashMap<String, ConstDecl>,
    /// Plan 33.1 Ф.3: список доказанных (fn_name, contract span) контрактов.
    /// Codegen в release-сборке стирает соответствующие runtime-checks
    /// (zero-cost guarantee). В debug — checks всегда emit'ятся.
    pub proven_contracts: Vec<(String, Span)>,
}

/// Минимальная проверка модуля. Регистрирует имена и базовую структуру —
/// для bootstrap'а этого достаточно: интерпретатор ловит ошибки типов в
/// runtime через match-mismatch и method-not-found.
pub fn check_module(module: &Module) -> Result<ModuleEnv, Vec<Diagnostic>> {
    let mut env = ModuleEnv::default();
    let mut errors = Vec::new();
    let mut names: HashSet<String> = HashSet::new();

    // D82: `external fn` whitelisted только в `std/runtime/*.nv`. User-код
    // не должен использовать external — это keyword для документирования
    // stdlib runtime-функций, реализованных в nova_rt/*.h. Будущий
    // `extern("C")` для FFI к сторонним libs — отдельный keyword.
    //
    // Plan 42 Sub-plan 42.6: detect runtime module по обоих declaration
    // форматов (rev-1 legacy + rev-3 parent.X). Logic — в manifest helper.
    let is_runtime_module = crate::manifest::is_stdlib_runtime_module(&module.name);
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
                // D84: overload по любой из четырёх осей (receiver-type,
                // arg-types, result-type, arity). Под одним именем может
                // быть несколько overloads, различающихся sig'ами; codegen
                // и bound-checker резолвят call-site по argument-types.
                //
                // Запрещено только **точное дублирование signature**
                // (одинаковые arity + одинаковые arg-types) — это была бы
                // ambiguity без возможности резолва. Проверка ниже.
                names.insert(key.clone()); // names — для конфликтов с типами/const'ами
                let entry = env.fns.entry(key.clone()).or_default();
                // D84: overload-disambiguation по любой из четырёх осей.
                // Точное дублирование запрещено — это требует одновременного
                // совпадения **arity + arg-types + return-type** (плюс
                // receiver-type, который уже включён в `key`). Если хоть одна
                // ось различается — overload валиден.
                let new_arg_tys: Vec<&TypeRef> = fd.params.iter().map(|p| &p.ty).collect();
                let dup_existing = entry.iter().find(|existing| {
                    // Arity + arg-types одинаковы?
                    let args_equal = existing.params.len() == fd.params.len()
                        && existing.params.iter().zip(new_arg_tys.iter())
                            .all(|(p, new_ty)| typeref_equal(&p.ty, new_ty));
                    if !args_equal { return false; }
                    // Return-type одинаков? (None / None или Some/Some equal).
                    match (&existing.return_type, &fd.return_type) {
                        (None, None) => true,
                        (Some(a), Some(b)) => typeref_equal(a, b),
                        _ => false,
                    }
                });
                if let Some(prev) = dup_existing {
                    errors.push(Diagnostic::new(
                        format!(
                            "duplicate definition `{}` with same signature \
                             (overload requires distinct param types, arity, или return type — \
                             см. D84); previous definition has identical params and return type",
                            key
                        ),
                        fd.span,
                    ));
                    let _ = prev; // silence unused
                } else {
                    entry.push(fd.clone());
                }
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

    // (typeref_equal — helper для D84 duplicate-signature detection,
    // определён в конце файла.)

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

    // D90 Plan 20 Ф.3: defer/errdefer body constraints.
    //
    // Body запрещает:
    //  - exit-control (return/throw/break/continue) — нельзя hijack
    //    exit семантику scope'а.
    //  - Fail-эффект (?/!!/throw) — double-throw невозможно сделать
    //    корректно. throw обнаруживается через AST-walk; ?/!! — в codegen
    //    они desugar'ятся в throw, поэтому достаточно catch throw.
    //  - suspend-операции (Net.*, Fs.*, Db.*, Time.sleep, parallel for,
    //    spawn, supervised, select) — defer должен быть быстрым cleanup.
    //
    // Walks по всем bodies всех функций. Spec — D90.
    check_defer_bodies(module, &mut errors);

    // D61 §1430-1434 / D90 Ф.8 (1): handler-method для эффект-операции
    // с return type `Never` ОБЯЗАН закончиться exit-control'ом
    // (`interrupt v` или `throw err` / `panic` / `exit`). Иначе нет
    // значения типа Never для возврата — handler не может законно
    // завершиться normally.
    //
    // Применяется к: Fail.fail (built-in, return Never), любым
    // user-defined effect-operations с return type Never.
    //
    // Walks все handler-литералы в module, проверяет для каждого
    // method'а, является ли соответствующая operation Never-возврат-
    // ной, и если да — body должен diverge (static analysis).
    check_handler_never_ops(module, &mut errors);

    // Plan 33.3 Ф.9 (D24): validate axiom-bodies в effect-блоках.
    // Каждый axiom должен ссылаться только на binders + pure_view-ops
    // **того же эффекта** + литералы + boolean/arith operators. Любой
    // другой identifier (включая non-pure_view ops) → error. Это
    // фундамент SMT encoding (UF mapping в Ф.9.4).
    check_effect_axioms(module, &mut errors);

    // Plan 33.3 Ф.9.6: handler verification gate.
    // Если эффект имеет pure_view-ops, любая `with E = handler` для
    // этого эффекта обязана быть помечена `#verify_handler` или
    // `#trusted_handler`. Без атрибута — compile error.
    check_handler_verification_gate(module, &mut errors);

    // Name-resolution фаза: статический поиск undefined идентификаторов
    // в expr-position. Запускается ПОСЛЕ BoundCtx/CapabilityCtx, чтобы
    // более фундаментальные ошибки (signatures/effects) приходили первыми.
    //
    // Без этой фазы код вроде `let r = 1 | undefined_var` проходил
    // typecheck и падал только на cc-этапе с малочитаемой ошибкой
    // "необъявленный идентификатор". См. NameResCtx ниже.
    let name_res = NameResCtx::build(module);
    name_res.check_module(module, &mut errors);

    // Plan 33.1 Ф.2 (D24): contract checking + purity inference.
    // Минимальный pass: проверка базовых правил для контрактов:
    // - `result` запрещён в `requires`;
    // - `old(...)` запрещён в `requires`;
    // - composition (вызов другой fn в контракте) запрещён в 33.1
    //   (будет разрешён для #pure в 33.2).
    let contract_ctx = ContractCtx::build(module);
    contract_ctx.check_module(module, &mut errors);

    // Plan 33.3 Ф.9.7 (D24): ghost-var usage check.
    // Non-ghost код не может читать ghost-var (Verus/Dafny semantics).
    // До этого: catch'илось на C-level через «undeclared identifier»;
    // теперь — proper compile-error с понятным сообщением.
    check_ghost_usage(module, &mut errors);

    // Plan 33.1 Ф.3 (D24): SMT verification.
    // TrivialBackend по умолчанию (Z3 — отдельная feature в будущем).
    // Доказанные контракты записываются в env для zero-cost release.
    // `#must_verify` errors / counterexample warnings — попадают в errors.
    if errors.is_empty() {
        // Verify только если предыдущие фазы прошли (иначе encode на
        // невалидном AST может крашнуть).
        let report = crate::verify::verify_module(module);
        env.proven_contracts = report.proven;
        for e in report.errors { errors.push(e); }
        // warnings пока silent — добавим warning infrastructure
        // в Plan 36 production hardening.
        // Note: counterexample-warnings (без #must_verify) бэк-port'ятся
        // в errors временно, чтобы в 33.1 negative-тесты могли их детектить.
        // Это будет уточнено когда добавится warning severity (Plan 36).
        let _ = report.warnings; // intentionally silent
    }

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
    /// D84: HashMap → Vec<&FnDecl> чтобы хранить multiple overloads
    /// одного имени (методы и свободные функции). Резолв в check_call_bounds —
    /// фильтр по arity. Полный type-based resolve остаётся за codegen (где
    /// есть type-инфер аргументов).
    fn_decls: HashMap<String, Vec<&'a FnDecl>>,
    method_table: HashMap<String, HashMap<String, Vec<&'a FnDecl>>>,
}

impl<'a> BoundCtx<'a> {
    fn build(module: &'a Module) -> Self {
        let mut protocol_specs = HashMap::new();
        let mut fn_decls: HashMap<String, Vec<&FnDecl>> = HashMap::new();
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
                        // D84: свободные функции тоже могут иметь overloads.
                        fn_decls.entry(f.name.clone()).or_default().push(f);
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
            // D90 Plan 20 Ф.2: body парсится, walk'аем — bound-checker
            // получит call'ы внутри body. Body-constraint проверки
            // (no Fail, no suspend, no exit-control) добавляются в Ф.3.
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, scope, errors);
            }
            // Plan 33.2 Ф.8: assert_static — walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr, scope, errors),
        }
    }

    fn walk_expr(&self, e: &Expr, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        // Проверяем сам call перед рекурсией в args (порядок не важен).
        self.check_call_bounds(e, scope, errors);
        match &e.kind {
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func, scope, errors);
                for a in args {
                    self.walk_expr(a.expr(), scope, errors);
                }
                if let Some(t) = trailing {
                    match t {
                        crate::ast::Trailing::Block(b) => self.walk_block(b, scope, errors),
                        crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                            self.walk_block(&tb.body, scope, errors)
                        }
                        crate::ast::Trailing::Fn(sb) => {
                            // Trailing-fn body: Expr или Block.
                            match &sb.body {
                                FnBody::Expr(e) => self.walk_expr(e, scope, errors),
                                FnBody::Block(b) => self.walk_block(b, scope, errors),
                                FnBody::External => {}
                            }
                        }
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, scope, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, scope, errors);
                self.walk_expr(right, scope, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, scope, errors),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => self.walk_expr(inner, scope, errors),
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
            // Plan 19, C5: BoundCtx обходит тело closure-light /
            // closure-full для генерик-bound проверок. Полный
            // bidirectional inference — фаза C6; здесь — только walk.
            ExprKind::ClosureLight { body, .. } => match body {
                crate::ast::ClosureBody::Expr(e) => self.walk_expr(e, scope, errors),
                crate::ast::ClosureBody::Block(b) => self.walk_block(b, scope, errors),
            },
            ExprKind::ClosureFull(sb) => match &sb.body {
                FnBody::Expr(e) => self.walk_expr(e, scope, errors),
                FnBody::Block(b) => self.walk_block(b, scope, errors),
                FnBody::External => {}
            },
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
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => self.walk_expr(chan, scope, errors),
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, scope, errors);
                            self.walk_expr(value, scope, errors);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard { self.walk_expr(g, scope, errors); }
                    self.walk_block(&arm.body, scope, errors);
                }
            }
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
        // D84: fn_decls — Vec<&FnDecl>. Резолв overload по arity (то, что
        // bound-checker может определить без full type-inference).
        // Если несколько overloads подходят по arity — bound-checker не
        // делает разрешение (это работа codegen, у которого есть type-info).
        // Bound-проверка пропускается; codegen ловит ambiguity на своём
        // уровне.
        let Some(overloads) = self.fn_decls.get(&fn_name) else { return; };
        let arity_matches: Vec<&&FnDecl> = overloads.iter()
            .filter(|f| f.params.len() == args.len())
            .collect();
        let callee: &FnDecl = match arity_matches.as_slice() {
            [single] => *single,
            _ => return, // нет однозначной overload по arity — пропускаем
        };
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
    // D91 (Plan 21): Channel.new allocates Nova_ChannelState + Sender + Receiver + buf.
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
    /// D84: Vec<&FnDecl> для multi-overload — все overloads имени.
    /// Capability check ходит по всем overloads (см. check_capabilities_at).
    fn_decls: HashMap<String, Vec<&'a FnDecl>>,
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
        let mut fn_decls: HashMap<String, Vec<&FnDecl>> = HashMap::new();
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
                        // D84: свободные функции тоже могут иметь overloads.
                        fn_decls.entry(f.name.clone()).or_default().push(f);
                    }
                }
                _ => {}
            }
        }
        CapabilityCtx { fn_decls, method_table, effect_decls }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        // Plan 42 Sub-plan 42.A: file-level #forbid declarations.
        // Initial forbidden set из module.attrs (per-file scope).
        // Все functions в этом file получают эти effects forbidden.
        let mut file_forbidden: HashSet<String> = HashSet::new();
        for attr in &module.attrs {
            if matches!(attr.kind, crate::ast::ModuleAttrKind::Forbid) {
                for e in &attr.effects {
                    file_forbidden.insert(e.clone());
                }
            }
        }
        for item in &module.items {
            match item {
                Item::Fn(f) => {
                    let mut state = CapState::default();
                    // Plan 42 Sub-plan 42.A: file-level #forbid initial frame.
                    if !file_forbidden.is_empty() {
                        state.forbidden_stack.push(file_forbidden.clone());
                    }
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
                    if !file_forbidden.is_empty() {
                        state.forbidden_stack.push(file_forbidden.clone());
                    }
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
            // D90 Plan 20 Ф.2: проверяем capability'и внутри body
            // defer'а. Полные constraints (no Fail/suspend/exit-control)
            // — Ф.3.
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, state, errors);
            }
            // Plan 33.2 Ф.8: assert_static — walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr, state, errors),
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
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func, state, errors);
                for a in args { self.walk_expr(a.expr(), state, errors); }
                if let Some(t) = trailing {
                    match t {
                        crate::ast::Trailing::Block(b) => self.walk_block(b, state, errors),
                        crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                            self.walk_block(&tb.body, state, errors)
                        }
                        crate::ast::Trailing::Fn(sb) => match &sb.body {
                            FnBody::Expr(e) => self.walk_expr(e, state, errors),
                            FnBody::Block(b) => self.walk_block(b, state, errors),
                            FnBody::External => {}
                        },
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, state, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, state, errors);
                self.walk_expr(right, state, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, state, errors),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => self.walk_expr(inner, state, errors),
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
            // Plan 19, C5: CapabilityCtx обходит тело closure для
            // forbid/realtime проверок (D63/D64). Closure-light и
            // closure-full одинаково — walk by body kind.
            ExprKind::ClosureLight { body, .. } => match body {
                crate::ast::ClosureBody::Expr(e) => self.walk_expr(e, state, errors),
                crate::ast::ClosureBody::Block(b) => self.walk_block(b, state, errors),
            },
            ExprKind::ClosureFull(sb) => match &sb.body {
                FnBody::Expr(e) => self.walk_expr(e, state, errors),
                FnBody::Block(b) => self.walk_block(b, state, errors),
                FnBody::External => {}
            },
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
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => self.walk_expr(chan, state, errors),
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, state, errors);
                            self.walk_expr(value, state, errors);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard { self.walk_expr(g, state, errors); }
                    self.walk_block(&arm.body, state, errors);
                }
            }
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
        // D84: fn_decls — Vec<&FnDecl>. Без полного type-resolve в
        // bound-checker'е невозможно выбрать конкретную overload —
        // проверяем эффекты у **всех** overloads (consistent с тем что
        // делает method_table-ветка ниже). False-positive если разные
        // overloads имеют разные эффекты — в реальных API маловероятно
        // (overloads обычно отличаются типом аргумента, не эффектами),
        // но если случится — программист дисамбигуирует через cast.
        if path.len() == 1 {
            if let Some(overloads) = self.fn_decls.get(&path[0]) {
                for callee in overloads.iter() {
                    self.check_callee_effects(callee, &path[0], state, e.span, errors);
                }
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

// ============================================================================
// Name-resolution фаза.
//
// Pre-collects top-level имена (fns/types/consts/variants/built-ins) +
// walk fn/test bodies со scope-стеком. На `ExprKind::Ident(name)`
// проверяет, что `name` в (текущий scope ∪ top-level ∪ built-ins).
// Иначе — diagnostic «undefined identifier`.
//
// **Конкервативная стратегия**: лучше пропустить undefined чем
// false-positive. Случаи, где не проверяем:
//   - `obj.method(args)` / `Type.method(args)` — method-имена resolve'ятся
//     через method_table (могут быть на любом типе).
//   - `obj.field` / `Record { field: val }` — поля, не идентификаторы.
//   - Path-сегменты `mod1::mod2::name` (intermediate — модули, не expr).
//   - Tagged-template tags.
//   - Generic-params в TypeRef (это типы, не expressions).
//   - Sum-variant tag в pattern (`Some(x)` — constructor name, не expr).
// ============================================================================

/// Plan 19+: статическая проверка undefined идентификаторов.
struct NameResCtx {
    /// Plan 42.15: per-group shared declarations (Rule C). Key = file_id
    /// peer'а. Value = declarations всех peers ЕГО module-group (folder-
    /// module с общим parent dir). Peers одной группы делят namespace;
    /// между группами — НЕ делят (imported folder-module's decls не
    /// протекают).
    group_decls: HashMap<FileId, HashSet<String>>,
    /// Plan 42.15: fallback для legacy/single-file (peer_files пуст) —
    /// flat все module.items. Используется когда file_id не в group_decls.
    shared_decls: HashSet<String>,
    /// Plan 42.15: union ВСЕХ declarations (все группы + imported). НЕ
    /// для name-resolution enforcement (это нарушило бы Rule C) —
    /// используется ТОЛЬКО как эвристика в `collect_pattern_bindings`
    /// (отличить pattern-binding `let x` от variant-pattern `Some`).
    all_decls: HashSet<String>,
    /// Plan 42.15: per-peer imported item names — items ставшие
    /// видимыми в peer'е через его прямые `import` (после rename +
    /// selective filter). Rule C: imports НЕ shared между peers.
    peer_imported_names: HashMap<FileId, HashSet<String>>,
    /// Built-in имена, доступные в любом scope без объявления:
    /// primitive types, prelude variants (None/Some/Ok/Err), bool
    /// литералы (true/false), builtin functions (assert/print/...),
    /// special idents (Self).
    builtins: HashSet<String>,
    /// Per-peer import namespace (Plan 42.4 Rule C).
    /// Key = file_id of peer file (MAIN_FILE_ID for entry).
    /// Value = set of module/alias names visible in that peer.
    peer_module_names: HashMap<FileId, HashSet<String>>,
}

impl NameResCtx {
    fn build(module: &Module) -> Self {
        // Plan 42.15: per-group shared declarations (Rule C).
        //
        // **Module-group** = набор peer-файлов одного folder-module
        // (имеют общий parent dir). Внутри группы peers делят
        // declarations namespace (Rule C: «peers share declarations»).
        // МЕЖДУ группами — НЕ делят (imported folder-module's decls не
        // протекают в entry's namespace).
        //
        // `group_decls`: HashMap<FileId, HashSet<String>> — для каждого
        // peer'а (по file_id) → declarations всех peers его группы.
        let mut group_decls: HashMap<FileId, HashSet<String>> = HashMap::new();
        // Fallback для legacy/single-file (peer_files пуст).
        let mut shared_decls: HashSet<String> = HashSet::new();

        fn collect_decl_names(items: &[Item], out: &mut HashSet<String>) {
            for item in items {
                match item {
                    Item::Fn(fd) => {
                        // free-functions (без receiver) валидны как
                        // bare-ident `foo()`. Методы — через obj.method.
                        if fd.receiver.is_none() {
                            out.insert(fd.name.clone());
                        }
                    }
                    Item::Type(td) => {
                        out.insert(td.name.clone());
                        // Variant-имена sum-типов: `Some(x)`, `Red`, etc.
                        if let TypeDeclKind::Sum(variants) = &td.kind {
                            for v in variants {
                                out.insert(v.name.clone());
                            }
                        }
                    }
                    Item::Const(cd) => {
                        out.insert(cd.name.clone());
                    }
                    Item::Let(_) | Item::Test(_) => {}
                }
            }
        }

        if module.peer_files.is_empty() {
            // Legacy/single-file: flat — все module.items.
            collect_decl_names(&module.items, &mut shared_decls);
        } else {
            // Группируем peers по parent dir пути. Все peers одной
            // папки = одна module-group, делят declarations.
            let mut groups: HashMap<std::path::PathBuf, HashSet<String>> = HashMap::new();
            let mut peer_group_key: HashMap<FileId, std::path::PathBuf> = HashMap::new();
            for pf in &module.peer_files {
                let group_key = pf.path.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| pf.path.clone());
                peer_group_key.insert(pf.file_id, group_key.clone());
                let entry = groups.entry(group_key).or_default();
                collect_decl_names(&pf.items_here, entry);
            }
            // Разворачиваем: для каждого peer'а — decls его группы.
            for pf in &module.peer_files {
                if let Some(gk) = peer_group_key.get(&pf.file_id) {
                    if let Some(decls) = groups.get(gk) {
                        group_decls.insert(pf.file_id, decls.clone());
                    }
                }
            }
        }

        let builtins: HashSet<String> = [
            // Numeric primitives.
            "int", "i8", "i16", "i32", "i64",
            "u8", "u16", "u32", "u64",
            "f32", "f64", "uint", "size",
            // Other primitives.
            "bool", "str", "byte", "char", "unit", "Never", "any",
            // Boolean literals (parsed как Ident в bool-context кое-где).
            "true", "false",
            // Special idents.
            "Self", "self",
            // Prelude variants Option / Result / Error / RuntimeError.
            "None", "Some", "Ok", "Err",
            "Option", "Result", "Error",
            "DivByZero", "Overflow", "IndexOutOfBounds",
            "TypeMismatch", "AssertFailed", "NoHandler",
            "RuntimeError",
            // Built-in functions (см. codegen::emit_c.rs special-cases).
            "assert", "debug_assert", "print", "println",
            "panic", "exit",
            // Plan 32: GC introspection namespace (std.runtime.gc).
            // Используется как `gc.heap_size()`, `gc.collect()` и т.д.
            // Source of truth для signatures: std/runtime/gc.nv (external fn).
            // Codegen dispatch: emit_c.rs:7155 special-case на name == "gc".
            // Builtin запись нужна потому что cross-file bare-name resolve
            // не работает (Plan 35 Ф.1).
            "gc",
            // Plan 44.2 Этап 3: fiber arena introspection namespace
            // (std.runtime.fibers). `fibers.slot_count()`, etc.
            // Source of truth: std/runtime/fibers.nv. Codegen dispatch:
            // emit_c.rs `name == "fibers"`.
            "fibers",
            // Plan 44 Этап 0: M:N runtime control namespace
            // (std.runtime.runtime). `runtime.init(n)`, `runtime.shutdown()`.
            "runtime",
            // Default Fail-effect type (D65 placeholder).
            "Fail",
            // Detach effect-type для detach {} expression (D50).
            "Detach",
            // CancelToken — bind name в cancel_scope { tok => ... } (D75)
            // регистрируется отдельно во время walk; в общий builtin
            // не добавляем.
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        // Plan 42.4 Rule C: per-peer import namespace isolation.
        // Build a map from file_id → visible module names for that peer.
        // If peer_files is empty (legacy/single-file), fall back to entry.
        let mut peer_module_names: HashMap<FileId, HashSet<String>> = HashMap::new();

        let build_import_names = |imports: &[Import], module_name: &[String]| -> HashSet<String> {
            let mut names: HashSet<String> = HashSet::new();
            for imp in imports {
                if let Some(alias) = &imp.alias {
                    names.insert(alias.clone());
                }
                if let Some(last) = imp.path.last() {
                    names.insert(last.clone());
                }
                if let Some(head) = imp.path.first() {
                    names.insert(head.clone());
                }
            }
            // Own module name (last + first segment) for self-reference.
            if let Some(head) = module_name.first() { names.insert(head.clone()); }
            if let Some(last) = module_name.last() { names.insert(last.clone()); }
            names
        };

        if module.peer_files.is_empty() {
            // Legacy/single-file: entry imports under MAIN_FILE_ID.
            peer_module_names.insert(
                MAIN_FILE_ID,
                build_import_names(&module.imports, &module.name),
            );
        } else {
            for pf in &module.peer_files {
                peer_module_names.insert(
                    pf.file_id,
                    build_import_names(&pf.imports, &module.name),
                );
            }
        }

        // Plan 42.15: per-peer imported item names. Resolver наполнил
        // `PeerFile.imported_item_names` (items притащенные прямыми
        // imports этого peer'а). Rule C: imports не shared между peers.
        let mut peer_imported_names: HashMap<FileId, HashSet<String>> = HashMap::new();
        for pf in &module.peer_files {
            peer_imported_names.insert(pf.file_id, pf.imported_item_names.clone());
        }

        // Plan 42.15: all_decls — union ВСЕХ declarations (эвристика для
        // pattern-binding detection, НЕ для enforcement).
        let mut all_decls: HashSet<String> = shared_decls.clone();
        for gd in group_decls.values() {
            all_decls.extend(gd.iter().cloned());
        }
        // Также merged module.items (imported items для эвристики).
        collect_decl_names(&module.items, &mut all_decls);

        NameResCtx {
            group_decls, shared_decls, all_decls, builtins,
            peer_module_names, peer_imported_names,
        }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        for item in &module.items {
            let file_id = match item {
                Item::Fn(f) => f.span.file_id,
                Item::Test(t) => t.span.file_id,
                Item::Const(c) => c.span.file_id,
                Item::Type(t) => t.span.file_id,
                Item::Let(l) => l.span.file_id,
            };
            match item {
                Item::Fn(f) => self.walk_fn(f, file_id, errors),
                Item::Test(t) => {
                    let mut scope: Vec<HashSet<String>> = vec![HashSet::new()];
                    self.walk_block(&t.body, file_id, &mut scope, errors);
                }
                Item::Const(c) => {
                    let mut scope: Vec<HashSet<String>> = vec![HashSet::new()];
                    self.walk_expr(&c.value, file_id, &mut scope, errors);
                }
                _ => {}
            }
        }
    }

    fn walk_fn(&self, f: &FnDecl, file_id: FileId, errors: &mut Vec<Diagnostic>) {
        // External — нет тела.
        if matches!(f.body, FnBody::External) { return; }
        let mut scope: Vec<HashSet<String>> = vec![HashSet::new()];
        let mut frame: HashSet<String> = HashSet::new();
        // Receiver: self/Self доступны через builtins; нет нужды добавлять.
        if let Some(_recv) = &f.receiver {
            frame.insert("self".to_string());
        }
        for p in &f.params {
            frame.insert(p.name.clone());
        }
        // Generic-params могут использоваться в expr-position? — Нет
        // (по spec). Но безопасно их добавить чтобы не флагать False+
        // если parser/codegen где-то их так трактует.
        for g in &f.generics {
            frame.insert(g.name.clone());
        }
        scope.push(frame);
        match &f.body {
            FnBody::Expr(e) => self.walk_expr(e, file_id, &mut scope, errors),
            FnBody::Block(b) => self.walk_block(b, file_id, &mut scope, errors),
            FnBody::External => {}
        }
        scope.pop();
    }

    fn walk_block(
        &self,
        b: &Block,
        file_id: FileId,
        scope: &mut Vec<HashSet<String>>,
        errors: &mut Vec<Diagnostic>,
    ) {
        scope.push(HashSet::new());
        for s in &b.stmts {
            self.walk_stmt(s, file_id, scope, errors);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(t, file_id, scope, errors);
        }
        scope.pop();
    }

    fn walk_stmt(
        &self,
        s: &Stmt,
        file_id: FileId,
        scope: &mut Vec<HashSet<String>>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, file_id, scope, errors),
            Stmt::Let(d) => {
                // Right-side вычисляется в текущем scope (let не
                // рекурсивный). Затем pattern-bindings добавляются в
                // текущий frame.
                self.walk_expr(&d.value, file_id, scope, errors);
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(&d.pattern, &mut bindings);
                if let Some(top) = scope.last_mut() {
                    for n in bindings { top.insert(n); }
                }
            }
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, file_id, scope, errors);
                self.walk_expr(value, file_id, scope, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, file_id, scope, errors); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, file_id, scope, errors),
            // D90 (Plan 20): defer/errdefer body — обычный expr в текущем
            // scope. Bindings внутри body локальны их собственным under-scope'ам;
            // на верхнем уровне defer не вводит новых имён.
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, file_id, scope, errors);
            }
            // Plan 33.2 Ф.8: assert_static — walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr, file_id, scope, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }

    fn walk_expr(
        &self,
        e: &Expr,
        file_id: FileId,
        scope: &mut Vec<HashSet<String>>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match &e.kind {
            ExprKind::Ident(name) => {
                if !self.is_known(name, file_id, scope) {
                    errors.push(Diagnostic::new(
                        format!("undefined identifier `{}`", name),
                        e.span,
                    ));
                }
            }
            // Path-form `Module.func` / `Type.method`: head — модуль или
            // type. Plan 42.15 Ф.3: head-segment check для lowercase
            // module-alias'ов (Rule C: peer видит только свои imports).
            //
            // Проверяем ТОЛЬКО lowercase head: Capitalized = тип/effect/
            // variant (cross-file, bootstrap-консервативно пропускаем).
            // lowercase head должен быть: builtin namespace (gc/fibers/
            // runtime) ИЛИ module-alias в peer's import scope. Если нет —
            // вероятно use чужого import'а (Rule C violation) или typo.
            ExprKind::Path(parts) => {
                if let Some(head) = parts.first() {
                    let is_lowercase = head.chars().next()
                        .map(|c| c.is_ascii_lowercase())
                        .unwrap_or(false);
                    if is_lowercase {
                        let in_builtins = self.builtins.contains(head);
                        let in_peer_modules = self.peer_module_names.get(&file_id)
                            .or_else(|| self.peer_module_names.get(&MAIN_FILE_ID))
                            .map_or(false, |s| s.contains(head));
                        // Также head может быть local binding (struct в
                        // scope) — тогда это фактически Member-access;
                        // парсер иногда эмитит Path. Проверяем scope.
                        let in_scope = scope.iter().rev()
                            .any(|frame| frame.contains(head));
                        if !in_builtins && !in_peer_modules && !in_scope {
                            errors.push(Diagnostic::new(
                                format!(
                                    "undefined module / name `{}` in path expression \
                                     (Rule C: peer sees only its own imports)",
                                    head),
                                e.span,
                            ));
                        }
                    }
                }
            }
            // SelfAccess — `@field` или `@method`. Не Ident.
            ExprKind::SelfAccess => {}

            // Литералы.
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit => {}

            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr(e) = p {
                        self.walk_expr(e, file_id, scope, errors);
                    }
                }
            }

            ExprKind::Call { func, args, trailing } => {
                // Special-case: если func — bare Ident, может быть
                // variant-constructor (`Square(5)`) — top_level.contains.
                // is_known покрывает оба варианта (fn + variant).
                self.walk_expr(func, file_id, scope, errors);
                for a in args {
                    self.walk_expr(a.expr(), file_id, scope, errors);
                }
                if let Some(t) = trailing {
                    self.walk_trailing(t, file_id, scope, errors);
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, file_id, scope, errors),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => {
                self.walk_expr(inner, file_id, scope, errors)
            }
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, file_id, scope, errors);
                self.walk_expr(b, file_id, scope, errors);
            }
            ExprKind::As(e, _) | ExprKind::Is(e, _) => self.walk_expr(e, file_id, scope, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, file_id, scope, errors);
                self.walk_expr(right, file_id, scope, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, file_id, scope, errors),

            // Member-access: проверяем obj (это expr), но НЕ name (field/method).
            ExprKind::Member { obj, .. } => self.walk_expr(obj, file_id, scope, errors),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, file_id, scope, errors);
                self.walk_expr(index, file_id, scope, errors);
            }

            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, file_id, scope, errors);
                self.walk_block(then, file_id, scope, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, file_id, scope, errors),
                        ElseBranch::If(e) => self.walk_expr(e, file_id, scope, errors),
                    }
                }
            }
            ExprKind::IfLet { pattern, scrutinee, then, else_ } => {
                self.walk_expr(scrutinee, file_id, scope, errors);
                // Pattern-bindings — в scope только для then-branch.
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(pattern, &mut bindings);
                scope.push(bindings);
                self.walk_block(then, file_id, scope, errors);
                scope.pop();
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, file_id, scope, errors),
                        ElseBranch::If(e) => self.walk_expr(e, file_id, scope, errors),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, file_id, scope, errors);
                for arm in arms {
                    let mut bindings: HashSet<String> = HashSet::new();
                    self.collect_pattern_bindings(&arm.pattern, &mut bindings);
                    scope.push(bindings);
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g, file_id, scope, errors);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                        MatchArmBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                    }
                    scope.pop();
                }
            }
            ExprKind::For { pattern, iter, body } => {
                self.walk_expr(iter, file_id, scope, errors);
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(pattern, &mut bindings);
                scope.push(bindings);
                self.walk_block(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::ParallelFor { pattern, iter, body } => {
                self.walk_expr(iter, file_id, scope, errors);
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(pattern, &mut bindings);
                scope.push(bindings);
                self.walk_block(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::While { cond, body } => {
                self.walk_expr(cond, file_id, scope, errors);
                self.walk_block(body, file_id, scope, errors);
            }
            ExprKind::WhileLet { pattern, scrutinee, body } => {
                self.walk_expr(scrutinee, file_id, scope, errors);
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(pattern, &mut bindings);
                scope.push(bindings);
                self.walk_block(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::Loop { body } => self.walk_block(body, file_id, scope, errors),
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { binding, chan } => {
                            self.walk_expr(chan, file_id, scope, errors);
                            let mut bindings: HashSet<String> = HashSet::new();
                            if let Some(b) = binding { bindings.insert(b.clone()); }
                            scope.push(bindings);
                            if let Some(g) = &arm.guard { self.walk_expr(g, file_id, scope, errors); }
                            self.walk_block(&arm.body, file_id, scope, errors);
                            scope.pop();
                        }
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, file_id, scope, errors);
                            self.walk_expr(value, file_id, scope, errors);
                            if let Some(g) = &arm.guard { self.walk_expr(g, file_id, scope, errors); }
                            self.walk_block(&arm.body, file_id, scope, errors);
                        }
                        SelectOp::Default => {
                            if let Some(g) = &arm.guard { self.walk_expr(g, file_id, scope, errors); }
                            self.walk_block(&arm.body, file_id, scope, errors);
                        }
                    }
                }
            }

            ExprKind::Block(b) => self.walk_block(b, file_id, scope, errors),

            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => {
                            self.walk_expr(e, file_id, scope, errors);
                        }
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for e in elems { self.walk_expr(e, file_id, scope, errors); }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    match &f.value {
                        Some(v) => self.walk_expr(v, file_id, scope, errors),
                        None => {
                            // Shorthand `{ name }` (D52 field punning):
                            // `name` — это ident, который должен быть
                            // в scope.
                            if !f.is_spread && !self.is_known(&f.name, file_id, scope) {
                                errors.push(Diagnostic::new(
                                    format!("undefined identifier `{}`", f.name),
                                    f.span,
                                ));
                            }
                        }
                    }
                }
            }

            // Tagged-template: tag — это специальный DSL-marker
            // (sql, json, html, ...). В bootstrap'е tag-функция
            // игнорируется (parts конкатенируются), но в production
            // tag — это runtime-функция/macro. Не проверяем tag как
            // Ident — это special-form syntax, не обычный expr-call.
            // Args (`${expr}` интерполяции) — обычные expressions.
            ExprKind::TaggedTemplate { args, .. } => {
                for a in args { self.walk_expr(a, file_id, scope, errors); }
            }

            // Lambda (legacy) / closure-light / closure-full — params
            // push'ятся как новый scope frame.
            ExprKind::Lambda { params, body, .. } => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in params { frame.insert(p.name.clone()); }
                scope.push(frame);
                self.walk_expr(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::ClosureLight { params, body } => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in params {
                    if p.name != "_" { frame.insert(p.name.clone()); }
                }
                scope.push(frame);
                match body {
                    crate::ast::ClosureBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                    crate::ast::ClosureBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                }
                scope.pop();
            }
            ExprKind::ClosureFull(sb) => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in &sb.params { frame.insert(p.name.clone()); }
                scope.push(frame);
                match &sb.body {
                    FnBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                    FnBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                    FnBody::External => {}
                }
                scope.pop();
            }

            ExprKind::With { bindings, body } => {
                // Effect-handler vals — обычные expressions.
                for b in bindings {
                    self.walk_expr(&b.handler, file_id, scope, errors);
                }
                self.walk_block(body, file_id, scope, errors);
            }
            ExprKind::HandlerLit { methods, .. } => {
                // Каждый method — handler-op с собственным scope params.
                for m in methods {
                    let mut frame: HashSet<String> = HashSet::new();
                    for p in &m.params { frame.insert(p.name.clone()); }
                    scope.push(frame);
                    match &m.body {
                        HandlerMethodBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                        HandlerMethodBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                    }
                    scope.pop();
                }
            }
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt { self.walk_expr(e, file_id, scope, errors); }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.walk_block(body, file_id, scope, errors);
            }
            ExprKind::Range { start, end, .. } => {
                self.walk_expr(start, file_id, scope, errors);
                self.walk_expr(end, file_id, scope, errors);
            }
            ExprKind::Spawn(body) => self.walk_expr(body, file_id, scope, errors),
            ExprKind::Supervised(body) | ExprKind::Detach(body) => {
                self.walk_block(body, file_id, scope, errors);
            }
            ExprKind::CancelScope { token_name, body } => {
                let mut frame: HashSet<String> = HashSet::new();
                frame.insert(token_name.clone());
                scope.push(frame);
                self.walk_block(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::Throw(inner) => self.walk_expr(inner, file_id, scope, errors),
        }
    }

    fn walk_trailing(
        &self,
        t: &crate::ast::Trailing,
        file_id: FileId,
        scope: &mut Vec<HashSet<String>>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match t {
            crate::ast::Trailing::Block(b) => self.walk_block(b, file_id, scope, errors),
            crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in &tb.params { frame.insert(p.name.clone()); }
                scope.push(frame);
                self.walk_block(&tb.body, file_id, scope, errors);
                scope.pop();
            }
            crate::ast::Trailing::Fn(sb) => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in &sb.params { frame.insert(p.name.clone()); }
                scope.push(frame);
                match &sb.body {
                    FnBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                    FnBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                    FnBody::External => {}
                }
                scope.pop();
            }
        }
    }

    /// Собрать все bindings из pattern (только names, без проверки
    /// variant-tag'ов или field-name'ов — это constructor/field
    /// references, не expr-bindings).
    fn collect_pattern_bindings(&self, p: &Pattern, out: &mut HashSet<String>) {
        match p {
            Pattern::Wildcard(_) => {}
            Pattern::Literal(_, _) => {}
            Pattern::Ident { name, .. } => {
                // Edge-case: Pattern::Ident { name: "Some" } — это
                // unit-variant Some? Нет, парсер emit'ит Variant { path:
                // ["Some"], kind: Unit }. Здесь — настоящий binding.
                // Но если имя совпадает с известным variant — считаем
                // это variant-pattern, не binding (D52 семантика
                // pattern-matching). Также Capitalized-имена в bootstrap
                // — это всегда type/variant (cross-file), не binding.
                let is_variant_like = self.builtins.contains(name)
                    || self.all_decls.contains(name)
                    || name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false);
                if !is_variant_like {
                    out.insert(name.clone());
                }
            }
            Pattern::Variant { kind, .. } => {
                // path = variant-tag — не binding.
                match kind {
                    VariantPatternKind::Unit => {}
                    VariantPatternKind::Tuple { patterns, .. } => {
                        for sub in patterns {
                            self.collect_pattern_bindings(sub, out);
                        }
                    }
                }
            }
            Pattern::Record { fields, .. } => {
                for f in fields {
                    match &f.pattern {
                        Some(sub) => self.collect_pattern_bindings(sub, out),
                        // Shorthand `{ name }` — name — это binding
                        // (одновременно field-name и bound variable).
                        None => { out.insert(f.name.clone()); }
                    }
                }
            }
            Pattern::Array { elems, .. } => {
                for el in elems {
                    match el {
                        ArrayPatternElem::Item(sub) => self.collect_pattern_bindings(sub, out),
                        ArrayPatternElem::Rest => {}
                        ArrayPatternElem::RestBind(name) => { out.insert(name.clone()); }
                    }
                }
            }
            Pattern::Tuple(elems, _) => {
                for sub in elems { self.collect_pattern_bindings(sub, out); }
            }
            Pattern::Binding { name, inner, .. } => {
                out.insert(name.clone());
                self.collect_pattern_bindings(inner, out);
            }
            Pattern::Or { alternatives, .. } => {
                // По spec все alternatives имеют одинаковый набор
                // bindings; берём из первого. (Bootstrap-семантика — см.
                // ast::Pattern::Or doc.)
                if let Some(first) = alternatives.first() {
                    self.collect_pattern_bindings(first, out);
                }
            }
        }
    }

    fn is_known(&self, name: &str, file_id: FileId, scope: &[HashSet<String>]) -> bool {
        if self.builtins.contains(name) { return true; }
        // Plan 42.15 Rule C: declarations module-group этого peer'а
        // (peers одного folder-module делят declarations namespace).
        // Fallback на flat shared_decls для legacy/single-file.
        if let Some(gd) = self.group_decls.get(&file_id) {
            if gd.contains(name) { return true; }
        } else if self.shared_decls.contains(name) {
            return true;
        }
        // Plan 42.15: per-peer imported item names — items притащенные
        // прямыми imports ИМЕННО этого peer'а. Rule C: imports НЕ shared.
        // Fallback на MAIN_FILE_ID если file_id не найден (legacy).
        let imported = self.peer_imported_names.get(&file_id)
            .or_else(|| self.peer_imported_names.get(&MAIN_FILE_ID));
        if imported.map_or(false, |s| s.contains(name)) { return true; }
        // Plan 42.4 Rule C: per-peer import namespace (module/alias names).
        let module_names = self.peer_module_names.get(&file_id)
            .or_else(|| self.peer_module_names.get(&MAIN_FILE_ID));
        if module_names.map_or(false, |s| s.contains(name)) { return true; }
        for frame in scope.iter().rev() {
            if frame.contains(name) { return true; }
        }
        // Bootstrap-консервативность: имена начинающиеся с заглавной
        // буквы по convention — типы / variants / модули. Bootstrap
        // не имеет cross-file name resolution, поэтому ident вроде
        // `HashMap` (из другого .nv файла) приходит сюда не задекларированным.
        // Чтобы не флагать такие cross-file типы как undefined,
        // пропускаем Capitalized-ident'ы. Опечатки в lowercase
        // именах (snake_case convention для vars/fns) — настоящие
        // undefined и будут ловиться.
        if let Some(c) = name.chars().next() {
            if c.is_ascii_uppercase() { return true; }
        }
        false
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
        // D90: defer/errdefer body **запрещают** throw внутри (Ф.3
        // body-constraint). Throw в body — compile error. Поэтому
        // body не считается throw-носителем — он отдельный scope с
        // ограничением. Если в body throw обнаружен — Ф.3 даст
        // отдельную compile error раньше этой проверки.
        Stmt::Defer { .. } | Stmt::ErrDefer { .. } => false,
        // Plan 33.2 Ф.8: assert_static — bool expr, no throw inside.
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => has_throw_in_expr(expr),
    }
}

fn has_throw_in_expr(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Throw(_) => true,
        ExprKind::Try(inner) => has_throw_in_expr(inner),
        // Plan 19, C7 (D85): `!!` тоже может бросить (`Err`/`None`).
        ExprKind::Bang(inner) => has_throw_in_expr(inner),
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
        ExprKind::Select { arms } => arms.iter().any(|a| {
            (match &a.op {
                SelectOp::Recv { chan, .. } => has_throw_in_expr(chan),
                SelectOp::Send { chan, value } => has_throw_in_expr(chan) || has_throw_in_expr(value),
                SelectOp::Default => false,
            }) || a.guard.as_ref().map_or(false, has_throw_in_expr)
              || has_throw_in_block(&a.body)
        }),
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

/// D84: structural equality для TypeRef (игнорирует Span'ы).
///
/// Используется для detection дублированных signatures свободных
/// функций — "точное совпадение" arity + arg-types запрещено как
/// ambiguous overload без возможности резолва.
///
/// Не использует PartialEq/Eq derive потому что TypeRef содержит
/// Span'ы (позиции в исходнике), которые отличаются у разных
/// определений того же типа.
fn typeref_equal(a: &TypeRef, b: &TypeRef) -> bool {
    match (a, b) {
        (
            TypeRef::Named { path: pa, generics: ga, .. },
            TypeRef::Named { path: pb, generics: gb, .. },
        ) => {
            pa == pb
                && ga.len() == gb.len()
                && ga.iter().zip(gb.iter()).all(|(x, y)| typeref_equal(x, y))
        }
        (TypeRef::Array(ia, _), TypeRef::Array(ib, _)) => typeref_equal(ia, ib),
        (TypeRef::FixedArray(na, ia, _), TypeRef::FixedArray(nb, ib, _)) => {
            na == nb && typeref_equal(ia, ib)
        }
        (TypeRef::Tuple(ea, _), TypeRef::Tuple(eb, _)) => {
            ea.len() == eb.len()
                && ea.iter().zip(eb.iter()).all(|(x, y)| typeref_equal(x, y))
        }
        (
            TypeRef::Func { params: pa, return_type: ra, effects: ea, .. },
            TypeRef::Func { params: pb, return_type: rb, effects: eb, .. },
        ) => {
            pa.len() == pb.len()
                && pa.iter().zip(pb.iter()).all(|(x, y)| typeref_equal(x, y))
                && match (ra.as_deref(), rb.as_deref()) {
                    (Some(x), Some(y)) => typeref_equal(x, y),
                    (None, None) => true,
                    _ => false,
                }
                && ea.len() == eb.len()
                && ea.iter().zip(eb.iter()).all(|(x, y)| typeref_equal(x, y))
        }
        (TypeRef::Unit(_), TypeRef::Unit(_)) => true,
        _ => false,
    }
}

// ============================================================================
// D90 Plan 20 Ф.3: defer/errdefer body constraints
// ============================================================================
//
// Body запрещает три категории конструкций:
//
// 1. **Exit-control:** `return`, `throw`, `break`, `continue` нельзя
//    использовать в defer body — defer часть exit-процесса, не может
//    hijack его. Compile error: «defer body cannot use ... — это
//    нарушит exit семантику scope'а».
//
// 2. **Fail-effect:** `?`, `!!`, `throw` desugar'ятся в throw через
//    эффект Fail. Defer body должно быть infallible — double-throw
//    невозможно сделать корректно. Detection через AST-walk
//    (ExprKind::Throw, ExprKind::Try, ExprKind::Bang).
//
// 3. **Suspend operations:** Net.*, Fs.*, Db.*, Time.sleep,
//    Channel.recv (blocking), parallel for, spawn, supervised, select.
//    Defer должен быть быстрым cleanup — suspend делает exit-семантику
//    непредсказуемой. Detection: AST-форма (ParallelFor, Spawn,
//    Supervised) + callee.effects intersect с SUSPEND_EFFECTS списком.

/// Эффекты, которые считаются suspend в контексте defer body.
/// Это approximation для bootstrap — D90 spec говорит «cleanup быстрый»,
/// безопаснее запретить целую группу чем пытаться различить
/// blocking vs non-blocking варианты для каждого эффекта.
const SUSPEND_EFFECT_NAMES: &[&str] = &[
    "Net", "Fs", "Db", "Time",
];

/// AST-формы которые сами по себе считаются suspend (даже если effects
/// не объявлены).
fn is_suspend_expr_kind(kind: &ExprKind) -> bool {
    matches!(kind,
        ExprKind::ParallelFor { .. }
        | ExprKind::Spawn(_)
        | ExprKind::Supervised(_)
        | ExprKind::Detach(_)
        | ExprKind::CancelScope { .. }
    )
}

/// D90 Ф.8 (1): walk модуля, для каждого `HandlerLit { methods }`
/// проверяет, что methods обрабатывающие Never-operations завершаются
/// exit-control'ом.
///
/// Never-operation = operation, чей return type — `Never`. Handler-method
/// для такой operation не может завершиться normally (нет значения типа
/// Never). По D61 (стр. 1430-1434) body обязан `interrupt v`, `throw err`,
/// `panic(...)` или `exit(...)`.
///
/// Bootstrap-stage: знаем что built-in `Fail.fail(value) -> Never` —
/// единственная Never-operation в prelude. Hardcoded effect_name="Fail",
/// method_name="fail". User-defined effects с Never-methods будут покрыты
/// общей effect-schema-аналитикой (Plan 25+).
fn check_handler_never_ops(module: &Module, errors: &mut Vec<Diagnostic>) {
    // Сбор: какие user-defined effect-methods имеют return type Never.
    // Bootstrap: только Fail.fail — встроенный. User effects парсятся
    // через TypeDecl::Effect — анализируем их EffectMethod.return_type.
    let mut never_ops: HashSet<(String, String)> = HashSet::new();
    // Always-true: built-in Fail.fail.
    never_ops.insert(("Fail".to_string(), "fail".to_string()));
    // User-defined effects.
    for item in &module.items {
        if let Item::Type(td) = item {
            if let TypeDeclKind::Effect(methods) = &td.kind {
                for m in methods {
                    if let Some(rt) = &m.return_type {
                        if type_ref_is_never(rt) {
                            never_ops.insert((td.name.clone(), m.name.clone()));
                        }
                    }
                }
            }
        }
    }
    // Walk all expressions, найдём HandlerLit'ы.
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                if let FnBody::Block(b) = &f.body {
                    walk_block_for_handler_lits(b, &never_ops, errors);
                } else if let FnBody::Expr(e) = &f.body {
                    walk_expr_for_handler_lits(e, &never_ops, errors);
                }
            }
            Item::Test(t) => walk_block_for_handler_lits(&t.body, &never_ops, errors),
            _ => {}
        }
    }
}

/// Plan 33.3 Ф.9.6 (D24): handler verification gate.
///
/// Если эффект имеет хотя бы одну `pure_view` op'у, любое использование
/// handler'а через `with E = h` обязано декларировать verification
/// статус через `#verify_handler` или `#trusted_handler`. Без атрибута —
/// compile error.
///
/// Семантика:
/// - `#verify_handler` — symbolic verification handler.action body
///   против axiom'ов эффекта (Ф.9.7). Bootstrap V1: атрибут принимается
///   но реальной верификации нет — placeholder для Ф.9.7.
/// - `#trusted_handler` — программист берёт ответственность.
/// - Default (Unverified) для эффектов с pure_views — **error**.
///
/// Эффекты БЕЗ pure_views — никаких ограничений (default = Unverified
/// допустим).
///
/// Эта проверка консервативна: даже если body не вызывает pure_view-
/// using функции, gate всё равно требует attribute для эффекта с
/// pure_views. Это упрощает V1 (нет cross-fn analysis); Ф.9.7
/// уточнит до actually-uses analysis.
fn check_handler_verification_gate(module: &Module, errors: &mut Vec<Diagnostic>) {
    // Шаг 1: какие эффекты имеют axioms?
    // Refactor: gate срабатывает только при axiom-присутствии — pure_view сам по
    // себе ничего не утверждает, утверждение делает axiom. Без axiom handler
    // верифицировать не на что.
    let mut effects_with_axioms: HashSet<String> = HashSet::new();
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        if !matches!(&td.kind, TypeDeclKind::Effect(_)) { continue; }
        if !td.axioms.is_empty() {
            effects_with_axioms.insert(td.name.clone());
        }
    }
    if effects_with_axioms.is_empty() { return; }

    // Шаг 2: walk all expressions, найти WithBinding'и с такими эффектами.
    for item in &module.items {
        match item {
            Item::Fn(f) => match &f.body {
                FnBody::Block(b) => walk_block_for_with_gate(b, &effects_with_axioms, errors),
                FnBody::Expr(e) => walk_expr_for_with_gate(e, &effects_with_axioms, errors),
                FnBody::External => {}
            }
            Item::Test(t) => walk_block_for_with_gate(&t.body, &effects_with_axioms, errors),
            _ => {}
        }
    }
}

fn walk_block_for_with_gate(b: &Block, eff_pv: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Expr(e) => walk_expr_for_with_gate(e, eff_pv, errors),
            Stmt::Let(LetDecl { value, .. }) => walk_expr_for_with_gate(value, eff_pv, errors),
            Stmt::Assign { target, value, .. } => {
                walk_expr_for_with_gate(target, eff_pv, errors);
                walk_expr_for_with_gate(value, eff_pv, errors);
            }
            _ => {}
        }
    }
    if let Some(t) = &b.trailing { walk_expr_for_with_gate(t, eff_pv, errors); }
}

fn walk_expr_for_with_gate(e: &Expr, eff_pv: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    use crate::ast::ExprKind::*;
    match &e.kind {
        With { bindings, body } => {
            for b in bindings {
                let eff_name = match &b.effect {
                    TypeRef::Named { path, .. } => path.last().cloned().unwrap_or_default(),
                    _ => String::new(),
                };
                if !eff_pv.contains(&eff_name) { continue; }
                if matches!(b.verification, HandlerVerification::Unverified) {
                    errors.push(Diagnostic::new(
                        format!(
                            "handler for effect `{}` must be marked `#verify` \
                             or `#trusted` (effect has `axiom` declarations, so any \
                             handler must declare verification status). Examples:\n  \
                             with #trusted {0} = my_handler {{ ... }}\n  \
                             with #verify {0} = my_handler {{ ... }}",
                            eff_name,
                        ),
                        b.span,
                    ));
                }
                walk_expr_for_with_gate(&b.handler, eff_pv, errors);
            }
            walk_block_for_with_gate(body, eff_pv, errors);
        }
        Block(b) => walk_block_for_with_gate(b, eff_pv, errors),
        Call { func, args, .. } => {
            walk_expr_for_with_gate(func, eff_pv, errors);
            for a in args { walk_expr_for_with_gate(a.expr(), eff_pv, errors); }
        }
        Binary { left, right, .. } => {
            walk_expr_for_with_gate(left, eff_pv, errors);
            walk_expr_for_with_gate(right, eff_pv, errors);
        }
        Unary { operand, .. } => walk_expr_for_with_gate(operand, eff_pv, errors),
        Member { obj, .. } => walk_expr_for_with_gate(obj, eff_pv, errors),
        Index { obj, index } => {
            walk_expr_for_with_gate(obj, eff_pv, errors);
            walk_expr_for_with_gate(index, eff_pv, errors);
        }
        If { cond, then, else_ } => {
            walk_expr_for_with_gate(cond, eff_pv, errors);
            walk_block_for_with_gate(then, eff_pv, errors);
            match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => walk_block_for_with_gate(b, eff_pv, errors),
                Some(crate::ast::ElseBranch::If(ie)) => walk_expr_for_with_gate(ie, eff_pv, errors),
                None => {}
            }
        }
        _ => {}
    }
}

fn type_ref_is_never(t: &TypeRef) -> bool {
    if let TypeRef::Named { path, .. } = t {
        if let Some(last) = path.last() {
            return last == "Never";
        }
    }
    false
}

/// Plan 33.3 Ф.9 (D24): валидация axiom-формул внутри effect-блоков.
///
/// Контракт: внутри `axiom name(binders) => formula` разрешены только:
///   - литералы (int/bool/str/unit);
///   - идентификаторы из `binders`;
///   - вызовы pure_view-ops **того же эффекта**: `balance(id) >= 0`;
///   - стандартные бинарные/унарные/comparison/boolean операторы;
///   - `if/else` без stmts.
///
/// Запрещены:
///   - non-pure_view operations (`SetBalance(...)`);
///   - вызовы любых других fn (включая built-ins за пределами разрешённых
///     операторов);
///   - record/sum constructors, member access, method calls.
///
/// Эти ограничения нужны для чистой SMT-кодировки (`pure_view` → UF,
/// axiom → assert) в Ф.9.4. Если разрешить произвольный код — SMT
/// encoding теряет soundness.
fn check_effect_axioms(module: &Module, errors: &mut Vec<Diagnostic>) {
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        // Plan 33.3 Ф.9 (refactor): unique-name + axiom-formula checks
        // применяются и к effect, и к protocol (в обоих можно объявлять
        // #pure ops и axioms).
        let methods = match &td.kind {
            TypeDeclKind::Effect(m) | TypeDeclKind::Protocol(m) => m,
            _ => continue,
        };

        // Plan 33.3 (refactor): unique-name checks внутри effect/protocol.
        //
        // Перегрузка op разрешена — уникальность по (name + param_types).
        // Axioms уникальны по имени (overloading axioms не поддерживается).
        // Axiom name не может совпадать с именем любого op (независимо от
        // типов параметров) — они в одном logical namespace.
        fn type_key(ty: &TypeRef) -> String {
            match ty {
                TypeRef::Named { path, generics, .. } => {
                    let base = path.join(".");
                    if generics.is_empty() {
                        base
                    } else {
                        let a: Vec<_> = generics.iter().map(type_key).collect();
                        format!("{}[{}]", base, a.join(","))
                    }
                }
                TypeRef::Tuple(ts, _) => {
                    let a: Vec<_> = ts.iter().map(type_key).collect();
                    format!("({})", a.join(","))
                }
                TypeRef::Func { params, return_type, .. } => {
                    let ps: Vec<_> = params.iter().map(type_key).collect();
                    let ret = return_type.as_deref().map(type_key).unwrap_or_default();
                    format!("fn({})->{}", ps.join(","), ret)
                }
                TypeRef::Array(t, _) => format!("[]{}", type_key(t)),
                TypeRef::FixedArray(n, t, _) => format!("[{}]{}", n, type_key(t)),
                TypeRef::Unit(_) => "()".to_string(),
            }
        }
        fn op_sig(m: &EffectMethod) -> String {
            let types: Vec<String> = m.params.iter()
                .map(|p| type_key(&p.ty))
                .collect();
            format!("{}({})", m.name, types.join(","))
        }
        let mut op_sigs: HashSet<String> = HashSet::new();
        // op_names_only: все имена operations (для проверки axiom↔op коллизии).
        let mut op_names_only: HashSet<&String> = HashSet::new();
        for m in methods {
            op_names_only.insert(&m.name);
            let sig = op_sig(m);
            if !op_sigs.insert(sig) {
                errors.push(Diagnostic::new(
                    format!("effect `{}`: duplicate operation `{}` \
                             (same name and parameter types)",
                        td.name, m.name),
                    m.span,
                ));
            }
        }
        let mut axiom_names: HashSet<&String> = HashSet::new();
        for ax in &td.axioms {
            if !axiom_names.insert(&ax.name) {
                errors.push(Diagnostic::new(
                    format!("effect `{}`: duplicate axiom `{}`",
                        td.name, ax.name),
                    ax.span,
                ));
            }
            if op_names_only.contains(&ax.name) {
                errors.push(Diagnostic::new(
                    format!("effect `{}`: axiom `{}` conflicts with operation \
                             of the same name (axiom names must be distinct \
                             from operations / `#pure` views)",
                        td.name, ax.name),
                    ax.span,
                ));
            }
        }

        if td.axioms.is_empty() { continue; }

        // Собираем pure_view-имена эффекта: имя → ожидаемая арность.
        let mut pure_views: HashMap<String, usize> = HashMap::new();
        for m in methods {
            if matches!(m.kind, EffectOpKind::PureView) {
                pure_views.insert(m.name.clone(), m.params.len());
            }
        }

        for ax in &td.axioms {
            // Duplicate-binder check.
            let mut seen: HashSet<&String> = HashSet::new();
            for (b, _ty) in &ax.binders {
                if !seen.insert(b) {
                    errors.push(Diagnostic::new(
                        format!("axiom `{}.{}`: duplicate binder `{}`",
                            td.name, ax.name, b),
                        ax.span,
                    ));
                }
            }
            let binders: HashSet<&String> = ax.binders.iter().map(|(n, _)| n).collect();
            check_axiom_expr(&ax.formula, &td.name, &ax.name,
                             &binders, &pure_views, errors);
        }
    }
}

/// Walk `expr` в axiom-formula и пушит ошибки на запрещённые конструкции.
fn check_axiom_expr(
    e: &Expr,
    effect_name: &str,
    axiom_name: &str,
    binders: &HashSet<&String>,
    pure_views: &HashMap<String, usize>,
    errors: &mut Vec<Diagnostic>,
) {
    use crate::ast::ExprKind::*;
    match &e.kind {
        IntLit(_) | BoolLit(_) | StrLit(_) | CharLit(_) | UnitLit => {}
        Ident(n) => {
            if binders.contains(&n.to_string()) { return; }
            if pure_views.contains_key(n) {
                // Reference to pure_view без вызова — V1 запрещаем
                // (требуем `name(args)`-форму для arity-clarity).
                errors.push(Diagnostic::new(
                    format!(
                        "axiom `{}.{}`: pure_view `{}` must be called \
                         with arguments (e.g. `{}(...)`), not used as value",
                        effect_name, axiom_name, n, n,
                    ),
                    e.span,
                ));
                return;
            }
            errors.push(Diagnostic::new(
                format!(
                    "axiom `{}.{}`: unknown identifier `{}` (axiom-body \
                     may only reference binders {:?} or pure_view ops \
                     of effect `{}`)",
                    effect_name, axiom_name, n,
                    binders.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                    effect_name,
                ),
                e.span,
            ));
        }
        Binary { left, right, .. } => {
            check_axiom_expr(left, effect_name, axiom_name, binders, pure_views, errors);
            check_axiom_expr(right, effect_name, axiom_name, binders, pure_views, errors);
        }
        Unary { operand, .. } => {
            check_axiom_expr(operand, effect_name, axiom_name, binders, pure_views, errors);
        }
        If { cond, then, else_ } => {
            check_axiom_expr(cond, effect_name, axiom_name, binders, pure_views, errors);
            if !then.stmts.is_empty() {
                errors.push(Diagnostic::new(
                    format!("axiom `{}.{}`: if-branch must not contain statements",
                        effect_name, axiom_name),
                    e.span,
                ));
            }
            if let Some(trailing) = &then.trailing {
                check_axiom_expr(trailing, effect_name, axiom_name, binders, pure_views, errors);
            }
            match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => {
                    if !b.stmts.is_empty() {
                        errors.push(Diagnostic::new(
                            format!("axiom `{}.{}`: else-branch must not contain statements",
                                effect_name, axiom_name),
                            e.span,
                        ));
                    }
                    if let Some(t) = &b.trailing {
                        check_axiom_expr(t, effect_name, axiom_name, binders, pure_views, errors);
                    }
                }
                Some(crate::ast::ElseBranch::If(ie)) => {
                    check_axiom_expr(ie, effect_name, axiom_name, binders, pure_views, errors);
                }
                None => {}
            }
        }
        Call { func, args, trailing } => {
            if trailing.is_some() {
                errors.push(Diagnostic::new(
                    format!(
                        "axiom `{}.{}`: trailing blocks not allowed in axiom-formulas",
                        effect_name, axiom_name,
                    ),
                    e.span,
                ));
                return;
            }
            // V1: разрешена форма `<pure_view_name>(args)`.
            let pv_name = match &func.kind {
                Ident(n) => n.clone(),
                _ => {
                    errors.push(Diagnostic::new(
                        format!(
                            "axiom `{}.{}`: callee must be a pure_view of effect `{}`",
                            effect_name, axiom_name, effect_name,
                        ),
                        e.span,
                    ));
                    return;
                }
            };
            let Some(&expected) = pure_views.get(&pv_name) else {
                errors.push(Diagnostic::new(
                    format!(
                        "axiom `{}.{}`: `{}` is not a pure_view of effect `{}` \
                         (axioms may only reference pure_view ops)",
                        effect_name, axiom_name, pv_name, effect_name,
                    ),
                    e.span,
                ));
                return;
            };
            if args.len() != expected {
                errors.push(Diagnostic::new(
                    format!(
                        "axiom `{}.{}`: pure_view `{}` expects {} arg(s), got {}",
                        effect_name, axiom_name, pv_name, expected, args.len(),
                    ),
                    e.span,
                ));
            }
            for a in args {
                check_axiom_expr(a.expr(), effect_name, axiom_name, binders, pure_views, errors);
            }
        }
        _ => {
            errors.push(Diagnostic::new(
                format!(
                    "axiom `{}.{}`: this expression form is not allowed inside \
                     axiom-formula (only literals, binders, pure_view calls, \
                     arith/bool ops, and if/else)",
                    effect_name, axiom_name,
                ),
                e.span,
            ));
        }
    }
}

/// Walk block recursively: ищет HandlerLit, проверяет never-ops.
fn walk_block_for_handler_lits(b: &Block, never_ops: &HashSet<(String, String)>, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Let(decl) => walk_expr_for_handler_lits(&decl.value, never_ops, errors),
            Stmt::Expr(e) => walk_expr_for_handler_lits(e, never_ops, errors),
            Stmt::Assign { target, value, .. } => {
                walk_expr_for_handler_lits(target, never_ops, errors);
                walk_expr_for_handler_lits(value, never_ops, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { walk_expr_for_handler_lits(v, never_ops, errors); }
            }
            Stmt::Throw { value, .. } => walk_expr_for_handler_lits(value, never_ops, errors),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                walk_expr_for_handler_lits(body, never_ops, errors);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => walk_expr_for_handler_lits(expr, never_ops, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
    if let Some(t) = &b.trailing { walk_expr_for_handler_lits(t, never_ops, errors); }
}

fn walk_expr_for_handler_lits(e: &Expr, never_ops: &HashSet<(String, String)>, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::HandlerLit { effect_name, methods } => {
            // effect_name — Vec<String>, последний компонент = effect's last name.
            let eff_last = effect_name.last().cloned().unwrap_or_default();
            for m in methods {
                let key = (eff_last.clone(), m.name.clone());
                if never_ops.contains(&key) {
                    if !handler_body_diverges(&m.body) {
                        errors.push(Diagnostic::new(
                            format!(
                                "handler-method `{}.{}` обрабатывает операцию с возвращаемым типом `Never` \
                                 (D61 §1430-1434, D65): body обязан завершиться через `interrupt v`, \
                                 `throw err`, `panic(...)` или `exit(...)`. Нельзя завершить handler-method \
                                 normally — нет значения типа `Never` для return.",
                                eff_last, m.name
                            ),
                            m.span,
                        ));
                    }
                }
            }
            // Также recurse в bodies handler-методов (могут содержать nested
            // HandlerLit).
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(ex) => walk_expr_for_handler_lits(ex, never_ops, errors),
                    HandlerMethodBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                }
            }
        }
        // Recurse в остальные expr-kinds (используем существующий walk
        // через ExprKind::Block + остальные expressions).
        ExprKind::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
        ExprKind::With { bindings, body } => {
            for bd in bindings { walk_expr_for_handler_lits(&bd.handler, never_ops, errors); }
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::If { cond, then, else_ } => {
            walk_expr_for_handler_lits(cond, never_ops, errors);
            walk_block_for_handler_lits(then, never_ops, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => walk_block_for_handler_lits(b, never_ops, errors),
                Some(ElseBranch::If(e2)) => walk_expr_for_handler_lits(e2, never_ops, errors),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr_for_handler_lits(scrutinee, never_ops, errors);
            walk_block_for_handler_lits(then, never_ops, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => walk_block_for_handler_lits(b, never_ops, errors),
                Some(ElseBranch::If(e2)) => walk_expr_for_handler_lits(e2, never_ops, errors),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_for_handler_lits(scrutinee, never_ops, errors);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(ex) => walk_expr_for_handler_lits(ex, never_ops, errors),
                    MatchArmBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                }
                if let Some(g) = &a.guard { walk_expr_for_handler_lits(g, never_ops, errors); }
            }
        }
        ExprKind::For { iter, body, .. } => {
            walk_expr_for_handler_lits(iter, never_ops, errors);
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::While { cond, body } | ExprKind::WhileLet { scrutinee: cond, body, .. } => {
            walk_expr_for_handler_lits(cond, never_ops, errors);
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::Loop { body } => walk_block_for_handler_lits(body, never_ops, errors),
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => walk_expr_for_handler_lits(chan, never_ops, errors),
                    SelectOp::Send { chan, value } => {
                        walk_expr_for_handler_lits(chan, never_ops, errors);
                        walk_expr_for_handler_lits(value, never_ops, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { walk_expr_for_handler_lits(g, never_ops, errors); }
                walk_block_for_handler_lits(&arm.body, never_ops, errors);
            }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::Supervised(b) | ExprKind::Detach(b) => walk_block_for_handler_lits(b, never_ops, errors),
        ExprKind::Spawn(ex) => walk_expr_for_handler_lits(ex, never_ops, errors),
        ExprKind::CancelScope { body, .. } => walk_block_for_handler_lits(body, never_ops, errors),
        ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr_for_handler_lits(iter, never_ops, errors);
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::Call { func, args, trailing } => {
            walk_expr_for_handler_lits(func, never_ops, errors);
            for a in args { walk_expr_for_handler_lits(a.expr(), never_ops, errors); }
            if let Some(tr) = trailing {
                match tr {
                    Trailing::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                    Trailing::Fn(fsb) => match &fsb.body {
                        FnBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                        FnBody::Expr(e2) => walk_expr_for_handler_lits(e2, never_ops, errors),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => walk_block_for_handler_lits(&tb.body, never_ops, errors),
                }
            }
        }
        ExprKind::Binary { left, right, .. } => {
            walk_expr_for_handler_lits(left, never_ops, errors);
            walk_expr_for_handler_lits(right, never_ops, errors);
        }
        ExprKind::Unary { operand, .. } => walk_expr_for_handler_lits(operand, never_ops, errors),
        ExprKind::Coalesce(a, b) => {
            walk_expr_for_handler_lits(a, never_ops, errors);
            walk_expr_for_handler_lits(b, never_ops, errors);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => walk_expr_for_handler_lits(e2, never_ops, errors),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } | ExprKind::TurboFish { base: obj, .. } => {
            walk_expr_for_handler_lits(obj, never_ops, errors);
        }
        ExprKind::Range { start, end, .. } => {
            walk_expr_for_handler_lits(start, never_ops, errors);
            walk_expr_for_handler_lits(end, never_ops, errors);
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => walk_expr_for_handler_lits(e2, never_ops, errors),
                }
            }
        }
        ExprKind::TupleLit(elems) => { for el in elems { walk_expr_for_handler_lits(el, never_ops, errors); } }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields { if let Some(v) = &f.value { walk_expr_for_handler_lits(v, never_ops, errors); } }
        }
        ExprKind::Throw(v) | ExprKind::Try(v) | ExprKind::Bang(v) | ExprKind::Interrupt(Some(v)) => {
            walk_expr_for_handler_lits(v, never_ops, errors);
        }
        ExprKind::Interrupt(None) => {}
        ExprKind::Lambda { body, .. } => walk_expr_for_handler_lits(body, never_ops, errors),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e2) => walk_expr_for_handler_lits(e2, never_ops, errors),
            ClosureBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
        },
        ExprKind::ClosureFull(fsb) => match &fsb.body {
            FnBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
            FnBody::Expr(e2) => walk_expr_for_handler_lits(e2, never_ops, errors),
            FnBody::External => {}
        },
        // Interpolated string — recurse в её parts (могут содержать expressions).
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e2) = p {
                    walk_expr_for_handler_lits(e2, never_ops, errors);
                }
            }
        }
        // TaggedTemplate имеет args со sub-expressions — но bootstrap-stage
        // редко используется; для completeness'а добавим shallow walk.
        ExprKind::TaggedTemplate { .. } => {}
        // Leaf expressions — nothing to recurse into.
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::CharLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::UnitLit
        | ExprKind::SelfAccess => {}
    }
}

/// Static analysis: завершается ли handler-method body через exit-control?
///
/// Exit-control = `interrupt`, `throw`, `panic(...)`, `exit(...)` —
/// expressions/stmts которые гарантированно НЕ возвращают control в
/// caller операции (Never-returning).
///
/// Bootstrap conservative: проверяем самые частые паттерны:
///   - Expr body = exit-control expression.
///   - Block body = последний stmt/trailing — exit-control.
///   - Conditional structures (if/match) — ВСЕ ветки exit-control.
///
/// Если не уверены — возвращаем `false` (нечасто-используемый граничный
/// случай → программист обязан явно exit'нуть).
fn handler_body_diverges(body: &HandlerMethodBody) -> bool {
    match body {
        HandlerMethodBody::Expr(e) => expr_diverges(e),
        HandlerMethodBody::Block(b) => block_diverges(b),
    }
}

fn expr_diverges(e: &Expr) -> bool {
    match &e.kind {
        // Direct exit-control.
        ExprKind::Interrupt(_) | ExprKind::Throw(_) => true,
        // panic(...) / exit(...) — Never-returning builtins (D13).
        ExprKind::Call { func, .. } => {
            if let ExprKind::Ident(name) = &func.kind {
                matches!(name.as_str(), "panic" | "exit")
            } else {
                false
            }
        }
        // Conditional: все ветки должны diverge.
        ExprKind::If { then, else_, .. } => {
            block_diverges(then)
                && match else_ {
                    Some(ElseBranch::Block(b)) => block_diverges(b),
                    Some(ElseBranch::If(e2)) => expr_diverges(e2),
                    None => false, // нет else — fall-through possible
                }
        }
        ExprKind::IfLet { then, else_, .. } => {
            block_diverges(then)
                && match else_ {
                    Some(ElseBranch::Block(b)) => block_diverges(b),
                    Some(ElseBranch::If(e2)) => expr_diverges(e2),
                    None => false,
                }
        }
        ExprKind::Match { arms, .. } => {
            !arms.is_empty()
                && arms.iter().all(|a| match &a.body {
                    MatchArmBody::Expr(ex) => expr_diverges(ex),
                    MatchArmBody::Block(b) => block_diverges(b),
                })
        }
        // Block-as-expr.
        ExprKind::Block(b) => block_diverges(b),
        // Loop без condition — diverges (если нет break).
        ExprKind::Loop { .. } => true,
        _ => false,
    }
}

fn block_diverges(b: &Block) -> bool {
    // Сначала проверим: есть ли в block.stmts unconditional throw/return/etc
    // на верхнем уровне? Это early-diverge.
    for s in &b.stmts {
        if stmt_diverges(s) {
            return true;
        }
    }
    // Иначе — проверка trailing expression.
    if let Some(t) = &b.trailing {
        return expr_diverges(t);
    }
    false
}

fn stmt_diverges(s: &Stmt) -> bool {
    match s {
        Stmt::Return { .. } | Stmt::Throw { .. } => true,
        Stmt::Expr(e) => expr_diverges(e),
        // Break/Continue exit'ят loop, не handler-fn — не diverge для
        // handler-purposes (handler body должен иметь exit к caller'у
        // операции, не к outer loop).
        Stmt::Break(_) | Stmt::Continue(_) => false,
        _ => false,
    }
}

/// Walk модуля: для каждого defer/errdefer statement в bodies функций
/// и тестах — проверить body constraints.
fn check_defer_bodies(module: &Module, errors: &mut Vec<Diagnostic>) {
    // Lookup callee effects: fn_name -> effects (для suspend detection).
    let mut fn_effects: HashMap<String, Vec<TypeRef>> = HashMap::new();
    for item in &module.items {
        if let Item::Fn(f) = item {
            let key = match &f.receiver {
                Some(r) => format!("{}.{}", r.type_name, f.name),
                None => f.name.clone(),
            };
            fn_effects.entry(key).or_default().extend(f.effects.iter().cloned());
        }
    }

    // Walk bodies функций и тестов.
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                if let FnBody::Block(b) = &f.body {
                    walk_block_for_defers(b, &fn_effects, errors);
                } else if let FnBody::Expr(e) = &f.body {
                    walk_expr_for_defers(e, &fn_effects, errors);
                }
            }
            Item::Test(t) => {
                walk_block_for_defers(&t.body, &fn_effects, errors);
            }
            _ => {}
        }
    }
}

/// Walk block: для каждого Stmt::Defer/ErrDefer — проверить body;
/// рекурсивно walk остальные stmts (там может быть вложенный block с
/// defer'ами).
fn walk_block_for_defers(b: &Block, fn_effects: &HashMap<String, Vec<TypeRef>>, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Defer { body, .. } => {
                check_defer_body(body, /*is_errdefer*/ false, fn_effects, errors);
            }
            Stmt::ErrDefer { body, .. } => {
                check_defer_body(body, /*is_errdefer*/ true, fn_effects, errors);
            }
            Stmt::Let(decl) => walk_expr_for_defers(&decl.value, fn_effects, errors),
            Stmt::Expr(e) => walk_expr_for_defers(e, fn_effects, errors),
            Stmt::Assign { target, value, .. } => {
                walk_expr_for_defers(target, fn_effects, errors);
                walk_expr_for_defers(value, fn_effects, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { walk_expr_for_defers(v, fn_effects, errors); }
            }
            Stmt::Throw { value, .. } => walk_expr_for_defers(value, fn_effects, errors),
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => walk_expr_for_defers(expr, fn_effects, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
    if let Some(t) = &b.trailing {
        walk_expr_for_defers(t, fn_effects, errors);
    }
}

/// Walk expression: рекурсивно ищем вложенные блоки с defer'ами.
/// Сам по себе expression не проверяется — только nested blocks.
fn walk_expr_for_defers(e: &Expr, fn_effects: &HashMap<String, Vec<TypeRef>>, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Block(b) => walk_block_for_defers(b, fn_effects, errors),
        ExprKind::If { cond, then, else_ } => {
            walk_expr_for_defers(cond, fn_effects, errors);
            walk_block_for_defers(then, fn_effects, errors);
            if let Some(ElseBranch::Block(b)) = else_ { walk_block_for_defers(b, fn_effects, errors); }
            if let Some(ElseBranch::If(e2)) = else_ { walk_expr_for_defers(e2, fn_effects, errors); }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr_for_defers(scrutinee, fn_effects, errors);
            walk_block_for_defers(then, fn_effects, errors);
            if let Some(ElseBranch::Block(b)) = else_ { walk_block_for_defers(b, fn_effects, errors); }
            if let Some(ElseBranch::If(e2)) = else_ { walk_expr_for_defers(e2, fn_effects, errors); }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_for_defers(scrutinee, fn_effects, errors);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(e2) => walk_expr_for_defers(e2, fn_effects, errors),
                    MatchArmBody::Block(b) => walk_block_for_defers(b, fn_effects, errors),
                }
                if let Some(g) = &a.guard { walk_expr_for_defers(g, fn_effects, errors); }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr_for_defers(iter, fn_effects, errors);
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::While { cond, body } => {
            walk_expr_for_defers(cond, fn_effects, errors);
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            walk_expr_for_defers(scrutinee, fn_effects, errors);
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::Loop { body } => walk_block_for_defers(body, fn_effects, errors),
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => walk_expr_for_defers(chan, fn_effects, errors),
                    SelectOp::Send { chan, value } => {
                        walk_expr_for_defers(chan, fn_effects, errors);
                        walk_expr_for_defers(value, fn_effects, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { walk_expr_for_defers(g, fn_effects, errors); }
                walk_block_for_defers(&arm.body, fn_effects, errors);
            }
        }
        ExprKind::With { body, .. } | ExprKind::Forbid { body, .. }
        | ExprKind::Realtime { body, .. } | ExprKind::Supervised(body)
        | ExprKind::Detach(body) | ExprKind::CancelScope { body, .. } => {
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::Call { func, args, trailing } => {
            walk_expr_for_defers(func, fn_effects, errors);
            for a in args {
                walk_expr_for_defers(a.expr(), fn_effects, errors);
            }
            if let Some(tr) = trailing {
                match tr {
                    Trailing::Block(b) => walk_block_for_defers(b, fn_effects, errors),
                    Trailing::Fn(fsb) => {
                        if let FnBody::Block(b) = &fsb.body { walk_block_for_defers(b, fn_effects, errors); }
                        else if let FnBody::Expr(e2) = &fsb.body { walk_expr_for_defers(e2, fn_effects, errors); }
                    }
                    Trailing::LegacyBlockWithParams(tb) => {
                        walk_block_for_defers(&tb.body, fn_effects, errors);
                    }
                }
            }
        }
        ExprKind::Spawn(body) => walk_expr_for_defers(body, fn_effects, errors),
        ExprKind::Binary { left, right, .. } => {
            walk_expr_for_defers(left, fn_effects, errors);
            walk_expr_for_defers(right, fn_effects, errors);
        }
        ExprKind::Unary { operand, .. } => walk_expr_for_defers(operand, fn_effects, errors),
        ExprKind::Try(e2) | ExprKind::Bang(e2) | ExprKind::Throw(e2) => {
            walk_expr_for_defers(e2, fn_effects, errors);
        }
        ExprKind::Coalesce(a, b) => {
            walk_expr_for_defers(a, fn_effects, errors);
            walk_expr_for_defers(b, fn_effects, errors);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => walk_expr_for_defers(e2, fn_effects, errors),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } => walk_expr_for_defers(obj, fn_effects, errors),
        ExprKind::TurboFish { base, .. } => walk_expr_for_defers(base, fn_effects, errors),
        ExprKind::Lambda { body, .. } | ExprKind::Interrupt(Some(body)) => walk_expr_for_defers(body, fn_effects, errors),
        ExprKind::Range { start, end, .. } => {
            walk_expr_for_defers(start, fn_effects, errors);
            walk_expr_for_defers(end, fn_effects, errors);
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => walk_expr_for_defers(e2, fn_effects, errors),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { walk_expr_for_defers(el, fn_effects, errors); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { walk_expr_for_defers(v, fn_effects, errors); }
            }
        }
        // Лямбды closure-full: body внутри FnSigBody.
        ExprKind::ClosureFull(fsb) => {
            if let FnBody::Block(b) = &fsb.body { walk_block_for_defers(b, fn_effects, errors); }
            else if let FnBody::Expr(e2) = &fsb.body { walk_expr_for_defers(e2, fn_effects, errors); }
        }
        ExprKind::ClosureLight { body, .. } => {
            match body {
                ClosureBody::Expr(e2) => walk_expr_for_defers(e2, fn_effects, errors),
                ClosureBody::Block(b) => walk_block_for_defers(b, fn_effects, errors),
            }
        }
        // Простые узлы без вложенных блоков.
        _ => {}
    }
}

/// Body constraint check: exit-control, Fail-effect, suspend.
fn check_defer_body(body: &Expr, is_errdefer: bool, fn_effects: &HashMap<String, Vec<TypeRef>>, errors: &mut Vec<Diagnostic>) {
    let kw = if is_errdefer { "errdefer" } else { "defer" };
    // D90 Plan 20 Ф.3 (revised): Вариант 3 — return/break/continue разрешены
    // только внутри nested loop/fn-literal в defer body (local control). На
    // top-level defer body они запрещены — нельзя hijack scope-exit
    // окружающей функции/цикла.
    //
    // Ctx tracks: loop-nesting depth (break/continue ok если >0), fn-literal
    // depth (return ok если >0).
    let ctx = DeferBodyCtx { loop_depth: 0, fn_depth: 0 };
    check_defer_body_inner(body, kw, fn_effects, &ctx, errors);
}

#[derive(Clone, Copy)]
struct DeferBodyCtx {
    /// Текущая глубина loop'ов (for/while/loop) внутри defer body. Если >0,
    /// `break`/`continue` локальны — разрешены.
    loop_depth: usize,
    /// Текущая глубина fn-литералов (closure/lambda) внутри defer body. Если
    /// >0, `return` локален — разрешён (relates только к ближайшему fn).
    fn_depth: usize,
}

fn check_defer_body_inner(e: &Expr, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
    // Сначала проверяем узел сам по себе.
    match &e.kind {
        // Exit-control: throw expression-form (D85 redirected via Fail).
        ExprKind::Throw(_) => {
            errors.push(Diagnostic::new(
                format!("`throw` is not allowed inside `{}` body (D90): defer body must be infallible — \
                         it cannot raise errors. If cleanup may fail, wrap with `with Fail = ...` handler.", kw),
                e.span,
            ));
        }
        // ? и !! desugar в throw → запрещены по той же причине (no Fail).
        ExprKind::Try(_) => {
            errors.push(Diagnostic::new(
                format!("`?` operator is not allowed inside `{}` body (D90): defer body must be infallible — \
                         `?` requires Fail effect.", kw),
                e.span,
            ));
        }
        ExprKind::Bang(_) => {
            errors.push(Diagnostic::new(
                format!("`!!` operator is not allowed inside `{}` body (D90): defer body must be infallible — \
                         `!!` requires Fail effect.", kw),
                e.span,
            ));
        }
        // Interrupt — досрочный exit with-блока, hijack'ит scope exit-семантику.
        ExprKind::Interrupt(_) => {
            errors.push(Diagnostic::new(
                format!("`interrupt` is not allowed inside `{}` body (D90): defer body cannot hijack scope exit.", kw),
                e.span,
            ));
        }
        // Suspend constructs by AST-form.
        ExprKind::Spawn(_) | ExprKind::Supervised(_) | ExprKind::Detach(_)
        | ExprKind::CancelScope { .. } | ExprKind::ParallelFor { .. } => {
            errors.push(Diagnostic::new(
                format!("suspend operation (`spawn`/`supervised`/`detach`/`cancel_scope`/`parallel for`) \
                         is not allowed inside `{}` body (D90): defer must be fast cleanup.", kw),
                e.span,
            ));
        }
        // Call с suspend-эффектами (callee.effects ∩ SUSPEND_EFFECT_NAMES).
        ExprKind::Call { func, .. } => {
            if let Some(callee_name) = call_target_name(func) {
                if let Some(effs) = fn_effects.get(&callee_name) {
                    for ef in effs {
                        if let TypeRef::Named { path, .. } = ef {
                            if let Some(name) = path.last() {
                                if SUSPEND_EFFECT_NAMES.contains(&name.as_str()) {
                                    errors.push(Diagnostic::new(
                                        format!("call to `{}` requires suspend-effect `{}`, not allowed inside `{}` body (D90): \
                                                 defer must be fast cleanup.",
                                                callee_name, name, kw),
                                        e.span,
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            // Also: built-in effect ops `Time.sleep`, `Net.get`, etc. —
            // обнаруживаются по member-path первого identifier'а.
            if let ExprKind::Member { obj, .. } = &func.kind {
                if let ExprKind::Ident(head) = &obj.kind {
                    if SUSPEND_EFFECT_NAMES.contains(&head.as_str()) {
                        errors.push(Diagnostic::new(
                            format!("operation `{}.{}` (effect `{}`) is not allowed inside `{}` body (D90): \
                                     defer must be fast cleanup.",
                                    head,
                                    match &func.kind { ExprKind::Member { name, .. } => name.as_str(), _ => "" },
                                    head, kw),
                            e.span,
                        ));
                    }
                }
            }
        }
        _ => {}
    }

    // Рекурсивно вглубь — вложенные scope (block, if, etc.) подчиняются тем же
    // ограничениям, т.к. они часть defer body.
    walk_defer_subexprs(e, kw, fn_effects, ctx, errors);
}

fn walk_defer_subexprs(e: &Expr, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Block(b) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
        ExprKind::If { cond, then, else_ } => {
            check_defer_body_inner(cond, kw, fn_effects, ctx, errors);
            check_defer_body_block(then, kw, fn_effects, ctx, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
                Some(ElseBranch::If(e2)) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, ctx, errors);
            check_defer_body_block(then, kw, fn_effects, ctx, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
                Some(ElseBranch::If(e2)) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, ctx, errors);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(e2) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
                    MatchArmBody::Block(b) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
                }
                if let Some(g) = &a.guard { check_defer_body_inner(g, kw, fn_effects, ctx, errors); }
            }
        }
        ExprKind::For { iter, body, .. } => {
            check_defer_body_inner(iter, kw, fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth };
            check_defer_body_block(body, kw, fn_effects, &inner, errors);
        }
        ExprKind::While { cond, body } => {
            check_defer_body_inner(cond, kw, fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth };
            check_defer_body_block(body, kw, fn_effects, &inner, errors);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth };
            check_defer_body_block(body, kw, fn_effects, &inner, errors);
        }
        ExprKind::Loop { body } => {
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth };
            check_defer_body_block(body, kw, fn_effects, &inner, errors);
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => check_defer_body_inner(chan, kw, fn_effects, ctx, errors),
                    SelectOp::Send { chan, value } => {
                        check_defer_body_inner(chan, kw, fn_effects, ctx, errors);
                        check_defer_body_inner(value, kw, fn_effects, ctx, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { check_defer_body_inner(g, kw, fn_effects, ctx, errors); }
                check_defer_body_block(&arm.body, kw, fn_effects, ctx, errors);
            }
        }
        ExprKind::With { body, .. } | ExprKind::Forbid { body, .. }
        | ExprKind::Realtime { body, .. } => {
            check_defer_body_block(body, kw, fn_effects, ctx, errors);
        }
        ExprKind::Call { func, args, trailing } => {
            check_defer_body_inner(func, kw, fn_effects, ctx, errors);
            for a in args { check_defer_body_inner(a.expr(), kw, fn_effects, ctx, errors); }
            if let Some(tr) = trailing {
                match tr {
                    Trailing::Block(b) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
                    Trailing::Fn(fsb) => {
                        // Trailing fn-literal `fn { ... }` — это лямбда; return
                        // внутри неё локален для лямбды, а не для defer body.
                        let inner = DeferBodyCtx { loop_depth: ctx.loop_depth, fn_depth: ctx.fn_depth + 1 };
                        if let FnBody::Block(b) = &fsb.body { check_defer_body_block(b, kw, fn_effects, &inner, errors); }
                        else if let FnBody::Expr(e2) = &fsb.body { check_defer_body_inner(e2, kw, fn_effects, &inner, errors); }
                    }
                    Trailing::LegacyBlockWithParams(tb) => {
                        let inner = DeferBodyCtx { loop_depth: ctx.loop_depth, fn_depth: ctx.fn_depth + 1 };
                        check_defer_body_block(&tb.body, kw, fn_effects, &inner, errors);
                    }
                }
            }
        }
        ExprKind::Binary { left, right, .. } => {
            check_defer_body_inner(left, kw, fn_effects, ctx, errors);
            check_defer_body_inner(right, kw, fn_effects, ctx, errors);
        }
        ExprKind::Unary { operand, .. } => check_defer_body_inner(operand, kw, fn_effects, ctx, errors),
        ExprKind::Coalesce(a, b) => {
            check_defer_body_inner(a, kw, fn_effects, ctx, errors);
            check_defer_body_inner(b, kw, fn_effects, ctx, errors);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } => check_defer_body_inner(obj, kw, fn_effects, ctx, errors),
        ExprKind::TurboFish { base, .. } => check_defer_body_inner(base, kw, fn_effects, ctx, errors),
        ExprKind::Range { start, end, .. } => {
            check_defer_body_inner(start, kw, fn_effects, ctx, errors);
            check_defer_body_inner(end, kw, fn_effects, ctx, errors);
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { check_defer_body_inner(el, kw, fn_effects, ctx, errors); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { check_defer_body_inner(v, kw, fn_effects, ctx, errors); }
            }
        }
        // Lambda/closure bodies — это отдельный scope для defer'а
        // (defer внутри lambda относится к scope lambda, не parent).
        // Не проверяем — это уже не defer body, а его callees, которые
        // могут быть call'аны откуда угодно. Лямбда сама **может** быть
        // call'нута асинхронно — но это не defer issue, это её caller's
        // concern.
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_) => {}
        // Suspend / Throw / Interrupt — уже flagged выше в check_defer_body_inner.
        _ => {}
    }
}

fn check_defer_body_block(b: &Block, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Return { span, value } => {
                // Вариант 3 (D90): return локален только внутри nested fn-литерала.
                if ctx.fn_depth == 0 {
                    errors.push(Diagnostic::new(
                        format!("`return` is not allowed at the top level of `{}` body (D90): defer body cannot hijack scope exit of the enclosing function. \
                                 (Local `return` inside nested `fn`/closure внутри defer body разрешён.)", kw),
                        *span,
                    ));
                }
                if let Some(v) = value {
                    check_defer_body_inner(v, kw, fn_effects, ctx, errors);
                }
            }
            Stmt::Break(span) => {
                if ctx.loop_depth == 0 {
                    errors.push(Diagnostic::new(
                        format!("`break` is not allowed at the top level of `{}` body (D90): defer body cannot hijack the enclosing loop. \
                                 (Local `break` inside nested loop разрешён.)", kw),
                        *span,
                    ));
                }
            }
            Stmt::Continue(span) => {
                if ctx.loop_depth == 0 {
                    errors.push(Diagnostic::new(
                        format!("`continue` is not allowed at the top level of `{}` body (D90): defer body cannot hijack the enclosing loop. \
                                 (Local `continue` inside nested loop разрешён.)", kw),
                        *span,
                    ));
                }
            }
            Stmt::Throw { span, .. } => {
                errors.push(Diagnostic::new(
                    format!("`throw` is not allowed inside `{}` body (D90): defer body must be infallible.", kw),
                    *span,
                ));
            }
            Stmt::Let(decl) => check_defer_body_inner(&decl.value, kw, fn_effects, ctx, errors),
            Stmt::Expr(e) => check_defer_body_inner(e, kw, fn_effects, ctx, errors),
            Stmt::Assign { target, value, .. } => {
                check_defer_body_inner(target, kw, fn_effects, ctx, errors);
                check_defer_body_inner(value, kw, fn_effects, ctx, errors);
            }
            // Nested defer/errdefer — это OK. Это новый scope (block),
            // defer'ы внутри регистрируются для этого внутреннего scope'а,
            // не для родительского. Их body тоже проверяется — но через
            // основной walk (check_defer_bodies проходит по всем bodies).
            Stmt::Defer { body, .. } => check_defer_body(body, false, fn_effects, errors),
            Stmt::ErrDefer { body, .. } => check_defer_body(body, true, fn_effects, errors),
            // Plan 33.2 Ф.8: assert_static в defer body — walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => check_defer_body_inner(expr, kw, fn_effects, ctx, errors),
        }
    }
    if let Some(t) = &b.trailing {
        check_defer_body_inner(t, kw, fn_effects, ctx, errors);
    }
}

/// Извлечь имя callee если выражение — call target (Ident или Type.method).
fn call_target_name(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(n) => Some(n.clone()),
        ExprKind::Path(parts) if parts.len() >= 2 => Some(parts.join(".")),
        ExprKind::Member { obj, name } => {
            if let ExprKind::Ident(head) = &obj.kind {
                Some(format!("{}.{}", head, name))
            } else {
                None
            }
        }
        _ => None,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Plan 33.1 Ф.2 (D24): ContractCtx — проверка базовых правил контрактов.
//
// Минимальный pass для 33.1. Полная type-проверка (контракт должен быть
// bool, result.value под guard'ом, и т.д.) — в Ф.3 вместе с SMT-кодировкой.
//
// Базовые правила (33.1):
// 1. `result` запрещён в `requires` (значения ещё нет).
// 2. `old(...)` запрещён в `requires` (нет «до»).
// 3. composition: вызов другой fn в контракте — error в 33.1 (Plan 33.2
//    разрешит для @pure функций).
// ──────────────────────────────────────────────────────────────────────────

/// Контекст контракт-проверок.
///
/// Plan 33.2 Ф.7: разрешает composition — вызов `#pure` функций
/// в контрактах. Non-`#pure` функции в контрактах — compile error.
struct ContractCtx {
    /// Имена всех top-level fn.
    fn_names: HashSet<String>,
    /// Имена fn объявленных `#pure` (через атрибут).
    /// Используются для разрешения composition в контрактах (33.2).
    pure_fn_names: HashSet<String>,
    /// Plan 33.3 Ф.9: pure_view-имя → (effect_name, arity).
    /// При вызове `balance(id)` в контракте определяем (а) что это
    /// pure_view, (б) к какому эффекту относится, (в) что эффект в
    /// сигнатуре enclosing fn.
    pure_views: HashMap<String, (String, usize)>,
}

impl ContractCtx {
    fn build(module: &Module) -> Self {
        let mut fn_names = HashSet::new();
        let mut pure_fn_names = HashSet::new();
        let mut pure_views: HashMap<String, (String, usize)> = HashMap::new();
        for item in &module.items {
            match item {
                Item::Fn(fd) => {
                    fn_names.insert(fd.name.clone());
                    if matches!(fd.purity, Purity::Pure) {
                        pure_fn_names.insert(fd.name.clone());
                    }
                }
                Item::Type(td) => {
                    if let TypeDeclKind::Effect(methods) = &td.kind {
                        for m in methods {
                            if matches!(m.kind, EffectOpKind::PureView) {
                                pure_views.insert(
                                    m.name.clone(),
                                    (td.name.clone(), m.params.len()),
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Self { fn_names, pure_fn_names, pure_views }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        for item in &module.items {
            if let Item::Fn(fd) = item {
                self.check_fn(fd, errors);
            }
        }
    }

    fn check_fn(&self, fd: &FnDecl, errors: &mut Vec<Diagnostic>) {
        // Plan 33.2 Ф.5: проверка modifies-frame.
        // Если объявлен `modifies`, все assignment'ы внутри body должны
        // быть покрыты frame-target'ами.
        if !fd.modifies.is_empty() {
            self.check_modifies_frame(fd, errors);
        }
        // Plan 33.1 Ф.4: контракты на Fail-функциях требуют ContractResult
        // + flow-аналитики для result.is_ok / result.value / result.error.
        // Это полная реализация — отложена до Ф.3 SMT integration вместе
        // с Z3-кодировкой ContractResult-datatype.
        // В 33.1 — explicit compile error чтобы избежать silent unsoundness.
        if !fd.contracts.is_empty() && Self::fn_has_fail(fd) {
            errors.push(Diagnostic::new(
                format!(
                    "contracts on `Fail`-returning functions not yet supported in Plan 33.1 \
                     (`{}` has `Fail` effect; ContractResult + flow-analysis for \
                     result.is_ok / result.value / result.error — Plan 33.1 Ф.3 / Ф.4 follow-up)",
                    fd.name
                ),
                fd.span,
            ));
            // Контракты не проверяем дальше — error уже выдан.
            return;
        }
        // Plan 33.3 Ф.9: множество имён эффектов из сигнатуры функции
        // (для разрешения pure_view-вызовов в контрактах).
        let fn_effects: HashSet<String> = fd.effects.iter()
            .filter_map(|tr| match tr {
                TypeRef::Named { path, .. } => path.last().cloned(),
                _ => None,
            })
            .collect();
        for contract in &fd.contracts {
            match contract.kind {
                ContractKind::Requires => {
                    self.check_requires_expr(&contract.expr, &fn_effects, &fd.name, errors);
                }
                ContractKind::Ensures => {
                    self.check_ensures_expr(&contract.expr, &fn_effects, &fd.name, errors);
                }
            }
        }
    }

    /// Plan 33.2 Ф.5: проверка `modifies`-frame.
    /// Walks body, для каждого Stmt::Assign к **non-local** target'у
    /// (параметр / self / поле) проверяет что target покрыт frame-target'ом.
    ///
    /// Локальные `let mut` НЕ требуют frame-cover'а — `modifies` относится
    /// к **API-visible** mutations (параметры, self.fields). Это паритет с
    /// Dafny: «modifies clause is about heap effect, not stack locals».
    fn check_modifies_frame(&self, fd: &FnDecl, errors: &mut Vec<Diagnostic>) {
        let block = match &fd.body {
            FnBody::Block(b) => b,
            FnBody::Expr(_) | FnBody::External => return, // no assigns possible
        };
        // Collect local-binding names (let / let mut в block).
        let mut locals: std::collections::HashSet<String> = std::collections::HashSet::new();
        for stmt in &block.stmts {
            if let Stmt::Let(LetDecl { pattern, .. }) = stmt {
                Self::collect_binding_names(pattern, &mut locals);
            }
        }
        for stmt in &block.stmts {
            if let Stmt::Assign { target, span, .. } = stmt {
                // Skip locals.
                if let Some(root_name) = Self::root_lvalue_name(target) {
                    if locals.contains(&root_name) { continue; }
                }
                if !Self::is_assign_covered(target, &fd.modifies) {
                    errors.push(Diagnostic::new(
                        format!(
                            "assignment to `{}` is not covered by `modifies` clause of `{}`",
                            Self::expr_display(target), fd.name
                        ),
                        *span,
                    ));
                }
            }
        }
    }

    fn collect_binding_names(p: &Pattern, out: &mut std::collections::HashSet<String>) {
        match p {
            Pattern::Ident { name, .. } => { out.insert(name.clone()); }
            Pattern::Binding { name, inner, .. } => {
                out.insert(name.clone());
                Self::collect_binding_names(inner, out);
            }
            Pattern::Tuple(ps, _) => for sub in ps { Self::collect_binding_names(sub, out); }
            Pattern::Record { fields, .. } => for f in fields {
                if let Some(sub) = &f.pattern { Self::collect_binding_names(sub, out); }
                else { out.insert(f.name.clone()); }
            },
            Pattern::Array { elems, .. } => for e in elems {
                match e {
                    ArrayPatternElem::Item(pp) => Self::collect_binding_names(pp, out),
                    ArrayPatternElem::RestBind(n) => { out.insert(n.clone()); }
                    ArrayPatternElem::Rest => {}
                }
            },
            _ => {}
        }
    }

    fn root_lvalue_name(e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident(n) => Some(n.clone()),
            ExprKind::Member { obj, .. } => Self::root_lvalue_name(obj),
            ExprKind::Index { obj, .. } => Self::root_lvalue_name(obj),
            _ => None,
        }
    }

    /// Проверка: один target покрыт `modifies`-list'ом.
    fn is_assign_covered(target: &Expr, frame: &[FrameTarget]) -> bool {
        for ft in frame {
            if Self::frame_covers(ft, target) {
                return true;
            }
        }
        false
    }

    fn frame_covers(ft: &FrameTarget, target: &Expr) -> bool {
        match ft {
            FrameTarget::Whole(e) => Self::same_lvalue(e, target),
            FrameTarget::Field { receiver, field, .. } => {
                if let ExprKind::Member { obj, name } = &target.kind {
                    name == field && Self::same_lvalue(receiver, obj)
                } else {
                    false
                }
            }
            FrameTarget::ArrayElem { array, index, .. } => {
                if let ExprKind::Index { obj, index: tidx } = &target.kind {
                    Self::same_lvalue(array, obj) && Self::same_lvalue(index, tidx)
                } else {
                    false
                }
            }
            FrameTarget::ArrayAll { array, .. } => {
                if let ExprKind::Index { obj, .. } = &target.kind {
                    Self::same_lvalue(array, obj)
                } else {
                    false
                }
            }
        }
    }

    /// Простой сравнитель l-value (без полного structural equality).
    fn same_lvalue(a: &Expr, b: &Expr) -> bool {
        match (&a.kind, &b.kind) {
            (ExprKind::Ident(n1), ExprKind::Ident(n2)) => n1 == n2,
            (ExprKind::SelfAccess, ExprKind::SelfAccess) => true,
            (ExprKind::Member { obj: o1, name: n1 }, ExprKind::Member { obj: o2, name: n2 }) => {
                n1 == n2 && Self::same_lvalue(o1, o2)
            }
            _ => false,
        }
    }

    fn expr_display(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Ident(n) => n.clone(),
            ExprKind::SelfAccess => "self".into(),
            ExprKind::Member { obj, name } => format!("{}.{}", Self::expr_display(obj), name),
            ExprKind::Index { obj, .. } => format!("{}[..]", Self::expr_display(obj)),
            _ => "<expr>".into(),
        }
    }

    /// Проверка: функция объявляет `Fail` (любой вариант) в effects.
    fn fn_has_fail(fd: &FnDecl) -> bool {
        fd.effects.iter().any(|e| {
            matches!(e, TypeRef::Named { path, .. }
                if !path.is_empty() && path.last().map(|s| s.as_str()) == Some("Fail"))
        })
    }

    /// `requires`: запрещены `result` и `old(...)`.
    fn check_requires_expr(
        &self,
        e: &Expr,
        fn_effects: &HashSet<String>,
        fn_name: &str,
        errors: &mut Vec<Diagnostic>,
    ) {
        self.walk_expr(e, fn_effects, fn_name, errors, /*in_ensures*/ false);
    }

    /// `ensures`: `result`/`old(...)` разрешены; composition запрещён в 33.1.
    fn check_ensures_expr(
        &self,
        e: &Expr,
        fn_effects: &HashSet<String>,
        fn_name: &str,
        errors: &mut Vec<Diagnostic>,
    ) {
        self.walk_expr(e, fn_effects, fn_name, errors, /*in_ensures*/ true);
    }

    fn walk_expr(
        &self,
        e: &Expr,
        fn_effects: &HashSet<String>,
        fn_name: &str,
        errors: &mut Vec<Diagnostic>,
        in_ensures: bool,
    ) {
        match &e.kind {
            ExprKind::Ident(n) => {
                if n == "result" && !in_ensures {
                    errors.push(Diagnostic::new(
                        "`result` is not available in `requires` (only in `ensures`)",
                        e.span,
                    ));
                }
            }
            ExprKind::Call { func, args, .. } => {
                // Detect `old(...)` — special-cased call.
                if let ExprKind::Ident(name) = &func.kind {
                    if name == "old" {
                        if !in_ensures {
                            errors.push(Diagnostic::new(
                                "`old(...)` is not available in `requires` (only in `ensures`)",
                                e.span,
                            ));
                        }
                        // Walk old() arg ONCE; it's a snapshot of pre-state,
                        // not a composition.
                        for a in args {
                            self.walk_expr(a.expr(), fn_effects, fn_name, errors, in_ensures);
                        }
                        return;
                    }
                    // Plan 33.3 Ф.9.3 part 2: pure_view-вызов в контракте
                    // разрешён только если соответствующий эффект объявлен в
                    // сигнатуре enclosing fn (`(...) Eff -> ...`). pure_view
                    // — read-only observation, нужен effect-handler в scope.
                    if let Some((effect_name, expected_arity)) = self.pure_views.get(name) {
                        if !fn_effects.contains(effect_name) {
                            errors.push(Diagnostic::new(
                                format!(
                                    "pure_view `{}.{}` referenced in contract of `{}`, \
                                     but effect `{}` is not in this function's signature \
                                     (add `{}` to effects)",
                                    effect_name, name, fn_name, effect_name, effect_name,
                                ),
                                e.span,
                            ));
                        }
                        if args.len() != *expected_arity {
                            errors.push(Diagnostic::new(
                                format!(
                                    "pure_view `{}.{}` expects {} arg(s), got {}",
                                    effect_name, name, expected_arity, args.len(),
                                ),
                                e.span,
                            ));
                        }
                        // pure_view-вызов разрешён; walk args, не walk
                        // callee (это identifier-name pure_view, не fn).
                        for a in args {
                            self.walk_expr(a.expr(), fn_effects, fn_name, errors, in_ensures);
                        }
                        return;
                    }
                    // Plan 33.2 Ф.7 composition: вызов другой fn в контракте
                    // разрешён ТОЛЬКО если она `#pure`.
                    if self.fn_names.contains(name) && !self.pure_fn_names.contains(name) {
                        errors.push(Diagnostic::new(
                            format!(
                                "calling user function `{}` in contracts requires `#pure` attribute \
                                 (Plan 33.2 composition: only #pure functions allowed)",
                                name
                            ),
                            e.span,
                        ));
                    }
                }
                // Walk callee + args.
                self.walk_expr(func, fn_effects, fn_name, errors, in_ensures);
                for a in args {
                    self.walk_expr(a.expr(), fn_effects, fn_name, errors, in_ensures);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, fn_effects, fn_name, errors, in_ensures);
                self.walk_expr(right, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Unary { operand, .. } => {
                self.walk_expr(operand, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Member { obj, .. } => {
                self.walk_expr(obj, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, fn_effects, fn_name, errors, in_ensures);
                self.walk_expr(index, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::As(inner, _) | ExprKind::Is(inner, _) => {
                self.walk_expr(inner, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Try(inner) | ExprKind::Bang(inner) => {
                self.walk_expr(inner, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Coalesce(l, r) => {
                self.walk_expr(l, fn_effects, fn_name, errors, in_ensures);
                self.walk_expr(r, fn_effects, fn_name, errors, in_ensures);
            }
            // Литералы, paths, и прочее — не интересно для базовых правил.
            _ => {}
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Plan 33.3 Ф.9.7 (D24): ghost-var usage check.
//
// Verus/Dafny semantics: ghost binding (`ghost let x = ...`) — spec-only,
// не emit'ится в runtime. Non-ghost код не может читать ghost-var.
// До этого: catch'илось C-compiler'ом как «undeclared identifier» (ghost
// эрейзится в codegen). Теперь — proper compile-error на type-check этапе
// с понятным сообщением.
//
// Эвристика: walk каждый fn body, в каждом block:
// 1. Собираем `ghost let` имена в scope.
// 2. Walk остальные stmt'ы (non-ghost) и trailing — если ident ссылается
//    на ghost-name → error.
//
// Ограничения bootstrap:
// - Не учитываем `requires`/`ensures` (ghost OK там — но walk их не
//   делаем, и не должны catches as «non-ghost»).
// - Nested blocks: ghost из outer scope виден inner non-ghost — это
//   ошибка (по Verus); ловим через accumulating ghost-set.
// - Pattern bindings: только Ident-pattern (простой случай).
// ──────────────────────────────────────────────────────────────────────────

fn check_ghost_usage(module: &Module, errors: &mut Vec<Diagnostic>) {
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if let FnBody::Block(b) = &fd.body {
                let ghosts: HashSet<String> = HashSet::new();
                check_ghost_in_block(b, &ghosts, errors);
            } else if let FnBody::Expr(e) = &fd.body {
                let ghosts: HashSet<String> = HashSet::new();
                check_ghost_in_expr(e, &ghosts, errors);
            }
        } else if let Item::Test(t) = item {
            let ghosts: HashSet<String> = HashSet::new();
            check_ghost_in_block(&t.body, &ghosts, errors);
        }
    }
}

fn check_ghost_in_block(b: &Block, parent_ghosts: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    // Local ghost-set начинаем с parent + добавляем ghost-let'ы из этого
    // block'а в порядке появления.
    let mut ghosts = parent_ghosts.clone();
    for stmt in &b.stmts {
        if let Stmt::Let(decl) = stmt {
            if decl.is_ghost {
                // Ghost let value-expr может читать другие ghost-vars
                // — это OK. Не проверяем walk_expr на value.
                if let Pattern::Ident { name, .. } = &decl.pattern {
                    ghosts.insert(name.clone());
                }
                continue;
            }
        }
        // Non-ghost stmt: walk expr и проверяем что не читает ghost.
        check_ghost_in_stmt(stmt, &ghosts, errors);
    }
    if let Some(t) = &b.trailing {
        check_ghost_in_expr(t, &ghosts, errors);
    }
}

fn check_ghost_in_stmt(s: &Stmt, ghosts: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    match s {
        Stmt::Let(decl) => {
            // Non-ghost let: value не должен использовать ghost-vars.
            check_ghost_in_expr(&decl.value, ghosts, errors);
        }
        Stmt::Expr(e) => check_ghost_in_expr(e, ghosts, errors),
        Stmt::Assign { target, value, .. } => {
            check_ghost_in_expr(target, ghosts, errors);
            check_ghost_in_expr(value, ghosts, errors);
        }
        Stmt::Return { value: Some(v), .. } => check_ghost_in_expr(v, ghosts, errors),
        Stmt::Throw { value, .. } => check_ghost_in_expr(value, ghosts, errors),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => check_ghost_in_expr(body, ghosts, errors),
        // assert_static/assume — это spec-уровень, ghost-vars там OK.
        // Skip walk через них чтобы не выдавать false-positives.
        Stmt::AssertStatic { .. } | Stmt::Assume { .. } => {}
        _ => {}
    }
}

fn check_ghost_in_expr(e: &Expr, ghosts: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Ident(n) => {
            if ghosts.contains(n) {
                errors.push(Diagnostic::new(
                    format!(
                        "ghost variable `{}` cannot be read in non-ghost code \
                         (Plan 33.3 Ф.9.1: ghost vars are spec-only, Verus/Dafny semantics). \
                         Move usage into a contract clause (`requires`/`ensures`/`invariant`) \
                         or another `ghost let` binding.",
                        n
                    ),
                    e.span,
                ));
            }
        }
        ExprKind::Binary { left, right, .. } => {
            check_ghost_in_expr(left, ghosts, errors);
            check_ghost_in_expr(right, ghosts, errors);
        }
        ExprKind::Unary { operand, .. } => check_ghost_in_expr(operand, ghosts, errors),
        ExprKind::Member { obj, .. } => check_ghost_in_expr(obj, ghosts, errors),
        ExprKind::Index { obj, index } => {
            check_ghost_in_expr(obj, ghosts, errors);
            check_ghost_in_expr(index, ghosts, errors);
        }
        ExprKind::Call { func, args, .. } => {
            check_ghost_in_expr(func, ghosts, errors);
            for a in args { check_ghost_in_expr(a.expr(), ghosts, errors); }
        }
        ExprKind::If { cond, then, else_ } => {
            check_ghost_in_expr(cond, ghosts, errors);
            check_ghost_in_block(then, ghosts, errors);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => check_ghost_in_block(b, ghosts, errors),
                    ElseBranch::If(e) => check_ghost_in_expr(e, ghosts, errors),
                }
            }
        }
        ExprKind::Block(b) => check_ghost_in_block(b, ghosts, errors),
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) | ExprKind::Try(inner) | ExprKind::Bang(inner) => {
            check_ghost_in_expr(inner, ghosts, errors);
        }
        ExprKind::Coalesce(l, r) => {
            check_ghost_in_expr(l, ghosts, errors);
            check_ghost_in_expr(r, ghosts, errors);
        }
        _ => {}
    }
}
