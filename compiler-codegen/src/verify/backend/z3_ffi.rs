//! Plan 33 Z3 milestone: минимальные FFI bindings для libz3 C API.
//!
//! Дизайн (см. feedback_third_party_libs):
//! - Никакой внешний `z3-sys`/`z3` crate. Только то что используется
//!   в `Z3Backend` — это меньше 30 функций.
//! - Расширяется по мере необходимости (FP/strings/quantifiers — Plan 33.3).
//! - Все extern-функции — `unsafe`; safe-обёртки живут в `z3.rs`.
//!
//! Reference: <https://z3prover.github.io/api/html/group__capi.html>.
//! Соответствует Z3 4.13.x C API (vcpkg manifest pulls 4.13.0+).

#![cfg(feature = "z3-backend")]
#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::os::raw::{c_char, c_int, c_uint, c_void};

// Opaque handles. Z3 raw pointers — Z3 manages allocation, мы только
// держим refcounts через Z3_inc_ref / Z3_dec_ref.
pub type Z3_config = *mut c_void;
pub type Z3_context = *mut c_void;
pub type Z3_symbol = *mut c_void;
pub type Z3_ast = *mut c_void;
pub type Z3_sort = Z3_ast; // в C API — это alias
pub type Z3_func_decl = Z3_ast;
pub type Z3_app = Z3_ast;
pub type Z3_solver = *mut c_void;
pub type Z3_model = *mut c_void;
pub type Z3_ast_vector = *mut c_void;
pub type Z3_string = *const c_char;

// Z3_lbool — `(Z3_L_FALSE = -1, Z3_L_UNDEF = 0, Z3_L_TRUE = 1)`.
pub const Z3_L_FALSE: c_int = -1;
pub const Z3_L_UNDEF: c_int = 0;
pub const Z3_L_TRUE: c_int = 1;

// Z3_bool — typedef int. true = 1, false = 0.

