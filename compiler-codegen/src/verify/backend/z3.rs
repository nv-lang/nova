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

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_uint};
use std::ptr;

use super::super::ir::*;
use super::SmtBackend;
use super::z3_ffi as ffi;

// Thread-local флаг: Z3 error handler ставит его при любой API ошибке.
// translate_inner проверяет после каждого вызова и возвращает Err.
thread_local! {
    static Z3_ERROR: Cell<bool> = Cell::new(false);
    static Z3_ERROR_MSG: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
}

unsafe extern "C" fn z3_error_handler(_ctx: ffi::Z3_context, code: c_int) {
    Z3_ERROR.with(|f| f.set(true));
    Z3_ERROR_MSG.with(|m| {
        *m.borrow_mut() = format!("Z3 API error code {}", code);
    });
}

fn check_z3_error() -> Result<(), String> {
    Z3_ERROR.with(|f| {
        if f.get() {
            f.set(false);
            let msg = Z3_ERROR_MSG.with(|m| m.borrow().clone());
            Err(msg)
        } else {
            Ok(())
        }
    })
}

/// Полноценный SMT backend через libz3.
///
/// Lifecycle: `new` → `declare_var`* → `assert`* → `push` / `pop` /
/// `check_sat`. На `Drop` все Z3 references освобождаются.
pub struct Z3Backend {
    ctx: ffi::Z3_context,
    solver: ffi::Z3_solver,
    /// Кэш sort'ов — int/bool/str используются массово.
    int_sort: ffi::Z3_sort,
    bool_sort: ffi::Z3_sort,
    str_sort: ffi::Z3_sort,
    /// Rounding mode RNE (round nearest, ties to even) — default для FP ops.
    /// FP sort'ы (f32/f64) запрашиваются свежо через Z3_mk_fpa_sort_32/64 каждый раз
    /// (Z3 hash-cons'ит их, overhead минимальный, зато нет проблем с pointer validity).
    rne: ffi::Z3_ast,
    /// Declared variables: name → (Z3_ast, sort).
    vars: HashMap<String, (ffi::Z3_ast, SortRef)>,
    /// Plan 33.3 Ф.9: declared uninterpreted functions (pure_view'ы).
    /// name → (Z3_func_decl, return_sort).
    func_decls: HashMap<String, (ffi::Z3_func_decl, SortRef)>,
    /// Все AST refs которые мы должны dec_ref при Drop.
    refs: Vec<ffi::Z3_ast>,
    /// Сохраняются для extract_model — solver assertion order.
    assertions: Vec<Assertion>,
    /// push/pop scope stack — храним высоту `assertions`/`refs` чтобы
    /// откатывать.
    scopes: Vec<(usize, usize)>,
    /// Plan 33.8 Ф.6.2: формула не транслировалась в Z3 AST (`assert`
    /// получил `Err`). Если непереведённой оказалась `not goal` из
    /// `try_prove`, её молчаливый отброс мог бы дать ложный `Unsat`
    /// (= ложный `Proven`) на противоречивом контексте. Поэтому при
    /// любой ошибке трансляции помечаем backend tainted → следующий
    /// `check_sat` обязан вернуть `Unknown`, а не доверять решателю.
    translation_failed: bool,
}

// SAFETY: Z3 context is не thread-safe для одновременного использования
// из разных потоков, но **ownership transfer** между потоками безопасен.
// `Z3Backend` инкапсулирует context/solver полностью; ни один pointer
// не уезжает за пределы методов. SmtBackend trait требует `Send` чтобы
// pipeline мог положить backend в Box<dyn SmtBackend>.
unsafe impl Send for Z3Backend {}

