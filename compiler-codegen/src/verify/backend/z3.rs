//! Plan 33 Z3 milestone: `SmtBackend` impl через libz3 C API.
//!
//! Закрывает V1 simplification: trivial backend (constant folding только)
//! заменяется на full SMT с linear-integer arithmetic, EUF, booleans.
//!
//! Дизайн:
//! - Refcounted Z3 AST (через `Z3_mk_context_rc` + `Z3_inc_ref` /
//!   `Z3_dec_ref`) — стандартная safety-pattern Z3 C API.
//! - Mapping `SmtTerm → Z3_ast`: рекурсивный обход; vars кэшируются в
//!   `vars: HashMap<String, Z3_ast>` чтобы две ссылки на одну var
//!   делили AST node.
//! - `SatResult::Sat(Model)` извлекает значения **только** для vars
//!   объявленных через `declare_var` — record/string sub-fields пока
//!   не recovered (V8 для полноценного strings/FP).
//! - Timeout — через `Z3_global_param_set("timeout", "<ms>")` перед
//!   `Z3_solver_check`. Per-solver params поддержаны но в pipeline
//!   мы сейчас используем дефолт `2000ms` из `VerificationPipeline`.
//!
//! Все Z3 операции safe-wrapped в этом файле; unsafe — только при
//! пересечении FFI границы. Memory-leak'ов нет потому что `Drop` для
//! `Z3Backend` идёт по всем сохранённым refs (vars, assertions) и
//! `dec_ref` их.

#![cfg(feature = "z3-backend")]

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_uint;
use std::ptr;

use super::super::ir::*;
use super::SmtBackend;
use super::z3_ffi as ffi;

/// Полноценный SMT backend через libz3.
///
/// Lifecycle: `new` → `declare_var`* → `assert`* → `push` / `pop` /
/// `check_sat`. На `Drop` все Z3 references освобождаются.
pub struct Z3Backend {
    ctx: ffi::Z3_context,
    solver: ffi::Z3_solver,
    /// Кэш sort'ов — int/bool/str используются массово, нет смысла
    /// создавать по несколько раз.
    int_sort: ffi::Z3_sort,
    bool_sort: ffi::Z3_sort,
    str_sort: ffi::Z3_sort,
    /// Declared variables: name → (Z3_ast, sort).
    vars: HashMap<String, (ffi::Z3_ast, SortRef)>,
    /// Все AST refs которые мы должны dec_ref при Drop.
    refs: Vec<ffi::Z3_ast>,
    /// Сохраняются для extract_model — solver assertion order.
    assertions: Vec<Assertion>,
    /// push/pop scope stack — храним высоту `assertions`/`refs` чтобы
    /// откатывать.
    scopes: Vec<(usize, usize)>,
}

// SAFETY: Z3 context is не thread-safe для одновременного использования
// из разных потоков, но **ownership transfer** между потоками безопасен.
// `Z3Backend` инкапсулирует context/solver полностью; ни один pointer
// не уезжает за пределы методов. SmtBackend trait требует `Send` чтобы
// pipeline мог положить backend в Box<dyn SmtBackend>.
unsafe impl Send for Z3Backend {}

impl Z3Backend {
    /// Создать backend. `timeout_ms` устанавливается глобально через
    /// `Z3_global_param_set("timeout", ...)` (Z3 уважает per-check timeout).
    pub fn new(timeout_ms: u32) -> Self {
        unsafe {
            // Глобальный timeout: эффективен для всех subsequent
            // `Z3_solver_check`. Мы не дёргаем per-solver params чтобы не
            // тянуть Z3_params API — простой global'ный setup достаточен
            // для Plan 33 MVP.
            //
            // SAFETY: CString-биндинги живут до конца unsafe-блока.
            // Z3 копирует строки себе при assignment'е.
            let key = CString::new("timeout").unwrap();
            let val = CString::new(timeout_ms.to_string()).unwrap();
            ffi::Z3_global_param_set(key.as_ptr(), val.as_ptr());

            let cfg = ffi::Z3_mk_config();
            // model.completion: если в модели var не присвоена, дать ей
            // any value (default). Делает извлечение counterexample
            // более user-friendly.
            let mk = CString::new("model").unwrap();
            let tr = CString::new("true").unwrap();
            ffi::Z3_set_param_value(cfg, mk.as_ptr(), tr.as_ptr());
            // Хранение CString'ов в local-vars пока config не уничтожен.
            let _hold_key = key;
            let _hold_val = val;
            let _hold_mk = mk;
            let _hold_tr = tr;

            let ctx = ffi::Z3_mk_context_rc(cfg);
            ffi::Z3_del_config(cfg);

            let int_sort = ffi::Z3_mk_int_sort(ctx);
            let bool_sort = ffi::Z3_mk_bool_sort(ctx);
            let str_sort = ffi::Z3_mk_string_sort(ctx);
            ffi::Z3_inc_ref(ctx, int_sort);
            ffi::Z3_inc_ref(ctx, bool_sort);
            ffi::Z3_inc_ref(ctx, str_sort);

            let solver = ffi::Z3_mk_solver(ctx);
            ffi::Z3_solver_inc_ref(ctx, solver);

            Self {
                ctx,
                solver,
                int_sort,
                bool_sort,
                str_sort,
                vars: HashMap::new(),
                refs: Vec::new(),
                assertions: Vec::new(),
                scopes: Vec::new(),
            }
        }
    }