// SAFETY: extern "C" — must match Z3 ABI exactly. Don't reorder, don't
// drop `extern "C"`. Все эти декларации проверяются линкером — если
// libz3 build из vcpkg имеет другие сигнатуры, linker error будет
// громким.
extern "C" {
    // Configuration & context.
    pub fn Z3_mk_config() -> Z3_config;
    pub fn Z3_del_config(c: Z3_config);
    pub fn Z3_set_param_value(c: Z3_config, param_id: Z3_string, param_value: Z3_string);
    pub fn Z3_mk_context_rc(c: Z3_config) -> Z3_context;
    pub fn Z3_del_context(c: Z3_context);
    pub fn Z3_inc_ref(c: Z3_context, a: Z3_ast);
    pub fn Z3_dec_ref(c: Z3_context, a: Z3_ast);

    // Global parameters (timeout etc).
    pub fn Z3_global_param_set(param_id: Z3_string, param_value: Z3_string);

    // Sorts.
    pub fn Z3_mk_int_sort(c: Z3_context) -> Z3_sort;
    pub fn Z3_mk_bool_sort(c: Z3_context) -> Z3_sort;
    pub fn Z3_mk_string_sort(c: Z3_context) -> Z3_sort;
    pub fn Z3_mk_uninterpreted_sort(c: Z3_context, s: Z3_symbol) -> Z3_sort;

    // Symbols.
    pub fn Z3_mk_string_symbol(c: Z3_context, s: Z3_string) -> Z3_symbol;

    // Constants & variables.
    pub fn Z3_mk_const(c: Z3_context, s: Z3_symbol, ty: Z3_sort) -> Z3_ast;

    // Literals.
    pub fn Z3_mk_int(c: Z3_context, v: c_int, ty: Z3_sort) -> Z3_ast;
    pub fn Z3_mk_int64(c: Z3_context, v: i64, ty: Z3_sort) -> Z3_ast;
    pub fn Z3_mk_true(c: Z3_context) -> Z3_ast;
    pub fn Z3_mk_false(c: Z3_context) -> Z3_ast;
    pub fn Z3_mk_string(c: Z3_context, s: Z3_string) -> Z3_ast;

    // Arithmetic.
    pub fn Z3_mk_add(c: Z3_context, num_args: c_uint, args: *const Z3_ast) -> Z3_ast;
    pub fn Z3_mk_sub(c: Z3_context, num_args: c_uint, args: *const Z3_ast) -> Z3_ast;
    pub fn Z3_mk_mul(c: Z3_context, num_args: c_uint, args: *const Z3_ast) -> Z3_ast;
    pub fn Z3_mk_div(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_mod(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_unary_minus(c: Z3_context, a: Z3_ast) -> Z3_ast;

    // Comparison.
    pub fn Z3_mk_eq(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_lt(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_le(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_gt(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_ge(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;

    // Boolean.
    pub fn Z3_mk_not(c: Z3_context, a: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_and(c: Z3_context, num_args: c_uint, args: *const Z3_ast) -> Z3_ast;
    pub fn Z3_mk_or(c: Z3_context, num_args: c_uint, args: *const Z3_ast) -> Z3_ast;
    pub fn Z3_mk_implies(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_iff(c: Z3_context, a1: Z3_ast, a2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_distinct(c: Z3_context, num_args: c_uint, args: *const Z3_ast) -> Z3_ast;

    // Solver.
    pub fn Z3_mk_solver(c: Z3_context) -> Z3_solver;
    pub fn Z3_solver_inc_ref(c: Z3_context, s: Z3_solver);
    pub fn Z3_solver_dec_ref(c: Z3_context, s: Z3_solver);
    pub fn Z3_solver_push(c: Z3_context, s: Z3_solver);
    pub fn Z3_solver_pop(c: Z3_context, s: Z3_solver, n: c_uint);
    pub fn Z3_solver_reset(c: Z3_context, s: Z3_solver);
    pub fn Z3_solver_assert(c: Z3_context, s: Z3_solver, a: Z3_ast);
    pub fn Z3_solver_check(c: Z3_context, s: Z3_solver) -> c_int; // Z3_lbool
    pub fn Z3_solver_get_model(c: Z3_context, s: Z3_solver) -> Z3_model;
    pub fn Z3_solver_get_reason_unknown(c: Z3_context, s: Z3_solver) -> Z3_string;
    pub fn Z3_solver_set_params(c: Z3_context, s: Z3_solver, p: *mut c_void);

    // Params (for solver timeout etc).
    pub fn Z3_mk_params(c: Z3_context) -> *mut c_void;
    pub fn Z3_params_inc_ref(c: Z3_context, p: *mut c_void);
    pub fn Z3_params_dec_ref(c: Z3_context, p: *mut c_void);
    pub fn Z3_params_set_uint(c: Z3_context, p: *mut c_void, k: Z3_symbol, v: c_uint);

    // Model.
    pub fn Z3_model_inc_ref(c: Z3_context, m: Z3_model);
    pub fn Z3_model_dec_ref(c: Z3_context, m: Z3_model);
    pub fn Z3_model_eval(
        c: Z3_context,
        m: Z3_model,
        t: Z3_ast,
        model_completion: c_int,
        v: *mut Z3_ast,
    ) -> c_int;
    pub fn Z3_get_numeral_int64(c: Z3_context, v: Z3_ast, i: *mut i64) -> c_int;
    pub fn Z3_get_bool_value(c: Z3_context, a: Z3_ast) -> c_int;

    // AST inspection (для extract counterexample).
    pub fn Z3_ast_to_string(c: Z3_context, a: Z3_ast) -> Z3_string;

    // Plan 33.3 Ф.9: quantifiers (universal forall) для axiom encoding.
    //
    // Z3_mk_forall_const принимает массив bound-constants (созданных через
    // Z3_mk_const) и body. Pattern'ы — для trigger-based instantiation
    // (опциональны; передаём пустой массив для default heuristics).
    //
    // weight=0 — default priority.
    pub fn Z3_mk_forall_const(
        c: Z3_context,
        weight: c_uint,
        num_bound: c_uint,
        bound: *const Z3_app,
        num_patterns: c_uint,
        patterns: *const *mut c_void, // Z3_pattern*; pустой массив
        body: Z3_ast,
    ) -> Z3_ast;

    // Z3_to_app конвертирует Z3_ast (когда это application — например
    // const) в Z3_app. Нужно для bound-constants в forall_const.
    pub fn Z3_to_app(c: Z3_context, a: Z3_ast) -> Z3_app;

    // Z3_mk_pattern: создать trigger-pattern из одного или нескольких
    // ground terms. Паттерн указывает Z3 на какие term-shapes instantiate
    // quantifier. Используется для pure fn body axioms чтобы обеспечить
    // instantiation при появлении `_pure_fn_X(...)` в формуле.
    pub fn Z3_mk_pattern(
        c: Z3_context,
        num_patterns: c_uint,
        terms: *const Z3_ast,
    ) -> *mut c_void; // Z3_pattern

    // Plan 33.3 Ф.9: real uninterpreted function declarations для
    // pure_view ops. Без них Z3_mk_const с pointer-keyed именами даёт
    // soundness-баг (alpha-rename binder'а ломает axiom propagation).
    //
    // Z3_mk_func_decl: декларирует UF `name : domain[0] × ... × domain[n-1] → range`.
    pub fn Z3_mk_func_decl(
        c: Z3_context,
        s: Z3_symbol,
        domain_size: c_uint,
        domain: *const Z3_sort,
        range: Z3_sort,
    ) -> Z3_func_decl;

    // If-then-else: ite(cond: Bool, then: T, else: T) -> T.
    // Правильное ITE для arithmetic — не теряет информацию как or+and encoding.
    pub fn Z3_mk_ite(c: Z3_context, t1: Z3_ast, t2: Z3_ast, t3: Z3_ast) -> Z3_ast;

    // Z3_mk_app: применить func_decl к аргументам, получая term.
    pub fn Z3_mk_app(
        c: Z3_context,
        d: Z3_func_decl,
        num_args: c_uint,
        args: *const Z3_ast,
    ) -> Z3_ast;

    // ─── Floating-point (IEEE 754) ────────────────────────────────────────
    // Sorts.
    pub fn Z3_mk_fpa_sort_32(c: Z3_context) -> Z3_sort;  // f32 = (fp 8 24)
    pub fn Z3_mk_fpa_sort_64(c: Z3_context) -> Z3_sort;  // f64 = (fp 11 53)

    // Rounding mode sort (нужен для arithmetic ops).
    pub fn Z3_mk_fpa_rounding_mode_sort(c: Z3_context) -> Z3_sort;
    // Rounding modes.
    pub fn Z3_mk_fpa_round_nearest_ties_to_even(c: Z3_context) -> Z3_ast; // RNE
    pub fn Z3_mk_fpa_round_toward_zero(c: Z3_context) -> Z3_ast;          // RTZ

    // Numerals.
    pub fn Z3_mk_fpa_numeral_double(c: Z3_context, v: f64, ty: Z3_sort) -> Z3_ast;
    pub fn Z3_mk_fpa_numeral_float(c: Z3_context, v: f32, ty: Z3_sort) -> Z3_ast;

    // Arithmetic (все принимают rounding_mode + два fp аргумента).
    pub fn Z3_mk_fpa_add(c: Z3_context, rm: Z3_ast, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_sub(c: Z3_context, rm: Z3_ast, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_mul(c: Z3_context, rm: Z3_ast, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_div(c: Z3_context, rm: Z3_ast, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_abs(c: Z3_context, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_neg(c: Z3_context, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_sqrt(c: Z3_context, rm: Z3_ast, t: Z3_ast) -> Z3_ast;

    // Comparisons (все Bool).
    pub fn Z3_mk_fpa_eq(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_lt(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_leq(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_gt(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_geq(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;

    // Predicates.
    pub fn Z3_mk_fpa_is_nan(c: Z3_context, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_is_infinite(c: Z3_context, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_is_positive(c: Z3_context, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_is_negative(c: Z3_context, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_fpa_is_zero(c: Z3_context, t: Z3_ast) -> Z3_ast;

    // Conversion fp → fp (для cast f32↔f64).
    pub fn Z3_mk_fpa_to_fp_float(
        c: Z3_context, rm: Z3_ast, t: Z3_ast, s: Z3_sort,
    ) -> Z3_ast;
    // Conversion Int → fp.
    pub fn Z3_mk_fpa_to_fp_signed(
        c: Z3_context, rm: Z3_ast, t: Z3_ast, s: Z3_sort,
    ) -> Z3_ast;

    // ─── Strings / Sequences (Z3 Seq theory) ──────────────────────────────
    pub fn Z3_mk_seq_sort(c: Z3_context, s: Z3_sort) -> Z3_sort;

    // String операции.
    pub fn Z3_mk_seq_length(c: Z3_context, s: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_seq_concat(c: Z3_context, n: c_uint, args: *const Z3_ast) -> Z3_ast;
    pub fn Z3_mk_seq_contains(c: Z3_context, container: Z3_ast, containee: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_seq_prefix(c: Z3_context, prefix: Z3_ast, s: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_seq_suffix(c: Z3_context, suffix: Z3_ast, s: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_seq_extract(c: Z3_context, s: Z3_ast, offset: Z3_ast, length: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_seq_index(c: Z3_context, s: Z3_ast, substr: Z3_ast, offset: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_seq_unit(c: Z3_context, elem: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_seq_empty(c: Z3_context, s: Z3_sort) -> Z3_ast;

    // Error handler: перехватывает Z3 API ошибки (sort mismatch, etc.)
    // вместо abort(). Тип callback: fn(ctx, error_code).
    // error_code — Z3_error_code enum (see z3_api.h), нас интересует
    // только сам факт ошибки.
    pub fn Z3_set_error_handler(
        c: Z3_context,
        h: Option<unsafe extern "C" fn(c: Z3_context, e: c_int)>,
    );

    // Получить текущий error code контекста.
    pub fn Z3_get_error_code(c: Z3_context) -> c_int;

    // Сбросить error code.
    pub fn Z3_reset_error_code(c: Z3_context);

    // ─── Bit-vectors (Plan 33.7) ──────────────────────────────────────────
    // Sort: (_ BitVec sz) — ширина sz бит.
    pub fn Z3_mk_bv_sort(c: Z3_context, sz: c_uint) -> Z3_sort;

    // Numerals.
    pub fn Z3_mk_unsigned_int64(c: Z3_context, v: u64, ty: Z3_sort) -> Z3_ast;

    // Arithmetic (результат wrap-around по модулю 2^N — 2's complement).
    pub fn Z3_mk_bvadd(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvsub(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvmul(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvsdiv(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast; // signed div
    pub fn Z3_mk_bvsrem(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast; // signed rem
    pub fn Z3_mk_bvudiv(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast; // unsigned div
    pub fn Z3_mk_bvurem(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast; // unsigned rem
    pub fn Z3_mk_bvneg(c: Z3_context, t: Z3_ast) -> Z3_ast;               // unary minus

    // Bitwise.
    pub fn Z3_mk_bvand(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvor(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvxor(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvnot(c: Z3_context, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvshl(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvlshr(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast; // logical shift right
    pub fn Z3_mk_bvashr(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast; // arithmetic shift right

    // Signed comparisons (→ Bool).
    pub fn Z3_mk_bvslt(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvsle(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvsgt(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvsge(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;

    // Unsigned comparisons (→ Bool).
    pub fn Z3_mk_bvult(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvule(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvugt(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvuge(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;

    // Overflow predicates: возвращают Bool — «нет переполнения при операции».
    // is_signed: 1 = signed, 0 = unsigned.
    pub fn Z3_mk_bvadd_no_overflow(c: Z3_context, t1: Z3_ast, t2: Z3_ast, is_signed: c_int) -> Z3_ast;
    pub fn Z3_mk_bvadd_no_underflow(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvsub_no_overflow(c: Z3_context, t1: Z3_ast, t2: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_bvsub_no_underflow(c: Z3_context, t1: Z3_ast, t2: Z3_ast, is_signed: c_int) -> Z3_ast;
    pub fn Z3_mk_bvmul_no_overflow(c: Z3_context, t1: Z3_ast, t2: Z3_ast, is_signed: c_int) -> Z3_ast;

    // Resize (Plan 33.7 V2): cast между BV-ширинами.
    // zero_ext/sign_ext: расширение на `i` дополнительных бит (нулями / знаком).
    // extract: выделение бит [high..low] включительно → BV ширины (high-low+1).
    pub fn Z3_mk_zero_ext(c: Z3_context, i: c_uint, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_sign_ext(c: Z3_context, i: c_uint, t: Z3_ast) -> Z3_ast;
    pub fn Z3_mk_extract(c: Z3_context, high: c_uint, low: c_uint, t: Z3_ast) -> Z3_ast;
}