impl Z3Backend {
    /// Создать backend. `timeout_ms` устанавливается per-solver через
    /// `Z3_solver_set_params` (Ф.8.3, Plan 33.6). Раньше использовался
    /// `Z3_global_param_set` — race-condition при parallel verify.
    pub fn new(timeout_ms: u32) -> Self {
        unsafe {
            let cfg = ffi::Z3_mk_config();
            // model.completion: если в модели var не присвоена, дать ей
            // any value (default). Делает извлечение counterexample
            // более user-friendly.
            let mk = CString::new("model").unwrap();
            let tr = CString::new("true").unwrap();
            ffi::Z3_set_param_value(cfg, mk.as_ptr(), tr.as_ptr());
            // Хранение CString'ов в local-vars пока config не уничтожен.
            let _hold_mk = mk;
            let _hold_tr = tr;

            let ctx = ffi::Z3_mk_context_rc(cfg);
            ffi::Z3_del_config(cfg);
            // Перехватываем Z3 API ошибки (sort mismatch и т.д.) вместо abort().
            ffi::Z3_set_error_handler(ctx, Some(z3_error_handler));

            let int_sort = ffi::Z3_mk_int_sort(ctx);
            let bool_sort = ffi::Z3_mk_bool_sort(ctx);
            let str_sort = ffi::Z3_mk_string_sort(ctx);
            let rne = ffi::Z3_mk_fpa_round_nearest_ties_to_even(ctx);
            ffi::Z3_inc_ref(ctx, int_sort);
            ffi::Z3_inc_ref(ctx, bool_sort);
            ffi::Z3_inc_ref(ctx, str_sort);
            ffi::Z3_inc_ref(ctx, rne);

            let solver = ffi::Z3_mk_solver(ctx);
            ffi::Z3_solver_inc_ref(ctx, solver);

            // Ф.8.3 (Plan 33.6): per-solver timeout (раньше Z3_global_param_set —
            // race-condition при parallel verify Ф.5.1).
            let params = ffi::Z3_mk_params(ctx);
            ffi::Z3_params_inc_ref(ctx, params);
            let timeout_key = CString::new("timeout").unwrap();
            let timeout_sym = ffi::Z3_mk_string_symbol(ctx, timeout_key.as_ptr());
            ffi::Z3_params_set_uint(ctx, params, timeout_sym, timeout_ms);
            ffi::Z3_solver_set_params(ctx, solver, params);

            Self {
                ctx,
                solver,
                int_sort,
                bool_sort,
                str_sort,
                rne,
                vars: HashMap::new(),
                func_decls: HashMap::new(),
                refs: Vec::new(),
                assertions: Vec::new(),
                scopes: Vec::new(),
                translation_failed: false,
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
            SortRef::F32 => unsafe { ffi::Z3_mk_fpa_sort_32(self.ctx) },
            SortRef::F64 => unsafe { ffi::Z3_mk_fpa_sort_64(self.ctx) },
            SortRef::BitVec { width, .. } => unsafe { ffi::Z3_mk_bv_sort(self.ctx, *width) },
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
        // Сбросить флаг ошибки перед каждым top-level translate.
        Z3_ERROR.with(|f| f.set(false));
        unsafe { self.translate_inner(term) }
    }

    unsafe fn translate_inner(&mut self, term: &SmtTerm) -> Result<ffi::Z3_ast, String> {
        let result = self.translate_inner_impl(term);
        // Проверить не возникла ли Z3 API ошибка (sort mismatch и т.п.)
        // после любого вложенного вызова.
        if result.is_ok() {
            if let Err(msg) = check_z3_error() {
                return Err(msg);
            }
        }
        result
    }

    unsafe fn translate_inner_impl(&mut self, term: &SmtTerm) -> Result<ffi::Z3_ast, String> {
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
            SmtTerm::F32Lit(bits) => {
                let v = f32::from_bits(*bits);
                let f32_sort = ffi::Z3_mk_fpa_sort_32(self.ctx);
                let ast = ffi::Z3_mk_fpa_numeral_float(self.ctx, v, f32_sort);
                Ok(self.track(ast))
            }
            SmtTerm::F64Lit(bits) => {
                let v = f64::from_bits(*bits);
                let f64_sort = ffi::Z3_mk_fpa_sort_64(self.ctx);
                let ast = ffi::Z3_mk_fpa_numeral_double(self.ctx, v, f64_sort);
                Ok(self.track(ast))
            }
            // Plan 33.7: bit-vector literal.
            SmtTerm::BitVecLit(v, w) => {
                let bv_sort = ffi::Z3_mk_bv_sort(self.ctx, *w);
                let ast = ffi::Z3_mk_unsigned_int64(self.ctx, *v, bv_sort);
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
            // Plan 33.3 Ф.9: universal quantifier через Z3_mk_forall_const.
            //
            // Создаём fresh constants для каждого binder, translate body
            // (где binder-имена резолвятся через `vars` HashMap'у), затем
            // Z3_mk_forall_const упаковывает в forall AST.
            SmtTerm::Forall(binders, patterns, body) => {
                if binders.is_empty() {
                    // Empty forall == body unchanged.
                    return self.translate_inner(body);
                }
                // Создаём fresh consts для binders, регистрируем в vars.
                // Сохраняем previous bindings чтобы откатить после quantifier
                // (capture-avoiding semantics: binder shadows outer var
                // только внутри body).
                let mut bound_apps: Vec<ffi::Z3_app> = Vec::with_capacity(binders.len());
                let mut saved: Vec<(String, Option<(ffi::Z3_ast, SortRef)>)> = Vec::with_capacity(binders.len());
                for (bname, bsort) in binders {
                    let prev = self.vars.get(bname).cloned();
                    saved.push((bname.clone(), prev));
                    let z3_sort = self.sort_for(bsort);
                    let cname = CString::new(bname.as_str())
                        .unwrap_or_else(|_| CString::new("_b").unwrap());
                    let sym = ffi::Z3_mk_string_symbol(self.ctx, cname.as_ptr());
                    let ast = ffi::Z3_mk_const(self.ctx, sym, z3_sort);
                    ffi::Z3_inc_ref(self.ctx, ast);
                    self.refs.push(ast);
                    self.vars.insert(bname.clone(), (ast, bsort.clone()));
                    let app = ffi::Z3_to_app(self.ctx, ast);
                    bound_apps.push(app);
                }
                let body_ast = self.translate_inner(body)?;
                // Ф.1.2 (Plan 33.5): используем patterns из SmtTerm::Forall.patterns
                // (собранные encode.rs::collect_triggers). Каждый pattern —
                // Vec<SmtTerm>, переводим в Z3_pattern через Z3_mk_pattern.
                // Если patterns пустые — Z3 использует heuristic.
                // Z3_pattern is *mut c_void (no separate alias in our ffi).
                let mut z3_patterns: Vec<*mut std::ffi::c_void> = Vec::new();
                for pat_terms in patterns {
                    if pat_terms.is_empty() { continue; }
                    let mut term_asts: Vec<ffi::Z3_ast> = Vec::with_capacity(pat_terms.len());
                    let mut ok = true;
                    for pt in pat_terms {
                        match self.translate_inner(pt) {
                            Ok(a) => term_asts.push(a),
                            Err(_) => { ok = false; break; }
                        }
                    }
                    if ok && !term_asts.is_empty() {
                        let z3_pat = ffi::Z3_mk_pattern(
                            self.ctx,
                            term_asts.len() as c_uint,
                            term_asts.as_ptr(),
                        );
                        z3_patterns.push(z3_pat);
                    }
                }
                // Restore previous var-bindings.
                for (bname, prev) in saved {
                    match prev {
                        Some(p) => { self.vars.insert(bname, p); }
                        None => { self.vars.remove(&bname); }
                    }
                }
                let num_patterns = z3_patterns.len() as c_uint;
                let pat_ptr = if z3_patterns.is_empty() {
                    ptr::null()
                } else {
                    z3_patterns.as_ptr()
                };
                let forall_ast = ffi::Z3_mk_forall_const(
                    self.ctx,
                    0, // weight
                    bound_apps.len() as c_uint,
                    bound_apps.as_ptr(),
                    num_patterns,
                    pat_ptr as *const *mut std::ffi::c_void,
                    body_ast,
                );
                Ok(self.track(forall_ast))
            }
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
            // ITE: правильный if-then-else для arithmetic и bool terms.
            ("ite", &[cond, then, else_]) => ffi::Z3_mk_ite(ctx, cond, then, else_),

            // ─── FP IEEE 754 (Plan 33.3 Ф.11) ────────────────────────────
            // Arithmetic с rounding mode RNE.
            ("fp.add", &[x, y]) => ffi::Z3_mk_fpa_add(ctx, self.rne, x, y),
            ("fp.sub", &[x, y]) => ffi::Z3_mk_fpa_sub(ctx, self.rne, x, y),
            ("fp.mul", &[x, y]) => ffi::Z3_mk_fpa_mul(ctx, self.rne, x, y),
            ("fp.div", &[x, y]) => ffi::Z3_mk_fpa_div(ctx, self.rne, x, y),
            ("fp.abs", &[x]) => ffi::Z3_mk_fpa_abs(ctx, x),
            ("fp.neg", &[x]) => ffi::Z3_mk_fpa_neg(ctx, x),
            ("fp.sqrt", &[x]) => ffi::Z3_mk_fpa_sqrt(ctx, self.rne, x),
            // FP comparisons (fp.eq — IEEE eq, не total order).
            ("fp.eq",  &[x, y]) => ffi::Z3_mk_fpa_eq(ctx, x, y),
            ("fp.lt",  &[x, y]) => ffi::Z3_mk_fpa_lt(ctx, x, y),
            ("fp.leq", &[x, y]) => ffi::Z3_mk_fpa_leq(ctx, x, y),
            ("fp.gt",  &[x, y]) => ffi::Z3_mk_fpa_gt(ctx, x, y),
            ("fp.geq", &[x, y]) => ffi::Z3_mk_fpa_geq(ctx, x, y),
            // FP predicates.
            ("fp.is_nan",      &[x]) => ffi::Z3_mk_fpa_is_nan(ctx, x),
            ("fp.is_infinite", &[x]) => ffi::Z3_mk_fpa_is_infinite(ctx, x),
            ("fp.is_positive",  &[x]) => ffi::Z3_mk_fpa_is_positive(ctx, x),
            ("fp.is_negative",  &[x]) => ffi::Z3_mk_fpa_is_negative(ctx, x),
            ("fp.is_zero",     &[x]) => ffi::Z3_mk_fpa_is_zero(ctx, x),

            // ─── Strings / Seq (Plan 33.3 Ф.11) ──────────────────────────
            ("str.len",      &[x]) => ffi::Z3_mk_seq_length(ctx, x),
            ("str.contains", &[x, y]) => ffi::Z3_mk_seq_contains(ctx, x, y),
            ("str.prefix",   &[x, y]) => ffi::Z3_mk_seq_prefix(ctx, x, y),
            ("str.suffix",   &[x, y]) => ffi::Z3_mk_seq_suffix(ctx, x, y),
            ("str.concat", args_arr) if !args_arr.is_empty() => {
                ffi::Z3_mk_seq_concat(ctx, args_arr.len() as c_uint, args_arr.as_ptr())
            }
            ("str.substr", &[s, off, len]) => ffi::Z3_mk_seq_extract(ctx, s, off, len),
            ("str.index",  &[s, sub, off]) => ffi::Z3_mk_seq_index(ctx, s, sub, off),

            // ─── Bit-vectors (Plan 33.7) ──────────────────────────────────
            // Arithmetic (wrap-around, 2's complement).
            ("bvadd", &[x, y]) => ffi::Z3_mk_bvadd(ctx, x, y),
            ("bvsub", &[x, y]) => ffi::Z3_mk_bvsub(ctx, x, y),
            ("bvmul", &[x, y]) => ffi::Z3_mk_bvmul(ctx, x, y),
            ("bvsdiv", &[x, y]) => ffi::Z3_mk_bvsdiv(ctx, x, y),
            ("bvsrem", &[x, y]) => ffi::Z3_mk_bvsrem(ctx, x, y),
            ("bvudiv", &[x, y]) => ffi::Z3_mk_bvudiv(ctx, x, y),
            ("bvurem", &[x, y]) => ffi::Z3_mk_bvurem(ctx, x, y),
            ("bvneg", &[x]) => ffi::Z3_mk_bvneg(ctx, x),
            // Bitwise.
            ("bvand", &[x, y]) => ffi::Z3_mk_bvand(ctx, x, y),
            ("bvor",  &[x, y]) => ffi::Z3_mk_bvor(ctx, x, y),
            ("bvxor", &[x, y]) => ffi::Z3_mk_bvxor(ctx, x, y),
            ("bvnot", &[x]) => ffi::Z3_mk_bvnot(ctx, x),
            ("bvshl",  &[x, y]) => ffi::Z3_mk_bvshl(ctx, x, y),
            ("bvlshr", &[x, y]) => ffi::Z3_mk_bvlshr(ctx, x, y),
            ("bvashr", &[x, y]) => ffi::Z3_mk_bvashr(ctx, x, y),
            // Signed comparisons.
            ("bvslt", &[x, y]) => ffi::Z3_mk_bvslt(ctx, x, y),
            ("bvsle", &[x, y]) => ffi::Z3_mk_bvsle(ctx, x, y),
            ("bvsgt", &[x, y]) => ffi::Z3_mk_bvsgt(ctx, x, y),
            ("bvsge", &[x, y]) => ffi::Z3_mk_bvsge(ctx, x, y),
            // Unsigned comparisons.
            ("bvult", &[x, y]) => ffi::Z3_mk_bvult(ctx, x, y),
            ("bvule", &[x, y]) => ffi::Z3_mk_bvule(ctx, x, y),
            ("bvugt", &[x, y]) => ffi::Z3_mk_bvugt(ctx, x, y),
            ("bvuge", &[x, y]) => ffi::Z3_mk_bvuge(ctx, x, y),
            // Overflow predicates (для #nooverflow VC).
            ("bvadd_no_overflow_s", &[x, y]) => ffi::Z3_mk_bvadd_no_overflow(ctx, x, y, 1),
            ("bvadd_no_overflow_u", &[x, y]) => ffi::Z3_mk_bvadd_no_overflow(ctx, x, y, 0),
            ("bvadd_no_underflow",  &[x, y]) => ffi::Z3_mk_bvadd_no_underflow(ctx, x, y),
            ("bvsub_no_overflow",   &[x, y]) => ffi::Z3_mk_bvsub_no_overflow(ctx, x, y),
            ("bvsub_no_underflow_s",&[x, y]) => ffi::Z3_mk_bvsub_no_underflow(ctx, x, y, 1),
            ("bvsub_no_underflow_u",&[x, y]) => ffi::Z3_mk_bvsub_no_underflow(ctx, x, y, 0),
            ("bvmul_no_overflow_s", &[x, y]) => ffi::Z3_mk_bvmul_no_overflow(ctx, x, y, 1),
            ("bvmul_no_overflow_u", &[x, y]) => ffi::Z3_mk_bvmul_no_overflow(ctx, x, y, 0),

            // Plan 33.7 V2: BV cast-resize. Op-строка несёт числовой
            // параметр: "zero_extend N" / "sign_extend N" / "extract H L".
            (op_name, &[x]) if op_name.starts_with("zero_extend ") => {
                let n: u32 = op_name["zero_extend ".len()..].trim().parse()
                    .map_err(|_| format!("z3: bad zero_extend param in `{}`", op_name))?;
                ffi::Z3_mk_zero_ext(ctx, n as c_uint, x)
            }
            (op_name, &[x]) if op_name.starts_with("sign_extend ") => {
                let n: u32 = op_name["sign_extend ".len()..].trim().parse()
                    .map_err(|_| format!("z3: bad sign_extend param in `{}`", op_name))?;
                ffi::Z3_mk_sign_ext(ctx, n as c_uint, x)
            }
            (op_name, &[x]) if op_name.starts_with("extract ") => {
                let rest = &op_name["extract ".len()..];
                let mut parts = rest.split_whitespace();
                let high: u32 = parts.next().and_then(|s| s.parse().ok())
                    .ok_or_else(|| format!("z3: bad extract high in `{}`", op_name))?;
                let low: u32 = parts.next().and_then(|s| s.parse().ok())
                    .ok_or_else(|| format!("z3: bad extract low in `{}`", op_name))?;
                ffi::Z3_mk_extract(ctx, high as c_uint, low as c_uint, x)
            }

            // Plan 33.3 Ф.9: pure_view-UF через real Z3_func_decl
            // (pre-declared в declare_function, sorts из effect-сигнатуры).
            (op_name, args_arr) if op_name.starts_with("_view_") => {
                return self.uf_app(op_name, args_arr);
            }
            // Plan 33.4 D.0.2: pure fn UFs (`_pure_fn_*`) — pre-declared via
            // declare_function. Routes through uf_app for proper Z3_func_decl use.
            (op_name, args_arr) if op_name.starts_with("_pure_fn_") => {
                return self.uf_app(op_name, args_arr);
            }
            // Plan 33.6 Ф.4.2: trusted external fn UFs (`_trusted_*`).
            (op_name, args_arr) if op_name.starts_with("_trusted_") => {
                return self.uf_app(op_name, args_arr);
            }
            // Legacy: record member access (`_field_X(obj)`) кодируется
            // через fake fresh-const trick.
            (op_name, args_arr) if op_name.starts_with("_field_") => {
                return self.legacy_uninterpreted_app(op_name, args_arr);
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

    /// Plan 33.3 Ф.9: legacy fake-UF для `_field_X(obj)` (record member).
    /// Создаёт fresh constant с именем «uf__{name}__{ptr_of_arg}». Mixed
    /// sorts через один name (Counter.value, AnotherEffect.value) работают
    /// корректно потому что fresh-const'ы независимы.
    ///
    /// Plan 33.6 (2026-05-18): size-accessor UFs (`_field_len_int`,
    /// `_field_cap_int`, `_field_byte_len_int`) — non-negative by
    /// construction (соответствуют `obj.len()` / `obj.cap()` / `obj.byte_len()`,
    /// которые runtime-emit'ятся через `_size_t`-возвращающие builtins).
    /// Без этого axiom'а Z3 находил counterexample где `len()` < 0
    /// (см. trivial_string_len_positive.nv). TrivialBackend уже шорткатит
    /// `>= 0` goal для `_field_len*` через trivial.rs:628; здесь обеспечиваем
    /// тот же гарант на real-SMT уровне.
    unsafe fn legacy_uninterpreted_app(&mut self, name: &str, args: &[ffi::Z3_ast]) -> Result<ffi::Z3_ast, String> {
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

        if matches!(
            name,
            "_field_len_int" | "_field_cap_int" | "_field_byte_len_int"
        ) {
            let zero = ffi::Z3_mk_int(self.ctx, 0, self.int_sort);
            ffi::Z3_inc_ref(self.ctx, zero);
            self.refs.push(zero);
            let ge = ffi::Z3_mk_ge(self.ctx, ast, zero);
            ffi::Z3_inc_ref(self.ctx, ge);
            self.refs.push(ge);
            ffi::Z3_solver_assert(self.ctx, self.solver, ge);
        }

        Ok(ast)
    }

    /// Plan 33.3 Ф.9: применение pure_view UF (`_view_X_Y`).
    /// Использует pre-declared Z3_func_decl (из `declare_function`).
    /// Без pre-decl — auto-declare с Int sorts (fallback для unit-тестов).
    unsafe fn uf_app(&mut self, name: &str, args: &[ffi::Z3_ast]) -> Result<ffi::Z3_ast, String> {
        let decl = if let Some((d, _)) = self.func_decls.get(name) {
            *d
        } else {
            // Auto-declare с Int sort'ами (legacy fallback для `_field_*`).
            let int_sort = self.int_sort;
            let domain: Vec<ffi::Z3_sort> = args.iter().map(|_| int_sort).collect();
            let cname = CString::new(name).unwrap_or_else(|_| CString::new("_uf").unwrap());
            let sym = ffi::Z3_mk_string_symbol(self.ctx, cname.as_ptr());
            let d = ffi::Z3_mk_func_decl(
                self.ctx,
                sym,
                domain.len() as c_uint,
                domain.as_ptr(),
                int_sort,
            );
            ffi::Z3_inc_ref(self.ctx, d);
            self.refs.push(d);
            self.func_decls.insert(name.to_string(), (d, SortRef::Int));
            d
        };
        let ast = ffi::Z3_mk_app(self.ctx, decl, args.len() as c_uint, args.as_ptr());
        Ok(self.track(ast))
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
                    // Plan 33.7: BitVec — попытаться извлечь как int64.
                    SortRef::BitVec { .. } => {
                        let mut iv: i64 = 0;
                        if ffi::Z3_get_numeral_int64(self.ctx, out, &mut iv) != 0 {
                            ModelValue::Int(iv)
                        } else {
                            ModelValue::Unknown
                        }
                    }
                    SortRef::Named(_) | SortRef::F32 | SortRef::F64 => ModelValue::Unknown,
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
            ffi::Z3_dec_ref(self.ctx, self.rne);
            ffi::Z3_solver_dec_ref(self.ctx, self.solver);
            ffi::Z3_del_context(self.ctx);
        }
    }
}

impl SmtBackend for Z3Backend {
    fn name(&self) -> &'static str { "z3" }

    fn declare_function(
        &mut self,
        name: &str,
        param_sorts: &[SortRef],
        return_sort: SortRef,
    ) {
        if self.func_decls.contains_key(name) { return; }
        unsafe {
            let domain: Vec<ffi::Z3_sort> = param_sorts.iter()
                .map(|s| self.sort_for(s))
                .collect();
            let range = self.sort_for(&return_sort);
            let cname = CString::new(name)
                .unwrap_or_else(|_| CString::new("_uf").unwrap());
            let sym = ffi::Z3_mk_string_symbol(self.ctx, cname.as_ptr());
            let d = ffi::Z3_mk_func_decl(
                self.ctx,
                sym,
                domain.len() as c_uint,
                domain.as_ptr(),
                range,
            );
            // Refcounted-context: inc_ref на func_decl чтобы Z3 не GC'нул
            // его до Drop. (Z3_func_decl — это alias Z3_ast, ref-mgmt тот же.)
            ffi::Z3_inc_ref(self.ctx, d);
            self.refs.push(d);
            self.func_decls.insert(name.to_string(), (d, return_sort));
        }
    }

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
                // Plan 33.8 Ф.6.2: формула не транслировалась. Молча
                // отбрасывать НЕЛЬЗЯ — если это была `not goal` из
                // try_prove, а контекст противоречив, check_sat вернёт
                // Unsat = ложный Proven. Помечаем backend tainted →
                // check_sat вернёт Unknown(BackendError).
                self.translation_failed = true;
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

    fn get_witness(&mut self, var_name: &str) -> Option<ModelValue> {
        // Ф.10.2 (Plan 33.6): извлечь witness через текущую модель Z3.
        // Только после check_sat → Sat. Если check_sat не вызывался или дал
        // не-Sat — extract_model даст пустую модель → return None.
        let model = self.extract_model();
        model.bindings.get(var_name).cloned()
    }

    fn check_sat(&mut self) -> SatResult {
        // Plan 33.8 Ф.6.2: если хоть одна формула не транслировалась в Z3
        // (`assert` получил `Err`), решателю доверять нельзя. `try_prove`
        // вызывает `assert(not goal)` непосредственно перед `check_sat` —
        // значит провал трансляции `not goal` гарантированно попадает в
        // этот флаг до проверки. Возвращаем `Unknown`, а не рискуем
        // ложным `Unsat` (= ложный `Proven`) на противоречивом контексте.
        if self.translation_failed {
            self.translation_failed = false;
            return SatResult::Unknown(UnknownReason::BackendError(
                "формула не транслировалась в Z3 AST — результат \
                 проверки не определён (Plan 33.8 Ф.6.2)".into(),
            ));
        }
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