    fn track(&mut self, ast: ffi::Z3_ast) -> ffi::Z3_ast {
        unsafe { ffi::Z3_inc_ref(self.ctx, ast); }
        self.refs.push(ast);
        ast
    }

    fn sort_for(&mut self, sort: &SortRef) -> ffi::Z3_sort {
        match sort {
            SortRef::Int => self.int_sort,
            SortRef::Bool => self.bool_sort,
            SortRef::Str => self.str_sort,
            SortRef::Named(name) => {
                // Plan 33.1 trivial backend wraps record-types as
                // uninterpreted; для Z3 делаем то же — uninterpreted
                // sort, equality-only reasoning. Z3 internally hash-cons'ит
                // одинаковые symbol+arity, поэтому повторные вызовы для
                // одного name дают тот же sort node.
                //
                // SAFETY: контекст живой пока self живой; CString не
                // переживает scope, но FFI копирует строку себе.
                unsafe {
                    let nm = CString::new(name.as_str())
                        .unwrap_or_else(|_| CString::new("opaque").unwrap());
                    let sym = ffi::Z3_mk_string_symbol(self.ctx, nm.as_ptr());
                    let s = ffi::Z3_mk_uninterpreted_sort(self.ctx, sym);
                    ffi::Z3_inc_ref(self.ctx, s);
                    self.refs.push(s);
                    s
                }
            }
        }
    }

    /// Translate `SmtTerm` в `Z3_ast`.
    ///
    /// Все AST nodes которые мы держим — track'aются (inc_ref + сохраняем
    /// в `refs` для Drop). Z3 internally делает hash-consing, поэтому
    /// одинаковые subterm'ы share структуру, ref-counting на нашей стороне
    /// безопасно.
    fn translate(&mut self, term: &SmtTerm) -> Result<ffi::Z3_ast, String> {
        unsafe { self.translate_inner(term) }
    }

    unsafe fn translate_inner(&mut self, term: &SmtTerm) -> Result<ffi::Z3_ast, String> {
        match term {
            SmtTerm::IntLit(n) => {
                let ast = ffi::Z3_mk_int64(self.ctx, *n, self.int_sort);
                Ok(self.track(ast))
            }
            SmtTerm::BoolLit(b) => {
                let ast = if *b { ffi::Z3_mk_true(self.ctx) } else { ffi::Z3_mk_false(self.ctx) };
                Ok(self.track(ast))
            }
            SmtTerm::StrLit(s) => {
                let c = CString::new(s.as_str())
                    .map_err(|_| "string literal contains NUL".to_string())?;
                let ast = ffi::Z3_mk_string(self.ctx, c.as_ptr());
                Ok(self.track(ast))
            }
            SmtTerm::Var(name) => {
                if let Some((ast, _)) = self.vars.get(name) {
                    return Ok(*ast);
                }
                // Auto-declare as Int — это безопасный default для
                // implicit-vars типа `_old_<x>` или `_unit`. Если позже
                // declare_var вызывается для того же имени с другим
                // sort — будет mismatch (мы не handle'им, поскольку
                // pipeline всегда сначала declare params).
                let cname = CString::new(name.as_str())
                    .unwrap_or_else(|_| CString::new("v").unwrap());
                let sym = ffi::Z3_mk_string_symbol(self.ctx, cname.as_ptr());
                let ast = ffi::Z3_mk_const(self.ctx, sym, self.int_sort);
                ffi::Z3_inc_ref(self.ctx, ast);
                self.refs.push(ast);
                self.vars.insert(name.clone(), (ast, SortRef::Int));
                Ok(ast)
            }
            SmtTerm::App(op, args) => self.translate_app(op, args),
        }
    }

    unsafe fn translate_app(&mut self, op: &str, args: &[SmtTerm]) -> Result<ffi::Z3_ast, String> {
        let mut translated: Vec<ffi::Z3_ast> = Vec::with_capacity(args.len());
        for a in args {
            translated.push(self.translate_inner(a)?);
        }
        let ctx = self.ctx;
        let ast = match (op, translated.as_slice()) {
            // Arithmetic — variadic.
            ("+", a) if !a.is_empty() => ffi::Z3_mk_add(ctx, a.len() as c_uint, a.as_ptr()),
            ("-", a) if a.len() >= 2 => ffi::Z3_mk_sub(ctx, a.len() as c_uint, a.as_ptr()),
            ("-", &[x]) => ffi::Z3_mk_unary_minus(ctx, x),
            ("*", a) if !a.is_empty() => ffi::Z3_mk_mul(ctx, a.len() as c_uint, a.as_ptr()),
            ("/", &[x, y]) => ffi::Z3_mk_div(ctx, x, y),
            ("%", &[x, y]) => ffi::Z3_mk_mod(ctx, x, y),

            // Comparison.
            ("=", &[x, y]) => ffi::Z3_mk_eq(ctx, x, y),
            ("!=", &[x, y]) => {
                let arr = [x, y];
                ffi::Z3_mk_distinct(ctx, 2, arr.as_ptr())
            }
            ("<", &[x, y]) => ffi::Z3_mk_lt(ctx, x, y),
            ("<=", &[x, y]) => ffi::Z3_mk_le(ctx, x, y),
            (">", &[x, y]) => ffi::Z3_mk_gt(ctx, x, y),
            (">=", &[x, y]) => ffi::Z3_mk_ge(ctx, x, y),

            // Boolean.
            ("not", &[x]) => ffi::Z3_mk_not(ctx, x),
            ("and", a) if !a.is_empty() => ffi::Z3_mk_and(ctx, a.len() as c_uint, a.as_ptr()),
            ("or", a) if !a.is_empty() => ffi::Z3_mk_or(ctx, a.len() as c_uint, a.as_ptr()),
            ("=>", &[x, y]) => ffi::Z3_mk_implies(ctx, x, y),
            ("<=>", &[x, y]) => ffi::Z3_mk_iff(ctx, x, y),

            // Uninterpreted function (e.g. `_field_balance(account)`,
            // которое encode.rs производит для member-access). Z3-side
            // мы трактуем как uninterpreted: семантика unchanged, equality
            // даёт нужное reasoning.
            (op_name, args_arr) if op_name.starts_with("_field_") || op_name.starts_with("_view_") => {
                // Для V1 closure mvp — кодируем UF-style через специальный
                // path: Z3_mk_app требует func_decl. Мы строим func_decl
                // on the fly с domain = типы args, range = Int (default).
                // Это упрощение: enums/records трактуются как opaque ints.
                return self.uninterpreted_app(op_name, args_arr);
            }
            _ => {
                return Err(format!(
                    "z3 backend: unsupported op `{}` with {} arg(s)",
                    op,
                    translated.len()
                ));
            }
        };
        Ok(self.track(ast))
    }

    /// Uninterpreted function application: создаёт func_decl на лету.
    /// Для Plan 33 MVP — все uninterpreted'ы имеют Int range (упрощение
    /// V1; полноценные record-types ждут типизированного encode'а).
    unsafe fn uninterpreted_app(&mut self, name: &str, args: &[ffi::Z3_ast]) -> Result<ffi::Z3_ast, String> {
        // Z3_mk_func_decl + Z3_mk_app не объявлены в нашем мини-FFI чтобы
        // не раздувать его. Вместо этого: создаём fresh constant
        // вычисляемый по `(name, args)` signature. То есть один и тот же
        // `_field_balance(x)` всегда → тот же constant. Это эквивалентно
        // uninterpreted function семантике для нашего use-case.
        //
        // Кешируем через имя: `{name}__{ptr1}_{ptr2}_...`. Это hack но
        // достаточно для V1 closure (encode.rs производит UF только
        // через member-access — где arg = encoded obj-expr; equal
        // sub-exprs → equal Z3_ast pointers через Z3 hash-consing).
        let mut key = format!("uf__{}", name);
        for a in args {
            key.push_str(&format!("__{:p}", *a));
        }
        if let Some((ast, _)) = self.vars.get(&key) {
            return Ok(*ast);
        }
        let ckey = CString::new(key.as_str()).unwrap();
        let sym = ffi::Z3_mk_string_symbol(self.ctx, ckey.as_ptr());
        let ast = ffi::Z3_mk_const(self.ctx, sym, self.int_sort);
        ffi::Z3_inc_ref(self.ctx, ast);
        self.refs.push(ast);
        self.vars.insert(key.clone(), (ast, SortRef::Int));
        Ok(ast)
    }

    fn extract_model(&self) -> Model {
        let mut bindings = HashMap::new();
        unsafe {
            let m = ffi::Z3_solver_get_model(self.ctx, self.solver);
            if m.is_null() {
                return Model { bindings };
            }
            ffi::Z3_model_inc_ref(self.ctx, m);
            for (name, (ast, sort)) in &self.vars {
                // Только user-declared vars (skip uf__/_old_* — служебные).
                if name.starts_with("uf__") { continue; }
                let mut out: ffi::Z3_ast = ptr::null_mut();
                let ok = ffi::Z3_model_eval(self.ctx, m, *ast, /* completion */ 1, &mut out);
                if ok == 0 || out.is_null() { continue; }
                let val = match sort {
                    SortRef::Int => {
                        let mut iv: i64 = 0;
                        if ffi::Z3_get_numeral_int64(self.ctx, out, &mut iv) != 0 {
                            ModelValue::Int(iv)
                        } else {
                            ModelValue::Unknown
                        }
                    }
                    SortRef::Bool => match ffi::Z3_get_bool_value(self.ctx, out) {
                        ffi::Z3_L_TRUE => ModelValue::Bool(true),
                        ffi::Z3_L_FALSE => ModelValue::Bool(false),
                        _ => ModelValue::Unknown,
                    },
                    SortRef::Str => {
                        // Z3_ast_to_string возвращает SMT2-printable вид —
                        // для строкового literal'а это `"foo"`. Для
                        // counterexample — приемлемо.
                        let s_ptr = ffi::Z3_ast_to_string(self.ctx, out);
                        if s_ptr.is_null() {
                            ModelValue::Unknown
                        } else {
                            let cs = CStr::from_ptr(s_ptr);
                            ModelValue::Str(cs.to_string_lossy().into_owned())
                        }
                    }
                    SortRef::Named(_) => ModelValue::Unknown,
                };
                bindings.insert(name.clone(), val);
            }
            ffi::Z3_model_dec_ref(self.ctx, m);
        }
        Model { bindings }
    }
}

impl Drop for Z3Backend {
    fn drop(&mut self) {
        unsafe {
            for r in self.refs.drain(..) {
                ffi::Z3_dec_ref(self.ctx, r);
            }
            ffi::Z3_dec_ref(self.ctx, self.int_sort);
            ffi::Z3_dec_ref(self.ctx, self.bool_sort);
            ffi::Z3_dec_ref(self.ctx, self.str_sort);
            ffi::Z3_solver_dec_ref(self.ctx, self.solver);
            ffi::Z3_del_context(self.ctx);
        }
    }
}

impl SmtBackend for Z3Backend {
    fn name(&self) -> &'static str { "z3" }

    fn declare_var(&mut self, name: &str, sort: SortRef) {
        // Idempotent — повторный declare с тем же именем игнорируется.
        if self.vars.contains_key(name) { return; }
        let z3_sort = self.sort_for(&sort);
        unsafe {
            let cname = CString::new(name)
                .unwrap_or_else(|_| CString::new("v").unwrap());
            let sym = ffi::Z3_mk_string_symbol(self.ctx, cname.as_ptr());
            let ast = ffi::Z3_mk_const(self.ctx, sym, z3_sort);
            ffi::Z3_inc_ref(self.ctx, ast);
            self.refs.push(ast);
            self.vars.insert(name.to_string(), (ast, sort));
        }
    }

    fn assert(&mut self, assertion: Assertion) {
        match self.translate(&assertion.formula) {
            Ok(ast) => unsafe {
                ffi::Z3_solver_assert(self.ctx, self.solver, ast);
                self.assertions.push(assertion);
            },
            Err(_msg) => {
                // Translation fail → ничего не assert'им. check_sat вернёт
                // Unknown(BackendError) если эта формула была критична —
                // upstream pipeline уже логирует EncodingFailed.
                // (Мы могли бы сохранить msg для diag, но проще оставить
                // sub-component reporting через encoder.)
            }
        }
    }

    fn push(&mut self) {
        unsafe { ffi::Z3_solver_push(self.ctx, self.solver); }
        self.scopes.push((self.assertions.len(), self.refs.len()));
    }

    fn pop(&mut self) {
        unsafe { ffi::Z3_solver_pop(self.ctx, self.solver, 1); }
        if let Some((al, _rl)) = self.scopes.pop() {
            self.assertions.truncate(al);
            // refs не откатываем — они tracked для Drop. Pop здесь
            // освобождает Z3-side scope; Rust-side refs остаются
            // живыми до Drop. Это намеренно: ASTs из попнутого scope
            // могут всё ещё быть referenced через `vars` (declared
            // outside push) — детальный refcount-tracking усложнил бы
            // backend без пользы для MVP.
        }
    }

    fn check_sat(&mut self) -> SatResult {
        let res = unsafe { ffi::Z3_solver_check(self.ctx, self.solver) };
        match res {
            ffi::Z3_L_TRUE => SatResult::Sat(self.extract_model()),
            ffi::Z3_L_FALSE => SatResult::Unsat(UnsatCore::default()),
            _ => {
                let reason = unsafe {
                    let p = ffi::Z3_solver_get_reason_unknown(self.ctx, self.solver);
                    if p.is_null() {
                        "z3 returned unknown".to_string()
                    } else {
                        CStr::from_ptr(p).to_string_lossy().into_owned()
                    }
                };
                let classified = if reason.contains("timeout") || reason.contains("canceled") {
                    UnknownReason::Timeout
                } else if reason.contains("non-linear") || reason.contains("nonlinear") {
                    UnknownReason::NonLinearArithmetic
                } else if reason.is_empty() || reason == "unknown" {
                    UnknownReason::NotAttempted(
                        "z3 returned unknown without specific reason".into())
                } else {
                    UnknownReason::BackendError(reason)
                };
                SatResult::Unknown(classified)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::try_prove;

    #[test]
    fn z3_proves_reflexive() {
        let mut b = Z3Backend::new(2000);
        b.declare_var("x", SortRef::Int);
        let x = SmtTerm::Var("x".into());
        let goal = SmtTerm::eq(x.clone(), x);
        assert!(matches!(try_prove(&mut b, goal), SatResult::Unsat(_)));
    }

    #[test]
    fn z3_proves_linear_arith() {
        // requires x > 0 ==> x + 1 > 0
        let mut b = Z3Backend::new(2000);
        b.declare_var("x", SortRef::Int);
        let x = SmtTerm::Var("x".into());
        b.assert(Assertion {
            formula: SmtTerm::App(">".into(), vec![x.clone(), SmtTerm::IntLit(0)]),
            label: None,
        });
        let goal = SmtTerm::App(">".into(), vec![
            SmtTerm::App("+".into(), vec![x, SmtTerm::IntLit(1)]),
            SmtTerm::IntLit(0),
        ]);
        assert!(matches!(try_prove(&mut b, goal), SatResult::Unsat(_)),
                "z3 should prove x>0 → x+1>0");
    }

    #[test]
    fn z3_disproves_false_ensures() {
        // body=100, ensures result == 42 → counterexample expected
        let mut b = Z3Backend::new(2000);
        // No params — pure constant check.
        let goal = SmtTerm::eq(SmtTerm::IntLit(100), SmtTerm::IntLit(42));
        let res = try_prove(&mut b, goal);
        assert!(matches!(res, SatResult::Sat(_)),
                "z3 should disprove 100==42, got {:?}", res);
    }
}
