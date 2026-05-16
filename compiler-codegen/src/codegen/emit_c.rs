use crate::ast::*;
use crate::diag::Span;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;

/// Plan 11 Ф.1: одна signature методa в multi-overload registry.
/// `param_c_types` — C-типы параметров (без receiver'а), используются
/// для resolve по arg-types и для C-name mangling.
#[derive(Debug, Clone)]
pub struct MethodSig {
    pub param_c_types: Vec<String>,
    pub return_c_type: String,
    pub is_instance: bool,
    pub is_external: bool,
    /// Plan 11 Ф.9.2: D39 anonymous embed `use _ Type` auto-генерирует
    /// прокси методы. Override-precedence: Own (declared прямо на receiver'е)
    /// побеждает Delegated (auto-proxy). Резолвер фильтрует Delegated если
    /// есть совпадающий Own.
    pub is_delegated: bool,
    /// Mangled C name. Для single-overload — `Nova_T_method_m` /
    /// `Nova_T_static_m`. Для overloaded — с `_<param_type1>_<...>` суффиксом.
    pub c_name: String,
    /// Plan 14 Ф.6 (D69): true если последний параметр variadic
    /// (`...items []T`). На call-site emit_call collects args[N-1..]
    /// в синтезированный ArrayLit и передаёт как последний аргумент.
    pub variadic_last: bool,
}

/// Plan 39 Issue A: classification of `with`-block trail type for
/// choosing which NovaInterruptFrame slot to use.
///
/// - `IntLike`: `nova_int`, `nova_bool`, `nova_byte`, `nova_char`, plain
///   integers that fit in `nova_int` slot.
/// - `Pointer`: any C type containing `*` — `Nova_X*`, `NovaArray_X*`,
///   `void*`. Stored in `value_ptr` directly.
/// - `ValueStruct`: heap-stored value structs (`NovaOpt_X`, `NovaResult_X_E`,
///   tuples). Stored via heap-alloc'd slot pointed to by `value_ptr`.
/// - `UnitVoid`: unit / void — no value, slot unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithResultCategory {
    IntLike,
    Pointer,
    ValueStruct,
    UnitVoid,
}

/// D109: встроенные методы примитивных типов.
enum PrimBuiltin { Fn(&'static str), BinOp(&'static str) }

/// Plan 39 Issue A: walk handler expression (typically a ClosureLight /
/// ClosureFull / HandlerLit) и найти первый `interrupt VAL` — вернуть
/// C-тип VAL. Используется в `infer_expr_c_type` для `With`, когда body
/// не имеет trailing (тогда тип blocка определяется handler'ом).
pub fn infer_handler_interrupt_ty(emitter: &CEmitter, handler: &Expr) -> Option<String> {
    use crate::ast::{ClosureBody, FnBody, ElseBranch};
    fn walk_expr(emitter: &CEmitter, e: &Expr, out: &mut Option<String>) {
        if out.is_some() { return; }
        match &e.kind {
            ExprKind::Interrupt(Some(v)) => {
                *out = Some(emitter.infer_expr_c_type(v));
            }
            ExprKind::Interrupt(None) => {
                *out = Some("nova_int".into());
            }
            ExprKind::Block(b) => {
                walk_block(emitter, b, out);
            }
            ExprKind::If { then, else_, .. } => {
                walk_block(emitter, then, out);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => walk_block(emitter, b, out),
                        ElseBranch::If(if_expr) => walk_expr(emitter, if_expr, out),
                    }
                }
            }
            ExprKind::Match { arms, .. } => {
                use crate::ast::MatchArmBody;
                for a in arms {
                    match &a.body {
                        MatchArmBody::Expr(e) => walk_expr(emitter, e, out),
                        MatchArmBody::Block(b) => walk_block(emitter, b, out),
                    }
                }
            }
            _ => {}
        }
    }
    fn walk_stmt(emitter: &CEmitter, s: &Stmt, out: &mut Option<String>) {
        match s {
            Stmt::Expr(e) => walk_expr(emitter, e, out),
            Stmt::Return { value: Some(e), .. } => walk_expr(emitter, e, out),
            _ => {}
        }
    }
    fn walk_block(emitter: &CEmitter, b: &Block, out: &mut Option<String>) {
        for s in &b.stmts { walk_stmt(emitter, s, out); }
        if let Some(t) = &b.trailing { walk_expr(emitter, t, out); }
    }
    let mut out: Option<String> = None;
    match &handler.kind {
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e) => walk_expr(emitter, e, &mut out),
            ClosureBody::Block(b) => walk_block(emitter, b, &mut out),
        },
        ExprKind::ClosureFull(sb) => match &sb.body {
            FnBody::Expr(e) => walk_expr(emitter, e, &mut out),
            FnBody::Block(b) => walk_block(emitter, b, &mut out),
            FnBody::External => {}
        },
        ExprKind::Lambda { body, .. } => walk_expr(emitter, body, &mut out),
        ExprKind::HandlerLit { methods, .. } => {
            use crate::ast::HandlerMethodBody;
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(e) => walk_expr(emitter, e, &mut out),
                    HandlerMethodBody::Block(b) => walk_block(emitter, b, &mut out),
                }
                if out.is_some() { break; }
            }
        }
        _ => {}
    }
    out
}

/// Plan 39 Issue A: pick category from C type string.
pub fn with_result_category(c_type: &str) -> WithResultCategory {
    let t = c_type.trim();
    if t == "nova_unit" || t == "void" || t.is_empty() {
        return WithResultCategory::UnitVoid;
    }
    if t == "nova_int" || t == "nova_bool" || t == "nova_byte"
        || t == "nova_char" || t == "nova_i8" || t == "nova_i16"
        || t == "nova_i32" || t == "nova_i64"
        || t == "nova_u8" || t == "nova_u16" || t == "nova_u32"
        || t == "nova_u64" || t == "nova_f32" || t == "nova_f64"
    {
        return WithResultCategory::IntLike;
    }
    if t.contains('*') {
        return WithResultCategory::Pointer;
    }
    // String types are wrappers (nova_str is a struct by value).
    // Value structs: NovaOpt_X, NovaResult_X_Y, tuples, user-defined records by value.
    WithResultCategory::ValueStruct
}

pub struct CEmitter {
    out: String,
    /// File-scope handler impl function bodies (ctx structs + forward decls + bodies)
    deferred_impls: String,
    /// File-scope lambda forward declarations (static fn sig only). Flushed before fn definitions.
    lambda_forward_decls: String,
    /// Plan 36 followup: user-type forward decls (`typedef struct Nova_T Nova_T;`).
    /// Splice'ятся в `/*__USER_TYPE_FWD_DECLS__*/` ДО NovaOpt typedef'ов,
    /// чтобы `NovaOpt_Nova_T_p { Nova_T* value; }` не падал с
    /// `unknown type name 'Nova_T'`. Fills'ится pre-pass'ом в emit_module.
    user_type_fwd_decls: String,
    /// File-scope lambda implementations (structs + function bodies). Flushed before fn definitions.
    lambda_impls: String,
    indent: usize,
    tmp_counter: usize,
    /// Monotonic counter for handler literals — used to generate stable, predictable IDs
    handler_counter: usize,
    /// Monotonic counter for spawn expressions — stable IDs for pre-scan matching
    spawn_counter: usize,
    /// Monotonic counter for supervised scopes — used to name local NovaFiberQueue variables.
    supervised_counter: usize,
    /// When inside a `supervised { }` scope, holds the C name of the local NovaFiberQueue.
    /// `spawn` inside such a scope goes into the queue via nova_fiber_spawn_into.
    /// Outside a scope, this is None and `spawn` uses the eager-blocking nova_fiber_run path.
    current_scope_queue: Option<String>,
    /// When emitting a spawn-entry body, captures are accessed via `(*_c->name)`.
    /// We DON'T use `#define` macros for this — they leak into nested supervised
    /// scopes where `name` could appear as a struct field-declarator and get
    /// rewritten by the preprocessor. Instead, ExprKind::Ident checks this set
    /// and rewrites references inline.
    current_spawn_captures: Option<HashSet<String>>,
    /// Subset of current_spawn_captures that are captured by value (T field, not T*).
    /// These names rewrite to `_c->name`, not `(*_c->name)`.
    current_spawn_capture_by_value: Option<HashSet<String>>,
    /// Maps variable name → C type string (best-effort)
    var_types: HashMap<String, String>,
    /// Names of variables declared as `let mut` (mutable) — used by spawn-capture
    /// to decide between copy-by-value (immutable scalar) and capture-by-pointer.
    var_mutable: HashSet<String>,
    /// Maps struct name → field name → C type
    record_schemas: HashMap<String, HashMap<String, String>>,
    /// Maps sum type name → variant name → field types (positional)
    sum_schemas: HashMap<String, HashMap<String, Vec<String>>>,
    /// Maps effect name → method name → (param_types, return_type)
    effect_schemas: HashMap<String, HashMap<String, (Vec<String>, String)>>,
    /// Maps method name → (type_name, is_instance) for user-defined methods.
    /// Used at call sites to resolve `obj.method(args)` → `Nova_T_method_m(obj, args)`.
    method_receivers: HashMap<String, (String, bool)>,
    /// Plan 06 Ф.1: multi-key registry — `(type_name, method_name) → is_instance`.
    /// `method_receivers` single-key страдает от last-wins (если два типа имеют
    /// одноимённый method, второй вытесняет первый). `all_methods` хранит все.
    /// Используется в for-in для Iter[T] dispatch: проверяем
    /// `all_methods.contains((iter_struct, "next"))`.
    all_methods: HashSet<(String, String)>,
    /// Plan 11 Ф.1: multi-overload registry. Key = `(type_name, method_name)`,
    /// value = list of overloaded signatures (param C types, is_instance,
    /// is_external). Используется на call-site для resolve по arg-types
    /// (Ф.2). Single-key `method_receivers` остаётся для backward compat —
    /// single-overload пути ссылаются на него.
    method_overloads: HashMap<(String, String), Vec<MethodSig>>,
    /// Plan 12: builtins.nv-driven external dispatch registry.
    /// Single source of truth для StringBuilder/WriteBuffer/ReadBuffer/
    /// str.from(char) — `std/runtime/builtins.nv`. Codegen читает AST
    /// и применяет mangling/type-mapping автоматически (вместо hard-coded
    /// таблиц). Загружается один раз в `CEmitter::new()`.
    pub external_registry: super::external_registry::ExternalRegistry,
    /// D39 / Plan 11 Ф.9: embed-поля per record-type.
    /// Key = wrapper type name; value = list of (field_name, embedded_type_name,
    /// is_anonymous). Используется для auto-proxy generation после AST-walk fn-items.
    embed_fields: HashMap<String, Vec<(String, String, bool)>>,
    /// Plan 06 Ф.3: для каждого типа Coll с методом `mut @iter() -> IterT`
    /// запоминаем имя IterT. Используется в for-in: при `for x in coll`
    /// (где `coll: Coll`) вставляем implicit `.iter()` и emit'им loop
    /// против IterT.
    iter_returns: HashMap<String, String>,
    /// D73 v2 auto-derive: target_type → list of source_types for which
    /// `target.from(src V)` is explicitly defined. Used to synthesize
    /// `v.into()` for V via target.from when no explicit `@into` exists.
    from_targets: HashMap<String, Vec<String>>,
    /// Plan 08 Ф.3: try_from/try_into registries (D77 4-way auto-derive).
    /// Параллельно with from_targets/into_targets.
    try_from_targets: HashMap<String, Vec<String>>,
    try_into_targets: HashMap<String, String>,
    /// D73 v2 auto-derive: source_type → target_type for which
    /// `fn V @into() -> T` is explicitly defined. Used to synthesize
    /// `T.from(v)` via v.into() when no explicit `T.from` exists.
    into_targets: HashMap<String, String>,
    /// Maps tuple variable name → per-element C types.
    /// Used at field access `pair.0` to cast back to the original element type when needed.
    tuple_element_types: HashMap<String, Vec<String>>,
    /// Maps (type_name, variant_name, field_name) key → C type for record variants.
    /// Key format: "TypeName::VariantName::field_name"
    record_variant_field_types: HashMap<String, String>,
    /// Maps "TypeName::VariantName" → ordered list of field names (insertion order).
    record_variant_field_order: HashMap<String, Vec<String>>,
    /// Return type of the currently-emitting function, used for match result type inference.
    current_fn_return_ty: Option<String>,
    /// Plan 33.3 Ф.9.2 (D24): record-invariants per-type. Map struct_name →
    /// list of (invariant-expr, span). Используется emit_record_lit'ом для
    /// wrap'а конструкции в runtime-check (`if (!Inv(tmp)) violation; tmp`).
    /// Заполняется в emit_module pre-pass.
    record_invariants: HashMap<String, Vec<(Expr, Span)>>,
    /// Plan 33.1 Ф.4 (D24): если установлено — функция имеет ensures-контракты,
    /// и все `Stmt::Return X` подменяются на `{ _nova_result = X; goto <label>; }`.
    /// Trailing block-expression также. После label эмитятся ensures-checks
    /// и финальный return.
    contracts_post_label: Option<String>,
    /// Plan 33.3 Ф.9.1+Ф.9.7: имена ghost-vars в scope текущей fn.
    /// Используется в Stmt::AssertStatic / Stmt::Assume / inject'нутых
    /// loop invariants — если expression читает ghost-var, runtime check
    /// skip'ается (ghost эрейзится в codegen, не доступен в C-output).
    /// Type-check уже catches non-ghost reads ghost (Ф.9.7); это просто
    /// allow spec-position reads silently не падать на C-level.
    ghost_vars: std::collections::HashSet<String>,
    /// Plan 33.3 Ф.9.9 (D24): proven контракты (fn_name, span.start).
    /// Заполняется через set_proven_contracts из VerificationPipeline result.
    /// Codegen skip emit для runtime check'ов помеченных как proven —
    /// true zero-cost даже в debug.
    proven_contracts: std::collections::HashSet<(String, usize)>,
    /// Maps array variable name → actual element C type (e.g. "Nova_Box*").
    /// The array always uses nova_int storage but elements may be pointers to records.
    array_element_types: HashMap<String, String>,
    /// Maps Option variable name → inner boxed type when value is a heap-boxed struct pointer.
    /// E.g. "outer" → "NovaOpt_nova_int*" when outer = Some(Some(42)).
    option_inner_types: HashMap<String, String>,
    /// Set during emit_call when boxing a struct for nova_make_Option_Some.
    /// Consumed by next Stmt::Let to annotate the bound variable's inner type.
    pending_option_inner_type: Option<String>,
    /// Set of array variable names that store boxed nova_str* (as nova_int).
    /// Index access on these arrays must dereference: *(nova_str*)(arr->data[i]).
    str_box_arrays: HashSet<String>,
    /// C type of the current method receiver (e.g. "Nova_Box"), for resolving `Self`.
    current_receiver_type: Option<String>,
    /// Expected struct type для anonymous record literal `=> { ... }` —
    /// устанавливается при эмите function body, когда нужно использовать
    /// declared return type как target для anonymous record (D55).
    expected_record_type: Option<String>,
    /// Hint for empty/uninferable array literals: element C type (e.g. "nova_str").
    /// Set when target type is NovaArray_X* so `[]` emits nova_array_new_X not nova_int.
    current_array_elem_hint: Option<String>,
    /// Maps local variable name → (param_c_types, return_c_type) for function-typed parameters.
    /// Used to emit proper function pointer calls for `body(args)` where body is a fn param.
    fn_param_sigs: HashMap<String, (Vec<String>, String)>,
    /// Plan 55 Ф.1: maps variable name of `[]fn(P...) -> R` type → element closure
    /// signature `(P_c_tys, R_c_ty)`. Used in `emit_for` so that `for f in fns { f() }`
    /// can register the loop binding in `fn_param_sigs` and route `f()` through
    /// `NOVA_CLOS_CALL_*` / `NovaClosBase` dispatch instead of treating `f` as a free
    /// function name (which previously emitted undefined `nova_fn_f()`).
    array_param_fn_sigs: HashMap<String, (Vec<String>, String)>,
    /// Plan 14 Ф.3: signature of every top-level user fn (`fn name(...)`).
    /// Используется для emit free-fn-as-value: при `let f = inc` или
    /// `xs.map(inc)` — нужно знать sig чтобы построить thunk и closure.
    /// Обновляется при register_fn в первом проходе.
    user_fn_sigs: HashMap<String, (Vec<String>, String)>,
    /// Bidirectional inference: maps (callee_name, param_index) → inner closure
    /// signature (param_types, ret_type) when the HOF parameter at that position
    /// has type `fn(T...) -> R`. Populated during register_fn pass; consulted in
    /// emit_call when a ClosureLight argument needs its parameter types inferred.
    hof_param_fn_sigs: HashMap<(String, usize), (Vec<String>, String)>,
    /// Plan 14 Ф.6 (D69): set of variadic-fn names. На call-site
    /// `emit_call` для имени из этого set'а собирает args[N-1..] в
    /// синтезированный ArrayLit и передаёт как последний аргумент.
    user_fn_variadic: HashSet<String>,
    /// Plan 14 Ф.6: guard от infinite-recursion в `emit_call`. После
    /// преобразования variadic args → ArrayLit мы recurse'имся в
    /// `emit_call` с новыми args; флаг говорит recursion'у пропустить
    /// variadic-routing-check (он уже сделан).
    suppress_variadic_routing: bool,
    /// Plan 14 Ф.3: имена user fn'ов, для которых уже эмитнут thunk
    /// (envless adapter `static <ret> nova_fn_<name>_thunk(void* env, args)
    /// { return nova_fn_<name>(args); }`). Дедупликация — несколько
    /// references к одной fn делят один thunk.
    emitted_fn_thunks: HashSet<String>,
    /// Plan 14 Ф.2: имена const'ов с runtime-init (record-литерал, call,
    /// и т.д.). На use-site `Ident(name)` для них эмитится `nova_const_<name>()`
    /// (lazy-init геттер) вместо имени переменной. Тип сохраняется в
    /// `var_types[name]` (как для обычных const'ов).
    lazy_consts: HashSet<String>,
    /// Plan 14 Ф.4: fn-typed поля record'ов — `(record_name, field_name)
    /// → (param_c_tys, ret_c_ty)`. Заполняется при `emit_type_decl`
    /// для record-полей с TypeRef::Func. Используется в Member-call
    /// для routing'а через `NOVA_CLOS_CALL_*`.
    record_field_fn_sigs: HashMap<(String, String), (Vec<String>, String)>,
    /// Monotonic counter for trailing block functions — generates unique names.
    trailing_block_counter: usize,
    /// Counter for lambda/closure static functions.
    lambda_counter: usize,
    /// Maps function name → (param_c_tys, ret_c_ty) when the function returns a fn(...) type.
    /// Used to register let-bindings of function-call results in fn_param_sigs.
    fn_returns_fn_sig: HashMap<String, (Vec<String>, String)>,
    /// Set of function names that are generic (have type parameters).
    /// Generic functions are emitted with void* erasure; call sites must box/unbox.
    generic_fns: HashSet<String>,
    /// Set of type names that are generic (have type parameters).
    /// Methods on these types have void*-erased params; call sites must box/unbox.
    generic_types: HashSet<String>,
    /// Maps generic function name → tuple arity when the function returns a tuple of type params.
    /// Used to populate tuple_element_types at call sites.
    generic_fn_tuple_arity: HashMap<String, usize>,
    /// Maps type alias name → resolved C type string (e.g. "Name" → "nova_str").
    /// Type aliases don't use pointer indirection; their C type is used directly.
    type_aliases: HashMap<String, String>,
    /// D71 `parallel for → []T` mode. When Some, the next `emit_spawn` writes its
    /// trailing-expression value into `result[idx]` instead of discarding.
    /// Tuple: (idx_var_name, result_var_name, element_c_type).
    current_parfor_slot: Option<(String, String, String)>,
    /// Optional Nova source text — when Some, используется для (1) line:col
    /// в codegen-ошибках (Plan 14 std-fix) и (2) `/* SRC: ... */` комментов
    /// при `annotation_enabled=true`. Set via `--annotate-source` CLI flag
    /// активирует комментарии; источник передаётся всегда.
    annotation_source: Option<String>,
    /// Plan 14 std-fix: контролирует только эмит SRC-комментариев. Source
    /// в `annotation_source` остаётся для line:col в ошибках.
    annotation_enabled: bool,
    /// Plan 14 Ф.1: typedef'ы NovaOpt_<T> для T без NOVA_ARRAY_DECL в
    /// runtime — эмитятся лениво при первом упоминании в type_ref_to_c.
    ///
    /// Buffer накапливает строки typedef'ов в registration order
    /// (нижние слои — innermost — регистрируются первыми, что даёт
    /// правильный topological order: NovaOpt_X должен быть до
    /// NovaOpt_NovaOpt_X в файле).
    ///
    /// На preamble эмитится маркер `/*__NOVAOPT_TYPEDEFS__*/`. После
    /// полного emit_module маркер заменяется содержимым буфера —
    /// типы попадают в file scope сразу после tuple-typedef'ов.
    ///
    /// Interior mutability: используется из `&self`-методов
    /// (type_ref_to_c, infer_expr_c_type).
    novaopt_typedefs_buf: std::cell::RefCell<String>,
    /// Set sanitized-имён NovaOpt_<X> которые уже эмитированы в
    /// `novaopt_typedefs_buf` (для dedup'а). Pre-populated в `new()`
    /// из `NOVA_ARRAY_DECL` списка в `nova_rt/array.h` — runtime их
    /// уже даёт, не нужен duplicate typedef.
    novaopt_decls_seen: std::cell::RefCell<std::collections::HashSet<String>>,
    /// Plan 54 Ф.9: sanitized NovaOpt-id → real C-type значения. Нужно
    /// чтобы pattern_bind_typed для `Some(v) => v` где scrutinee
    /// `NovaOpt_Nova_X_p` (sanitized) восстановил correct `v` тип
    /// `Nova_X*` (не sanitized "Nova_X_p"). Без map'a `t_from_scr` =
    /// strip("NovaOpt_") даёт sanitized, что breaks pointer types.
    novaopt_value_types: std::cell::RefCell<std::collections::HashMap<String, String>>,
    /// Accumulated lint warnings from codegen (e.g. anonymous-embed override).
    /// Returned from emit_module instead of printed directly to stderr,
    /// so test runner can route them to captured_stderr rather than leaking
    /// to the terminal.
    warnings: Vec<String>,
    /// Plan 20 Ф.4: stack of active defer/errdefer scopes during emission.
    /// Each block that contains a `defer`/`errdefer` stmt pushes a `DeferScope`
    /// on entry and pops on exit. `Stmt::Return`/`Break`/`Continue` walk the
    /// stack to invoke pending defers in LIFO before the actual jump.
    /// `errdefer` cleanup is gated on a per-scope `is_error` flag set by
    /// `setjmp`-handled fail-frame.
    defer_scopes: Vec<DeferScope>,
    /// Plan 20 Ф.4: monotonic block-ID counter for stable, unique C names
    /// (`_defer_<BLKID>_<N>_active`, `_defer_cleanup_<BLKID>`, etc.).
    defer_block_counter: usize,
    /// Closure mut-capture heap-box registry. Maps variable name → C box-pointer
    /// variable name (`_box_<name>`). When a mut local is captured by a closure,
    /// it is heap-promoted: a `T* _box_x = nova_alloc(sizeof(T)); *_box_x = x;`
    /// is emitted, followed by `#define x (*_box_x)` so all subsequent caller-side
    /// reads/writes go through the box. The closure env stores `_box_x` directly
    /// (no dangling-ptr risk on escape). Cleared and #undef'd at function exit.
    var_boxed: HashMap<String, String>,
    /// Plan 48: generic FnDecls for monomorphization worklist drain.
    /// Key = Nova fn name (e.g. "within"). Populated during pre-pass.
    mono_fn_decls: HashMap<String, crate::ast::FnDecl>,
    /// Plan 48: generic instance-method FnDecls for method monomorphization.
    /// Key = (receiver_type_name, method_name). Methods with own type params (e.g. @execute[T,E]).
    mono_method_decls: HashMap<(String, String), crate::ast::FnDecl>,
    /// Plan 48: monomorphization worklist — (nova_fn_name, type_subst, mangled_c_name).
    mono_worklist: Vec<(String, Vec<(String, String)>, String)>,
    /// Plan 48: already-instantiated mangled names (for dedup).
    mono_instantiated: HashSet<String>,
    /// Plan 48: active type substitution during monomorphized fn emission.
    /// Maps type_param_name → concrete C type. Set/cleared around emit_monomorphized_fn.
    current_type_subst: HashMap<String, String>,
    /// Plan 48: forward declarations for monomorphized functions.
    /// Spliced into output via /*__MONO_FWD_DECLS__*/ marker.
    mono_fwd_decls: String,
    /// Plan 48 Ф.3: template declarations for generic types (record/sum).
    /// Stored here instead of immediately emitting — instantiated lazily per usage.
    generic_type_templates: HashMap<String, crate::ast::TypeDecl>,
    /// Plan 48 Ф.3: worklist for lazy generic type instance emission.
    /// Each entry: (base_type_name, type_args_c, mangled_name).
    /// Uses RefCell so type_ref_to_c (&self) can enqueue instances.
    generic_type_worklist: std::cell::RefCell<Vec<(String, Vec<String>, String)>>,
    /// Plan 48 Ф.3: already-emitted generic type instances (by mangled name).
    emitted_generic_type_instances: HashSet<String>,
    /// Plan 48 Ф.3: methods per generic type template.
    /// Key = base type name, value = Vec of FnDecl for that type's methods.
    generic_type_methods: HashMap<String, Vec<crate::ast::FnDecl>>,
    /// Plan 48 Ф.3: buffer for generic type instance definitions.
    /// Emitted separately and spliced into output before fn definitions via marker.
    generic_type_defs_buf: String,
    /// Plan 48 Ф.3: mangled type name → (base_type_name, type_args_c).
    /// Uses RefCell so type_ref_to_c (&self) can register instances.
    generic_type_instance_info: std::cell::RefCell<HashMap<String, (String, Vec<String>)>>,
    /// Plan 48 Ф.7.6: maximum monomorphization-worklist drain depth.
    /// Default 500; overridable via CLI `--mono-depth=N` or env var
    /// `NOVA_MONO_DEPTH` (CLI wins). Guards against polymorphic recursion.
    mono_depth_limit: usize,
    /// Plan 49 Ф.6 P0 fix: per-variable Nova-level CancelToken[T] tracking.
    /// key = local variable name (`tok`), value = T's C-type (`nova_int`).
    /// Populated при `let tok CancelToken[T] = ...` или `let tok = CancelToken[T].new()`.
    /// Использовано emit_call для `tok.reason()` — emit'ит per-T un-box вместо
    /// runtime-fixed `nova_cancel_token_reason_str` (которая молча возвращает
    /// garbage для T≠str). Без entry — default str-form (backward compat).
    cancel_token_t_map: HashMap<String, String>,
    /// Plan 49 Ф.6 cross-type cascade: module-wide dedup для converter
    /// wrappers (`_nova_cancel_conv_<A>_from_<B>`). lambda_impls очищается
    /// между fn-bodies, поэтому single per-test contains-check НЕ ловит
    /// re-emit между tests. Этот set tracks все уже emitted wrappers.
    emitted_cancel_converters: HashSet<String>,
}

/// Plan 20 Ф.4: per-defer-stmt entry — tracks one `defer { ... }` or
/// `errdefer { ... }` statement registered inside a block.
struct DeferEntry {
    /// C variable name of the `int` activation flag. Initialized to 0
    /// at block start; set to 1 inline at the defer's textual position
    /// (so partial-init exits run only defers that already executed).
    active_var: String,
    /// `true` if this is `errdefer` (runs only on error-exit), `false`
    /// for plain `defer` (runs on every exit path).
    is_errdefer: bool,
    /// AST body to re-emit at cleanup point. AST stores defer body as
    /// arbitrary `Expr` (parser wraps `defer { ... }` in ExprKind::Block).
    body: Expr,
}

/// Plan 20 Ф.4: per-block defer state. One scope per block that contains
/// at least one defer/errdefer.
struct DeferScope {
    /// Unique block ID for naming.
    block_id: usize,
    /// All defer/errdefer entries registered in this block, in textual order.
    /// Cleanup walks this in reverse for LIFO semantics.
    entries: Vec<DeferEntry>,
    /// Running index into `entries` — incremented each time emit_stmt
    /// reaches a Defer/ErrDefer and activates its flag.
    next_idx: usize,
    /// `true` if the scope has at least one `errdefer` — triggers
    /// NovaFailFrame setjmp wrapper so throw-path can detect error exit.
    needs_failframe: bool,
    /// Name of the C NovaFailFrame variable when `needs_failframe`.
    failframe_var: String,
    /// Name of the C `int` "fail-frame popped" flag (set to 1 by
    /// early-exit cleanup so leave_defer_scope skips the second
    /// `nova_fail_pop()`). Valid when `needs_failframe`.
    failframe_popped_var: String,
    /// Plan 20 Ф.8 (2): name of C NovaInterruptFrame variable. Always
    /// present when any defer is in the block — defers run on interrupt-
    /// path тоже (D90 п.8).
    intframe_var: String,
    /// Plan 20 Ф.8 (2): name of C `int` "interrupt-frame popped" flag
    /// (set to 1 by early-exit cleanup / interrupt-path handler).
    intframe_popped_var: String,
    /// `true` if this scope is loop-body — break/continue stop here
    /// rather than walking outer scopes.
    is_loop_body: bool,
}

impl CEmitter {
    pub fn new() -> Self {
        Self {
            out: String::new(),
            deferred_impls: String::new(),
            lambda_forward_decls: String::new(),
            user_type_fwd_decls: String::new(),
            lambda_impls: String::new(),
            indent: 0,
            tmp_counter: 0,
            handler_counter: 0,
            spawn_counter: 0,
            supervised_counter: 0,
            current_scope_queue: None,
            current_spawn_captures: None,
            current_spawn_capture_by_value: None,
            var_types: HashMap::new(),
            var_mutable: HashSet::new(),
            record_schemas: HashMap::new(),
            sum_schemas: HashMap::new(),
            effect_schemas: HashMap::new(),
            method_receivers: HashMap::new(),
            method_overloads: HashMap::new(),
            external_registry: super::external_registry::ExternalRegistry::load_builtins()
                .expect("failed to load std/runtime/*.nv (Plan 13 Ф.8)"),
            embed_fields: HashMap::new(),
            all_methods: HashSet::new(),
            iter_returns: HashMap::new(),
            from_targets: HashMap::new(),
            into_targets: HashMap::new(),
            try_from_targets: HashMap::new(),
            try_into_targets: HashMap::new(),
            tuple_element_types: HashMap::new(),
            record_variant_field_types: HashMap::new(),
            record_variant_field_order: HashMap::new(),
            current_fn_return_ty: None,
            contracts_post_label: None,
            ghost_vars: std::collections::HashSet::new(),
            proven_contracts: std::collections::HashSet::new(),
            record_invariants: HashMap::new(),
            array_element_types: HashMap::new(),
            option_inner_types: HashMap::new(),
            pending_option_inner_type: None,
            str_box_arrays: HashSet::new(),
            current_receiver_type: None,
            expected_record_type: None,
            current_array_elem_hint: None,
            fn_param_sigs: HashMap::new(),
            array_param_fn_sigs: HashMap::new(),
            user_fn_sigs: HashMap::new(),
            hof_param_fn_sigs: HashMap::new(),
            user_fn_variadic: HashSet::new(),
            suppress_variadic_routing: false,
            emitted_fn_thunks: HashSet::new(),
            lazy_consts: HashSet::new(),
            record_field_fn_sigs: HashMap::new(),
            trailing_block_counter: 0,
            lambda_counter: 0,
            fn_returns_fn_sig: HashMap::new(),
            generic_fns: HashSet::new(),
            generic_types: HashSet::new(),
            generic_fn_tuple_arity: HashMap::new(),
            type_aliases: HashMap::new(),
            current_parfor_slot: None,
            annotation_source: None,
            annotation_enabled: false,
            // Plan 14 Ф.1: NovaOpt_<T> lazy-decl. Pre-populated с T-ками,
            // которые `nova_rt/array.h` уже декларирует через NOVA_ARRAY_DECL.
            // Для прочих — typedef эмитится в novaopt_typedefs_buf и
            // splice'ится через маркер /*__NOVAOPT_TYPEDEFS__*/.
            novaopt_typedefs_buf: std::cell::RefCell::new(String::new()),
            novaopt_decls_seen: {
                let mut s = std::collections::HashSet::new();
                s.insert("nova_int".to_string());
                s.insert("nova_byte".to_string());
                s.insert("nova_bool".to_string());
                s.insert("nova_str".to_string());
                s.insert("nova_f64".to_string());
                std::cell::RefCell::new(s)
            },
            // Plan 54 Ф.9: pre-populated primitive sanitized → c_ty
            // pairs (для них sanitized совпадает с c_ty).
            novaopt_value_types: {
                let mut m = std::collections::HashMap::new();
                for t in ["nova_int", "nova_byte", "nova_bool", "nova_str", "nova_f64"] {
                    m.insert(t.to_string(), t.to_string());
                }
                std::cell::RefCell::new(m)
            },
            defer_scopes: Vec::new(),
            defer_block_counter: 0,
            var_boxed: HashMap::new(),
            warnings: Vec::new(),
            mono_fn_decls: HashMap::new(),
            mono_method_decls: HashMap::new(),
            mono_worklist: Vec::new(),
            mono_instantiated: HashSet::new(),
            current_type_subst: HashMap::new(),
            mono_fwd_decls: String::new(),
            generic_type_templates: HashMap::new(),
            generic_type_worklist: std::cell::RefCell::new(Vec::new()),
            emitted_generic_type_instances: HashSet::new(),
            generic_type_methods: HashMap::new(),
            generic_type_defs_buf: String::new(),
            generic_type_instance_info: std::cell::RefCell::new(HashMap::new()),
            // Plan 48 Ф.7.6: NOVA_MONO_DEPTH env var still honored as a
            // fallback; CLI `--mono-depth=N` overrides via set_mono_depth_limit.
            mono_depth_limit: std::env::var("NOVA_MONO_DEPTH").ok()
                .and_then(|s| s.parse::<usize>().ok())
                .filter(|n| *n > 0)
                .unwrap_or(500),
            cancel_token_t_map: HashMap::new(),
            emitted_cancel_converters: HashSet::new(),
        }
    }

    /// Plan 48 Ф.7.6: CLI override for the monomorphization-worklist drain
    /// depth limit. Called from `nova build` / `nova test` / `nova test-build`
    /// when `--mono-depth=N` is set; takes priority over `NOVA_MONO_DEPTH`.
    pub fn set_mono_depth_limit(&mut self, n: usize) {
        if n > 0 { self.mono_depth_limit = n; }
    }

    /// Enable source-annotation mode: codegen will insert `/* SRC: ... */`
    /// comments before each statement showing the originating Nova source.
    /// On by default since Plan 14 std-fix (нужно для line:col в ошибках);
    /// SRC-комментарии в C-output контролируются `annotation_enabled` flag.
    pub fn set_source_for_annotations(&mut self, src: String) {
        self.annotation_source = Some(src);
        self.annotation_enabled = true;
    }

    /// Plan 14 std-fix: выключает SRC-комментарии но оставляет source для
    /// line:col в codegen-ошибках. Вызывается main.rs когда `--annotate-source`
    /// не передан.
    pub fn disable_source_annotations(&mut self) {
        self.annotation_enabled = false;
    }

    /// Plan 33.3 Ф.9.9 (D24): передать список доказанных контрактов от
    /// VerificationPipeline. Codegen для proven контрактов **не эмитит**
    /// runtime check даже в debug — true zero-cost для доказанного.
    /// Key: (fn_name, contract span start byte offset).
    pub fn set_proven_contracts(&mut self, proven: &[(String, crate::diag::Span)]) {
        self.proven_contracts.clear();
        for (name, span) in proven {
            self.proven_contracts.insert((name.clone(), span.start));
        }
    }

    /// Get the Span of a statement (where in source it came from).
    fn stmt_span(stmt: &Stmt) -> Span {
        match stmt {
            Stmt::Let(d) => d.span,
            Stmt::Expr(e) => e.span,
            Stmt::Assign { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Throw { span, .. }
            | Stmt::Defer { span, .. }
            | Stmt::ErrDefer { span, .. }
            | Stmt::AssertStatic { span, .. }
            | Stmt::Assume { span, .. }
            | Stmt::Apply { span, .. }
            | Stmt::Calc { span, .. } => *span,
            Stmt::Break(s) | Stmt::Continue(s) => *s,
        }
    }

    /// If source-annotation mode is on, emit `/* SRC: <line> */` before the
    /// statement's C code. Multi-line statements get just the first line +
    /// `…` ellipsis to keep .c readable.
    fn emit_source_annotation_for_stmt(&mut self, stmt: &Stmt) {
        self.emit_source_annotation_for_span(Self::stmt_span(stmt));
    }

    /// Same as `..._for_stmt` but for trailing-expression of a block (which
    /// parser routes into `block.trailing` instead of `block.stmts`).
    fn emit_source_annotation_for_expr(&mut self, expr: &Expr) {
        self.emit_source_annotation_for_span(expr.span);
    }

    fn emit_source_annotation_for_span(&mut self, span: Span) {
        // Plan 14 std-fix: SRC-комментарии теперь управляются отдельно
        // от наличия source (source нужен для line:col в ошибках).
        if !self.annotation_enabled { return; }
        let Some(src) = self.annotation_source.clone() else { return; };
        let snippet = src
            .get(span.start..span.end)
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .trim();
        if snippet.is_empty() {
            return;
        }
        // Sanitize for C comment: replace `*/` sequences with `* /` to avoid
        // closing the comment prematurely. Lone `*` and `/` are kept (so
        // multiplication and division operators читаемы). Truncate long lines.
        let truncated: String = snippet.chars().take(120).collect();
        let suffix = if snippet.chars().count() > truncated.chars().count() {
            " …"
        } else {
            ""
        };
        let safe = truncated.replace("*/", "* /");
        self.line(&format!("/* SRC: {}{} */", safe, suffix));
    }

    pub fn emit_module(mut self, module: &Module) -> Result<(String, Vec<String>), String> {
        // Plan 33.3 Ф.9.2 (D24): pre-pass — собрать invariants для record-типов.
        // Used в emit_record_lit для wrap'а конструкции в runtime-check.
        for item in &module.items {
            if let Item::Type(td) = item {
                if !td.invariants.is_empty() {
                    let invs: Vec<(Expr, Span)> = td.invariants.iter()
                        .map(|c| (c.expr.clone(), c.span)).collect();
                    self.record_invariants.insert(td.name.clone(), invs);
                }
            }
        }
        // Pre-populate sum_schemas with built-in Option and Result types.
        {
            let mut opt_variants = HashMap::new();
            opt_variants.insert("Some".to_string(), vec!["nova_int".to_string()]);
            opt_variants.insert("None".to_string(), vec![]);
            self.sum_schemas.insert("Option".to_string(), opt_variants.clone());
            self.sum_schemas.insert("NovaOpt_nova_int".to_string(), opt_variants);

            let mut res_variants = HashMap::new();
            res_variants.insert("Ok".to_string(), vec!["nova_int".to_string()]);
            res_variants.insert("Err".to_string(), vec!["nova_str".to_string()]);
            self.sum_schemas.insert("Result".to_string(), res_variants);
        }

        // D26 prelude: Error — record для quick errors с msg.
        // Декларирован в spec/decisions/08-runtime.md; runtime тип
        // в nova_rt/array.h (Nova_Error). Регистрируем schema чтобы
        // codegen видел Nova_Error*.msg как nova_str и эмитил
        // Nova_Error_static_new для Error.new(...).
        {
            let mut err_schema = HashMap::new();
            err_schema.insert("msg".to_string(), "nova_str".to_string());
            self.record_schemas.insert("Error".to_string(), err_schema);
            self.method_receivers.insert(
                "new".to_string(),
                ("Error".to_string(), false),
            );
        }

        // Plan 53 Ф.6.4: register Nova_ChannelPair schema so the generic
        // `pattern_bind_typed` path can destructure `let { tx, rx } = Channel.new(cap)`
        // without a hardcoded special-case. `Nova_ChannelPair` is a C-runtime
        // value-type (declared in nova_rt/channel.h) — schema mirrors the C
        // struct layout. `is_value_type` already returns true for it so the
        // generic path uses `.` accessor.
        {
            let mut cp_schema = HashMap::new();
            cp_schema.insert("tx".to_string(), "Nova_ChanWriter*".to_string());
            cp_schema.insert("rx".to_string(), "Nova_ChanReader*".to_string());
            self.record_schemas.insert("ChannelPair".to_string(), cp_schema);
        }

        // D26 prelude: RuntimeError — sum-тип встроенных runtime-сбоев.
        // Variants (D65): DivByZero, Overflow, IndexOutOfBounds {index,length},
        // TypeMismatch(str), AssertFailed(str), NoHandler(str).
        // Конструкторы — `nova_make_RuntimeError_<Variant>` в nova_rt/array.h.
        {
            let mut rt_variants: HashMap<String, Vec<String>> = HashMap::new();
            rt_variants.insert("DivByZero".to_string(), vec![]);
            rt_variants.insert("Overflow".to_string(), vec![]);
            rt_variants.insert("IndexOutOfBounds".to_string(),
                vec!["nova_int".to_string(), "nova_int".to_string()]);
            rt_variants.insert("TypeMismatch".to_string(), vec!["nova_str".to_string()]);
            rt_variants.insert("AssertFailed".to_string(), vec!["nova_str".to_string()]);
            rt_variants.insert("NoHandler".to_string(), vec!["nova_str".to_string()]);
            self.sum_schemas.insert("RuntimeError".to_string(), rt_variants);
            // IndexOutOfBounds — record-variant; field order для constructor.
            self.record_variant_field_order.insert(
                "RuntimeError::IndexOutOfBounds".to_string(),
                vec!["index".to_string(), "length".to_string()],
            );
            self.record_variant_field_types.insert(
                "RuntimeError::IndexOutOfBounds::index".to_string(), "nova_int".into());
            self.record_variant_field_types.insert(
                "RuntimeError::IndexOutOfBounds::length".to_string(), "nova_int".into());
        }

        // Pre-register Fail as a built-in effect (D25 / D62 / D65).
        // Operation: `fail(msg str) -> nova_unit`. `throw expr` desugars to
        // `Fail.fail(expr)` — same dispatch path as any other effect operation.
        // Default handler installed by runtime (Nova_Fail_fail) calls nova_throw,
        // which longjmp's to nearest fail-frame (test_frame or spawn-entry frame).
        // User can override via `with Fail = (msg) => ... { body }` (D31 sugar).
        {
            let mut fail_schema: HashMap<String, (Vec<String>, String)> = HashMap::new();
            fail_schema.insert("fail".to_string(), (vec!["nova_str".into()], "nova_unit".into()));
            self.effect_schemas.insert("Fail".to_string(), fail_schema);
        }

        // Pre-register Time as a built-in effect (D11 / D14 / D62).
        // Operations: `now() -> int` (monotonic ms), `sleep(ms int) -> unit`
        // (yields/sleeps depending on context — see fibers.h). User override
        // via `with Time = handler Time { sleep(ms) {...} now() {...} } { body }`.
        {
            let mut time_schema: HashMap<String, (Vec<String>, String)> = HashMap::new();
            time_schema.insert("sleep".to_string(), (vec!["nova_int".into()],    "nova_unit".into()));
            time_schema.insert("now".to_string(),   (vec![],                      "nova_int".into()));
            time_schema.insert("after".to_string(), (vec!["nova_int".into()],    "Nova_ChanReader*".into()));
            self.effect_schemas.insert("Time".to_string(), time_schema);
        }

        // Pre-register Mem as a built-in effect for runtime introspection.
        // Operations:
        //   - alloc_count() -> int : total nova_alloc calls since gc_init/reset
        //   - free_count()  -> int : total frees (plain malloc backend → 0)
        //   - live()        -> int : alloc_count - free_count
        //   - reset()       -> unit: zero stats counters (per-test isolation)
        // Used by leak/growth tests (see nova_tests/runtime/memory_growth.nv).
        {
            let mut mem_schema: HashMap<String, (Vec<String>, String)> = HashMap::new();
            mem_schema.insert("alloc_count".to_string(), (vec![], "nova_int".into()));
            mem_schema.insert("free_count".to_string(),  (vec![], "nova_int".into()));
            mem_schema.insert("live".to_string(),        (vec![], "nova_int".into()));
            mem_schema.insert("reset".to_string(),       (vec![], "nova_unit".into()));
            self.effect_schemas.insert("Mem".to_string(), mem_schema);
        }

        // Plan 04 Этап 6: Buffer удалён из языка (REMOVED). Заменён на
        // StringBuilder/WriteBuffer/ReadBuffer split. Старая Q-buffer
        // регистрация (record_schemas + method_receivers для add_*/
        // into_str_unchecked) — удалена.

        // Plan 12: register built-in opaque types и method_receivers
        // automatically из ExternalRegistry (single source of truth —
        // std/runtime/builtins.nv). Hard-coded таблицы для StringBuilder/
        // WriteBuffer/ReadBuffer удалены.
        for recv_ty in self.external_registry.receiver_types.clone() {
            // primitive str — не record, не нужен schema. Только
            // user-defined opaque types (StringBuilder/WriteBuffer/...).
            if recv_ty == "str" { continue; }
            self.record_schemas.entry(recv_ty.clone())
                .or_insert_with(HashMap::new);
        }
        // method_receivers (single-key, last-wins) — для backward compat
        // dispatch'ей которые ещё не мигрированы на multi-overload путь.
        // Plan 11 multi-overload + Plan 12 registry — основные пути; этот
        // legacy registry остаётся для conservative routing.
        //
        // NOTE: используем `entry().or_insert()` чтобы НЕ перетирать
        // existing entries из prelude (Error.new etc.). Single-key
        // registry — last-wins, но prelude занят сначала.
        for (key, decls) in self.external_registry.by_key.clone().into_iter() {
            let (recv_ty, method_name) = key;
            if recv_ty.is_empty() { continue; }     // free fns
            if let Some(decl) = decls.first() {
                self.method_receivers.entry(method_name)
                    .or_insert((recv_ty, decl.is_instance));
            }
        }

        self.emit_preamble();

        // Plan 36 followup: pre-pass — forward-decl всех user types
        // через `typedef struct Nova_T Nova_T;`. Splice'ится в маркер
        // `/*__USER_TYPE_FWD_DECLS__*/` (ставится в preamble ДО
        // `/*__NOVAOPT_TYPEDEFS__*/`). Без этого NovaOpt_<T> typedef'ы
        // ссылаются на не-объявленный `Nova_T` (NovaOpt splice'ится
        // ПЕРЕД emit_type_decl).

        // Collect locally-defined type names first.
        let local_types: HashSet<String> = module.items.iter()
            .filter_map(|i| if let Item::Type(t) = i { Some(t.name.clone()) } else { None })
            .collect();
        // Locally-defined effect types — emit_effect_type generates an anonymous
        // typedef struct, which would conflict with our named forward decl.
        let local_effects: HashSet<String> = module.items.iter()
            .filter_map(|i| if let Item::Type(t) = i {
                if matches!(t.kind, TypeDeclKind::Effect(_)) { Some(t.name.clone()) } else { None }
            } else { None })
            .collect();

        for item in &module.items {
            if let Item::Type(t) = item {
                match &t.kind {
                    TypeDeclKind::Record(_) | TypeDeclKind::Sum(_) => {
                        self.user_type_fwd_decls.push_str(&format!(
                            "typedef struct Nova_{0} Nova_{0};\n", t.name));
                    }
                    _ => {}
                }
            }
        }

        // Also forward-decl external user types and effect vtables referenced
        // in this module's type fields and function signatures. Without this,
        // imported types (e.g. Duration from std.time.duration) and effect
        // vtables (e.g. NovaVtable_Random from Handler[Random]) cause
        // 'unknown type name' errors.
        {
            let mut external_names: HashSet<String> = HashSet::new();
            let mut vtable_names: HashSet<String> = HashSet::new();
            for item in &module.items {
                match item {
                    Item::Type(t) => Self::collect_typeref_names_in_typedecl(
                        t, &mut external_names, &mut vtable_names),
                    Item::Fn(f) => {
                        for p in &f.params {
                            Self::collect_typeref_names(&p.ty, &mut external_names, &mut vtable_names);
                        }
                        if let Some(r) = &f.return_type {
                            Self::collect_typeref_names(r, &mut external_names, &mut vtable_names);
                        }
                        // Direct effect annotations on fn → vtable names
                        for e in &f.effects {
                            if let TypeRef::Named { path, .. } = e {
                                if let Some(n) = path.last() { vtable_names.insert(n.clone()); }
                            }
                        }
                    }
                    _ => {}
                }
            }
            const BUILTIN_TYPE_NAMES: &[&str] = &[
                "int", "i64", "i32", "i16", "i8",
                "u64", "u32", "u16", "u8",
                "f64", "f32", "bool", "str", "byte", "char",
                "Option", "Result", "Self", "Handler", "CancelToken",
                "Never", "Error",
            ];
            // Runtime types defined in nova_rt/*.h with anonymous typedef'd structs.
            // A named forward decl `typedef struct Nova_X Nova_X;` would conflict
            // with the runtime's `typedef struct { ... } Nova_X;` (different types).
            const BUILTIN_RUNTIME_TYPES: &[&str] = &[
                "Result", "Error", "RuntimeError",
                "ReadBuffer", "StringBuilder", "WriteBuffer",
                "ChanReader", "ChanWriter", "ChannelPair",
                "AtomicInt", "AtomicBool", "Mutex", "WaitGroup", "Once",
                "Timestamp",
            ];
            for name in external_names {
                if local_types.contains(&name) { continue; }
                if BUILTIN_TYPE_NAMES.contains(&name.as_str()) { continue; }
                if BUILTIN_RUNTIME_TYPES.contains(&name.as_str()) { continue; }
                // Only emit forward decl for names starting with uppercase
                // (user-defined types map to Nova_Name*).
                if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    self.user_type_fwd_decls.push_str(&format!(
                        "typedef struct Nova_{0} Nova_{0};\n", name));
                }
            }
            // Built-in vtables defined in nova_rt/effects.h — skip.
            const BUILTIN_VTABLE_NAMES: &[&str] = &["Fail", "Time"];
            for name in vtable_names {
                if BUILTIN_VTABLE_NAMES.contains(&name.as_str()) { continue; }
                // Local effects — emit_effect_type generates an anonymous typedef
                // which would conflict with a named forward decl. Skip.
                if local_effects.contains(&name) { continue; }
                if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    self.user_type_fwd_decls.push_str(&format!(
                        "typedef struct NovaVtable_{0} NovaVtable_{0};\n", name));
                }
            }
        }

        // 1a. Pre-populate generic_types + generic_type_templates BEFORE emit_type_decl
        // and method registration, so both know which types are generic templates.
        for item in &module.items {
            if let Item::Type(t) = item {
                if !t.generics.is_empty() {
                    self.generic_types.insert(t.name.clone());
                    self.generic_type_templates.insert(t.name.clone(), t.clone());
                }
            }
        }
        // 1a2. Collect FnDecls for methods on generic types (needed for Ф.3 dispatch).
        for item in &module.items {
            if let Item::Fn(f) = item {
                if let Some(recv) = &f.receiver {
                    if self.generic_types.contains(&recv.type_name) {
                        self.generic_type_methods
                            .entry(recv.type_name.clone())
                            .or_default()
                            .push(f.clone());
                    }
                }
            }
        }

        // 1. Type declarations first (structs/unions needed by fn signatures)
        for item in &module.items {
            if let Item::Type(t) = item {
                self.emit_type_decl(t)?;
            }
        }

        // Plan 52.2 Ф.2: forward-declare mono'd struct types для
        // const-decls с generic-типом. Без этого `const X HashMap[K,V] = [...]`
        // эмитится с typeref'ом `Nova_HashMap____K__V*` ДО mono pass,
        // и C-compiler не знает такого type-name.
        //
        // Mono pass позже эмитит полное `struct Nova_HashMap____K__V {...}`
        // — forward-declare через `typedef struct X X;` достаточно для
        // pointer-type использования в const-decl (правда compile-time
        // init не работает, lazy init через nova_const_X() — pointer
        // OK через forward-decl).
        for item in &module.items {
            if let Item::Const(c) = item {
                if let Some(ty) = &c.ty {
                    self.forward_declare_generic_type(ty);
                }
            }
        }

        // 1b. Const declarations (after types, before fn forward decls)
        for item in &module.items {
            if let Item::Const(c) = item {
                self.emit_const_decl(c)?;
            }
        }

        // 1b2. D39 / Plan 11 Ф.9: collect embed-fields per record-type.
        // Используется на 1d для генерации auto-proxy methods.
        for item in &module.items {
            if let Item::Type(t) = item {
                if let TypeDeclKind::Record(fields) = &t.kind {
                    let mut embeds: Vec<(String, String, bool)> = Vec::new();
                    // Plan 11 Ф.9.4: multi-anonymous detection. Подсчитать
                    // count anonymous embeds per embedded-type — если ≥2
                    // одного типа → compile error (нет alias'а для disambig).
                    let mut anon_counts: HashMap<String, usize> = HashMap::new();
                    for f in fields {
                        if !f.is_embed { continue; }
                        let embedded_ty_name = match &f.ty {
                            TypeRef::Named { path, .. } => path.join("_"),
                            _ => continue,
                        };
                        if f.embed_anonymous {
                            *anon_counts.entry(embedded_ty_name.clone()).or_insert(0) += 1;
                        }
                        embeds.push((f.name.clone(), embedded_ty_name, f.embed_anonymous));
                    }
                    for (ty_name, count) in &anon_counts {
                        if *count > 1 {
                            return Err(format!(
                                "type `{}`: multiple anonymous embeds of `{}`; \
                                 use named alias `use <name> {}` to disambiguate",
                                t.name, ty_name, ty_name));
                        }
                    }
                    if !embeds.is_empty() {
                        self.embed_fields.insert(t.name.clone(), embeds);
                    }
                }
            }
        }

        // 1c. Pre-populate method_receivers so emit_call can route obj.method() correctly
        // Plus D84: register free-functions в method_overloads с sentinel-key
        // ("", name) — единый mechanism для overload resolution.
        for item in &module.items {
            if let Item::Fn(f) = item {
                // === D84: free-function overload registration ===
                if f.receiver.is_none() {
                    let param_c_types: Vec<String> = f.params.iter()
                        .map(|p| self.type_ref_to_c(&p.ty)
                            .unwrap_or_else(|_| "nova_int".into()))
                        .collect();
                    let return_c_type = match &f.return_type {
                        Some(t) => self.type_ref_to_c(t)
                            .unwrap_or_else(|_| "nova_int".into()),
                        // Plan 55 Ф.3: infer return-type из body для free fn без annotation.
                        None => self.return_type_c(f).unwrap_or_else(|_| "nova_unit".into()),
                    };
                    // Sentinel-key: пустая строка вместо receiver-type.
                    // Не конфликтует с user-types (имена ≠ пустой строке).
                    let key = ("".to_string(), f.name.clone());
                    let existing_count = self.method_overloads.get(&key)
                        .map(|v| v.len()).unwrap_or(0);
                    let base_c_name = format!("nova_fn_{}", f.name);
                    let c_name = if existing_count == 0 {
                        base_c_name.clone()
                    } else {
                        // Mangling по param-types (тот же sanitize что для методов).
                        let suffix = param_c_types.iter()
                            .map(|t| t.replace('*', "_p")
                                      .replace(' ', "_")
                                      .replace('[', "_arr_")
                                      .replace(']', ""))
                            .collect::<Vec<_>>()
                            .join("_");
                        if suffix.is_empty() {
                            base_c_name.clone()
                        } else {
                            format!("{}__{}", base_c_name, suffix)
                        }
                    };
                    let variadic_last = f.params.last()
                        .map(|p| p.is_variadic).unwrap_or(false);
                    let sig = MethodSig {
                        param_c_types,
                        return_c_type,
                        is_instance: false,    // free-function: not instance
                        is_external: f.is_external,
                        is_delegated: false,
                        c_name,
                        variadic_last,
                    };
                    self.method_overloads.entry(key).or_default().push(sig);
                    continue;
                }
                if let Some(recv) = &f.receiver {
                    // Plan 48: generic methods with own type params (f.generics non-empty) are
                    // normally handled by monomorphization. Exception: array extension methods
                    // (recv.type_name starts with "[]") are not user-defined generic types and
                    // never get monomorphized — register them with erased types instead.
                    let is_array_ext = recv.type_name.starts_with("[]");
                    if !f.generics.is_empty() && !is_array_ext {
                        continue;
                    }
                    // Plan 48 Ф.3: methods on generic receiver types are registered here for
                    // erased-mode dispatch (when receiver is unparameterized, e.g. `Nova_Pair*`).
                    // Monomorphized dispatch (block 5b) fires first and returns early for
                    // concrete instances, so no conflict.
                    let is_instance = matches!(recv.kind, ReceiverKind::Instance);
                    self.method_receivers.insert(
                        f.name.clone(),
                        (recv.type_name.clone(), is_instance),
                    );
                    // Plan 06 Ф.1: multi-key для for-in Iter[T] dispatch.
                    self.all_methods.insert((recv.type_name.clone(), f.name.clone()));
                    // Plan 11 Ф.1: register signature в multi-overload registry.
                    // param_c_types — C-типы параметров без receiver'а.
                    // Plan 48 Ф.3: for generic receiver types, use erased types so
                    // that type-param references (e.g. Pair[B,A]) don't monomorphize.
                    let is_generic_recv = self.generic_types.contains(&recv.type_name) || is_array_ext;
                    let recv_type_params: HashSet<String> = if is_generic_recv {
                        // Collect type params from receiver generics (e.g. T in []T)
                        let from_recv = recv.generics.iter().filter_map(|tr| {
                            if let TypeRef::Named { path, .. } = tr { path.first().cloned() } else { None }
                        });
                        // For array extension methods, also erase method-level generics (e.g. U in map[U])
                        let from_fn = if is_array_ext {
                            f.generics.iter().map(|g| g.name.clone()).collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                        from_recv.chain(from_fn.into_iter()).collect()
                    } else {
                        HashSet::new()
                    };
                    let param_c_types: Vec<String> = f.params.iter()
                        .map(|p| if is_generic_recv {
                            self.erased_type_ref_c(&Some(p.ty.clone()), &recv_type_params)
                        } else {
                            self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into())
                        })
                        .collect();
                    // Resolve return type. `Self` → recv.type_name.
                    // Plan 55 Ф.3: если return-type не указан, infer из body.
                    let return_c_type = match &f.return_type {
                        Some(TypeRef::Named { path, .. }) if path.len() == 1 && path[0] == "Self" => {
                            self.receiver_c_type(&recv.type_name)
                        }
                        Some(t) if is_generic_recv => {
                            self.erased_type_ref_c(&Some(t.clone()), &recv_type_params)
                        }
                        Some(t) => self.type_ref_to_c(t)
                            .unwrap_or_else(|_| "nova_int".into()),
                        None => self.return_type_c(f).unwrap_or_else(|_| "nova_unit".into()),
                    };
                    // For array extension methods, use the C-identifier form as the key so that
                    // call-site lookups (which derive the key from "NovaArray_nova_int*" → "NovaArray_nova_int")
                    // can find the registration. For regular types, keep the Nova type name.
                    let key = if is_array_ext {
                        (Self::receiver_type_c_ident(&recv.type_name), f.name.clone())
                    } else {
                        (recv.type_name.clone(), f.name.clone())
                    };
                    let existing_count = self.method_overloads.get(&key).map(|v| v.len()).unwrap_or(0);
                    // Mangling: для первой overload — короткое имя
                    // (backward compat); для второй+ — с param-types suffix.
                    let safe_recv_name = Self::receiver_type_c_ident(&recv.type_name);
                    let base_c_name = if is_instance {
                        format!("Nova_{}_method_{}", safe_recv_name, f.name)
                    } else {
                        format!("Nova_{}_static_{}", safe_recv_name, f.name)
                    };
                    let c_name = if existing_count == 0 {
                        base_c_name
                    } else {
                        // Mangling по param-types. Sanitize: `*` / `[`
                        // не валидны в C-identifier'ах.
                        let suffix = param_c_types.iter()
                            .map(|t| t.replace('*', "_p")
                                      .replace(' ', "_")
                                      .replace('[', "_arr_")
                                      .replace(']', ""))
                            .collect::<Vec<_>>()
                            .join("_");
                        if suffix.is_empty() {
                            base_c_name
                        } else {
                            format!("{}__{}", base_c_name, suffix)
                        }
                    };
                    // Plan 14 Ф.6: variadic-флаг — true если последний
                    // параметр is_variadic. Только последний валиден
                    // (parser проверяет position constraint).
                    let variadic_last = f.params.last()
                        .map(|p| p.is_variadic).unwrap_or(false);
                    let sig = MethodSig {
                        param_c_types,
                        return_c_type,
                        is_instance,
                        is_external: f.is_external,
                        is_delegated: false,    // own declaration
                        c_name,
                        variadic_last,
                    };
                    self.method_overloads.entry(key).or_default().push(sig);
                    // D73 v2 auto-derive registry:
                    //   - `T.from(v V)`     → from_targets[T] += V
                    //   - `fn V @into() -> T` → into_targets[V] = T
                    if !is_instance && f.name == "from" && !f.params.is_empty() {
                        if let TypeRef::Named { path, .. } = &f.params[0].ty {
                            if !path.is_empty() {
                                self.from_targets.entry(recv.type_name.clone())
                                    .or_default()
                                    .push(path.join("_"));
                            }
                        }
                    }
                    if is_instance && f.name == "into" {
                        if let Some(TypeRef::Named { path, .. }) = &f.return_type {
                            if !path.is_empty() {
                                self.into_targets.insert(recv.type_name.clone(), path.join("_"));
                            }
                        }
                    }
                    // Plan 06 Ф.3: instance-method `mut @iter() -> IterT` —
                    // запоминаем `Coll → IterT` для implicit .iter() в for-in.
                    if is_instance && f.name == "iter" {
                        if let Some(TypeRef::Named { path, .. }) = &f.return_type {
                            if !path.is_empty() {
                                self.iter_returns.insert(
                                    recv.type_name.clone(), path.join("_"));
                            }
                        }
                    }
                    // Plan 08 Ф.3: D77 try_from/try_into registries.
                    // - `T.try_from(v V)` → try_from_targets[T] += V
                    // - `fn V @try_into() -> Result[T, E]` → try_into_targets[V] = T
                    if !is_instance && f.name == "try_from" && !f.params.is_empty() {
                        if let TypeRef::Named { path, .. } = &f.params[0].ty {
                            if !path.is_empty() {
                                self.try_from_targets.entry(recv.type_name.clone())
                                    .or_default()
                                    .push(path.join("_"));
                            }
                        }
                    }
                    if is_instance && f.name == "try_into" {
                        // Берём первый generic-arg возвращаемого Result (если есть).
                        if let Some(TypeRef::Named { path, generics, .. }) = &f.return_type {
                            if path.last().map(|s| s.as_str()) == Some("Result") {
                                if let Some(TypeRef::Named { path: tp, .. }) = generics.first() {
                                    if !tp.is_empty() {
                                        self.try_into_targets.insert(
                                            recv.type_name.clone(), tp.join("_"));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 1c2. D39 / Plan 11 Ф.9: register auto-proxy delegated methods.
        // Для каждого record-type с embed-полями: для каждого метода
        // embedded-типа (instance) добавить Delegated MethodSig в registry
        // wrapper'а. Override-precedence (Own > Delegated) применяется на
        // call-site в resolve_overload (Ф.9.3). Multi-anonymous detection
        // уже сделан в 1b2.
        let embed_keys: Vec<String> = self.embed_fields.keys().cloned().collect();
        for wrapper_type in embed_keys {
            let embeds = self.embed_fields.get(&wrapper_type).cloned().unwrap_or_default();
            for (field_name, embedded_ty, _is_anon) in &embeds {
                // Найти все instance-методы embedded-типа.
                let embedded_methods: Vec<(String, MethodSig)> = self.method_overloads.iter()
                    .filter(|((t, _), _)| t == embedded_ty)
                    .flat_map(|((_, m), sigs)| sigs.iter().map(move |s| (m.clone(), s.clone())))
                    .filter(|(_, s)| s.is_instance && !s.is_delegated)
                    .collect();
                for (method_name, base_sig) in embedded_methods {
                    // Сгенерировать proxy MethodSig.
                    let key = (wrapper_type.clone(), method_name.clone());
                    let existing_count = self.method_overloads.get(&key)
                        .map(|v| v.len()).unwrap_or(0);
                    let base_c = format!("Nova_{}_method_{}", wrapper_type, method_name);
                    let proxy_c_name = if existing_count == 0 {
                        base_c
                    } else {
                        let suffix = base_sig.param_c_types.iter()
                            .map(|t| t.replace('*', "_p")
                                      .replace(' ', "_")
                                      .replace('[', "_arr_")
                                      .replace(']', ""))
                            .collect::<Vec<_>>()
                            .join("_");
                        if suffix.is_empty() { base_c } else { format!("{}__{}", base_c, suffix) }
                    };
                    let proxy_sig = MethodSig {
                        param_c_types: base_sig.param_c_types.clone(),
                        return_c_type: base_sig.return_c_type.clone(),
                        is_instance: true,
                        is_external: false,
                        is_delegated: true,
                        c_name: proxy_c_name,
                        // Plan 14 Ф.6: proxy наследует variadic-флаг
                        // от исходного метода (тот же signature).
                        variadic_last: base_sig.variadic_last,
                    };
                    self.method_overloads.entry(key).or_default().push(proxy_sig);
                    // all_methods (для Plan 06 Iter[T] dispatch).
                    self.all_methods.insert((wrapper_type.clone(), method_name.clone()));
                    // method_receivers backward compat — single-key, last-wins
                    // OK поскольку wrapper_type registered как owner of method.
                    if !self.method_receivers.contains_key(&method_name) {
                        self.method_receivers.insert(method_name.clone(),
                            (wrapper_type.clone(), true));
                    }
                    let _ = field_name; // используется при emit (ниже)
                }
            }
        }

        // 1d. Pre-populate generic_fns/generic_types sets for type-erased call site handling
        for item in &module.items {
            if let Item::Fn(f) = item {
                if !f.generics.is_empty() {
                    self.generic_fns.insert(f.name.clone());
                    if f.receiver.is_none() {
                        // Plan 48: store for monomorphization worklist drain
                        self.mono_fn_decls.insert(f.name.clone(), f.clone());
                    }
                }
            }
            if let Item::Type(t) = item {
                if !t.generics.is_empty() {
                    self.generic_types.insert(t.name.clone());
                }
            }
        }

        // Plan 48 Ф.3: drain initial generic type usages from step 1 type declarations.
        // This covers types that appear in other type declarations (nested generics).
        self.drain_generic_type_worklist()?;

        // Plan 48 Ф.3: placeholder for generic type instance definitions (filled after drain).
        self.line("/*__GENERIC_TYPE_DEFS__*/");

        // 2. Forward declarations for all functions (types are now known)
        for item in &module.items {
            if let Item::Fn(f) = item {
                self.emit_fn_forward_decl(f)?;
            }
        }
        // Plan 48: placeholder for mono function forward declarations (filled at end)
        self.line("/*__MONO_FWD_DECLS__*/");
        // Forward declarations for test impl functions
        {
            let mut idx = 0usize;
            for item in &module.items {
                if let Item::Test(t) = item {
                    let safe = Self::mangle_test_name_indexed(&t.name, idx);
                    idx += 1;
                    self.line(&format!("static nova_unit nova_test_{}(void);", safe));
                }
            }
        }
        self.line("");

        // 3. Pre-scan: emit forward decls for all handler impl functions before fn definitions.
        //    Uses handler_counter (starts at 0 here) to assign stable IDs matching step 4.
        self.emit_handler_forward_decls(module)?;

        // 4. Function definitions — but first a pre-pass to collect lambda forward decls.
        // Lambda impls are collected during the first pass into lambda_forward_decls + lambda_impls.
        // We do a two-step emit: (a) pre-emit all fns/tests to collect lambdas, then
        // (b) insert lambda_forward_decls + lambda_impls before the fn output.
        // Simpler approach: emit all fns; before flush, emit lambda_forward_decls + lambda_impls.
        for item in &module.items {
            if let Item::Fn(f) = item {
                self.emit_fn(f)?;
            }
        }

        // 4b. D39 / Plan 11 Ф.9: emit auto-proxy method bodies for embeds.
        self.emit_embed_proxies()?;

        // 5. Test function definitions
        {
            let mut idx = 0usize;
            for item in &module.items {
                if let Item::Test(t) = item {
                    self.emit_test(t, idx)?;
                    idx += 1;
                }
            }
        }

        // Plan 48: drain monomorphization worklist to fixpoint (R3: polymorphic recursion guard).
        // Ф.7.6: limit задаётся через CLI `--mono-depth=N` (set_mono_depth_limit)
        // или fallback на env var `NOVA_MONO_DEPTH`; оба читаются в `new()`.
        {
            let limit = self.mono_depth_limit;
            let mut safety = 0usize;
            while !self.mono_worklist.is_empty() {
                safety += 1;
                if safety > limit {
                    return Err(format!(
                        "instantiation depth limit {} exceeded (possible polymorphic recursion); \
                         add a non-generic base case, use explicit bounds to terminate, \
                         or raise via --mono-depth=N CLI flag (or NOVA_MONO_DEPTH env var)",
                        limit
                    ));
                }
                let batch: Vec<_> = std::mem::take(&mut self.mono_worklist);
                for (fn_name, type_subst, mono_name) in batch {
                    // Plan 48 V1 fallback: __erased__ prefix marks on-demand erased emission.
                    if let Some(real_name) = fn_name.strip_prefix("__erased__") {
                        if let Some(fn_decl) = self.mono_fn_decls.get(real_name).cloned() {
                            self.emit_generic_fn_erased(&fn_decl)?;
                        }
                        continue;
                    }
                    // Plan 48: __method__TYPE::name prefix marks generic method instances.
                    if let Some(rest) = fn_name.strip_prefix("__method__") {
                        if let Some((recv_type, mname)) = rest.split_once("::") {
                            let key = (recv_type.to_string(), mname.to_string());
                            // 1. Direct lookup (for non-generic types in mono_method_decls).
                            // 2. Fallback via base name lookup when recv_type is mangled:
                            //    a. try mono_method_decls[base]
                            //    b. try generic_type_methods[base] (methods skipped in 1c)
                            let base_opt: Option<String> = if self.mono_method_decls.contains_key(&key) {
                                None
                            } else {
                                // recv_type from section 5b has no "Nova_" prefix; map keys do.
                                let info = self.generic_type_instance_info.borrow();
                                info.get(recv_type)
                                    .or_else(|| info.get(&format!("Nova_{}", recv_type)))
                                    .map(|(b, _)| b.clone())
                            };
                            let fn_decl_opt = if let Some(fd) = self.mono_method_decls.get(&key).cloned() {
                                Some(fd)
                            } else if let Some(ref base) = base_opt {
                                self.mono_method_decls.get(&(base.clone(), mname.to_string()))
                                    .cloned()
                                    .or_else(|| {
                                        self.generic_type_methods.get(base)
                                            .and_then(|ms| ms.iter().find(|m| m.name == mname))
                                            .cloned()
                                    })
                            } else {
                                None
                            };
                            if let Some(fn_decl) = fn_decl_opt {
                                let rt = recv_type.to_string();
                                self.emit_monomorphized_method(&fn_decl, type_subst, &mono_name, &rt)?;
                            }
                        }
                        continue;
                    }
                    if let Some(fn_decl) = self.mono_fn_decls.get(&fn_name).cloned() {
                        self.emit_monomorphized_fn(&fn_decl, type_subst, &mono_name)?;
                    }
                }
                // Plan 48 Ф.3: drain new generic type instances enqueued by mono'd fn bodies.
                self.drain_generic_type_worklist()?;
            }
        }

        // 6. Handler impl function bodies (ctx structs + bodies at file scope, after fn defs)
        if !self.deferred_impls.is_empty() {
            self.out.push_str(&self.deferred_impls.clone());
            self.out.push('\n');
        }

        self.emit_main_wrapper(module);

        // Plan 36 followup: splice user-type forward decls в маркер
        // `/*__USER_TYPE_FWD_DECLS__*/`. Должно быть ДО NovaOpt replace,
        // потому что NovaOpt typedef'ы могут ссылаться на эти forward decls.
        let user_fwd_replacement = if self.user_type_fwd_decls.is_empty() {
            String::new()
        } else {
            format!(
                "/* Plan 36: forward decls для user types — нужны для NovaOpt_<T> */\n{}",
                self.user_type_fwd_decls)
        };
        self.out = self.out.replace("/*__USER_TYPE_FWD_DECLS__*/", &user_fwd_replacement);

        // Plan 14 Ф.1: splice NovaOpt_<T> typedefs в позицию маркера.
        // К этому моменту все type_ref_to_c-вызовы (включая в bodies)
        // отработали и заполнили novaopt_typedefs_buf в правильном
        // topological order.
        let typedefs = self.novaopt_typedefs_buf.borrow().clone();
        let replacement = if typedefs.is_empty() {
            String::new()
        } else {
            format!(
                "/* Plan 14 Ф.1: lazy NovaOpt_<T> typedef'ы — для T без \
                 NOVA_ARRAY_DECL в runtime. Order: registration */\n{}",
                typedefs)
        };
        self.out = self.out.replace("/*__NOVAOPT_TYPEDEFS__*/", &replacement);
        // Plan 48 Ф.3: splice generic type instance definitions
        let generic_type_defs = std::mem::take(&mut self.generic_type_defs_buf);
        self.out = self.out.replace("/*__GENERIC_TYPE_DEFS__*/", &generic_type_defs);
        // Plan 48: splice monomorphized fn forward declarations
        let mono_fwd = self.mono_fwd_decls.clone();
        self.out = self.out.replace("/*__MONO_FWD_DECLS__*/", &mono_fwd);
        Ok((self.out, self.warnings))
    }

    /// Mangle a test name and append a numeric suffix to guarantee uniqueness.
    fn mangle_test_name_indexed(name: &str, index: usize) -> String {
        let base: String = name.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        format!("{}_{}", base, index)
    }

    fn emit_test(&mut self, t: &TestDecl, idx: usize) -> Result<(), String> {
        let safe = Self::mangle_test_name_indexed(&t.name, idx);
        // Buffer the test body so we can prepend any lambdas discovered during emit
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        // Plan 54 Ф.1: snapshot var_types перед test body, restore после.
        // Без этого pattern-bound vars (`Some(v) => v`) и регулярные
        // let-bindings leak'ят между tests, что ломает match-arm inference
        // (например test 3 binds `v: bool`, test 6 binds `v: int` через
        // тот же match-pattern → infer_expr_c_type(`v`) возвращает
        // stale bool → match-result inferred bool → assert fails).
        // Same scope-cleanup для var_mutable + cancel_token_t_map.
        let saved_var_types = self.var_types.clone();
        let saved_var_mutable = self.var_mutable.clone();
        let saved_cancel_token_t_map = self.cancel_token_t_map.clone();
        self.line(&format!("static nova_unit nova_test_{}(void) {{", safe));
        self.indent = 1;
        self.emit_block_stmts(&t.body, "nova_unit")?;
        self.indent = 0;
        self.line("}");
        self.line("");
        // Restore scope-state — fixes leak (Plan 54 Ф.1).
        self.var_types = saved_var_types;
        self.var_mutable = saved_var_mutable;
        self.cancel_token_t_map = saved_cancel_token_t_map;
        let test_body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        // Flush any lambdas discovered during this test's emit
        if !self.lambda_forward_decls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_forward_decls));
        }
        if !self.lambda_impls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_impls));
        }
        self.out.push_str(&test_body);
        Ok(())
    }

    // ---- preamble ----

    fn emit_preamble(&mut self) {
        self.line("/* Generated by nova-codegen. Do not edit. */");
        self.line("#include \"nova_rt/nova_rt.h\"");
        self.line("");
        // Pre-declare tuple structs for arities 1–8 so they are at file scope.
        // This avoids MSVC C2011 (struct redefinition) when TupleLit is used inside functions.
        for n in 1..=8usize {
            let fields: String = (0..n).map(|i| format!("nova_int f{};", i)).collect::<Vec<_>>().join(" ");
            self.line(&format!("typedef struct {{ {} }} _NovaTuple{};", fields, n));
        }
        self.line("");
        // Plan 36 followup: forward decls user types ДО NovaOpt typedef'ов.
        // Иначе `NovaOpt_Nova_Range_p { Nova_Range* value; }` падает с
        // `unknown type name 'Nova_Range'` — типы декларируются после
        // марker'а в emit_type_decl, а NovaOpt splice'ится в marker.
        // Решение: pre-pass forward-decl всех user types через отдельный
        // marker, splice'ится в emit_module finalize.
        self.line("/*__USER_TYPE_FWD_DECLS__*/");
        // Plan 14 Ф.1: маркер для splice'а typedef'ов NovaOpt_<T> (для T
        // без NOVA_ARRAY_DECL в runtime). Заполняется в `register_novaopt_decl`
        // в registration order (innermost first); splice'ится в финальный
        // `out` через `replace` после полного emit_module.
        self.line("/*__NOVAOPT_TYPEDEFS__*/");
        self.line("");
    }

    // ---- const declarations ----

    fn emit_const_decl(&mut self, c: &ConstDecl) -> Result<(), String> {
        let ty_c = if let Some(ty) = &c.ty {
            self.type_ref_to_c(ty)?
        } else {
            self.infer_expr_c_type(&c.value)
        };
        // Emit as a static const variable (MSVC-safe, no VLAs or macros needed)
        // We emit the value as an expression; for string literals this needs
        // a compound literal initialiser which MSVC doesn't support at file scope.
        // For nova_str we use a special approach: emit a static initialiser macro.
        if ty_c == "nova_str" {
            // nova_str is {const char* ptr, size_t len}
            // MSVC doesn't support compound-literal initialisers at file scope in C.
            // Emit as a static struct with individual field initialisers.
            if let ExprKind::StrLit(s) = &c.value.kind {
                let escaped = Self::escape_c_str(s);
                let len = s.len();
                self.line(&format!(
                    "static const nova_str {} = {{(const char*)\"{}\" , {}}};",
                    c.name, escaped, len
                ));
                self.var_types.insert(c.name.clone(), ty_c.clone());
                return Ok(());
            }
        }
        // General case: emit as static const with initialiser expression
        // (covers int/bool/etc.). Передаём ty_c как target для integer-
        // литералов, чтобы emit правильный suffix/cast (например u32-const
        // получит `(uint32_t)NU`, не `((nova_int)NLL)` — последнее вызывает
        // implementation-defined signed→unsigned conversion для значений
        // вне диапазона int64, баг был замечен в std/checksums/fnv.nv).
        match self.emit_const_expr_typed(&c.value, Some(&ty_c)) {
            Ok(val) => {
                self.line(&format!("static const {} {} = {};", ty_c, c.name, val));
                // Регистрируем тип const'а в var_types, чтобы Ident(name) на
                // use-site инферился с правильным c-типом (например u32-const,
                // используемый как `let mut h = FOO`, должен дать `uint32_t h`,
                // а не nova_int — баг был замечен в std/checksums/fnv.nv).
                self.var_types.insert(c.name.clone(), ty_c.clone());
                Ok(())
            }
            Err(_) => {
                // Plan 14 Ф.2: non-constant initialiser (record-literal,
                // function call, и т.п.) — desugaring в lazy-init геттер.
                self.emit_lazy_const(&c.name, &ty_c, &c.value)
            }
        }
    }

    /// Plan 14 Ф.2: эмит const'а с runtime-init через lazy-init геттер.
    ///
    /// ```c
    /// static <Ty> _nova_const_<name>_value;
    /// static int _nova_const_<name>_init = 0;
    /// static <Ty> nova_const_<name>(void) {
    ///     if (!_nova_const_<name>_init) {
    ///         <emit_expr statements>
    ///         _nova_const_<name>_value = <expr_val>;
    ///         _nova_const_<name>_init = 1;
    ///     }
    ///     return _nova_const_<name>_value;
    /// }
    /// ```
    ///
    /// На use-site `Ident(name)` для lazy const'ов эмитим `nova_const_<name>()`.
    fn emit_lazy_const(&mut self, name: &str, ty_c: &str, value: &Expr) -> Result<(), String> {
        // Регистрируем имя как lazy — use-site Ident(name) станет вызовом геттера.
        self.lazy_consts.insert(name.to_string());
        // Регистрируем тип, чтобы infer_expr_c_type(Ident(name)) возвращал
        // правильный c-тип (для записи в var_types — как обычный binding).
        self.var_types.insert(name.to_string(), ty_c.to_string());
        // Эмитим storage + init-flag (file-scope statics).
        self.line(&format!("static {} _nova_const_{}_value;", ty_c, name));
        self.line(&format!("static int _nova_const_{}_init = 0;", name));
        // Эмитим геттер. Тело уходит в deferred_impls (после всех forward
        // declarations), чтобы вложенные emit'ы (record-литерал → side
        // statements) не разрушали file-scope порядок.
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        self.line(&format!("static {} nova_const_{}(void) {{", ty_c, name));
        self.indent = 1;
        self.line(&format!("if (!_nova_const_{}_init) {{", name));
        self.indent = 2;
        // Передать ty_c как ожидаемый record-target для D55 coercion
        // (`const FOO = { ... }` без явного имени типа должен подхватить
        // тип из аннотации/typed-target).
        let saved_expected = self.expected_record_type.clone();
        self.expected_record_type = Self::struct_name_from_c_type(ty_c);
        let val = self.emit_expr(value)?;
        self.expected_record_type = saved_expected;
        self.line(&format!("_nova_const_{}_value = {};", name, val));
        self.line(&format!("_nova_const_{}_init = 1;", name));
        self.indent = 1;
        self.line("}");
        self.line(&format!("return _nova_const_{}_value;", name));
        self.indent = 0;
        self.line("}");
        self.line("");
        let getter_body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        // Forward decl рядом с storage — чтобы вызовы видели функцию.
        self.line(&format!("static {} nova_const_{}(void);", ty_c, name));
        // Тело уходит в deferred_impls (печатается после forward decls).
        self.deferred_impls.push_str(&getter_body);
        Ok(())
    }

    /// Эмит integer-литерала с правильным C-типом (suffix + cast).
    ///
    /// Для unsigned-целевых типов важно эмитить unsigned-литерал
    /// (`U` / `ULL` suffix), иначе signed `(nova_int)<N>LL` cast в
    /// беззнаковый — implementation-defined для значений вне диапазона
    /// int64 (например 0xCBF29CE484222325 как FNV-64 offset).
    fn emit_typed_int_literal(n: i64, ty_c: &str) -> String {
        match ty_c {
            "uint8_t" | "uint16_t" | "uint32_t" => {
                // Unsigned 32-bit и меньше: U-suffix + cast к точному типу.
                // n хранится как i64; для отрицательных значений или > i32::MAX
                // используем явное приведение через хеш-bit-pattern.
                format!("(({}){}U)", ty_c, n as u32)
            }
            "uint64_t" => {
                // ULL-suffix; используем bit-pattern u64 чтобы корректно
                // передать значения, не помещающиеся в i64 (FNV-64 и т.п.).
                format!("(({})0x{:X}ULL)", ty_c, n as u64)
            }
            "int8_t" | "int16_t" | "int32_t" => {
                format!("(({}){})", ty_c, n as i32)
            }
            // nova_int (= int64_t), int64_t — default LL-suffix.
            _ => format!("(({}){}LL)", ty_c, n),
        }
    }

    /// Emit a constant expression — like emit_expr but without side-effect statements.
    /// Used for file-scope const initialisers.
    ///
    /// `target_ty_c` (если задан) — c-тип целевого const'а. Для integer-литералов
    /// используется чтобы выбрать правильный suffix/cast (unsigned vs signed).
    fn emit_const_expr_typed(&mut self, expr: &Expr, target_ty_c: Option<&str>) -> Result<String, String> {
        match &expr.kind {
            ExprKind::IntLit(n) => {
                let ty_c = target_ty_c.unwrap_or("nova_int");
                Ok(Self::emit_typed_int_literal(*n, ty_c))
            }
            ExprKind::CharLit(cp) => {
                let ty_c = target_ty_c.unwrap_or("nova_int");
                Ok(Self::emit_typed_int_literal(*cp as i64, ty_c))
            }
            ExprKind::Unary { op: UnOp::Neg, operand } => {
                // `-IntLit` → typed-literal с минусом. ВАЖНО: для unsigned-типа
                // негативный литерал в const'е концептуально некорректен (заворот),
                // оставляем как есть (программист сам отвечает).
                if let ExprKind::IntLit(n) = &operand.kind {
                    let ty_c = target_ty_c.unwrap_or("nova_int");
                    return Ok(Self::emit_typed_int_literal(-*n, ty_c));
                }
                let inner = self.emit_const_expr_typed(operand, target_ty_c)?;
                Ok(format!("(-({}))", inner))
            }
            _ => self.emit_const_expr(expr),
        }
    }

    fn emit_const_expr(&mut self, expr: &Expr) -> Result<String, String> {
        match &expr.kind {
            ExprKind::IntLit(n) => Ok(format!("((nova_int){}LL)", n)),
            ExprKind::CharLit(cp) => Ok(format!("((nova_int){}LL)", cp)),
            ExprKind::BoolLit(b) => Ok(if *b { "1".into() } else { "0".into() }),
            ExprKind::StrLit(s) => {
                let len = s.len();
                Ok(format!("{{.ptr=\"{}\", .len={}}}", Self::escape_c_str(s), len))
            }
            ExprKind::InterpolatedStr { .. } => {
                // String interpolation в const-инициализаторе требует runtime-
                // вычислений (StringBuilder.append + into) — не constexpr.
                Err("string interpolation `${...}` is not allowed in const initialiser \
                     (use a plain string literal or a runtime expression)".to_string())
            }
            ExprKind::FloatLit(f) => {
                // То же что в emit_expr — гарантируем что C видит double-литерал,
                // не integer (избегает overflow на 1e20 → "100000000000000000000").
                let s = if f.is_finite() && (f.abs() >= 1e16 || (f.abs() != 0.0 && f.abs() < 1e-4)) {
                    format!("{:e}", f)
                } else {
                    let raw = f.to_string();
                    if raw.contains('.') || raw.contains('e') || raw.contains('E') {
                        raw
                    } else {
                        format!("{}.0", raw)
                    }
                };
                Ok(s)
            }
            ExprKind::Unary { op, operand } => {
                let inner = self.emit_const_expr(operand)?;
                let op_str = match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                };
                Ok(format!("({}({}))", op_str, inner))
            }
            _ => Err(format!("non-constant expression in const declaration: {:?}", expr.kind)),
        }
    }

    // ---- type mapping ----

    fn type_ref_to_c(&self, ty: &TypeRef) -> Result<String, String> {
        // Plan 48: type parameter substitution (monomorphization context)
        if let TypeRef::Named { path, generics, .. } = ty {
            if generics.is_empty() {
                let name = path.join("_");
                if let Some(concrete) = self.current_type_subst.get(&name) {
                    return Ok(concrete.clone());
                }
            }
        }
        match ty {
            TypeRef::Named { path, generics, .. } => {
                let name = path.join("_");
                match name.as_str() {
                    "int"  => Ok("nova_int".into()),
                    "i64"  => Ok("nova_int".into()),
                    "i32"  => Ok("int32_t".into()),
                    "i16"  => Ok("int16_t".into()),
                    "i8"   => Ok("int8_t".into()),
                    "u64"  => Ok("uint64_t".into()),
                    "u32"  => Ok("uint32_t".into()),
                    "u16"  => Ok("uint16_t".into()),
                    "u8"   => Ok("uint8_t".into()),
                    "f64"  => Ok("nova_f64".into()),
                    "f32"  => Ok("nova_f32".into()),
                    "bool" => Ok("nova_bool".into()),
                    "str"  => Ok("nova_str".into()),
                    "byte" => Ok("nova_byte".into()),
                    // D26 Q-string-indexing школа B: char это codepoint = nova_int в bootstrap.
                    // Без этой ветки fallback вёл к `Nova_char*` (struct ptr) → invalid C
                    // (`Nova_char` undefined и коллизия с C keyword `char`).
                    "char" => Ok("nova_int".into()),
                    "Option" => {
                        // Plan 14 Ф.1: Option[T] правильно типизирован
                        // через generic. Для T без NOVA_ARRAY_DECL в
                        // runtime — typedef эмитится лениво в preamble
                        // (emit_lazy_novaopt_decls).
                        //
                        // Fallback на NovaOpt_nova_int (legacy int-stomp):
                        //   - no generics specified;
                        //   - inner T = void* (generic erased);
                        //   - inner T = Nova_<X>* где X — type-param
                        //     (нет struct/sum decl) — generic-erased.
                        if let Some(inner) = generics.first() {
                            let inner_c = self.type_ref_to_c(inner)?;
                            if inner_c == "void*" {
                                return Ok("NovaOpt_nova_int".into());
                            }
                            // Detect type-param: Nova_<X>* где X не в
                            // record_schemas / sum_schemas / generic_types.
                            // Это означает X — type-param метода/типа,
                            // а не реальный struct.
                            if let Some(x) = inner_c
                                .strip_suffix('*')
                                .and_then(|s| s.trim().strip_prefix("Nova_"))
                            {
                                let is_concrete_type =
                                    self.record_schemas.contains_key(x)
                                    || self.sum_schemas.contains_key(x)
                                    || self.generic_types.contains(x);
                                if !is_concrete_type {
                                    return Ok("NovaOpt_nova_int".into());
                                }
                            }
                            let sanitized = Self::sanitize_for_novaopt(&inner_c);
                            self.register_novaopt_decl(&sanitized, &inner_c);
                            Ok(format!("NovaOpt_{}", sanitized))
                        } else {
                            Ok("NovaOpt_nova_int".into())
                        }
                    }
                    "Result" => Ok("Nova_Result*".into()),
                    "Self" => {
                        // Self resolves to current receiver type
                        if let Some(recv) = &self.current_receiver_type {
                            Ok(format!("Nova_{}*", recv))
                        } else {
                            Ok("Nova_Self*".into())
                        }
                    }
                    "Handler" => {
                        // Handler[EffectName] → NovaVtable_EffectName*
                        if let Some(g) = generics.first() {
                            if let TypeRef::Named { path: eff_path, .. } = g {
                                return Ok(format!("NovaVtable_{}*", eff_path.join("_")));
                            }
                        }
                        Ok("void*".into())
                    }
                    // D75 (revised, Plan 47): CancelToken — caller-owned
                    // cancellation handle. C-тип — NovaCancelToken* (без
                    // подчёркивания Nova_ — это runtime-struct, не Nova-record).
                    "CancelToken" => Ok("NovaCancelToken*".into()),
                    _ => {
                        // Check if it's a type alias — return the aliased type directly (no *)
                        if let Some(aliased_c) = self.type_aliases.get(&name).cloned() {
                            return Ok(aliased_c);
                        }
                        // Plan 48 Ф.3: if this is a generic type with concrete type args,
                        // compute mangled name and enqueue instance emission.
                        if !generics.is_empty() && self.generic_type_templates.contains_key(&name) {
                            let type_args_c: Vec<String> = generics.iter()
                                .map(|g| self.type_ref_to_c(g).unwrap_or_else(|_| "nova_int".into()))
                                .collect();
                            // If any arg is still a type-param (void* or unknown), fall back
                            let any_erased = type_args_c.iter().any(|a|
                                a == "void*" || a.starts_with("Nova_") && !self.record_schemas.contains_key(
                                    a.trim_start_matches("Nova_").trim_end_matches('*').trim()));
                            if any_erased && !self.current_type_subst.is_empty() {
                                // In mono context: all type-params should be substituted already
                                // (type_ref_to_c does subst at top). If still void*, it's genuinely void.
                            }
                            let mangled = Self::compute_generic_type_c_name(&name, &type_args_c);
                            // Enqueue instance if not yet registered (via RefCell — allows &self)
                            if !self.emitted_generic_type_instances.contains(&mangled) {
                                let mut wl = self.generic_type_worklist.borrow_mut();
                                if !wl.iter().any(|(_, _, m)| m == &mangled) {
                                    wl.push((name.clone(), type_args_c.clone(), mangled.clone()));
                                }
                            }
                            // Register instance info for emit_call method dispatch
                            self.generic_type_instance_info.borrow_mut()
                                .entry(mangled.clone())
                                .or_insert_with(|| (name.clone(), type_args_c));
                            return Ok(format!("{}*", mangled));
                        }
                        // User-defined type — pointer to struct
                        Ok(format!("Nova_{}*", name))
                    }
                }
            }
            TypeRef::Unit(_) => Ok("nova_unit".into()),
            TypeRef::Array(inner, _) => {
                // Plan 55 Ф.1: `[]fn(...) -> T` → array of closure pointers.
                // Storage = NovaArray_void_p* (typedef void* void_p).
                // Element call goes through NovaClosBase dispatch.
                if matches!(inner.as_ref(), TypeRef::Func { .. }) {
                    return Ok("NovaArray_void_p*".into());
                }
                // Мономорфизация по primitive elem-type. Каждый primitive
                // type имеет собственный NovaArray_T с реальным packed
                // storage (не int64-erasure). Для byte это критично:
                // `[]byte` = `uint8_t[]`, не int64[].
                // Record/sum/array-of-array хранятся через nova_int slots
                // (boxed pointers) — bootstrap-ограничение.
                if let TypeRef::Named { path, .. } = inner.as_ref() {
                    if path.len() == 1 {
                        // Resolve type-param substitution first (e.g. K→nova_str).
                        // current_type_subst stores C-type names, so map back to
                        // the canonical elem-name used in the match below.
                        let nova_name = path[0].as_str();
                        let elem_key = if let Some(c_ty) = self.current_type_subst.get(nova_name) {
                            // Map C type name back to canonical Nova array key.
                            match c_ty.as_str() {
                                "nova_str"  => "str",
                                "nova_byte" | "uint8_t" | "int8_t" => "byte",
                                "nova_bool" => "bool",
                                "nova_f64" | "nova_f32" | "float" | "double" => "f64",
                                _ => nova_name,   // fallback → NovaArray_nova_int*
                            }
                        } else {
                            nova_name
                        };
                        return Ok(match elem_key {
                            "str" => "NovaArray_nova_str*".into(),
                            "byte" | "u8" => "NovaArray_nova_byte*".into(),
                            "bool" => "NovaArray_nova_bool*".into(),
                            "f64" | "f32" => "NovaArray_nova_f64*".into(),
                            // int/i8-i64/u16-u64/char и unknown user types
                            // — через int64 slot (boxed pointers для record/sum).
                            _ => "NovaArray_nova_int*".into(),
                        });
                    }
                }
                Ok("NovaArray_nova_int*".into())
            }
            TypeRef::Tuple(elems, _) => {
                let n = elems.len();
                Ok(format!("_NovaTuple{}", n))
            }
            TypeRef::Func { .. } => {
                // Function type — use a void pointer as opaque representation
                Ok("void*".into())
            }
            TypeRef::FixedArray(_n, inner, span) => {
                // [N]T в bootstrap — тот же runtime-тип что и `[]T` (NovaArray_T*).
                // Размер N запоминается в типе AST, но в bootstrap-codegen не
                // enforce'ится: dynamic-array под капотом, push/pop/len работают
                // одинаково. Stack-allocation как в C `T[N]` пока не делаем —
                // это будет отдельная оптимизация (production), здесь bootstrap
                // приоритезирует совместимость с `[]T`-кодом.
                self.type_ref_to_c(&TypeRef::Array(inner.clone(), *span))
            }
        }
    }

    fn return_type_c(&self, f: &FnDecl) -> Result<String, String> {
        match &f.return_type {
            Some(ty) => self.type_ref_to_c(ty),
            // Plan 55 Ф.3: если return type не указан, infer из expression-body.
            // `fn foo() => expr` и `fn foo() { ...; expr }` — оба должны брать тип
            // из выражения. Раньше всегда возвращали nova_unit → CC-FAIL в callers
            // ожидающих real type (e.g. str.from(d.@as_secs_f64()) внутри Duration).
            None => {
                let inferred = match &f.body {
                    FnBody::Expr(e) => self.infer_expr_c_type(e),
                    FnBody::Block(b) => b.trailing.as_ref()
                        .map(|e| self.infer_expr_c_type(e))
                        .unwrap_or_else(|| "nova_unit".into()),
                    FnBody::External => "nova_unit".into(),
                };
                // Защита: void* / пустые → fallback unit.
                if inferred.is_empty() || inferred == "void*" {
                    Ok("nova_unit".into())
                } else {
                    Ok(inferred)
                }
            }
        }
    }

    // ---- effect with/handler ----

    fn emit_with(&mut self, bindings: &[WithBinding], body: &Block) -> Result<String, String> {
        let mut saves: Vec<(String, String)> = Vec::new(); // (effect_name, prev_var)
        let mut has_fail = false;

        for binding in bindings {
            let effect_name = match &binding.effect {
                TypeRef::Named { path, .. } => path.join("_"),
                _ => return Err("non-named effect in with binding".into()),
            };
            if effect_name == "Fail" { has_fail = true; }
            // Plan 19, C8 codegen (D31-rev): handler-лямбда
            // `with EffectName = |args| body` — sugar над handler-
            // литералом для эффектов с одной операцией. Десугаризуем
            // ClosureLight/ClosureFull в синтетический HandlerLit
            // перед emit_expr.
            let handler_val = if let Some((effect_path, lit_expr)) =
                self.desugar_handler_lambda(&binding.effect, &binding.handler)?
            {
                let _ = effect_path; // подсветим для clippy
                self.emit_expr(&lit_expr)?
            } else {
                self.emit_expr(&binding.handler)?
            };
            let prev_var = self.fresh_tmp();
            self.line(&format!(
                "NovaVtable_{eff}* {prev} = _nova_handler_{eff};",
                eff = effect_name, prev = prev_var
            ));
            // Plan 20 Ф.8 (4): D65 правило 3 — vtable->prev = outer handler.
            // Nova_Fail_fail swap'ает _nova_handler_Fail = current->prev на
            // время invocation, чтобы re-throw в handler-body dispatch'ился
            // на outer handler (skip current frame).
            if effect_name == "Fail" {
                self.line(&format!(
                    "{hv}->prev = {prev};",
                    hv = handler_val, prev = prev_var
                ));
            }
            self.line(&format!(
                "_nova_handler_{eff} = {hv};",
                eff = effect_name, hv = handler_val
            ));
            saves.push((effect_name, prev_var));
        }

        // For `with Fail = ... { body }`: install a fail-frame around body so
        // that throw inside body (Nova_Fail_fail with installed handler) ends
        // up unwinding back here. Body normal completion → result; throw →
        // handler runs (state captured), then nova_throw → fail-frame catches.
        // (D65 «Fail strict»: fail() is Never from caller's perspective.)
        let fframe = if has_fail { Some(self.fresh_tmp()) } else { None };
        if let Some(ff) = &fframe {
            self.line(&format!("NovaFailFrame {};", ff));
            self.line(&format!("nova_fail_push(&{});", ff));
        }

        // Plan 39 Issue A: infer T_body to pick correct result slot.
        // Category of trail type decides storage:
        //   - int/bool/unit → use NovaInterruptFrame.value (nova_int slot)
        //   - pointer type (contains '*') → use NovaInterruptFrame.value_ptr (void* slot)
        //   - value struct (NovaOpt_X, NovaResult_X_E, etc.) → heap-allocate,
        //     pointer goes through value_ptr; reader dereferences.
        // Если body не имеет trailing (заканчивается throw/return), смотрим на
        // handler interrupt-VAL type — это semantically тип W (D61 §10).
        let trail_ty = if let Some(t) = &body.trailing {
            self.infer_expr_c_type(t)
        } else {
            // Probe handler-лямбды на interrupt VAL.
            let mut found: Option<String> = None;
            for b in bindings {
                if let Some(ty) = infer_handler_interrupt_ty(self, &b.handler) {
                    found = Some(ty);
                    break;
                }
            }
            found.unwrap_or_else(|| "nova_unit".into())
        };
        let category = with_result_category(&trail_ty);

        // Emit interrupt frame so `interrupt v` can early-exit this with-block
        let iframe = self.fresh_tmp();
        let result_tmp = self.fresh_tmp();
        self.line(&format!("NovaInterruptFrame {};", iframe));
        // Declare result_tmp with the actual trail type (not always nova_int).
        let result_decl_ty = match category {
            WithResultCategory::IntLike => "nova_int".to_string(),
            WithResultCategory::Pointer => trail_ty.clone(),
            WithResultCategory::ValueStruct => trail_ty.clone(),
            WithResultCategory::UnitVoid => "nova_int".to_string(),
        };
        self.line(&format!("{} {};", result_decl_ty, result_tmp));
        self.line(&format!("nova_interrupt_push(&{});", iframe));

        // If we have fail-frame, wrap interrupt-setjmp inside fail-setjmp.
        if let Some(ff) = &fframe {
            self.line(&format!("if (setjmp({ff}.jmp) == 0) {{", ff = ff));
            self.indent += 1;
        }

        self.line(&format!("if (setjmp({iframe}.jmp) == 0) {{", iframe = iframe));
        self.indent += 1;

        // Body executes in the normal path
        self.line("{");
        self.indent += 1;
        // Emit block statements with defer scope; if there's a trailing expr use it as the int result
        let with_block_id = self.enter_defer_scope(body, false);
        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            self.emit_source_annotation_for_expr(trailing);
            let tv = self.emit_expr(trailing)?;
            match category {
                WithResultCategory::IntLike => {
                    self.line(&format!("{} = (nova_int)({});", result_tmp, tv));
                }
                WithResultCategory::Pointer | WithResultCategory::ValueStruct => {
                    self.line(&format!("{} = ({});", result_tmp, tv));
                }
                WithResultCategory::UnitVoid => {
                    self.line(&format!("(void)({});", tv));
                    self.line(&format!("{} = ((nova_int)0LL);", result_tmp));
                }
            }
        } else {
            match category {
                WithResultCategory::IntLike | WithResultCategory::UnitVoid => {
                    self.line(&format!("{} = ((nova_int)0LL);", result_tmp));
                }
                WithResultCategory::Pointer => {
                    self.line(&format!("{} = NULL;", result_tmp));
                }
                WithResultCategory::ValueStruct => {
                    self.line(&format!("{} = ({}){{0}};", result_tmp, result_decl_ty));
                }
            }
        }
        self.leave_defer_scope(with_block_id);
        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        // Interrupt path: read from the slot matching the category.
        match category {
            WithResultCategory::IntLike | WithResultCategory::UnitVoid => {
                self.line(&format!("{} = {iframe}.value;", result_tmp, iframe = iframe));
            }
            WithResultCategory::Pointer => {
                self.line(&format!("{} = ({}){iframe}.value_ptr;", result_tmp, result_decl_ty, iframe = iframe));
            }
            WithResultCategory::ValueStruct => {
                // value_ptr holds heap-allocated slot of the value struct.
                self.line(&format!(
                    "{} = *(({}*){iframe}.value_ptr);",
                    result_tmp, result_decl_ty, iframe = iframe));
            }
        }
        self.indent -= 1;
        self.line("}");

        // Close fail-frame outer if we opened it
        if let Some(ff) = &fframe {
            self.indent -= 1;
            self.line("} else {");
            self.indent += 1;
            // Plan 49 Ф.3: kind-aware re-dispatch.
            // CANCEL throw НЕ обработан этим Fail-handler'ом (отмена
            // структурна, не ошибка) — re-throw нагору с тем же reason.
            // Pop fail-frame ПЕРЕД re-throw чтобы next-outer-frame
            // получил throw, не наш собственный (защита от двойного unwind,
            // см. Plan 49 Риск R3).
            self.line(&format!("if ({ff}.error_kind == NOVA_THROW_CANCEL) {{",
                ff = ff));
            self.indent += 1;
            self.line("nova_fail_pop();");
            // Также pop interrupt frame чтобы инвариант stack discipline
            // сохранился (open/close симметрично с normal path).
            self.line("nova_interrupt_pop();");
            // Restore handlers — отмена не должна оставить scope handlers
            // активным; emit_with normal-path делает то же ниже.
            for (effect_name, prev_var) in saves.iter().rev() {
                self.line(&format!("_nova_handler_{eff} = {prev};",
                    eff = effect_name, prev = prev_var));
            }
            self.line(&format!(
                "nova_throw_cancel_reason({ff}.error_msg, {ff}.error_reason_ptr);",
                ff = ff));
            self.indent -= 1;
            self.line("}");
            // USER path: handler already ran; result is unit/zero. (Existing
            // semantics: D65 Fail-handler сам decide'ит результат через
            // interrupt v ИЛИ throw; если throw — мы здесь, результат default).
            match category {
                WithResultCategory::IntLike | WithResultCategory::UnitVoid => {
                    self.line(&format!("{} = ((nova_int)0LL);", result_tmp));
                }
                WithResultCategory::Pointer => {
                    self.line(&format!("{} = NULL;", result_tmp));
                }
                WithResultCategory::ValueStruct => {
                    self.line(&format!("{} = ({}){{0}};", result_tmp, result_decl_ty));
                }
            }
            self.indent -= 1;
            self.line("}");
            self.line("nova_fail_pop();");
        }

        // Restore handlers (regardless of path)
        for (effect_name, prev_var) in saves.iter().rev() {
            self.line(&format!("_nova_handler_{eff} = {prev};",
                eff = effect_name, prev = prev_var));
        }
        self.line("nova_interrupt_pop();");

        Ok(result_tmp)
    }

    /// Plan 19, C8 codegen (D31-rev): desugar handler-лямбды
    /// `with EffectName = |args| body` в синтетический HandlerLit
    /// с одной операцией. Возвращает Some((effect-path, synthetic
    /// HandlerLit-Expr)) если binding.handler — closure-light/full;
    /// иначе None (вызывающий код использует обычный emit_expr).
    ///
    /// Логика:
    /// 1. Если handler не закрытие — None.
    /// 2. Вытащить effect-name path.
    /// 3. Из effect_schemas найти единственную операцию эффекта
    ///    (если операций > 1, возвращаем None — компилятор type-checker
    ///    эту ситуацию обнаружит на seamntic-стадии; здесь fallback
    ///    на обычный emit, который выдаст codegen-error).
    /// 4. Синтезировать HandlerMethod с params/body из closure'а.
    /// 5. Обернуть в ExprKind::HandlerLit.
    fn desugar_handler_lambda(
        &self,
        effect: &TypeRef,
        handler: &Expr,
    ) -> Result<Option<(Vec<String>, Expr)>, String> {
        let effect_path = match effect {
            TypeRef::Named { path, .. } => path.clone(),
            _ => return Ok(None),
        };
        let eff_key = effect_path.join("_");

        // Извлекаем params и body в форме HandlerMethod.
        let (handler_params, handler_body): (
            Vec<HandlerMethodParam>,
            HandlerMethodBody,
        ) = match &handler.kind {
            ExprKind::ClosureLight { params, body } => {
                let p: Vec<HandlerMethodParam> = params
                    .iter()
                    .map(|cp| HandlerMethodParam {
                        name: cp.name.clone(),
                        ty: None,
                        span: cp.span,
                    })
                    .collect();
                let b = match body {
                    crate::ast::ClosureBody::Expr(e) => HandlerMethodBody::Expr((**e).clone()),
                    crate::ast::ClosureBody::Block(blk) => HandlerMethodBody::Block(blk.clone()),
                };
                (p, b)
            }
            ExprKind::ClosureFull(sb) => {
                let p: Vec<HandlerMethodParam> = sb
                    .params
                    .iter()
                    .map(|fp| HandlerMethodParam {
                        name: fp.name.clone(),
                        ty: Some(fp.ty.clone()),
                        span: fp.span,
                    })
                    .collect();
                let b = match &sb.body {
                    FnBody::Expr(e) => HandlerMethodBody::Expr(e.clone()),
                    FnBody::Block(blk) => HandlerMethodBody::Block(blk.clone()),
                    FnBody::External => return Ok(None),
                };
                (p, b)
            }
            _ => return Ok(None),
        };

        // Находим единственную операцию эффекта.
        let op_name = if let Some(schema) = self.effect_schemas.get(&eff_key) {
            if schema.len() != 1 {
                // Не sugar-applicable: > 1 операции или 0.
                return Ok(None);
            }
            schema.keys().next().cloned().unwrap_or_default()
        } else {
            // Эффект не в schemas (ещё не был обработан) —
            // безопасный fallback: пропустить sugar, обычный emit_expr
            // даст более понятную диагностику.
            return Ok(None);
        };

        // Синтезируем HandlerMethod с захваченным span (берём span
        // самого handler-выражения — он покрывает всё закрытие).
        let synth_method = HandlerMethod {
            name: op_name,
            params: handler_params,
            body: handler_body,
            span: handler.span,
        };

        // Оборачиваем в HandlerLit.
        let lit_expr = Expr::new(
            ExprKind::HandlerLit {
                effect_name: effect_path.clone(),
                methods: vec![synth_method],
            },
            handler.span,
        );

        Ok(Some((effect_path, lit_expr)))
    }

    fn emit_handler_lit(
        &mut self,
        effect_name: &[String],
        methods: &[HandlerMethod],
    ) -> Result<String, String> {
        let eff = effect_name.join("_");

        // Collect method return types from effect schema
        let schema = self.effect_schemas.get(&eff).cloned()
            .unwrap_or_default();

        // Use handler_counter for a stable, predictable ID (not tmp_counter)
        // so the pre-scan can emit forward decls with matching names.
        let handler_id = format!("_nova_handler_lit_{}", self.handler_counter);
        self.handler_counter += 1;

        // ---- Collect free variables referenced in method bodies ----
        // We do a simple name-scan: any Ident in the body that is in var_types
        // and is not a parameter of the method itself is a captured variable.
        let mut all_captures: Vec<(String, String)> = Vec::new(); // (name, c_type)
        for m in methods {
            let method_param_names: std::collections::HashSet<String> =
                m.params.iter().map(|p| p.name.clone()).collect();
            let refs = Self::collect_idents_in_handler_method(m);
            for name in refs {
                if method_param_names.contains(&name) {
                    continue;
                }
                if all_captures.iter().any(|(n, _)| n == &name) {
                    continue;
                }
                if let Some(ty) = self.var_types.get(&name).cloned() {
                    all_captures.push((name, ty));
                }
            }
        }

        // ---- Emit context struct type inline (local typedef, valid in C99+) ----
        // This goes directly into out (inside the function body) before the vtable.
        // MSVC supports local typedefs in function scope.
        let ctx_struct = format!("NovaCtx_{}", handler_id);
        // For each capture: pointer types stored directly (heap object already by-ref),
        // scalar/struct types stored as pointer-to (so mutations are visible in caller).
        let capture_ptr_tys: Vec<String> = all_captures.iter().map(|(_, cap_ty)| {
            if cap_ty.ends_with('*') { cap_ty.clone() } else { format!("{}*", cap_ty) }
        }).collect();
        self.line(&format!("typedef struct {{"));
        for ((cap_name, _), ptr_ty) in all_captures.iter().zip(capture_ptr_tys.iter()) {
            self.line(&format!("    {} {};", ptr_ty, cap_name));
        }
        if all_captures.is_empty() {
            self.line("    char _dummy;");
        }
        self.line(&format!("}} {};", ctx_struct));

        // ---- Emit context struct typedef into deferred_impls (file scope) ----
        // The impl functions (file-scope) also need to know the ctx struct type.
        let _ = writeln!(self.deferred_impls, "typedef struct {{");
        for ((cap_name, _), ptr_ty) in all_captures.iter().zip(capture_ptr_tys.iter()) {
            let _ = writeln!(self.deferred_impls, "    {} {};", ptr_ty, cap_name);
        }
        if all_captures.is_empty() {
            let _ = writeln!(self.deferred_impls, "    char _dummy;");
        }
        let _ = writeln!(self.deferred_impls, "}} {};", ctx_struct);

        // ---- Emit heap-allocated vtable and context ----
        // Heap allocation ensures handlers can be returned from functions safely.
        let vtable_var = format!("{}_vtable", handler_id);
        let ctx_var = format!("{}_ctx", handler_id);
        self.line(&format!(
            "NovaVtable_{eff}* {vt} = (NovaVtable_{eff}*)nova_alloc(sizeof(NovaVtable_{eff}));",
            eff = eff, vt = vtable_var
        ));
        self.line(&format!(
            "{ctx_ty}* {ctx} = ({ctx_ty}*)nova_alloc(sizeof({ctx_ty}));",
            ctx_ty = ctx_struct, ctx = ctx_var
        ));
        for ((cap_name, cap_ty), _ptr_ty) in all_captures.iter().zip(capture_ptr_tys.iter()) {
            if cap_ty.ends_with('*') {
                // Pointer type: store the pointer value directly (heap object, no indirection)
                self.line(&format!("{ctx}->{cap} = {cap};", ctx = ctx_var, cap = cap_name));
            } else {
                // Scalar/struct: store address so mutations are visible to caller
                self.line(&format!("{ctx}->{cap} = &{cap};", ctx = ctx_var, cap = cap_name));
            }
        }
        // Patch vtable at runtime — use mangled field name for overloaded ops
        self.line(&format!("{vt}->ctx = {ctx};",
            vt = vtable_var, ctx = ctx_var));
        for m in methods {
            let fn_name = format!("{}_impl_{}_{}", handler_id, eff, m.name);
            // Resolve mangled vtable field: look up by plain name in schema,
            // find the matching key (mangled or plain).
            let field = {
                let schema_snap = self.effect_schemas.get(&eff).cloned().unwrap_or_default();
                // Find the schema key whose prefix matches m.name
                let mangled_key = schema_snap.keys()
                    .find(|k| *k == &m.name || k.starts_with(&format!("{}__", m.name)))
                    .cloned()
                    .unwrap_or_else(|| m.name.clone());
                mangled_key
            };
            self.line(&format!("{vt}->{field} = {fn};",
                vt = vtable_var, field = field, fn = fn_name));
        }
        // Plan 20 Ф.8 (4): vtable->prev initialized to NULL here.
        // Будет перезаписан в `with X = h { ... }` codegen перед install'ом
        // (см. emit_with: `h->prev = _nova_handler_X; _nova_handler_X = h;`).
        // Для эффектов БЕЗ `prev` поля (не Fail-shaped) это noop — мы
        // эмитим `prev = NULL` только для встроенных Fail-like vtables.
        // Bootstrap-stage: hardcoded на эффект "Fail" — единственный с
        // prev в runtime.
        if eff == "Fail" {
            self.line(&format!("{vt}->prev = NULL;", vt = vtable_var));
        }

        // ---- Emit forward declarations into deferred_impls (file scope) ----
        for m in methods {
            let (param_types, ret_ty) = Self::schema_lookup(&schema, &m.name)
                .cloned()
                .unwrap_or_else(|| (vec![], "nova_unit".into()));
            let mut fn_params = vec!["void* _ctx".to_string()];
            for (i, p) in m.params.iter().enumerate() {
                let ty = param_types.get(i).cloned().unwrap_or_else(|| "nova_int".into());
                fn_params.push(format!("{} {}", ty, p.name));
            }
            let fn_name = format!("{}_impl_{}_{}", handler_id, eff, m.name);
            let _ = writeln!(self.deferred_impls,
                "static {ret} {fn}({params});",
                ret = ret_ty, fn = fn_name, params = fn_params.join(", ")
            );
        }

        // Return pointer to vtable (caller installs it)
        let result_ptr = self.fresh_tmp();
        self.line(&format!(
            "NovaVtable_{eff}* {res} = {vt};",
            eff = eff, res = result_ptr, vt = vtable_var
        ));

        // ---- Emit impl function bodies into DEFERRED file-scope buffer ----
        // We need to temporarily redirect emit_expr output to the deferred buffer.
        // Strategy: swap out/indent, emit, swap back.
        let saved_out    = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;

        for m in methods {
            let (param_types, ret_ty) = schema.get(&m.name)
                .cloned()
                .unwrap_or_else(|| (vec![], "nova_unit".into()));

            let mut fn_params = vec!["void* _ctx".to_string()];
            let mut method_param_types: Vec<(String, String)> = Vec::new();
            for (i, p) in m.params.iter().enumerate() {
                let ty = param_types.get(i).cloned().unwrap_or_else(|| "nova_int".into());
                fn_params.push(format!("{} {}", ty, p.name));
                method_param_types.push((p.name.clone(), ty));
            }

            let fn_name = format!("{}_impl_{}_{}", handler_id, eff, m.name);

            // Emit function signature + ctx unpacking into self.out (which we'll move to deferred)
            self.line(&format!(
                "static {ret} {fn}({params}) {{",
                ret = ret_ty, fn = fn_name, params = fn_params.join(", ")
            ));
            self.indent += 1;

            // Register method params in var_types so infer_expr_c_type works inside the body
            let saved_params: Vec<(String, Option<String>)> = method_param_types.iter()
                .map(|(n, t)| (n.clone(), self.var_types.insert(n.clone(), t.clone())))
                .collect();

            // Unpack context: expose captured variables so body code can use them directly
            self.line(&format!("{ctx}* _c = ({ctx}*)_ctx;", ctx = ctx_struct));
            for (cap_name, cap_ty) in &all_captures {
                if cap_ty.ends_with('*') {
                    // Pointer-typed capture stored directly: `#define cap (_c->cap)` (no deref)
                    self.line(&format!("#define {cap} (_c->{cap})", cap = cap_name));
                } else {
                    // Scalar capture stored as pointer: `#define cap (*_c->cap)` (deref)
                    self.line(&format!("#define {cap} (*_c->{cap})", cap = cap_name));
                }
            }

            match &m.body {
                HandlerMethodBody::Expr(e) => {
                    let v = self.emit_expr(e)?;
                    if ret_ty == "nova_unit" {
                        self.line(&format!("(void)({}); return NOVA_UNIT;", v));
                    } else if v == "NOVA_UNIT" {
                        // Body was interrupt/throw (unreachable); emit a type-correct zero.
                        let zero = Self::zero_literal_for_type(&ret_ty);
                        self.line(&format!("return {};", zero));
                    } else {
                        self.line(&format!("return {};", v));
                    }
                }
                HandlerMethodBody::Block(b) => {
                    let hm_block_id = self.enter_defer_scope(b, false);
                    for stmt in &b.stmts {
                        self.emit_stmt(stmt)?;
                    }
                    let last_is_return = b.stmts.last()
                        .map(|s| matches!(s, Stmt::Return { .. }))
                        .unwrap_or(false);
                    if let Some(trailing) = &b.trailing {
                        let v = self.emit_expr(trailing)?;
                        self.leave_defer_scope(hm_block_id);
                        if ret_ty == "nova_unit" {
                            self.line(&format!("(void)({}); return NOVA_UNIT;", v));
                        } else if v == "NOVA_UNIT" {
                            // Trailing was interrupt/throw (unreachable); emit type-correct zero.
                            let zero = Self::zero_literal_for_type(&ret_ty);
                            self.line(&format!("return {};", zero));
                        } else {
                            self.line(&format!("return {};", v));
                        }
                    } else if last_is_return {
                        self.leave_defer_scope(hm_block_id);
                        // Explicit return already emitted — no additional return needed.
                    } else if ret_ty == "nova_unit" {
                        self.leave_defer_scope(hm_block_id);
                        self.line("return NOVA_UNIT;");
                    } else {
                        self.leave_defer_scope(hm_block_id);
                        // No trailing expr: body likely ended with interrupt/throw (unreachable).
                        // Emit a zero return to satisfy the C type checker.
                        let zero = Self::zero_literal_for_type(&ret_ty);
                        self.line(&format!("return {};", zero));
                    }
                }
            }

            // Undef the macros so they don't leak
            for (cap_name, _) in &all_captures {
                self.line(&format!("#undef {}", cap_name));
            }

            // Restore var_types state for method params
            for (name, prev) in saved_params {
                match prev {
                    Some(old) => { self.var_types.insert(name, old); }
                    None => { self.var_types.remove(&name); }
                }
            }

            self.indent -= 1;
            self.line("}");
            self.line("");
        }

        // Move the emitted impl functions into deferred_impls
        let impl_code = std::mem::replace(&mut self.out, saved_out);
        self.deferred_impls.push_str(&impl_code);
        self.indent = saved_indent;

        Ok(result_ptr)
    }

    // ---- spawn ----

    fn emit_spawn(&mut self, body: &Expr) -> Result<String, String> {
        // D50/D71: spawn разрешён только внутри structured-scope.
        // В bootstrap-codegen — только supervised. Вне scope — compile error.
        if self.current_scope_queue.is_none() {
            return Err(format!(
                "spawn is only allowed inside `supervised`, `parallel for` or other structured-scope (D50). \
                 Wrap your code in `supervised {{ ... }}` to enable concurrent execution."
            ));
        }
        let spawn_id = format!("_nova_spawn_{}", self.spawn_counter);
        self.spawn_counter += 1;

        // Collect all identifiers referenced in the spawn body
        let mut refs: Vec<String> = Vec::new();
        Self::collect_idents_expr(body, &mut refs);
        refs.sort();
        refs.dedup();

        // Collect all names *bound* inside the spawn body (let bindings, for patterns, match arms).
        // These are local to the spawn and must not be captured from the outer scope.
        let mut bound: std::collections::HashSet<String> = std::collections::HashSet::new();
        Self::collect_bound_names_expr(body, &mut bound);

        // A name is a capture only if: it is in outer var_types AND not bound inside spawn.
        // Each capture is recorded with `by_value` flag:
        //   - immutable scalar (let, not let mut, type ∈ {nova_int, nova_bool, nova_f64})
        //     → captured BY VALUE (snapshot at spawn site).
        //   - everything else (mutable, or non-scalar) → BY POINTER (shared mutation).
        // Rationale: parallel-for / supervised holds spawns until end-of-scope; loop-
        // variables (`let cur = xs[i]`) are immutable scalars that change ВНЕШНЕ across
        // iterations. Capturing by value snapshots them; by pointer would let all queued
        // fibers see the last iteration's value.
        let mut captures: Vec<(String, String, bool)> = Vec::new();
        for name in refs {
            if bound.contains(&name) {
                continue;
            }
            if let Some(ty) = self.var_types.get(&name).cloned() {
                let is_scalar = matches!(ty.as_str(),
                    "nova_int" | "nova_bool" | "nova_f64" | "nova_f32" | "nova_byte");
                let is_mut = self.var_mutable.contains(&name);
                let by_value = is_scalar && !is_mut;
                captures.push((name, ty, by_value));
            }
        }

        // Ctx struct typedef is emitted ONLY into lambda_forward_decls (file scope).
        // We do NOT emit a duplicate typedef inside the current function: when this spawn
        // appears nested in another fiber's body, capture macros could tokenize-rewrite
        // the field declarations and break compilation.
        let ctx_ty = format!("NovaSpawnCtx_{}", &spawn_id[1..]); // strip leading _

        // Heap-alloc the ctx — each spawn inside a loop iteration needs its own ctx,
        // and the queue holds them until scope-exit. nova_alloc returns zeroed memory.
        let ctx_var = format!("{}_ctx", spawn_id);
        self.line(&format!("{ty}* {var} = ({ty}*)nova_alloc(sizeof({ty}));",
            ty = ctx_ty, var = ctx_var));

        for (cap, _, by_value) in &captures {
            // If cap is itself a capture of the *enclosing* fiber, the outer ctx field
            // is either T (by-value) or T* (by-pointer). For inner spawn:
            //   - inner by-value: copy the current value; if outer is by-pointer, deref.
            //   - inner by-pointer: pass the same pointer; if outer is by-value, take address.
            let is_outer_cap = self.current_spawn_captures.as_ref()
                .map(|s| s.contains(cap)).unwrap_or(false);
            let outer_by_value = self.current_spawn_capture_by_value.as_ref()
                .map(|s| s.contains(cap)).unwrap_or(false);
            let access_outer = if is_outer_cap {
                if outer_by_value { format!("_c->{}", cap) }            // T value
                else { format!("(*_c->{})", cap) }                       // *T
            } else {
                cap.clone()                                              // local var
            };
            let address_outer = if is_outer_cap {
                if outer_by_value { format!("&_c->{}", cap) }           // address of T field
                else { format!("_c->{}", cap) }                          // already a pointer
            } else {
                format!("&{}", cap)                                      // local var address
            };
            if *by_value {
                // Copy current value into the new ctx (snapshot).
                self.line(&format!("{ctx}->{cap} = {acc};",
                    ctx = ctx_var, cap = cap, acc = access_outer));
            } else {
                // Store a pointer for shared mutation.
                self.line(&format!("{ctx}->{cap} = {addr};",
                    ctx = ctx_var, cap = cap, addr = address_outer));
            }
        }

        // D71 `parallel for → []T`: also snapshot the per-iteration index and
        // result-array pointer so the spawn body can write its result slot.
        let parfor_slot = self.current_parfor_slot.clone();
        if let Some((idx_var, result_var, _)) = &parfor_slot {
            self.line(&format!("{ctx}->_nova_par_idx = {iv};",
                ctx = ctx_var, iv = idx_var));
            self.line(&format!("{ctx}->_nova_par_result = {rv};",
                ctx = ctx_var, rv = result_var));
        }

        // Push the fiber into the scope queue. spawn returns unit (D50/D71):
        // results from concurrent execution come through mut-captures or
        // `parallel for` (homogeneous results), never from spawn itself.
        let queue = self.current_scope_queue.clone().expect("scope queue must be active");
        // Plan 44.5 Layer 5 fix: initialize _nova_worker_slot = -1 explicitly.
        // nova_alloc zero-initializes (slot=0), but 0 is a valid slot index —
        // -1 is required as "not yet set" sentinel for the worker loop restore logic.
        self.line(&format!("{ctx}->_nova_worker_slot = -1;", ctx = ctx_var));
        // Plan 44.5 Layer 5: implicit M:N — runtime initialized → push в worker
        // deque; иначе single-thread path unchanged.
        self.line("if (nova_runtime_is_initialized()) {");
        self.indent += 1;
        self.line(&format!("{ctx}->_nova_parent_scope = &{q};",
            ctx = ctx_var, q = queue));
        self.line(&format!("nova_runtime_spawn_into(&{q}, {id}, {ctx});",
            q = queue, id = spawn_id, ctx = ctx_var));
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.line(&format!("{ctx}->_nova_parent_scope = NULL;", ctx = ctx_var));
        self.line(&format!("nova_fiber_spawn_into(&{q}, {id}, {ctx});",
            q = queue, id = spawn_id, ctx = ctx_var));
        self.indent -= 1;
        self.line("}");

        // Emit the ctx-struct typedef into lambda_forward_decls — flushed before the
        // current function in `out`, so the typedef is visible at the spawn-instance
        // declaration site (and also for the entry fn body which lives in deferred_impls).
        //
        // Plan 44.5 Layer 5 fix: base fields MUST be FIRST so NovaSpawnCtxBase* cast
        // in runtime.c worker loop is safe (fixed offsets). User captures follow.
        let _ = writeln!(self.lambda_forward_decls, "typedef struct {{");
        // Base fields first (NovaSpawnCtxBase layout — must match fibers.h).
        // Plan 44.5 Layer 5: parent scope для remote-spawn tracking.
        // NULL = single-thread (без runtime.init). Always present.
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaFiberQueue* _nova_parent_scope;");
        // Plan 44.5 Layer 5 park/wake: slot index в worker scope.
        // Initialized to -1; set by preamble on first run. -1 = not yet set.
        let _ = writeln!(self.lambda_forward_decls,
            "    int _nova_worker_slot;");
        // Plan 44.5 Layer 5 fix: per-fiber fail/interrupt-top chain snapshot.
        // Worker saves/restores these around mco_resume to isolate fiber fail-stacks.
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaFailFrame* _nova_saved_fail_top;");
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaInterruptFrame* _nova_saved_interrupt_top;");
        // Plan 44.5 Layer 5 deadlock fix: home worker scope for work-stealing.
        // Set once in preamble; worker restores _nova_active_scope from this
        // before each mco_resume so channel ops always capture the correct scope.
        // NULL before preamble runs (_nova_worker_slot == -1 guards that path).
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaFiberQueue* _nova_fiber_scope;");
        // User capture fields follow base fields.
        for (cap, ty, by_value) in &captures {
            if *by_value {
                let _ = writeln!(self.lambda_forward_decls, "    {} {};", ty, cap);
            } else {
                let _ = writeln!(self.lambda_forward_decls, "    {}* {};", ty, cap);
            }
        }
        // D71 `parallel for → []T`: extra ctx fields for per-iteration result write.
        if let Some((_, _, elem_ty)) = &parfor_slot {
            let _ = writeln!(self.lambda_forward_decls, "    int64_t _nova_par_idx;");
            let _ = writeln!(self.lambda_forward_decls,
                "    NovaArray_{}* _nova_par_result;", elem_ty);
        }
        // Note: no `_nova_result` field — spawn returns unit (D50/D71).
        let _ = writeln!(self.lambda_forward_decls, "}} {};", ctx_ty);

        // Swap out to deferred_impls for body emission
        let saved_out    = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;

        // Plan 48 Ф.4 ([M-mono-spawn-fwd-decls]): pre-scan `scan_expr_fwd`
        // emits forward declarations for every spawn-body it sees in the
        // ORIGINAL module AST, before fn definitions. Monomorphized fn
        // bodies are synthesized during the mono-worklist drain, AFTER the
        // pre-scan ran — so spawn-bodies emitted from inside a mono'd fn
        // have no pre-scan'ed forward decl, and the mono'd fn would
        // reference `_nova_spawn_N` undefined-identifier. Detect mono
        // context via non-empty `current_type_subst` and push the missing
        // forward decl into `mono_fwd_decls`, which gets spliced into the
        // `/*__MONO_FWD_DECLS__*/` marker at the top of the C file (before
        // any fn definition). Idempotent: each spawn_id is unique per
        // counter, no risk of duplicates.
        if !self.current_type_subst.is_empty() {
            self.mono_fwd_decls.push_str(&format!(
                "static void {}(mco_coro* _co);\n", spawn_id));
        }

        self.line(&format!("static void {}(mco_coro* _co) {{", spawn_id));
        self.indent += 1;
        // Plan 44.5 Layer 5: _c всегда нужен — entry function reads
        // _c->_nova_parent_scope для remote-fiber cleanup (decrement
        // pending_remote + signal_main). Раньше для empty-capture spawn
        // было `(void)_co;` — теперь _c always.
        self.line(&format!("{ctx}* _c = ({ctx}*)mco_get_user_data(_co);", ctx = ctx_ty));
        // Plan 44.5 Layer 5 park/wake: alloc slot in worker scope on first resume.
        // Required so _nova_active_slot >= 0 (D92 invariant) when fiber calls
        // Time.sleep / Channel.recv in worker context.
        // Single-thread path: _nova_parent_scope == NULL → skip.
        self.line("if (_c->_nova_parent_scope) {");
        self.indent += 1;
        self.line("_nova_active_slot = nova_scope_alloc_slot(_nova_active_scope, _co);");
        self.line("_c->_nova_worker_slot = _nova_active_slot;");
        self.line("_c->_nova_fiber_scope = _nova_active_scope;");
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.line("_c->_nova_worker_slot = -1;");
        self.indent -= 1;
        self.line("}");
        // Activate capture rewriting: ExprKind::Ident → `(*_c->name)` or `_c->name`.
        let mut cap_set: HashSet<String> = HashSet::new();
        let mut cap_by_value: HashSet<String> = HashSet::new();
        for (cap, _, by_value) in &captures {
            cap_set.insert(cap.clone());
            if *by_value { cap_by_value.insert(cap.clone()); }
        }
        let prev_caps = std::mem::replace(&mut self.current_spawn_captures, Some(cap_set));
        let prev_by_value = std::mem::replace(&mut self.current_spawn_capture_by_value, Some(cap_by_value));

        // Wrap body in a fail-frame so that `throw` inside fiber is caught here
        // (longjmp lands on THIS fiber's stack — safe). After catch, report the
        // error to the active scope queue via nova_fiber_report_error, and let
        // the fiber finish cleanly. Scope-runner re-throws on main flow after
        // all fibers have been drained (nova_supervised_run).
        self.line("NovaFailFrame _ff;");
        self.line("nova_fail_push(&_ff);");
        self.line("if (setjmp(_ff.jmp) == 0) {");
        self.indent += 1;

        // Inside the spawn body, the parfor_slot belongs to THIS spawn — but any
        // *nested* spawn must not inherit it. Temporarily disable while emitting body.
        let saved_parfor = self.current_parfor_slot.take();

        // Emit body, discard its value (spawn returns unit) — UNLESS in parfor mode,
        // where the trailing expression's value is written to result[idx].
        match &body.kind {
            ExprKind::Block(b) => {
                for stmt in &b.stmts {
                    self.emit_stmt(stmt)?;
                }
                if let Some(trailing) = &b.trailing {
                    let v = self.emit_expr(trailing)?;
                    if saved_parfor.is_some() {
                        self.line(&format!("_c->_nova_par_result->data[_c->_nova_par_idx] = {};", v));
                    } else {
                        self.line(&format!("(void)({});", v));
                    }
                }
            }
            _ => {
                let v = self.emit_expr(body)?;
                if saved_parfor.is_some() {
                    self.line(&format!("_c->_nova_par_result->data[_c->_nova_par_idx] = {};", v));
                } else {
                    self.line(&format!("(void)({});", v));
                }
            }
        }

        // Restore parfor_slot so the surrounding emit_parallel_for can clear it
        // after the for-loop body has run.
        self.current_parfor_slot = saved_parfor;

        self.line("nova_fail_pop();");
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.line("nova_fail_pop();");
        // D61: distinguish real error from cross-mco-boundary `interrupt v`
        // (nova_interrupt sets the sentinel message "__nova_interrupt__" and
        // populates scope->interrupt_pending/interrupt_value). For interrupt
        // we DON'T report a fiber error — supervised_run will re-issue the
        // interrupt on main-flow after drain.
        self.line("if (_ff.error_msg.ptr && _ff.error_msg.len == 18 && memcmp(_ff.error_msg.ptr, \"__nova_interrupt__\", 18) == 0) {");
        self.indent += 1;
        self.line("/* interrupt: scope state already set, fiber dies cleanly */");
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        // Plan 44.5 Layer 5: error reporting — remote vs local.
        // Plan 49 Ф.2 + Ф.5: kinded — пробрасываем _ff.error_kind / error_reason_ptr.
        // Local path: USER-precedence через nova_fiber_report_error_kinded.
        // Remote path: kinded atomic report (compare-kind CAS-loop —
        // CANCEL→USER overwrite, иначе keep).
        self.line("if (_c->_nova_parent_scope) {");
        self.indent += 1;
        self.line("nova_fiber_report_atomic_kinded(_c->_nova_parent_scope, _ff.error_msg.ptr, _ff.error_kind, _ff.error_reason_ptr);");
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.line("nova_fiber_report_error_kinded(_ff.error_msg.ptr, _ff.error_kind, _ff.error_reason_ptr);");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");

        // Plan 44.5 Layer 5: remote fiber post-completion cleanup.
        // (1) Free worker scope slot (alloc'd in preamble) — before decrement
        //     so the slot is available for the next fiber immediately.
        // (2) Decrement parent's pending_remote (release ordering) — main
        //     thread в supervised_run wait-loop увидит decrement через
        //     nova_aint_load(acquire). signal_main wake'ом wake'ает main
        //     thread'а из uv_run(UV_RUN_ONCE).
        self.line("if (_c->_nova_parent_scope) {");
        self.indent += 1;
        self.line("if (_c->_nova_worker_slot >= 0) {");
        self.indent += 1;
        self.line("nova_scope_free_slot(_nova_active_scope, _c->_nova_worker_slot);");
        self.line("_nova_active_slot = -1;");
        self.indent -= 1;
        self.line("}");
        self.line("(void)nova_aint_fetch_sub_release(&_c->_nova_parent_scope->pending_remote);");
        self.line("nova_runtime_signal_main();");
        self.indent -= 1;
        self.line("}");

        // Deactivate capture rewriting before emitting closing brace.
        self.current_spawn_captures = prev_caps;
        self.current_spawn_capture_by_value = prev_by_value;
        self.indent -= 1;
        self.line("}");
        self.line("");

        let entry_code = std::mem::replace(&mut self.out, saved_out);
        self.deferred_impls.push_str(&entry_code);
        self.indent = saved_indent;

        // spawn evaluates to unit.
        Ok("NOVA_UNIT".to_string())
    }

    // ---- supervised scope ----

    /// Emit `supervised { body }` — D50 structured-concurrency scope.
    /// All `spawn` inside the body push fibers into a local NovaFiberQueue;
    /// at scope-exit, nova_supervised_run drives them round-robin to completion.
    /// Emit `supervised { body }` / `supervised(cancel: tok) { body }`
    /// (D50 / D75 revised, Plan 47).
    ///
    /// Если `cancel` присутствует — токен (`NovaCancelToken*`) вычисляется
    /// при входе в scope, ПРИВЯЗЫВАЕТСЯ к scope-queue прямо перед
    /// `nova_supervised_run_cancel` (после эмиссии тела — так прямой `throw`
    /// в стейтменте тела не оставит висящий `bound_scope`), и ОТВЯЗЫВАЕТСЯ
    /// внутри `nova_supervised_run_cancel` на всех путях выхода (нормальный
    /// возврат + re-throw).
    fn emit_supervised(&mut self, body: &Block, cancel: Option<&Expr>)
        -> Result<String, String>
    {
        let id = self.supervised_counter;
        self.supervised_counter += 1;
        let queue_var = format!("_nova_scope_q_{}", id);
        let prev_scope_var = format!("_nova_prev_scope_{}", id);

        // Wrap the scope in a C block so the queue is local.
        self.line("{");
        self.indent += 1;
        self.line(&format!("NovaFiberQueue {} = {{0}};", queue_var));
        self.line(&format!("nova_scope_init(&{});", queue_var));

        // Plan 47: evaluate the cancel-token expr at scope entry — source
        // order, `(cancel: tok)` стоит до `{ body }`. Кладём в temp;
        // bind происходит ниже, после тела, перед run'ом.
        let cancel_tok_var = if let Some(cexpr) = cancel {
            let cval = self.emit_expr(cexpr)?;
            let tv = format!("_nova_cancel_tok_{}", id);
            self.line(&format!("NovaCancelToken* {} = {};", tv, cval));
            Some(tv)
        } else {
            None
        };

        // Set _nova_active_scope to this queue so that on main-flow,
        // Time.sleep (default handler) finds the right scope to drive.
        // Saved/restored around the body.
        self.line(&format!("NovaFiberQueue* {} = _nova_active_scope;", prev_scope_var));
        self.line(&format!("_nova_active_scope = &{};", queue_var));

        // Activate scope: spawn inside body routes into queue.
        let prev = std::mem::replace(&mut self.current_scope_queue, Some(queue_var.clone()));

        // Emit body statements with defer scope (supervised body can contain defer).
        let block_id = self.enter_defer_scope(body, false);
        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            let v = self.emit_expr(trailing)?;
            self.line(&format!("(void)({});", v));
        }
        self.leave_defer_scope(block_id);

        // Restore scope state.
        self.current_scope_queue = prev;

        // Run the scheduler: round-robin until all fibers in queue are dead.
        // Plan 47: для cancel-формы — bind токена к scope-queue здесь (после
        // тела: прямой throw в body-стейтменте не оставит dangling
        // bound_scope), затем run-with-cancel; unbind делается внутри
        // nova_supervised_run_cancel перед нормальным возвратом И перед
        // любым re-throw.
        if let Some(tv) = &cancel_tok_var {
            self.line(&format!("nova_cancel_token_bind({}, &{});", tv, queue_var));
            self.line(&format!("nova_supervised_run_cancel(&{}, {});", queue_var, tv));
        } else {
            self.line(&format!("nova_supervised_run(&{});", queue_var));
        }
        // Restore previous active scope (may be NULL or outer scope).
        self.line(&format!("_nova_active_scope = {};", prev_scope_var));

        self.indent -= 1;
        self.line("}");

        // supervised expression evaluates to unit.
        Ok("NOVA_UNIT".to_string())
    }

    /// Emit `parallel for x in iter { body }` — D14 fan-out via desugar to
    /// `supervised { for x in iter { spawn { body } } }`. Each iteration spawns
    /// a fiber capturing the loop-variable BY VALUE (immutable scalar) so all
    /// queued fibers see their own snapshot.
    ///
    /// D71 `parallel for → []T`: when `body` has a trailing expression, the
    /// parallel-for evaluates to `[]T` where T is the trailing's type. Each
    /// fiber writes its result into `result.data[idx]` at a pre-allocated slot.
    /// When body has no trailing (purely effectful), the form yields unit.
    fn emit_parallel_for(
        &mut self,
        pattern: &Pattern,
        iter: &Expr,
        body: &Block,
    ) -> Result<String, String> {
        use crate::diag::Span;
        let span = Span::dummy();

        // Detect array-mode: body has a trailing expression that yields a value.
        // In that case we evaluate the trailing's type to size the result array
        // and route each spawn's value into result[idx].
        let array_mode = body.trailing.is_some();

        if !array_mode {
            // Statement-mode (legacy): pure desugar.
            let spawn_body_expr = Expr::new(ExprKind::Block(body.clone()), span);
            let spawn_expr = Expr::new(ExprKind::Spawn(Box::new(spawn_body_expr)), span);
            let for_body = Block {
                stmts: vec![Stmt::Expr(spawn_expr)],
                trailing: None,
                span,
            };
            let for_expr = Expr::new(
                ExprKind::For { pattern: pattern.clone(), iter: Box::new(iter.clone()), body: for_body, invariants: vec![], decreases: None },
                span,
            );
            let supervised_block = Block { stmts: vec![Stmt::Expr(for_expr)], trailing: None, span };
            return self.emit_supervised(&supervised_block, None);
        }

        // Array-mode: infer element type from the trailing expression.
        // Best-effort — fall back to nova_int.
        let trailing = body.trailing.as_ref().unwrap();
        let elem_ty = self.infer_expr_c_type(trailing);
        // Element type names used in NovaArray_T are restricted to a few primitives.
        // For pointer types or unsupported forms, conservatively bail out by pretending
        // the body has no trailing — caller ends up with a unit `parallel for`.
        let elem_ty_name = match elem_ty.as_str() {
            "nova_int" | "nova_bool" | "nova_f64" | "nova_str" => elem_ty.clone(),
            _ => {
                // Unsupported element type for D71 v1 — degrade to statement mode.
                let spawn_body_expr = Expr::new(ExprKind::Block(body.clone()), span);
                let spawn_expr = Expr::new(ExprKind::Spawn(Box::new(spawn_body_expr)), span);
                let for_body = Block {
                    stmts: vec![Stmt::Expr(spawn_expr)],
                    trailing: None,
                    span,
                };
                let for_expr = Expr::new(
                    ExprKind::For { pattern: pattern.clone(), iter: Box::new(iter.clone()), body: for_body, invariants: vec![], decreases: None },
                    span,
                );
                let supervised_block = Block { stmts: vec![Stmt::Expr(for_expr)], trailing: None, span };
                return self.emit_supervised(&supervised_block, None);
            }
        };

        // ---- emit array-mode lowering, by hand ----
        // 1) compute iteration count N and a per-iteration "current value" expression.
        //    Supported iterators: ArrayLit, Range a..b, RangeInclusive a..=b, Ident
        //    bound to an array. For unsupported iter shapes, fall back to unit.
        let len_expr: String;
        let iter_setup: String; // C statements that set up `nova_int _i` and per-iter `cur` value
        let cur_value_expr: String; // expression evaluating to the current loop element

        match &iter.kind {
            ExprKind::Range { start, end, inclusive } => {
                let s = self.emit_expr(start)?;
                let e = self.emit_expr(end)?;
                let plus_one = if *inclusive { " + 1" } else { "" };
                len_expr = format!("({} - {}{})", e, s, plus_one);
                iter_setup = format!("nova_int _nova_par_start = {}; (void)_nova_par_start;", s);
                cur_value_expr = "(_nova_par_start + _nova_par_i)".to_string();
            }
            ExprKind::ArrayLit(elems) => {
                // No spread support in v1.
                if elems.iter().any(|e| matches!(e, ArrayElem::Spread(_))) {
                    let spawn_body_expr = Expr::new(ExprKind::Block(body.clone()), span);
                    let spawn_expr = Expr::new(ExprKind::Spawn(Box::new(spawn_body_expr)), span);
                    let for_body = Block { stmts: vec![Stmt::Expr(spawn_expr)], trailing: None, span };
                    let for_expr = Expr::new(
                        ExprKind::For { pattern: pattern.clone(), iter: Box::new(iter.clone()), body: for_body, invariants: vec![], decreases: None },
                        span,
                    );
                    let supervised_block = Block { stmts: vec![Stmt::Expr(for_expr)], trailing: None, span };
                    return self.emit_supervised(&supervised_block, None);
                }
                // Materialise the array once, then walk indices.
                let arr_var = format!("_nova_par_src_{}", self.tmp_counter);
                self.tmp_counter += 1;
                // Element C type for the *iter* array — for simplicity, assume nova_int when
                // not inferable; the loop variable is bound as nova_int regardless.
                let iter_elem_ty = "nova_int".to_string();
                let mut emitted = Vec::with_capacity(elems.len());
                for el in elems {
                    if let ArrayElem::Item(x) = el {
                        emitted.push(self.emit_expr(x)?);
                    }
                }
                self.line(&format!("NovaArray_{}* {} = nova_array_new_{}({});",
                    iter_elem_ty, arr_var, iter_elem_ty,
                    if elems.is_empty() { 8 } else { elems.len() }));
                for v in &emitted {
                    self.line(&format!("nova_array_push_{}({}, {});", iter_elem_ty, arr_var, v));
                }
                len_expr = format!("{}->len", arr_var);
                iter_setup = format!("NovaArray_{}* _nova_par_src = {}; (void)_nova_par_src;",
                    iter_elem_ty, arr_var);
                cur_value_expr = "_nova_par_src->data[_nova_par_i]".to_string();
            }
            ExprKind::Ident(name) => {
                // Assume name is bound to NovaArray_T*.
                let arr_var = format!("(({})", name);
                len_expr = format!("{})->len", arr_var);
                iter_setup = format!("NovaArray_{}* _nova_par_src = {}; (void)_nova_par_src;",
                    elem_ty_name, name);
                cur_value_expr = "_nova_par_src->data[_nova_par_i]".to_string();
            }
            _ => {
                // Unsupported iter shape for array-mode; degrade.
                let spawn_body_expr = Expr::new(ExprKind::Block(body.clone()), span);
                let spawn_expr = Expr::new(ExprKind::Spawn(Box::new(spawn_body_expr)), span);
                let for_body = Block {
                    stmts: vec![Stmt::Expr(spawn_expr)],
                    trailing: None,
                    span,
                };
                let for_expr = Expr::new(
                    ExprKind::For { pattern: pattern.clone(), iter: Box::new(iter.clone()), body: for_body, invariants: vec![], decreases: None },
                    span,
                );
                let supervised_block = Block { stmts: vec![Stmt::Expr(for_expr)], trailing: None, span };
                return self.emit_supervised(&supervised_block, None);
            }
        };

        // 2) Declare the result array at the *outer* scope so its name is visible
        //    after the supervised block runs (we don't have GCC statement-exprs).
        let id = self.supervised_counter;
        self.supervised_counter += 1;
        let queue_var = format!("_nova_scope_q_{}", id);
        let prev_scope_var = format!("_nova_prev_scope_{}", id);
        let result_var = format!("_nova_par_res_{}", id);
        let len_var = format!("_nova_par_len_{}", id);

        self.line(&iter_setup);
        self.line(&format!("nova_int {} = {};", len_var, len_expr));
        self.line(&format!("NovaArray_{ty}* {res} = nova_array_new_{ty}({len});",
            ty = elem_ty_name, res = result_var, len = len_var));
        self.line(&format!("{res}->len = {len};", res = result_var, len = len_var));

        // Open scope-block.
        self.line("{");
        self.indent += 1;
        self.line(&format!("NovaFiberQueue {} = {{0}};", queue_var));
        self.line(&format!("nova_scope_init(&{});", queue_var));
        self.line(&format!("NovaFiberQueue* {} = _nova_active_scope;", prev_scope_var));
        self.line(&format!("_nova_active_scope = &{};", queue_var));

        // Activate scope for nested spawns.
        let prev = std::mem::replace(&mut self.current_scope_queue, Some(queue_var.clone()));

        // 3) Emit the for-loop in C.
        self.line(&format!("for (nova_int _nova_par_i = 0; _nova_par_i < {}; _nova_par_i++) {{", len_var));
        self.indent += 1;
        let bind_name = match pattern {
            Pattern::Ident { name, .. } => name.clone(),
            _ => "_nova_par_loopvar".to_string(),
        };
        self.line(&format!("nova_int {} = {};", bind_name, cur_value_expr));
        self.var_types.insert(bind_name.clone(), "nova_int".to_string());
        self.var_mutable.remove(&bind_name);

        // 4) Activate parfor_slot for this spawn.
        let saved_slot = self.current_parfor_slot.replace((
            "_nova_par_i".to_string(),
            result_var.clone(),
            elem_ty_name.clone(),
        ));

        let spawn_body_expr = Expr::new(ExprKind::Block(body.clone()), span);
        let spawn_expr = Expr::new(ExprKind::Spawn(Box::new(spawn_body_expr)), span);
        let _ = self.emit_expr(&spawn_expr)?;

        self.current_parfor_slot = saved_slot;
        self.indent -= 1;
        self.line("}");

        // 5) Run scheduler, restore scope state.
        self.current_scope_queue = prev;
        self.line(&format!("nova_supervised_run(&{});", queue_var));
        self.line(&format!("_nova_active_scope = {};", prev_scope_var));

        self.indent -= 1;
        self.line("}");

        // Track the element type so let-binding propagation (`xs[i]` typing) works.
        self.array_element_types.insert(result_var.clone(), elem_ty_name.clone());

        // The expression value is the result array pointer.
        Ok(result_var)
    }

    /// Emit `detach { body }` — D50 fire-and-forget primitive.
    /// Bootstrap default handler is SyncDetach: body executes inline in the caller's
    /// stack, no fiber, no scheduler. Production runtime would route to a global
    /// supervisor on a separate OS thread (with LogAndDrop default panic policy).
    fn emit_detach(&mut self, body: &Block) -> Result<String, String> {
        // Wrap in a C block so any locals introduced by the body don't leak.
        self.line("{");
        self.indent += 1;
        let block_id = self.enter_defer_scope(body, false);
        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            let v = self.emit_expr(trailing)?;
            self.line(&format!("(void)({});", v));
        }
        self.leave_defer_scope(block_id);
        self.indent -= 1;
        self.line("}");
        Ok("NOVA_UNIT".to_string())
    }

    /// Pre-scan the module for HandlerLit and Spawn nodes; emit file-scope forward decls.
    fn emit_handler_forward_decls(&mut self, module: &Module) -> Result<(), String> {
        let mut h_ctr = 0usize; // handler_counter
        let mut s_ctr = 0usize; // spawn_counter
        for item in &module.items {
            match item {
                Item::Fn(f) => self.scan_fn_fwd(f, &mut h_ctr, &mut s_ctr)?,
                Item::Test(t) => self.scan_block_fwd(&t.body, &mut h_ctr, &mut s_ctr)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn scan_fn_fwd(&mut self, f: &FnDecl, h: &mut usize, s: &mut usize) -> Result<(), String> {
        match &f.body {
            FnBody::Expr(e) => self.scan_expr_fwd(e, h, s),
            FnBody::Block(b) => self.scan_block_fwd(b, h, s),
            // D82: external fn — тела нет, scan'ить нечего.
            FnBody::External => Ok(()),
        }
    }

    fn scan_expr_fwd(&mut self, expr: &Expr, h: &mut usize, s: &mut usize) -> Result<(), String> {
        match &expr.kind {
            ExprKind::HandlerLit { effect_name, methods } => {
                let eff = effect_name.join("_");
                let handler_id = format!("_nova_handler_lit_{}", *h);
                *h += 1;
                let schema = self.effect_schemas.get(&eff).cloned().unwrap_or_default();
                for m in methods {
                    let (param_types, ret_ty) = Self::schema_lookup(&schema, &m.name)
                        .cloned()
                        .unwrap_or_else(|| (vec![], "nova_unit".into()));
                    let mut fn_params = vec!["void* _ctx".to_string()];
                    for (i, p) in m.params.iter().enumerate() {
                        let ty = param_types.get(i).cloned().unwrap_or_else(|| "nova_int".into());
                        fn_params.push(format!("{} {}", ty, p.name));
                    }
                    let fn_name = format!("{}_impl_{}_{}", handler_id, eff, m.name);
                    self.line(&format!(
                        "static {ret} {fn}({params});",
                        ret = ret_ty, fn = fn_name, params = fn_params.join(", ")
                    ));
                }
            }
            ExprKind::Spawn(body) => {
                let spawn_id = format!("_nova_spawn_{}", *s);
                *s += 1;
                self.line(&format!("static void {}(mco_coro* _co);", spawn_id));
                // Plan 47: рекурсия в тело spawn'а — вложенные spawn'ы
                // (`spawn { supervised { spawn {...} } }`) тоже нуждаются в
                // forward-decl, и `*s` counter обязан совпадать с emit'овским
                // (emit_spawn инкрементит spawn_counter, затем эмитит тело →
                // depth-first). Без рекурсии scan/emit рассинхронизировались
                // и вложенные entry-функции оставались undeclared.
                self.scan_expr_fwd(body, h, s)?;
            }
            ExprKind::Block(b) => self.scan_block_fwd(b, h, s)?,
            ExprKind::If { cond, then, else_ } => {
                self.scan_expr_fwd(cond, h, s)?;
                self.scan_block_fwd(then, h, s)?;
                match else_.as_ref() {
                    Some(ElseBranch::Block(b)) => self.scan_block_fwd(b, h, s)?,
                    Some(ElseBranch::If(e)) => self.scan_expr_fwd(e, h, s)?,
                    None => {}
                }
            }
            ExprKind::With { bindings, body } => {
                for b in bindings {
                    // Plan 19, C8 codegen (D31-rev): handler-лямбда
                    // должна быть desugar'ена на pre-scan в синтетический
                    // HandlerLit, иначе h-counter rassinch'нется
                    // и forward-decls не совпадут с emit_with-side.
                    if let Some((_, lit_expr)) =
                        self.desugar_handler_lambda(&b.effect, &b.handler)?
                    {
                        self.scan_expr_fwd(&lit_expr, h, s)?;
                    } else {
                        self.scan_expr_fwd(&b.handler, h, s)?;
                    }
                }
                self.scan_block_fwd(body, h, s)?;
            }
            ExprKind::Call { func, args, .. } => {
                self.scan_expr_fwd(func, h, s)?;
                for a in args { self.scan_expr_fwd(a.expr(), h, s)?; }
            }
            ExprKind::Binary { left, right, .. } => {
                self.scan_expr_fwd(left, h, s)?;
                self.scan_expr_fwd(right, h, s)?;
            }
            ExprKind::While { cond, body, .. } => {
                self.scan_expr_fwd(cond, h, s)?;
                self.scan_block_fwd(body, h, s)?;
            }
            ExprKind::For { iter, body, .. } => {
                self.scan_expr_fwd(iter, h, s)?;
                self.scan_block_fwd(body, h, s)?;
            }
            ExprKind::ParallelFor { iter, body, .. } => {
                // Desugar mirrors supervised { for x in iter { spawn { body } } }
                // — pre-scan reserves a spawn id for the implicit spawn.
                self.scan_expr_fwd(iter, h, s)?;
                self.scan_block_fwd(body, h, s)?;
                let spawn_id = format!("_nova_spawn_{}", *s);
                *s += 1;
                self.line(&format!("static void {}(mco_coro* _co);", spawn_id));
            }
            ExprKind::Loop { body, .. } => {
                self.scan_block_fwd(body, h, s)?;
            }
            ExprKind::Match { scrutinee, arms } => {
                self.scan_expr_fwd(scrutinee, h, s)?;
                for arm in arms {
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.scan_expr_fwd(e, h, s)?,
                        MatchArmBody::Block(b) => self.scan_block_fwd(b, h, s)?,
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.scan_expr_fwd(operand, h, s)?,
            ExprKind::Supervised { body, .. } => self.scan_block_fwd(body, h, s)?,
            ExprKind::Detach(b) => self.scan_block_fwd(b, h, s)?,
            _ => {}
        }
        Ok(())
    }

    fn scan_block_fwd(&mut self, block: &Block, h: &mut usize, s: &mut usize) -> Result<(), String> {
        for stmt in &block.stmts {
            self.scan_stmt_fwd(stmt, h, s)?;
        }
        if let Some(t) = &block.trailing {
            self.scan_expr_fwd(t, h, s)?;
        }
        Ok(())
    }

    fn scan_stmt_fwd(&mut self, stmt: &Stmt, h: &mut usize, s: &mut usize) -> Result<(), String> {
        match stmt {
            Stmt::Let(d) => self.scan_expr_fwd(&d.value, h, s),
            Stmt::Expr(e) => self.scan_expr_fwd(e, h, s),
            Stmt::Assign { value, .. } => self.scan_expr_fwd(value, h, s),
            Stmt::Return { value: Some(v), .. } => self.scan_expr_fwd(v, h, s),
            Stmt::Throw { value, .. } => self.scan_expr_fwd(value, h, s),
            _ => Ok(()),
        }
    }

    /// Collect all identifier names referenced inside a handler method body.
    fn collect_idents_in_handler_method(m: &HandlerMethod) -> Vec<String> {
        let mut names = Vec::new();
        match &m.body {
            HandlerMethodBody::Expr(e) => Self::collect_idents_expr(e, &mut names),
            HandlerMethodBody::Block(b) => {
                for stmt in &b.stmts {
                    Self::collect_idents_stmt(stmt, &mut names);
                }
                if let Some(t) = &b.trailing {
                    Self::collect_idents_expr(t, &mut names);
                }
            }
        }
        names.sort();
        names.dedup();
        names
    }

    /// Collect all names *introduced* (bound) inside an expression:
    /// let-bindings, for-pattern, match-arm patterns, if-let, while-let.
    /// These names are local to the spawn body and must not be treated as captures.
    fn collect_bound_names_expr(expr: &Expr, out: &mut std::collections::HashSet<String>) {
        match &expr.kind {
            ExprKind::Block(b) => Self::collect_bound_names_block(b, out),
            ExprKind::If { then, else_, .. } => {
                Self::collect_bound_names_block(then, out);
                match else_.as_ref() {
                    Some(ElseBranch::Block(b)) => Self::collect_bound_names_block(b, out),
                    Some(ElseBranch::If(e))    => Self::collect_bound_names_expr(e, out),
                    None => {}
                }
            }
            ExprKind::IfLet { pattern, then, else_, .. } => {
                Self::collect_bound_names_pattern(pattern, out);
                Self::collect_bound_names_block(then, out);
                match else_.as_ref() {
                    Some(ElseBranch::Block(b)) => Self::collect_bound_names_block(b, out),
                    Some(ElseBranch::If(e))    => Self::collect_bound_names_expr(e, out),
                    None => {}
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                Self::collect_bound_names_expr(scrutinee, out);
                for arm in arms {
                    Self::collect_bound_names_pattern(&arm.pattern, out);
                    match &arm.body {
                        MatchArmBody::Expr(e)  => Self::collect_bound_names_expr(e, out),
                        MatchArmBody::Block(b) => Self::collect_bound_names_block(b, out),
                    }
                }
            }
            ExprKind::For { pattern, iter, body, .. }
            | ExprKind::ParallelFor { pattern, iter, body, .. } => {
                Self::collect_bound_names_expr(iter, out);
                Self::collect_bound_names_pattern(pattern, out);
                Self::collect_bound_names_block(body, out);
            }
            ExprKind::While { body, .. } => Self::collect_bound_names_block(body, out),
            ExprKind::WhileLet { pattern, body, .. } => {
                Self::collect_bound_names_pattern(pattern, out);
                Self::collect_bound_names_block(body, out);
            }
            ExprKind::Loop { body, .. } => Self::collect_bound_names_block(body, out),
            ExprKind::With { body, .. } => Self::collect_bound_names_block(body, out),
            ExprKind::Supervised { body, .. } => Self::collect_bound_names_block(body, out),
            ExprKind::Detach(body) => Self::collect_bound_names_block(body, out),
            ExprKind::Select { arms } => {
                for arm in arms {
                    if let SelectOp::Recv { binding: Some(b), .. } = &arm.op {
                        out.insert(b.clone());
                    }
                    Self::collect_bound_names_block(&arm.body, out);
                }
            }
            _ => {}
        }
    }

    fn collect_bound_names_block(block: &Block, out: &mut std::collections::HashSet<String>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Let(d) => {
                    Self::collect_bound_names_pattern(&d.pattern, out);
                    Self::collect_bound_names_expr(&d.value, out);
                }
                Stmt::Expr(e) => Self::collect_bound_names_expr(e, out),
                Stmt::Assign { target, value, .. } => {
                    Self::collect_bound_names_expr(target, out);
                    Self::collect_bound_names_expr(value, out);
                }
                _ => {}
            }
        }
        if let Some(t) = &block.trailing {
            Self::collect_bound_names_expr(t, out);
        }
    }

    fn collect_bound_names_pattern(pat: &Pattern, out: &mut std::collections::HashSet<String>) {
        match pat {
            Pattern::Ident { name, .. } => { out.insert(name.clone()); }
            Pattern::Binding { name, inner, .. } => {
                out.insert(name.clone());
                Self::collect_bound_names_pattern(inner, out);
            }
            Pattern::Variant { kind, .. } => {
                if let VariantPatternKind::Tuple { patterns, .. } = kind {
                    for p in patterns { Self::collect_bound_names_pattern(p, out); }
                }
            }
            Pattern::Record { fields, .. } => {
                for f in fields {
                    if let Some(p) = &f.pattern {
                        Self::collect_bound_names_pattern(p, out);
                    } else {
                        out.insert(f.name.clone());
                    }
                }
            }
            Pattern::Array { elems, .. } => {
                for e in elems {
                    match e {
                        ArrayPatternElem::Item(p) => Self::collect_bound_names_pattern(p, out),
                        ArrayPatternElem::RestBind(name) => { out.insert(name.clone()); }
                        ArrayPatternElem::Rest => {}
                    }
                }
            }
            Pattern::Tuple(pats, _) => {
                for p in pats { Self::collect_bound_names_pattern(p, out); }
            }
            Pattern::Or { alternatives, .. } => {
                // Используем bindings из первого альтернатива (canonical).
                if let Some(first) = alternatives.first() {
                    Self::collect_bound_names_pattern(first, out);
                }
            }
            Pattern::Wildcard(_) | Pattern::Literal(_, _) => {}
        }
    }

    fn collect_idents_expr(expr: &Expr, out: &mut Vec<String>) {
        match &expr.kind {
            ExprKind::Ident(name) => out.push(name.clone()),
            ExprKind::Binary { left, right, .. } => {
                Self::collect_idents_expr(left, out);
                Self::collect_idents_expr(right, out);
            }
            ExprKind::Unary { operand, .. } => Self::collect_idents_expr(operand, out),
            ExprKind::Call { func, args, .. } => {
                Self::collect_idents_expr(func, out);
                for a in args { Self::collect_idents_expr(a.expr(), out); }
            }
            ExprKind::Member { obj, .. } => Self::collect_idents_expr(obj, out),
            ExprKind::Index { obj, index } => {
                Self::collect_idents_expr(obj, out);
                Self::collect_idents_expr(index, out);
            }
            ExprKind::If { cond, then, else_ } => {
                Self::collect_idents_expr(cond, out);
                Self::collect_idents_block(then, out);
                if let Some(ElseBranch::Block(b)) = else_.as_ref() {
                    Self::collect_idents_block(b, out);
                }
                if let Some(ElseBranch::If(e)) = else_.as_ref() {
                    Self::collect_idents_expr(e, out);
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                Self::collect_idents_expr(scrutinee, out);
                Self::collect_idents_block(then, out);
                if let Some(ElseBranch::Block(b)) = else_.as_ref() {
                    Self::collect_idents_block(b, out);
                }
                if let Some(ElseBranch::If(e)) = else_.as_ref() {
                    Self::collect_idents_expr(e, out);
                }
            }
            ExprKind::While { cond, body, .. } => {
                Self::collect_idents_expr(cond, out);
                Self::collect_idents_block(body, out);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                Self::collect_idents_expr(scrutinee, out);
                Self::collect_idents_block(body, out);
            }
            ExprKind::For { iter, body, .. }
            | ExprKind::ParallelFor { iter, body, .. } => {
                Self::collect_idents_expr(iter, out);
                Self::collect_idents_block(body, out);
            }
            ExprKind::Loop { body, .. } => Self::collect_idents_block(body, out),
            ExprKind::Match { scrutinee, arms } => {
                Self::collect_idents_expr(scrutinee, out);
                for arm in arms {
                    if let Some(g) = &arm.guard { Self::collect_idents_expr(g, out); }
                    match &arm.body {
                        MatchArmBody::Expr(e) => Self::collect_idents_expr(e, out),
                        MatchArmBody::Block(b) => Self::collect_idents_block(b, out),
                    }
                }
            }
            ExprKind::Range { start, end, .. } => {
                Self::collect_idents_expr(start, out);
                Self::collect_idents_expr(end, out);
            }
            ExprKind::Lambda { body, .. } => Self::collect_idents_expr(body, out),
            ExprKind::TupleLit(elems) => {
                for e in elems { Self::collect_idents_expr(e, out); }
            }
            ExprKind::ArrayLit(elems) => {
                for elem in elems {
                    match elem {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => Self::collect_idents_expr(x, out),
                    }
                }
            }
            ExprKind::MapLit { pairs, .. } => {
                for (k, v) in pairs {
                    Self::collect_idents_expr(k, out);
                    Self::collect_idents_expr(v, out);
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value { Self::collect_idents_expr(v, out); }
                }
            }
            ExprKind::Spawn(body) => Self::collect_idents_expr(body, out),
            ExprKind::With { bindings, body } => {
                for b in bindings { Self::collect_idents_expr(&b.handler, out); }
                Self::collect_idents_block(body, out);
            }
            ExprKind::Coalesce(l, r) => {
                Self::collect_idents_expr(l, out);
                Self::collect_idents_expr(r, out);
            }
            ExprKind::Try(e) | ExprKind::Bang(e) | ExprKind::As(e, _) | ExprKind::Is(e, _) => {
                Self::collect_idents_expr(e, out);
            }
            ExprKind::Interrupt(Some(v)) => Self::collect_idents_expr(v, out),
            ExprKind::Block(b) => Self::collect_idents_block(b, out),
            ExprKind::Supervised { body, cancel } => {
                Self::collect_idents_block(body, out);
                if let Some(c) = cancel { Self::collect_idents_expr(c, out); }
            }
            ExprKind::Detach(b) => Self::collect_idents_block(b, out),
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => Self::collect_idents_expr(chan, out),
                        SelectOp::Send { chan, value } => {
                            Self::collect_idents_expr(chan, out);
                            Self::collect_idents_expr(value, out);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard { Self::collect_idents_expr(g, out); }
                    Self::collect_idents_block(&arm.body, out);
                }
            }
            _ => {}
        }
    }

    fn collect_idents_block(block: &Block, out: &mut Vec<String>) {
        for stmt in &block.stmts {
            Self::collect_idents_stmt(stmt, out);
        }
        if let Some(t) = &block.trailing {
            Self::collect_idents_expr(t, out);
        }
    }

    fn collect_idents_stmt(stmt: &Stmt, out: &mut Vec<String>) {
        match stmt {
            Stmt::Let(d) => Self::collect_idents_expr(&d.value, out),
            Stmt::Expr(e) => Self::collect_idents_expr(e, out),
            Stmt::Assign { target, value, .. } => {
                Self::collect_idents_expr(target, out);
                Self::collect_idents_expr(value, out);
            }
            Stmt::Return { value: Some(v), .. } => Self::collect_idents_expr(v, out),
            Stmt::Throw { value, .. } => Self::collect_idents_expr(value, out),
            _ => {}
        }
    }

    // ---- forward declarations ----

    fn emit_fn_forward_decl(&mut self, f: &FnDecl) -> Result<(), String> {
        // D82: external fn — forward decl не нужен (реализация в nova_rt/*.h
        // уже включена через preamble #include).
        if f.is_external {
            return Ok(());
        }
        if f.name == "main" {
            return Ok(());
        }
        // Plan 48: Generic free functions → store for monomorphization; no erased forward decl.
        if !f.generics.is_empty() && f.receiver.is_none() {
            self.mono_fn_decls.insert(f.name.clone(), f.clone());
            // Track tuple return arity so call sites can populate tuple_element_types
            if let Some(TypeRef::Tuple(elems, _)) = &f.return_type {
                self.generic_fn_tuple_arity.insert(f.name.clone(), elems.len());
            }
            return Ok(());
        }
        // Plan 48: Generic methods with own type params → store for monomorphization.
        // Exception: array extension methods ([]T receivers) never get monomorphized;
        // they fall through to the regular forward decl path below.
        if !f.generics.is_empty() {
            if let Some(recv) = &f.receiver {
                let is_array_ext = recv.type_name.starts_with("[]");
                if !is_array_ext {
                    self.mono_method_decls.insert((recv.type_name.clone(), f.name.clone()), f.clone());
                    // Register sentinel MethodSig so call sites can find and mono-route this method.
                    let sentinel_name = format!("__mono_method__{}__{}", recv.type_name, f.name);
                    let sig = MethodSig {
                        param_c_types: vec![],
                        return_c_type: "void*".to_string(),
                        is_instance: !matches!(f.receiver.as_ref().map(|r| &r.kind),
                            Some(crate::ast::ReceiverKind::Static)),
                        is_external: false,
                        is_delegated: false,
                        c_name: sentinel_name,
                        variadic_last: false,
                    };
                    let key = (recv.type_name.clone(), f.name.clone());
                    self.method_overloads.entry(key).or_default().push(sig);
                    return Ok(());
                }
                // is_array_ext: fall through to regular forward decl below.
            } else {
                // Free generic fn: already handled above.
                return Ok(());
            }
        }
        if let Some(recv) = &f.receiver {
            if !recv.generics.is_empty() {
                // Generic method: emit erased forward decl
                let type_params: HashSet<String> = recv.generics.iter().filter_map(|tr| {
                    if let TypeRef::Named { path, .. } = tr { path.first().cloned() } else { None }
                }).collect();
                let is_instance = matches!(recv.kind, ReceiverKind::Instance);
                let mangled = self.mangle_fn(f);
                // Set receiver type so `Self` in return type resolves to
                // `Nova_{RecvType}*` instead of the invalid `Nova_Self*`.
                let prev_recv = self.current_receiver_type.replace(recv.type_name.clone());
                let ret_c = self.erased_type_ref_c(&f.return_type, &type_params);
                self.current_receiver_type = prev_recv;
                let mut parts = if is_instance {
                    vec![format!("{} nova_self", self.receiver_c_type(&recv.type_name))]
                } else {
                    vec![]
                };
                for p in &f.params {
                    let p_c = self.erased_type_ref_c(&Some(p.ty.clone()), &type_params);
                    parts.push(format!("{} {}", p_c, p.name));
                }
                let params_s = if parts.is_empty() { "void".into() } else { parts.join(", ") };
                self.var_types.insert(format!("fn_ret_{}", f.name), ret_c.clone());
                self.line(&format!("static {} {}({});", ret_c, mangled, params_s));
                return Ok(());
            }
        }
        // Set receiver type for Self resolution
        if let Some(recv) = &f.receiver {
            self.current_receiver_type = Some(recv.type_name.clone());
        } else {
            self.current_receiver_type = None;
        }
        let ret = self.return_type_c(f)?;
        let params = self.params_c(f)?;
        let mangled = self.mangle_fn(f);
        // Register return type so call sites can infer print helper
        self.var_types.insert(format!("fn_ret_{}", f.name), ret.clone());
        // Register fn-typed return signature for closure binding propagation
        if let Some(TypeRef::Func { params: fp, return_type, .. }) = &f.return_type {
            let ptys: Vec<String> = fp.iter().map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into())).collect();
            let rty = return_type.as_ref().map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into())).unwrap_or_else(|| "nova_unit".into());
            self.fn_returns_fn_sig.insert(f.name.clone(), (ptys, rty));
        }
        // Plan 14 Ф.3: регистрируем сигнатуру free fn в user_fn_sigs для
        // emit free-fn-as-value (`let f = inc`, `xs.map(inc)`).
        // Только для top-level fn без receiver'а (не методы) и не для
        // generic fn (мономорфизация по call-site, sig зависит от инстанциации).
        if f.receiver.is_none() && f.generics.is_empty() {
            let param_c_tys: Vec<String> = f.params.iter()
                .map(|p| self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into()))
                .collect();
            self.user_fn_sigs.insert(f.name.clone(), (param_c_tys, ret.clone()));
            // Bidirectional inference: for each fn-typed parameter, record the
            // inner closure signature so ClosureLight call-site args can infer
            // their parameter types without explicit annotations.
            for (idx, p) in f.params.iter().enumerate() {
                if let TypeRef::Func { params: fp, return_type, .. } = &p.ty {
                    let inner_ptys: Vec<String> = fp.iter()
                        .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                        .collect();
                    let inner_rty = return_type.as_ref()
                        .map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into()))
                        .unwrap_or_else(|| "nova_unit".into());
                    self.hof_param_fn_sigs.insert(
                        (f.name.clone(), idx),
                        (inner_ptys, inner_rty),
                    );
                }
            }
            // Plan 14 Ф.6: регистрация variadic-флага.
            if f.params.last().map(|p| p.is_variadic).unwrap_or(false) {
                self.user_fn_variadic.insert(f.name.clone());
            }
        }
        self.line(&format!("static {} {}({});", ret, mangled, params));
        Ok(())
    }

    // ---- type declarations ----

    /// Plan 52.2 Ф.2: forward-declare mono'd struct-type для использования
    /// в const-decl до того как mono pass эмитит full struct definition.
    ///
    /// Для `HashMap[str, int]` эмитит:
    /// ```c
    /// typedef struct Nova_HashMap____nova_str__nova_int Nova_HashMap____nova_str__nova_int;
    /// ```
    ///
    /// Это даёт C-compiler'у знать имя struct (opaque), достаточно для
    /// pointer-type declaration в const-decl. Полная struct-definition
    /// эмитится позже через mono pass; pointer-type работает с opaque
    /// forward-decl.
    fn forward_declare_generic_type(&mut self, ty: &TypeRef) {
        let TypeRef::Named { path, generics, .. } = ty else { return; };
        if generics.is_empty() { return; }
        // Только для record/sum-типов (не primitive Option/Array)
        let base_name = path.join("_");
        if matches!(base_name.as_str(),
            "Option" | "Array" | "int" | "str" | "bool" | "f64" | "f32"
            | "i32" | "i64" | "u32" | "u64" | "i8" | "i16" | "u8" | "u16"
            | "byte" | "char") {
            return;
        }
        // Вычислить C-имена type-args
        let type_args_c: Vec<String> = generics.iter()
            .filter_map(|g| self.type_ref_to_c(g).ok())
            .collect();
        if type_args_c.len() != generics.len() { return; }
        let mono_name = Self::compute_generic_type_c_name(&base_name, &type_args_c);
        // Forward-declare через typedef. Idempotent если повторно вызван.
        self.line(&format!("typedef struct {0} {0};", mono_name));
    }

    fn emit_type_decl(&mut self, t: &TypeDecl) -> Result<(), String> {
        // Plan 48 Ф.3: generic types are emitted in erased form (void* for type-param fields)
        // for bootstrap erasure mode. Monomorphized instances are emitted lazily by
        // drain_generic_type_worklist when explicit type args are provided.
        // Templates already stored in 1a pre-pass; here we emit the erased fallback only.
        if !t.generics.is_empty() {
            if let TypeDeclKind::Record(fields) = &t.kind {
                let type_params: HashSet<String> = t.generics.iter().map(|g| g.name.clone()).collect();
                let mut schema = HashMap::new();
                self.line(&format!("typedef struct Nova_{0} Nova_{0};", t.name));
                self.line(&format!("struct Nova_{} {{", t.name));
                self.indent += 1;
                for f in fields {
                    let c_ty = match &f.ty {
                        TypeRef::Named { path, .. }
                            if path.len() == 1 && type_params.contains(&path[0]) =>
                            "void*".to_string(),
                        TypeRef::Array(inner, _)
                            if matches!(inner.as_ref(),
                                TypeRef::Named { path, .. }
                                if path.len() == 1 && type_params.contains(&path[0])) =>
                            "NovaArray_nova_int*".to_string(),
                        // Named generic type with type-param args (e.g. HashMap[K, V]) → void*
                        // in erased form. Array fields ([]Slot[K,V]) fall through and get
                        // NovaArray_nova_int* from type_ref_to_c's Array arm.
                        TypeRef::Named { path, generics, .. }
                            if path.len() >= 1
                            && !generics.is_empty()
                            && generics.iter().any(|g| Self::type_ref_uses_any_type_param(g, &type_params)) =>
                            "void*".to_string(),
                        _ => self.type_ref_to_c(&f.ty).unwrap_or_else(|_| "nova_int".into()),
                    };
                    let mangled = Self::mangle_field_name(&f.name);
                    self.line(&format!("{} {};", c_ty, mangled));
                    schema.insert(f.name.clone(), c_ty);
                }
                self.indent -= 1;
                self.line("};");
                self.line("");
                self.record_schemas.insert(t.name.clone(), schema);
            }
            // Generic sum types: emit erased form so that erased method bodies
            // (emit_generic_method_erased) can access ->tag and ->payload.
            if let TypeDeclKind::Sum(variants) = &t.kind {
                let type_params: HashSet<String> = t.generics.iter().map(|g| g.name.clone()).collect();
                // Tag enum
                self.line("typedef enum {");
                self.indent += 1;
                for v in variants {
                    self.line(&format!("NOVA_TAG_{}_{},", t.name, v.name));
                }
                self.indent -= 1;
                self.line(&format!("}} Nova_{}_Tag;", t.name));
                self.line(&format!("typedef struct Nova_{0} Nova_{0};", t.name));
                self.line(&format!("struct Nova_{} {{", t.name));
                self.indent += 1;
                self.line(&format!("Nova_{}_Tag tag;", t.name));
                self.line("union {");
                self.indent += 1;
                let mut sum_schema: HashMap<String, Vec<String>> = HashMap::new();
                let has_payload = variants.iter().any(|v| !matches!(v.kind, SumVariantKind::Unit));
                if !has_payload { self.line("char _dummy;"); }
                for v in variants {
                    match &v.kind {
                        SumVariantKind::Unit => { sum_schema.insert(v.name.clone(), vec![]); }
                        SumVariantKind::Tuple(types) => {
                            let mut field_types = Vec::new();
                            self.line("struct {");
                            self.indent += 1;
                            for (i, ty) in types.iter().enumerate() {
                                let c_ty = if let TypeRef::Named { path, generics, .. } = ty {
                                    if path.len() == 1 && type_params.contains(&path[0]) {
                                        "void*".to_string()
                                    } else if !generics.is_empty()
                                        && generics.iter().any(|g| Self::type_ref_uses_any_type_param(g, &type_params)) {
                                        "void*".to_string()
                                    } else {
                                        self.type_ref_to_c(ty).unwrap_or_else(|_| "void*".into())
                                    }
                                } else {
                                    self.type_ref_to_c(ty).unwrap_or_else(|_| "void*".into())
                                };
                                field_types.push(c_ty.clone());
                                self.line(&format!("{} _{};", c_ty, i));
                            }
                            self.indent -= 1;
                            self.line(&format!("}} {};", v.name));
                            sum_schema.insert(v.name.clone(), field_types);
                        }
                        SumVariantKind::Record(fields) => {
                            let mut field_types = Vec::new();
                            self.line("struct {");
                            self.indent += 1;
                            for f in fields {
                                let c_ty = if let TypeRef::Named { path, generics, .. } = &f.ty {
                                    if path.len() == 1 && type_params.contains(&path[0]) {
                                        "void*".to_string()
                                    } else if !generics.is_empty()
                                        && generics.iter().any(|g| Self::type_ref_uses_any_type_param(g, &type_params)) {
                                        "void*".to_string()
                                    } else {
                                        self.type_ref_to_c(&f.ty).unwrap_or_else(|_| "void*".into())
                                    }
                                } else {
                                    self.type_ref_to_c(&f.ty).unwrap_or_else(|_| "void*".into())
                                };
                                field_types.push(c_ty.clone());
                                let mf = Self::mangle_field_name(&f.name);
                                self.line(&format!("{} {};", c_ty, mf));
                                let key = format!("{}::{}::{}", t.name, v.name, f.name);
                                self.record_variant_field_types.insert(key, c_ty);
                            }
                            let order_key = format!("{}::{}", t.name, v.name);
                            let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                            self.record_variant_field_order.insert(order_key, field_names);
                            self.indent -= 1;
                            self.line(&format!("}} {};", v.name));
                            sum_schema.insert(v.name.clone(), field_types);
                        }
                    }
                }
                self.indent -= 1;
                self.line("} payload;");
                self.indent -= 1;
                self.line("};");
                self.line("");
                // Emit erased constructor functions (name with Nova_ prefix for C)
                let type_name = t.name.clone();
                let cname = format!("Nova_{}", type_name);
                let sum_schema_clone = sum_schema.clone();
                for v in variants {
                    let field_types = sum_schema_clone.get(&v.name).cloned().unwrap_or_default();
                    let params: String = field_types.iter().enumerate()
                        .map(|(i, ty)| format!("{} _{}", ty, i))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let params_str = if params.is_empty() { "void".to_string() } else { params };
                    self.line(&format!(
                        "static {cname}* nova_make_{tname}_{var}({params}) {{",
                        cname = cname, tname = type_name, var = v.name, params = params_str
                    ));
                    self.indent += 1;
                    self.line(&format!(
                        "{cname}* _r = ({cname}*)nova_alloc(sizeof({cname}));",
                        cname = cname
                    ));
                    self.line(&format!("_r->tag = NOVA_TAG_{name}_{var};",
                        name = type_name, var = v.name));
                    match &v.kind {
                        SumVariantKind::Unit => {}
                        SumVariantKind::Tuple(_) => {
                            for (i, _) in field_types.iter().enumerate() {
                                self.line(&format!("_r->payload.{var}._{i} = _{i};",
                                    var = v.name, i = i));
                            }
                        }
                        SumVariantKind::Record(fields) => {
                            for (i, f) in fields.iter().enumerate() {
                                let mf = Self::mangle_field_name(&f.name);
                                self.line(&format!("_r->payload.{var}.{fname} = _{i};",
                                    var = v.name, fname = mf, i = i));
                            }
                        }
                    }
                    self.line("return _r;");
                    self.indent -= 1;
                    self.line("}");
                    self.line("");
                }
                self.sum_schemas.insert(t.name.clone(), sum_schema);
            }
            return Ok(());
        }
        match &t.kind {
            TypeDeclKind::Record(fields) => {
                self.emit_record_type(&t.name, fields)?;
            }
            TypeDeclKind::Sum(variants) => {
                self.emit_sum_type(&t.name, variants)?;
            }
            TypeDeclKind::Newtype(inner) => {
                let inner_c = self.type_ref_to_c(inner)?;
                self.line(&format!("typedef {} Nova_{};", inner_c, t.name));
                // Newtypes are typedef'd scalars — use inner type directly (no pointer indirection)
                self.type_aliases.insert(t.name.clone(), inner_c);
            }
            TypeDeclKind::Alias(inner) => {
                let inner_c = self.type_ref_to_c(inner)?;
                self.line(&format!("typedef {} Nova_{};", inner_c, t.name));
                // Register alias so type_ref_to_c returns inner type directly (no extra *)
                self.type_aliases.insert(t.name.clone(), inner_c);
            }
            TypeDeclKind::Effect(methods) => {
                self.emit_effect_type(&t.name, methods)?;
            }
            // Plan 15 D53 strict: protocols — compile-time-only
            // структурные контракты (D72 bound checking). Vtable не
            // нужен — нет runtime-dispatch'а. Skip emission.
            // Бонус: попутно фиксит pre-existing codegen-bug, где
            // Self в protocol-методе ломал vtable (Nova_Self*
            // undefined). Без vtable type_ref_to_c для protocol-методов
            // вообще не вызывается.
            TypeDeclKind::Protocol(_) => {}
        }
        Ok(())
    }

    fn emit_effect_type(&mut self, name: &str, methods: &[EffectMethod]) -> Result<(), String> {
        // Pre-compute C param types for all methods (needed for mangle_op).
        let mut method_param_c: Vec<(String, Vec<String>)> = Vec::new();
        for m in methods {
            let mut ptypes: Vec<String> = Vec::new();
            for p in &m.params {
                ptypes.push(self.type_ref_to_c(&p.ty)?);
            }
            method_param_c.push((m.name.clone(), ptypes));
        }
        // Build the name+param pairs needed by mangle_op.
        let all_method_pairs: Vec<(&str, &[String])> = method_param_c.iter()
            .map(|(n, p)| (n.as_str(), p.as_slice()))
            .collect();

        // 1. Vtable struct: one fn ptr per method, plus void* ctx
        self.line(&format!("typedef struct {{"));
        self.indent += 1;
        self.line("void* ctx;");
        let mut schema: HashMap<String, (Vec<String>, String)> = HashMap::new();
        for (m, (_, param_c_types)) in methods.iter().zip(method_param_c.iter()) {
            let ret = match &m.return_type {
                None => "nova_unit".to_string(),
                Some(t) => self.type_ref_to_c(t)?,
            };
            let mangled = Self::mangle_op(&m.name, param_c_types, &all_method_pairs);
            let mut param_types_with_ctx = vec!["void*".to_string()]; // ctx first
            param_types_with_ctx.extend(param_c_types.iter().cloned());
            let params_sig = param_types_with_ctx.join(", ");
            self.line(&format!("{} (*{})({}); ", ret, mangled, params_sig));
            schema.insert(mangled.clone(), (param_c_types.clone(), ret));
        }
        self.indent -= 1;
        self.line(&format!("}} NovaVtable_{};", name));
        self.line("");

        // 2. Thread-local handler slot.
        self.line("#ifdef _MSC_VER");
        self.line(&format!(
            "__declspec(thread) NovaVtable_{name}* _nova_handler_{name} = NULL;",
            name = name
        ));
        self.line("#else");
        self.line(&format!(
            "__thread NovaVtable_{name}* _nova_handler_{name} = NULL;",
            name = name
        ));
        self.line("#endif");
        self.line("");

        // 3. Dispatch helpers: Nova_Effect_method() calls through vtable
        for (m, (_, param_c_types)) in methods.iter().zip(method_param_c.iter()) {
            let mangled = Self::mangle_op(&m.name, param_c_types, &all_method_pairs);
            let (_, ret) = schema.get(&mangled).unwrap();
            let ret = ret.clone();
            let mut fn_params: Vec<String> = Vec::new();
            let mut call_args: Vec<String> = vec![
                format!("_nova_handler_{name}->ctx", name = name)
            ];
            for (p, ty) in m.params.iter().zip(param_c_types.iter()) {
                fn_params.push(format!("{} {}", ty, p.name));
                call_args.push(p.name.clone());
            }
            let fn_params_str = if fn_params.is_empty() {
                "void".to_string()
            } else {
                fn_params.join(", ")
            };
            let call_args_str = call_args.join(", ");
            self.line(&format!(
                "static inline {ret} Nova_{name}_{method}({params}) {{",
                ret = ret, name = name, method = mangled, params = fn_params_str
            ));
            self.indent += 1;
            self.line(&format!(
                "return _nova_handler_{name}->{field}({args});",
                name = name, field = mangled, args = call_args_str
            ));
            self.indent -= 1;
            self.line("}");
            self.line("");
        }

        self.effect_schemas.insert(name.to_string(), schema);
        Ok(())
    }

    /// D39 / Plan 11 Ф.9: эмитит auto-proxy методы для wrapper-типов с
    /// embed'ами. Для каждого Delegated MethodSig в `method_overloads`
    /// генерирует C-функцию которая делегирует на embedded-объекта
    /// через `nova_self->field`.
    ///
    /// Override-precedence: если есть Own MethodSig с тем же ключом и
    /// param_c_types, Delegated пропускается (skip — собственный метод
    /// уже эмитен в emit_fn).
    fn emit_embed_proxies(&mut self) -> Result<(), String> {
        // Snapshot ключей чтобы избежать borrow-конфликта.
        let wrapper_types: Vec<String> = self.embed_fields.keys().cloned().collect();
        for wrapper_type in wrapper_types {
            let embeds = self.embed_fields.get(&wrapper_type).cloned().unwrap_or_default();
            // Collect все методы wrapper'а и разделить на Own / Delegated.
            // Pair (method_name, param_types) → ключ для override-detection.
            let all_overloads: Vec<((String, String), MethodSig)> = self.method_overloads.iter()
                .filter(|((t, _), _)| t == &wrapper_type)
                .flat_map(|(k, sigs)| sigs.iter().map(move |s| (k.clone(), s.clone())))
                .collect();
            for ((_, method_name), sig) in &all_overloads {
                if !sig.is_delegated { continue; }
                // Plan 11 Ф.9.3: override-precedence. Если есть Own с тем же
                // method_name и param_c_types — пропустить delegated.
                let has_own_override = all_overloads.iter().any(|((_, mn), s)|
                    mn == method_name
                    && !s.is_delegated
                    && s.param_c_types == sig.param_c_types);
                if has_own_override {
                    // Plan 11 Ф.9.5: lint warning "possible infinite recursion"
                    // если own-method вызывает себя без явного base-call'а
                    // (anonymous embed не даёт имени). Накапливаем в warnings
                    // (не eprintln!) — test runner направит в captured_stderr.
                    if embeds.iter().any(|(_, _, anon)| *anon) {
                        self.warnings.push(format!(
                            "warning: type `{}` overrides delegated method `{}({})`; \
                             anonymous embed has no name for explicit base-call — \
                             possible infinite recursion",
                            wrapper_type, method_name,
                            sig.param_c_types.join(", ")));
                    }
                    continue;
                }
                // Найти подходящий embed: тот в котором этот method есть
                // как Own (не Delegated).
                let mut target_field: Option<(String, String)> = None;
                for (fname, embedded_ty, _) in &embeds {
                    let found = self.method_overloads.get(&(embedded_ty.clone(), method_name.clone()))
                        .map(|sigs| sigs.iter().any(|s|
                            !s.is_delegated && s.param_c_types == sig.param_c_types))
                        .unwrap_or(false);
                    if found {
                        target_field = Some((fname.clone(), embedded_ty.clone()));
                        break;
                    }
                }
                let (field_name, embedded_ty) = match target_field {
                    Some(t) => t,
                    None => continue,    // не нашли base — skip
                };
                // Найти base-method's c_name (mangled).
                let base_c_name = self.method_overloads.get(&(embedded_ty.clone(), method_name.clone()))
                    .and_then(|sigs| sigs.iter()
                        .find(|s| !s.is_delegated && s.param_c_types == sig.param_c_types)
                        .map(|s| s.c_name.clone()));
                let base_c_name = match base_c_name {
                    Some(n) => n,
                    None => continue,
                };
                // Build params + arg names.
                let mut param_decls: Vec<String> = vec![format!("Nova_{}* nova_self", wrapper_type)];
                let mut arg_names: Vec<String> = Vec::new();
                for (i, ty) in sig.param_c_types.iter().enumerate() {
                    param_decls.push(format!("{} arg{}", ty, i));
                    arg_names.push(format!("arg{}", i));
                }
                // Forward decl + body.
                self.line(&format!("static {} {}({});",
                    sig.return_c_type, sig.c_name, param_decls.join(", ")));
                self.line(&format!("static {} {}({}) {{",
                    sig.return_c_type, sig.c_name, param_decls.join(", ")));
                self.indent += 1;
                let field_mangled = Self::mangle_field_name(&field_name);
                let mut call_args = vec![format!("nova_self->{}", field_mangled)];
                call_args.extend(arg_names);
                if sig.return_c_type == "nova_unit" {
                    self.line(&format!("{}({});", base_c_name, call_args.join(", ")));
                    self.line("return NOVA_UNIT;");
                } else {
                    self.line(&format!("return {}({});",
                        base_c_name, call_args.join(", ")));
                }
                self.indent -= 1;
                self.line("}");
                self.line("");
            }
        }
        Ok(())
    }

    fn emit_record_type(&mut self, name: &str, fields: &[RecordField]) -> Result<(), String> {
        let mut schema = HashMap::new();
        self.line(&format!("typedef struct Nova_{0} Nova_{0};", name));
        self.line(&format!("struct Nova_{} {{", name));
        self.indent += 1;
        for f in fields {
            let ty_c = self.type_ref_to_c(&f.ty)?;
            schema.insert(f.name.clone(), ty_c.clone());
            // Mangle если коллизия с C reserved keyword.
            let mangled = Self::mangle_field_name(&f.name);
            self.line(&format!("{} {};", ty_c, mangled));
            // Plan 14 Ф.4: записываем fn-typed поля в реестр sig'ов
            // record-полей. Использует Member-call routing для эмита
            // closure-call (`obj.f(x)` → NOVA_CLOS_CALL_*).
            if let TypeRef::Func { params: fp, return_type, .. } = &f.ty {
                let ptys: Vec<String> = fp.iter()
                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                    .collect();
                let rty = return_type.as_ref()
                    .map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into()))
                    .unwrap_or_else(|| "nova_unit".into());
                self.record_field_fn_sigs.insert(
                    (name.to_string(), f.name.clone()),
                    (ptys, rty),
                );
            }
        }
        self.indent -= 1;
        self.line("};");
        self.line("");
        self.record_schemas.insert(name.to_string(), schema);
        Ok(())
    }

    fn emit_sum_type(&mut self, name: &str, variants: &[SumVariant]) -> Result<(), String> {
        // Tag enum
        self.line("typedef enum {");
        self.indent += 1;
        for v in variants {
            self.line(&format!("NOVA_TAG_{}_{},", name, v.name));
        }
        self.indent -= 1;
        self.line(&format!("}} Nova_{}_Tag;", name));

        // Collect schema while building union
        let mut sum_schema: HashMap<String, Vec<String>> = HashMap::new();

        // Union payload
        self.line(&format!("typedef struct Nova_{0} Nova_{0};", name));
        self.line(&format!("struct Nova_{} {{", name));
        self.indent += 1;
        self.line(&format!("Nova_{}_Tag tag;", name));
        self.line("union {");
        self.indent += 1;
        // Check if any variant has payload — MSVC requires at least one member
        let has_payload = variants.iter().any(|v| !matches!(v.kind, SumVariantKind::Unit));
        if !has_payload {
            self.line("char _dummy;");
        }
        for v in variants {
            match &v.kind {
                SumVariantKind::Unit => {
                    sum_schema.insert(v.name.clone(), vec![]);
                }
                SumVariantKind::Tuple(types) => {
                    let mut field_types = Vec::new();
                    self.line("struct {");
                    self.indent += 1;
                    for (i, ty) in types.iter().enumerate() {
                        let tc = self.type_ref_to_c(ty)?;
                        field_types.push(tc.clone());
                        self.line(&format!("{} _{};", tc, i));
                    }
                    self.indent -= 1;
                    self.line(&format!("}} {};", v.name));
                    sum_schema.insert(v.name.clone(), field_types);
                }
                SumVariantKind::Record(fields) => {
                    let mut field_types = Vec::new();
                    let mut field_names_ordered: Vec<String> = Vec::new();
                    self.line("struct {");
                    self.indent += 1;
                    for f in fields {
                        let tc = self.type_ref_to_c(&f.ty)?;
                        field_types.push(tc.clone());
                        field_names_ordered.push(f.name.clone());
                        // Mangle если коллизия с C-keyword.
                        let mfn = Self::mangle_field_name(&f.name);
                        self.line(&format!("{} {};", tc, mfn));
                        let key = format!("{}::{}::{}", name, v.name, f.name);
                        self.record_variant_field_types.insert(key, tc);
                    }
                    let order_key = format!("{}::{}", name, v.name);
                    self.record_variant_field_order.insert(order_key, field_names_ordered);
                    self.indent -= 1;
                    self.line(&format!("}} {};", v.name));
                    sum_schema.insert(v.name.clone(), field_types);
                }
            }
        }
        self.indent -= 1;
        self.line("} payload;");
        self.indent -= 1;
        self.line("};");
        self.line("");

        // Constructor functions: Nova_Shape* nova_make_Shape_Circle(nova_f64 _0) { ... }
        for v in variants {
            let field_types = sum_schema.get(&v.name).cloned().unwrap_or_default();
            let params: String = field_types.iter().enumerate()
                .map(|(i, t)| format!("{} _{}", t, i))
                .collect::<Vec<_>>()
                .join(", ");
            let params_str = if params.is_empty() { "void".to_string() } else { params };
            self.line(&format!(
                "static Nova_{name}* nova_make_{name}_{var}({params}) {{",
                name = name, var = v.name, params = params_str
            ));
            self.indent += 1;
            self.line(&format!(
                "Nova_{name}* _r = (Nova_{name}*)nova_alloc(sizeof(Nova_{name}));",
                name = name
            ));
            self.line(&format!("_r->tag = NOVA_TAG_{name}_{var};",
                name = name, var = v.name));
            match &v.kind {
                SumVariantKind::Unit => {}
                SumVariantKind::Tuple(_) => {
                    for (i, _) in field_types.iter().enumerate() {
                        self.line(&format!("_r->payload.{var}._{i} = _{i};",
                            var = v.name, i = i));
                    }
                }
                SumVariantKind::Record(fields) => {
                    // Named fields — assign by field name, not positional index.
                    // Mangle field if collides with C-keyword.
                    for (i, f) in fields.iter().enumerate() {
                        let mfn = Self::mangle_field_name(&f.name);
                        self.line(&format!("_r->payload.{var}.{fname} = _{i};",
                            var = v.name, fname = mfn, i = i));
                    }
                }
            }
            self.line("return _r;");
            self.indent -= 1;
            self.line("}");
            self.line("");
        }

        self.sum_schemas.insert(name.to_string(), sum_schema);
        Ok(())
    }

    /// Если field-name коллизирует с C reserved-keyword'ом — добавим
    /// префикс `nv_`. Применяется ко **всем** field-emission точкам:
    /// struct decl, record-literal, member-access, pattern match.
    /// Иначе генерируется invalid C (`nova_int char;`).
    ///
    /// `n` это C-keyword? Список — стандартный C99 + popular extensions.
    fn mangle_field_name(name: &str) -> String {
        if Self::is_c_keyword(name) {
            format!("nv_{}", name)
        } else {
            name.to_string()
        }
    }

    fn is_c_keyword(name: &str) -> bool {
        matches!(name,
            "auto" | "break" | "case" | "char" | "const" | "continue" |
            "default" | "do" | "double" | "else" | "enum" | "extern" |
            "float" | "for" | "goto" | "if" | "inline" | "int" | "long" |
            "register" | "restrict" | "return" | "short" | "signed" |
            "sizeof" | "static" | "struct" | "switch" | "typedef" |
            "union" | "unsigned" | "void" | "volatile" | "while" |
            "_Bool" | "_Atomic" | "_Complex" | "_Imaginary" |
            "_Generic" | "_Thread_local" | "_Static_assert" | "_Noreturn" |
            "asm" | "fortran"
        )
    }

    // ---- function emission ----

    fn mangle_fn(&self, f: &FnDecl) -> String {
        if let Some(recv) = &f.receiver {
            // Plan 11 Ф.3: если есть multi-overload registry для (type, name),
            // ищем по сигнатуре и берём её c_name (mangled). Иначе — старый mangling.
            let key = (recv.type_name.clone(), f.name.clone());
            if let Some(overloads) = self.method_overloads.get(&key) {
                if overloads.len() > 1 {
                    // Резолвим по param C-типам этого FnDecl'а.
                    let want_params: Vec<String> = f.params.iter()
                        .map(|p| self.type_ref_to_c(&p.ty)
                            .unwrap_or_else(|_| "nova_int".into()))
                        .collect();
                    for sig in overloads {
                        if sig.param_c_types == want_params {
                            return sig.c_name.clone();
                        }
                    }
                }
            }
            let safe_type = Self::receiver_type_c_ident(&recv.type_name);
            match recv.kind {
                ReceiverKind::Instance => format!("Nova_{}_method_{}", safe_type, f.name),
                ReceiverKind::Static   => format!("Nova_{}_static_{}", safe_type, f.name),
            }
        } else {
            // D84: free-function — тот же путь через registry с sentinel-key
            // ("", name). Если несколько overloads — резолвим по param C-типам
            // этого FnDecl'а и возвращаем mangled c_name.
            let key = ("".to_string(), f.name.clone());
            if let Some(overloads) = self.method_overloads.get(&key) {
                if overloads.len() > 1 {
                    let want_params: Vec<String> = f.params.iter()
                        .map(|p| self.type_ref_to_c(&p.ty)
                            .unwrap_or_else(|_| "nova_int".into()))
                        .collect();
                    for sig in overloads {
                        if sig.param_c_types == want_params {
                            return sig.c_name.clone();
                        }
                    }
                }
            }
            format!("nova_fn_{}", f.name)
        }
    }

    /// Plan 11 Ф.2: overload resolution. Возвращает выбранный MethodSig
    /// или подробную ошибку. `arg_c_types` — типы args без receiver'а.
    /// Strict matching, no implicit conversions.
    fn resolve_overload(
        &self,
        receiver_type: &str,
        method_name: &str,
        arg_c_types: &[String],
    ) -> Option<MethodSig> {
        let key = (receiver_type.to_string(), method_name.to_string());
        let overloads = self.method_overloads.get(&key)?;
        // Single-overload — short-circuit.
        if overloads.len() == 1 {
            return Some(overloads[0].clone());
        }
        // Filter по arity + param types. Strict.
        let matches: Vec<&MethodSig> = overloads.iter()
            .filter(|sig| sig.param_c_types.len() == arg_c_types.len())
            .filter(|sig| sig.param_c_types.iter().zip(arg_c_types.iter())
                .all(|(want, got)| want == got))
            .collect();
        match matches.len() {
            1 => Some(matches[0].clone()),
            // 0 или >1 — single-overload fallback path не помогает.
            // Возвращаем None, вызывающий code сам fallback'нется на старую
            // логику или эмитит ошибку.
            _ => None,
        }
    }

    /// Mangle an effect op name with its param C-types for vtable field naming.
    /// Single overload (no collision): returns plain name.
    /// Overloaded: returns `name__type1_type2` (e.g. `balance__nova_int`).
    /// `all_methods` is the full list of methods in this effect.
    fn mangle_op(name: &str, param_c_types: &[String], all_methods: &[(&str, &[String])]) -> String {
        let same_name_count = all_methods.iter().filter(|(n, _)| *n == name).count();
        if same_name_count <= 1 {
            return name.to_string();
        }
        // Mangle: replace pointer stars and spaces with underscores for valid C identifier
        let suffix: String = param_c_types.iter()
            .map(|t| t.replace("* ", "_ptr_").replace('*', "_ptr").replace(' ', "_"))
            .collect::<Vec<_>>()
            .join("_");
        if suffix.is_empty() {
            format!("{}_void", name)
        } else {
            format!("{}__{}", name, suffix)
        }
    }

    /// Lookup in effect schema by op name, supporting both mangled and plain keys.
    /// Used at call-sites where only the plain method name is known.
    fn schema_lookup<'s>(
        schema: &'s HashMap<String, (Vec<String>, String)>,
        method_name: &str,
    ) -> Option<&'s (Vec<String>, String)> {
        if let Some(v) = schema.get(method_name) {
            return Some(v);
        }
        // Mangled key search: find first key that is `method_name` or starts with `method_name__`
        let prefix = format!("{}__", method_name);
        schema.iter()
            .find(|(k, _)| k.as_str() == method_name || k.starts_with(&prefix))
            .map(|(_, v)| v)
    }

    /// Recursively collect type names from a TypeRef.
    /// `out` — regular struct names (Nova_X). `vtable_out` — effect vtable names (NovaVtable_X)
    /// from Handler[X] generics and Func effects.
    fn collect_typeref_names(
        ty: &crate::ast::TypeRef,
        out: &mut HashSet<String>,
        vtable_out: &mut HashSet<String>,
    ) {
        use crate::ast::TypeRef;
        match ty {
            TypeRef::Named { path, generics, .. } => {
                let name = path.last().cloned();
                if let Some(n) = &name {
                    // Handler[X] → X is a vtable name, not a struct name
                    if n == "Handler" {
                        if let Some(TypeRef::Named { path: gpath, .. }) = generics.first() {
                            if let Some(eff) = gpath.last() { vtable_out.insert(eff.clone()); }
                        }
                        return;
                    }
                    out.insert(n.clone());
                }
                for g in generics {
                    Self::collect_typeref_names(g, out, vtable_out);
                }
            }
            TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
                Self::collect_typeref_names(inner, out, vtable_out);
            }
            TypeRef::Tuple(items, _) => {
                for item in items {
                    Self::collect_typeref_names(item, out, vtable_out);
                }
            }
            TypeRef::Func { params, effects, return_type, .. } => {
                for p in params { Self::collect_typeref_names(p, out, vtable_out); }
                // Effects in fn type → vtable names
                for e in effects {
                    if let TypeRef::Named { path, .. } = e {
                        if let Some(n) = path.last() { vtable_out.insert(n.clone()); }
                    }
                }
                if let Some(r) = return_type { Self::collect_typeref_names(r, out, vtable_out); }
            }
            TypeRef::Unit(_) => {}
        }
    }

    /// Plan 48 Ф.3: check if TypeRef contains any of the given type params (directly or in generics).
    /// Used to detect when erased struct fields should be void* rather than a mangled generic name.
    fn type_ref_uses_any_type_param(ty: &crate::ast::TypeRef, type_params: &HashSet<String>) -> bool {
        use crate::ast::TypeRef;
        match ty {
            TypeRef::Named { path, generics, .. } => {
                if path.len() == 1 && type_params.contains(&path[0]) { return true; }
                generics.iter().any(|g| Self::type_ref_uses_any_type_param(g, type_params))
            }
            TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
                Self::type_ref_uses_any_type_param(inner, type_params)
            }
            TypeRef::Tuple(ts, _) => ts.iter().any(|t| Self::type_ref_uses_any_type_param(t, type_params)),
            TypeRef::Func { params, return_type, .. } => {
                params.iter().any(|t| Self::type_ref_uses_any_type_param(t, type_params))
                    || return_type.as_ref().map_or(false, |t| Self::type_ref_uses_any_type_param(t, type_params))
            }
            TypeRef::Unit(_) => false,
        }
    }

    /// Collect type names referenced in a type declaration's fields/variants.
    fn collect_typeref_names_in_typedecl(
        t: &crate::ast::TypeDecl,
        out: &mut HashSet<String>,
        vtable_out: &mut HashSet<String>,
    ) {
        use crate::ast::{TypeDeclKind, SumVariantKind};
        match &t.kind {
            TypeDeclKind::Record(fields) => {
                for f in fields { Self::collect_typeref_names(&f.ty, out, vtable_out); }
            }
            TypeDeclKind::Sum(variants) => {
                for v in variants {
                    match &v.kind {
                        SumVariantKind::Unit => {}
                        SumVariantKind::Tuple(tys) => {
                            for ty in tys { Self::collect_typeref_names(ty, out, vtable_out); }
                        }
                        SumVariantKind::Record(fields) => {
                            for f in fields { Self::collect_typeref_names(&f.ty, out, vtable_out); }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// C type for receiver-typed parameter (D35 v2: receiver may be a primitive).
    /// Returns the C type to use for `nova_self`. Primitives are passed by value;
    /// records/sums by pointer.
    fn receiver_c_type(&self, type_name: &str) -> String {
        match type_name {
            "int" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => "nova_int".to_string(),
            "f32" => "nova_f32".to_string(),
            "f64" => "nova_f64".to_string(),
            "bool" => "nova_bool".to_string(),
            "str" => "nova_str".to_string(),
            "byte" => "nova_byte".to_string(),
            other => {
                // Extension methods on array types: []T, []str, []int, etc.
                if let Some(elem_ty) = other.strip_prefix("[]") {
                    let c_elem = match elem_ty {
                        "str"  => "nova_str",
                        "bool" => "nova_bool",
                        "f64"  => "nova_f64",
                        "f32"  => "nova_f32",
                        "byte" => "nova_byte",
                        "int"  | "i8" | "i16" | "i32" | "i64"
                        | "u8" | "u16" | "u32" | "u64" => "nova_int",
                        _ => "nova_int", // erased T
                    };
                    return format!("NovaArray_{}*", c_elem);
                }
                format!("Nova_{}*", other)
            }
        }
    }

    /// Convert a receiver type name to a valid C identifier component.
    /// []T → "NovaArray_nova_int", []str → "NovaArray_nova_str", etc.
    /// Other names are returned unchanged (already valid C identifiers).
    fn receiver_type_c_ident(type_name: &str) -> String {
        if let Some(elem_ty) = type_name.strip_prefix("[]") {
            let c_elem = match elem_ty {
                "str"  => "nova_str",
                "bool" => "nova_bool",
                "f64" | "f32" => "nova_f64",
                "byte" => "nova_byte",
                "int" | "i8" | "i16" | "i32" | "i64"
                | "u8" | "u16" | "u32" | "u64" => "nova_int",
                _ => "nova_int", // erased T
            };
            return format!("NovaArray_{}", c_elem);
        }
        type_name.to_string()
    }

    fn params_c(&self, f: &FnDecl) -> Result<String, String> {
        let mut parts = Vec::new();
        // Instance methods receive the receiver as the first parameter.
        // Primitives by value (D35 v2), records/sums by pointer.
        if let Some(recv) = &f.receiver {
            if matches!(recv.kind, ReceiverKind::Instance) {
                parts.push(format!("{} nova_self", self.receiver_c_type(&recv.type_name)));
            }
        }
        for p in &f.params {
            let ty_c = self.type_ref_to_c(&p.ty)?;
            parts.push(format!("{} {}", ty_c, p.name));
        }
        if parts.is_empty() {
            Ok("void".into())
        } else {
            Ok(parts.join(", "))
        }
    }

    /// Emit a type-erased version of a generic free function.
    /// Convert a TypeRef to C, erasing type parameters (in type_params set) to void*.
    /// Used for generic method emission where T→void*, []T→void*, Option[T]→NovaOpt_nova_int.
    fn erased_type_ref_c(&self, ty_opt: &Option<TypeRef>, type_params: &HashSet<String>) -> String {
        let ty = match ty_opt {
            None => return "nova_unit".into(),
            Some(t) => t,
        };
        match ty {
            TypeRef::Named { path, generics, .. } => {
                let name = path.join("_");
                if type_params.contains(&name) {
                    return "void*".into();
                }
                // Option[T] where T is a type param → NovaOpt_nova_int
                if name == "Option" {
                    if let Some(g) = generics.first() {
                        if let TypeRef::Named { path: gp, .. } = g {
                            if type_params.contains(&gp.join("_")) {
                                return "NovaOpt_nova_int".into();
                            }
                        }
                    }
                }
                // Plan 48 Ф.3: generic type with type-param args (e.g. Pair[B, A] in erased context)
                // must NOT be monomorphized — return erased base pointer to avoid spurious instances.
                if !generics.is_empty() && self.generic_type_templates.contains_key(&name) {
                    let any_param = generics.iter().any(|g| {
                        if let TypeRef::Named { path: gp, .. } = g {
                            type_params.contains(&gp.join("_"))
                        } else { false }
                    });
                    if any_param {
                        return format!("Nova_{}*", name);
                    }
                }
                self.type_ref_to_c(ty).unwrap_or_else(|_| "nova_int".into())
            }
            TypeRef::Array(inner, _) => {
                if let TypeRef::Named { path, .. } = inner.as_ref() {
                    if type_params.contains(&path.join("_")) {
                        return "NovaArray_nova_int*".into();
                    }
                }
                self.type_ref_to_c(ty).unwrap_or_else(|_| "void*".into())
            }
            TypeRef::Unit(_) => "nova_unit".into(),
            _ => self.type_ref_to_c(ty).unwrap_or_else(|_| "nova_int".into()),
        }
    }

    /// Emit a minimal stub body for a method on a generic type.
    /// Used for static constructors and instance methods with void*-field bodies
    /// that would generate invalid C. The stub body matches the forward-decl signature.
    fn emit_generic_static_method_stub(&mut self, f: &FnDecl) -> Result<(), String> {
        let recv = f.receiver.as_ref().unwrap();
        let is_instance = matches!(recv.kind, ReceiverKind::Instance);
        let type_params: HashSet<String> = recv.generics.iter().filter_map(|tr| {
            if let TypeRef::Named { path, .. } = tr { path.first().cloned() } else { None }
        }).collect();
        let mangled = self.mangle_fn(f);
        // Set receiver type so `Self` in return type resolves correctly.
        let prev_recv = self.current_receiver_type.replace(recv.type_name.clone());
        let ret_c = self.erased_type_ref_c(&f.return_type, &type_params);
        self.current_receiver_type = prev_recv;
        let recv_c = self.receiver_c_type(&recv.type_name);
        // Match the same signature as the forward declaration in emit_fn_decl
        let mut parts: Vec<String> = if is_instance {
            vec![format!("{} nova_self", recv_c)]
        } else {
            vec![]
        };
        for p in &f.params {
            let p_c = self.erased_type_ref_c(&Some(p.ty.clone()), &type_params);
            parts.push(format!("{} {}", p_c, p.name));
        }
        let params_s = if parts.is_empty() { "void".into() } else { parts.join(", ") };
        self.line(&format!("static {} {}({}) {{", ret_c, mangled, params_s));
        self.indent += 1;
        if is_instance { self.line("(void)nova_self;"); }
        for p in &f.params {
            self.line(&format!("(void){};", p.name));
        }
        if ret_c == "nova_unit" {
            self.line("return NOVA_UNIT;");
        } else if ret_c.ends_with('*') {
            self.line("return NULL;");
        } else {
            self.line(&format!("return ({}){{0}};", ret_c));
        }
        self.indent -= 1;
        self.line("}");
        self.line("");
        Ok(())
    }


    /// Emit a type-erased version of a generic method (instance or static).
    /// Type params in recv.generics map to void*.
    fn emit_generic_method_erased(&mut self, f: &FnDecl) -> Result<(), String> {
        let recv = f.receiver.as_ref().unwrap();
        let is_instance = matches!(recv.kind, ReceiverKind::Instance);
        let type_params: HashSet<String> = recv.generics.iter().filter_map(|tr| {
            if let TypeRef::Named { path, .. } = tr { path.first().cloned() } else { None }
        }).collect();
        // If the receiver type has fields that become void* in erased form (Named generic types
        // with type-param args, same condition as emit_type_decl), the erased body would try
        // to access sub-fields on void* — generate a stub instead.
        let has_void_ptr_fields = if let Some(template) = self.generic_type_templates.get(&recv.type_name).cloned() {
            use crate::ast::TypeDeclKind;
            if let TypeDeclKind::Record(fields) = &template.kind {
                fields.iter().any(|fld| {
                    // Same condition as emit_type_decl erased field → void* arm (lines 3666-3670)
                    if let TypeRef::Named { path, generics, .. } = &fld.ty {
                        path.len() >= 1
                        && !generics.is_empty()
                        && generics.iter().any(|g| Self::type_ref_uses_any_type_param(g, &type_params))
                    } else { false }
                })
            } else { false }
        } else { false };
        if has_void_ptr_fields {
            return self.emit_generic_static_method_stub(f);
        }
        // D109: Methods whose params use bare type params (e.g. find_slot(key K)) generate
        // broken erased code when the body calls methods on those params (key.hash(),
        // key.eq()). Stub only when the receiver type has Array fields with type-param
        // element types (collection types like HashMap). Simple generic types (Result2,
        // Option, Wrapper) just pass/return bare type-param params and work correctly in
        // erased form — their erased body compiles to valid C.
        let has_type_param_params = f.params.iter().any(|p| {
            if let TypeRef::Named { path, generics, .. } = &p.ty {
                generics.is_empty() && path.len() == 1 && type_params.contains(&path[0])
            } else { false }
        });
        let has_array_fields_with_type_params = if has_type_param_params {
            if let Some(template) = self.generic_type_templates.get(&recv.type_name).cloned() {
                use crate::ast::TypeDeclKind;
                if let TypeDeclKind::Record(fields) = &template.kind {
                    fields.iter().any(|fld| {
                        if let TypeRef::Array(inner, _) = &fld.ty {
                            if let TypeRef::Named { generics, .. } = inner.as_ref() {
                                !generics.is_empty() && generics.iter().any(|g| {
                                    Self::type_ref_uses_any_type_param(g, &type_params)
                                })
                            } else { false }
                        } else { false }
                    })
                } else { false }
            } else { false }
        } else { false };
        if has_type_param_params && has_array_fields_with_type_params {
            return self.emit_generic_static_method_stub(f);
        }
        let mangled = self.mangle_fn(f);
        // Set receiver type so `Self` in return type resolves to the erased pointer.
        let prev_recv = self.current_receiver_type.replace(recv.type_name.clone());
        let ret_c = self.erased_type_ref_c(&f.return_type, &type_params);
        self.current_receiver_type = prev_recv;
        let recv_c = self.receiver_c_type(&recv.type_name);
        // Static methods don't get nova_self; instance methods do.
        let mut parts: Vec<String> = if is_instance {
            vec![format!("{} nova_self", recv_c)]
        } else {
            vec![]
        };
        for p in &f.params {
            let p_c = self.erased_type_ref_c(&Some(p.ty.clone()), &type_params);
            parts.push(format!("{} {}", p_c, p.name));
        }
        let params_s = if parts.is_empty() { "void".into() } else { parts.join(", ") };
        // Plan 47: буферизуем тело — см. emit_generic_fn_erased (тот же баг:
        // spawn в generic-методе → ctx-typedef после использования).
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        self.line(&format!("static {} {}({}) {{", ret_c, mangled, params_s));
        self.indent += 1;
        // Register nova_self only for instance methods
        let mut saved_array_elem_keys: Vec<String> = Vec::new();
        if is_instance {
            self.var_types.insert("nova_self".into(), recv_c.clone());
            // Pre-populate array_element_types for array fields of the receiver type
            // so that @field[idx] emits the right cast in the erased body.
            // Key: the C expression "(nova_self->{field})" that emit_expr(Member) produces.
            if let Some(template) = self.generic_type_templates.get(&recv.type_name).cloned() {
                use crate::ast::TypeDeclKind;
                if let TypeDeclKind::Record(fields) = &template.kind {
                    for fld in fields {
                        if let crate::ast::TypeRef::Array(elem_ty, _) = &fld.ty {
                            let elem_c = self.erased_type_ref_c(&Some(*elem_ty.clone()), &type_params);
                            // Only register if the element type is a pointer (sum types, records).
                            // nova_int elements don't need a cast.
                            if elem_c.ends_with('*') && elem_c != "nova_int*" {
                                let field_c = Self::mangle_field_name(&fld.name);
                                // Use the same C-expression format that emit_expr(Member) produces.
                                let key = format!("(nova_self->{})", field_c);
                                self.array_element_types.insert(key.clone(), elem_c);
                                saved_array_elem_keys.push(key);
                            }
                        }
                    }
                }
            }
        }
        let saved: Vec<(String, Option<String>)> = f.params.iter().map(|p| {
            let p_c = self.erased_type_ref_c(&Some(p.ty.clone()), &type_params);
            (p.name.clone(), self.var_types.insert(p.name.clone(), p_c))
        }).collect();
        // Register fn-typed param signatures so that calls like `pred(h)` inside the erased
        // body know the return type (e.g. `bool` for `fn(T) -> bool` — without this, the
        // call infers `nova_int` by default and an `if pred(h)` body fails the strict-bool check.
        let saved_fn_sigs: Vec<(String, Option<(Vec<String>, String)>)> = f.params.iter()
            .filter_map(|p| {
                if let TypeRef::Func { params: fp, return_type, .. } = &p.ty {
                    let erase_unk = |c: String| -> String {
                        if let Some(inner) = c.strip_prefix("Nova_").and_then(|s| s.strip_suffix('*')) {
                            let name = inner.trim();
                            if !self.record_schemas.contains_key(name)
                                && !self.sum_schemas.contains_key(name)
                                && !self.generic_types.contains(name)
                            {
                                return "nova_int".to_string();
                            }
                        }
                        c
                    };
                    let param_c_tys: Vec<String> = fp.iter()
                        .map(|t| erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into())))
                        .collect();
                    let ret_c = match return_type {
                        Some(rt) => erase_unk(self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into())),
                        None => "nova_unit".into(),
                    };
                    let prev = self.fn_param_sigs.insert(p.name.clone(), (param_c_tys, ret_c));
                    Some((p.name.clone(), prev))
                } else { None }
            })
            .collect();
        self.current_receiver_type = Some(recv.type_name.clone());
        // Emit body
        let saved_expected = self.expected_record_type.clone();
        self.expected_record_type = Self::struct_name_from_c_type(&ret_c);
        match &f.body {
            FnBody::Expr(e) => {
                self.emit_source_annotation_for_expr(e);
                let val = self.emit_expr(e)?;
                if ret_c == "nova_unit" {
                    self.line(&format!("{};", val));
                    self.line("return NOVA_UNIT;");
                } else {
                    self.line(&format!("return {};", val));
                }
            }
            FnBody::Block(block) => {
                self.emit_block_stmts(block, &ret_c)?;
            }
            // D82: external fn — body заэмичен в nova_rt; здесь игнорируем
            // (вызов будет диспатчен через external dispatch table).
            FnBody::External => {}
        }
        self.expected_record_type = saved_expected;
        // Restore params
        for (name, prev) in saved {
            match prev {
                Some(old) => { self.var_types.insert(name, old); }
                None => { self.var_types.remove(&name); }
            }
        }
        // Restore fn-typed param sigs
        for (name, prev) in saved_fn_sigs {
            match prev {
                Some(old) => { self.fn_param_sigs.insert(name, old); }
                None => { self.fn_param_sigs.remove(&name); }
            }
        }
        if is_instance { self.var_types.remove("nova_self"); }
        // Restore array_element_types entries added for this erased body
        for key in &saved_array_elem_keys {
            self.array_element_types.remove(key);
        }
        self.current_receiver_type = None;
        self.flush_boxed_vars();
        self.indent -= 1;
        self.line("}");
        self.line("");
        // Plan 47: restore + flush spawn-ctx typedefs / lambda impls before body.
        let fn_body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        if !self.lambda_forward_decls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_forward_decls));
        }
        if !self.lambda_impls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_impls));
        }
        self.out.push_str(&fn_body);
        Ok(())
    }


    // ---- Plan 48: monomorphization helpers ----

    /// Plan 48: sanitize a C type string to a valid C identifier component.
    /// "nova_int" → "nova_int", "Nova_Box*" → "Nova_Box_p", "NovaArray_nova_int*" → "NovaArray_nova_int_p"
    fn sanitize_c_for_ident(c_type: &str) -> String {
        c_type
            .replace("* ", "_p_")
            .replace('*', "_p")
            .replace(' ', "_")
            .replace('[', "_arr_")
            .replace(']', "")
            .replace('-', "_")
    }

    /// Plan 49 Ф.6 cross-type cascade helper: C-type string → Nova type name
    /// (для lookup в from_targets и user-friendly diagnostic messages).
    /// "nova_int"  → "int"
    /// "nova_str"  → "str"
    /// "nova_bool" → "bool"
    /// "Nova_Foo*" → "Foo"
    /// (Generic mono'd names like "Nova_X____Y" остаются как есть.)
    fn c_type_to_nova_name(c_ty: &str) -> String {
        let trimmed = c_ty.trim_end_matches('*').trim();
        match trimmed {
            "nova_int"  => "int".to_string(),
            "nova_str"  => "str".to_string(),
            "nova_bool" => "bool".to_string(),
            "nova_f64"  => "f64".to_string(),
            "nova_f32"  => "f32".to_string(),
            "nova_byte" => "byte".to_string(),
            other => other.strip_prefix("Nova_").unwrap_or(other).to_string(),
        }
    }

    /// Plan 48 Ф.3: compute the mangled C name for a generic type instance.
    /// compute_generic_type_c_name("HashMap", ["nova_str", "nova_int"]) → "Nova_HashMap____nova_str__nova_int"
    fn compute_generic_type_c_name(base_name: &str, type_args_c: &[String]) -> String {
        if type_args_c.is_empty() {
            return format!("Nova_{}", base_name);
        }
        let args: String = type_args_c.iter()
            .map(|c_ty| Self::sanitize_c_for_ident(c_ty))
            .collect::<Vec<_>>()
            .join("__");
        format!("Nova_{}____{}", base_name, args)
    }

    /// Plan 48: compute the monomorphized C name.
    /// compute_mono_name("nova_fn_within", [("T","nova_int")]) → "nova_fn_within____nova_int"
    fn compute_mono_name(base_c_name: &str, type_subst: &[(String, String)]) -> String {
        if type_subst.is_empty() {
            return base_c_name.to_string();
        }
        let args: String = type_subst.iter()
            .map(|(_, c_ty)| Self::sanitize_c_for_ident(c_ty))
            .collect::<Vec<_>>()
            .join("__");
        format!("{}____{}", base_c_name, args)
    }

    /// Plan 48 Ф.7.4 (partial): try to infer mono type-args for a bare variant
    /// constructor call like `Ok2(42)` where the parent sum-type is generic.
    /// Returns (parent_type_name, mangled_instance_c_name, type_args_c) when:
    ///   - the variant's parent type is a generic template
    ///   - the variant is tuple-shaped with at least one positional arg
    ///   - every generic param can be inferred from a corresponding arg's C type
    /// On success, also enqueues the instance for mono emission.
    ///
    /// Returns None for unit variants (`Err2`) or when inference is incomplete —
    /// caller falls back to the erased emit path. Unit-variant inference would
    /// need usage-context propagation (deferred to V2).
    fn try_infer_variant_mono_args(
        &self,
        variant_name: &str,
        args: &[crate::ast::CallArg],
    ) -> Option<(String, String, Vec<String>)> {
        use crate::ast::{TypeDeclKind, SumVariantKind};
        // 1. Find parent sum-type by variant name.
        let (parent_type, _) = self.find_variant(variant_name)?;
        // 2. Must be a generic template (has type params).
        let template = self.generic_type_templates.get(&parent_type)?.clone();
        if template.generics.is_empty() { return None; }
        // 3. Must be Sum with the variant present and tuple-shaped.
        let variants = match &template.kind {
            TypeDeclKind::Sum(vs) => vs,
            _ => return None,
        };
        let variant = variants.iter().find(|v| v.name == variant_name)?;
        let field_types = match &variant.kind {
            SumVariantKind::Tuple(tys) => tys.clone(),
            // Unit / record variants are out of scope for this helper.
            _ => return None,
        };
        if field_types.is_empty() || args.is_empty() { return None; }
        // 4. Infer subst[T] from each (declared field type, arg C type).
        let mut subst: Vec<(String, Option<String>)> = template.generics.iter()
            .map(|g| (g.name.clone(), None))
            .collect();
        for (field_ty, arg) in field_types.iter().zip(args.iter()) {
            let arg_c = self.infer_expr_c_type(arg.expr());
            Self::infer_type_param_binding(field_ty, &arg_c, &mut subst);
        }
        // 5. Require every generic param resolved.
        let type_args_c: Vec<String> = subst.iter()
            .map(|(_, opt)| opt.clone())
            .collect::<Option<Vec<String>>>()?;
        // 6. Compute mangled instance name and enqueue for mono emit.
        let mangled = Self::compute_generic_type_c_name(&parent_type, &type_args_c);
        if !self.emitted_generic_type_instances.contains(&mangled) {
            let mut wl = self.generic_type_worklist.borrow_mut();
            if !wl.iter().any(|(_, _, m)| m == &mangled) {
                wl.push((parent_type.clone(), type_args_c.clone(), mangled.clone()));
            }
        }
        self.generic_type_instance_info.borrow_mut()
            .entry(mangled.clone())
            .or_insert_with(|| (parent_type.clone(), type_args_c.clone()));
        Some((parent_type, mangled, type_args_c))
    }

    /// Plan 48 Ф.0: resolve concrete type args for a generic fn call.
    /// Returns Vec<(param_name, c_type)> or Err with a helpful message (R5).
    /// Priority: turbofish > arg-type inference > return-type context.
    fn resolve_mono_type_args(
        &self,
        fn_decl: &crate::ast::FnDecl,
        turbofish_refs: &[crate::ast::TypeRef],
        args: &[crate::ast::CallArg],
    ) -> Result<Vec<(String, String)>, String> {
        let type_params: Vec<String> = fn_decl.generics.iter().map(|g| g.name.clone()).collect();
        if type_params.is_empty() {
            return Ok(vec![]);
        }
        // Initialize with None slots
        let mut subst: Vec<(String, Option<String>)> = type_params.iter()
            .map(|n| (n.clone(), None))
            .collect();
        // Source 1: turbofish (highest priority)
        for (i, tr) in turbofish_refs.iter().enumerate() {
            if i < subst.len() {
                if let Ok(c_ty) = self.type_ref_to_c(tr) {
                    if !c_ty.is_empty() && c_ty != "void*" {
                        subst[i].1 = Some(c_ty);
                    }
                }
            }
        }
        // Source 2: infer from actual arg types.
        // D109 Ф.7.7: two-pass — non-array params first so that a direct `target K` binding
        // (e.g. K = Nova_GrmPoint*) is set before the array param `items []K` would infer
        // K = nova_int from the erased NovaArray_nova_int* runtime representation.
        for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
            if !matches!(param.ty, crate::ast::TypeRef::Array(..)) {
                let arg_c = self.infer_expr_c_type(arg.expr());
                Self::infer_type_param_binding(&param.ty, &arg_c, &mut subst);
            }
        }
        for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
            if matches!(param.ty, crate::ast::TypeRef::Array(..)) {
                let arg_c = self.infer_expr_c_type(arg.expr());
                Self::infer_type_param_binding(&param.ty, &arg_c, &mut subst);
            }
        }
        // Source 2b: for fn-typed params, infer return type T from closure arg body.
        // Handles `body fn() Fail[E] -> T` where arg is `|| 42` → T = nova_int.
        for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
            if let crate::ast::TypeRef::Func { return_type: Some(ret_ty_ref), .. } = &param.ty {
                let closure_ret_c = match &arg.expr().kind {
                    ExprKind::ClosureLight { body, .. } => match body {
                        crate::ast::ClosureBody::Expr(e) => {
                            let t = self.infer_expr_c_type(e);
                            if t.is_empty() || t == "void*" { String::new() } else { t }
                        }
                        crate::ast::ClosureBody::Block(b) => b.trailing.as_ref()
                            .map(|e| self.infer_expr_c_type(e))
                            .filter(|t| !t.is_empty() && t != "void*")
                            .unwrap_or_default(),
                    },
                    ExprKind::ClosureFull(sb) => sb.return_type.as_ref()
                        .and_then(|rt| self.type_ref_to_c(rt).ok())
                        .filter(|t| !t.is_empty() && t != "void*")
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                if !closure_ret_c.is_empty() {
                    Self::infer_type_param_binding(ret_ty_ref.as_ref(), &closure_ret_c, &mut subst);
                }
            }
        }
        // Plan 55 Ф.1 (Source 2b-array): for `[]fn(P...) -> T` params, infer T
        // from the array literal's first closure element. Without this, the
        // `[]T` Source 2 above sees concrete=`NovaArray_void_p*` and would
        // bind T = "void_p", which is wrong. This source overrides with the
        // actual closure return type.
        for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
            if let crate::ast::TypeRef::Array(inner, _) = &param.ty {
                if let crate::ast::TypeRef::Func { return_type: Some(ret_ty_ref), .. } = inner.as_ref() {
                    // Find first closure-literal element of the array arg.
                    let closure_ret_c: String = if let ExprKind::ArrayLit(elems) = &arg.expr().kind {
                        elems.iter().find_map(|e| {
                            let ArrayElem::Item(expr) = e else { return None; };
                            match &expr.kind {
                                ExprKind::ClosureLight { body, .. } => match body {
                                    crate::ast::ClosureBody::Expr(ce) => {
                                        let t = self.infer_expr_c_type(ce);
                                        if t.is_empty() || t == "void*" { None } else { Some(t) }
                                    }
                                    crate::ast::ClosureBody::Block(b) => b.trailing.as_ref()
                                        .map(|ce| self.infer_expr_c_type(ce))
                                        .filter(|t| !t.is_empty() && t != "void*"),
                                },
                                ExprKind::ClosureFull(sb) => sb.return_type.as_ref()
                                    .and_then(|rt| self.type_ref_to_c(rt).ok())
                                    .filter(|t| !t.is_empty() && t != "void*"),
                                _ => None,
                            }
                        }).unwrap_or_default()
                    } else if let ExprKind::Ident(name) = &arg.expr().kind {
                        // Variable holding a []fn(...) -> T value — look up element sig.
                        self.array_param_fn_sigs.get(name)
                            .map(|(_, r)| r.clone())
                            .filter(|t| !t.is_empty() && t != "void*" && t != "nova_unit")
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    if !closure_ret_c.is_empty() {
                        // Clear any prior "void_p" binding before re-binding to the real T.
                        if let Some(name) = match ret_ty_ref.as_ref() {
                            crate::ast::TypeRef::Named { path, generics, .. } if generics.is_empty() => Some(path.join("_")),
                            _ => None,
                        } {
                            if let Some(slot) = subst.iter_mut().find(|(n, _)| n == &name) {
                                if slot.1.as_deref() == Some("void_p") { slot.1 = None; }
                            }
                        }
                        Self::infer_type_param_binding(ret_ty_ref.as_ref(), &closure_ret_c, &mut subst);
                    }
                }
            }
        }
        // Plan 54 Ф.5 (Source 2d): для fn-typed param когда arg —
        // variable reference (не closure literal). Если var_types/
        // fn_param_sigs знают return-type of variable's closure, можем
        // infer T. Пример: nested generic call `with_timeout[T] calls
        // within(ms, body)` — body's return type из fn_param_sigs уже
        // substituted (если we внутри mono'd with_timeout body), используем
        // его чтобы infer within's T.
        for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
            if let crate::ast::TypeRef::Func { return_type: Some(ret_ty_ref), .. } = &param.ty {
                if let ExprKind::Ident(name) = &arg.expr().kind {
                    if let Some((_, ret_c)) = self.fn_param_sigs.get(name) {
                        if !ret_c.is_empty() && ret_c != "void*" && ret_c != "nova_unit" {
                            Self::infer_type_param_binding(ret_ty_ref.as_ref(), ret_c, &mut subst);
                        }
                    }
                }
            }
        }
        // Source 2c: for generic-type params (e.g. `box_get[T](b Box[T])`), extract T from
        // monomorphized instance info. After Ф.3, Box[int] arg has C type Nova_Box____nova_int*
        // and generic_type_instance_info maps "Nova_Box____nova_int" → ("Box", ["nova_int"]).
        for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
            if let crate::ast::TypeRef::Named { generics, .. } = &param.ty {
                if generics.is_empty() { continue; }
                let arg_c = self.infer_expr_c_type(arg.expr());
                let key = arg_c.trim_end_matches('*').trim().to_string();
                let instance_args: Option<Vec<String>> = self.generic_type_instance_info
                    .borrow()
                    .get(&key)
                    .map(|(_, args)| args.clone());
                if let Some(iargs) = instance_args {
                    if iargs.len() == generics.len() {
                        for (gen_ty, c_ty) in generics.iter().zip(iargs.iter()) {
                            Self::infer_type_param_binding(gen_ty, c_ty, &mut subst);
                        }
                    }
                }
            }
        }
        // Source 3: infer from current_fn_return_ty vs fn return type
        if let Some(ref ret_ty) = fn_decl.return_type {
            if let Some(ref actual_ret) = self.current_fn_return_ty {
                Self::infer_type_param_binding(ret_ty, actual_ret, &mut subst);
            }
        }
        // Collect results — error on unresolved
        let mut result = Vec::new();
        for (name, resolved) in subst {
            match resolved {
                Some(c_ty) => result.push((name, c_ty)),
                None => {
                    // Plan 48 Ф.7.5: указать в каком параметре T встречается.
                    // Помогает LLM/user понять, какой argument добавить или
                    // как явно дать turbofish.
                    let positions: Vec<String> = fn_decl.params.iter().enumerate()
                        .filter_map(|(i, p)| {
                            let mut found = false;
                            let mut names = std::collections::HashSet::new();
                            Self::collect_typeref_names(&p.ty, &mut names, &mut std::collections::HashSet::new());
                            if names.contains(&name) { found = true; }
                            if found {
                                Some(format!("param `{}` (#{i})", p.name))
                            } else { None }
                        })
                        .collect();
                    let where_used = if positions.is_empty() {
                        " (returned only — turbofish required)".to_string()
                    } else {
                        format!(" — appears in {}", positions.join(", "))
                    };
                    return Err(format!(
                        "cannot infer type argument `{name}` for generic function `{}`{}; \
                         use turbofish: `{}[{name}](...)`",
                        fn_decl.name, where_used, fn_decl.name
                    ));
                }
            }
        }
        Ok(result)
    }

    /// Plan 48 Ф.0: match param_typeref against concrete_c, bind type params in subst.
    fn infer_type_param_binding(
        param_ty: &crate::ast::TypeRef,
        concrete_c: &str,
        subst: &mut Vec<(String, Option<String>)>,
    ) {
        if concrete_c.is_empty() || concrete_c == "void*" { return; }
        match param_ty {
            // Bare T → bind T = concrete_c
            crate::ast::TypeRef::Named { path, generics, .. } if generics.is_empty() => {
                let name = path.join("_");
                if let Some(slot) = subst.iter_mut().find(|(n, _)| n == &name) {
                    if slot.1.is_none() {
                        slot.1 = Some(concrete_c.to_string());
                    }
                }
            }
            // []T → extract element type from NovaArray_X*
            crate::ast::TypeRef::Array(inner, _) => {
                // concrete_c like "NovaArray_nova_int*"
                if let Some(inner_c) = concrete_c
                    .strip_prefix("NovaArray_")
                    .and_then(|s| s.strip_suffix('*'))
                {
                    Self::infer_type_param_binding(inner, inner_c, subst);
                }
            }
            // fn(..)->T: skip (closure C type doesn't encode return type directly)
            _ => {}
        }
    }

    /// Plan 48: apply a type param substitution to a TypeRef, returning a C type string.
    /// Used in infer_expr_c_type for resolving the return type of generic fn calls.
    /// Returns None if type cannot be resolved from the subst alone (e.g. non-named types).
    fn apply_type_subst_to_ref(
        ty: &crate::ast::TypeRef,
        subst: &[(String, Option<String>)],
    ) -> Option<String> {
        match ty {
            crate::ast::TypeRef::Named { path, generics, .. } if generics.is_empty() => {
                let name = path.join("_");
                // Check if this is a type param
                if let Some((_, Some(c_ty))) = subst.iter().find(|(n, _)| n == &name) {
                    return Some(c_ty.clone());
                }
                // Known primitive names
                let c = match name.as_str() {
                    "int" | "i64" => "nova_int",
                    "f64" => "nova_f64",
                    "bool" => "nova_bool",
                    "str" => "nova_str",
                    "byte" => "nova_byte",
                    _ => return None,
                };
                Some(c.to_string())
            }
            // Option[T] → NovaOpt_<T_c>
            crate::ast::TypeRef::Named { path, generics, .. }
                if path.last().map(|s| s.as_str()) == Some("Option") && generics.len() == 1 =>
            {
                let inner_c = Self::apply_type_subst_to_ref(&generics[0], subst)?;
                let sanitized = Self::sanitize_for_novaopt(&inner_c);
                Some(format!("NovaOpt_{}", sanitized))
            }
            crate::ast::TypeRef::Array(inner, _) => {
                // []T → NovaArray_<inner_c>*
                let inner_c = Self::apply_type_subst_to_ref(inner, subst)?;
                Some(format!("NovaArray_{}*", inner_c))
            }
            // Generic user-defined type e.g. Pair[B, A] → Nova_Pair____T1__T2*
            crate::ast::TypeRef::Named { path, generics, .. }
                if !generics.is_empty()
                    && path.last().map(|s| s.as_str()) != Some("Option") =>
            {
                let mut resolved = Vec::new();
                for g in generics {
                    if let Some(c) = Self::apply_type_subst_to_ref(g, subst) {
                        resolved.push(c);
                    } else {
                        return None;
                    }
                }
                let base = path.last().cloned().unwrap_or_default();
                let mangled = Self::compute_generic_type_c_name(&base, &resolved);
                Some(format!("{}*", mangled))
            }
            crate::ast::TypeRef::Unit(_) => Some("nova_unit".to_string()),
            crate::ast::TypeRef::Tuple(elems, _) if elems.is_empty() => {
                // () zero-tuple is nova_unit, same as TypeRef::Unit.
                Some("nova_unit".to_string())
            }
            crate::ast::TypeRef::Tuple(elems, _) => {
                // (A, B, ...) → erased as void* (tuple mono is not V1 scope).
                // _NovaTupleN uses nova_int fields, can't hold nova_str directly.
                let _ = elems;
                None
            }
            _ => None,
        }
    }

    /// D109: convert TypeRef to C type string without &self context.
    /// Used in infer_expr_c_type for TurboFish member call return type resolution.
    fn simple_type_ref_to_c(tr: &crate::ast::TypeRef) -> String {
        use crate::ast::TypeRef;
        match tr {
            TypeRef::Named { path, generics, .. } if generics.is_empty() => {
                match path.join("_").as_str() {
                    "str"           => "nova_str".to_string(),
                    "int" | "i64"  => "nova_int".to_string(),
                    "bool"          => "nova_bool".to_string(),
                    "f64"           => "nova_f64".to_string(),
                    "f32"           => "nova_f32".to_string(),
                    "byte" | "u8"  => "nova_byte".to_string(),
                    "unit"          => "nova_unit".to_string(),
                    other           => format!("Nova_{}*", other),
                }
            }
            TypeRef::Named { path, generics, .. } => {
                let base = path.last().cloned().unwrap_or_default();
                let args: Vec<String> = generics.iter()
                    .map(|g| Self::simple_type_ref_to_c(g))
                    .collect();
                format!("{}*", Self::compute_generic_type_c_name(&base, &args))
            }
            _ => "nova_int".to_string(),
        }
    }

    /// Plan 48 V1 fallback: register/emit a void*-erased version of a generic fn on demand.
    /// Called when type argument inference fails (e.g. generic record params).
    /// Idempotent — body is only emitted once (guarded by mono_instantiated).
    fn register_erased_instance(&mut self, fn_decl: &crate::ast::FnDecl) {
        let erased_name = format!("nova_fn_{}", fn_decl.name);
        if self.mono_instantiated.contains(&erased_name) {
            return; // Already registered (either erased or mono base name)
        }
        self.mono_instantiated.insert(erased_name.clone());
        // Build erased signature: type params → void*, other params → void*
        let type_params: HashSet<String> = fn_decl.generics.iter().map(|g| g.name.clone()).collect();
        let params_str = if fn_decl.params.is_empty() {
            "void".to_string()
        } else {
            fn_decl.params.iter().map(|p| {
                match &p.ty {
                    TypeRef::Named { path, generics, .. } => {
                        let nm = path.join("_");
                        if type_params.contains(&nm) { format!("void* {}", p.name) }
                        else if !generics.is_empty() && self.record_schemas.contains_key(&nm) {
                            format!("Nova_{}* {}", nm, p.name)
                        } else { format!("void* {}", p.name) }
                    }
                    _ => format!("void* {}", p.name),
                }
            }).collect::<Vec<_>>().join(", ")
        };
        // Emit forward decl into mono_fwd_decls buffer
        self.mono_fwd_decls.push_str(&format!("static void* {}({});\n", erased_name, params_str));
        // Register var_types for erased return (legacy)
        self.var_types.insert(format!("fn_ret_{}", fn_decl.name), "void*".into());
        // Enqueue in worklist with special marker: empty type_subst = erased mode
        // We store the ERASED_SENTINEL as fn_name with special prefix to distinguish from mono
        self.mono_worklist.push((
            format!("__erased__{}", fn_decl.name),
            vec![],
            erased_name,
        ));
    }

    /// Plan 48: register a monomorphized fn instance (add forward decl + worklist entry).
    /// Idempotent — safe to call multiple times with the same mono_name.
    fn register_mono_instance(
        &mut self,
        fn_decl: &crate::ast::FnDecl,
        type_subst: Vec<(String, String)>,
        mono_name: &str,
    ) {
        if self.mono_instantiated.contains(mono_name) {
            return;
        }
        self.mono_instantiated.insert(mono_name.to_string());
        // Compute param and return C types with substitution applied
        let saved_subst = std::mem::replace(
            &mut self.current_type_subst,
            type_subst.iter().cloned().collect(),
        );
        let param_c_tys: Vec<String> = fn_decl.params.iter()
            .map(|p| self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into()))
            .collect();
        let ret_c = fn_decl.return_type.as_ref()
            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
            .unwrap_or_else(|| "nova_unit".into());
        self.current_type_subst = saved_subst;
        let params_str = if fn_decl.params.is_empty() {
            "void".to_string()
        } else {
            fn_decl.params.iter().zip(&param_c_tys)
                .map(|(p, ty)| format!("{} {}", ty, p.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        // Emit forward decl into buffer
        self.mono_fwd_decls.push_str(&format!(
            "static {} {}({});\n",
            ret_c, mono_name, params_str
        ));
        // Enqueue for body emission
        self.mono_worklist.push((fn_decl.name.clone(), type_subst, mono_name.to_string()));
    }

    /// Plan 48: register a monomorphized METHOD instance (add forward decl + worklist entry).
    /// Like register_mono_instance but prepends `nova_self` receiver param.
    fn register_mono_method_instance(
        &mut self,
        fn_decl: &crate::ast::FnDecl,
        type_subst: Vec<(String, String)>,
        mono_name: &str,
        recv_type: &str,
    ) {
        if self.mono_instantiated.contains(mono_name) {
            return;
        }
        self.mono_instantiated.insert(mono_name.to_string());
        // Compute param and return C types with substitution applied
        let saved_subst = std::mem::replace(
            &mut self.current_type_subst,
            type_subst.iter().cloned().collect(),
        );
        let recv_c = self.receiver_c_type(recv_type);
        let param_c_tys: Vec<String> = fn_decl.params.iter()
            .map(|p| self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into()))
            .collect();
        // Set current_receiver_type before computing ret_c so that `Self` in
        // the return type resolves to the concrete monomorphized type.
        let prev_recv_for_ret = self.current_receiver_type.replace(recv_type.to_string());
        let ret_c = fn_decl.return_type.as_ref()
            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
            .unwrap_or_else(|| "nova_unit".into());
        self.current_receiver_type = prev_recv_for_ret;
        self.current_type_subst = saved_subst;
        // Plan 48 Ф.7.2: static-методы без nova_self.
        let is_instance = matches!(fn_decl.receiver.as_ref().map(|r| &r.kind),
            Some(crate::ast::ReceiverKind::Instance));
        let mut parts: Vec<String> = if is_instance {
            vec![format!("{} nova_self", recv_c)]
        } else {
            Vec::new()
        };
        for (p, ty) in fn_decl.params.iter().zip(&param_c_tys) {
            parts.push(format!("{} {}", ty, p.name));
        }
        let params_str = if parts.is_empty() { "void".to_string() } else { parts.join(", ") };
        // Forward decl
        self.mono_fwd_decls.push_str(&format!(
            "static {} {}({});\n",
            ret_c, mono_name, params_str
        ));
        // Enqueue for body emission — prefix __method__TYPE::name so worklist drain can route
        let worklist_key = format!("__method__{}::{}", recv_type, fn_decl.name);
        self.mono_worklist.push((worklist_key, type_subst, mono_name.to_string()));
    }

    /// Plan 48: emit a monomorphized METHOD body (instance method variant of emit_monomorphized_fn).
    fn emit_monomorphized_method(
        &mut self,
        fn_decl: &crate::ast::FnDecl,
        type_subst: Vec<(String, String)>,
        mono_name: &str,
        recv_type: &str,
    ) -> Result<(), String> {
        use crate::ast::FnBody;
        // Set type substitution
        let saved_subst = std::mem::replace(
            &mut self.current_type_subst,
            type_subst.iter().cloned().collect(),
        );
        let recv_c = self.receiver_c_type(recv_type);
        let param_c_tys: Vec<String> = fn_decl.params.iter()
            .map(|p| self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into()))
            .collect();
        // Set current_receiver_type before computing ret_c so that `Self` in
        // the return type resolves to the concrete monomorphized type.
        let prev_recv_for_ret = self.current_receiver_type.replace(recv_type.to_string());
        let ret_c = fn_decl.return_type.as_ref()
            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
            .unwrap_or_else(|| "nova_unit".into());
        self.current_receiver_type = prev_recv_for_ret;
        // Plan 48 Ф.7.2: static-методы без nova_self.
        let is_instance = matches!(fn_decl.receiver.as_ref().map(|r| &r.kind),
            Some(crate::ast::ReceiverKind::Instance));
        let mut parts: Vec<String> = if is_instance {
            vec![format!("{} nova_self", recv_c)]
        } else {
            Vec::new()
        };
        for (p, ty) in fn_decl.params.iter().zip(&param_c_tys) {
            parts.push(format!("{} {}", ty, p.name));
        }
        let params_str = if parts.is_empty() { "void".to_string() } else { parts.join(", ") };
        // Buffer body
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        self.line(&format!("static {} {}({}) {{", ret_c, mono_name, params_str));
        self.indent += 1;
        // Register nova_self and params in var_types (только если instance method)
        let prev_self = if is_instance {
            self.var_types.insert("nova_self".to_string(), recv_c.clone())
        } else {
            None
        };
        let saved_var_types: Vec<(String, Option<String>)> = fn_decl.params.iter()
            .zip(&param_c_tys)
            .map(|(p, ty)| (p.name.clone(), self.var_types.insert(p.name.clone(), ty.clone())))
            .collect();
        // Register function-typed params in fn_param_sigs with concrete types
        let mut saved_fn_sigs: Vec<(String, Option<(Vec<String>, String)>)> = Vec::new();
        for (p, _c_ty) in fn_decl.params.iter().zip(&param_c_tys) {
            if let crate::ast::TypeRef::Func { params: fp, return_type, .. } = &p.ty {
                let inner_ptys: Vec<String> = fp.iter()
                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                    .collect();
                let inner_ret = return_type.as_ref()
                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
                    .unwrap_or_else(|| "nova_unit".into());
                let prev = self.fn_param_sigs.insert(p.name.clone(), (inner_ptys, inner_ret));
                saved_fn_sigs.push((p.name.clone(), prev));
            }
        }
        // Pre-populate tuple_element_types for array-of-tuple parameters in mono context.
        // Enables `for (k, v) in pairs` to extract typed fields without losing the
        // concrete K/V → nova_str/nova_int substitution stored in current_type_subst.
        let mut saved_tuple_elem_keys: Vec<String> = Vec::new();
        for p in &fn_decl.params {
            if let crate::ast::TypeRef::Array(inner, _) = &p.ty {
                if let crate::ast::TypeRef::Tuple(elems, _) = inner.as_ref() {
                    if !elems.is_empty() {
                        let field_tys: Vec<String> = elems.iter()
                            .map(|e| {
                                let c_ty = self.type_ref_to_c(e)
                                    .unwrap_or_else(|_| "nova_int".to_string());
                                // Mirror emit_tuple_lit boxing: struct types (nova_str, nova_unit,
                                // _NovaTupleN, NovaOpt_*) are heap-allocated and stored as pointer.
                                let needs_heap = c_ty.starts_with("_NovaTuple")
                                    || c_ty.starts_with("NovaOpt_")
                                    || c_ty == "nova_str"
                                    || c_ty == "nova_unit";
                                if needs_heap && !c_ty.ends_with('*') {
                                    format!("{}*", c_ty)
                                } else {
                                    c_ty
                                }
                            })
                            .collect();
                        self.tuple_element_types.insert(p.name.clone(), field_tys);
                        saved_tuple_elem_keys.push(p.name.clone());
                    }
                }
            }
        }
        // D109: Pre-populate array_element_types for pointer-stomped array fields of the
        // receiver type, so @buckets[idx] casts to the concrete element type.
        // (current_type_subst is already set, so type_ref_to_c resolves K/V correctly.)
        let mut saved_mono_array_elem_keys: Vec<String> = Vec::new();
        if is_instance {
            let base_opt = self.generic_type_instance_info.borrow()
                .get(&format!("Nova_{}", recv_type)).map(|(b, _)| b.clone());
            if let Some(base_name) = base_opt {
                if let Some(template) = self.generic_type_templates.get(&base_name).cloned() {
                    use crate::ast::TypeDeclKind;
                    if let TypeDeclKind::Record(fields) = &template.kind {
                        for fld in fields {
                            if let crate::ast::TypeRef::Array(elem_ty, _) = &fld.ty {
                                if let Ok(elem_c) = self.type_ref_to_c(elem_ty) {
                                    if elem_c.ends_with('*') && elem_c != "nova_int*" {
                                        let field_c = Self::mangle_field_name(&fld.name);
                                        let key = format!("(nova_self->{})", field_c);
                                        self.array_element_types.insert(key.clone(), elem_c);
                                        saved_mono_array_elem_keys.push(key);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Drain any generic type instances enqueued during array_element_types setup above.
        // Without this, sum_schemas / record_variant_field_types for types like
        // Slot____nova_int__nova_unit are not yet populated when pattern_bind_typed
        // runs during body emission — causing field-type lookups to fall back to the
        // erased base type (e.g. "Slot") whose fields are typed void* (the erased form).
        if !self.generic_type_worklist.borrow().is_empty() {
            self.drain_generic_type_worklist()?;
        }
        // Set receiver type so @field and Self resolve correctly
        let saved_recv = std::mem::replace(&mut self.current_receiver_type, Some(recv_type.to_string()));
        let saved_ret_ty = std::mem::replace(&mut self.current_fn_return_ty, Some(ret_c.clone()));
        let saved_expected = std::mem::replace(
            &mut self.expected_record_type,
            fn_decl.return_type.as_ref().and_then(|t| {
                if let crate::ast::TypeRef::Named { path, generics, .. } = t {
                    if generics.is_empty() { Some(path.join("_")) } else { None }
                } else { None }
            }),
        );
        // Ф.7: Erased Array runtime always returns NovaOpt_nova_int; emit a bridge wrapper
        // for methods whose concrete return type is NovaOpt_T (T != nova_int).
        // Bridge only works for pointer types: scalars (nova_str, nova_bool, nova_f64, nova_byte)
        // are stored by value in typed arrays — the erased version reads them as nova_int
        // (wrong element size), so those must get a proper monomorphized body instead.
        let bridge_emitted = if let Some(inner_t) = ret_c.strip_prefix("NovaOpt_") {
            if inner_t != "nova_int" && is_instance && inner_t.ends_with('*') {
                let info = self.generic_type_instance_info.borrow();
                let base_opt = info.get(&format!("Nova_{}", recv_type)).map(|(b, _)| b.clone());
                drop(info);
                if let Some(base_name) = base_opt {
                    let erased_method = format!("Nova_{}_method_{}", base_name, fn_decl.name);
                    let base_recv_ty = format!("Nova_{}*", base_name);
                    let value_convert = format!("({})(intptr_t)(_erased.value)", inner_t);
                    self.line(&format!(
                        "NovaOpt_nova_int _erased = {}(({})nova_self);",
                        erased_method, base_recv_ty
                    ));
                    self.line("if (_erased.tag == NOVA_TAG_Option_None) {");
                    self.indent += 1;
                    self.line(&format!(
                        "return (({}){{.tag = NOVA_TAG_Option_None}});",
                        ret_c
                    ));
                    self.indent -= 1;
                    self.line("}");
                    self.line(&format!(
                        "return (({}){{.tag = NOVA_TAG_Option_Some, .value = {}}});",
                        ret_c, value_convert
                    ));
                    true
                } else { false }
            } else { false }
        } else { false };
        // Emit body
        let body_clone = fn_decl.body.clone();
        if !bridge_emitted { match &body_clone {
            FnBody::Expr(e) => {
                self.emit_source_annotation_for_expr(e);
                let val = self.emit_expr(e)?;
                if ret_c == "nova_unit" {
                    self.line(&format!("{};", val));
                    self.line("return NOVA_UNIT;");
                } else {
                    self.line(&format!("return {};", val));
                }
            }
            FnBody::Block(block) => {
                let block_id = self.enter_defer_scope(block, false);
                for stmt in &block.stmts {
                    self.emit_stmt(stmt)?;
                }
                if let Some(trailing) = &block.trailing {
                    self.emit_source_annotation_for_expr(trailing);
                    let trailing_ty = self.infer_expr_c_type(trailing);
                    let val = self.emit_expr(trailing)?;
                    self.leave_defer_scope(block_id);
                    if ret_c == "nova_unit" {
                        self.line(&format!("{};", val));
                        self.line("return NOVA_UNIT;");
                    } else if trailing_ty == "nova_unit" && ret_c != "nova_unit" {
                        // Trailing is unit (e.g. infinite loop with internal returns)
                        // but fn returns non-unit. Emit side-effect + dummy unreachable return.
                        self.line(&format!("{};", val));
                        self.line(&format!("return ({})0; /* unreachable */", ret_c));
                    } else {
                        self.line(&format!("return {};", val));
                    }
                } else {
                    self.leave_defer_scope(block_id);
                    self.line("return NOVA_UNIT;");
                }
            }
            FnBody::External => {}
        } } // end: match body, if !bridge_emitted
        // Restore
        for (name, prev) in saved_var_types {
            match prev {
                Some(old) => { self.var_types.insert(name, old); }
                None => { self.var_types.remove(&name); }
            }
        }
        if is_instance {
            match prev_self {
                Some(old) => { self.var_types.insert("nova_self".to_string(), old); }
                None => { self.var_types.remove("nova_self"); }
            }
        } else {
            let _ = prev_self;
        }
        for (name, prev) in saved_fn_sigs {
            match prev {
                Some(old) => { self.fn_param_sigs.insert(name, old); }
                None => { self.fn_param_sigs.remove(&name); }
            }
        }
        self.current_receiver_type = saved_recv;
        self.current_fn_return_ty = saved_ret_ty;
        self.expected_record_type = saved_expected;
        for key in &saved_mono_array_elem_keys {
            self.array_element_types.remove(key);
        }
        for key in &saved_tuple_elem_keys {
            self.tuple_element_types.remove(key);
        }
        self.flush_boxed_vars();
        self.indent -= 1;
        self.line("}");
        self.line("");
        let fn_body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        if !self.lambda_forward_decls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_forward_decls));
        }
        if !self.lambda_impls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_impls));
        }
        self.out.push_str(&fn_body);
        self.current_type_subst = saved_subst;
        Ok(())
    }

    /// Plan 48 Ф.2: emit a monomorphized function body.
    /// This is like emit_fn but with `current_type_subst` set for concrete type resolution.
    /// Plan 48 Ф.3: drain the generic type instance worklist.
    /// Emits concrete struct/sum definitions for each queued instance into
    /// `generic_type_defs_buf` (spliced before fn definitions via marker).
    /// May enqueue further instances (nested generics), so loops until empty.
    fn drain_generic_type_worklist(&mut self) -> Result<(), String> {
        let mut depth = 0usize;
        loop {
            if self.generic_type_worklist.borrow().is_empty() { break; }
            let batch: Vec<(String, Vec<String>, String)> =
                self.generic_type_worklist.borrow_mut().drain(..).collect();
            for (base_name, type_args_c, mangled) in batch {
                if self.emitted_generic_type_instances.contains(&mangled) { continue; }
                self.emitted_generic_type_instances.insert(mangled.clone());
                // Register instance info for Source 2c in resolve_mono_type_args:
                // "Nova_Box____nova_int" → ("Box", ["nova_int"])
                self.generic_type_instance_info.borrow_mut()
                    .entry(mangled.clone())
                    .or_insert_with(|| (base_name.clone(), type_args_c.clone()));
                let template = match self.generic_type_templates.get(&base_name).cloned() {
                    Some(t) => t,
                    None => continue,
                };
                let type_subst: Vec<(String, String)> = template.generics.iter()
                    .zip(type_args_c.iter())
                    .map(|(g, c)| (g.name.clone(), c.clone()))
                    .collect();
                // Redirect output to generic_type_defs_buf so instances appear
                // before fn definitions in the final C output (via marker splice).
                let saved_out = std::mem::take(&mut self.out);
                self.emit_generic_type_instance(&template.clone(), &type_subst, &mangled)?;
                let instance_code = std::mem::take(&mut self.out);
                self.generic_type_defs_buf.push_str(&instance_code);
                self.out = saved_out;
            }
            depth += 1;
            if depth > self.mono_depth_limit {
                return Err(format!(
                    "generic type instantiation depth limit {} exceeded (possible recursive generic types); \
                     raise via --mono-depth=N CLI flag (or NOVA_MONO_DEPTH env var)",
                    self.mono_depth_limit
                ));
            }
        }
        Ok(())
    }

    /// Plan 48 Ф.3: emit a concrete C struct/union for one generic type instance.
    fn emit_generic_type_instance(
        &mut self,
        template: &crate::ast::TypeDecl,
        type_subst: &[(String, String)],
        mangled: &str,
    ) -> Result<(), String> {
        use crate::ast::TypeDeclKind;
        use crate::ast::SumVariantKind;

        let saved_subst = std::mem::replace(
            &mut self.current_type_subst,
            type_subst.iter().cloned().collect(),
        );

        match template.kind.clone() {
            TypeDeclKind::Record(fields) => {
                let mut schema: HashMap<String, String> = HashMap::new();
                // Pre-compute field types so we can emit forward decls for
                // pointer-to-struct fields before the struct definition. This
                // handles cases where a field references another generic instance
                // (e.g. `Nova_Lru____K__V` has `Nova_HashMap____K__V* store`)
                // that may be instantiated later in generic_type_defs_buf.
                let field_ctys: Vec<String> = fields.iter()
                    .map(|f| self.type_ref_to_c(&f.ty).unwrap_or_else(|_| "nova_int".into()))
                    .collect();
                for c_ty in &field_ctys {
                    if let Some(inner) = c_ty.strip_suffix('*') {
                        let inner = inner.trim();
                        if inner.starts_with("Nova_") {
                            self.line(&format!("typedef struct {0} {0};", inner));
                        }
                    }
                }
                // Forward decl to handle circular/self-referential types
                self.line(&format!("typedef struct {0} {0};", mangled));
                self.line(&format!("struct {} {{", mangled));
                self.indent += 1;
                for (f, c_ty) in fields.iter().zip(field_ctys) {
                    let mf = Self::mangle_field_name(&f.name);
                    self.line(&format!("{} {};", c_ty, mf));
                    schema.insert(f.name.clone(), c_ty);
                }
                self.indent -= 1;
                self.line("};");
                self.line("");
                let schema_key = mangled.strip_prefix("Nova_").unwrap_or(mangled);
                self.record_schemas.insert(schema_key.to_string(), schema);
            }
            TypeDeclKind::Sum(variants) => {
                // Tag enum
                self.line("typedef enum {");
                self.indent += 1;
                for v in &variants {
                    self.line(&format!("NOVA_TAG_{}_{},", mangled, v.name));
                }
                self.indent -= 1;
                self.line(&format!("}} {}_Tag;", mangled));

                let mut sum_schema: HashMap<String, Vec<String>> = HashMap::new();
                // Forward decl
                self.line(&format!("typedef struct {0} {0};", mangled));
                self.line(&format!("struct {} {{", mangled));
                self.indent += 1;
                self.line(&format!("{}_Tag tag;", mangled));
                self.line("union {");
                self.indent += 1;
                let has_payload = variants.iter().any(|v| !matches!(v.kind, SumVariantKind::Unit));
                if !has_payload {
                    self.line("char _dummy;");
                }
                for v in &variants {
                    match &v.kind {
                        SumVariantKind::Unit => {
                            sum_schema.insert(v.name.clone(), vec![]);
                        }
                        SumVariantKind::Tuple(types) => {
                            let mut field_types = Vec::new();
                            self.line("struct {");
                            self.indent += 1;
                            for (i, ty) in types.iter().enumerate() {
                                let tc = self.type_ref_to_c(ty)?;
                                field_types.push(tc.clone());
                                self.line(&format!("{} _{};", tc, i));
                            }
                            self.indent -= 1;
                            self.line(&format!("}} {};", v.name));
                            sum_schema.insert(v.name.clone(), field_types);
                        }
                        SumVariantKind::Record(fields) => {
                            let mut field_types = Vec::new();
                            self.line("struct {");
                            self.indent += 1;
                            for f in fields {
                                let tc = self.type_ref_to_c(&f.ty)?;
                                field_types.push(tc.clone());
                                let mf = Self::mangle_field_name(&f.name);
                                self.line(&format!("{} {};", tc, mf));
                                let schema_base = mangled.strip_prefix("Nova_").unwrap_or(mangled);
                                let key = format!("{}::{}::{}", schema_base, v.name, f.name);
                                self.record_variant_field_types.insert(key, tc);
                            }
                            let schema_base = mangled.strip_prefix("Nova_").unwrap_or(mangled);
                            let order_key = format!("{}::{}", schema_base, v.name);
                            let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                            self.record_variant_field_order.insert(order_key, field_names);
                            self.indent -= 1;
                            self.line(&format!("}} {};", v.name));
                            sum_schema.insert(v.name.clone(), field_types);
                        }
                    }
                }
                self.indent -= 1;
                self.line("} payload;");
                self.indent -= 1;
                self.line("};");
                self.line("");

                // Constructor functions for each variant
                let mangled_clone = mangled.to_string();
                for v in &variants {
                    let field_types = sum_schema.get(&v.name).cloned().unwrap_or_default();
                    let params: String = field_types.iter().enumerate()
                        .map(|(i, t)| format!("{} _{}", t, i))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let params_str = if params.is_empty() { "void".to_string() } else { params };
                    self.line(&format!(
                        "static {name}* nova_make_{name}_{var}({params}) {{",
                        name = mangled_clone, var = v.name, params = params_str
                    ));
                    self.indent += 1;
                    self.line(&format!(
                        "{name}* _r = ({name}*)nova_alloc(sizeof({name}));",
                        name = mangled_clone
                    ));
                    self.line(&format!("_r->tag = NOVA_TAG_{name}_{var};",
                        name = mangled_clone, var = v.name));
                    match &v.kind {
                        SumVariantKind::Unit => {}
                        SumVariantKind::Tuple(_) => {
                            for (i, _) in field_types.iter().enumerate() {
                                self.line(&format!("_r->payload.{var}._{i} = _{i};", var = v.name, i = i));
                            }
                        }
                        SumVariantKind::Record(fields) => {
                            for (i, f) in fields.iter().enumerate() {
                                let mf = Self::mangle_field_name(&f.name);
                                self.line(&format!("_r->payload.{var}.{fname} = _{i};",
                                    var = v.name, fname = mf, i = i));
                            }
                        }
                    }
                    self.line("return _r;");
                    self.indent -= 1;
                    self.line("}");
                    self.line("");
                }
                let sum_key = mangled.strip_prefix("Nova_").unwrap_or(mangled);
                self.sum_schemas.insert(sum_key.to_string(), sum_schema);
            }
            _ => { /* Protocol/effect/alias/newtype — not generic record/sum */ }
        }

        self.current_type_subst = saved_subst;
        Ok(())
    }

    fn emit_monomorphized_fn(
        &mut self,
        fn_decl: &crate::ast::FnDecl,
        type_subst: Vec<(String, String)>,
        mono_name: &str,
    ) -> Result<(), String> {
        use crate::ast::FnBody;
        // Set type substitution
        let saved_subst = std::mem::replace(
            &mut self.current_type_subst,
            type_subst.iter().cloned().collect(),
        );
        // Compute concrete param types
        let param_c_tys: Vec<String> = fn_decl.params.iter()
            .map(|p| self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into()))
            .collect();
        let ret_c = fn_decl.return_type.as_ref()
            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
            .unwrap_or_else(|| "nova_unit".into());
        let params_str = if fn_decl.params.is_empty() {
            "void".to_string()
        } else {
            fn_decl.params.iter().zip(&param_c_tys)
                .map(|(p, ty)| format!("{} {}", ty, p.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        // D109 Ф.7.7: pre-populate array_element_types for []K params where K is substituted
        // to a concrete pointer type. emit_for uses this to cast array elements correctly,
        // so `for it in items` gives `Nova_GrmPoint* it = (Nova_GrmPoint*)arr->data[i]`
        // instead of `nova_int it = arr->data[i]` when K = Nova_GrmPoint*.
        let mut added_array_elem_keys: Vec<String> = Vec::new();
        for param in &fn_decl.params {
            if let crate::ast::TypeRef::Array(inner, _) = &param.ty {
                if let crate::ast::TypeRef::Named { path, generics, .. } = inner.as_ref() {
                    if generics.is_empty() {
                        let tparam_name = path.join("_");
                        if let Some(concrete) = self.current_type_subst.get(&tparam_name).cloned() {
                            if concrete.ends_with('*') && concrete != "nova_int*" {
                                self.array_element_types.insert(param.name.clone(), concrete);
                                added_array_elem_keys.push(param.name.clone());
                            }
                        }
                    }
                }
            }
        }
        // Buffer body (same as emit_fn / emit_generic_fn_erased pattern)
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        self.line(&format!("static {} {}({}) {{", ret_c, mono_name, params_str));
        self.indent += 1;
        // Register params in var_types with concrete C types
        let saved_var_types: Vec<(String, Option<String>)> = fn_decl.params.iter()
            .zip(&param_c_tys)
            .map(|(p, ty)| (p.name.clone(), self.var_types.insert(p.name.clone(), ty.clone())))
            .collect();
        // Register function-typed params in fn_param_sigs with concrete types
        let mut saved_fn_sigs: Vec<(String, Option<(Vec<String>, String)>)> = Vec::new();
        for (p, _c_ty) in fn_decl.params.iter().zip(&param_c_tys) {
            if let crate::ast::TypeRef::Func { params: fp, return_type, .. } = &p.ty {
                let inner_ptys: Vec<String> = fp.iter()
                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                    .collect();
                let inner_ret = return_type.as_ref()
                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
                    .unwrap_or_else(|| "nova_unit".into());
                let prev = self.fn_param_sigs.insert(p.name.clone(), (inner_ptys, inner_ret));
                saved_fn_sigs.push((p.name.clone(), prev));
            }
        }
        // Set receiver type (None for free fns)
        let saved_recv = self.current_receiver_type.take();
        // Set return type
        let saved_ret_ty = std::mem::replace(&mut self.current_fn_return_ty, Some(ret_c.clone()));
        // Set expected_record_type from fn return type (for anonymous record literals)
        let saved_expected = std::mem::replace(
            &mut self.expected_record_type,
            fn_decl.return_type.as_ref().and_then(|t| {
                if let crate::ast::TypeRef::Named { path, generics, .. } = t {
                    if generics.is_empty() { Some(path.join("_")) } else { None }
                } else { None }
            }),
        );
        // Emit body
        let body_clone = fn_decl.body.clone();
        match &body_clone {
            FnBody::Expr(e) => {
                self.emit_source_annotation_for_expr(e);
                let val = self.emit_expr(e)?;
                if ret_c == "nova_unit" {
                    self.line(&format!("{};", val));
                    self.line("return NOVA_UNIT;");
                } else {
                    self.line(&format!("return {};", val));
                }
            }
            FnBody::Block(block) => {
                let block_id = self.enter_defer_scope(block, false);
                for stmt in &block.stmts {
                    self.emit_stmt(stmt)?;
                }
                if let Some(trailing) = &block.trailing {
                    self.emit_source_annotation_for_expr(trailing);
                    let trailing_ty = self.infer_expr_c_type(trailing);
                    let val = self.emit_expr(trailing)?;
                    self.leave_defer_scope(block_id);
                    if ret_c == "nova_unit" {
                        self.line(&format!("{};", val));
                        self.line("return NOVA_UNIT;");
                    } else if trailing_ty == "nova_unit" && ret_c != "nova_unit" {
                        self.line(&format!("{};", val));
                        self.line(&format!("return ({})0; /* unreachable */", ret_c));
                    } else {
                        self.line(&format!("return {};", val));
                    }
                } else {
                    self.leave_defer_scope(block_id);
                    self.line("return NOVA_UNIT;");
                }
            }
            FnBody::External => {}
        }
        // Restore
        for (name, prev) in saved_var_types {
            match prev {
                Some(old) => { self.var_types.insert(name, old); }
                None => { self.var_types.remove(&name); }
            }
        }
        for (name, prev) in saved_fn_sigs {
            match prev {
                Some(old) => { self.fn_param_sigs.insert(name, old); }
                None => { self.fn_param_sigs.remove(&name); }
            }
        }
        // D109 Ф.7.7: clean up array_element_types entries added for this call
        for key in added_array_elem_keys {
            self.array_element_types.remove(&key);
        }
        self.current_receiver_type = saved_recv;
        self.current_fn_return_ty = saved_ret_ty;
        self.expected_record_type = saved_expected;
        self.flush_boxed_vars();
        self.indent -= 1;
        self.line("}");
        self.line("");
        // Plan 47 pattern: flush lambda decls before body
        let fn_body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        if !self.lambda_forward_decls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_forward_decls));
        }
        if !self.lambda_impls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_impls));
        }
        self.out.push_str(&fn_body);
        // Restore type substitution
        self.current_type_subst = saved_subst;
        Ok(())
    }

    /// All type parameters map to void*. The body is emitted with type params erased.
    fn emit_generic_fn_erased(&mut self, f: &FnDecl) -> Result<(), String> {
        let mangled = self.mangle_fn(f);
        let type_params: HashSet<String> = f.generics.iter().map(|g| g.name.clone()).collect();
        // Build param types: bare T → void*, generic record T[U] → Nova_T*
        let param_c_tys: Vec<String> = f.params.iter().map(|p| {
            match &p.ty {
                TypeRef::Named { path, generics, .. } => {
                    let name = path.join("_");
                    if type_params.contains(&name) {
                        "void*".into()
                    } else if !generics.is_empty() && self.record_schemas.contains_key(&name) {
                        // Generic record type like Box[T] → Nova_Box*
                        format!("Nova_{}*", name)
                    } else {
                        "void*".into()
                    }
                }
                _ => "void*".into(),
            }
        }).collect();
        let params_str = if f.params.is_empty() {
            "void".to_string()
        } else {
            f.params.iter().zip(&param_c_tys).map(|(p, ty)| format!("{} {}", ty, p.name)).collect::<Vec<_>>().join(", ")
        };
        // Plan 47: буферизуем тело — spawn-ctx typedefs (lambda_forward_decls)
        // должны флашиться ПЕРЕД телом. Иначе `spawn` внутри generic-функции
        // (например stdlib `within`/`race`) → ctx-typedef оказывается ПОСЛЕ
        // использования → "undeclared NovaSpawnCtx_*". emit_fn/emit_test уже
        // делают так; emit_generic_fn_erased — нет (был баг).
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        self.line(&format!("static void* {}({}) {{", mangled, params_str));
        self.indent += 1;
        // Register params with their concrete (or erased) types
        let saved: Vec<(String, Option<String>)> = f.params.iter().zip(&param_c_tys)
            .map(|(p, ty)| (p.name.clone(), self.var_types.insert(p.name.clone(), ty.clone())))
            .collect();
        // Emit body with type erasure — the first param is returned for identity-like fns
        let emit_erased_return = |this: &mut Self, val: &str, val_ty: &str| {
            // For struct types: heap-allocate and return pointer
            // For scalar/pointer types: cast via intptr_t
            if val_ty.starts_with("_NovaTuple") || val_ty.starts_with("Nova_")
               || val_ty == "nova_str" || val_ty.starts_with("NovaOpt_")
            {
                let heap_tmp = this.fresh_tmp();
                this.line(&format!("{ty}* {tmp} = ({ty}*)nova_alloc(sizeof({ty}));",
                    ty = val_ty, tmp = heap_tmp));
                this.line(&format!("*{} = {};", heap_tmp, val));
                this.line(&format!("return (void*){};", heap_tmp));
            } else {
                this.line(&format!("return (void*)(intptr_t)({});", val));
            }
        };
        match &f.body {
            FnBody::Expr(e) => {
                self.emit_source_annotation_for_expr(e);
                let val_ty = self.infer_expr_c_type(e);
                let val = self.emit_expr(e)?;
                emit_erased_return(self, &val, &val_ty);
            }
            FnBody::Block(block) => {
                // Plan 20 Ф.8 follow-up: defer scope для generic-erased fn body.
                // Без этого defer/errdefer внутри generic fn panic'ит codegen
                // ("defer/errdefer outside defer scope").
                let block_id = self.enter_defer_scope(block, false);
                for stmt in &block.stmts {
                    self.emit_stmt(stmt)?;
                }
                if let Some(trailing) = &block.trailing {
                    self.emit_source_annotation_for_expr(trailing);
                    let val_ty = self.infer_expr_c_type(trailing);
                    let val = self.emit_expr(trailing)?;
                    // Cleanup ДО return (defer body не должен влиять на val).
                    self.leave_defer_scope(block_id);
                    emit_erased_return(self, &val, &val_ty);
                } else {
                    self.leave_defer_scope(block_id);
                    self.line("return NULL;");
                }
            }
            // D82: external — wrapper-эмиттер не вызывается для external fn.
            FnBody::External => {}
        }
        // Restore param types
        for (name, prev) in saved {
            match prev {
                Some(old) => { self.var_types.insert(name, old); }
                None => { self.var_types.remove(&name); }
            }
        }
        self.flush_boxed_vars();
        self.indent -= 1;
        self.line("}");
        self.line("");
        // Plan 47: restore + flush spawn-ctx typedefs / lambda impls before body.
        let fn_body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        if !self.lambda_forward_decls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_forward_decls));
        }
        if !self.lambda_impls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_impls));
        }
        self.out.push_str(&fn_body);
        Ok(())
    }

    /// Plan 33.1 Ф.4 (D24): emit ensures-checks для функции `f`.
    /// Вызывается после вычисления body, до return'а. Перед вызовом
    /// `result` должна быть зарегистрирована в var_types и быть
    /// доступной как обычная C-переменная `_nova_result`.
    fn emit_ensures_checks(&mut self, f: &FnDecl) -> Result<(), String> {
        self.line("#ifdef NOVA_CONTRACTS_RUNTIME");
        // Подставляем `result` → `_nova_result` при emit'е выражения.
        // emit_expr на Ident("result") вернёт "result" (она в var_types),
        // нам нужно "_nova_result". Используем post-process подмену.
        for c in &f.contracts {
            if matches!(c.kind, ContractKind::Ensures) {
                // Plan 33.3 Ф.9.9: skip emit для proven контрактов.
                if self.proven_contracts.contains(&(f.name.clone(), c.span.start)) {
                    continue;
                }
                // D.1.3: квантор не может быть проверен в runtime — пропускаем.
                if matches!(c.expr.kind, ExprKind::Forall { .. } | ExprKind::Exists { .. }) {
                    continue;
                }
                let expr_c = self.emit_expr(&c.expr)?;
                // Простая подмена идентификатора `result` на `_nova_result`.
                // Работает для случаев без collision (что справедливо в 33.1).
                let expr_c_subst = Self::substitute_result_var(&expr_c);
                let expr_src = Self::expr_to_display(&c.expr);
                self.line(&format!(
                    "if (!({})) nova_contract_violation(NOVA_CONTRACT_POST, \"{}\", \"{}\", \"{}\", {});",
                    expr_c_subst, f.name, Self::escape_c_str(&expr_src),
                    "<contract>", c.span.start
                ));
            }
        }
        self.line("#endif");
        Ok(())
    }

    /// Простая текстовая подмена идентификатора `result` → `_nova_result`
    /// в C-коде. Используется при emit'е ensures-выражений. Работает
    /// потому что `result` — magic-имя, не может конфликтовать с
    /// пользовательскими (Ф.2 валидирует это).
    fn substitute_result_var(c: &str) -> String {
        // Word-boundary replace через простой parser.
        let mut out = String::with_capacity(c.len());
        let bytes = c.as_bytes();
        let target = b"result";
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            let is_word = b.is_ascii_alphanumeric() || b == b'_';
            // Если стартует с `result` и не word-continuation вокруг — заменяем.
            if i + target.len() <= bytes.len()
                && &bytes[i..i + target.len()] == target
                && (i == 0 || !(bytes[i-1].is_ascii_alphanumeric() || bytes[i-1] == b'_'))
                && (i + target.len() == bytes.len()
                    || !(bytes[i+target.len()].is_ascii_alphanumeric() || bytes[i+target.len()] == b'_'))
            {
                out.push_str("_nova_result");
                i += target.len();
                continue;
            }
            out.push(b as char);
            i += 1;
            let _ = is_word;
        }
        out
    }

    /// Plan 33.3 Ф.9.1: walks block для сбора `ghost let` имён.
    /// Используется в codegen чтобы runtime-check'и (assert_static/assume)
    /// читающие ghost-vars не emit'ились в C (ghost эрейзится).
    fn collect_ghost_vars_in_block(b: &Block, out: &mut std::collections::HashSet<String>) {
        for stmt in &b.stmts {
            if let Stmt::Let(decl) = stmt {
                if decl.is_ghost {
                    if let Pattern::Ident { name, .. } = &decl.pattern {
                        out.insert(name.clone());
                    }
                }
            }
        }
    }

    /// Plan 33.3 Ф.9.1: проверка, использует ли expr ghost-var.
    /// Используется для skip runtime check в codegen для spec-positions
    /// (assert_static/assume/loop invariants).
    fn expr_uses_ghost(e: &Expr, ghost_vars: &std::collections::HashSet<String>) -> bool {
        match &e.kind {
            ExprKind::Ident(n) => ghost_vars.contains(n),
            ExprKind::Binary { left, right, .. } => {
                Self::expr_uses_ghost(left, ghost_vars) || Self::expr_uses_ghost(right, ghost_vars)
            }
            ExprKind::Unary { operand, .. } => Self::expr_uses_ghost(operand, ghost_vars),
            ExprKind::Member { obj, .. } => Self::expr_uses_ghost(obj, ghost_vars),
            ExprKind::Index { obj, index } => {
                Self::expr_uses_ghost(obj, ghost_vars) || Self::expr_uses_ghost(index, ghost_vars)
            }
            ExprKind::Call { func, args, .. } => {
                if Self::expr_uses_ghost(func, ghost_vars) { return true; }
                args.iter().any(|a| Self::expr_uses_ghost(a.expr(), ghost_vars))
            }
            ExprKind::As(inner, _) | ExprKind::Is(inner, _)
            | ExprKind::Try(inner) | ExprKind::Bang(inner) => Self::expr_uses_ghost(inner, ghost_vars),
            ExprKind::Coalesce(l, r) => {
                Self::expr_uses_ghost(l, ghost_vars) || Self::expr_uses_ghost(r, ghost_vars)
            }
            _ => false,
        }
    }

    fn emit_fn(&mut self, f: &FnDecl) -> Result<(), String> {
        // D82: external fn — Nova body отсутствует, реализация в nova_rt/.
        // Skip emit'инг полностью: dispatch на C-функцию делается в emit_call.
        if f.is_external {
            return Ok(());
        }
        if f.name == "main" {
            return self.emit_nova_main(f);
        }
        // Plan 33.3 Ф.9.1: collect ghost-var names в body для runtime-check
        // skip в assert_static/assume/loop-invariant.
        self.ghost_vars.clear();
        if let FnBody::Block(b) = &f.body {
            Self::collect_ghost_vars_in_block(b, &mut self.ghost_vars);
        }
        // Plan 48: Generic free functions → monomorphized on demand; skip erased body.
        if !f.generics.is_empty() && f.receiver.is_none() {
            // FnDecl already stored in mono_fn_decls during forward-decl phase.
            return Ok(());
        }
        // Plan 48: Generic methods with own type params → monomorphized on demand; skip body.
        // Exception: array extension methods ([]T receivers) never get monomorphized — fall
        // through to regular emit. fn_param_sigs registration below erases unknown type params.
        if !f.generics.is_empty() && f.receiver.is_some() {
            let is_array_ext = f.receiver.as_ref().map_or(false, |r| r.type_name.starts_with("[]"));
            if !is_array_ext {
                // FnDecl stored in mono_method_decls during forward-decl phase.
                return Ok(());
            }
            // Fall through to regular emit path.
        }
        if let Some(recv) = &f.receiver {
            // Array extension methods ([]T, []str, etc.) are emitted directly — not erased.
            // They use a concrete NovaArray_nova_int receiver, not a generic struct.
            let is_array_ext = recv.type_name.starts_with("[]");
            if !recv.generics.is_empty() && !is_array_ext {
                if matches!(recv.kind, ReceiverKind::Static) {
                    // Static methods on generic types: emit minimal stub.
                    // Static constructors (new, from, with_capacity) are always
                    // called from concrete/monomorphized contexts. The erased body
                    // for complex methods generates invalid C (tuple destructuring,
                    // TurboFish calls to other generic methods with K/V unknowns).
                    return self.emit_generic_static_method_stub(f);
                }
                // Instance methods on generic types. Concrete-instance code
                // is emitted via drain_generic_type_worklist →
                // emit_generic_type_instance whenever the type is monomorphized
                // (Plan 48 Ф.3). The erased emit below remains as the V1
                // fallback for code paths the mono pipeline doesn't yet cover:
                // bare unit-variant references like `let r = Err2` where T
                // cannot be inferred from the constructor alone. Ф.7.4 is
                // therefore kept partial — full removal blocks on usage-context
                // inference for unit variants (tracked as V2 follow-up).
                return self.emit_generic_method_erased(f);
            }
        }
        // Set receiver type FIRST so Self resolves correctly in return_type_c/params_c
        if let Some(recv) = &f.receiver {
            self.current_receiver_type = Some(recv.type_name.clone());
        } else {
            self.current_receiver_type = None;
        }
        let ret = self.return_type_c(f)?;
        // Plan 55 Ф.4: save/restore current_fn_return_ty чтобы прошлый
        // ret не leak'ал при recursive emit (e.g. mono pass запускает
        // emit_fn для transitively'd dependencies из тела generic).
        let saved_ret_ty = std::mem::replace(
            &mut self.current_fn_return_ty,
            Some(ret.clone()),
        );
        let params = self.params_c(f)?;
        let mangled = self.mangle_fn(f);
        // Register param types in var_types for match/infer
        if let Some(recv) = &f.receiver {
            if matches!(recv.kind, ReceiverKind::Instance) {
                self.var_types.insert("nova_self".into(), self.receiver_c_type(&recv.type_name));
            }
        }
        for p in &f.params {
            if let Ok(ty_c) = self.type_ref_to_c(&p.ty) {
                self.var_types.insert(p.name.clone(), ty_c);
                // Register function-typed params so body() calls emit proper function pointer calls
                if let TypeRef::Func { params: fp, return_type, .. } = &p.ty {
                    // Erase unknown Nova pointer types (Nova_T*, Nova_U*, etc.) to nova_int.
                    // These appear in erased contexts like array extension methods (fn []T @map[U])
                    // where T and U are type params, not real Nova record/sum types.
                    let erase_unk = |c: String| -> String {
                        if let Some(inner) = c.strip_prefix("Nova_").and_then(|s| s.strip_suffix('*')) {
                            let name = inner.trim();
                            if !self.record_schemas.contains_key(name)
                                && !self.sum_schemas.contains_key(name)
                                && !self.generic_types.contains(name)
                            {
                                return "nova_int".to_string();
                            }
                        }
                        c
                    };
                    let param_c_tys: Vec<String> = fp.iter()
                        .map(|t| erase_unk(self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into())))
                        .collect();
                    let ret_c = match return_type {
                        Some(rt) => erase_unk(self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into())),
                        None => "nova_unit".into(),
                    };
                    self.fn_param_sigs.insert(p.name.clone(), (param_c_tys, ret_c));
                }
                // Register element type for array params of non-primitive types
                if let TypeRef::Array(inner, _) = &p.ty {
                    // Plan 55 Ф.1: `[]fn(P...) -> R` param → record element-closure
                    // signature so emit_for can register loop var in fn_param_sigs.
                    if let TypeRef::Func { params: fp, return_type, .. } = inner.as_ref() {
                        let inner_ptys: Vec<String> = fp.iter()
                            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                            .collect();
                        let inner_ret = return_type.as_ref()
                            .map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into()))
                            .unwrap_or_else(|| "nova_unit".into());
                        self.array_param_fn_sigs.insert(p.name.clone(), (inner_ptys, inner_ret));
                    } else if let Ok(elem_ty) = self.type_ref_to_c(inner) {
                        if elem_ty != "nova_int" && elem_ty != "nova_bool" && elem_ty != "nova_f64" && elem_ty != "nova_str" {
                            // Arrays store tuples and structs as heap pointers
                            let stored_ty = if elem_ty.starts_with("_NovaTuple") && !elem_ty.ends_with('*') {
                                format!("{}*", elem_ty)
                            } else {
                                elem_ty
                            };
                            self.array_element_types.insert(p.name.clone(), stored_ty);
                        }
                    }
                }
            }
        }
        // Buffer the function body so lambdas can be prepended before it
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        self.line(&format!("static {} {}({}) {{", ret, mangled, params));
        self.indent = 1;
        // Plan 44.7: preemption safepoint. First statement of every Nova
        // function — a TLS-flag check that cooperatively yields when the
        // M:N sysmon flagged this worker for an overrun. No-op (≈1 cycle,
        // predicted-not-taken) in single-thread mode where the flag is
        // never raised. Together with the loop-backedge check this gives
        // observable Go-style preemption: a CPU-bound fiber can't starve
        // its peers even with no explicit runtime.yield().
        self.line("nova_preempt_check();");
        let saved_expected = self.expected_record_type.clone();
        self.expected_record_type = Self::struct_name_from_c_type(&ret);
        // Plan 33.1 Ф.4 (D24): emit contracts.
        // Только в debug сборке (контролируется через NOVA_CONTRACTS_RUNTIME
        // macro). В release контракты со статусом @unverified / Default
        // в 33.1 (без SMT) — стираются. Для @must_verify в 33.1 без SMT
        // ошибки не выдаём (отложено до Ф.3); поведение runtime-fallback'а
        // на debug — стандартное.
        let has_contracts = !f.contracts.is_empty()
            && !matches!(f.verify_mode, VerifyMode::Unverified);
        // Plan 33.3 Ф.9.4 (D24): `decreases <expr>` для fn → recursion-depth
        // guard. Каждый entry в fn инкрементит thread-local counter; если
        // превышает порог (10000) — runtime panic. Это catches infinite
        // recursion в debug. Полный well-founded check (m_new < m_old) —
        // ждёт SMT (Z3 backend).
        let depth_var = if f.decreases.is_some() {
            // Sanitize fn name для C-identifier.
            let san: String = f.name.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let var = format!("_nova_decreases_depth_{}", san);
            // Declare thread-local counter BEFORE fn (на file-scope).
            // Делаем через separate preamble line — emit'им сюда не можем,
            // используем static local внутри fn (init=0, sticky между calls).
            self.line(&format!("static int {} = 0;", var));
            self.line("#ifdef NOVA_CONTRACTS_RUNTIME");
            self.line(&format!("if ({}++ > 1000000) nova_contract_violation(NOVA_CONTRACT_PRE, \"{}\", \"decreases recursion depth exceeded 1000000\", \"<decreases>\", {});",
                var, f.name, f.span.start));
            self.line("#endif");
            Some(var)
        } else {
            None
        };
        // emit requires checks
        if has_contracts {
            self.line("#ifdef NOVA_CONTRACTS_RUNTIME");
            for c in &f.contracts {
                if matches!(c.kind, ContractKind::Requires) {
                    // Plan 33.3 Ф.9.9: skip emit для proven контрактов
                    // (true zero-cost даже в debug). proven_contracts —
                    // set от VerificationPipeline. Key: (fn_name, span.start).
                    if self.proven_contracts.contains(&(f.name.clone(), c.span.start)) {
                        continue;
                    }
                    // D.1.3: квантор не может быть проверен в runtime — пропускаем.
                    if matches!(c.expr.kind, ExprKind::Forall { .. } | ExprKind::Exists { .. }) {
                        continue;
                    }
                    let expr_c = self.emit_expr(&c.expr)?;
                    let expr_src = Self::expr_to_display(&c.expr);
                    self.line(&format!(
                        "if (!({})) nova_contract_violation(NOVA_CONTRACT_PRE, \"{}\", \"{}\", \"{}\", {});",
                        expr_c, f.name, Self::escape_c_str(&expr_src),
                        "<contract>", c.span.start
                    ));
                }
            }
            self.line("#endif");
        }
        // emit body — collect into _nova_result if ensures present
        let has_ensures = has_contracts && f.contracts.iter().any(|c| matches!(c.kind, ContractKind::Ensures));
        match &f.body {
            FnBody::Expr(e) => {
                self.emit_source_annotation_for_expr(e);
                let val = self.emit_expr(e)?;
                if ret == "nova_unit" {
                    self.line(&format!("{};", val));
                    if has_ensures {
                        self.emit_ensures_checks(f)?;
                    }
                    self.line("return NOVA_UNIT;");
                } else if has_ensures {
                    self.line(&format!("{} _nova_result = {};", ret, val));
                    // Register `result` as visible var inside ensures.
                    self.var_types.insert("result".into(), ret.clone());
                    self.emit_ensures_checks(f)?;
                    self.var_types.remove("result");
                    self.line("return _nova_result;");
                } else {
                    self.line(&format!("return {};", val));
                }
            }
            FnBody::Block(block) => {
                if has_ensures {
                    // Plan 33.1 Ф.4 (D24 production-grade): block-body +
                    // ensures. Перехватываем все return'ы через goto post-label,
                    // collect'им результат в `_nova_result`, потом ensures-checks
                    // + final return.
                    let post_label = format!("_nova_contract_post_{}", f.name);
                    // Объявляем _nova_result заранее. Для unit-return игнорируем
                    // value, но всё равно нужен label-target.
                    if ret != "nova_unit" {
                        self.line(&format!("{} _nova_result;", ret));
                    }
                    let saved_label = self.contracts_post_label.take();
                    self.contracts_post_label = Some(post_label.clone());
                    self.var_types.insert("result".into(), ret.clone());
                    self.emit_block_stmts(block, &ret)?;
                    // Post-label: ensures-checks + return.
                    // Эмитим label на 0-индентации (C label синтаксис).
                    let saved_indent = self.indent;
                    self.indent = 0;
                    self.line(&format!("{}:;", post_label));
                    self.indent = saved_indent;
                    self.emit_ensures_checks(f)?;
                    self.var_types.remove("result");
                    self.contracts_post_label = saved_label;
                    if ret == "nova_unit" {
                        self.line("return NOVA_UNIT;");
                    } else {
                        self.line("return _nova_result;");
                    }
                } else {
                    self.emit_block_stmts(block, &ret)?;
                }
            }
            // D82: external — этот path не должен вызываться для external fn,
            // т.к. emit_fn skip'ает их раньше. Safety-fallback.
            FnBody::External => {}
        }
        self.expected_record_type = saved_expected;
        // Plan 55 Ф.4: restore prior current_fn_return_ty (mono-pass leak fix).
        self.current_fn_return_ty = saved_ret_ty;
        // Undef any heap-promoted mut-captures so macros don't leak to sibling fns.
        self.flush_boxed_vars();
        self.indent = 0;
        self.line("}");
        self.line("");
        let fn_body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        // Flush any lambdas discovered during this function's emit
        if !self.lambda_forward_decls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_forward_decls));
        }
        if !self.lambda_impls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_impls));
        }
        self.out.push_str(&fn_body);
        Ok(())
    }

    fn emit_nova_main(&mut self, f: &FnDecl) -> Result<(), String> {
        // nova main() → stored separately, called from C main()
        // Buffer body so spawn-ctx typedefs (lambda_forward_decls) flush
        // BEFORE main's body — иначе typedef в out появится после своего
        // первого usage внутри тела main.
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        self.line("static nova_unit nova_fn_main_impl(void) {");
        self.indent = 1;
        match &f.body {
            FnBody::Expr(e) => {
                self.emit_source_annotation_for_expr(e);
                let val = self.emit_expr(e)?;
                self.line(&format!("{};", val));
                self.line("return NOVA_UNIT;");
            }
            FnBody::Block(block) => {
                self.emit_block_stmts(block, "nova_unit")?;
            }
            // D82: main() не может быть external. Safety-fallback.
            FnBody::External => {}
        }
        self.flush_boxed_vars();
        self.indent = 0;
        self.line("}");
        self.line("");
        let body = std::mem::replace(&mut self.out, saved_out);
        self.indent = saved_indent;
        // Flush forward decls for spawn-ctx typedefs / lambda forward decls
        // accumulated during main's body emit — they must precede the body.
        if !self.lambda_forward_decls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_forward_decls));
        }
        if !self.lambda_impls.is_empty() {
            self.out.push_str(&std::mem::take(&mut self.lambda_impls));
        }
        self.out.push_str(&body);
        Ok(())
    }

    fn emit_main_wrapper(&mut self, module: &Module) {
        let has_main = module.items.iter().any(|i| {
            if let Item::Fn(f) = i { f.name == "main" } else { false }
        });
        let tests: Vec<&TestDecl> = module.items.iter().filter_map(|i| {
            if let Item::Test(t) = i { Some(t) } else { None }
        }).collect();

        if !tests.is_empty() && !has_main {
            // Generate a test runner as nova_fn_main_impl + C main
            self.line("static nova_unit nova_fn_main_impl(void) {");
            self.indent += 1;
            self.line(&format!("int _nova_tests_total = {};", tests.len()));
            self.line("int _nova_tests_failed = 0;");
            self.line("printf(\"Running %d tests...\\n\", _nova_tests_total);");
            for (idx, t) in tests.iter().enumerate() {
                let safe = Self::mangle_test_name_indexed(&t.name, idx);
                let escaped = Self::escape_c_str(&t.name);
                self.line("{");
                self.indent += 1;
                self.line("NovaTestFrame _tf;");
                self.line("_tf.fail_msg = NULL;");
                self.line("_nova_test_frame = &_tf;");
                /* Push a fail-frame too: assertion failures inside a fiber are
                 * routed to the nearest NovaFailFrame (so longjmp stays on the
                 * fiber's own stack); supervised_run re-throws on main flow via
                 * nova_throw. Without a top-level fail-frame here, that re-throw
                 * would abort. We catch it via _tf_fail and report as test
                 * failure. _tf still catches plain main-flow asserts. */
                self.line("NovaFailFrame _tf_fail;");
                self.line("_tf_fail.error_msg = (nova_str){.ptr=NULL, .len=0};");
                self.line("nova_fail_push(&_tf_fail);");
                self.line("int _tf_jmp = setjmp(_tf.jmp);");
                self.line("int _tf_fail_jmp = (_tf_jmp == 0) ? setjmp(_tf_fail.jmp) : 0;");
                self.line("if (_tf_jmp == 0 && _tf_fail_jmp == 0) {");
                self.indent += 1;
                self.line(&format!("nova_test_{}();", safe));
                self.line(&format!("printf(\"  PASS: {}\\n\");", escaped));
                self.indent -= 1;
                self.line("} else {");
                self.indent += 1;
                self.line("const char* _tf_msg = _tf.fail_msg ? _tf.fail_msg : (_tf_fail.error_msg.ptr ? _tf_fail.error_msg.ptr : \"assertion failed\");");
                self.line(&format!("printf(\"  FAIL: {} — %s\\n\", _tf_msg);", escaped));
                self.line("_nova_tests_failed++;");
                self.indent -= 1;
                self.line("}");
                self.line("nova_fail_pop();");
                self.line("_nova_test_frame = NULL;");
                self.indent -= 1;
                self.line("}");
            }
            self.line("printf(\"%d/%d passed\\n\", _nova_tests_total - _nova_tests_failed, _nova_tests_total);");
            self.line("if (_nova_tests_failed > 0) { exit(1); }");
            self.line("return NOVA_UNIT;");
            self.indent -= 1;
            self.line("}");
            self.line("");
        }

        self.line("int main(void) {");
        self.indent += 1;
        self.line("nova_gc_init();");
        // Plan 22 Ф.2: глобальный event loop. Под NOVA_USE_LIBUV даёт
        // настоящий uv_default_loop, иначе — stub no-op. Idempotent.
        self.line("nova_evloop_init();");
        // Per-fiber handler scoping: register all built-in effect-storage
        // addresses so nova_supervised_step может save/restore их в
        // per-fiber snapshot. Без этой регистрации fiber-snapshot был бы
        // пустой и handlers утекали бы между fibers.
        self.line("nova_register_effect_storage((void**)&_nova_handler_Fail);");
        self.line("nova_register_effect_storage((void**)&_nova_handler_Time);");
        // User-defined effects регистрируются здесь же — codegen эмитит
        // дополнительные nova_register_effect_storage(...) для каждого
        // объявленного `type X effect { }`.
        self.emit_user_effect_registrations();
        // Plan 22 Ф.5 (D92): implicit main-scope. Top-level main теперь
        // имеет supervised-like scope для detach'ей, pending timer'ов и
        // background fiber'ов. Они доработают до quiescence перед exit.
        // _nova_active_slot = -1 означает "main-flow, не fiber".
        self.line("NovaFiberQueue _nova_main_scope; nova_scope_init(&_nova_main_scope);");
        self.line("_nova_active_scope = &_nova_main_scope;");
        self.line("_nova_active_slot  = -1;");
        // Plan 22 Ф.10 + F2: SIGINT handler — Ctrl+C → cancel main-scope →
        // graceful shutdown. libuv mandatory (Plan 22 F2), без #ifdef.
        self.line("nova_evloop_install_sigint(&_nova_main_scope);");
        self.line("nova_fn_main_impl();");
        // D92: drain implicit main-scope до quiescence перед exit.
        // Detach'ы / pending fiber'ы пробуждённые callback'ами после
        // main-body доработают. Не используем nova_supervised_run потому
        // что он re-throws fiber-errors на main-flow (которого уже нет),
        // вызывая abort. Используем drain-no-throw variant — fiber-throw'ы
        // в detach'ах logged but не abort'ят процесс (D50 fire-and-forget).
        self.line("nova_supervised_drain_main_scope(&_nova_main_scope);");
        self.line("_nova_active_scope = NULL;");
        self.line("_nova_active_slot  = -1;");
        // Plan 22 Ф.2: graceful shutdown event loop'а перед GC shutdown.
        // Закрывает active handles, drain pending callbacks. Под stub'ом —
        // no-op.
        self.line("nova_evloop_close();");
        self.line("nova_gc_shutdown();");
        self.line("return 0;");
        self.indent -= 1;
        self.line("}");
    }

    /// Register handler-storage TLS addresses for all user-defined effects
    /// so per-fiber snapshot mechanism (effects.h) can swap them.
    fn emit_user_effect_registrations(&mut self) {
        // effect_schemas содержит и built-in (Fail, Time, Mem) и user-defined.
        // Built-in уже регистрируются явно в emit_main_wrapper. Для user-defined
        // эмитим nova_register_effect_storage для каждого `_nova_handler_X`.
        let mut names: Vec<String> = self.effect_schemas.keys().cloned().collect();
        names.sort();  // deterministic order
        for name in names {
            // Skip built-ins (зарегистрированы явно).
            if name == "Fail" || name == "Time" || name == "Mem" { continue; }
            self.line(&format!(
                "nova_register_effect_storage((void**)&_nova_handler_{});", name));
        }
    }

    // ---- block / statements ----

    // ---- Plan 20 Ф.4: defer/errdefer codegen helpers ----

    /// Scan a block for any `defer`/`errdefer` stmts (non-recursive — defers
    /// in nested blocks have their own scope). Used to decide whether to set
    /// up defer-state for this block at all (fast-path otherwise).
    fn block_has_defers(block: &Block) -> (bool, bool) {
        let mut has_defer = false;
        let mut has_errdefer = false;
        for s in &block.stmts {
            match s {
                Stmt::Defer { .. } => has_defer = true,
                Stmt::ErrDefer { .. } => { has_defer = true; has_errdefer = true; }
                _ => {}
            }
        }
        (has_defer, has_errdefer)
    }

    /// Push a new defer scope onto the stack and emit its prologue:
    /// declaration of activation flags (zero-init), and the NovaFailFrame
    /// setjmp wrapper for errdefer-bearing blocks. Returns block_id.
    fn enter_defer_scope(&mut self, block: &Block, is_loop_body: bool) -> usize {
        let (has_defer, has_errdefer) = Self::block_has_defers(block);
        if !has_defer {
            return 0;
        }
        self.defer_block_counter += 1;
        let block_id = self.defer_block_counter;
        let mut entries: Vec<DeferEntry> = Vec::new();
        let mut idx = 0usize;
        for s in &block.stmts {
            if let Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } = s {
                let is_err = matches!(s, Stmt::ErrDefer { .. });
                let var = format!("_defer_{}_{}_active", block_id, idx);
                entries.push(DeferEntry {
                    active_var: var.clone(),
                    is_errdefer: is_err,
                    body: body.clone(),
                });
                self.line(&format!("int {} = 0;", var));
                idx += 1;
            }
        }
        let failframe_var = format!("_defer_{}_ff", block_id);
        let failframe_popped_var = format!("_defer_{}_ff_popped", block_id);
        // Plan 20 Ф.8 follow-up (3): fail-frame нужен ВСЕГДА когда есть
        // defer (любой), не только когда есть errdefer. Spec D90 п.8:
        // defer fires on throw. Без local fail-frame'а throw скипает
        // scope с longjmp'ом, defer cleanup пропускается.
        // На fail-path: invoke ALL defers (LIFO; defer + errdefer оба
        // fire on error exit), pop fail-frame, re-throw outer.
        let _has_errdefer = has_errdefer; // suppress unused warning
        self.line(&format!("int {} = 0;", failframe_popped_var));
        self.line(&format!("NovaFailFrame {};", failframe_var));
        self.line(&format!("nova_fail_push(&{});", failframe_var));
        self.line(&format!("if (setjmp({}.jmp) != 0) {{", failframe_var));
        self.indent += 1;
        for entry in entries.iter().rev() {
            self.line(&format!("if ({}) {{", entry.active_var));
            self.indent += 1;
            let _ = self.emit_defer_body_void(&entry.body);
            self.indent -= 1;
            self.line("}");
        }
        self.line("nova_fail_pop();");
        self.line(&format!("{} = 1;", failframe_popped_var));
        self.line(&format!("nova_throw({}.error_msg);", failframe_var));
        self.indent -= 1;
        self.line("}");
        // Plan 20 Ф.8 (2): interrupt-path cleanup для `defer`.
        // По D90 п.8 `defer` запускается на ВСЕХ exit'ах, включая
        // `interrupt v` (когда outer handler делает interrupt → longjmp
        // на NovaInterruptFrame, минуя fail-frame).
        // Эмитим local interrupt-frame setjmp wrapper, который перехватывает
        // interrupt longjmp, запускает `defer` cleanup (НЕ errdefer — это
        // handled exit), pop'ает interrupt-frame и re-interrupt'ит с тем
        // же value, чтобы outer interrupt-frame получил value.
        let intframe_var = format!("_defer_{}_if", block_id);
        let intframe_popped_var = format!("_defer_{}_if_popped", block_id);
        self.line(&format!("int {} = 0;", intframe_popped_var));
        self.line(&format!("NovaInterruptFrame {};", intframe_var));
        self.line(&format!("nova_interrupt_push(&{});", intframe_var));
        self.line(&format!("if (setjmp({}.jmp) != 0) {{", intframe_var));
        self.indent += 1;
        // Interrupt path: invoke only `defer` (skip `errdefer` — handled exit).
        for entry in entries.iter().rev() {
            if entry.is_errdefer { continue; }
            self.line(&format!("if ({}) {{", entry.active_var));
            self.indent += 1;
            let _ = self.emit_defer_body_void(&entry.body);
            self.indent -= 1;
            self.line("}");
        }
        self.line("nova_interrupt_pop();");
        self.line(&format!("{} = 1;", intframe_popped_var));
        // Re-interrupt с тем же value через nova_interrupt — find outer
        // interrupt frame and longjmp туда с captured value.
        // Plan 39 Issue A: defer-scope не знает category outer with-блока,
        // поэтому re-issue ОБА slot'а: outer frame прочитает нужный по
        // своей category. Сохраняем оба значения, выбираем по тому что
        // непустое: если value_ptr != NULL — pointer-route, иначе int.
        self.line(&format!(
            "if ({}.value_ptr) {{ nova_interrupt_ptr({}.value_ptr); }} else {{ nova_interrupt({}.value); }}",
            intframe_var, intframe_var, intframe_var));
        self.indent -= 1;
        self.line("}");
        self.defer_scopes.push(DeferScope {
            block_id,
            entries,
            next_idx: 0,
            needs_failframe: has_errdefer,
            failframe_var,
            failframe_popped_var,
            intframe_var,
            intframe_popped_var,
            is_loop_body,
        });
        block_id
    }

    /// Emit cleanup for the current top defer scope (normal-exit path):
    /// invokes each entry's body in LIFO. Skips `errdefer` entries since
    /// is_error=0 on normal exit. Pops fail-frame if present, and pops
    /// the scope from the stack.
    fn leave_defer_scope(&mut self, block_id: usize) {
        if block_id == 0 {
            return;
        }
        let scope = self.defer_scopes.pop().expect("defer_scopes balanced");
        debug_assert_eq!(scope.block_id, block_id);
        // Entries: emit `if (active) { body; active = 0; }` — `= 0` чтобы
        // долгий-jump throw-handler (если он сработает после этой точки)
        // не вызвал defer повторно. Само-by-this-point throw уже не может
        // случиться внутри этого block scope (мы покидаем его), но defer
        // body выше могло иметь nested setjmp/longjmp.
        for entry in scope.entries.iter().rev() {
            if entry.is_errdefer {
                continue;
            }
            self.line(&format!("if ({}) {{", entry.active_var));
            self.indent += 1;
            let _ = self.emit_defer_body_void(&entry.body);
            self.line(&format!("{} = 0;", entry.active_var));
            self.indent -= 1;
            self.line("}");
        }
        // Fail-frame теперь всегда push'нут (Plan 20 Ф.8 follow-up).
        // Skip повторный pop, если early-exit cleanup уже сделал pop.
        self.line(&format!("if (!{}) {{ nova_fail_pop(); }}", scope.failframe_popped_var));
        // Plan 20 Ф.8 (2): pop interrupt-frame (всегда push'нут когда has_defer).
        self.line(&format!("if (!{}) {{ nova_interrupt_pop(); }}", scope.intframe_popped_var));
    }

    /// Emit defer-cleanup for an early exit (return/break/continue) walking
    /// scopes from innermost outward. `stop_at_loop` means walk only inner
    /// scopes up to (but not including) the first loop-body scope — used by
    /// break/continue. `stop_at_loop=false` means walk ALL scopes — used by
    /// return.
    /// Emit defer-cleanup for an early exit:
    ///   - return: walk ALL scopes (fn-level exit; ALL leave_defer_scope's
    ///     remaining cleanup will NOT run, so pop fail-frames manually).
    ///   - break/continue: walk ONLY the innermost loop-body scope (the C
    ///     `break`/`continue` exits one loop level — outer scopes remain
    ///     active and clean themselves up later via their own leave_defer_scope).
    ///
    /// In both cases we DEACTIVATE the defer flag (`= 0`) so that the eventual
    /// leave_defer_scope or fail-frame longjmp handler doesn't re-invoke.
    fn emit_early_exit_cleanup(&mut self, stop_at_loop: bool) {
        // Plan cleanup без clone(): сначала вытаскиваем scopes из
        // self.defer_scopes (mem::take заменяет на пустой Vec, освобождая
        // borrow), iterate over них, emit, потом возвращаем обратно.
        // Это позволяет вызывать &mut-методы (self.line, emit_defer_body_void)
        // внутри loop'а без borrow conflict.
        let scopes = std::mem::take(&mut self.defer_scopes);
        'outer: for scope in scopes.iter().rev() {
            for entry in scope.entries.iter().rev() {
                if entry.is_errdefer {
                    continue;
                }
                self.line(&format!("if ({}) {{", entry.active_var));
                self.indent += 1;
                let _ = self.emit_defer_body_void(&entry.body);
                self.line(&format!("{} = 0;", entry.active_var));
                self.indent -= 1;
                self.line("}");
            }
            // For return: pop fail-frames as we go (control will never reach
            // leave_defer_scope of these scopes again). For break/continue:
            // ONLY the innermost loop scope gets the pop (we walk just one).
            // Outer scopes remain active, will pop normally at their own
            // leave_defer_scope.
            // Fail-frame теперь всегда push'нут (Ф.8 follow-up).
            self.line("nova_fail_pop();");
            self.line(&format!("{} = 1;", scope.failframe_popped_var));
            // Plan 20 Ф.8 (2): pop interrupt-frame для early-exit тоже.
            self.line("nova_interrupt_pop();");
            self.line(&format!("{} = 1;", scope.intframe_popped_var));
            if stop_at_loop && scope.is_loop_body {
                break 'outer;
            }
        }
        // Восстанавливаем scopes — early-exit cleanup НЕ pop'ает scopes из
        // стека (это разные операции; pop scope происходит только в
        // leave_defer_scope).
        self.defer_scopes = scopes;
    }

    /// Emit a loop-body block (for/while/loop): integrates defer-scope around
    /// the body so defer/errdefer на каждой итерации correctly runs (LIFO,
    /// throw-path through NovaFailFrame). is_loop_body=true: break/continue
    /// в нашей собственной body — local, не пересекают loop boundary.
    fn emit_loop_body_inline(&mut self, body: &Block) -> Result<(), String> {
        let block_id = self.enter_defer_scope(body, true);
        // Plan 44.7: preemption safepoint at the loop backedge. Emitted as
        // the first statement of the body so it runs at the start of every
        // iteration — this also covers the `continue` edge (continue jumps
        // to the condition, re-enters the body, hits this check). Without
        // it a tight arithmetic loop with no function calls (`while i < N
        // { i = i + 1 }`) would never reach a prologue safepoint and could
        // monopolise its worker. No-op in single-thread mode.
        self.line("nova_preempt_check();");
        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            let v = self.emit_expr(trailing)?;
            self.line(&format!("(void)({});", v));
        }
        self.leave_defer_scope(block_id);
        Ok(())
    }

    /// Emit a defer/errdefer body as void-effect statements (no result value,
    /// no return). Used to splice defer body code into the cleanup-cascade.
    /// The body itself was already verified in Ф.3 to be infallible (no Fail,
    /// no suspend, no top-level return/break/continue/throw), so we can emit
    /// raw stmts + trailing-as-void.
    fn emit_defer_body_void(&mut self, body: &Expr) -> Result<(), String> {
        match &body.kind {
            // Common case: parser wraps `defer { ... }` body in ExprKind::Block.
            ExprKind::Block(b) => {
                for stmt in &b.stmts {
                    self.emit_stmt(stmt)?;
                }
                if let Some(trailing) = &b.trailing {
                    let v = self.emit_expr(trailing)?;
                    self.line(&format!("(void)({});", v));
                }
            }
            // Fallback: treat body as a single expression.
            _ => {
                let v = self.emit_expr(body)?;
                self.line(&format!("(void)({});", v));
            }
        }
        Ok(())
    }

    fn emit_block_stmts(&mut self, block: &Block, ret_ty: &str) -> Result<(), String> {
        let block_id = self.enter_defer_scope(block, false);
        for stmt in &block.stmts {
            self.emit_stmt(stmt)?;
        }
        // Plan 33.1 Ф.4: при активных ensures (contracts_post_label установлен)
        // trailing expression идёт в `_nova_result` + goto, чтобы ensures-checks
        // отработали ПОСЛЕ body (как и для explicit `return X`).
        let post_label = self.contracts_post_label.clone();
        if let Some(trailing) = &block.trailing {
            self.emit_source_annotation_for_expr(trailing);
            let val = self.emit_expr(trailing)?;
            if let Some(label) = post_label {
                // Contracts mode: trailing → _nova_result; goto post.
                if ret_ty == "nova_unit" {
                    self.line(&format!("{};", val));
                } else {
                    self.line(&format!("_nova_result = {};", val));
                }
                self.leave_defer_scope(block_id);
                self.line(&format!("goto {};", label));
            } else if ret_ty == "nova_unit" {
                self.line(&format!("{};", val));
                self.leave_defer_scope(block_id);
                self.line("return NOVA_UNIT;");
            } else {
                // Stash result in a tmp so defer cleanup runs *before* the return.
                let tmp = self.fresh_tmp();
                self.line(&format!("{} {} = {};", ret_ty, tmp, val));
                self.leave_defer_scope(block_id);
                self.line(&format!("return {};", tmp));
            }
        } else if ret_ty == "nova_unit" {
            self.leave_defer_scope(block_id);
            if let Some(label) = post_label {
                self.line(&format!("goto {};", label));
            } else {
                self.line("return NOVA_UNIT;");
            }
        } else {
            self.leave_defer_scope(block_id);
        }
        Ok(())
    }

    fn emit_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        // Source annotation hook: if --annotate-source enabled, emit the
        // originating Nova source as a /* SRC: ... */ comment.
        self.emit_source_annotation_for_stmt(stmt);
        match stmt {
            Stmt::Let(decl) => {
                // Plan 33.3 Ф.9.1 (D24): ghost erasure.
                // `ghost let x = ...` НЕ emit'ится в C-output (паритет с
                // Verus/Dafny). Ghost — spec-only, no runtime effect.
                // Type-check ensures ghost vars не reads из non-ghost code
                // (TODO: enforce когда добавится ghost-flow checker).
                if decl.is_ghost {
                    return Ok(());
                }
                // Special case: tuple destructure  `let (a, b, c) = expr`
                if let Pattern::Tuple(pats, _) = &decl.pattern {
                    return self.emit_tuple_destructure(pats, &decl.value);
                }
                // Plan 53: record destructure  `let { tx, rx } = expr` /
                // `let Pair { tx, rx } = expr`. Делегирует биндинг
                // полей в существующий `pattern_bind_typed` (он умеет
                // plain-record case через record_schemas). Refutable
                // patterns (sum-variant в type_path) ловятся
                // type-checker'ом — codegen ассамит irrefutable.
                if let Pattern::Record { .. } = &decl.pattern {
                    return self.emit_record_destructure(decl);
                }
                // Infer type BEFORE emitting so record literals get the right type
                let binding = self.pattern_binding(&decl.pattern)?;
                let ty_c = if let Some(ty) = &decl.ty {
                    self.type_ref_to_c(ty)?
                } else {
                    self.infer_expr_c_type(&decl.value)
                };
                // target-type-aware emit: для typed-integer ty_c литералы внутри
                // Binary получают native-typed cast вместо ((nova_int)NLL).
                //
                // Plan 51 Ф.1: typed `let x T = { ... }` — anonymous record
                // literal без префикса берёт тип из аннотации (D55, mirror
                // `const`-handling). Гейт узкий: значение — **напрямую**
                // typeless record-литерал. Иначе expected_record_type не
                // трогаем, чтобы тип `x` не «протёк» во вложенные литералы
                // внутри других выражений (`let x T = foo({ ... })`).
                let direct_typeless_record = matches!(
                    &decl.value.kind,
                    ExprKind::RecordLit { type_name: None, .. });
                let saved_expected = self.expected_record_type.clone();
                if decl.ty.is_some() && direct_typeless_record {
                    self.expected_record_type = Self::struct_name_from_c_type(&ty_c);
                }
                let val = self.emit_expr_with_target_type(&decl.value, &ty_c)?;
                self.expected_record_type = saved_expected;
                // For pointer types: the emitted tmp expression already carries the type.
                // Just declare the binding with the right type.
                self.var_types.insert(binding.clone(), ty_c.clone());
                // Plan 49 Ф.6 P0 fix: track CancelToken[T] T для per-T `reason()`
                // un-box. Rebind ОЧИЩАЕТ предыдущую запись чтобы не утечь между
                // function/test bodies (cancel_token_t_map не scope'd).
                self.cancel_token_t_map.remove(&binding);
                if let Some(crate::ast::TypeRef::Named { path, generics, .. }) = &decl.ty {
                    if path.len() == 1 && path[0] == "CancelToken" {
                        if let Some(t_ref) = generics.first() {
                            if let Ok(t_c) = self.type_ref_to_c(t_ref) {
                                self.cancel_token_t_map.insert(binding.clone(), t_c);
                            }
                        }
                    }
                }
                // Track mutability so spawn-capture can decide copy-by-value vs by-ptr.
                if decl.mutable {
                    self.var_mutable.insert(binding.clone());
                } else {
                    self.var_mutable.remove(&binding);
                }
                // Propagate tuple element types so pair.0 can be correctly typed
                if let Some(elem_tys) = self.tuple_element_types.get(&val).cloned() {
                    self.tuple_element_types.insert(binding.clone(), elem_tys);
                }
                // Propagate array element type so xs[i].field can be correctly typed
                if let Some(arr_elem_ty) = self.array_element_types.get(&val).cloned() {
                    self.array_element_types.insert(binding.clone(), arr_elem_ty);
                }
                // Plan 55 Ф.1: track element closure-sig for local `[]fn(...) -> T` vars
                // so `for f in xs { f() }` and `xs.push(|| ...)` work in non-param contexts.
                if let Some(crate::ast::TypeRef::Array(inner, _)) = &decl.ty {
                    if let crate::ast::TypeRef::Func { params: fp, return_type, .. } = inner.as_ref() {
                        let ptys: Vec<String> = fp.iter()
                            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                            .collect();
                        let rty = return_type.as_ref()
                            .map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into()))
                            .unwrap_or_else(|| "nova_unit".into());
                        self.array_param_fn_sigs.insert(binding.clone(), (ptys, rty));
                        // Hint emit_array_lit when value is `[]` so storage uses void_p.
                        if matches!(decl.value.kind, ExprKind::ArrayLit(ref e) if e.is_empty()) {
                            // Already handled via emit_expr_with_target_type below if hint set.
                        }
                    }
                }
                // Special case: `let xs = s.bytes()` / `s.chars()` — set element type
                // explicitly, even though val is not a known variable.
                if let ExprKind::Call { func, .. } = &decl.value.kind {
                    // D38: turbofish прозрачен — смотрим под него.
                    let func = func.unwrap_turbofish();
                    if let ExprKind::Member { obj, name } = &func.kind {
                        if self.infer_expr_c_type(obj) == "nova_str" {
                            match name.as_str() {
                                "bytes" => {
                                    self.array_element_types
                                        .insert(binding.clone(), "nova_byte".into());
                                }
                                "chars" => {
                                    // codepoints stored as nova_int — нет специального
                                    // element-type; default из NovaArray_nova_int* подойдёт.
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // Consume pending Option inner type (set when boxing a struct for nova_make_Option_Some)
                if let Some(inner_ty) = self.pending_option_inner_type.take() {
                    self.option_inner_types.insert(binding.clone(), inner_ty);
                }
                self.line(&format!("{} {} = {};", ty_c, binding, val));
                // Plan 11 Ф.4: RHS — method value `obj.@method` или `Type.@method`.
                // Регистрируем binding в fn_param_sigs так чтобы `f(args)` работало.
                // Plan 11 Ф.5: `expr as fn(P...) -> R` — type annotation для disambig
                // overloaded method values. Берём signature из аннотации, а не из registry.
                let (mv_expr, type_anno_sig): (&Expr, Option<(Vec<String>, String)>) =
                    if let ExprKind::As(inner, ty) = &decl.value.kind {
                        if let TypeRef::Func { params: fp, return_type, .. } = ty {
                            let ptys: Vec<String> = fp.iter()
                                .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                                .collect();
                            let rty = return_type.as_ref()
                                .map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into()))
                                .unwrap_or_else(|| "nova_unit".into());
                            (inner.as_ref(), Some((ptys, rty)))
                        } else {
                            (&decl.value, None)
                        }
                    } else {
                        (&decl.value, None)
                    };
                if let ExprKind::Member { obj, name } = &mv_expr.kind {
                    if let Some(method_name) = name.strip_prefix('@') {
                        // Plan 11 Ф.5: type annotation override — берём sig из
                        // `as fn(...) -> R` если есть. Иначе — first overload.
                        if let Some(anno_sig) = type_anno_sig.clone() {
                            self.fn_param_sigs.insert(binding.clone(), anno_sig);
                        } else {
                            // Resolve receiver type.
                            let (type_name, is_unbound) = match &obj.kind {
                                ExprKind::Ident(n) => {
                                    let is_type = n.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
                                        || matches!(n.as_str(),
                                            "int" | "i8" | "i16" | "i32" | "i64"
                                            | "u8" | "u16" | "u32" | "u64"
                                            | "f32" | "f64" | "byte" | "bool" | "char" | "str");
                                    if is_type { (n.clone(), true) } else {
                                        let obj_ty = self.var_types.get(n).cloned().unwrap_or_default();
                                        let t = Self::nova_type_name_from_c(&obj_ty);
                                        (t, false)
                                    }
                                }
                                ExprKind::Path(parts) if parts.len() == 1 => (parts[0].clone(), true),
                                _ => {
                                    let obj_ty = self.infer_expr_c_type(obj);
                                    let t = Self::nova_type_name_from_c(&obj_ty);
                                    (t, false)
                                }
                            };
                            let key = (type_name.clone(), method_name.to_string());
                            if let Some(overloads) = self.method_overloads.get(&key).cloned() {
                                if let Some(sig) = overloads.first() {
                                    let recv_c_ty = match type_name.as_str() {
                                        "int" | "i64" => "nova_int".to_string(),
                                        "f64" => "nova_f64".to_string(),
                                        "f32" => "nova_f32".to_string(),
                                        "str" => "nova_str".to_string(),
                                        "char" => "nova_int".to_string(),
                                        "byte" => "nova_byte".to_string(),
                                        "bool" => "nova_bool".to_string(),
                                        _ => format!("Nova_{}*", type_name),
                                    };
                                    let param_tys: Vec<String> = if is_unbound {
                                        std::iter::once(recv_c_ty).chain(sig.param_c_types.iter().cloned()).collect()
                                    } else {
                                        sig.param_c_types.clone()
                                    };
                                    self.fn_param_sigs.insert(binding.clone(),
                                        (param_tys, sig.return_c_type.clone()));
                                }
                            }
                        }
                    }
                }
                // Plan 14 Ф.3: если RHS — Ident, ссылающийся на user fn
                // (`let f = inc`), регистрируем binding в fn_param_sigs
                // через user_fn_sigs. Тогда `f(x)` пойдёт через
                // NOVA_CLOS_CALL_* macro (так же как для lambda).
                if let ExprKind::Ident(rhs_name) = &decl.value.kind {
                    if !self.var_types.contains_key(rhs_name) {
                        if let Some(sig) = self.user_fn_sigs.get(rhs_name).cloned() {
                            self.fn_param_sigs.insert(binding.clone(), sig);
                        }
                    }
                }
                // If RHS is a lambda, register the binding in fn_param_sigs so inc(5) works
                if let ExprKind::Lambda { params, return_type, .. } = &decl.value.kind {
                    let param_c_tys: Vec<String> = params.iter().map(|p| {
                        if let Some(ty) = &p.ty { self.type_ref_to_c(ty).unwrap_or_else(|_| "nova_int".into()) }
                        else { "nova_int".into() }
                    }).collect();
                    // Infer return type: priority — let-annotation > lambda annotation > default.
                    let ret_c = if let Some(TypeRef::Func { return_type: rt, .. }) = decl.ty.as_ref() {
                        rt.as_ref().map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                            .unwrap_or_else(|| "nova_int".into())
                    } else if let Some(rt) = return_type {
                        // Plan 08 Ф.4 prerequisite: lambda с явной `-> T`-аннотацией.
                        self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into())
                    } else {
                        "nova_int".into()
                    };
                    self.fn_param_sigs.insert(binding.clone(), (param_c_tys, ret_c));
                }
                // Plan 19, C5: closure-light в let-биндинге. Параметры
                // untyped, типы выводятся из контекста. Bootstrap — без
                // глубокого inference: если есть `let f fn(...) -> R = |x|...`
                // аннотация, берём типы оттуда; иначе все params/ret
                // дефолтятся в nova_int. Это позволяет `let zero = || 0;
                // zero()` корректно резолвиться через NOVA_CLOS_CALL_*.
                if let ExprKind::ClosureLight { params, .. } = &decl.value.kind {
                    let arity = params.len();
                    let (param_c_tys, ret_c) = if let Some(TypeRef::Func { params: anno_params, return_type: anno_ret, .. }) = decl.ty.as_ref() {
                        let ptys: Vec<String> = anno_params.iter()
                            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                            .collect();
                        let rty = anno_ret.as_ref()
                            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                            .unwrap_or_else(|| "nova_int".into());
                        (ptys, rty)
                    } else {
                        // Без annotation: дефолт nova_int для arity и
                        // ret. Bidirectional inference из first-use —
                        // C6 фаза Plan 19; здесь — bootstrap fallback.
                        let ptys: Vec<String> = (0..arity).map(|_| "nova_int".to_string()).collect();
                        (ptys, "nova_int".to_string())
                    };
                    self.fn_param_sigs.insert(binding.clone(), (param_c_tys, ret_c));
                }
                // Plan 19, C5: closure-full в let-биндинге. Типы
                // параметров и return явные — берём из FnSigBody.
                if let ExprKind::ClosureFull(sb) = &decl.value.kind {
                    let param_c_tys: Vec<String> = sb.params.iter()
                        .map(|p| self.type_ref_to_c(&p.ty).unwrap_or_else(|_| "nova_int".into()))
                        .collect();
                    let ret_c = sb.return_type.as_ref()
                        .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                        .unwrap_or_else(|| "nova_unit".into());
                    self.fn_param_sigs.insert(binding.clone(), (param_c_tys, ret_c));
                }
                // If RHS is a call to a function that returns fn(...), propagate closure sig to binding
                if let ExprKind::Call { func, args, .. } = &decl.value.kind {
                    // D38: turbofish прозрачен — смотрим под него.
                    let func = func.unwrap_turbofish();
                    if let ExprKind::Ident(fname) = &func.kind {
                        if let Some(sig) = self.fn_returns_fn_sig.get(fname).cloned() {
                            self.fn_param_sigs.insert(binding.clone(), sig);
                        }
                        // If RHS is a call to a generic fn returning a tuple, infer element types from args
                        if let Some(&arity) = self.generic_fn_tuple_arity.get(fname.as_str()) {
                            let elem_tys: Vec<String> = args.iter().take(arity)
                                .map(|a| self.infer_expr_c_type(a.expr()))
                                .collect();
                            if elem_tys.len() == arity {
                                self.tuple_element_types.insert(binding.clone(), elem_tys);
                            }
                        }
                    }
                }
            }
            Stmt::Expr(e) => {
                let val = self.emit_expr(e)?;
                // Plan 55 Ф.3: nova_unit — struct{}; `_tmp;` invalid C для struct.
                // Cast в (void) даёт valid expression-statement и работает для всех типов.
                let ty = self.infer_expr_c_type(e);
                if ty == "nova_unit" || Self::is_struct_type(&ty) {
                    self.line(&format!("(void)({});", val));
                } else {
                    self.line(&format!("{};", val));
                }
            }
            Stmt::Assign { target, op, value, .. } => {
                // Special case: array element assignment where elements are stored
                // as pointer-stomped nova_int (e.g. @buckets[idx] = Occupied{...}).
                // emit_expr(target) returns a cast lvalue which is illegal in C.
                // Instead emit: raw_arr->data[idx] = (nova_int)(intptr_t)val.
                if *op == AssignOp::Assign {
                    if let ExprKind::Index { obj: arr_obj, index } = &target.kind {
                        let elem_ty = self.infer_expr_c_type(target);
                        if elem_ty.ends_with('*') && elem_ty != "nova_int*" {
                            let arr_c = self.emit_expr(arr_obj)?;
                            let idx_c = self.emit_expr(index)?;
                            let val = self.emit_expr(value)?;
                            self.line(&format!("{}->data[{}] = (nova_int)(intptr_t)({});", arr_c, idx_c, val));
                            return Ok(());
                        }
                    }
                }
                let tgt = self.emit_expr(target)?;
                let val = self.emit_expr(value)?;
                let op_str = match op {
                    AssignOp::Assign => "=",
                    AssignOp::Add    => "+=",
                    AssignOp::Sub    => "-=",
                    AssignOp::Mul    => "*=",
                    AssignOp::Div    => "/=",
                };
                self.line(&format!("{} {} {};", tgt, op_str, val));
            }
            Stmt::Return { value, .. } => {
                // Plan 33.1 Ф.4 (D24): если функция имеет ensures-контракты,
                // `return X` подменяется на `{ _nova_result = X; goto <label>; }`
                // чтобы ensures-checks работали для **всех** return-точек,
                // включая early-return в block-bodies.
                let post_label = self.contracts_post_label.clone();
                // Plan 20 Ф.4: emit defer cleanup for ALL outer scopes before
                // returning (return is functional-level exit — walks ALL).
                // If no defers active, this is a no-op.
                if let Some(v) = value {
                    let val = self.emit_expr(v)?;
                    let ret_ty = self.current_fn_return_ty.clone().unwrap_or_else(|| "nova_int".to_string());
                    if let Some(label) = post_label {
                        // Contracts mode: stash в _nova_result, defer cleanup,
                        // потом goto. Если defers пустой — просто assign + goto.
                        self.line(&format!("_nova_result = {};", val));
                        if !self.defer_scopes.is_empty() {
                            self.emit_early_exit_cleanup(/*stop_at_loop=*/false);
                        }
                        self.line(&format!("goto {};", label));
                    } else if self.defer_scopes.is_empty() {
                        self.line(&format!("return {};", val));
                    } else {
                        // Stash result in a tmp so defer bodies can't see
                        // it / mutate it.
                        let tmp = self.fresh_tmp();
                        self.line(&format!("{} {} = {};", ret_ty, tmp, val));
                        self.emit_early_exit_cleanup(/*stop_at_loop=*/false);
                        self.line(&format!("return {};", tmp));
                    }
                } else {
                    if let Some(label) = post_label {
                        // Contracts mode unit-return.
                        if !self.defer_scopes.is_empty() {
                            self.emit_early_exit_cleanup(/*stop_at_loop=*/false);
                        }
                        self.line(&format!("goto {};", label));
                    } else {
                        if !self.defer_scopes.is_empty() {
                            self.emit_early_exit_cleanup(/*stop_at_loop=*/false);
                        }
                        self.line("return NOVA_UNIT;");
                    }
                }
            }
            Stmt::Break(_) => {
                if !self.defer_scopes.is_empty() {
                    self.emit_early_exit_cleanup(/*stop_at_loop=*/true);
                }
                self.line("break;");
            }
            Stmt::Continue(_) => {
                if !self.defer_scopes.is_empty() {
                    self.emit_early_exit_cleanup(/*stop_at_loop=*/true);
                }
                self.line("continue;");
            }
            Stmt::Throw { value, .. } => {
                // `throw expr` desugars to `Fail.fail(expr)` — operation of the
                // built-in `Fail` effect (D25/D62/D65). Compiler dispatches via
                // _nova_handler_Fail. Default handler calls nova_throw, which
                // longjmp's to the nearest setjmp-frame (test_frame or spawn-
                // entry frame). User can install handler-lambda via
                // `with Fail = (msg) => ... { body }` (D31).
                let val_ty = self.infer_expr_c_type(value);
                let val = self.emit_expr(value)?;
                if val_ty == "nova_str" {
                    self.line(&format!("Nova_Fail_fail({});", val));
                } else {
                    self.line(&format!("Nova_Fail_fail(nova_int_to_str((nova_int)({})));", val));
                }
            }
            // D90 Plan 20 Ф.4: defer/errdefer codegen. Активация флага в
            // позиции defer/errdefer; cleanup инвоцируется в leave_defer_scope
            // (для normal-exit) и в setjmp-fail-handler (для throw-path).
            // enter_defer_scope уже декларировал `int _defer_BID_N_active = 0`.
            // Здесь — просто переключаем флаг и инкрементим next_idx.
            Stmt::Defer { .. } | Stmt::ErrDefer { .. } => {
                let scope = self.defer_scopes.last_mut()
                    .expect("defer/errdefer outside defer scope (enter_defer_scope missed?)");
                let idx = scope.next_idx;
                let var = scope.entries[idx].active_var.clone();
                scope.next_idx += 1;
                self.line(&format!("{} = 1;", var));
            }
            // Plan 33.2 Ф.8 (D24): `assert_static <expr>` — intermediate
            // proof obligation. Сейчас (без full SMT body-encoding)
            // эмитим как runtime check в debug. В release без
            // NOVA_CONTRACTS_RUNTIME — стирается препроцессором.
            Stmt::AssertStatic { expr, span } => {
                // Plan 33.3 Ф.9.1: skip runtime check если expr читает
                // ghost-var (ghost эрейзится в codegen; SMT-verify в Z3
                // будет работать). assert_static с ghost — pure spec-level.
                if Self::expr_uses_ghost(expr, &self.ghost_vars) {
                    // No-op в codegen — assert_static с ghost — spec-only.
                } else {
                    let v = self.emit_expr(expr)?;
                    let src = Self::expr_to_display(expr);
                    self.line("#ifdef NOVA_CONTRACTS_RUNTIME");
                    self.line(&format!(
                        "if (!({})) nova_contract_violation(NOVA_CONTRACT_PRE, \"<assert_static>\", \"{}\", \"<contract>\", {});",
                        v, Self::escape_c_str(&src), span.start
                    ));
                    self.line("#endif");
                }
            }
            // Plan 33.3 (D24): `assume <expr>` — runtime check в debug
            // (программист подтверждает что expr истинен; если нет —
            // это bug в коде, не bug в верификации).
            Stmt::Assume { expr, span } => {
                // Plan 33.3 Ф.9.1: skip если expr читает ghost-var.
                if Self::expr_uses_ghost(expr, &self.ghost_vars) {
                    // No-op в codegen — assume с ghost — spec-only.
                } else {
                    let v = self.emit_expr(expr)?;
                    let src = Self::expr_to_display(expr);
                    self.line("#ifdef NOVA_CONTRACTS_RUNTIME");
                    self.line(&format!(
                        "if (!({})) nova_contract_violation(NOVA_CONTRACT_PRE, \"<assume>\", \"{}\", \"<contract>\", {});",
                        v, Self::escape_c_str(&src), span.start
                    ));
                    self.line("#endif");
                }
            }
            // Plan 33.5 Ф.4.1: `apply lemma_name(args)` — ghost statement,
            // полностью стирается в codegen. SMT-семантика обрабатывается
            // в verify/pipeline.rs (assert lemma.ensures[args/params]).
            Stmt::Apply { .. } => {
                // Ghost erasure — никакого C-кода не эмитируем.
            }
            // Plan 33.5 Ф.4.2: `calc { ... }` — ghost statement, полностью
            // стирается в codegen. SMT-семантика в verify/pipeline.rs.
            Stmt::Calc { .. } => {
                // Ghost erasure.
            }
        }
        Ok(())
    }

    // ---- expressions ----

    /// Emit `expr` zная c-тип цели. Если target — typed-integer
    /// (uint8/16/32/64, int8/16/32), литералы (IntLit/CharLit) и операнды
    /// integer-арифметических Binary/Unary получают «нативный» суффикс/cast:
    /// `((uint32_t)NU)` вместо стандартного `((nova_int)NLL)`.
    ///
    /// Для не-typed-integer target или non-арифметического выражения —
    /// fallback в обычный `emit_expr`. Это **обёртка**, не замена.
    ///
    /// Используется в let-binding с известным `ty_c`, в `emit_block_into`
    /// (trailing — известен ty блока), и `emit_if_expr` для `else if`
    /// ветки (известен if_ty).
    fn emit_expr_with_target_type(&mut self, expr: &Expr, target_ty_c: &str) -> Result<String, String> {
        // Plan 48: `None` initializer should match target NovaOpt_X type when target is known.
        // Otherwise None falls back to NovaOpt_nova_int (per current_fn_return_ty), which
        // breaks `let mut result NovaOpt_nova_str = None` in mono'd generic bodies.
        if target_ty_c.starts_with("NovaOpt_") {
            if let ExprKind::Ident(name) = &expr.kind {
                if name == "None" {
                    return Ok(format!(
                        "(({}){{.tag = NOVA_TAG_Option_None}})", target_ty_c));
                }
            }
        }
        // When target is NovaArray_X* and expr is an array literal, set hint so
        // emit_array_lit uses X element type instead of defaulting to nova_int.
        // Handles empty `[]` in typed contexts: `let xs []str = []` or `{ items: [] }`.
        if let ExprKind::ArrayLit(_) = &expr.kind {
            if let Some(inner) = target_ty_c.strip_prefix("NovaArray_") {
                let hint_elem = inner.trim_end_matches('*').to_string();
                let prev = self.current_array_elem_hint.replace(hint_elem);
                let result = self.emit_expr(expr);
                self.current_array_elem_hint = prev;
                return result;
            }
        }
        // Только typed-integer target пропагируем — для nova_int/struct/etc.
        // обычный emit_expr уже корректен.
        if !Self::is_typed_integer(target_ty_c) {
            return self.emit_expr(expr);
        }
        match &expr.kind {
            ExprKind::IntLit(n) => Ok(Self::emit_typed_int_literal(*n, target_ty_c)),
            ExprKind::CharLit(cp) => Ok(Self::emit_typed_int_literal(*cp as i64, target_ty_c)),
            ExprKind::Unary { op: UnOp::Neg, operand } => {
                if let ExprKind::IntLit(n) = &operand.kind {
                    return Ok(Self::emit_typed_int_literal(-*n, target_ty_c));
                }
                // Иначе рекурсивно с тем же target.
                let inner = self.emit_expr_with_target_type(operand, target_ty_c)?;
                Ok(format!("(-{})", inner))
            }
            ExprKind::Binary { op, left, right } => {
                // Пропагируем target только для integer-арифметики/побитовых/сдвигов.
                // Сравнения (Eq/Neq/Lt/...) и logic (And/Or) — bool result; их operand'ы
                // могут быть разных типов и не должны получать typed-cast.
                let is_integer_arith = matches!(op,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
                    | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor
                    | BinOp::Shl | BinOp::Shr
                );
                if !is_integer_arith {
                    return self.emit_expr(expr);
                }
                // Если operand'ы — non-integer типы (например str + str), fallback.
                // Проверяем по infer_expr_c_type: если any side — не nova_int/typed-int/void*,
                // обычный emit_expr корректнее обработает special-cases.
                let lty = self.infer_expr_c_type(left);
                let rty = self.infer_expr_c_type(right);
                let lhs_ok = lty == "nova_int" || Self::is_typed_integer(&lty) || lty == "void*";
                let rhs_ok = rty == "nova_int" || Self::is_typed_integer(&rty) || rty == "void*";
                if !lhs_ok || !rhs_ok {
                    return self.emit_expr(expr);
                }
                let l = self.emit_expr_with_target_type(left, target_ty_c)?;
                let r = self.emit_expr_with_target_type(right, target_ty_c)?;
                let op_str = match op {
                    BinOp::Add => "+",  BinOp::Sub => "-",
                    BinOp::Mul => "*",  BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<", BinOp::Shr => ">>",
                    _ => unreachable!(),
                };
                Ok(format!("({} {} {})", l, op_str, r))
            }
            _ => self.emit_expr(expr),
        }
    }

    /// Plan 48 Ф.4 ([M-spawn-closure-capture-mono]): if `name` is captured by
    /// the current spawn-body, return the C expression to read it from the
    /// spawn ctx (`_c->name` or `(*_c->name)`). Returns `None` otherwise so
    /// the caller can use the bare identifier. Centralizes the rewrite so
    /// both Ident reads and indirect uses (closure-call callee, address-of
    /// for spawn nesting) stay consistent.
    fn spawn_capture_access(&self, name: &str) -> Option<String> {
        let caps = self.current_spawn_captures.as_ref()?;
        if !caps.contains(name) { return None; }
        let by_value = self.current_spawn_capture_by_value.as_ref()
            .map(|s| s.contains(name)).unwrap_or(false);
        Some(if by_value {
            format!("_c->{}", name)
        } else {
            format!("(*_c->{})", name)
        })
    }

    fn emit_expr(&mut self, expr: &Expr) -> Result<String, String> {
        match &expr.kind {
            ExprKind::IntLit(n)   => Ok(format!("((nova_int){}LL)", n)),
            ExprKind::CharLit(cp) => Ok(format!("((nova_int){}LL)", cp)),
            ExprKind::FloatLit(f) => {
                // f.to_string() для 1e20 даёт "100000000000000000000" (без точки/exp)
                // — это integer-литерал в C, переполняет u64. Принудительно
                // используем scientific notation, добавляем суффикс если нужен dot.
                let s = if f.is_finite() && (f.abs() >= 1e16 || (f.abs() != 0.0 && f.abs() < 1e-4)) {
                    format!("{:e}", f)  // scientific для очень больших/малых
                } else {
                    let raw = f.to_string();
                    if raw.contains('.') || raw.contains('e') || raw.contains('E') {
                        raw
                    } else {
                        format!("{}.0", raw)  // целые f64-литералы — добавим .0
                    }
                };
                Ok(format!("((nova_f64){})", s))
            }
            ExprKind::BoolLit(b)  => Ok(if *b { "true".into() } else { "false".into() }),
            ExprKind::UnitLit     => Ok("NOVA_UNIT".into()),
            ExprKind::StrLit(s)   => {
                let escaped = Self::escape_c_str(s);
                Ok(format!("(nova_str){{.ptr=\"{}\", .len={}}}", escaped, s.len()))
            }
            ExprKind::InterpolatedStr { parts } => {
                self.emit_interpolated_str(parts)
            }

            ExprKind::Ident(name) => {
                // Unit variants (e.g. `Red` from `type Color | Red | Green`) are
                // not function calls in Nova but need `nova_make_Color_Red()` in C.
                if let Some((type_name, fields)) = self.find_variant(name) {
                    if fields.is_empty() {
                        // Plan 14 Ф.1: `None` — typed compound literal по
                        // current_fn_return_ty. Иначе — legacy nova_make.
                        if name == "None" {
                            let opt_ty: String = self.current_fn_return_ty.as_ref()
                                .filter(|t| t.starts_with("NovaOpt_"))
                                .cloned()
                                .unwrap_or_else(|| "NovaOpt_nova_int".into());
                            return Ok(format!(
                                "(({}){{.tag = NOVA_TAG_Option_None}})", opt_ty));
                        }
                        return Ok(format!("nova_make_{}_{}()", type_name, name));
                    }
                }
                // Heap-promoted mut-capture: dereference the box pointer.
                // Avoids #define which corrupts struct field access (foo->name).
                if let Some(box_var) = self.var_boxed.get(name) {
                    return Ok(format!("(*{})", box_var));
                }
                // Capture access inside spawn-entry body. By-pointer → `(*_c->name)`,
                // by-value → `_c->name` (T field, no deref).
                // This avoids `#define name ...` macros, which would corrupt
                // nested supervised's struct field declarators with the same name.
                if let Some(caps) = &self.current_spawn_captures {
                    if caps.contains(name) {
                        let by_value = self.current_spawn_capture_by_value.as_ref()
                            .map(|s| s.contains(name)).unwrap_or(false);
                        return if by_value {
                            Ok(format!("_c->{}", name))
                        } else {
                            Ok(format!("(*_c->{})", name))
                        };
                    }
                }
                // Function-as-first-class-value: если `name` это user fn
                // (fn_ret_<name> в var_types), а не local variable
                // (просто <name> в var_types) — emit closure-value для
                // совместимости с HOF/closure-call mechanism (Plan 14 Ф.3).
                //
                // Раньше (pre-Ф.3): эмитили `nova_fn_<name>` (raw fn-ptr).
                // Это работало только если callee — direct typed C function;
                // для HOF (`xs.map(inc)`) или fn_param_sigs-driven call'ов
                // через NOVA_CLOS_CALL_* — нужен closure-struct {fn, env}.
                //
                // Стратегия:
                //   - Если sig fn известна (user_fn_sigs) — эмитим thunk +
                //     closure-литерал через `emit_free_fn_value`.
                //   - Иначе fallback'имся на raw fn-pointer (для случаев
                //     где callee принимает direct C function: built-in
                //     dispatch, generic erasure, и т.д.).
                // Plan 14 Ф.2: lazy const → вызов геттера. Проверяем
                // ПЕРВЫМ, до is_local_var, потому что emit_lazy_const
                // регистрирует name в var_types для type-inference, а
                // is_local_var тогда был бы true.
                if self.lazy_consts.contains(name) {
                    return Ok(format!("nova_const_{}()", name));
                }
                let is_local_var = self.var_types.contains_key(name);
                let is_user_fn = self.var_types.contains_key(&format!("fn_ret_{}", name));
                if is_user_fn && !is_local_var {
                    if let Some(closure_value) = self.emit_free_fn_value(name) {
                        return Ok(closure_value);
                    }
                    return Ok(format!("nova_fn_{}", name));
                }
                Ok(name.clone())
            }
            ExprKind::Path(parts) => {
                // Plan 38: numeric type constants — `int.MAX`, `f64.NAN`, etc.
                // Mapping table в `numeric_type_constant_mapping`.
                if let Some((c_expr, _)) = Self::numeric_type_constant_mapping(parts) {
                    return Ok(c_expr.to_string());
                }
                // D109: qualified unit variant constructor: `Type.Variant`.
                // In monomorphized context, Type may be a generic type (e.g. Slot[K,V]).
                if parts.len() == 2 {
                    let type_name_raw = &parts[0];
                    let variant_name = &parts[1];
                    let schema_key = if let Some(tmpl) = self.generic_type_templates.get(type_name_raw.as_str()) {
                        let type_args_c: Vec<String> = tmpl.generics.iter()
                            .filter_map(|g| self.current_type_subst.get(&g.name).cloned())
                            .collect();
                        if type_args_c.len() == tmpl.generics.len() {
                            let mangled = Self::compute_generic_type_c_name(type_name_raw, &type_args_c);
                            mangled.strip_prefix("Nova_").unwrap_or(&mangled).to_string()
                        } else {
                            type_name_raw.clone()
                        }
                    } else {
                        type_name_raw.clone()
                    };
                    // Effective schema_key: prefer computed key, fall back to type_name_raw
                    // (happens when type args are erased placeholders like Nova_K* not registered
                    // in sum_schemas, but the base erased type is).
                    let eff_key = if self.sum_schemas.contains_key(&schema_key) {
                        schema_key.clone()
                    } else if self.sum_schemas.contains_key(type_name_raw.as_str()) {
                        type_name_raw.clone()
                    } else {
                        String::new()
                    };
                    if !eff_key.is_empty() {
                        if let Some(variants) = self.sum_schemas.get(&eff_key) {
                            if let Some(fields) = variants.get(variant_name.as_str()) {
                                if fields.is_empty() {
                                    // Constructor naming:
                                    // - erased ("Slot"): nova_make_Slot_Empty — no Nova_ prefix
                                    // - monomorphized ("Slot____nova_str__nova_int"):
                                    //   nova_make_Nova_Slot____..._Empty — with Nova_ prefix
                                    let ctor_prefix = if eff_key == type_name_raw.as_str() {
                                        type_name_raw.clone()
                                    } else {
                                        format!("Nova_{}", eff_key)
                                    };
                                    return Ok(format!(
                                        "(nova_int)(intptr_t)nova_make_{}_{}()",
                                        ctor_prefix, variant_name
                                    ));
                                }
                            }
                        }
                    }
                }
                // Plan 14 Ф.2: `FACTOR.x` парсится как Path(["FACTOR", "x"])
                // если первая часть — Ident с UpperCase (parser routing).
                // Для lazy const'ов нужно `nova_const_FACTOR()->x` вместо
                // `FACTOR_x` (last segment — record-поле).
                if parts.len() >= 2 && self.lazy_consts.contains(&parts[0]) {
                    let const_ty = self.var_types.get(&parts[0]).cloned()
                        .unwrap_or_default();
                    let accessor = if Self::is_value_type(&const_ty) { "." } else { "->" };
                    let mut acc = format!("nova_const_{}(){}{}", parts[0], accessor, parts[1]);
                    for p in &parts[2..] {
                        acc = format!("({}.{})", acc, p);
                    }
                    return Ok(acc);
                }
                Ok(parts.join("_"))
            }

            // D38 turbofish: type_args — explicit hint для monomorphization;
            // bootstrap monomorphizes по call-site / receiver-type, поэтому
            // type_args не нужны на этом этапе. Делегируем в base.
            ExprKind::TurboFish { base, .. } => self.emit_expr(base),

            ExprKind::Binary { op, left, right } => {
                // Infer types before emitting (emit_expr may add temporaries)
                let lty = self.infer_expr_c_type(left);
                let rty = self.infer_expr_c_type(right);
                let l = self.emit_expr(left)?;
                let r = self.emit_expr(right)?;
                // If either operand is void* (erased generic or unknown stub), handle carefully:
                // - void* vs nova_int/nova_bool: cast void* back to the concrete type and compare
                // - void* vs nova_str: dereference void* as nova_str* and use str equality
                // - void* vs void* (both unknown): comparison is meaningless, emit 0
                if lty == "void*" || rty == "void*" {
                    let concrete_ty = if lty == "void*" { &rty } else { &lty };
                    let void_side = if lty == "void*" { &l } else { &r };
                    let concrete_side = if lty == "void*" { &r } else { &l };
                    if concrete_ty == "nova_str" {
                        return match op {
                            BinOp::Eq  => Ok(format!("(nova_str_eq(*(nova_str*)({}), {}))", void_side, concrete_side)),
                            BinOp::Neq => Ok(format!("(!nova_str_eq(*(nova_str*)({}), {}))", void_side, concrete_side)),
                            _ => Ok("(0)".into()),
                        };
                    } else if concrete_ty == "nova_int" || concrete_ty == "nova_bool" || concrete_ty == "nova_f64" {
                        return match op {
                            BinOp::Eq  => Ok(format!("((({ct})(intptr_t)({vs})) == ({cs}))", ct = concrete_ty, vs = void_side, cs = concrete_side)),
                            BinOp::Neq => Ok(format!("((({ct})(intptr_t)({vs})) != ({cs}))", ct = concrete_ty, vs = void_side, cs = concrete_side)),
                            BinOp::Lt  => Ok(format!("((({ct})(intptr_t)({vs})) < ({cs}))", ct = concrete_ty, vs = void_side, cs = concrete_side)),
                            BinOp::Gt  => Ok(format!("((({ct})(intptr_t)({vs})) > ({cs}))", ct = concrete_ty, vs = void_side, cs = concrete_side)),
                            BinOp::Le  => Ok(format!("((({ct})(intptr_t)({vs})) <= ({cs}))", ct = concrete_ty, vs = void_side, cs = concrete_side)),
                            BinOp::Ge  => Ok(format!("((({ct})(intptr_t)({vs})) >= ({cs}))", ct = concrete_ty, vs = void_side, cs = concrete_side)),
                            _ => Ok("(0)".into()),
                        };
                    } else {
                        // Both void* (erased T vs erased T) — compare via intptr_t.
                        // Works correctly for T=int: nova_int values stored in array->data[]
                        // are accessed as void* casts; intptr_t comparison preserves order.
                        return match op {
                            BinOp::Lt  => Ok(format!("(((intptr_t)({})) < ((intptr_t)({})))", l, r)),
                            BinOp::Le  => Ok(format!("(((intptr_t)({})) <= ((intptr_t)({})))", l, r)),
                            BinOp::Gt  => Ok(format!("(((intptr_t)({})) > ((intptr_t)({})))", l, r)),
                            BinOp::Ge  => Ok(format!("(((intptr_t)({})) >= ((intptr_t)({})))", l, r)),
                            BinOp::Eq  => Ok(format!("(({}) == ({}))", l, r)),
                            BinOp::Neq => Ok(format!("(({}) != ({}))", l, r)),
                            _ => Ok("(0)".into()),
                        };
                    }
                }
                // Plan 13 Ф.9.2: оператор `+` через метод @plus (D46).
                // StringBuilder + str  → @plus(str)  → @append_str.
                // StringBuilder + char → @plus(char) → @append_char.
                // sb + sb (StringBuilder + StringBuilder) — не поддержано:
                // используй sb1.append_str(sb2.into()) явно.
                if matches!(op, BinOp::Add) && lty == "Nova_StringBuilder*" {
                    if rty == "nova_str" {
                        return Ok(format!("Nova_StringBuilder_method_append_str({}, {})", l, r));
                    }
                    if rty == "nova_int" {
                        // char через nova_int — Ф.9.2 char overload.
                        return Ok(format!("Nova_StringBuilder_method_append_char({}, {})", l, r));
                    }
                }
                // nova_str is a struct — can't use == directly.
                // Plan 13 Ф.9.2: BinOp::Add для str routes через @plus → @concat.
                // Invisible-intrinsic заменён на тот же C-вызов, но через
                // явную декларацию `str.@plus` в std/runtime/string.nv.
                if lty == "nova_str" || rty == "nova_str" {
                    return match op {
                        BinOp::Eq  => Ok(format!("(nova_str_eq({}, {}))", l, r)),
                        BinOp::Neq => Ok(format!("(!nova_str_eq({}, {}))", l, r)),
                        // Ф.9.2: routing через @plus body `=> @concat(other)`.
                        BinOp::Add => Ok(format!("(nova_str_concat({}, {}))", l, r)),
                        // 2026-05-12: lex byte-wise compare для nova_str.
                        // Bootstrap MVP — ASCII-correct; UTF-8 partial.
                        // Полное Unicode collation — production milestone.
                        // См. nova_rt.h nova_str_cmp/lt/le/gt/ge.
                        BinOp::Lt  => Ok(format!("(nova_str_lt({}, {}))", l, r)),
                        BinOp::Le  => Ok(format!("(nova_str_le({}, {}))", l, r)),
                        BinOp::Gt  => Ok(format!("(nova_str_gt({}, {}))", l, r)),
                        BinOp::Ge  => Ok(format!("(nova_str_ge({}, {}))", l, r)),
                        _ => Err(format!("unsupported operator {:?} on nova_str", op)),
                    };
                }
                // nova_unit == nova_unit is always true (unit has one value)
                if lty == "nova_unit" || rty == "nova_unit" {
                    return match op {
                        BinOp::Eq  => Ok("(1)".into()),
                        BinOp::Neq => Ok("(0)".into()),
                        _ => Err(format!("unsupported operator {:?} on nova_unit", op)),
                    };
                }
                // NovaArray_* is a pointer — pointer equality compares identity, not contents.
                // Nova's `==` on arrays means element-wise; emit a runtime call if available,
                // otherwise fall back to pointer eq (correct for test cases comparing same array).
                if lty.starts_with("NovaArray_") || rty.starts_with("NovaArray_") {
                    let elem_ty = lty.strip_prefix("NovaArray_").unwrap_or("nova_int")
                        .trim_end_matches('*');
                    return match op {
                        BinOp::Eq  => Ok(format!("(nova_array_eq_{}({}, {}))", elem_ty, l, r)),
                        BinOp::Neq => Ok(format!("(!nova_array_eq_{}({}, {}))", elem_ty, l, r)),
                        _ => Err(format!("unsupported operator {:?} on array", op)),
                    };
                }
                // NovaOpt_T is a struct — can't use == directly
                if lty.starts_with("NovaOpt_") || rty.starts_with("NovaOpt_") {
                    // Plan 39 Issue A: bare `None` on one side эмитируется как
                    // `NovaOpt_nova_int` (fallback `current_fn_return_ty`).
                    // Если другая сторона — конкретный `NovaOpt_<X>` где X !=
                    // nova_int — переписать None-литерал с правильным opt_ty.
                    let (canonical_opt_ty, _) = if lty.starts_with("NovaOpt_") && lty != "NovaOpt_nova_int" {
                        (lty.clone(), true)
                    } else if rty.starts_with("NovaOpt_") && rty != "NovaOpt_nova_int" {
                        (rty.clone(), true)
                    } else if lty.starts_with("NovaOpt_") {
                        (lty.clone(), false)
                    } else {
                        (rty.clone(), false)
                    };
                    let elem_ty = canonical_opt_ty.strip_prefix("NovaOpt_").unwrap_or("nova_int").to_string();
                    // Re-cast bare None-literal to canonical opt_ty if it slipped through.
                    // Pattern: "((NovaOpt_nova_int){.tag = NOVA_TAG_Option_None})"
                    let none_pat = "((NovaOpt_nova_int){.tag = NOVA_TAG_Option_None})";
                    let none_replacement = format!(
                        "(({}){{.tag = NOVA_TAG_Option_None}})", canonical_opt_ty);
                    let l_fixed = if lty == "NovaOpt_nova_int" && l.contains(none_pat) && canonical_opt_ty != "NovaOpt_nova_int" {
                        l.replace(none_pat, &none_replacement)
                    } else { l.clone() };
                    let r_fixed = if rty == "NovaOpt_nova_int" && r.contains(none_pat) && canonical_opt_ty != "NovaOpt_nova_int" {
                        r.replace(none_pat, &none_replacement)
                    } else { r.clone() };
                    return match op {
                        BinOp::Eq  => Ok(format!("(nova_opt_eq_{}({}, {}))", elem_ty, l_fixed, r_fixed)),
                        BinOp::Neq => Ok(format!("(!nova_opt_eq_{}({}, {}))", elem_ty, l_fixed, r_fixed)),
                        _ => Err(format!("unsupported operator {:?} on option", op)),
                    };
                }
                // _NovaTupleN is a struct — can't use == directly; use memcmp
                if lty.starts_with("_NovaTuple") || rty.starts_with("_NovaTuple") {
                    let struct_ty = if lty.starts_with("_NovaTuple") { &lty } else { &rty };
                    return match op {
                        BinOp::Eq  => Ok(format!("(memcmp(&{}, &{}, sizeof({})) == 0)", l, r, struct_ty)),
                        BinOp::Neq => Ok(format!("(memcmp(&{}, &{}, sizeof({})) != 0)", l, r, struct_ty)),
                        _ => Err(format!("unsupported operator {:?} on tuple", op)),
                    };
                }
                // Nova_T* sum type pointer equality: compare tag + payload fields
                let sum_ty = if lty.starts_with("Nova_") && lty.ends_with('*') { Some(lty.clone()) }
                    else if rty.starts_with("Nova_") && rty.ends_with('*') { Some(rty.clone()) }
                    else { None };
                if let Some(sty) = sum_ty {
                    let type_name_sum = sty.strip_prefix("Nova_").unwrap_or("").trim_end_matches('*').to_string();
                    // D46 operator overloading: Nova_T* + Nova_T* → T_method_plus(l, r).
                    if matches!(op, BinOp::Add) {
                        return Ok(format!("{}_method_plus({}, {})", type_name_sum, l, r));
                    }
                    if matches!(op, BinOp::Eq | BinOp::Neq) {
                        let type_name = type_name_sum;
                        // Build equality: tags equal AND for each variant matching, all fields equal
                        // Simplified: tags equal AND bitwise memcmp of payload (works for int fields)
                        // Full: (l->tag == r->tag) && (l->tag != VarA || l->payload.A._0 == r->payload.A._0) && ...
                        let variants = self.sum_schemas.get(&type_name).cloned().unwrap_or_default();
                        let mut field_conds: Vec<String> = Vec::new();
                        for (var_name, field_types) in &variants {
                            if !field_types.is_empty() {
                                let mut var_fields: Vec<String> = Vec::new();
                                for i in 0..field_types.len() {
                                    var_fields.push(format!("({l})->payload.{v}._{i} == ({r})->payload.{v}._{i}",
                                        l = l, r = r, v = var_name, i = i));
                                }
                                field_conds.push(format!("(({l})->tag != NOVA_TAG_{ty}_{v} || ({fields}))",
                                    l = l, ty = type_name, v = var_name,
                                    fields = var_fields.join(" && ")));
                            }
                        }
                        let tag_eq = format!("(({l})->tag == ({r})->tag)", l = l, r = r);
                        let eq = if field_conds.is_empty() {
                            tag_eq
                        } else {
                            format!("({} && {})", tag_eq, field_conds.join(" && "))
                        };
                        return match op {
                            BinOp::Eq  => Ok(format!("({})", eq)),
                            BinOp::Neq => Ok(format!("(!({}))", eq)),
                            _ => unreachable!(),
                        };
                    }
                }
                // Plan 33.1 (D24): импликация/эквивалентность — sugar.
                // `A ==> B` → `(!A || B)`; `A <==> B` → `(A == B)`.
                match op {
                    BinOp::Implies => return Ok(format!("((!({})) || ({}))", l, r)),
                    BinOp::Iff => return Ok(format!("(({}) == ({}))", l, r)),
                    _ => {}
                }
                let op_str = match op {
                    BinOp::Add => "+",  BinOp::Sub => "-",
                    BinOp::Mul => "*",  BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Eq  => "==", BinOp::Neq => "!=",
                    BinOp::Lt  => "<",  BinOp::Le  => "<=",
                    BinOp::Gt  => ">",  BinOp::Ge  => ">=",
                    BinOp::And => "&&", BinOp::Or  => "||",
                    BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<", BinOp::Shr => ">>",
                    BinOp::Implies | BinOp::Iff => unreachable!("handled above"),
                };
                Ok(format!("({} {} {})", l, op_str, r))
            }

            ExprKind::Unary { op, operand } => {
                let v = self.emit_expr(operand)?;
                let op_str = match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                };
                Ok(format!("({}{})", op_str, v))
            }

            ExprKind::If { cond, then, else_ } => {
                self.emit_if_expr(cond, then, else_.as_ref())
            }

            ExprKind::Block(block) => {
                self.emit_block_expr(block)
            }

            ExprKind::Call { func, args, trailing } => {
                // Plan 33.1 Ф.4 (D24): `old(expr)` — special-case в контрактах.
                // В 33.1 нет mut state, поэтому old(expr) — это просто expr
                // (значение не меняется между entry и exit). Snapshot для mut —
                // в 33.2 вместе с frame conditions.
                if let ExprKind::Ident(n) = &func.kind {
                    if n == "old" && args.len() == 1 && trailing.is_none() {
                        // Просто emit аргумент.
                        return self.emit_expr(args[0].expr());
                    }
                }
                // Plan 19, C5: trailing разбираем три варианта.
                // - `Block` / `LegacyBlockWithParams` — конвертируем в
                //   legacy `TrailingBlock` для emit_call_with_trailing.
                // - `Fn(sb)` — pre-rewrite: вставляем synthetic
                //   ClosureFull-аргумент в конец `args`, trailing
                //   обнуляем. Codegen дальше обработает как обычный
                //   closure-аргумент.
                if let Some(crate::ast::Trailing::Fn(sb)) = trailing.as_ref() {
                    let closure_expr = Expr::new(
                        ExprKind::ClosureFull(sb.clone()),
                        sb.span,
                    );
                    let mut args_extended = args.clone();
                    args_extended.push(CallArg::Item(closure_expr));
                    return self.emit_call_with_trailing(
                        func,
                        &args_extended,
                        None,
                    );
                }
                let legacy_tb = trailing.as_ref().and_then(|t| match t {
                    crate::ast::Trailing::Block(b) => Some(crate::ast::TrailingBlock {
                        params: Vec::new(),
                        body: (**b).clone(),
                        span: b.span,
                    }),
                    crate::ast::Trailing::LegacyBlockWithParams(tb) => Some((**tb).clone()),
                    crate::ast::Trailing::Fn(_) => unreachable!(
                        "Trailing::Fn handled above by pre-rewrite"
                    ),
                });
                self.emit_call_with_trailing(func, args, legacy_tb.as_ref())
            }

            ExprKind::Member { obj, name } => {
                // Plan 11 Ф.4: method values (bound / unbound).
                // `obj.@method` — bound method value (закрывает `obj` как self).
                // `Type.@method` — unbound method value (fn-pointer, явный self).
                // Парсер маркирует обе формы префиксом `@` в имени.
                if let Some(method_name) = name.strip_prefix('@') {
                    return self.emit_method_value(obj, method_name);
                }
                let obj_ty = self.infer_expr_c_type(obj);
                let o = self.emit_expr(obj)?;
                // D26 (school B): s.len — длина в codepoint'ах, O(n).
                // Для байтовой длины — s.byte_len() (см. str_method_to_rt).
                if obj_ty == "nova_str" && name == "len" {
                    return Ok(format!("nova_str_char_len({})", o));
                }
                // NovaArray_T*.len → arr->len (already nova_int/int64_t)
                if obj_ty.starts_with("NovaArray_") && name == "len" {
                    return Ok(format!("({}->len)", o));
                }
                // NovaArray_T*.is_empty → (arr->len == 0) — bool, D38 built-in.
                if obj_ty.starts_with("NovaArray_") && name == "is_empty" {
                    return Ok(format!("(({}->len) == 0)", o));
                }
                // nova_str.is_empty → (s.len == 0) — bool. D26: str.len в
                // codepoint'ах (O(n)); is_empty можно проверить по byte_len O(1).
                if obj_ty == "nova_str" && name == "is_empty" {
                    return Ok(format!("(({}.len) == 0)", o));
                }
                // Tuple field access: t.0 → t.f0, t.1 → t.f1, etc.
                if name.chars().all(|c| c.is_ascii_digit()) {
                    let idx: usize = name.parse().unwrap_or(0);
                    let field_name = format!("f{}", idx);
                    // For void* (erased generic return), cast to _NovaTupleN* if we know the element types
                    if obj_ty == "void*" {
                        if let ExprKind::Ident(var_name) = &obj.kind {
                            if let Some(elem_tys) = self.tuple_element_types.get(var_name.as_str()).cloned() {
                                let arity = elem_tys.len();
                                if let Some(elem_ty) = elem_tys.get(idx) {
                                    // Unbox based on element type. Fields are nova_int storing void* values.
                                    // Note: parens around cast are essential: ((_NovaTupleN*)(ptr))->field
                                    if elem_ty == "nova_str*" || elem_ty == "nova_str" {
                                        // Field stores a nova_str* as intptr_t; cast back and dereference
                                        return Ok(format!("(*(nova_str*)(intptr_t)(((_NovaTuple{n}*)({o}))->{f}))", n=arity, o=o, f=field_name));
                                    } else if elem_ty == "nova_int" || elem_ty == "nova_bool" {
                                        return Ok(format!("((nova_int)(intptr_t)(((_NovaTuple{n}*)({o}))->{f}))", n=arity, o=o, f=field_name));
                                    } else {
                                        return Ok(format!("(((_NovaTuple{n}*)({o}))->{f})", n=arity, o=o, f=field_name));
                                    }
                                }
                            }
                        }
                        // Unknown void* tuple access — emit NULL
                        return Ok("NULL".into());
                    }
                    let accessor = if Self::is_value_type(&obj_ty) { "." } else { "->" };
                    let raw = format!("({}{}{} )", o, accessor, field_name);
                    // If we know the original element type (from tuple_element_types), cast back
                    if let ExprKind::Ident(var_name) = &obj.kind {
                        if let Some(elem_tys) = self.tuple_element_types.get(var_name.as_str()).cloned() {
                            if let Some(elem_ty) = elem_tys.get(idx) {
                                if elem_ty != "nova_int" && !elem_ty.is_empty() {
                                    if elem_ty.ends_with('*') {
                                        // Decide whether to dereference:
                                        // - Types that were heap-allocated from value types (like _NovaTuple*, nova_str*):
                                        //   need deref to get the value back.
                                        // - Types that were already pointers (Nova_T*, NovaArray_*):
                                        //   just cast back without deref.
                                        let base = elem_ty.trim_end_matches('*');
                                        let was_heap_wrapped = base.starts_with("_NovaTuple")
                                            || base.starts_with("NovaOpt_")
                                            || base == "nova_str";
                                        if was_heap_wrapped {
                                            return Ok(format!("(*({}*)({}{}{}))", base, o, accessor, field_name));
                                        } else {
                                            return Ok(format!("(({})({}{}{}))", elem_ty, o, accessor, field_name));
                                        }
                                    } else {
                                        return Ok(format!("(({})({}{}{}))", elem_ty, o, accessor, field_name));
                                    }
                                }
                            }
                        }
                    }
                    return Ok(raw);
                }
                // Non-tuple field access on void* (erased generic) — can't resolve
                if obj_ty == "void*" {
                    return Ok("NULL".into());
                }
                // Mangle field name если коллизия с C-keyword (`char`, `int` и т.п.).
                let field_name = Self::mangle_field_name(name);
                // Check if obj is an array index expression whose elements are record pointers.
                // e.g. xs[1].inner where xs = NovaArray_nova_int* containing Nova_Box*.
                if let ExprKind::Index { obj: arr_obj, .. } = &obj.kind {
                    let arr_var_name = match &arr_obj.kind {
                        ExprKind::Ident(n) => Some(n.as_str()),
                        _ => None,
                    };
                    if let Some(arr_name) = arr_var_name {
                        if let Some(real_elem_ty) = self.array_element_types.get(arr_name).cloned() {
                            if real_elem_ty.ends_with('*') {
                                // Cast the nova_int array element back to the real pointer type
                                return Ok(format!("((({})({}))->{})", real_elem_ty, o, field_name));
                            }
                        }
                    }
                }
                // nova_str and other value types use `.`, pointer types use `->`
                let accessor = if Self::is_value_type(&obj_ty) { "." } else { "->" };
                Ok(format!("({}{}{})", o, accessor, field_name))
            }

            ExprKind::For { pattern, iter, body, .. } => {
                self.emit_for(pattern, iter, body)
            }

            ExprKind::While { cond, body, .. } => {
                // Plan 08 Ф.4: strict bool-check.
                let cond_ty = self.infer_expr_c_type(cond);
                self.check_bool_condition_at(&cond_ty, "while", cond.span)?;
                let cond_val = self.emit_expr(cond)?;
                let tmp = self.fresh_tmp_named("while");
                self.line(&format!("nova_unit {};", tmp));
                self.line(&format!("while ({}) {{", cond_val));
                self.indent += 1;
                // Plan 20 Ф.4/Ф.8: defer/errdefer внутри loop body
                // регистрируется на каждой итерации (is_loop_body=true).
                self.emit_loop_body_inline(body)?;
                self.indent -= 1;
                self.line("}");
                self.line(&format!("{} = NOVA_UNIT;", tmp));
                Ok(tmp)
            }

            ExprKind::Loop { body, .. } => {
                let tmp = self.fresh_tmp_named("loop");
                self.line(&format!("nova_unit {};", tmp));
                self.line("for (;;) {");
                self.indent += 1;
                self.emit_loop_body_inline(body)?;
                self.indent -= 1;
                self.line("}");
                self.line(&format!("{} = NOVA_UNIT;", tmp));
                Ok(tmp)
            }

            ExprKind::Match { scrutinee, arms } => {
                self.emit_match(scrutinee, arms)
            }

            ExprKind::Select { arms } => {
                self.emit_select(arms)
            }

            ExprKind::Range { start, end, inclusive } => {
                // Plan 36 followup: emit literal `0..N` как Nova_Range
                // alloc + init если `Range` type зарегистрирован
                // (например через std/collections/range.nv в same module
                // или импорт after Plan 35). Иначе placeholder для
                // emit_for Case 1 (primitive int loop matches это до
                // emit_expr через ExprKind::Range pattern).
                //
                // Inclusive `..=N` — эмитим как `start..end+1` (полу-
                // открытый эквивалент). Range type не имеет inclusive
                // поля (std/collections/range.nv:30 — `{start, end}`).
                let s = self.emit_expr(start)?;
                let e = self.emit_expr(end)?;
                if self.record_schemas.contains_key("Range") {
                    let tmp = self.fresh_tmp();
                    let end_expr = if *inclusive {
                        format!("({} + ((nova_int)1LL))", e)
                    } else {
                        e.clone()
                    };
                    self.line(&format!(
                        "Nova_Range* {} = (Nova_Range*)nova_alloc(sizeof(Nova_Range));",
                        tmp));
                    self.line(&format!("{}->start = {};", tmp, s));
                    self.line(&format!("{}->end = {};", tmp, end_expr));
                    Ok(tmp)
                } else {
                    let _ = inclusive;
                    Ok(format!("/*range({}, {})*/NOVA_UNIT", s, e))
                }
            }

            ExprKind::RecordLit { type_name, fields, .. } => {
                // Plan 52 Ф.10: D55 map-coercion для `{field: v}` — обрабатывается
                // в desugar.rs (mirror MapLit-десугаринга) ДО codegen. Если узел
                // дошёл сюда с `inferred_map_v: Some(_)` без desugar — это bug,
                // но codegen консервативно идёт обычным record-path (упадёт на
                // mismatch'е полей если так).
                let lit = self.emit_record_lit(type_name.as_deref(), fields)?;
                // Plan 33.3 Ф.9.2 (D24): record invariant auto-enforce.
                // Если у type есть invariants — wrap'нуть конструкцию
                // в runtime-check (in debug). Используем statement-expression
                // GNU extension `({ ... })` чтобы expr-position сохранилась.
                if let Some(name) = type_name {
                    let struct_name = name.join("_");
                    if let Some(invs) = self.record_invariants.get(&struct_name).cloned() {
                        // Эмитим check inline. lit — это уже tmp-var имя
                        // (emit_record_lit возвращает имя), либо expression.
                        // Для простоты: декларируем temp, проверяем invariants,
                        // возвращаем temp. Это меняет inline structure — нужен
                        // отдельный line-emit, а не expr-substitution.
                        // Поскольку lit обычно tmp (см. emit_record_lit), просто
                        // эмитим check после.
                        for (inv_expr, span) in &invs {
                            // Bind поля record'а как `tmp->field` для invariant-eval.
                            // В bootstrap — простой text substitution через
                            // emit_expr с self.expected_record_type set.
                            // Сам invariant ссылается на field-names directly
                            // (записан как `balance >= 0` без `self.`).
                            // Чтобы он сработал — должны field'ы быть в scope.
                            // В bootstrap делаем макрос через define + undef.
                            let inv_src = Self::expr_to_display(inv_expr);
                            // Получаем поля типа.
                            if let Some(fields_schema) = self.record_schemas.get(&struct_name).cloned() {
                                self.line("#ifdef NOVA_CONTRACTS_RUNTIME");
                                self.line("{");
                                // Decl shadow-locals для каждого поля → tmp->field.
                                for (fname, ftyc) in &fields_schema {
                                    self.line(&format!("    {} {} = {}->{};", ftyc, fname, lit, fname));
                                }
                                let inv_c = self.emit_expr(inv_expr)?;
                                self.line(&format!(
                                    "    if (!({})) nova_contract_violation(NOVA_CONTRACT_INV, \"{}\", \"{}\", \"<invariant>\", {});",
                                    inv_c, struct_name, Self::escape_c_str(&inv_src), span.start
                                ));
                                self.line("}");
                                self.line("#endif");
                            }
                        }
                    }
                }
                Ok(lit)
            }

            ExprKind::ArrayLit(elems) => {
                self.emit_array_lit(elems)
            }

            // Plan 52 Ф.20: invariant — MapLit ДОЛЖЕН быть устранён
            // desugar pass'ом (desugar.rs::desugar_module) ДО входа в
            // codegen. Pipeline: parse → type-check → annotate → desugar
            // → codegen. Если сюда попал raw MapLit — compiler bug
            // (забыли вызвать desugar_module в pipeline wiring).
            ExprKind::MapLit { .. } => {
                Err("compiler bug: map literal `[k: v]` reached codegen без \
                     desugar pass — нарушение pipeline invariant. \
                     desugar_module() обязан быть вызван до codegen. \
                     Report issue: https://github.com/unitcraft/nova-lang/issues".into())
            }

            ExprKind::TupleLit(elems) => {
                // Tuple literals: use pre-declared _NovaTupleN typedef (file-scope).
                // All fields are nova_int, but we track element types so field access
                // `pair.0` can cast back to the original type when it's a pointer.
                // For nested tuple or struct elements, heap-allocate and store pointer as nova_int.
                let n = elems.len();
                let struct_name = format!("_NovaTuple{}", n);
                let mut vals: Vec<String> = Vec::new();
                let mut elem_types: Vec<String> = Vec::new();
                for e in elems {
                    let ety = self.infer_expr_c_type(e);
                    let v = self.emit_expr(e)?;
                    // If element is a struct type, heap-allocate it and store pointer as nova_int
                    let needs_heap = ety.starts_with("_NovaTuple") || ety.starts_with("NovaOpt_")
                        || ety == "nova_str" || ety == "nova_unit";
                    if needs_heap && !ety.ends_with('*') {
                        let ptr_tmp = self.fresh_tmp();
                        self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", ety, ptr_tmp, ety, ety));
                        self.line(&format!("*{} = {};", ptr_tmp, v));
                        vals.push(format!("(nova_int)({})", ptr_tmp));
                        elem_types.push(format!("{}*", ety));
                    } else {
                        vals.push(v);
                        elem_types.push(ety);
                    }
                }
                let tmp = self.fresh_tmp();
                self.line(&format!("{} {};", struct_name, tmp));
                for (i, v) in vals.iter().enumerate() {
                    if elem_types[i].ends_with('*') && !elem_types[i].starts_with("Nova_") {
                        // Already cast to nova_int above (pointer stored as nova_int)
                        self.line(&format!("{}.f{} = {};", tmp, i, v));
                    } else {
                        self.line(&format!("{}.f{} = (nova_int)({});", tmp, i, v));
                    }
                }
                self.var_types.insert(tmp.clone(), struct_name.clone());
                self.tuple_element_types.insert(tmp.clone(), elem_types);
                Ok(tmp)
            }

            ExprKind::Try(inner) => {
                let inner_ty = self.infer_expr_c_type(inner);
                let val = self.emit_expr(inner)?;
                let try_tmp = self.fresh_tmp();
                if inner_ty.starts_with("NovaOpt_") {
                    // Option?: if None, return None; else extract value.
                    // Plan 14 Ф.1: typed early-return None — текст compound
                    // literal'а соответствует current_fn_return_ty (= тип
                    // контейнера, в который мы возвращаем).
                    let none_expr: String = self.current_fn_return_ty.as_ref()
                        .filter(|t| t.starts_with("NovaOpt_"))
                        .map(|t| format!("(({}){{.tag = NOVA_TAG_Option_None}})", t))
                        .unwrap_or_else(|| "nova_make_Option_None()".to_string());
                    self.line(&format!("{} {} = {};", inner_ty, try_tmp, val));
                    self.line(&format!("if ({}.tag == NOVA_TAG_Option_None) {{ return {}; }}", try_tmp, none_expr));
                    Ok(format!("({}.value)", try_tmp))
                } else if inner_ty == "Nova_Result*" {
                    // Result?: if Err, propagate Err; else extract Ok value
                    self.line(&format!("Nova_Result* {} = {};", try_tmp, val));
                    self.line(&format!("if ({}->tag == NOVA_TAG_Result_Err) {{ return nova_make_Result_Err({}->payload.Err._0); }}", try_tmp, try_tmp));
                    Ok(format!("({}->payload.Ok._0)", try_tmp))
                } else {
                    // Unknown type: emit as-is with comment
                    Ok(format!("({} /* ? */)", val))
                }
            }

            // Plan 19, C7 (D85): postfix `!!` — throw-стиль.
            //
            // На Some(v)/Ok(v) — разворачивает в `v`.
            // На None — `nova_throw(RuntimeNoneError)` (longjmp в
            //   ближайший Fail-handler).
            // На Err(e) — `nova_throw(e)`.
            //
            // В отличие от `?` (early-return обёртки в caller), `!!`
            // использует runtime Fail-эффект через nova_throw / setjmp.
            // Caller должен иметь активный Fail-handler в скоупе
            // (через `with Fail = ...`) или Fail в effect-row, иначе
            // runtime ошибка станет fatal.
            ExprKind::Bang(inner) => {
                let inner_ty = self.infer_expr_c_type(inner);
                let val = self.emit_expr(inner)?;
                let bang_tmp = self.fresh_tmp();
                if inner_ty.starts_with("NovaOpt_") {
                    // Option!!: на None бросаем RuntimeNoneError.
                    self.line(&format!("{} {} = {};", inner_ty, bang_tmp, val));
                    self.line(&format!(
                        "if ({}.tag == NOVA_TAG_Option_None) {{ nova_throw_runtime_none_error(); }}",
                        bang_tmp
                    ));
                    Ok(format!("({}.value)", bang_tmp))
                } else if inner_ty == "Nova_Result*" {
                    // Result!!: на Err бросаем error value через
                    // generic nova_throw.
                    self.line(&format!("Nova_Result* {} = {};", bang_tmp, val));
                    self.line(&format!(
                        "if ({}->tag == NOVA_TAG_Result_Err) {{ nova_throw_value({}->payload.Err._0); }}",
                        bang_tmp, bang_tmp
                    ));
                    Ok(format!("({}->payload.Ok._0)", bang_tmp))
                } else {
                    Ok(format!("({} /* !! */)", val))
                }
            }

            ExprKind::As(inner, ty) => {
                // Plan 11 Ф.5: `obj.@method as fn(P...) -> R` — type annotation
                // disambig'ит overloaded method values. Передаём аннотированный
                // signature в emit_method_value чтобы выбрать right overload.
                if let ExprKind::Member { obj: mvobj, name: mvname } = &inner.kind {
                    if let Some(method_name) = mvname.strip_prefix('@') {
                        if let TypeRef::Func { params: fp, return_type, .. } = ty {
                            let target_ptys: Vec<String> = fp.iter()
                                .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                                .collect();
                            let target_rty = return_type.as_ref()
                                .map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into()))
                                .unwrap_or_else(|| "nova_unit".into());
                            return self.emit_method_value_typed(mvobj, method_name,
                                Some((target_ptys, target_rty)));
                        }
                    }
                }
                // D54: `expr as T` эмитит явный C-cast `((c_ty)(expr))`.
                // - numeric narrowing → wraparound (C-style truncate младших битов)
                // - newtype ↔ underlying → idempotent (одинаковое C-представление)
                // План 05.
                //
                // План 07: float → integer narrowing требует **saturation**
                // вместо C-cast (UB на out-of-range). Детектим источник как
                // f64/f32 и target как integer, эмитим runtime helper
                // `nova_<src>_to_<dst>` (см. nova_rt/cast.h). Saturation
                // совпадает с Rust 1.45+ (RFC #2484 sealed casts):
                //   - in-range → truncate towards zero
                //   - out-of-range positive → INT_MAX / UINT_MAX
                //   - out-of-range negative → INT_MIN / 0 (для unsigned)
                //   - NaN → 0
                //   - ±Infinity → границы
                //
                // Plan 08 Ф.5: as-cast restrictions для char/byte/bool.
                // По D54 запрещены: int as char (use char.try_from), int as bool
                // (use n != 0), char as byte (use byte.try_from), str ↔ T (use
                // str.from / T.try_from). Detection через original Nova-имя
                // target'а (TypeRef::Named path), не через C-имя — char и int
                // имеют одинаковый C-тип nova_int.
                let target_nova = if let TypeRef::Named { path, .. } = ty {
                    path.last().cloned()
                } else { None };
                let inner_c_ty_for_check = self.infer_expr_c_type(inner);
                // Получим Nova-имя источника для restrictions check.
                let src_nova = Self::nova_type_name_from_c(&inner_c_ty_for_check);
                if let Some(tgt_nova) = target_nova.as_deref() {
                    Self::check_as_cast_allowed(&src_nova, tgt_nova, &inner.kind)?;
                }
                let target_c = self.type_ref_to_c(ty)
                    .map_err(|e| format!("as-cast type error: {}", e))?;
                let inner_c_ty = inner_c_ty_for_check;
                let v = self.emit_expr(inner)?;

                let src_suffix = match inner_c_ty.as_str() {
                    "nova_f64" => Some("f64"),
                    "nova_f32" => Some("f32"),
                    _ => None,
                };
                let dst_suffix: Option<&str> = match target_c.as_str() {
                    "nova_int" | "int64_t" => Some("i64"),
                    "int32_t"              => Some("i32"),
                    "int16_t"              => Some("i16"),
                    "int8_t"               => Some("i8"),
                    "uint64_t"             => Some("u64"),
                    "uint32_t"             => Some("u32"),
                    "uint16_t"             => Some("u16"),
                    "nova_byte" | "uint8_t" => Some("u8"),
                    _ => None,
                };
                if let (Some(src), Some(dst)) = (src_suffix, dst_suffix) {
                    // План 07 saturation helper.
                    return Ok(format!("nova_{}_to_{}({})", src, dst, v));
                }
                // Все остальные cast'ы — прямой C-cast (план 05).
                Ok(format!("(({})({}))", target_c, v))
            }

            ExprKind::Is(inner, ty) => {
                // D54 v2: `expr is Variant` for sum-types — runtime tag check.
                // Get the variant name from the TypeRef (must be Named with len 1 or 2).
                let variant_name = match ty {
                    TypeRef::Named { path, .. } if path.len() == 1 => path[0].clone(),
                    TypeRef::Named { path, .. } if path.len() == 2 => path[1].clone(),
                    _ => return Err("`is` expects a variant name (e.g. `x is Some`)".into()),
                };
                // Find the sum-type that owns this variant.
                let (sum_type, _fields) = self.find_variant(&variant_name)
                    .ok_or_else(|| format!("`is {}` — unknown variant", variant_name))?;
                let inner_ty = self.infer_expr_c_type(inner);
                let inner_c = self.emit_expr(inner)?;
                // Tag access depends on layout:
                //   NovaOpt_nova_int (Option) → value-struct, dot accessor
                //   Nova_<Sum>* (custom sum)  → pointer, arrow accessor
                let accessor = if inner_ty.starts_with("NovaOpt_") && !inner_ty.ends_with('*') {
                    "."
                } else {
                    "->"
                };
                Ok(format!("(({}){}tag == NOVA_TAG_{}_{})",
                    inner_c, accessor, sum_type, variant_name))
            }

            ExprKind::Coalesce(left, right) => {
                let left_ty = self.infer_expr_c_type(left);
                let l = self.emit_expr(left)?;
                let r = self.emit_expr(right)?;
                if left_ty.starts_with("NovaOpt_") {
                    let opt_tmp = self.fresh_tmp();
                    self.line(&format!("{} {} = {};", left_ty, opt_tmp, l));
                    Ok(format!("({}.tag == NOVA_TAG_Option_Some ? {}.value : {})", opt_tmp, opt_tmp, r))
                } else {
                    Ok(format!("({} /*?? unsupported */ , {})", l, r))
                }
            }

            ExprKind::Lambda { params, body, return_type, .. } => {
                self.emit_lambda(params, body, None, return_type.as_ref())
            }
            // Plan 19, C5: closure-light codegen — конвертируем в
            // legacy `LambdaParam`/`Expr` и переиспользуем
            // emit_lambda. ClosureLight params не имеют типов, поэтому
            // emit_lambda выводит их через context_param_tys (`None`)
            // или дефолт `nova_int`. Block-body заворачивается в
            // `Expr(Block)`.
            ExprKind::ClosureLight { params, body } => {
                // Конвертация ClosureLightParam → LambdaParam (типы None).
                let legacy_params: Vec<LambdaParam> = params
                    .iter()
                    .map(|p| LambdaParam {
                        name: p.name.clone(),
                        ty: None,
                        span: p.span,
                    })
                    .collect();
                // Тело — bare expr или block. Block заворачиваем
                // в `Expr::Block(...)`, чтобы emit_lambda ожидал Expr.
                let body_expr: Expr = match body {
                    crate::ast::ClosureBody::Expr(e) => (**e).clone(),
                    crate::ast::ClosureBody::Block(b) => Expr::new(
                        ExprKind::Block(b.clone()),
                        b.span,
                    ),
                };
                self.emit_lambda(&legacy_params, &body_expr, None, None)
            }
            // Plan 19, C5: closure-full codegen — типизированный
            // closure аналогичен named fn без имени. Конвертируем
            // params (с типами) в `LambdaParam` (с Some(ty)) и
            // переиспользуем emit_lambda. FnBody::Expr → Expr,
            // FnBody::Block → Expr(Block).
            ExprKind::ClosureFull(sb) => {
                let legacy_params: Vec<LambdaParam> = sb
                    .params
                    .iter()
                    .map(|p| LambdaParam {
                        name: p.name.clone(),
                        ty: Some(p.ty.clone()),
                        span: p.span,
                    })
                    .collect();
                let body_expr: Expr = match &sb.body {
                    FnBody::Expr(e) => e.clone(),
                    FnBody::Block(b) => Expr::new(
                        ExprKind::Block(b.clone()),
                        b.span,
                    ),
                    FnBody::External => unreachable!(
                        "closure-full cannot be `external` — only named fn"
                    ),
                };
                self.emit_lambda(
                    &legacy_params,
                    &body_expr,
                    None,
                    sb.return_type.as_ref(),
                )
            }
            ExprKind::With { bindings, body } => {
                self.emit_with(bindings, body)
            }
            ExprKind::HandlerLit { effect_name, methods } => {
                self.emit_handler_lit(effect_name, methods)
            }
            ExprKind::Interrupt(val) => {
                // Plan 39 Issue A: choose slot by value category.
                //   - IntLike/UnitVoid → nova_interrupt(int_val)
                //   - Pointer → nova_interrupt_ptr(ptr_val)
                //   - ValueStruct → heap-alloc copy, nova_interrupt_ptr(slot)
                match val.as_deref().map(|e| (&e.kind, e)) {
                    None | Some((ExprKind::UnitLit, _)) => {
                        self.line("nova_interrupt(((nova_int)0LL));");
                    }
                    Some((_, v)) => {
                        let v_ty = self.infer_expr_c_type(v);
                        let category = with_result_category(&v_ty);
                        let vstr = self.emit_expr(v)?;
                        match category {
                            WithResultCategory::IntLike => {
                                self.line(&format!("nova_interrupt((nova_int)({}));", vstr));
                            }
                            WithResultCategory::UnitVoid => {
                                self.line(&format!("(void)({});", vstr));
                                self.line("nova_interrupt(((nova_int)0LL));");
                            }
                            WithResultCategory::Pointer => {
                                self.line(&format!("nova_interrupt_ptr((void*)({}));", vstr));
                            }
                            WithResultCategory::ValueStruct => {
                                // Heap-allocate slot, copy value, pass pointer.
                                let slot = self.fresh_tmp();
                                self.line(&format!(
                                    "{}* {} = ({}*)nova_alloc(sizeof({}));",
                                    v_ty, slot, v_ty, v_ty));
                                self.line(&format!("*{} = ({});", slot, vstr));
                                self.line(&format!("nova_interrupt_ptr((void*){});", slot));
                            }
                        }
                    }
                }
                // After interrupt the code is unreachable, but emit a dummy value
                Ok("NOVA_UNIT".into())
            }
            ExprKind::Throw(value) => {
                // D25/D65/D85: throw в expression-position. Тип Never —
                // control никогда не вернётся. Эмитируем effect-call
                // Nova_Fail_fail через comma-expression
                // `(Nova_Fail_fail(v), (nova_int)0LL)` — dummy nova_int нужного
                // типа для совместимости с C-каст'ами в caller'е.
                //
                // **Важно:** comma-expression, не statement+dummy через
                // self.line() — потому что statement+dummy ломает short-circuit
                // семантику родительских конструкций (?? coalesce → тернарник
                // `cond ? value : RHS`; conditional expression). С statement
                // throw выполнялся бы eagerly, **до** проверки cond.
                // Comma-expression — inline expression, выполняется только
                // когда родитель реально evaluates это выражение.
                //
                // Закрывает Q-throw-comma (тот же паттерн что для nv_panic /
                // nv_exit — см. special-case в emit_call).
                let v = self.emit_expr(value)?;
                Ok(format!("(Nova_Fail_fail({}), (nova_int)0LL)", v))
            }
            ExprKind::Forbid { body, .. } => {
                // forbid X { body } — in bootstrap, emit body as plain block (no runtime check)
                self.emit_block_expr(body)
            }
            ExprKind::Realtime { body, .. } => {
                // realtime block — in Phase 1 just emit as block
                self.emit_block_expr(body)
            }
            ExprKind::Spawn(body) => {
                self.emit_spawn(body)
            }
            ExprKind::Supervised { body, cancel } => {
                self.emit_supervised(body, cancel.as_deref())
            }
            ExprKind::Detach(body) => {
                self.emit_detach(body)
            }
            ExprKind::ParallelFor { pattern, iter, body } => {
                self.emit_parallel_for(pattern, iter, body)
            }
            ExprKind::TaggedTemplate { parts, args, .. } => {
                // Bootstrap: tag function ignored, parts concatenated with args as strings.
                // Build a single nova_str by concatenating all parts and arg string reprs.
                if parts.is_empty() {
                    return Ok("(nova_str){.ptr=\"\", .len=0}".into());
                }
                if args.is_empty() {
                    // Simple string literal: all content is in parts[0]
                    let combined = parts.join("");
                    let escaped = Self::escape_c_str(&combined);
                    return Ok(format!("(nova_str){{.ptr=\"{}\", .len={}}}", escaped, combined.len()));
                }
                // With interpolations: concatenate parts[0] + str.from(args[0]) + parts[1] + ...
                // (D73: string interpolation uses From[X]/str. In bootstrap codegen we call
                //  nova_int_to_str directly for int-like values; full From-dispatch is TBD.)
                let mut result_exprs = Vec::new();
                for (i, part) in parts.iter().enumerate() {
                    if !part.is_empty() {
                        let escaped = Self::escape_c_str(part);
                        result_exprs.push(format!("(nova_str){{.ptr=\"{}\", .len={}}}", escaped, part.len()));
                    }
                    if let Some(arg) = args.get(i) {
                        let v = self.emit_expr(arg)?;
                        let arg_ty = self.infer_expr_c_type(arg);
                        let str_expr = if arg_ty == "nova_str" {
                            v
                        } else {
                            format!("nova_int_to_str((nova_int)({}))", v)
                        };
                        result_exprs.push(str_expr);
                    }
                }
                if result_exprs.is_empty() {
                    return Ok("(nova_str){.ptr=\"\", .len=0}".into());
                }
                let mut acc = result_exprs[0].clone();
                for expr in &result_exprs[1..] {
                    acc = format!("nova_str_concat({}, {})", acc, expr);
                }
                Ok(acc)
            }
            ExprKind::SelfAccess => {
                Ok("nova_self".into())
            }
            ExprKind::Index { obj, index } => {
                let obj_ty = self.infer_expr_c_type(obj);
                let i = self.emit_expr(index)?;
                // D109: monomorphized @field[idx] — obj_ty may be "nova_int" (erased record
                // schema not registered for concrete instance), but array_element_types has
                // the concrete pointer type. Emit cast form directly, before obj_ty check.
                if let ExprKind::Member { obj: inner_obj, name: field } = &obj.kind {
                    if matches!(inner_obj.kind, ExprKind::SelfAccess) {
                        let key = format!("(nova_self->{})", Self::mangle_field_name(field));
                        if let Some(elem_ty) = self.array_element_types.get(&key).cloned() {
                            let o = self.emit_expr(obj)?;
                            return Ok(format!("(({})({}->data[{}]))", elem_ty, o, i));
                        }
                    }
                }
                if obj_ty.starts_with("NovaArray_") {
                    let o = self.emit_expr(obj)?;
                    // Check if elements are pointer types stored as nova_int (e.g. inner arrays or records)
                    let arr_var_name = if let ExprKind::Ident(n) = &obj.kind { Some(n.as_str()) } else { None };
                    let inner_elem_ty = arr_var_name
                        .and_then(|n| self.array_element_types.get(n).cloned())
                        // Fallback: look up by the emitted C expression string (covers @field[idx])
                        .or_else(|| self.array_element_types.get(o.as_str()).cloned());
                    if let Some(ref inner_ty) = inner_elem_ty {
                        if inner_ty.starts_with("NovaArray_") {
                            // array-of-arrays: cast the element and get data pointer
                            return Ok(format!("(({})({}->data[{}]))", inner_ty, o, i));
                        }
                        if inner_ty.ends_with('*') {
                            // array-of-record-pointers: cast element to real pointer type
                            return Ok(format!("(({})({}->data[{}]))", inner_ty, o, i));
                        }
                    }
                    // Check if array stores boxed nova_str* elements
                    let is_str_boxed = arr_var_name
                        .map(|n| self.str_box_arrays.contains(n))
                        .unwrap_or(false);
                    if is_str_boxed {
                        return Ok(format!("(*(nova_str*)(({})->data[{}]))", o, i));
                    }
                    // Default: NovaArray_nova_int element — raw data access
                    Ok(format!("({})->data[{}]", o, i))
                } else if let ExprKind::Index { obj: outer_arr, index: outer_idx } = &obj.kind {
                    // Double-indexing: arr[i][j] where arr[i] is a nova_int storing a NovaArray_*
                    // Check if the outer array has element type tracking
                    let outer_arr_name = if let ExprKind::Ident(n) = &outer_arr.kind { Some(n.as_str()) } else { None };
                    let inner_arr_ty = outer_arr_name
                        .and_then(|n| self.array_element_types.get(n).cloned());
                    if let Some(inner_ty) = inner_arr_ty {
                        if inner_ty.starts_with("NovaArray_") {
                            let outer_o = self.emit_expr(outer_arr)?;
                            let outer_i = self.emit_expr(outer_idx)?;
                            // (inner_ty)(outer_o->data[outer_i])->data[i]
                            return Ok(format!("((({})({}->data[{}]))->data[{}])", inner_ty, outer_o, outer_i, i));
                        }
                    }
                    let o = self.emit_expr(obj)?;
                    Ok(format!("({})[{}]", o, i))
                } else {
                    let o = self.emit_expr(obj)?;
                    Ok(format!("({})[{}]", o, i))
                }
            }
            ExprKind::IfLet { pattern, scrutinee, then, else_ } => {
                // Desugar: if let Pat = expr { then } else { else_ }
                // → evaluate scrutinee, check pattern cond, bind, run then or else_
                let scr = self.emit_expr(scrutinee)?;
                let scr_ty = self.infer_expr_c_type(scrutinee);
                let scr_tmp = self.fresh_tmp_named("scr");
                self.var_types.insert(scr_tmp.clone(), scr_ty.clone());
                self.line(&format!("{} {} = {};", scr_ty, scr_tmp, scr));
                if let Some(elem_tys) = self.tuple_element_types.get(scr.as_str()).cloned() {
                    self.tuple_element_types.insert(scr_tmp.clone(), elem_tys);
                }

                // Infer result type from then block
                let result_ty = then.trailing.as_ref()
                    .map(|e| self.infer_expr_c_type(e))
                    .unwrap_or_else(|| "nova_unit".into());
                let result_tmp = self.fresh_tmp_named("if_let");
                self.line(&format!("{} {};", result_ty, result_tmp));

                let cond = self.pattern_cond(pattern, &scr_tmp)?;
                self.line(&format!("if ({}) {{", cond));
                self.indent += 1;
                self.pattern_bind_typed(pattern, &scr_tmp)?;
                let then_block_id = self.enter_defer_scope(then, false);
                for stmt in &then.stmts { self.emit_stmt(stmt)?; }
                if let Some(trailing) = &then.trailing {
                    let v = self.emit_expr(trailing)?;
                    self.line(&format!("{} = {};", result_tmp, v));
                }
                self.leave_defer_scope(then_block_id);
                self.indent -= 1;
                match else_ {
                    Some(ElseBranch::Block(b)) => {
                        self.line("} else {");
                        self.indent += 1;
                        // Plan 20 Ф.4/Ф.8: else-branch body — defer scope.
                        // Trailing value присваивается ПОСЛЕ defer cleanup
                        // (defer body не должен влиять на результат branch).
                        let block_id = self.enter_defer_scope(b, false);
                        for stmt in &b.stmts { self.emit_stmt(stmt)?; }
                        if let Some(trailing) = &b.trailing {
                            let v = self.emit_expr(trailing)?;
                            self.line(&format!("{} = {};", result_tmp, v));
                        }
                        self.leave_defer_scope(block_id);
                        self.indent -= 1;
                        self.line("}");
                    }
                    Some(ElseBranch::If(e)) => {
                        self.line("} else {");
                        self.indent += 1;
                        let v = self.emit_expr(e)?;
                        self.line(&format!("{} = {};", result_tmp, v));
                        self.indent -= 1;
                        self.line("}");
                    }
                    None => {
                        self.line("}");
                    }
                }
                Ok(result_tmp)
            }
            ExprKind::WhileLet { pattern, scrutinee, body, .. } => {
                // while let Pat = expr { body }
                // → loop: evaluate scrutinee, if pattern matches bind and run body, else break
                let loop_tmp = self.fresh_tmp_named("while_let");
                self.line(&format!("nova_unit {};", loop_tmp));
                self.line("while (1) {");
                self.indent += 1;
                let scr = self.emit_expr(scrutinee)?;
                let scr_ty = self.infer_expr_c_type(scrutinee);
                let scr_tmp = self.fresh_tmp_named("scr");
                self.var_types.insert(scr_tmp.clone(), scr_ty.clone());
                self.line(&format!("{} {} = {};", scr_ty, scr_tmp, scr));
                if let Some(elem_tys) = self.tuple_element_types.get(scr.as_str()).cloned() {
                    self.tuple_element_types.insert(scr_tmp.clone(), elem_tys);
                }
                let cond = self.pattern_cond(pattern, &scr_tmp)?;
                self.line(&format!("if (!({cond})) break;"));
                self.pattern_bind_typed(pattern, &scr_tmp)?;
                // Plan 20 Ф.4/Ф.8: defer внутри while-let body на каждой итерации.
                self.emit_loop_body_inline(body)?;
                self.indent -= 1;
                self.line("}");
                self.line(&format!("{} = NOVA_UNIT;", loop_tmp));
                Ok(loop_tmp)
            }
            // D.1.3: квантор — только в контрактах, не в runtime-коде.
            ExprKind::Forall { .. } | ExprKind::Exists { .. } => {
                Err("forall/exists quantifiers are contract-only and cannot be compiled to C".into())
            }
        }
    }

    // ---- call emission ----

    /// Wrapper for emit_call that handles trailing blocks.
    /// A trailing block is emitted as a static C function and passed as an extra argument.
    fn emit_call_with_trailing(&mut self, func: &Expr, args: &[CallArg], trailing: Option<&TrailingBlock>) -> Result<String, String> {
        // D38 turbofish: для trailing-block пути нужен stripped func (Ident/Path)
        // для infer_trailing_block_sig / infer_func_c_name. Для обычного пути —
        // передаём оригинал в emit_call, чтобы TurboFish type_args дошли до
        // generic-fn dispatch (строка ~9975) и resolve_mono_type_args.
        let func_stripped = func.unwrap_turbofish();
        if let Some(tb) = trailing {
            // Generate a unique name for the trailing block function
            let id = self.trailing_block_counter;
            self.trailing_block_counter += 1;
            let fn_name = format!("nova_trailing_block_{}", id);

            // Determine return type from current function's return type context
            // (trailing block inherits return type of its enclosing call's expected type).
            // For simplicity: look up fn-param signature for the function being called.
            let (param_c_tys, ret_c_ty) = self.infer_trailing_block_sig(func_stripped, tb);

            // Trailing block body function takes (void* env, params...) like closures
            let body_param_list: String = {
                let mut parts = vec!["void* _env_ptr".to_string()];
                for (p, ty) in tb.params.iter().zip(param_c_tys.iter()) {
                    parts.push(format!("{} {}", ty, p.name));
                }
                parts.join(", ")
            };

            // Emit the trailing block function into deferred_impls (body function takes env + params)
            let old_out = std::mem::replace(&mut self.out, String::new());
            let old_indent = self.indent;
            self.indent = 0;
            self.line(&format!("static {} {}({}) {{", ret_c_ty, fn_name, body_param_list));
            self.indent += 1;
            // Register trailing block params in var_types
            let saved: Vec<(String, Option<String>)> = tb.params.iter().zip(param_c_tys.iter())
                .map(|(p, ty)| (p.name.clone(), self.var_types.insert(p.name.clone(), ty.clone())))
                .collect();
            // Emit body
            let block = &tb.body;
            let _ = self.emit_block_stmts_trailing(block, &ret_c_ty);
            // Restore param types
            for (name, prev) in saved {
                match prev {
                    Some(old) => { self.var_types.insert(name, old); }
                    None => { self.var_types.remove(&name); }
                }
            }
            self.indent -= 1;
            self.line("}");
            let block_body = std::mem::replace(&mut self.out, old_out);
            self.indent = old_indent;
            // Append to deferred_impls
            let _ = write!(self.deferred_impls, "\n{}", block_body);
            // Emit a forward declaration for the body function.
            // ВАЖНО: fwd-декларация должна быть **file-scope**, не внутри
            // функции — Clang/GCC корректно отвергают `static foo(void);`
            // в block scope (C99 §6.2.2¶7: block-scope decl со storage-class
            // `static` для функций не допускается). MSVC исторически принимает,
            // но это не portable. Кладём в `lambda_forward_decls` — тот же
            // буфер что и для closure-lambda fwd-декл'ов, эмитится file-scope
            // перед всеми fn-телами.
            let fwd = format!("static {} {}({});\n", ret_c_ty, fn_name, body_param_list);
            self.lambda_forward_decls.push_str(&fwd);

            // Wrap in a NovaClos_XX struct so fn_param_sigs call mechanism works
            let clos_struct = Self::clos_struct_name(&param_c_tys, &ret_c_ty);
            let clos_fn_ty = Self::clos_fn_ty(&param_c_tys, &ret_c_ty);
            let clos_tmp = self.fresh_tmp();
            self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", clos_struct, clos_tmp, clos_struct, clos_struct));
            self.line(&format!("{}->{} = ({})({});", clos_tmp, "fn", clos_fn_ty, fn_name));
            self.line(&format!("{}->{} = (void*)0;", clos_tmp, "env"));

            // For Member/Path calls (method calls), route through emit_call so that
            // the receiver is properly prepended and method dispatch resolves correctly.
            // The closure is passed as an additional synthetic argument (an Ident expr
            // referencing the just-emitted closure variable).
            if matches!(func_stripped.kind, ExprKind::Member { .. } | ExprKind::Path(_)) {
                // Register the closure variable type so infer_expr_c_type returns
                // a pointer type (which emit_call then casts to void* when needed).
                let prev_ty = self.var_types.insert(
                    clos_tmp.clone(),
                    format!("{}*", clos_struct),
                );
                let synthetic = CallArg::Item(Expr::new(
                    ExprKind::Ident(clos_tmp.clone()),
                    tb.span,
                ));
                let mut extended: Vec<CallArg> = args.to_vec();
                extended.push(synthetic);
                let result = self.emit_call(func, &extended);
                // Restore var_types so the synthetic name doesn't leak.
                match prev_ty {
                    Some(t) => { self.var_types.insert(clos_tmp.clone(), t); }
                    None => { self.var_types.remove(&clos_tmp); }
                }
                return result;
            }

            // Free function call (Ident): emit the call with closure pointer as extra arg
            let func_c = self.infer_func_c_name(func_stripped);
            let mut arg_strs: Vec<String> = Vec::new();
            for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
            arg_strs.push(format!("(void*)({})", clos_tmp));
            return Ok(format!("{}({})", func_c, arg_strs.join(", ")));
        }
        self.emit_call(func, args)
    }

    /// Infer the C signature (param types, return type) for a trailing block based on the called function.
    fn infer_trailing_block_sig(&self, func: &Expr, tb: &TrailingBlock) -> (Vec<String>, String) {
        // Look up the function being called to get the fn-param type info
        let fn_name = match &func.kind {
            ExprKind::Ident(n) => Some(n.clone()),
            _ => None,
        };
        if let Some(name) = fn_name {
            // Look up fn_ret_{name} for return type context
            let _ret_key = format!("fn_ret_{}", name);
            // Try to find the signature of the last parameter of this function
            // (the trailing block is always the last parameter)
            // We stored this in fn_param_sigs when registering the fn itself... but that's for calling functions
            // Look in var_types for function parameter names of the callee's last fn-typed param
        }
        // Default: infer param types from trailing block params (assume nova_int)
        let param_tys: Vec<String> = tb.params.iter()
            .map(|p| p.ty.as_ref()
                .and_then(|t| self.type_ref_to_c(t).ok())
                .unwrap_or_else(|| "nova_int".into()))
            .collect();
        // Default return type: nova_int (most common for blocks that return values)
        let ret_ty = "nova_int".to_string();
        (param_tys, ret_ty)
    }

    /// Infer the C function name for a call expression (without emitting args).
    fn infer_func_c_name(&self, func: &Expr) -> String {
        match &func.kind {
            ExprKind::Ident(name) => {
                if let Some((type_name, _)) = self.find_variant(name) {
                    format!("nova_make_{}_{}", type_name, name)
                } else {
                    format!("nova_fn_{}", name)
                }
            }
            _ => "nova_fn_unknown".into(),
        }
    }

    /// Emit block statements and trailing expression with explicit return type.
    fn emit_block_stmts_trailing(&mut self, block: &Block, ret_ty: &str) -> Result<(), String> {
        let block_id = self.enter_defer_scope(block, false);
        for stmt in &block.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &block.trailing {
            let v = self.emit_expr(trailing)?;
            if ret_ty == "nova_unit" {
                self.line(&format!("{};", v));
                self.leave_defer_scope(block_id);
                self.line("return NOVA_UNIT;");
            } else {
                let tmp = self.fresh_tmp();
                self.line(&format!("{} {} = {};", ret_ty, tmp, v));
                self.leave_defer_scope(block_id);
                self.line(&format!("return {};", tmp));
            }
        } else {
            self.leave_defer_scope(block_id);
            if ret_ty == "nova_unit" {
                self.line("return NOVA_UNIT;");
            }
        }
        Ok(())
    }

    fn emit_call(&mut self, func: &Expr, args: &[CallArg]) -> Result<String, String> {
        // Plan 14 Ф.6 (D69): variadic-routing.
        //
        // Если вызываемая fn имеет variadic-параметр на последней позиции
        // — args[regular_arity..] собираются в синтезированный ArrayLit
        // (с поддержкой `...spread` через ArrayElem::Spread, который уже
        // умеет emit_array_lit для D60).
        //
        // Затем перепишем args: первые `regular_arity` штук как-есть,
        // плюс один синтезированный аргумент (массив). Дальше обычный
        // emit-flow.
        //
        // Если fn НЕ variadic, но args содержит `Spread` — compile error.
        let variadic_arity: Option<usize> = self.lookup_variadic_arity(func);
        if let Some(regular_arity) = variadic_arity {
            // Гард: больше regular args чем у fn — undefined behavior.
            // (variadic-args начинаются с regular_arity-индекса.)
            if args.len() < regular_arity {
                return Err(format!(
                    "variadic call: ожидалось минимум {} regular-аргумента(ов), получено {}",
                    regular_arity, args.len()));
            }
            // Variadic-position args собираем в ArrayLit.
            // Plan 46 (D102): Named в variadic-position не попадает —
            // argbind ловит NamedForVariadic. Arm для exhaustiveness.
            let var_elems: Vec<ArrayElem> = args[regular_arity..].iter().map(|a| match a {
                CallArg::Item(e) => ArrayElem::Item(e.clone()),
                CallArg::Spread(e) => ArrayElem::Spread(e.clone()),
                CallArg::Named { value, .. } => ArrayElem::Item(value.clone()),
            }).collect();
            let synth_array = Expr {
                kind: ExprKind::ArrayLit(var_elems),
                span: func.span,
            };
            let mut new_args: Vec<CallArg> = args[..regular_arity].to_vec();
            new_args.push(CallArg::Item(synth_array));
            // Spread в regular-position — error.
            for (i, a) in new_args.iter().enumerate() {
                if i < regular_arity && a.is_spread() {
                    return Err("spread (...) разрешён только в variadic-позиции".into());
                }
            }
            // Recurse с переписанными args (variadic-флаг очищен через
            // synthesized array — fn видит обычный []T parameter).
            let saved = std::mem::replace(&mut self.suppress_variadic_routing, true);
            let result = self.emit_call(func, &new_args);
            self.suppress_variadic_routing = saved;
            return result;
        }
        // Non-variadic call: spread args не разрешены.
        if !self.suppress_variadic_routing {
            for a in args {
                if a.is_spread() {
                    return Err(format!(
                        "spread (...) на call-site разрешён только для variadic-fn (D69). \
                         Функция `{}` не variadic.",
                        Self::expr_to_display(func)));
                }
            }
        }
        // Special case: println / print builtins
        if let ExprKind::Ident(name) = &func.kind {
            if name == "println" || name == "print" {
                return self.emit_println(args, name == "println");
            }
            // D70 `to_str(x)` builtin removed (REPLACED → D73). String
            // conversion now via `str.from(x)` / `x.@into()` (with str-context).
            // assert(cond) / debug_assert(cond) → nova_assert(cond, "condition text").
            // По D81: assert — always runtime; debug_assert — debug-only,
            // в release no-op. В bootstrap'е (single build-mode) оба
            // эмитятся одинаково; production-runtime добавит conditional
            // compilation для debug_assert (например, через NDEBUG-style
            // пре-процессор или codegen-флаг).
            if name == "assert" || name == "debug_assert" {
                if let Some(cond_arg) = args.first() {
                    let cond_expr = cond_arg.expr();
                    let cond_val = self.emit_expr(cond_expr)?;
                    let cond_text = Self::expr_to_display(cond_expr);
                    let escaped_text = Self::escape_c_str(&cond_text);
                    return Ok(format!("nova_assert({}, \"{}\")", cond_val, escaped_text));
                }
            }
            // panic(msg str) -> Never — D13: смерть текущего fiber'а.
            // Routes через NovaFailFrame внутри fiber'а, через NovaTestFrame
            // в тестах, иначе — stderr + abort. См. nv_panic в effects.h.
            //
            // Эмитируется через comma-expression `(nv_panic(msg), 0)` —
            // правая часть — dummy nova_int нужного типа для совместимости
            // с C-каст'ами в caller'е. nv_panic имеет C-сигнатуру void и
            // никогда не возвращается (longjmp/abort), но C требует
            // value-expression для cast-target — comma operator решает это
            // без нарушения short-circuit семантики родительских конструкций
            // (?? coalesce, тернарник): nv_panic вызывается только когда
            // выражение реально evaluates, не безусловно как у statement+dummy.
            if name == "panic" {
                if args.len() != 1 {
                    return Err(format!(
                        "panic expects 1 argument (msg str), got {}",
                        args.len()));
                }
                let msg_val = self.emit_expr(args[0].expr())?;
                return Ok(format!("(nv_panic({}), (nova_int)0LL)", msg_val));
            }
            // exit(code int, msg str) -> Never — D13: смерть всего процесса.
            // НЕ перехватывается handler'ом. В тестах routes через NovaTestFrame
            // (test-runner-level), в production — exit(code). См. nv_exit.
            // Тот же comma-expression паттерн что и для panic.
            if name == "exit" {
                if args.len() != 2 {
                    return Err(format!(
                        "exit expects 2 arguments (code int, msg str), got {}",
                        args.len()));
                }
                let code_val = self.emit_expr(args[0].expr())?;
                let msg_val = self.emit_expr(args[1].expr())?;
                return Ok(format!("(nv_exit({}, {}), (nova_int)0LL)", code_val, msg_val));
            }
        }

        // Mangle user-defined function calls: `foo(...)` → `nova_fn_foo(...)`
        // But variant constructors: `Circle(r)` → `nova_make_Shape_Circle(r)`
        // And effect operations: `Counter.next()` → `Nova_Counter_next()`
        // Check if func is a function-typed parameter (closure call via NovaClos_XX macro)
        if let ExprKind::Ident(name) = &func.kind {
            if let Some((param_tys, ret_ty)) = self.fn_param_sigs.get(name).cloned() {
                let mut arg_strs = Vec::new();
                for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                // Plan 48 Ф.4 ([M-spawn-closure-capture-mono]): when this
                // closure call lives inside a spawn-body whose parent fn is
                // monomorphized, the fn-param identifier (`body`) is actually
                // captured into the spawn ctx. We must reach it via `_c->body`
                // (by-value) or `(*_c->body)` (by-pointer) — same rewrite that
                // emit_expr(Ident) does for plain reads. Without this, the
                // generated C references an undeclared identifier in the
                // spawn-entry function.
                let callee_expr = self.spawn_capture_access(name);
                let callee_str = callee_expr.as_deref().unwrap_or(name.as_str());
                // Determine which NovaClos macro to use based on (params, ret) types
                let macro_name = Self::clos_call_macro(&param_tys, &ret_ty);
                return match macro_name {
                    Some(m) => {
                        if arg_strs.is_empty() {
                            Ok(format!("{}({})", m, callee_str))
                        } else {
                            Ok(format!("{}({}, {})", m, callee_str, arg_strs.join(", ")))
                        }
                    }
                    None => {
                        // Plan 11 Ф.4: arbitrary-signature closure call. f points to
                        // a struct { fn_ptr; void* env } (NovaClosBase). Extract fn,
                        // cast to (ret(*)(void*, params...)), pass env + args.
                        let mut cast_params = vec!["void*".to_string()];
                        cast_params.extend(param_tys.iter().cloned());
                        let cast_params_str = cast_params.join(", ");
                        let mut all_args = vec![format!("((NovaClosBase*)({}))->env", callee_str)];
                        all_args.extend(arg_strs.iter().cloned());
                        Ok(format!("(({ret}(*)({params}))(((NovaClosBase*)({n}))->fn))({args})",
                            ret = ret_ty,
                            params = cast_params_str,
                            n = callee_str,
                            args = all_args.join(", ")))
                    }
                };
            }
        }

        let func_c = match &func.kind {
            ExprKind::Ident(name) => {
                if let Some((type_name, _)) = self.find_variant(name) {
                    // Plan 48 Ф.7.4 (partial): when the parent sum-type is
                    // generic and we can infer all type-args from the call args
                    // (e.g. `Ok2(42)` → Result2[nova_int]), route through the
                    // mono pipeline so the receiver gets a concrete C struct
                    // type (`Nova_Result2____nova_int*`) and method calls hit
                    // the mono dispatch path. Otherwise fall back to the erased
                    // constructor.
                    if let Some((_, mangled, _)) =
                        self.try_infer_variant_mono_args(name, args)
                    {
                        format!("nova_make_{}_{}", mangled, name)
                    } else {
                        format!("nova_make_{}_{}", type_name, name)
                    }
                } else {
                    // D84: free-function overload resolution. Если в registry
                    // несколько overloads — резолвим по статическим типам args.
                    let key = ("".to_string(), name.clone());
                    let overloads = self.method_overloads.get(&key).cloned();
                    if let Some(overloads) = overloads {
                        if overloads.len() > 1 {
                            // Соберём C-типы аргументов через infer_expr_c_type.
                            let arg_c_types: Vec<String> = args.iter()
                                .map(|a| self.infer_expr_c_type(a.expr()))
                                .collect();
                            // Filter по arity + param-types (strict matching,
                            // как в resolve_overload для методов).
                            let matches: Vec<&MethodSig> = overloads.iter()
                                .filter(|sig| sig.param_c_types.len() == arg_c_types.len())
                                .filter(|sig| sig.param_c_types.iter()
                                    .zip(arg_c_types.iter())
                                    .all(|(want, got)| want == got))
                                .collect();
                            match matches.len() {
                                1 => matches[0].c_name.clone(),
                                0 => return Err(format!(
                                    "no matching overload for `{}({})` — \
                                     candidates: {}",
                                    name,
                                    arg_c_types.join(", "),
                                    overloads.iter()
                                        .map(|s| format!("{}({})", name, s.param_c_types.join(", ")))
                                        .collect::<Vec<_>>().join(" | "))),
                                _ => return Err(format!(
                                    "ambiguous overload for `{}({})` — multiple candidates match: {}",
                                    name,
                                    arg_c_types.join(", "),
                                    matches.iter()
                                        .map(|s| s.c_name.clone())
                                        .collect::<Vec<_>>().join(" | "))),
                            }
                        } else {
                            // Single overload — короткое имя (backward compat).
                            format!("nova_fn_{}", name)
                        }
                    } else {
                        // Не зарегистрирована в registry (тесты, prelude builtins).
                        format!("nova_fn_{}", name)
                    }
                }
            }
            ExprKind::Member { obj, name: method } => {
                // Plan 11 Ф.4.5: D66 — `Self.method(...)` в expression
                // position. obj=Ident("Self") в теле метода → Ident(<current>).
                // Создаем local rebind, не мутируя обратно.
                let self_obj_storage;
                let obj: &Expr = match &obj.kind {
                    ExprKind::Ident(n) if n == "Self" => {
                        if let Some(recv) = &self.current_receiver_type {
                            self_obj_storage = Expr {
                                kind: ExprKind::Ident(recv.clone()),
                                span: obj.span,
                            };
                            &self_obj_storage
                        } else {
                            obj
                        }
                    }
                    _ => obj,
                };
                // Plan 14 Ф.4: если obj — record и `method` это fn-typed поле
                // (записано в record_field_fn_sigs), эмитим closure-call
                // через NOVA_CLOS_CALL_* macro.
                {
                    let obj_ty = self.infer_expr_c_type(obj);
                    if let Some(record_name) = obj_ty
                        .strip_prefix("Nova_")
                        .map(|s| s.trim_end_matches('*').to_string())
                    {
                        let key = (record_name.clone(), method.clone());
                        if let Some((param_tys, ret_ty)) = self.record_field_fn_sigs.get(&key).cloned() {
                            let o = self.emit_expr(obj)?;
                            let mut arg_strs = Vec::new();
                            for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                            let field_mangled = Self::mangle_field_name(method);
                            let f_expr = format!("({}->{})", o, field_mangled);
                            let macro_name = Self::clos_call_macro(&param_tys, &ret_ty);
                            return match macro_name {
                                Some(m) => {
                                    if arg_strs.is_empty() {
                                        Ok(format!("{}({})", m, f_expr))
                                    } else {
                                        Ok(format!("{}({}, {})", m, f_expr, arg_strs.join(", ")))
                                    }
                                }
                                None => {
                                    // Generic closure-call через NovaClosBase.
                                    let mut cast_params = vec!["void*".to_string()];
                                    cast_params.extend(param_tys.iter().cloned());
                                    let cps = cast_params.join(", ");
                                    let mut all_args = vec![format!("((NovaClosBase*)({}))->env", f_expr)];
                                    all_args.extend(arg_strs.iter().cloned());
                                    Ok(format!(
                                        "(({ret}(*)({params}))(((NovaClosBase*)({fc}))->fn))({args})",
                                        ret = ret_ty,
                                        params = cps,
                                        fc = f_expr,
                                        args = all_args.join(", ")
                                    ))
                                }
                            };
                        }
                    }
                }
                // D38 array-static-method: `[]T.new()` / `[]T.with_capacity(n)`.
                // Парсер строит obj = Path(["__array", "<T>"]); здесь
                // диспетчеризуем в `nova_array_new_<T>` runtime.
                if let ExprKind::Path(parts) = &obj.kind {
                    if parts.len() == 2 && parts[0] == "__array" {
                        let elem_t = &parts[1];
                        // Mapping Nova-type → NovaArray storage suffix.
                        let arr_suffix = match elem_t.as_str() {
                            "str"            => "nova_str",
                            "byte" | "u8"    => "nova_byte",
                            "bool"           => "nova_bool",
                            "f64" | "f32"    => "nova_f64",
                            // int / другие типы — через nova_int slot.
                            _                => "nova_int",
                        };
                        match method.as_str() {
                            "new" => {
                                // []T.new() → nova_array_new_<T>(default_cap=8)
                                return Ok(format!("nova_array_new_{}(8)", arr_suffix));
                            }
                            "with_capacity" => {
                                if let Some(arg) = args.first() {
                                    let v = self.emit_expr(arg.expr())?;
                                    return Ok(format!("nova_array_new_{}({})", arr_suffix, v));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                // D75 (revised, Plan 47): built-in methods on NovaCancelToken*.
                // Plan 49 Ф.1: `cancel(reason)` принимает optional str reason;
                // `reason() -> Option[str]` возвращает причину отмены.
                {
                    let obj_ty = self.infer_expr_c_type(obj);
                    if obj_ty == "NovaCancelToken*" {
                        let obj_c = self.emit_expr(obj)?;
                        match method.as_str() {
                            "cancel" => {
                                // Plan 49 Ф.1 + Ф.6: optional reason argument.
                                // Type-aware boxing:
                                //   T=str           → nova_cancel_box_str (existing).
                                //   T=pointer       → cast напрямую в void* (передаём ptr).
                                //   T=primitive/struct value
                                //                   → compound literal + memcpy через
                                //                     nova_cancel_box_copy_raw.
                                // Без аргументов — default "cancelled" str (T=str default).
                                if let Some(reason_arg) = args.first() {
                                    let reason_c = self.emit_expr(reason_arg.expr())?;
                                    let arg_ty = self.infer_expr_c_type(reason_arg.expr());
                                    let boxed = if arg_ty == "nova_str" {
                                        format!("nova_cancel_box_str({})", reason_c)
                                    } else if arg_ty.ends_with('*') {
                                        // Pointer types (record/sum/array) — pass as-is.
                                        format!("(void*)({})", reason_c)
                                    } else {
                                        // Primitive/value-struct — compound literal + box.
                                        format!(
                                            "nova_cancel_box_copy_raw(&(({}){{ {} }}), (int64_t)sizeof({}))",
                                            arg_ty, reason_c, arg_ty
                                        )
                                    };
                                    return Ok(format!(
                                        "(nova_cancel_token_cancel_reason({}, {}), NOVA_UNIT)",
                                        obj_c, boxed
                                    ));
                                }
                                return Ok(format!(
                                    "(nova_cancel_token_cancel_reason({}, nova_cancel_box_str((nova_str){{.ptr=\"cancelled\",.len=9}})), NOVA_UNIT)",
                                    obj_c));
                            }
                            "is_cancelled" => {
                                return Ok(format!("nova_cancel_token_is_cancelled({})", obj_c));
                            }
                            "reason" => {
                                // Plan 49 Ф.6 P0 fix: per-T un-box когда
                                // receiver — tracked CancelToken[T] переменная.
                                // Без этого default str-getter возвращает
                                // Option[str] с garbage content для T≠str
                                // (silent UB).
                                let t_c = if let ExprKind::Ident(name) = &obj.kind {
                                    self.cancel_token_t_map.get(name).cloned()
                                } else { None };
                                if let Some(t_c) = t_c {
                                    if t_c != "nova_str" {
                                        // Plan 54 Ф.9: read-back зависит от
                                        // pointer/value T:
                                        //   T pointer (Nova_X*) — reason_ptr
                                        //     УЖЕ is Nova_X* (cancel() сделал
                                        //     `(void*)(ptr_value)` без box).
                                        //     Cast: `(Nova_X*)reason_raw(tok)`.
                                        //   T value (nova_int, etc) — boxed
                                        //     via box_copy_raw на heap; read
                                        //     `*(T*)reason_raw(tok)`.
                                        // NovaOpt typedef emit'ится через
                                        // register_novaopt_decl.
                                        let sanitized = Self::sanitize_c_for_ident(&t_c);
                                        self.register_novaopt_decl(&sanitized, &t_c);
                                        let read_back = if t_c.ends_with('*') {
                                            format!("({})nova_cancel_token_reason_raw({})",
                                                t_c, obj_c)
                                        } else {
                                            format!("*({}*)nova_cancel_token_reason_raw({})",
                                                t_c, obj_c)
                                        };
                                        return Ok(format!(
                                            "(nova_cancel_token_has_reason({tok}) \
                                              ? (NovaOpt_{sn}){{ .tag = NOVA_TAG_Option_Some, .value = {rb} }} \
                                              : (NovaOpt_{sn}){{ .tag = NOVA_TAG_Option_None }})",
                                            tok = obj_c, rb = read_back, sn = sanitized
                                        ));
                                    }
                                }
                                // Default / fallback: T=str (или unknown — backward-compat).
                                return Ok(format!(
                                    "nova_cancel_token_reason_str({})", obj_c));
                            }
                            "merge" => {
                                // Plan 49 P3: `tok1.merge(tok2)` — композиция.
                                // Возвращает новый CancelToken который cancel'ится
                                // когда любой из источников cancel'ится. Same-T
                                // в V1 (cross-T merge — V2 нужны converter pair).
                                if let Some(other_arg) = args.first() {
                                    let other_c = self.emit_expr(other_arg.expr())?;
                                    return Ok(format!(
                                        "nova_cancel_token_merge2({}, {})",
                                        obj_c, other_c
                                    ));
                                }
                            }
                            "cancelled_by" => {
                                // `child.cancelled_by(parent)` — направленный
                                // каскад: parent.cancel() отменяет и child.
                                // Plan 49 Ф.6 cross-type: если оба тока имеют
                                // tracked T (cancel_token_t_map) И T различны,
                                // требуется `A: From[B]` и codegen эмитит
                                // converter wrapper + bind_cascade_typed.
                                // Same-type или unknown → existing bind_cascade.
                                if let Some(parent_arg) = args.first() {
                                    let parent_c = self.emit_expr(parent_arg.expr())?;
                                    let child_t = if let ExprKind::Ident(n) = &obj.kind {
                                        self.cancel_token_t_map.get(n).cloned()
                                    } else { None };
                                    let parent_t = if let ExprKind::Ident(n) = &parent_arg.expr().kind {
                                        self.cancel_token_t_map.get(n).cloned()
                                    } else { None };
                                    if let (Some(a_c), Some(b_c)) = (child_t.clone(), parent_t.clone()) {
                                        if a_c != b_c {
                                            // Cross-type: compile-time check `A: From[B]`.
                                            let a_nova = Self::c_type_to_nova_name(&a_c);
                                            let b_nova = Self::c_type_to_nova_name(&b_c);
                                            let from_ok = self.from_targets
                                                .get(&a_nova)
                                                .map(|vs| vs.iter().any(|v| v == &b_nova))
                                                .unwrap_or(false);
                                            if !from_ok {
                                                return Err(format!(
                                                    "cross-type cascade `CancelToken[{}].cancelled_by(CancelToken[{}])` \
                                                     requires `{}.from({})` to be defined \
                                                     (D73 `From` protocol); add `fn {}.from(v {}) -> Self` или \
                                                     используйте same-type cascade",
                                                    a_nova, b_nova, a_nova, b_nova, a_nova, b_nova
                                                ));
                                            }
                                            // Generate converter wrapper в lambda_impls.
                                            let conv_name = format!(
                                                "_nova_cancel_conv_{}_from_{}",
                                                Self::sanitize_c_for_ident(&a_c),
                                                Self::sanitize_c_for_ident(&b_c)
                                            );
                                            // Module-wide dedup — emitted_cancel_converters
                                            // в отличие от substring-check на lambda_impls
                                            // которая очищается между fn-bodies.
                                            if self.emitted_cancel_converters.insert(conv_name.clone()) {
                                                // Wrapper-pattern зависит от того pointer
                                                // или value B/A:
                                                //   B value (nova_int/str/bool): unbox `*(B*)_b_ptr`.
                                                //   B pointer (Nova_X*): cast `(B)_b_ptr`.
                                                //   A pointer (Nova_X*): cast result `(void*)A_ptr`.
                                                //   A value: heap-box.
                                                // Static method name берётся из Nova type
                                                // name (без Nova_ prefix и без *).
                                                let b_is_ptr = b_c.ends_with('*');
                                                let a_is_ptr = a_c.ends_with('*');
                                                let unbox_b = if b_is_ptr {
                                                    format!("({}){}_b_ptr", b_c, "")
                                                } else {
                                                    format!("*({}*)_b_ptr", b_c)
                                                };
                                                let a_short = a_nova.clone();  // Nova-level name для static dispatch
                                                let call_from = format!("Nova_{}_static_from(b)", a_short);
                                                let body = if a_is_ptr {
                                                    format!(
                                                        "    {b} b = {ub};\n    \
                                                           return (void*){call};\n",
                                                        b = b_c, ub = unbox_b, call = call_from)
                                                } else {
                                                    format!(
                                                        "    {b} b = {ub};\n    \
                                                           {a} a = {call};\n    \
                                                           {a}* boxed = ({a}*)nova_alloc(sizeof({a}));\n    \
                                                           *boxed = a;\n    \
                                                           return (void*)boxed;\n",
                                                        b = b_c, ub = unbox_b, a = a_c, call = call_from)
                                                };
                                                let wrapper = format!(
                                                    "static void* {conv}(void* _b_ptr) {{\n{body}}}\n",
                                                    conv = conv_name, body = body
                                                );
                                                self.lambda_forward_decls.push_str(
                                                    &format!("static void* {}(void*);\n", conv_name));
                                                self.lambda_impls.push_str(&wrapper);
                                            }
                                            return Ok(format!(
                                                "(nova_cancel_token_bind_cascade_typed({}, {}, &{}), NOVA_UNIT)",
                                                obj_c, parent_c, conv_name
                                            ));
                                        }
                                    }
                                    // Same-type / unknown — backward-compat.
                                    return Ok(format!(
                                        "(nova_cancel_token_bind_cascade({}, {}), NOVA_UNIT)",
                                        obj_c, parent_c
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                    // D26 prelude: built-in methods on Option (NovaOpt_T) and Result.
                    if obj_ty.starts_with("NovaOpt_") {
                        let elem_ty = obj_ty.strip_prefix("NovaOpt_")
                            .unwrap_or("nova_int")
                            .trim_end_matches('*')
                            .trim()
                            .to_string();
                        let obj_c = self.emit_expr(obj)?;
                        match method.as_str() {
                            "is_some" => return Ok(format!(
                                "Nova_Option_method_is_some_{}({})", elem_ty, obj_c)),
                            "is_none" => return Ok(format!(
                                "Nova_Option_method_is_none_{}({})", elem_ty, obj_c)),
                            "unwrap_or" => {
                                if let Some(arg) = args.first() {
                                    let v = self.emit_expr(arg.expr())?;
                                    return Ok(format!(
                                        "Nova_Option_method_unwrap_or_{}({}, {})",
                                        elem_ty, obj_c, v));
                                }
                            }
                            "unwrap" => {
                                // Inline check + Nova_Fail_fail on None
                                let tmp = self.fresh_tmp();
                                self.line(&format!("NovaOpt_{} {} = {};", elem_ty, tmp, obj_c));
                                self.line(&format!("if ({}.tag == NOVA_TAG_Option_None) {{", tmp));
                                self.indent += 1;
                                self.line("Nova_Fail_fail((nova_str){.ptr=\"called unwrap on None\", .len=21});");
                                self.indent -= 1;
                                self.line("}");
                                return Ok(format!("({}.value)", tmp));
                            }
                            // D26 prelude: Option.unwrap_or_else(f).
                            // Some(v) → v, None → f() (zero-arg closure).
                            "unwrap_or_else" => {
                                if let Some(arg) = args.first() {
                                    let f = self.emit_expr(arg.expr())?;
                                    let tmp = self.fresh_tmp();
                                    self.line(&format!("NovaOpt_{} {} = {};", elem_ty, tmp, obj_c));
                                    let result = self.fresh_tmp();
                                    self.line(&format!("{} {};", elem_ty, result));
                                    self.line(&format!("if ({}.tag == NOVA_TAG_Option_Some) {{", tmp));
                                    self.indent += 1;
                                    self.line(&format!("{} = {}.value;", result, tmp));
                                    self.indent -= 1;
                                    self.line("} else {");
                                    self.indent += 1;
                                    // Closure без аргументов (() → T): NovaClos_vi
                                    // (signature `T(*)(void*)`).
                                    self.line(&format!(
                                        "{} = (({}(*)(void*))(((NovaClos_vi*)({}))->fn))(((NovaClos_vi*)({}))->env);",
                                        result, elem_ty, f, f));
                                    self.indent -= 1;
                                    self.line("}");
                                    return Ok(result);
                                }
                            }
                            // D26 prelude: Option.map(f).
                            // Some(v) → Some(f(v)), None → None.
                            "map" => {
                                if let Some(arg) = args.first() {
                                    let f = self.emit_expr(arg.expr())?;
                                    let tmp = self.fresh_tmp();
                                    self.line(&format!("NovaOpt_{} {} = {};", elem_ty, tmp, obj_c));
                                    let out = self.fresh_tmp();
                                    self.line(&format!("NovaOpt_{} {};", elem_ty, out));
                                    self.line(&format!("if ({}.tag == NOVA_TAG_Option_Some) {{", tmp));
                                    self.indent += 1;
                                    // Closure (T → T): берём `nova_int(*)(void*, nova_int)`
                                    // signature через ручной cast (NovaClos_ii layout
                                    // совпадает в bootstrap'е для одинаковых-T-параметров).
                                    let mapped = self.fresh_tmp();
                                    self.line(&format!(
                                        "{} {} = (({}(*)(void*, {}))(((NovaClos_ii*)({}))->fn))(((NovaClos_ii*)({}))->env, {}.value);",
                                        elem_ty, mapped, elem_ty, elem_ty, f, f, tmp));
                                    self.line(&format!("{}.tag = NOVA_TAG_Option_Some;", out));
                                    self.line(&format!("{}.value = {};", out, mapped));
                                    self.indent -= 1;
                                    self.line("} else {");
                                    self.indent += 1;
                                    self.line(&format!("{}.tag = NOVA_TAG_Option_None;", out));
                                    self.line(&format!("{}.value = 0;", out));
                                    self.indent -= 1;
                                    self.line("}");
                                    return Ok(out);
                                }
                            }
                            // D26 prelude: Option.ok_or(e).
                            // Some(v) → Ok(v), None → Err(e).
                            "ok_or" => {
                                if let Some(arg) = args.first() {
                                    let e = self.emit_expr(arg.expr())?;
                                    let tmp = self.fresh_tmp();
                                    self.line(&format!("NovaOpt_{} {} = {};", elem_ty, tmp, obj_c));
                                    let out = self.fresh_tmp();
                                    self.line(&format!("Nova_Result* {};", out));
                                    self.line(&format!("if ({}.tag == NOVA_TAG_Option_Some) {{", tmp));
                                    self.indent += 1;
                                    self.line(&format!("{} = nova_make_Result_Ok((nova_int){}.value);", out, tmp));
                                    self.indent -= 1;
                                    self.line("} else {");
                                    self.indent += 1;
                                    self.line(&format!("{} = nova_make_Result_Err({});", out, e));
                                    self.indent -= 1;
                                    self.line("}");
                                    return Ok(out);
                                }
                            }
                            _ => {}
                        }
                    }
                    if obj_ty == "Nova_Result*" {
                        let obj_c = self.emit_expr(obj)?;
                        match method.as_str() {
                            "is_ok" => return Ok(format!("Nova_Result_method_is_ok({})", obj_c)),
                            "is_err" => return Ok(format!("Nova_Result_method_is_err({})", obj_c)),
                            "ok" => return Ok(format!("Nova_Result_method_ok({})", obj_c)),
                            // D26 prelude: Result.err() → Option[E].
                            // Bootstrap: Err type — nova_str. Возвращаем
                            // NovaOpt_nova_int с интерпретируемой как str
                            // (heap-боксированной) ссылкой. Простой путь:
                            // используем boxed nova_str* из tmp.
                            "err" => {
                                let tmp = self.fresh_tmp();
                                self.line(&format!("Nova_Result* {} = {};", tmp, obj_c));
                                let opt_tmp = self.fresh_tmp();
                                self.line(&format!("NovaOpt_nova_int {};", opt_tmp));
                                self.line(&format!("if ({}->tag == NOVA_TAG_Result_Err) {{", tmp));
                                self.indent += 1;
                                let str_box = self.fresh_tmp();
                                self.line(&format!("nova_str* {} = (nova_str*)nova_alloc(sizeof(nova_str));", str_box));
                                self.line(&format!("*{} = {}->payload.Err._0;", str_box, tmp));
                                self.line(&format!("{}.tag = NOVA_TAG_Option_Some;", opt_tmp));
                                self.line(&format!("{}.value = (nova_int)(intptr_t){};", opt_tmp, str_box));
                                self.indent -= 1;
                                self.line("} else {");
                                self.indent += 1;
                                self.line(&format!("{}.tag = NOVA_TAG_Option_None;", opt_tmp));
                                self.line(&format!("{}.value = 0;", opt_tmp));
                                self.indent -= 1;
                                self.line("}");
                                return Ok(opt_tmp);
                            }
                            "unwrap_or" => {
                                if let Some(arg) = args.first() {
                                    let v = self.emit_expr(arg.expr())?;
                                    return Ok(format!(
                                        "Nova_Result_method_unwrap_or({}, {})", obj_c, v));
                                }
                            }
                            // D26 prelude: Result.unwrap_or_else(f). Err →
                            // f(e), Ok(v) → v. f это closure (nova_str → nova_int).
                            "unwrap_or_else" => {
                                if let Some(arg) = args.first() {
                                    let f = self.emit_expr(arg.expr())?;
                                    let tmp = self.fresh_tmp();
                                    self.line(&format!("Nova_Result* {} = {};", tmp, obj_c));
                                    let result = self.fresh_tmp();
                                    self.line(&format!("nova_int {};", result));
                                    self.line(&format!("if ({}->tag == NOVA_TAG_Result_Ok) {{", tmp));
                                    self.indent += 1;
                                    self.line(&format!("{} = {}->payload.Ok._0;", result, tmp));
                                    self.indent -= 1;
                                    self.line("} else {");
                                    self.indent += 1;
                                    // Closure (nova_str → nova_int): NovaClos_si signature.
                                    self.line(&format!(
                                        "{} = ((nova_int(*)(void*, nova_str))(((NovaClos_ii*)({}))->fn))(((NovaClos_ii*)({}))->env, {}->payload.Err._0);",
                                        result, f, f, tmp));
                                    self.indent -= 1;
                                    self.line("}");
                                    return Ok(result);
                                }
                            }
                            // D26 prelude: Result.map(f). Ok(v) → Ok(f(v)),
                            // Err(e) → Err(e). f это closure (nova_int → nova_int).
                            "map" => {
                                if let Some(arg) = args.first() {
                                    let f = self.emit_expr(arg.expr())?;
                                    let tmp = self.fresh_tmp();
                                    self.line(&format!("Nova_Result* {} = {};", tmp, obj_c));
                                    let out = self.fresh_tmp();
                                    self.line(&format!("Nova_Result* {};", out));
                                    self.line(&format!("if ({}->tag == NOVA_TAG_Result_Ok) {{", tmp));
                                    self.indent += 1;
                                    let mapped = self.fresh_tmp();
                                    self.line(&format!(
                                        "nova_int {} = NOVA_CLOS_CALL_ii({}, {}->payload.Ok._0);",
                                        mapped, f, tmp));
                                    self.line(&format!("{} = nova_make_Result_Ok({});", out, mapped));
                                    self.indent -= 1;
                                    self.line("} else {");
                                    self.indent += 1;
                                    self.line(&format!("{} = {};", out, tmp));
                                    self.indent -= 1;
                                    self.line("}");
                                    return Ok(out);
                                }
                            }
                            // D26 prelude: Result.map_err(f). Err(e) → Err(f(e)),
                            // Ok остаётся. f это closure (nova_str → nova_str).
                            "map_err" => {
                                if let Some(arg) = args.first() {
                                    let f = self.emit_expr(arg.expr())?;
                                    let tmp = self.fresh_tmp();
                                    self.line(&format!("Nova_Result* {} = {};", tmp, obj_c));
                                    let out = self.fresh_tmp();
                                    self.line(&format!("Nova_Result* {};", out));
                                    self.line(&format!("if ({}->tag == NOVA_TAG_Result_Err) {{", tmp));
                                    self.indent += 1;
                                    // Closure (nova_str → nova_str): сигнатура
                                    // не в стандартных NOVA_CLOS_CALL_*, делаем
                                    // ручной cast fn-указателя.
                                    let new_err = self.fresh_tmp();
                                    self.line(&format!(
                                        "nova_str {} = ((nova_str(*)(void*, nova_str))(((NovaClos_ii*)({}))->fn))(((NovaClos_ii*)({}))->env, {}->payload.Err._0);",
                                        new_err, f, f, tmp));
                                    self.line(&format!("{} = nova_make_Result_Err({});", out, new_err));
                                    self.indent -= 1;
                                    self.line("} else {");
                                    self.indent += 1;
                                    self.line(&format!("{} = {};", out, tmp));
                                    self.indent -= 1;
                                    self.line("}");
                                    return Ok(out);
                                }
                            }
                            "unwrap" => {
                                let tmp = self.fresh_tmp();
                                self.line(&format!("Nova_Result* {} = {};", tmp, obj_c));
                                self.line(&format!("if ({}->tag == NOVA_TAG_Result_Err) {{", tmp));
                                self.indent += 1;
                                self.line(&format!("Nova_Fail_fail({}->payload.Err._0);", tmp));
                                self.indent -= 1;
                                self.line("}");
                                return Ok(format!("({}->payload.Ok._0)", tmp));
                            }
                            _ => {}
                        }
                    }
                    // D91 (Plan 21): Sender capability methods.
                    if obj_ty == "Nova_ChanWriter*" {
                        let obj_c = self.emit_expr(obj)?;
                        match method.as_str() {
                            "send" => {
                                if let Some(arg) = args.first() {
                                    let v = self.emit_expr(arg.expr())?;
                                    return Ok(format!(
                                        "nova_chan_writer_send({}, (nova_int)({}))",
                                        obj_c, v));
                                }
                            }
                            "try_send" => {
                                if let Some(arg) = args.first() {
                                    let v = self.emit_expr(arg.expr())?;
                                    // NovaChanTryResult → bool: OK=true, EMPTY/CLOSED=false
                                    return Ok(format!(
                                        "(nova_chan_writer_try_send({}, (nova_int)({})).tag == NOVA_CHAN_TRY_OK)",
                                        obj_c, v));
                                }
                            }
                            "close" => {
                                self.line(&format!("nova_chan_writer_close({});", obj_c));
                                return Ok("NOVA_UNIT".into());
                            }
                            "len"       => return Ok(format!("nova_chan_writer_len({})", obj_c)),
                            "capacity"  => return Ok(format!("nova_chan_writer_capacity({})", obj_c)),
                            "is_closed" => return Ok(format!("nova_chan_writer_is_closed({})", obj_c)),
                            "clone"     => return Ok(format!("nova_chan_writer_clone({})", obj_c)),
                            _ => {}
                        }
                    }
                    // D91 (Plan 21): Receiver capability methods.
                    if obj_ty == "Nova_ChanReader*" {
                        let obj_c = self.emit_expr(obj)?;
                        match method.as_str() {
                            "recv"      => return Ok(format!("nova_chan_reader_recv({})", obj_c)),
                            "try_recv"  => {
                                // NovaChanTryResult → NovaOpt_nova_int: OK→Some, EMPTY/CLOSED→None
                                let tmp = self.fresh_tmp();
                                self.line(&format!("NovaChanTryResult {} = nova_chan_reader_try_recv({});", tmp, obj_c));
                                return Ok(format!(
                                    "({}.tag == NOVA_CHAN_TRY_OK ? (NovaOpt_nova_int){{.tag=NOVA_TAG_Option_Some,.value={}.value}} : (NovaOpt_nova_int){{.tag=NOVA_TAG_Option_None,.value=0}})",
                                    tmp, tmp));
                            }
                            "len"       => return Ok(format!("nova_chan_reader_len({})", obj_c)),
                            "capacity"  => return Ok(format!("nova_chan_reader_capacity({})", obj_c)),
                            "is_closed" => return Ok(format!("nova_chan_reader_is_closed({})", obj_c)),
                            _ => {}
                        }
                    }
                    // Plan 04 Этап 6: Buffer removed. Use StringBuilder /
                    // WriteBuffer / ReadBuffer instead.
                    // Plan 12: registry-driven dispatch для opaque-types.
                    // Single source of truth — std/runtime/builtins.nv.
                    // Resolve по (recv_type, method_name) + arg-types
                    // (overload, Plan 11).
                    if obj_ty.starts_with("Nova_") && obj_ty.ends_with('*') {
                        let recv_ty = obj_ty.trim_start_matches("Nova_")
                            .trim_end_matches('*').trim();
                        if let Some(decls) = self.external_registry
                            .lookup(recv_ty, method).map(|s| s.to_vec())
                        {
                            // Filter instance overloads.
                            let candidates: Vec<_> = decls.into_iter()
                                .filter(|d| d.is_instance)
                                .collect();
                            if !candidates.is_empty() {
                                // Emit args + collect types.
                                let mut arg_strs = Vec::new();
                                let mut arg_types = Vec::new();
                                for a in args {
                                    arg_types.push(self.infer_expr_c_type(a.expr()));
                                    arg_strs.push(self.emit_expr(a.expr())?);
                                }
                                let chosen = if candidates.len() == 1 {
                                    Some(&candidates[0])
                                } else {
                                    candidates.iter()
                                        .find(|d| d.param_c_types.len() == arg_types.len()
                                            && d.param_c_types.iter().zip(arg_types.iter())
                                                .all(|(w, g)| w == g))
                                };
                                if let Some(decl) = chosen {
                                    let obj_c = self.emit_expr(obj)?;
                                    let mut full = vec![obj_c];
                                    full.extend(arg_strs);
                                    return Ok(format!("{}({})", decl.c_name, full.join(", ")));
                                }
                            }
                        }
                    }
                    // Plan 12 Ф.5: hard-coded dispatch для StringBuilder/
                    // WriteBuffer/ReadBuffer удалён. Все вызовы идут через
                    // registry-driven путь выше (Plan 12 Ф.3).
                }
                // D91 (Plan 21): Channel.new — returns Nova_ChannelPair.
                // Tuple-destructuring handled in emit_let/emit_assign.
                // Here emit as plain call; type inference returns Nova_ChannelPair.
                if let ExprKind::Ident(name) = &obj.kind {
                    if name == "Channel" && method == "new" {
                        if let Some(arg) = args.first() {
                            let v = self.emit_expr(arg.expr())?;
                            return Ok(format!("nova_channel_new({})", v));
                        }
                    }
                    // D75 (revised, Plan 47): CancelToken.new() — Member-form.
                    if name == "CancelToken" && method == "new" {
                        return Ok("nova_cancel_token_new()".to_string());
                    }
                }
                // Plan 12: registry-driven dispatch для Member-form static
                // (obj=Ident("Type")). Resolve через external_registry.
                // Skip `str.from` — см. Path-form блок выше.
                if let ExprKind::Ident(name) = &obj.kind {
                    let skip_str_from = name == "str" && method == "from";
                    if !skip_str_from { if let Some(decls) = self.external_registry
                        .lookup(name, method).map(|s| s.to_vec())
                    {
                        let candidates: Vec<_> = decls.into_iter()
                            .filter(|d| !d.is_instance)
                            .collect();
                        if !candidates.is_empty() {
                            let mut arg_strs = Vec::new();
                            let mut arg_types = Vec::new();
                            for a in args {
                                arg_types.push(self.infer_expr_c_type(a.expr()));
                                arg_strs.push(self.emit_expr(a.expr())?);
                            }
                            let chosen = if candidates.len() == 1 {
                                Some(&candidates[0])
                            } else {
                                candidates.iter()
                                    .find(|d| d.param_c_types.len() == arg_types.len()
                                        && d.param_c_types.iter().zip(arg_types.iter())
                                            .all(|(w, g)| w == g))
                            };
                            if let Some(decl) = chosen {
                                return Ok(format!("{}({})", decl.c_name, arg_strs.join(", ")));
                            }
                        }
                    }}
                }
                // Plan 12 Ф.5: hard-coded Member-form static dispatch для
                // StringBuilder/WriteBuffer/ReadBuffer удалён. Registry-
                // driven путь (Plan 12 Ф.3) обрабатывает это раньше.
                //
                // f64.from_bits / int.to_bits — НЕ в registry (это primitive
                // type methods, не external fn в std/runtime/builtins.nv).
                // Оставляем как hard-coded.
                if let ExprKind::Ident(name) = &obj.kind {
                    if name == "f64" && method == "from_bits" {
                        if let Some(arg) = args.first() {
                            let v = self.emit_expr(arg.expr())?;
                            return Ok(format!("nova_f64_from_bits({})", v));
                        }
                    }
                    if name == "int" && method == "to_bits" {
                        if let Some(arg) = args.first() {
                            let v = self.emit_expr(arg.expr())?;
                            return Ok(format!("nova_int_from_f64_bits({})", v));
                        }
                    }
                    // Plan 32: GC introspection API — std.runtime.gc.
                    // Хардкод как `gc.*` для namespace-style dispatch (без
                    // receiver-type, args без self).
                    if name == "gc" {
                        match method.as_str() {
                            "heap_size" if args.is_empty() => {
                                return Ok("((nova_int)nova_gc_heap_size())".to_string());
                            }
                            "live_count" if args.is_empty() => {
                                return Ok("((nova_int)nova_gc_live_count())".to_string());
                            }
                            "alloc_count" if args.is_empty() => {
                                return Ok("((nova_int)nova_gc_alloc_count())".to_string());
                            }
                            "collect" if args.is_empty() => {
                                // unit-return: comma-expression для совместимости
                                // с expression-position (как nv_panic).
                                return Ok("(nova_gc_collect(), (nova_int)0LL)".to_string());
                            }
                            "reset_stats" if args.is_empty() => {
                                return Ok("(nova_gc_reset_stats(), (nova_int)0LL)".to_string());
                            }
                            _ => {}
                        }
                    }
                    // Plan 44.2 Этап 3: fiber arena introspection — std.runtime.fibers.
                    if name == "fibers" {
                        match method.as_str() {
                            "virtual_reserved" if args.is_empty() => {
                                return Ok("((nova_int)nova_fibers_virtual_reserved())".to_string());
                            }
                            "slot_count" if args.is_empty() => {
                                return Ok("((nova_int)nova_fibers_slot_count())".to_string());
                            }
                            "slots_active" if args.is_empty() => {
                                return Ok("((nova_int)nova_fibers_slots_active())".to_string());
                            }
                            "high_water" if args.is_empty() => {
                                return Ok("((nova_int)nova_fibers_high_water())".to_string());
                            }
                            // Plan 44.2 R8 P41-3: explicit decay flush.
                            "compact" if args.is_empty() => {
                                return Ok("(nova_fibers_compact(), (nova_int)0LL)".to_string());
                            }
                            _ => {}
                        }
                    }
                    // Plan 44 Этап 0: M:N runtime — std.runtime.runtime.
                    if name == "runtime" {
                        match method.as_str() {
                            "init" if args.len() == 1 => {
                                let n = self.emit_expr(args[0].expr())?;
                                return Ok(format!("(nova_runtime_init((int){}), (nova_int)0LL)", n));
                            }
                            "shutdown" if args.is_empty() => {
                                return Ok("(nova_runtime_shutdown(), (nova_int)0LL)".to_string());
                            }
                            "worker_count" if args.is_empty() => {
                                return Ok("((nova_int)nova_runtime_worker_count())".to_string());
                            }
                            "is_initialized" if args.is_empty() => {
                                return Ok("((nova_bool)nova_runtime_is_initialized())".to_string());
                            }
                            "current_worker_id" if args.is_empty() => {
                                return Ok("((nova_int)nova_runtime_current_worker_id())".to_string());
                            }
                            "yield" if args.is_empty() => {
                                return Ok("(nova_fiber_yield(), (nova_int)0LL)".to_string());
                            }
                            _ => {}
                        }
                    }
                }
                // 0. Built-in primitive static methods (D35 + D73).
                //    `str.from(x)` — string conversion (replaces old D70 to_str).
                //    Auto-derive: if user defined `fn V @into() -> str` for V,
                //    call that instead of the builtin.
                if let ExprKind::Ident(prim) = &obj.kind {
                    if prim == "str" && method == "from" {
                        if let Some(arg) = args.first() {
                            let arg_ty = self.infer_expr_c_type(arg.expr());
                            let arg_type = arg_ty.trim_start_matches("Nova_").trim_end_matches('*').to_string();
                            // Plan 11: try multi-overload registry first.
                            let key = ("str".to_string(), "from".to_string());
                            if let Some(overloads) = self.method_overloads.get(&key).cloned() {
                                let static_overloads: Vec<MethodSig> = overloads.into_iter()
                                    .filter(|s| !s.is_instance).collect();
                                if !static_overloads.is_empty() {
                                    let v = self.emit_expr(arg.expr())?;
                                    let chosen = static_overloads.iter()
                                        .find(|s| s.param_c_types.len() == 1
                                            && s.param_c_types[0] == arg_ty);
                                    if let Some(sig) = chosen {
                                        return Ok(format!("{}({})", sig.c_name, v));
                                    }
                                }
                            }
                            if let Some(into_target) = self.into_targets.get(&arg_type) {
                                if into_target == "str" {
                                    let v = self.emit_expr(arg.expr())?;
                                    return Ok(format!("Nova_{}_method_into({})", arg_type, v));
                                }
                            }
                            let v = self.emit_expr(arg.expr())?;
                            return Ok(if arg_ty == "nova_str" {
                                v
                            } else {
                                format!("nova_int_to_str((nova_int)({}))", v)
                            });
                        }
                    }
                }
                // 1. Effect dispatch: `Counter.next()` → `Nova_Counter_next()`
                //    `Time` and `Fail` are pre-registered as built-in effects in
                //    emit_module — `Time.sleep(ms)` and `Fail.fail(msg)` go through
                //    this same path. Default handlers in runtime fall back to
                //    nova_fiber_yield / nova_throw respectively.
                let eff_name = match &obj.kind {
                    ExprKind::Ident(n) => Some(n.clone()),
                    ExprKind::Path(p) => Some(p.join("_")),
                    _ => None,
                };
                if let Some(ref eff) = eff_name {
                    if self.effect_schemas.contains_key(eff.as_str()) {
                        // Emit args immediately and return full call
                        let mut arg_strs = Vec::new();
                        for a in args {
                            arg_strs.push(self.emit_expr(a.expr())?);
                        }
                        return Ok(format!("Nova_{}_{}({})", eff, method, arg_strs.join(", ")));
                    }
                }

                // 1b. Plan 48 Ф.8: static call on generic type name via turbofish.
                // E.g. `HashMap[str, int].new()` — obj is TurboFish(Ident("HashMap"), [str, int]).
                // infer_expr_c_type ignores turbofish type_args → "nova_int" (wrong).
                // Fix: if obj is TurboFish over a known generic type, resolve the concrete name.
                if let ExprKind::TurboFish { base, type_args } = &obj.kind {
                    if let ExprKind::Ident(type_name) = &base.kind {
                        if self.generic_types.contains(type_name) {
                            let type_args_c: Vec<String> = type_args.iter()
                                .filter_map(|tr| self.type_ref_to_c(tr).ok())
                                .filter(|c| !c.is_empty() && c != "void*")
                                .collect();
                            if type_args_c.len() == type_args.len() {
                                let mangled = Self::compute_generic_type_c_name(type_name, &type_args_c);
                                self.generic_type_instance_info.borrow_mut()
                                    .entry(mangled.clone())
                                    .or_insert_with(|| (type_name.clone(), type_args_c.clone()));
                                if !self.emitted_generic_type_instances.contains(&mangled) {
                                    let mut wl = self.generic_type_worklist.borrow_mut();
                                    if !wl.iter().any(|(_, _, m)| m == &mangled) {
                                        wl.push((type_name.clone(), type_args_c, mangled.clone()));
                                    }
                                }
                                // Теперь dispatch идёт через generic instance path (9359)
                                // с obj_ty = "Nova_HashMap____...*"
                                let fake_obj_ty = format!("{}*", mangled);
                                let instance_opt: Option<(String, Vec<String>)> =
                                    self.generic_type_instance_info.borrow()
                                        .get(&mangled).cloned();
                                if let Some((base_name, targs)) = instance_opt {
                                    let method_decl = self.generic_type_methods.get(&base_name)
                                        .and_then(|ms| ms.iter().find(|m| m.name == *method))
                                        .cloned();
                                    if let Some(fn_decl) = method_decl {
                                        let tmpl_opt = self.generic_type_templates.get(&base_name).cloned();
                                        if let Some(tmpl) = tmpl_opt {
                                            let type_subst: Vec<(String, String)> = tmpl.generics.iter()
                                                .zip(targs.iter())
                                                .map(|(g, c)| (g.name.clone(), c.clone()))
                                                .collect();
                                            let is_inst = matches!(
                                                fn_decl.receiver.as_ref().map(|r| &r.kind),
                                                Some(crate::ast::ReceiverKind::Instance));
                                            let method_c_name = if is_inst {
                                                format!("{}_method_{}", mangled, method)
                                            } else {
                                                format!("{}_static_{}", mangled, method)
                                            };
                                            let saved_subst = std::mem::replace(
                                                &mut self.current_type_subst,
                                                type_subst.iter().cloned().collect(),
                                            );
                                            let mut arg_strs = Vec::new();
                                            for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                                            self.current_type_subst = saved_subst;
                                            // Strip "Nova_" so recv_type stays consistent with
                                            // instance-method path (receiver_c_type adds it back).
                                            let recv_type_stripped = mangled.strip_prefix("Nova_").unwrap_or(&mangled);
                                            self.register_mono_method_instance(
                                                &fn_decl, type_subst, &method_c_name, recv_type_stripped);
                                            let _ = fake_obj_ty;
                                            return Ok(format!("{}({})", method_c_name, arg_strs.join(", ")));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // 2. Array methods: `arr.get(i)` → `nova_array_get_nova_int(arr, i)`
                let obj_ty = self.infer_expr_c_type(obj);
                if obj_ty.starts_with("NovaArray_") {
                    let elem_ty = obj_ty.strip_prefix("NovaArray_").unwrap_or("nova_int")
                        .trim_end_matches('*').trim();
                    match method.as_str() {
                        "get" => {
                            let obj_c = self.emit_expr(obj)?;
                            let mut arg_strs = vec![obj_c];
                            for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                            return Ok(format!("nova_array_get_{}({})", elem_ty, arg_strs.join(", ")));
                        }
                        "push" => {
                            let obj_c = self.emit_expr(obj)?;
                            if elem_ty == "nova_int" && args.len() == 1 {
                                let arg_ty = self.infer_expr_c_type(args[0].expr());
                                let arg_c = self.emit_expr(args[0].expr())?;
                                if arg_ty == "nova_str" {
                                    // Box nova_str as pointer stored as nova_int
                                    let stmp = self.fresh_tmp();
                                    self.line(&format!("nova_str* {} = (nova_str*)nova_alloc(sizeof(nova_str));", stmp));
                                    self.line(&format!("*{} = {};", stmp, arg_c));
                                    self.line(&format!("nova_array_push_nova_int({}, (nova_int)({}));", obj_c, stmp));
                                    // Mark array variable as storing boxed nova_str*
                                    if let ExprKind::Ident(arr_name) = &obj.kind {
                                        self.str_box_arrays.insert(arr_name.clone());
                                    }
                                } else if arg_ty == "void*" {
                                    // Erased generic value: store pointer as nova_int
                                    self.line(&format!("nova_array_push_nova_int({}, (nova_int)(intptr_t)({}));", obj_c, arg_c));
                                } else {
                                    self.line(&format!("nova_array_push_nova_int({}, {});", obj_c, arg_c));
                                }
                            } else {
                                let mut arg_strs = vec![obj_c];
                                for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                                self.line(&format!("nova_array_push_{}({});", elem_ty, arg_strs.join(", ")));
                            }
                            return Ok("NOVA_UNIT".into());
                        }
                        "pop" => {
                            let obj_c = self.emit_expr(obj)?;
                            return Ok(format!("nova_array_pop_{}({})", elem_ty, obj_c));
                        }
                        _ => {}
                    }
                }

                // 3a. D74 math methods on f64/f32:
                //     `x.sqrt()` → `sqrt(x)` (libm <math.h>); `x.abs()` → `fabs(x)`.
                //     По D74: математические операции — instance-методы на числовых
                //     типах. В runtime'е они мапятся на стандартные C-функции
                //     из <math.h>.
                if obj_ty == "nova_f64" || obj_ty == "nova_f32" {
                    if let Some(c_fn) = Self::f64_method_to_c(method) {
                        let obj_c = self.emit_expr(obj)?;
                        let mut arg_strs = vec![obj_c];
                        for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                        return Ok(format!("{}({})", c_fn, arg_strs.join(", ")));
                    }
                }
                // 3b. D109: built-in primitive methods (hash/eq/ord).
                if let Some(builtin) = Self::prim_builtin_method(&obj_ty, method) {
                    let obj_c = self.emit_expr(obj)?;
                    return Ok(match builtin {
                        PrimBuiltin::Fn(fn_name) => {
                            let mut arg_strs = vec![obj_c];
                            for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                            format!("{}({})", fn_name, arg_strs.join(", "))
                        }
                        PrimBuiltin::BinOp(op) => {
                            let arg_c = if let Some(a) = args.first() {
                                self.emit_expr(a.expr())?
                            } else { "0".into() };
                            format!("({} {} {})", obj_c, op, arg_c)
                        }
                    });
                }
                // Plan 54 Ф.2: user-defined extension methods на primitives
                // (`fn int @millis() -> Duration`, `fn str @len() -> int`, etc).
                // Lookup в method_overloads через primitive Nova-name ("int",
                // "str", "bool", "f64", "byte"). Если найдено instance overload
                // — dispatch на mangled c_name (`Nova_int_method_millis`).
                // Это закрывает `[M-int-extension-record-field]` — bug когда
                // `100.millis()` в record-literal field generic static ctor
                // эмитил invalid C member-access на nova_int.
                {
                    let prim_nova_name = match obj_ty.as_str() {
                        "nova_int"  => Some("int"),
                        "nova_str"  => Some("str"),
                        "nova_bool" => Some("bool"),
                        "nova_f64"  => Some("f64"),
                        "nova_f32"  => Some("f32"),
                        "nova_byte" => Some("byte"),
                        _ => None,
                    };
                    if let Some(prim) = prim_nova_name {
                        let key = (prim.to_string(), method.clone());
                        if let Some(overloads) = self.method_overloads.get(&key).cloned() {
                            let inst_overloads: Vec<MethodSig> = overloads.into_iter()
                                .filter(|s| s.is_instance)
                                .collect();
                            if let Some(sig) = inst_overloads.first() {
                                let obj_c = self.emit_expr(obj)?;
                                let mut arg_strs = vec![obj_c];
                                for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                                return Ok(format!("{}({})", sig.c_name, arg_strs.join(", ")));
                            }
                        }
                    }
                }
                // 3c. D74 math methods on int (selected — abs, sign):
                //     `n.abs()` → `llabs(n)`. Большинство int-методов — это
                //     int-to-string, обработаны в str.from(...).
                if obj_ty == "nova_int" {
                    if let Some(c_fn) = Self::int_method_to_c(method) {
                        let obj_c = self.emit_expr(obj)?;
                        let mut arg_strs = vec![obj_c];
                        for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                        return Ok(format!("{}({})", c_fn, arg_strs.join(", ")));
                    }
                }
                // 3. String methods: `s.starts_with(...)` → `nova_str_starts_with(s, ...)`
                if obj_ty == "nova_str" {
                    if let Some(rt_fn) = Self::str_method_to_rt(method) {
                        let obj_c = self.emit_expr(obj)?;
                        let mut arg_strs = vec![obj_c];
                        for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                        return Ok(format!("{}({})", rt_fn, arg_strs.join(", ")));
                    }
                }

                // 4. Direct handler vtable call: `switcher.flip()` where switcher: NovaVtable_X*
                //    → `switcher->flip(switcher->ctx, args)`
                if obj_ty.starts_with("NovaVtable_") && obj_ty.ends_with('*') {
                    let obj_c = self.emit_expr(obj)?;
                    let mut arg_strs = vec![format!("{obj}->ctx", obj = obj_c)];
                    for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                    return Ok(format!("{obj}->{method}({args})",
                        obj = obj_c, method = method, args = arg_strs.join(", ")));
                }

                // 4b. If object type is void* (unknown generic stub), the method call is
                //     usually undefined — emit NULL to prevent calls to undeclared functions.
                // Exception: self-referential recursive call inside a sum-type method
                // (e.g. t.length() inside LinkedList.length where t: void* holds a LinkedList*).
                // In that case, cast void* to the current receiver type and dispatch erased.
                if obj_ty == "void*" {
                    let recv_ty_opt = self.current_receiver_type.clone();
                    let method_str = method.to_string();
                    let is_self_ref = recv_ty_opt.as_ref()
                        .map(|rt| self.all_methods.contains(&(rt.clone(), method_str.clone())))
                        .unwrap_or(false);
                    if is_self_ref {
                        let recv_ty = recv_ty_opt.unwrap();
                        let obj_c = self.emit_expr(obj)?;
                        let mut arg_strs = vec![format!("((Nova_{}*)({}))", recv_ty, obj_c)];
                        for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                        return Ok(format!("Nova_{}_method_{}({})", recv_ty, method_str, arg_strs.join(", ")));
                    }
                    for a in args { let _ = self.emit_expr(a.expr())?; }
                    return Ok("NULL".into());
                }

                // 4c. D73 v2 auto-derive: `v.into()` for type V where V has no
                //     explicit `@into`, but some target T has `T.from(v V)`.
                //     Emit `Nova_T_static_from(v)` in that case.
                if method == "into" && args.is_empty() {
                    let recv_ty = self.infer_expr_c_type(obj);
                    let recv_type = Self::nova_type_name_from_c(&recv_ty);
                    // Skip if explicit `fn V @into()` is present (handled by method_receivers below).
                    let has_explicit_into = self.method_receivers.get("into")
                        .map(|(t, _)| t == &recv_type).unwrap_or(false);
                    if !has_explicit_into {
                        // Look for any target T such that `T.from(v V)` is defined.
                        let target = self.from_targets.iter()
                            .find(|(_, sources)| sources.iter().any(|s| s == &recv_type))
                            .map(|(t, _)| t.clone());
                        if let Some(target_type) = target {
                            let obj_c = self.emit_expr(obj)?;
                            return Ok(format!("Nova_{}_static_from({})", target_type, obj_c));
                        }
                    }
                }
                // Plan 08 Ф.3: D77 4-way auto-derive — `v.@try_into()`.
                // Симметрия для @into. Если есть `T.try_from(v V)` и нет
                // явного `V.@try_into()`, эмитим как `T.try_from(v)`.
                if method == "try_into" && args.is_empty() {
                    let recv_ty = self.infer_expr_c_type(obj);
                    let recv_type = Self::nova_type_name_from_c(&recv_ty);
                    let has_explicit = self.method_receivers.get("try_into")
                        .map(|(t, _)| t == &recv_type).unwrap_or(false);
                    if !has_explicit {
                        let target = self.try_from_targets.iter()
                            .find(|(_, sources)| sources.iter().any(|s| s == &recv_type))
                            .map(|(t, _)| t.clone());
                        if let Some(target_type) = target {
                            let obj_c = self.emit_expr(obj)?;
                            return Ok(format!("Nova_{}_static_try_from({})", target_type, obj_c));
                        }
                    }
                }

                // 5. User-defined method call: `obj.method(args)` → `Nova_T_method_name(obj, args)`
                //    or static: `TypeName.method(args)` → `Nova_T_static_name(args)`
                //    Detect by checking method_receivers map populated at module-scan time.
                // Plan 11 Ф.2: сначала пытаемся multi-overload registry —
                // strict resolution по types args. Покрывает overload + решает
                // single-key last-wins для одноимённых методов на разных типах.
                {
                    // Определяем receiver-type:
                    //   - obj=Ident("T") где T — known type → static call.
                    //   - иначе (obj — variable / expr) → instance call;
                    //     receiver-type из обуточенного obj_ty.
                    let recv_type_name = if let ExprKind::Ident(n) = &obj.kind {
                        if self.method_overloads.keys().any(|(t, _)| t == n) {
                            // Static call.
                            Some(n.clone())
                        } else {
                            // Instance call (obj — variable). Берём из obj_ty.
                            let trimmed = obj_ty.trim_start_matches("Nova_")
                                .trim_end_matches('*').trim().to_string();
                            if !trimmed.is_empty() && trimmed != "void" {
                                Some(trimmed)
                            } else {
                                None
                            }
                        }
                    } else {
                        // Не-Ident obj (expr) → всегда instance.
                        let trimmed = obj_ty.trim_start_matches("Nova_")
                            .trim_end_matches('*').trim().to_string();
                        if !trimmed.is_empty() && trimmed != "void" {
                            Some(trimmed)
                        } else {
                            None
                        }
                    };
                    if let Some(rt) = recv_type_name {
                        // want_instance: true unless obj is Ident(known-type).
                        let want_instance = !matches!(&obj.kind, ExprKind::Ident(n)
                            if self.method_overloads.keys().any(|(t, _)| t == n));
                        let key = (rt.clone(), method.clone());
                        if let Some(overloads) = self.method_overloads.get(&key).cloned() {
                            let candidates: Vec<MethodSig> = overloads.into_iter()
                                .filter(|s| s.is_instance == want_instance)
                                .collect();
                            if !candidates.is_empty() {
                                // Plan 48: sentinel detection for generic methods with own type params.
                                if candidates.iter().any(|c| c.c_name.starts_with("__mono_method__")) {
                                    let recv_key = (rt.clone(), method.clone());
                                    if let Some(fn_decl) = self.mono_method_decls.get(&recv_key).cloned() {
                                        let type_subst = self.resolve_mono_type_args(&fn_decl, &[], args)
                                            .unwrap_or_else(|_| fn_decl.generics.iter()
                                                .map(|g| (g.name.clone(), "nova_str".to_string()))
                                                .collect());
                                        let base_c_name = format!("Nova_{}_method_{}", rt, method);
                                        let mono_name = Self::compute_mono_name(&base_c_name, &type_subst);
                                        let rt_clone = rt.clone();
                                        self.register_mono_method_instance(
                                            &fn_decl.clone(), type_subst.clone(), &mono_name.clone(), &rt_clone);
                                        let mut arg_strs = Vec::new();
                                        for (param_decl, a) in fn_decl.params.iter().zip(args.iter()) {
                                            if let crate::ast::TypeRef::Func { params: fp, return_type, .. } = &param_decl.ty {
                                                let saved_inner = std::mem::replace(
                                                    &mut self.current_type_subst,
                                                    type_subst.iter().cloned().collect(),
                                                );
                                                let inner_ptys: Vec<String> = fp.iter()
                                                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                                                    .collect();
                                                let inner_ret = return_type.as_ref()
                                                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
                                                    .unwrap_or_else(|| "nova_unit".into());
                                                self.current_type_subst = saved_inner;
                                                let prev_sig = self.fn_param_sigs.insert(
                                                    param_decl.name.clone(), (inner_ptys, inner_ret));
                                                let v = self.emit_expr(a.expr())?;
                                                match prev_sig {
                                                    Some(old) => { self.fn_param_sigs.insert(param_decl.name.clone(), old); }
                                                    None => { self.fn_param_sigs.remove(&param_decl.name); }
                                                }
                                                arg_strs.push(v);
                                            } else {
                                                arg_strs.push(self.emit_expr(a.expr())?);
                                            }
                                        }
                                        for a in args.iter().skip(fn_decl.params.len()) {
                                            arg_strs.push(self.emit_expr(a.expr())?);
                                        }
                                        if want_instance {
                                            let obj_c = self.emit_expr(obj)?;
                                            let mut full = vec![obj_c];
                                            full.extend(arg_strs);
                                            return Ok(format!("{}({})", mono_name, full.join(", ")));
                                        } else {
                                            return Ok(format!("{}({})", mono_name, arg_strs.join(", ")));
                                        }
                                    }
                                }
                                // Generic receiver (Stack[T], etc.): args боксируются
                                // в void*. Без этого вызов методов на generic типе
                                // даёт type-mismatch в C (param void*, arg int/str).
                                let is_generic_recv = self.generic_types.contains(&rt);
                                let mut arg_strs = Vec::new();
                                let mut arg_types = Vec::new();
                                for a in args {
                                    let aty = self.infer_expr_c_type(a.expr());
                                    let v = self.emit_expr(a.expr())?;
                                    if is_generic_recv {
                                        // Box arg в void* (паттерн как в legacy
                                        // single-key path).
                                        if aty == "nova_str" {
                                            let heap_tmp = self.fresh_tmp();
                                            self.line(&format!("nova_str* {} = (nova_str*)nova_alloc(sizeof(nova_str));", heap_tmp));
                                            self.line(&format!("*{} = {};", heap_tmp, v));
                                            arg_strs.push(format!("(void*)({})", heap_tmp));
                                        } else if aty.ends_with('*') || aty == "void*" {
                                            arg_strs.push(format!("(void*)({})", v));
                                        } else {
                                            arg_strs.push(format!("(void*)(intptr_t)({})", v));
                                        }
                                        arg_types.push("void*".to_string());
                                    } else {
                                        arg_strs.push(v);
                                        arg_types.push(aty);
                                    }
                                }
                                // Plan 11 Ф.9.3: override-precedence Own > Delegated.
                                // Strict-match candidates сначала; затем — если
                                // matches содержит Own, отфильтровать Delegated.
                                let strict: Vec<MethodSig> = if candidates.len() == 1 {
                                    candidates.clone()
                                } else {
                                    candidates.iter()
                                        .filter(|s| s.param_c_types.len() == arg_types.len())
                                        .filter(|s| s.param_c_types.iter().zip(arg_types.iter())
                                            .all(|(w, g)| w == g))
                                        .cloned()
                                        .collect()
                                };
                                let pool: Vec<MethodSig> = {
                                    let owns: Vec<MethodSig> = strict.iter()
                                        .filter(|s| !s.is_delegated)
                                        .cloned().collect();
                                    if !owns.is_empty() { owns } else { strict }
                                };
                                let chosen = pool.into_iter().next();
                                if let Some(sig) = chosen {
                                    if want_instance {
                                        let obj_c = self.emit_expr(obj)?;
                                        let mut full = vec![obj_c];
                                        full.extend(arg_strs);
                                        return Ok(format!("{}({})", sig.c_name, full.join(", ")));
                                    } else {
                                        return Ok(format!("{}({})", sig.c_name, arg_strs.join(", ")));
                                    }
                                }
                                // 0 matches при ≥2 candidates → fallback на старую
                                // логику ниже (или error на дальнейших шагах).
                            }
                        }
                    }
                }

                // 5b. Plan 48 Ф.3: generic type instance method dispatch.
                // If receiver type is a concrete generic instance (e.g. "Nova_HashMap____nova_str__nova_int*"),
                // look up the method in generic_type_methods and emit a monomorphized instance.
                {
                    let rt_trimmed = obj_ty.trim_start_matches("Nova_")
                        .trim_end_matches('*').trim().to_string();
                    // generic_type_instance_info keys have "Nova_" prefix; rt_trimmed doesn't.
                    let instance_opt: Option<(String, Vec<String>)> =
                        self.generic_type_instance_info.borrow()
                            .get(&format!("Nova_{}", rt_trimmed)).cloned();
                    if let Some((base_name, type_args_c)) = instance_opt {
                        let method_decl = self.generic_type_methods.get(&base_name)
                            .and_then(|ms| ms.iter().find(|m| m.name == *method))
                            .cloned();
                        if let Some(fn_decl) = method_decl {
                            let tmpl_opt = self.generic_type_templates.get(&base_name).cloned();
                            if let Some(tmpl) = tmpl_opt {
                                let type_subst: Vec<(String, String)> = tmpl.generics.iter()
                                    .zip(type_args_c.iter())
                                    .map(|(g, c)| (g.name.clone(), c.clone()))
                                    .collect();
                                let is_instance = matches!(
                                    fn_decl.receiver.as_ref().map(|r| &r.kind),
                                    Some(crate::ast::ReceiverKind::Instance));
                                let method_c_name = if is_instance {
                                    format!("{}_method_{}", rt_trimmed, method)
                                } else {
                                    format!("{}_static_{}", rt_trimmed, method)
                                };
                                // Apply subst to param types for fn_param_sigs (closure params)
                                let saved_subst = std::mem::replace(
                                    &mut self.current_type_subst,
                                    type_subst.iter().cloned().collect(),
                                );
                                let mut arg_strs = Vec::new();
                                for (param_decl, a) in fn_decl.params.iter().zip(args.iter()) {
                                    if let crate::ast::TypeRef::Func { params: fp, return_type, .. } = &param_decl.ty {
                                        let inner_ptys: Vec<String> = fp.iter()
                                            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                                            .collect();
                                        let inner_ret = return_type.as_ref()
                                            .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
                                            .unwrap_or_else(|| "nova_unit".into());
                                        let prev_sig = self.fn_param_sigs.insert(
                                            param_decl.name.clone(), (inner_ptys, inner_ret));
                                        let v = self.emit_expr(a.expr())?;
                                        match prev_sig {
                                            Some(old) => { self.fn_param_sigs.insert(param_decl.name.clone(), old); }
                                            None => { self.fn_param_sigs.remove(&param_decl.name); }
                                        }
                                        arg_strs.push(v);
                                    } else {
                                        arg_strs.push(self.emit_expr(a.expr())?);
                                    }
                                }
                                for a in args.iter().skip(fn_decl.params.len()) {
                                    arg_strs.push(self.emit_expr(a.expr())?);
                                }
                                self.current_type_subst = saved_subst;
                                // Enqueue mono method emission
                                self.register_mono_method_instance(
                                    &fn_decl, type_subst, &method_c_name, &rt_trimmed);
                                if is_instance {
                                    let obj_c = self.emit_expr(obj)?;
                                    let mut full = vec![obj_c];
                                    full.extend(arg_strs);
                                    return Ok(format!("{}({})", method_c_name, full.join(", ")));
                                } else {
                                    return Ok(format!("{}({})", method_c_name, arg_strs.join(", ")));
                                }
                            }
                        }
                    }
                }

                if let Some((type_name, is_instance)) = self.method_receivers.get(method).cloned() {
                    let is_generic_type = self.generic_types.contains(&type_name);
                    let safe_type = Self::receiver_type_c_ident(&type_name);
                    if is_instance {
                        let obj_c = self.emit_expr(obj)?;
                        let mut arg_strs = vec![obj_c];
                        for a in args {
                            if is_generic_type {
                                // Generic receiver: box args to void*
                                let arg_ty = self.infer_expr_c_type(a.expr());
                                let v = self.emit_expr(a.expr())?;
                                if arg_ty == "nova_str" {
                                    let heap_tmp = self.fresh_tmp();
                                    self.line(&format!("nova_str* {} = (nova_str*)nova_alloc(sizeof(nova_str));", heap_tmp));
                                    self.line(&format!("*{} = {};", heap_tmp, v));
                                    arg_strs.push(format!("(void*)({})", heap_tmp));
                                } else if arg_ty.ends_with('*') || arg_ty == "void*" {
                                    arg_strs.push(format!("(void*)({})", v));
                                } else {
                                    arg_strs.push(format!("(void*)(intptr_t)({})", v));
                                }
                            } else {
                                arg_strs.push(self.emit_expr(a.expr())?);
                            }
                        }
                        return Ok(format!("Nova_{}_method_{}({})", safe_type, method, arg_strs.join(", ")));
                    } else {
                        // Static method: obj is the type name (Ident), not a value
                        let mut arg_strs = Vec::new();
                        for a in args {
                            if is_generic_type {
                                let arg_ty = self.infer_expr_c_type(a.expr());
                                let v = self.emit_expr(a.expr())?;
                                if arg_ty == "nova_str" {
                                    let heap_tmp = self.fresh_tmp();
                                    self.line(&format!("nova_str* {} = (nova_str*)nova_alloc(sizeof(nova_str));", heap_tmp));
                                    self.line(&format!("*{} = {};", heap_tmp, v));
                                    arg_strs.push(format!("(void*)({})", heap_tmp));
                                } else if arg_ty.ends_with('*') || arg_ty == "void*" {
                                    arg_strs.push(format!("(void*)({})", v));
                                } else {
                                    arg_strs.push(format!("(void*)(intptr_t)({})", v));
                                }
                            } else {
                                arg_strs.push(self.emit_expr(a.expr())?);
                            }
                        }
                        return Ok(format!("Nova_{}_static_{}({})", safe_type, method, arg_strs.join(", ")));
                    }
                }
                // Fallback: generic member call (field-function or unknown)
                let accessor = if Self::is_value_type(&obj_ty) { "." } else { "->" };
                let obj_c = self.emit_expr(obj)?;
                format!("{obj}{acc}{method}", obj = obj_c, acc = accessor, method = method)
            }
            ExprKind::Path(parts) => {
                // Plan 11 Ф.4.5: D66 — Self в expression position (call).
                // `Self.method(args)` в теле метода резолвится в
                // `<current_type>.method(args)`. Тот же current_receiver_type
                // что используется для type-position (-> Self).
                let parts: Vec<String> = if !parts.is_empty() && parts[0] == "Self" {
                    if let Some(recv) = &self.current_receiver_type {
                        let mut new_parts = parts.clone();
                        new_parts[0] = recv.clone();
                        new_parts
                    } else {
                        parts.clone()
                    }
                } else {
                    parts.clone()
                };
                let parts: &[String] = &parts;
                // D91 (Plan 21): Channel.new — Path-form.
                if parts.len() == 2 && parts[0] == "Channel" && parts[1] == "new" {
                    if let Some(arg) = args.first() {
                        let v = self.emit_expr(arg.expr())?;
                        return Ok(format!("nova_channel_new({})", v));
                    }
                }
                // D75 (revised, Plan 47): CancelToken.new() — Path-form.
                // Type.new() парсится как ExprKind::Path (см. Plan 18 урок).
                if parts.len() == 2 && parts[0] == "CancelToken" && parts[1] == "new" {
                    return Ok("nova_cancel_token_new()".to_string());
                }
                // Plan 08 Ф.2: D77 try_from / D73 from для numeric/char/bool ↔ str.
                // T.try_from(v) → Result[T, ParseError]; здесь эмитим
                // через runtime helper'ы из nova_rt/conv.h.
                if parts.len() == 2 && parts[1] == "try_from" {
                    if let Some(arg) = args.first() {
                        let arg_ty = self.infer_expr_c_type(arg.expr());
                        let v = self.emit_expr(arg.expr())?;
                        // Plan 04 Этап 6: str.try_from([]byte) → Result[str, _].
                        // Validates UTF-8 + конвертирует в nova_str. Используется
                        // для финализации mixed text+binary в WriteBuffer.
                        if parts[0] == "str" && arg_ty == "NovaArray_nova_byte*" {
                            return Ok(format!("Nova_str_static_try_from_bytes({})", v));
                        }
                        // str → numeric / bool: используем парсеры.
                        if arg_ty == "nova_str" {
                            let target = parts[0].as_str();
                            let helper_name = match target {
                                "int" | "i64" => Some("nova_str_to_i64"),
                                "u64" | "u32" | "u16" | "u8" => Some("nova_str_to_u64"),
                                "i32" | "i16" | "i8" => Some("nova_str_to_i64"),
                                "f64" | "f32" => Some("nova_str_to_f64"),
                                "bool" => Some("nova_str_to_bool"),
                                "char" => Some("nova_str_to_char"),
                                _ => None,
                            };
                            if let Some(helper) = helper_name {
                                // Emit: parse → wrap в Result.
                                // nova_<helper>(s) даёт {value, ok}; если ok=true,
                                // возвращаем Ok(value), иначе Err(<msg>).
                                let tmp = self.fresh_tmp();
                                self.line(&format!("nova_str {} = {};", tmp, v));
                                let res_var = self.fresh_tmp();
                                let result_struct_ty = if helper == "nova_str_to_u64" {
                                    "nova_parse_u64_result"
                                } else if helper == "nova_str_to_f64" {
                                    "nova_parse_f64_result"
                                } else if helper == "nova_str_to_bool" {
                                    "nova_parse_bool_result"
                                } else if helper == "nova_str_to_char" {
                                    "nova_char_decode_result"
                                } else {
                                    "nova_parse_int_result"
                                };
                                self.line(&format!("{} {} = {}({});",
                                    result_struct_ty, res_var, helper, tmp));
                                let out = self.fresh_tmp();
                                self.line(&format!("Nova_Result* {};", out));
                                self.line(&format!("if ({}.ok) {{", res_var));
                                self.indent += 1;
                                // Cast value к nova_int payload (Result hardcoded на nova_int).
                                self.line(&format!(
                                    "{} = nova_make_Result_Ok((nova_int){}.value);",
                                    out, res_var));
                                self.indent -= 1;
                                self.line("} else {");
                                self.indent += 1;
                                let err_msg = format!("{}.try_from: parse error", target);
                                self.line(&format!(
                                    "{} = nova_make_Result_Err((nova_str){{.ptr=\"{}\", .len={}}});",
                                    out, err_msg, err_msg.len()));
                                self.indent -= 1;
                                self.line("}");
                                return Ok(out);
                            }
                        }
                        // int → char: range-check.
                        if arg_ty == "nova_int" && parts[0] == "char" {
                            let res_var = self.fresh_tmp();
                            self.line(&format!(
                                "nova_char_decode_result {} = nova_int_to_char({});",
                                res_var, v));
                            let out = self.fresh_tmp();
                            self.line(&format!("Nova_Result* {};", out));
                            self.line(&format!("if ({}.ok) {{", res_var));
                            self.indent += 1;
                            self.line(&format!(
                                "{} = nova_make_Result_Ok({}.value);",
                                out, res_var));
                            self.indent -= 1;
                            self.line("} else {");
                            self.indent += 1;
                            self.line(&format!(
                                "{} = nova_make_Result_Err((nova_str){{.ptr=\"char.try_from: invalid codepoint\", .len=37}});",
                                out));
                            self.indent -= 1;
                            self.line("}");
                            return Ok(out);
                        }
                    }
                }
                // Plan 04 follow-up: f64.from_bits(n int) — IEEE 754
                // bit-cast int → f64. Pair with int.to_bits(f f64). Используется
                // для распаковки try_read_f64_*: r.unwrap_or(0) даёт nova_int
                // bits, f64.from_bits(bits) → восстанавливает double.
                if parts.len() == 2 && parts[0] == "f64" && parts[1] == "from_bits" {
                    if let Some(arg) = args.first() {
                        let v = self.emit_expr(arg.expr())?;
                        return Ok(format!("nova_f64_from_bits({})", v));
                    }
                }
                if parts.len() == 2 && parts[0] == "int" && parts[1] == "to_bits" {
                    if let Some(arg) = args.first() {
                        let v = self.emit_expr(arg.expr())?;
                        return Ok(format!("nova_int_from_f64_bits({})", v));
                    }
                }
                // Plan 52.2 Ф.2: parts[0] это lazy const → это instance
                // method call на const-value, не static-method-call.
                // Конвертим Path(["KEYWORDS", "get"]) в Member { Ident("KEYWORDS"), "get" }
                // и делегируем в обычный method-call emit.
                if parts.len() == 2 && self.lazy_consts.contains(&parts[0]) {
                    let new_obj = Expr {
                        kind: ExprKind::Ident(parts[0].clone()),
                        span: func.span,
                    };
                    let new_func = Expr {
                        kind: ExprKind::Member {
                            obj: Box::new(new_obj),
                            name: parts[1].clone(),
                        },
                        span: func.span,
                    };
                    let new_call = Expr {
                        kind: ExprKind::Call {
                            func: Box::new(new_func),
                            args: args.to_vec(),
                            trailing: None,
                        },
                        span: func.span,
                    };
                    return self.emit_expr(&new_call);
                }
                // Plan 08 Ф.2: T.from(v) — infallible конверсии.
                // bool → str / char → str / f64 → str.
                if parts.len() == 2 && parts[1] == "from" {
                    if let Some(arg) = args.first() {
                        let arg_expr = arg.expr();
                        let arg_ty = self.infer_expr_c_type(arg_expr);
                        let v = self.emit_expr(arg_expr)?;
                        if parts[0] == "str" {
                            // CharLit detection — ДО numeric, потому что
                            // char хранится как nova_int (одно представление).
                            // emit_expr_c_type для CharLit даёт "nova_int",
                            // но семантика char→str ≠ int→str.
                            if let ExprKind::CharLit(_) = &arg_expr.kind {
                                return Ok(format!("nova_char_to_str({})", v));
                            }
                            match arg_ty.as_str() {
                                "nova_bool" => return Ok(format!("nova_bool_to_str({})", v)),
                                "nova_f64"  => return Ok(format!("nova_f64_to_str({})", v)),
                                "nova_int"  => return Ok(format!("nova_int_to_str({})", v)),
                                _ => {}
                            }
                        }
                    }
                }
                // Plan 12: registry-driven dispatch для Path-form static
                // (Type.method(args)). Resolve по (recv_type, method_name)
                // + arg-types.
                //
                // Skip `str.from` — есть hard-coded path ниже с auto-derive
                // через D73 into_targets. Registry знает только `str.from(char)`
                // но `str.from(int/f64/bool)` идут через builtin nova_*_to_str
                // helpers — которых в registry нет.
                if parts.len() == 2 && !(parts[0] == "str" && parts[1] == "from") {
                    let recv_ty = &parts[0];
                    let method_name = &parts[1];
                    if let Some(decls) = self.external_registry
                        .lookup(recv_ty, method_name).map(|s| s.to_vec())
                    {
                        let candidates: Vec<_> = decls.into_iter()
                            .filter(|d| !d.is_instance)
                            .collect();
                        if !candidates.is_empty() {
                            let mut arg_strs = Vec::new();
                            let mut arg_types = Vec::new();
                            for a in args {
                                arg_types.push(self.infer_expr_c_type(a.expr()));
                                arg_strs.push(self.emit_expr(a.expr())?);
                            }
                            let chosen = if candidates.len() == 1 {
                                Some(&candidates[0])
                            } else {
                                candidates.iter()
                                    .find(|d| d.param_c_types.len() == arg_types.len()
                                        && d.param_c_types.iter().zip(arg_types.iter())
                                            .all(|(w, g)| w == g))
                            };
                            if let Some(decl) = chosen {
                                return Ok(format!("{}({})", decl.c_name, arg_strs.join(", ")));
                            }
                        }
                    }
                }
                // Plan 12 Ф.5: hard-coded Path-form static dispatch для
                // StringBuilder/WriteBuffer/ReadBuffer удалён. Registry-
                // driven путь (Plan 12 Ф.3) обрабатывает это раньше.
                // Check if first segment is a known effect
                if parts.len() == 2 && self.effect_schemas.contains_key(&parts[0]) {
                    format!("Nova_{}_{}", parts[0], parts[1])
                } else if parts.len() == 2 {
                    // Built-in primitive static methods (D35 + D73).
                    // `str.from(x)` — convert any value to string (replaces
                    // old D70 to_str). Bootstrap implementation: dispatch on
                    // arg type — nova_str pass-through, nova_int via
                    // nova_int_to_str. Other types TBD.
                    //
                    // Auto-derive caveat: if user defined `fn V @into() -> str`
                    // for the arg's type V, we should call that instead of
                    // the builtin (so user code wins). Checked below before
                    // falling back to builtin.
                    if parts[0] == "str" && parts[1] == "from" {
                        if let Some(arg) = args.first() {
                            let arg_ty = self.infer_expr_c_type(arg.expr());
                            let arg_type = arg_ty.trim_start_matches("Nova_").trim_end_matches('*').to_string();
                            // Plan 11: try multi-overload registry first — strict
                            // arg-type match resolves between overloads (e.g. char vs int).
                            // If found, use the matching overload's c_name (with parameter
                            // mangling like `Nova_str_static_from_char`).
                            let key = ("str".to_string(), "from".to_string());
                            if let Some(overloads) = self.method_overloads.get(&key).cloned() {
                                let static_overloads: Vec<MethodSig> = overloads.into_iter()
                                    .filter(|s| !s.is_instance).collect();
                                if !static_overloads.is_empty() {
                                    let v = self.emit_expr(arg.expr())?;
                                    let chosen = static_overloads.iter()
                                        .find(|s| s.param_c_types.len() == 1
                                            && s.param_c_types[0] == arg_ty);
                                    if let Some(sig) = chosen {
                                        return Ok(format!("{}({})", sig.c_name, v));
                                    }
                                }
                            }
                            // Auto-derive: V has @into() -> str?
                            if let Some(into_target) = self.into_targets.get(&arg_type) {
                                if into_target == "str" {
                                    let v = self.emit_expr(arg.expr())?;
                                    return Ok(format!("Nova_{}_method_into({})", arg_type, v));
                                }
                            }
                            let v = self.emit_expr(arg.expr())?;
                            return Ok(if arg_ty == "nova_str" {
                                v
                            } else {
                                format!("nova_int_to_str((nova_int)({}))", v)
                            });
                        }
                    }
                    // Could be a static method call: `Type.method(args)`.
                    let method_name = &parts[1];
                    // Plan 11 Ф.2: используем multi-overload registry —
                    // strict resolution по типам args. Это работает и
                    // при single-overload (тогда match unique без проверки
                    // arg-types). Покрывает overload, и решает single-key
                    // last-wins проблему когда ≥2 типов имеют одноимённый
                    // static с разной сигнатурой.
                    let key = (parts[0].clone(), method_name.clone());
                    if let Some(overloads) = self.method_overloads.get(&key).cloned() {
                        // Только static-overloads (is_instance == false).
                        let static_overloads: Vec<MethodSig> = overloads.into_iter()
                            .filter(|s| !s.is_instance)
                            .collect();
                        if !static_overloads.is_empty() {
                            // Plan 48 Ф.7.2: sentinel detection для static generic
                            // методов с собственными type params. Без этого
                            // sentinel c_name (__mono_method__T__m) утекает в
                            // линковщик как undefined symbol.
                            if static_overloads.iter().any(|c| c.c_name.starts_with("__mono_method__")) {
                                let recv_key = (parts[0].clone(), method_name.clone());
                                if let Some(fn_decl) = self.mono_method_decls.get(&recv_key).cloned() {
                                    let type_subst = self.resolve_mono_type_args(&fn_decl, &[], args)
                                        .unwrap_or_else(|_| fn_decl.generics.iter()
                                            .map(|g| (g.name.clone(), "nova_str".to_string()))
                                            .collect());
                                    let base_c_name = format!("Nova_{}_static_{}", parts[0], method_name);
                                    let mono_name = Self::compute_mono_name(&base_c_name, &type_subst);
                                    let rt_clone = parts[0].clone();
                                    self.register_mono_method_instance(
                                        &fn_decl.clone(), type_subst.clone(),
                                        &mono_name.clone(), &rt_clone);
                                    // Emit args с fn_param_sigs context для closure params.
                                    let mut arg_strs = Vec::new();
                                    for (param_decl, a) in fn_decl.params.iter().zip(args.iter()) {
                                        if let crate::ast::TypeRef::Func { params: fp, return_type, .. } = &param_decl.ty {
                                            let saved_inner = std::mem::replace(
                                                &mut self.current_type_subst,
                                                type_subst.iter().cloned().collect(),
                                            );
                                            let inner_ptys: Vec<String> = fp.iter()
                                                .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                                                .collect();
                                            let inner_ret = return_type.as_ref()
                                                .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
                                                .unwrap_or_else(|| "nova_unit".into());
                                            self.current_type_subst = saved_inner;
                                            let prev_sig = self.fn_param_sigs.insert(
                                                param_decl.name.clone(), (inner_ptys, inner_ret));
                                            let v = self.emit_expr(a.expr())?;
                                            match prev_sig {
                                                Some(old) => { self.fn_param_sigs.insert(param_decl.name.clone(), old); }
                                                None => { self.fn_param_sigs.remove(&param_decl.name); }
                                            }
                                            arg_strs.push(v);
                                        } else {
                                            arg_strs.push(self.emit_expr(a.expr())?);
                                        }
                                    }
                                    for a in args.iter().skip(fn_decl.params.len()) {
                                        arg_strs.push(self.emit_expr(a.expr())?);
                                    }
                                    return Ok(format!("{}({})", mono_name, arg_strs.join(", ")));
                                }
                            }
                            // Эмиттим args сначала, чтобы получить C-типы.
                            let mut arg_strs = Vec::new();
                            let mut arg_types = Vec::new();
                            for a in args {
                                arg_types.push(self.infer_expr_c_type(a.expr()));
                                arg_strs.push(self.emit_expr(a.expr())?);
                            }
                            // Single-overload: short-circuit.
                            let chosen = if static_overloads.len() == 1 {
                                Some(static_overloads[0].clone())
                            } else {
                                // Multi-overload: strict match по arity + types.
                                static_overloads.iter()
                                    .filter(|s| s.param_c_types.len() == arg_types.len())
                                    .filter(|s| s.param_c_types.iter().zip(arg_types.iter())
                                        .all(|(w, g)| w == g))
                                    .next()
                                    .cloned()
                            };
                            if let Some(sig) = chosen {
                                return Ok(format!("{}({})", sig.c_name, arg_strs.join(", ")));
                            }
                            // 0 matches — fallback на старую логику ниже,
                            // которая может найти через method_receivers
                            // (single-key) или auto-derive.
                        }
                    }
                    // Legacy single-key path (для типов которые не
                    // зарегистрированы в method_overloads, например
                    // built-in opaque типы special-case'нутые выше).
                    if let Some((type_name, false)) = self.method_receivers.get(method_name.as_str()).cloned() {
                        // Strict match: type_name must equal parts[0].
                        if type_name == parts[0] {
                            let mut arg_strs = Vec::new();
                            for a in args { arg_strs.push(self.emit_expr(a.expr())?); }
                            return Ok(format!("Nova_{}_static_{}({})", type_name, method_name, arg_strs.join(", ")));
                        }
                    }
                    // D73 v2 auto-derive: `T.from(v)` when no explicit T.from
                    // exists, but `fn V @into() -> T` is defined where v: V.
                    if method_name == "from" && args.len() == 1 {
                        let target = parts[0].clone();
                        let arg_ty = self.infer_expr_c_type(args[0].expr());
                        let arg_type = arg_ty.trim_start_matches("Nova_").trim_end_matches('*').to_string();
                        // Check that V has @into() -> T defined.
                        if let Some(into_target) = self.into_targets.get(&arg_type) {
                            if into_target == &target {
                                let v = self.emit_expr(args[0].expr())?;
                                return Ok(format!("Nova_{}_method_into({})", arg_type, v));
                            }
                        }
                    }
                    format!("nova_fn_{}", parts.join("_"))
                } else {
                    format!("nova_fn_{}", parts.join("_"))
                }
            }
            _ => self.emit_expr(func)?,
        };

        // Plan 14 Ф.1: Option_Some/None — proper-typed compound literal.
        // Раньше эмитились через runtime helper `nova_make_Option_Some(v)`
        // → возвращает NovaOpt_nova_int независимо от T. Теперь —
        // compound literal `((NovaOpt_<T_sanitized>){.tag=Some, .value=(v)})`
        // с реальным T извлечённым:
        //   - для Some(v): T = тип arg'а;
        //   - для None: T = current_fn_return_ty (если NovaOpt_<X>),
        //     иначе fallback на NovaOpt_nova_int (legacy).
        if func_c == "nova_make_Option_Some" && args.len() == 1 {
            let arg = &args[0];
            let arg_ty = self.infer_expr_c_type(arg.expr());
            let arg_v = self.emit_expr(arg.expr())?;
            // Erased generic? — оставляем legacy путь (через
            // нижеследующий nova_make_Option_Some helper). Это покрывает
            // generic fns где T = void* и arg уже cast'нут в (void*).
            let is_erased = matches!(&func.kind, ExprKind::Ident(name)
                if self.generic_fns.contains(name.as_str()));
            if !is_erased && !arg_ty.is_empty() && arg_ty != "void*" {
                let sanitized = Self::sanitize_for_novaopt(&arg_ty);
                self.register_novaopt_decl(&sanitized, &arg_ty);
                return Ok(format!(
                    "((NovaOpt_{}){{.tag = NOVA_TAG_Option_Some, .value = ({})}})",
                    sanitized, arg_v));
            }
            // Fallback (erased/unknown arg-type): legacy helper.
            return Ok(format!("nova_make_Option_Some({})", arg_v));
        }
        if func_c == "nova_make_Option_None" && args.is_empty() {
            // None — T берётся из current_fn_return_ty если это NovaOpt_<X>.
            let opt_ty: String = self.current_fn_return_ty.as_ref()
                .filter(|t| t.starts_with("NovaOpt_"))
                .cloned()
                .unwrap_or_else(|| "NovaOpt_nova_int".into());
            return Ok(format!(
                "(({}){{.tag = NOVA_TAG_Option_None}})", opt_ty));
        }
        // Option/Result_Ok constructors use nova_int storage; nested struct args must be heap-boxed.
        // Result_Err takes nova_str directly. User-defined sum types have proper typed fields.
        let is_option_or_result_ok_ctor = func_c == "nova_make_Option_Some"
            || func_c == "nova_make_Result_Ok";
        // Plan 48: detect monomorphizable call (generic free fn, turbofish or inferred)
        let (mono_fn_name_opt, turbofish_type_refs): (Option<String>, Vec<crate::ast::TypeRef>) =
            match &func.kind {
                ExprKind::Ident(name) if self.generic_fns.contains(name.as_str()) => {
                    (Some(name.clone()), vec![])
                }
                ExprKind::TurboFish { base, type_args } => {
                    if let ExprKind::Ident(name) = &base.kind {
                        if self.generic_fns.contains(name.as_str()) {
                            (Some(name.clone()), type_args.clone())
                        } else { (None, vec![]) }
                    } else { (None, vec![]) }
                }
                _ => (None, vec![]),
            };

        if let Some(ref fn_name) = mono_fn_name_opt {
            if let Some(fn_decl) = self.mono_fn_decls.get(fn_name).cloned() {
                // Plan 48 V1: skip monomorphization for tuple-returning generics.
                // _NovaTupleN structs use nova_int fields (type erasure within tuples),
                // so storing a nova_str field directly doesn't work. Tuples continue
                // to use the void* erased path in V1.
                let has_tuple_return = matches!(fn_decl.return_type, Some(crate::ast::TypeRef::Tuple(..)));
                if has_tuple_return {
                    // Force erasure fallback for tuple-returning generic fns.
                    self.register_erased_instance(&fn_decl.clone());
                    let erased_name = format!("nova_fn_{}", fn_name);
                    let mut arg_strs = Vec::new();
                    for a in args.iter() {
                        let arg_ty = self.infer_expr_c_type(a.expr());
                        let v = self.emit_expr(a.expr())?;
                        if arg_ty == "nova_str" {
                            let heap_tmp = self.fresh_tmp();
                            self.line(&format!("nova_str* {} = (nova_str*)nova_alloc(sizeof(nova_str));", heap_tmp));
                            self.line(&format!("*{} = {};", heap_tmp, v));
                            arg_strs.push(format!("(void*)({})", heap_tmp));
                        } else if arg_ty.ends_with('*') || arg_ty == "void*" {
                            arg_strs.push(format!("(void*)({})", v));
                        } else {
                            arg_strs.push(format!("(void*)(intptr_t)({})", v));
                        }
                    }
                    return Ok(format!("{}({})", erased_name, arg_strs.join(", ")));
                }
                // Ф.0: resolve type args
                match self.resolve_mono_type_args(&fn_decl, &turbofish_type_refs, args) {
                    Ok(type_subst) => {
                        let base_c_name = format!("nova_fn_{}", fn_name);
                        let mono_name = Self::compute_mono_name(&base_c_name, &type_subst);
                        // Register instance (forward decl + worklist)
                        self.register_mono_instance(&fn_decl.clone(), type_subst.clone(), &mono_name.clone());
                        // Emit args WITHOUT boxing — concrete types
                        // For fn-typed params (closures): set up fn_param_sigs context
                        let mut arg_strs = Vec::new();
                        for (param_decl, a) in fn_decl.params.iter().zip(args.iter()) {
                            if let crate::ast::TypeRef::Func { params: fp, return_type, .. } = &param_decl.ty {
                                // Temporarily set type subst for inner type resolution
                                let saved_inner = std::mem::replace(
                                    &mut self.current_type_subst,
                                    type_subst.iter().cloned().collect(),
                                );
                                let inner_ptys: Vec<String> = fp.iter()
                                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                                    .collect();
                                let inner_ret = return_type.as_ref()
                                    .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_unit".into()))
                                    .unwrap_or_else(|| "nova_unit".into());
                                self.current_type_subst = saved_inner;
                                // Register temporarily so closure body knows the concrete sig
                                let prev_sig = self.fn_param_sigs.insert(
                                    param_decl.name.clone(), (inner_ptys, inner_ret));
                                let v = self.emit_expr(a.expr())?;
                                match prev_sig {
                                    Some(old) => { self.fn_param_sigs.insert(param_decl.name.clone(), old); }
                                    None => { self.fn_param_sigs.remove(&param_decl.name); }
                                }
                                arg_strs.push(v);
                            } else {
                                arg_strs.push(self.emit_expr(a.expr())?);
                            }
                        }
                        // Handle case where args list is longer than params (guard)
                        for a in args.iter().skip(fn_decl.params.len()) {
                            arg_strs.push(self.emit_expr(a.expr())?);
                        }
                        return Ok(format!("{}({})", mono_name, arg_strs.join(", ")));
                    }
                    Err(msg) => {
                        // Plan 48 Ф.7.3: cannot-infer → compile error, not silent erasure.
                        return Err(msg);
                    }
                }
            }
            // fn_decl not found — shouldn't happen if generic_fns is consistent with mono_fn_decls.
            // Fall through to old erasure path as safety net.
        }

        // Non-generic (or generic method — still erased in V1) call path.
        // is_generic_call is true for variant constructors on generic sum types
        // whose params are erased to `void*`. Plan 48 Ф.7.4 (partial): when
        // try_infer_variant_mono_args succeeds, the constructor is routed to
        // the monomorphized instance which expects concrete types — skip the
        // void* boxing so the arg type matches the mono signature.
        let is_generic_call = if let ExprKind::Ident(name) = &func.kind {
            if let Some((type_name, _)) = self.find_variant(name) {
                self.generic_types.contains(&type_name)
                    && self.try_infer_variant_mono_args(name, args).is_none()
            } else { false }
        } else { false };
        // Bidirectional inference: extract callee name for HOF context lookup.
        let callee_name_for_hof: Option<String> = match &func.kind {
            ExprKind::Ident(n) => Some(n.clone()),
            _ => None,
        };
        let mut arg_strs = Vec::new();
        for (arg_idx, a) in args.iter().enumerate() {
            let a: &CallArg = a;
            // Bidirectional inference: if this arg is an untyped ClosureLight,
            // look up the HOF's declared parameter type at this position and
            // pass it as context_param_tys to emit_lambda so `|x| x + 1` infers
            // `x: T` from the callee's signature rather than defaulting to nova_int.
            if let ExprKind::ClosureLight { params, body } = &a.expr().kind {
                if let Some(ref cname) = callee_name_for_hof {
                    if let Some(inner_sig) = self.hof_param_fn_sigs.get(&(cname.clone(), arg_idx)).cloned() {
                        let legacy_params: Vec<LambdaParam> = params
                            .iter()
                            .map(|p| LambdaParam { name: p.name.clone(), ty: None, span: p.span })
                            .collect();
                        let body_expr: Expr = match body {
                            crate::ast::ClosureBody::Expr(e) => (**e).clone(),
                            crate::ast::ClosureBody::Block(b) => Expr::new(
                                ExprKind::Block(b.clone()),
                                b.span,
                            ),
                        };
                        // Build context_param_tys: [(c_type, "")] per inner param.
                        let ctx: Vec<(String, String)> = inner_sig.0.iter()
                            .map(|t| (t.clone(), String::new()))
                            .collect();
                        let v = self.emit_lambda(&legacy_params, &body_expr, Some(&ctx), None)?;
                        arg_strs.push(v);
                        continue;
                    }
                }
            }
            let arg_ty = if is_option_or_result_ok_ctor || is_generic_call {
                self.infer_expr_c_type(a.expr())
            } else { String::new() };
            let v = self.emit_expr(a.expr())?;
            if is_generic_call {
                // For generic (void*-erased) functions: nova_str must be boxed as pointer
                if arg_ty == "nova_str" {
                    let heap_tmp = self.fresh_tmp();
                    self.line(&format!("nova_str* {} = (nova_str*)nova_alloc(sizeof(nova_str));", heap_tmp));
                    self.line(&format!("*{} = {};", heap_tmp, v));
                    arg_strs.push(format!("(void*)({})", heap_tmp));
                } else if arg_ty.ends_with('*') || arg_ty == "void*" {
                    arg_strs.push(format!("(void*)({})", v));
                } else {
                    arg_strs.push(format!("(void*)(intptr_t)({})", v));
                }
            } else if is_option_or_result_ok_ctor && (arg_ty.starts_with("NovaOpt_") || arg_ty.starts_with("_NovaTuple")) && !arg_ty.ends_with('*') {
                // Option.Some and Result.Ok take nova_int; struct-valued args must be heap-boxed
                let heap_tmp = self.fresh_tmp();
                self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", arg_ty, heap_tmp, arg_ty, arg_ty));
                self.line(&format!("*{} = {};", heap_tmp, v));
                arg_strs.push(format!("(nova_int)({})", heap_tmp));
                // Record that the next variable bound will have an inner boxed type
                self.pending_option_inner_type = Some(format!("{}*", arg_ty));
            } else {
                arg_strs.push(v);
            }
        }
        Ok(format!("{}({})", func_c, arg_strs.join(", ")))
    }

    fn emit_println(&mut self, args: &[CallArg], newline: bool) -> Result<String, String> {
        // We emit a statement-expression block that prints each arg.
        // Since emit_expr returns a C expression, we use a GNU statement-expr ({ ... value })
        // or just emit statements and return NOVA_UNIT.
        // Strategy: emit print calls as statements, capture in tmp.
        let tmp = self.fresh_tmp_named("println");
        self.line(&format!("nova_unit {};", tmp));
        self.line("{");
        self.indent += 1;
        for call_arg in args {
            // Plan 14 Ф.6: print/println всё ещё special-case;
            // spread в нём пока не поддержан (отдельная задача).
            if call_arg.is_spread() {
                return Err("spread (...) в println/print пока не поддержан".into());
            }
            let arg = call_arg.expr();
            let val = self.emit_expr(arg)?;
            // Detect type by AST node to pick correct print helper
            let print_call = self.make_print_call(arg, &val)?;
            self.line(&format!("{};", print_call));
        }
        if newline {
            self.line("nova_print_newline();");
        }
        self.indent -= 1;
        self.line("}");
        self.line(&format!("{} = NOVA_UNIT;", tmp));
        Ok(tmp)
    }

    fn make_print_call(&self, expr: &Expr, val: &str) -> Result<String, String> {
        // Use AST-level type info to pick the right print function.
        // Without a full type system, we use best-effort heuristics.
        let helper = self.infer_print_helper(expr);
        Ok(format!("{}({})", helper, val))
    }

    fn infer_print_helper(&self, expr: &Expr) -> &'static str {
        match &expr.kind {
            ExprKind::IntLit(_) => "nova_print_int",
            ExprKind::FloatLit(_) => "nova_print_f64",
            ExprKind::BoolLit(_) => "nova_print_bool",
            ExprKind::StrLit(_) => "nova_print_str",
            ExprKind::InterpolatedStr { .. } => "nova_print_str",
            ExprKind::Binary { op, .. } => match op {
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Le
                | BinOp::Gt | BinOp::Ge | BinOp::And | BinOp::Or
                | BinOp::Implies | BinOp::Iff => "nova_print_bool",
                _ => "nova_print_int",
            },
            ExprKind::Ident(name) => {
                // Look up variable type
                match self.var_types.get(name).map(|s| s.as_str()) {
                    Some("nova_f64") | Some("nova_f32") => "nova_print_f64",
                    Some("nova_bool") => "nova_print_bool",
                    Some("nova_str") => "nova_print_str",
                    _ => "nova_print_int",
                }
            }
            ExprKind::Member { obj, name } => {
                let obj_ty = self.infer_expr_c_type_str(obj);
                // nova_str.len → nova_print_int
                if obj_ty == "nova_str" && name == "len" {
                    return "nova_print_int";
                }
                // Determine object's struct type, then look up field type in schema
                let struct_name = obj_ty
                    .strip_prefix("Nova_")
                    .unwrap_or("")
                    .trim_end_matches('*')
                    .trim()
                    .to_string();
                if let Some(schema) = self.record_schemas.get(&struct_name) {
                    match schema.get(name.as_str()).map(|s| s.as_str()) {
                        Some("nova_f64") | Some("nova_f32") => "nova_print_f64",
                        Some("nova_bool") => "nova_print_bool",
                        Some("nova_str") => "nova_print_str",
                        _ => "nova_print_int",
                    }
                } else {
                    "nova_print_int"
                }
            }
            ExprKind::Call { func, .. } => {
                // String method calls: s.to_upper() → nova_print_str, s.starts_with() → nova_print_bool
                if let ExprKind::Member { obj, name: method } = &func.kind {
                    let obj_ty = self.infer_expr_c_type_str(obj);
                    if obj_ty == "nova_str" {
                        return match method.as_str() {
                            "to_upper" | "to_lower" | "trim" | "slice" | "concat" => "nova_print_str",
                            "starts_with" | "ends_with" | "contains" | "eq" => "nova_print_bool",
                            _ => "nova_print_int",
                        };
                    }
                }
                // Infer return type from function's known type in var_types
                if let ExprKind::Ident(name) = &func.kind {
                    let key = format!("fn_ret_{}", name);
                    match self.var_types.get(&key).map(|s| s.as_str()) {
                        Some("nova_str") => "nova_print_str",
                        Some("nova_f64") | Some("nova_f32") => "nova_print_f64",
                        Some("nova_bool") => "nova_print_bool",
                        _ => "nova_print_int",
                    }
                } else {
                    "nova_print_int"
                }
            }
            _ => "nova_print_int",
        }
    }

    // ---- if expression ----

    fn emit_if_expr(
        &mut self,
        cond: &Expr,
        then: &Block,
        else_: Option<&ElseBranch>,
    ) -> Result<String, String> {
        // Plan 08 Ф.4: strict `if cond: bool`. Spec D54: cond обязан быть
        // bool, не truthy-int (Rust/Swift/Kotlin прецедент). Закрывает
        // silent-bug class. Conservative — error только если ОЧЕВИДНО
        // non-bool (numeric/str). type-neutral (void*) — пропускаем.
        let cond_ty = self.infer_expr_c_type(cond);
        // Plan 55 Ф.4 debug tool: NOVA_DEBUG_MONO=1 dumps non-bool conds during
        // codegen to help diagnose protocol-method dispatch / type-context corruption.
        if std::env::var("NOVA_DEBUG_MONO").is_ok() && cond_ty != "nova_bool" && cond_ty != "void*" {
            eprintln!("DEBUG-MONO if-cond NON-BOOL: ty={} kind={:?} fn={:?} subst={:?}",
                cond_ty, cond.kind, self.current_fn_return_ty,
                self.current_type_subst.iter().collect::<Vec<_>>());
        }
        self.check_bool_condition_at(&cond_ty, "if", cond.span)?;
        // Infer result type from then-block (if any trailing), default nova_unit
        let if_ty = if else_.is_none() {
            "nova_unit".into()
        } else {
            then.trailing.as_ref()
                .map(|e| self.infer_expr_c_type(e))
                .unwrap_or_else(|| "nova_unit".into())
        };
        let cond_val = self.emit_expr(cond)?;
        let tmp = self.fresh_tmp_named("if");
        self.line(&format!("{} {};", if_ty, tmp));
        self.var_types.insert(tmp.clone(), if_ty.clone());
        self.line(&format!("if ({}) {{", cond_val));
        self.indent += 1;
        self.emit_block_into(&tmp, &if_ty, then)?;
        self.indent -= 1;
        match else_ {
            None => {
                self.line("}");
            }
            Some(ElseBranch::Block(b)) => {
                self.line("} else {");
                self.indent += 1;
                self.emit_block_into(&tmp, &if_ty, b)?;
                self.indent -= 1;
                self.line("}");
            }
            Some(ElseBranch::If(e)) => {
                self.line("} else {");
                self.indent += 1;
                // target-type-aware: literal-cleanup для typed-int if-result.
                let v = self.emit_expr_with_target_type(e, &if_ty)?;
                let ity = if_ty.clone();
                Self::emit_assign_typed(self, &tmp, &ity, &v);
                self.indent -= 1;
                self.line("}");
            }
        }
        Ok(tmp)
    }

    /// Emit a block's statements and assign its trailing value (or NOVA_UNIT) into `tmp`.
    fn emit_block_into(&mut self, tmp: &str, ty: &str, block: &Block) -> Result<(), String> {
        let block_id = self.enter_defer_scope(block, false);
        for stmt in &block.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &block.trailing {
            // target-type-aware emit: для typed-integer ty литералы в Binary
            // получают «нативный» suffix вместо ((nova_int)NLL).
            let v = self.emit_expr_with_target_type(trailing, ty)?;
            Self::emit_assign_typed(self, tmp, ty, &v);
        } else {
            Self::emit_zero_assign(self, tmp, ty);
        }
        // Cleanup AFTER assigning result (defer should not affect tmp).
        self.leave_defer_scope(block_id);
        Ok(())
    }

    /// Emit `tmp = v` with appropriate handling for different C types.
    fn is_struct_type(ty: &str) -> bool {
        ty == "nova_unit" || ty.contains("nova_str") || ty.starts_with("Nova_")
            || ty.starts_with("struct ") || ty.starts_with("NovaVtable_")
            || ty.starts_with("NovaOpt_") || ty.starts_with("_NovaTuple")
    }

    fn emit_assign_typed(&mut self, tmp: &str, ty: &str, v: &str) {
        if ty == "nova_unit" {
            // nova_unit is a struct — can't cast, just discard value and assign NOVA_UNIT
            self.line(&format!("{} = NOVA_UNIT; (void)({});", tmp, v));
        } else if Self::is_struct_type(ty) {
            // Struct/pointer — direct assignment, no cast
            self.line(&format!("{} = {};", tmp, v));
        } else {
            // Scalar (nova_int, nova_bool, nova_f64, etc.) — cast
            self.line(&format!("{} = ({})({});", tmp, ty, v));
        }
    }

    fn emit_zero_assign(&mut self, tmp: &str, ty: &str) {
        if ty == "nova_unit" {
            self.line(&format!("{} = NOVA_UNIT;", tmp));
        } else if Self::is_struct_type(ty) {
            self.line(&format!("memset(&{}, 0, sizeof({}));", tmp, tmp));
        } else {
            self.line(&format!("{} = ({})0;", tmp, ty));
        }
    }

    // ---- interpolated string (D44, Plan 17 Ф.4) ----

    /// Эмитит `"... ${expr} ..."` через цепочку StringBuilder.
    ///
    /// Стратегия:
    ///   1. Pre-size estimate из literal-частей (точная сумма длин
    ///      литералов + 16 байт на каждое interpolation-выражение —
    ///      эвристика для int/bool, для длинных значений SB сам grow'ит).
    ///   2. Создаём `Nova_StringBuilder*` с этой capacity.
    ///   3. Для каждой части эмитим `append_str(...)` (для literal —
    ///      direct nova_str, для expr — приведение к str через type-
    ///      dispatch: nova_str pass-through, nova_bool → nova_bool_to_str,
    ///      nova_f64 → nova_f64_to_str, char-литерал → nova_char_to_str
    ///      (UTF-8 encode), всё остальное → nova_int_to_str).
    ///   4. Финализация: `Nova_StringBuilder_method_into(sb) -> nova_str`.
    ///
    /// Одна аллокация под итоговый buffer (вместо O(N²) от цепочки `+`).
    fn emit_interpolated_str(
        &mut self,
        parts: &[InterpStrPart],
    ) -> Result<String, String> {
        // Pre-size estimate: точные литералы + 16 байт на expr.
        let mut estimate: usize = 0;
        for p in parts {
            match p {
                InterpStrPart::Lit(s) => estimate += s.len(),
                InterpStrPart::Expr(_) => estimate += 16,
            }
        }
        let sb = self.fresh_tmp_named("interp_sb");
        self.line(&format!(
            "Nova_StringBuilder* {} = Nova_StringBuilder_static_with_capacity({});",
            sb, estimate
        ));
        for p in parts {
            match p {
                InterpStrPart::Lit(s) => {
                    if s.is_empty() {
                        continue;
                    }
                    let escaped = Self::escape_c_str(s);
                    self.line(&format!(
                        "Nova_StringBuilder_method_append_str({}, (nova_str){{.ptr=\"{}\", .len={}}});",
                        sb, escaped, s.len()
                    ));
                }
                InterpStrPart::Expr(e) => {
                    let arg_ty = self.infer_expr_c_type(e);
                    // CharLit detection — char хранится как nova_int,
                    // но семантика char→str = UTF-8 encode codepoint, а не печать числа.
                    let v = self.emit_expr(e)?;
                    let s_expr = if matches!(e.kind, ExprKind::CharLit(_)) {
                        format!("nova_char_to_str({})", v)
                    } else {
                        match arg_ty.as_str() {
                            "nova_str" => v,
                            "nova_bool" => format!("nova_bool_to_str({})", v),
                            "nova_f64" => format!("nova_f64_to_str({})", v),
                            "nova_int" => format!("nova_int_to_str({})", v),
                            _ => {
                                // User-type: ищем @into() -> str через D73.
                                let arg_type = arg_ty
                                    .trim_start_matches("Nova_")
                                    .trim_end_matches('*')
                                    .to_string();
                                if let Some(into_target) = self.into_targets.get(&arg_type) {
                                    if into_target == "str" {
                                        format!("Nova_{}_method_into({})", arg_type, v)
                                    } else {
                                        format!("nova_int_to_str((nova_int)({}))", v)
                                    }
                                } else {
                                    format!("nova_int_to_str((nova_int)({}))", v)
                                }
                            }
                        }
                    };
                    self.line(&format!(
                        "Nova_StringBuilder_method_append_str({}, {});",
                        sb, s_expr
                    ));
                }
            }
        }
        let result = self.fresh_tmp_named("interp_str");
        self.line(&format!(
            "nova_str {} = Nova_StringBuilder_method_into({});",
            result, sb
        ));
        self.var_types
            .insert(result.clone(), "nova_str".to_string());
        Ok(result)
    }

    // ---- block expression ----

    fn emit_block_expr(&mut self, block: &Block) -> Result<String, String> {
        let tmp = self.fresh_tmp();
        // Infer block type from trailing expression (if any)
        let block_ty = block.trailing.as_ref()
            .map(|e| self.infer_expr_c_type(e))
            .unwrap_or_else(|| "nova_unit".into());
        self.line(&format!("{} {};", block_ty, tmp));
        self.var_types.insert(tmp.clone(), block_ty.clone());
        self.line("{");
        self.indent += 1;
        let block_id = self.enter_defer_scope(block, false);
        for stmt in &block.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &block.trailing {
            let v = self.emit_expr(trailing)?;
            let bty = block_ty.clone();
            Self::emit_assign_typed(self, &tmp, &bty, &v);
        } else {
            let bty = block_ty.clone();
            Self::emit_zero_assign(self, &tmp, &bty);
        }
        // Defer cleanup AFTER assigning result tmp (defer body не влияет
        // на значение block-expr).
        self.leave_defer_scope(block_id);
        self.indent -= 1;
        self.line("}");
        Ok(tmp)
    }

    // ---- for loop ----

    fn emit_for(&mut self, pattern: &Pattern, iter: &Expr, body: &Block) -> Result<String, String> {
        // Case 1: `for i in start..end`
        if let ExprKind::Range { start, end, inclusive } = &iter.kind {
            let binding = self.pattern_binding(pattern)?;
            let s = self.emit_expr(start)?;
            let e = self.emit_expr(end)?;
            let cmp = if *inclusive { "<=" } else { "<" };
            let tmp = self.fresh_tmp();
            self.line(&format!("nova_unit {};", tmp));
            self.line(&format!(
                "for (nova_int {} = {}; {} {} {}; {}++) {{",
                binding, s, binding, cmp, e, binding
            ));
            self.indent += 1;
            // Register loop-var so spawn-capture inside body can find it.
            // Range loop-var is immutable scalar (the for-loop drives it,
            // user shouldn't mutate it) — fits by-value capture.
            let prev_ty = self.var_types.insert(binding.clone(), "nova_int".to_string());
            let was_mut = self.var_mutable.remove(&binding);
            // Plan 20 Ф.4: defer/errdefer внутри loop body должен выполняться
            // на каждой итерации (LIFO, fail-frame throw-path).
            self.emit_loop_body_inline(body)?;
            // Restore prior state.
            match prev_ty {
                Some(t) => { self.var_types.insert(binding.clone(), t); }
                None => { self.var_types.remove(&binding); }
            }
            if was_mut { self.var_mutable.insert(binding); }
            self.indent -= 1;
            self.line("}");
            self.line(&format!("{} = NOVA_UNIT;", tmp));
            return Ok(tmp);
        }

        // Case 2: `for elem in array_expr`
        // Emit: { NovaArray_T* _arr = <iter>; for (int64_t _i = 0; _i < _arr->len; _i++) { T elem = _arr->data[_i]; ... } }
        let arr_ty = self.infer_expr_c_type(iter);
        if arr_ty.starts_with("NovaArray_") {
            let binding = self.pattern_binding(pattern)?;
            let arr_tmp = self.fresh_tmp();
            let idx_tmp = self.fresh_tmp();
            let result_tmp = self.fresh_tmp();

            let arr_expr = self.emit_expr(iter)?;
            self.line(&format!("{} {} = {};", arr_ty, arr_tmp, arr_expr));
            self.var_types.insert(arr_tmp.clone(), arr_ty.clone());

            // Check if the array stores a real element type other than nova_int
            // (e.g. _NovaTuple2* stored as nova_int pointer-stomp)
            let real_elem_ty = if let ExprKind::Ident(n) = &iter.kind {
                self.array_element_types.get(n.as_str()).cloned()
            } else {
                self.array_element_types.get(arr_expr.as_str()).cloned()
            };
            let elem_ty = real_elem_ty.unwrap_or_else(|| {
                arr_ty.strip_prefix("NovaArray_").unwrap_or("nova_int")
                    .trim_end_matches('*').trim().to_string()
            });

            self.line(&format!("nova_unit {};", result_tmp));
            self.line(&format!(
                "for (nova_int {} = 0; {} < {}->len; {}++) {{",
                idx_tmp, idx_tmp, arr_tmp, idx_tmp
            ));
            self.indent += 1;
            // If element type is a pointer stored as nova_int, cast back
            if elem_ty.ends_with('*') && elem_ty != "nova_int*" {
                self.line(&format!("{} {} = ({}){}->data[{}];",
                    elem_ty, binding, elem_ty, arr_tmp, idx_tmp));
            } else {
                self.line(&format!("{} {} = {}->data[{}];", elem_ty, binding, arr_tmp, idx_tmp));
            }
            self.var_types.insert(binding.clone(), elem_ty.clone());
            // Plan 55 Ф.1: array stored as NovaArray_void_p (= array of closures).
            // Register binding as fn-typed so `f()` routes through NOVA_CLOS_CALL_*.
            // Source of sig: array_param_fn_sigs (when iter is a fn-param) or by
            // peeking the array literal's first closure element.
            if arr_ty == "NovaArray_void_p*" {
                let sig_opt: Option<(Vec<String>, String)> = if let ExprKind::Ident(n) = &iter.kind {
                    self.array_param_fn_sigs.get(n.as_str()).cloned()
                        .or_else(|| self.fn_param_sigs.get(n.as_str()).cloned())
                } else {
                    None
                };
                if let Some(sig) = sig_opt {
                    self.fn_param_sigs.insert(binding.clone(), sig);
                }
            }
            // Plan 48 Ф.8.2: tuple-pattern in array for-loop.
            // `for (k, v) in pairs` where pairs is [](K, V) (array of tuples).
            // Elements are stored as nova_int (pointer-stomped _NovaTupleN*).
            // In a mono context, tuple_element_types[param_name] has the concrete
            // field types (e.g. ["nova_str*", "nova_int"] for K=str, V=int).
            if let Pattern::Tuple(parts, _) = pattern {
                let arity = parts.len();
                let field_tys_opt = if let ExprKind::Ident(n) = &iter.kind {
                    self.tuple_element_types.get(n.as_str()).cloned()
                } else {
                    self.tuple_element_types.get(arr_expr.as_str()).cloned()
                };
                let tup_tmp = self.fresh_tmp();
                // elem_ty == "nova_int" means the element is a heap-pointer to _NovaTupleN.
                // Otherwise it's the tuple struct stored directly.
                if elem_ty == "nova_int" {
                    self.line(&format!("_NovaTuple{} {} = *(_NovaTuple{}*)(intptr_t){};",
                        arity, tup_tmp, arity, binding));
                } else {
                    self.line(&format!("_NovaTuple{} {} = {};", arity, tup_tmp, binding));
                }
                for (i, p) in parts.iter().enumerate() {
                    let fty = field_tys_opt.as_ref()
                        .and_then(|v| v.get(i))
                        .cloned()
                        .unwrap_or_else(|| "nova_int".to_string());
                    match p {
                        Pattern::Ident { name, .. } => {
                            if fty.ends_with('*') {
                                // Field was heap-boxed (e.g. nova_str* → cast + deref)
                                let base_ty = fty.trim_end_matches('*');
                                self.line(&format!("{} {} = *(({}*)(intptr_t){}.f{});",
                                    base_ty, name, base_ty, tup_tmp, i));
                                self.var_types.insert(name.clone(), base_ty.to_string());
                            } else {
                                self.line(&format!("{} {} = ({})({}.f{});",
                                    fty, name, fty, tup_tmp, i));
                                self.var_types.insert(name.clone(), fty.clone());
                            }
                        }
                        Pattern::Wildcard(_) => {
                            self.line(&format!("(void)({}.f{});", tup_tmp, i));
                        }
                        _ => {}
                    }
                }
            }
            // Plan 20 Ф.4/Ф.8: for-in-array body defer/errdefer integration.
            self.emit_loop_body_inline(body)?;
            self.indent -= 1;
            self.line("}");
            self.line(&format!("{} = NOVA_UNIT;", result_tmp));
            return Ok(result_tmp);
        }

        // Plan 06 Ф.1 + Plan 39 Issue D: Iter[T] protocol (D58 §«implicit iter»).
        //
        // Algorithm (D58):
        //   1. Если c has `mut next() -> Option[T]` — use directly.
        //   2. Иначе если c has `iter() -> Iter[T]` — synthesize `c.iter()`,
        //      recurse.
        //   3. Иначе — error: «type 'X' has neither `next` nor `iter` method».
        //
        // Case 1 ниже; Case 2 — see "Plan 06 Ф.3: implicit `.iter()`" блок.
        // Case 3 — final error message в конце функции (улучшенный).
        let iter_struct = arr_ty.strip_prefix("Nova_").unwrap_or("")
            .trim_end_matches('*').trim().to_string();
        // For monomorphized iterator types like `KeysIter____nova_str__nova_int`,
        // `all_methods` only has the base `KeysIter` entry. Extract the base by
        // splitting on the mono separator `____` and check both.
        let iter_struct_base: String = iter_struct.split("____").next()
            .unwrap_or(&iter_struct).to_string();
        let next_in_all = self.all_methods.contains(&(iter_struct.clone(), "next".to_string()))
            || self.all_methods.contains(&(iter_struct_base.clone(), "next".to_string()));
        if !iter_struct.is_empty() && next_in_all {
            let iter_type = iter_struct.clone();
            {
                let binding = self.pattern_binding(pattern)?;
                let it_tmp = self.fresh_tmp();
                let opt_tmp = self.fresh_tmp();
                let result_tmp = self.fresh_tmp();

                let it_expr = self.emit_expr(iter)?;
                self.line(&format!("{} {} = {};", arr_ty, it_tmp, it_expr));
                self.var_types.insert(it_tmp.clone(), arr_ty.clone());

                self.line(&format!("nova_unit {};", result_tmp));
                self.line("for (;;) {");
                self.indent += 1;
                // Plan 14 Ф.1: правильно типизированный NovaOpt_<T>.
                //
                // method_overloads[(iter_struct, "next")].return_c_type
                // теперь отражает реальный NovaOpt_<T> (после рефактора
                // type_ref_to_c). Используем его как тип container'а
                // и берём поле .value напрямую как T (без cast'а).
                //
                // Tuple-pattern destructure (Pattern::Tuple) — payload
                // boxed как nova_int с intptr_t-cast'ом в tuple-pointer.
                // Это рабочий legacy-путь, оставляем для tuple-iter'ов
                // которые используют generic-erased `Option[(K,V)]` (с
                // nova_int box) — обычный путь для HashMap.iter() etc.
                let next_sig = self.method_overloads
                    .get(&(iter_type.clone(), "next".to_string()))
                    .or_else(|| self.method_overloads
                        .get(&(iter_struct_base.clone(), "next".to_string())))
                    .and_then(|sigs| sigs.first()).cloned();
                // For mono'd iterator types like `KeysIter____nova_str__nova_int`, the base
                // method_overloads sig has the erased return type (NovaOpt_nova_int). Override
                // with the proper mono return type by substituting the iterator's type args.
                // Also register the mono `next` instance so its body actually gets emitted.
                let mono_return_ty: Option<String> = if iter_struct != iter_struct_base {
                    let info = self.generic_type_instance_info.borrow();
                    let instance_opt = info.get(&format!("Nova_{}", iter_struct)).cloned();
                    drop(info);
                    if let Some((base_name, type_args_c)) = instance_opt {
                        let tmpl_opt = self.generic_type_templates.get(&base_name).cloned();
                        let method_opt = self.generic_type_methods.get(&base_name)
                            .and_then(|ms| ms.iter().find(|m| m.name == "next"))
                            .cloned();
                        if let (Some(tmpl), Some(method_decl)) = (tmpl_opt, method_opt) {
                            let type_subst: Vec<(String, String)> = tmpl.generics.iter()
                                .zip(type_args_c.iter())
                                .map(|(g, c)| (g.name.clone(), c.clone()))
                                .collect();
                            let subst_opt: Vec<(String, Option<String>)> = type_subst.iter()
                                .map(|(n, t)| (n.clone(), Some(t.clone()))).collect();
                            // Register the mono instance so the body gets emitted.
                            // Mono name matches the call emitted below: Nova_<iter_struct>_method_next.
                            let mono_call_name = format!("Nova_{}_method_next", iter_struct);
                            self.register_mono_method_instance(
                                &method_decl,
                                type_subst,
                                &mono_call_name,
                                &iter_struct,
                            );
                            method_decl.return_type.as_ref()
                                .and_then(|rt| Self::apply_type_subst_to_ref(rt, &subst_opt))
                        } else { None }
                    } else { None }
                } else { None };
                // Plan 39 Issue D: D58 требует `mut next()` — iterator advance
                // мутирует state. Warning'аем если registered как non-mut
                // (не блокируем — bootstrap может иметь edge cases с
                // structural-protocol matching).
                if let Some(sig) = &next_sig {
                    if !sig.is_instance {
                        // Static `next` — это не iterator method.
                        return Err(format!(
                            "for-in: type '{}' has `next` but it's static, not instance method (D58: `mut next() -> Option[T]` required)",
                            iter_type));
                    }
                }
                // Prefer the mono'd return type when available (KeysIter[str, int]
                // → NovaOpt_nova_str, not NovaOpt_nova_int from the erased base sig).
                let opt_c_ty = mono_return_ty
                    .or_else(|| next_sig.as_ref().map(|s| s.return_c_type.clone()))
                    .unwrap_or_else(|| "NovaOpt_nova_int".to_string());
                let elem_c_ty = opt_c_ty.strip_prefix("NovaOpt_")
                    .map(str::to_string)
                    .unwrap_or_else(|| "nova_int".to_string());
                self.line(&format!(
                    "{} {} = Nova_{}_method_next({});",
                    opt_c_ty, opt_tmp, iter_type, it_tmp));
                self.line(&format!(
                    "if ({}.tag == NOVA_TAG_Option_None) break;", opt_tmp));
                if let Pattern::Tuple(parts, _) = pattern {
                    let arity = parts.len();
                    if elem_c_ty == format!("_NovaTuple{}", arity) {
                        // Plan 14 Ф.1: NovaOpt_<_NovaTuple_N> хранит tuple
                        // как value напрямую (struct в struct). Direct
                        // copy через `_NovaTupleN binding = opt.value;`.
                        self.line(&format!(
                            "_NovaTuple{} {} = {}.value;",
                            arity, binding, opt_tmp));
                    } else if elem_c_ty == "nova_int" {
                        // Legacy путь: payload — nova_int box, intptr_t
                        // cast'ом превращается в _NovaTupleN*.
                        self.line(&format!(
                            "_NovaTuple{} {} = ({}.value == 0) ? (_NovaTuple{}){{0}} : *((_NovaTuple{}*)(intptr_t)({}.value));",
                            arity, binding, opt_tmp, arity, arity, opt_tmp));
                    } else if elem_c_ty == format!("_NovaTuple{}_p", arity)
                        || elem_c_ty == format!("_NovaTuple{}*", arity)
                    {
                        // Pointer-typed tuple: `*opt.value` для destructure.
                        self.line(&format!(
                            "_NovaTuple{} {} = ({}.value == NULL) ? (_NovaTuple{}){{0}} : *({}.value);",
                            arity, binding, opt_tmp, arity, opt_tmp));
                    } else {
                        return Err(format!(
                            "for-in tuple-pattern: непонятный elem-type `{}`", elem_c_ty));
                    }
                    self.pattern_destructure_tuple(pattern, &binding, false)?;
                } else {
                    self.line(&format!(
                        "{} {} = {}.value;", elem_c_ty, binding, opt_tmp));
                    self.var_types.insert(binding.clone(), elem_c_ty);
                }

                // Plan 20 Ф.4/Ф.8: for-in-iter (Iter[T] protocol) body
                // defer/errdefer integration на каждой итерации.
                self.emit_loop_body_inline(body)?;

                self.indent -= 1;
                self.line("}");
                self.line(&format!("{} = NOVA_UNIT;", result_tmp));
                return Ok(result_tmp);
            }
        }

        // Plan 06 Ф.3: implicit `.iter()` для коллекций.
        // Если у типа НЕТ метода `next` (значит это не Iter), но ЕСТЬ
        // метод `iter` — синтезируем call: `for x in coll` →
        // `for x in coll.iter()` и дёргаемся обратно. Без infinite loop:
        // у получаемого результата ДОЛЖЕН быть `next` (иначе error).
        if !iter_struct.is_empty()
            && self.all_methods.contains(&(iter_struct.clone(), "iter".to_string()))
        {
            // Synthesize Member-call: iter.iter()
            let iter_call = Expr {
                kind: ExprKind::Call {
                    func: Box::new(Expr {
                        kind: ExprKind::Member {
                            obj: Box::new(iter.clone()),
                            name: "iter".to_string(),
                        },
                        span: iter.span,
                    }),
                    args: Vec::new(),
                    trailing: None,
                },
                span: iter.span,
            };
            return self.emit_for(pattern, &iter_call, body);
        }

        // Plan 39 Issue D: explicit D58 algorithm Case 3 — error.
        //
        // Возможные причины:
        //   - Тип не имеет `mut next() -> Option[T]` метода (Case 1).
        //   - Тип не имеет `iter() -> Iter[T]` метода (Case 2).
        //   - Тип не зарегистрирован вообще (cross-file resolve gap?).
        //
        // Diagnostic specifically lists searched methods for AI / human
        // debugging.
        if iter_struct.is_empty() {
            Err(format!(
                "for-in: cannot resolve iterator type for expression of C-type '{}'.\n\
                 Hint: result type may не быть Nova user-type (cross-file resolve gap? \
                 see Plan 35).",
                arr_ty))
        } else {
            let has_next = self.all_methods.contains(&(iter_struct.clone(), "next".to_string()));
            let has_iter = self.all_methods.contains(&(iter_struct.clone(), "iter".to_string()));
            Err(format!(
                "for-in: type '{}' has neither `mut next() -> Option[T]` nor `iter() -> Iter[T]` methods (D58).\n\
                 Searched method_overloads: ({}, \"next\")={}, ({}, \"iter\")={}\n\
                 Hint: add one of the methods, or use `for x in c.iter()` if iterator is created externally.",
                iter_struct,
                iter_struct, has_next,
                iter_struct, has_iter))
        }
    }

    // ---- match ----

    fn emit_match(&mut self, scrutinee: &Expr, arms: &[MatchArm]) -> Result<String, String> {
        let scr = self.emit_expr(scrutinee)?;
        let scr_tmp = self.fresh_tmp_named("scr");
        let result_tmp = self.fresh_tmp_named("match");
        let matched_tmp = self.fresh_tmp_named("matched");

        // Determine scrutinee C type from its expression
        let scr_ty = self.infer_expr_c_type(scrutinee);
        self.var_types.insert(scr_tmp.clone(), scr_ty.clone());
        self.line(&format!("{} {} = {};", scr_ty, scr_tmp, scr));
        // Propagate tuple element type info from scrutinee var to scr_tmp
        if let Some(elem_tys) = self.tuple_element_types.get(scr.as_str()).cloned() {
            self.tuple_element_types.insert(scr_tmp.clone(), elem_tys);
        }
        // Propagate Option inner type info
        if let Some(inner_ty) = self.option_inner_types.get(scr.as_str()).cloned() {
            self.option_inner_types.insert(scr_tmp.clone(), inner_ty);
        }

        // Result type: infer from arms (prefer non-trivial types), fall back to fn return type
        let mut result_ty = "nova_unit".to_string();
        // Plan 55 Ф.2: per-arm helper — scoped pattern_inner_types override.
        // Для Some(v) с scr_ty=NovaOpt_T: register v: T в var_types на время
        // inference body, потом restore. Это позволяет `Some(v) => v` правильно
        // вернуть T, а не stale/leaked default. Также: nested patterns
        // (Some(Ok(v))) и Block arm trailing получают binding.
        let infer_arm = |this: &mut Self, arm: &MatchArm| -> String {
            let bindings: Vec<(String, String)> =
                Self::collect_pattern_inner_bindings(&arm.pattern, &scr_ty, this);
            let saved: Vec<(String, Option<String>)> = bindings.iter()
                .map(|(n, _)| (n.clone(), this.var_types.get(n).cloned()))
                .collect();
            for (n, t) in &bindings {
                this.var_types.insert(n.clone(), t.clone());
            }
            let t = match &arm.body {
                MatchArmBody::Expr(e) => this.infer_expr_c_type(e),
                MatchArmBody::Block(b) => b.trailing.as_ref()
                    .map(|e| this.infer_expr_c_type(e))
                    .unwrap_or_else(|| "nova_unit".into()),
            };
            for (n, prev) in saved {
                match prev {
                    Some(p) => { this.var_types.insert(n, p); }
                    None => { this.var_types.remove(&n); }
                }
            }
            t
        };
        // First pass: find a non-unit, non-nova_int type
        'outer: for arm in arms {
            let t = infer_arm(self, arm);
            if t != "nova_unit" && t != "nova_int" {
                result_ty = t;
                break 'outer;
            }
        }
        // Second pass: settle for nova_int if no better type found
        if result_ty == "nova_unit" {
            for arm in arms {
                let t = infer_arm(self, arm);
                if t != "nova_unit" { result_ty = t; break; }
            }
        }
        // Note: we intentionally don't inherit result_ty from current_fn_return_ty here,
        // because the match may be inside a for loop or other non-return context.
        // Exception: "NovaOpt_nova_int" is the erased generic fallback (inner type unknown).
        // When the fn return type is a more specific NovaOpt_*, upgrade to avoid
        // struct-type mismatch when assigning NovaOpt_nova_unit to NovaOpt_nova_int.
        if result_ty == "NovaOpt_nova_int" {
            if let Some(fn_ret) = &self.current_fn_return_ty {
                if fn_ret.starts_with("NovaOpt_") && fn_ret != "NovaOpt_nova_int" {
                    result_ty = fn_ret.clone();
                }
            }
        }

        self.line(&format!("{} {};", result_ty, result_tmp));
        self.var_types.insert(result_tmp.clone(), result_ty.clone());
        // matched flag: tracks if any arm matched (needed for guard fallthrough)
        self.line(&format!("int {} = 0;", matched_tmp));

        for arm in arms {
            // Each arm: if (!matched && pattern_cond) { bind; if(guard) { body; matched=1; } }
            let cond = self.pattern_cond(&arm.pattern, &scr_tmp)?;
            let has_guard = arm.guard.is_some();

            self.line(&format!("if (!{matched} && ({cond})) {{",
                matched = matched_tmp, cond = cond));
            self.indent += 1;

            // Emit pattern bindings
            self.pattern_bind_typed(&arm.pattern, &scr_tmp)?;

            if let Some(g) = &arm.guard {
                let gv = self.emit_expr(g)?;
                self.line(&format!("if ({}) {{", gv));
                self.indent += 1;
                self.emit_match_arm_body(&arm.body, &result_tmp)?;
                self.line(&format!("{} = 1;", matched_tmp));
                self.indent -= 1;
                self.line("}");
            } else {
                self.emit_match_arm_body(&arm.body, &result_tmp)?;
                self.line(&format!("{} = 1;", matched_tmp));
            }

            let _ = has_guard;
            self.indent -= 1;
            self.line("}");
        }

        Ok(result_tmp)
    }

    /// Plan 55 Ф.2: extract pattern-bound variable types from scrutinee C-type.
    /// Used during match-arm inference to give `Some(v) => v` the right result
    /// type T (instead of stale/leaked var_types fallback).
    ///
    /// Supports:
    /// - top-level `Ident { name }` — binding is whole scrutinee type.
    /// - `Some(p)` / `None` — recurse with NovaOpt_T inner type.
    /// - `Ok(p)` / `Err(p)` — recurse with Result Ok/Err payload type.
    /// - `Tuple(...)` — recurse via tuple_element_types[scr_tmp]
    ///   (not implemented for inference path — usually pattern is in let, not match).
    /// - `Variant(...)` user sum-type — recurse via sum_schemas.
    /// - `Or` — bindings from the first alternative.
    /// - `Binding { name, inner }` — outer name has scrutinee type, recurse inner.
    /// - `Record { fields }` — lookup record_schemas for field types.
    fn collect_pattern_inner_bindings(
        pat: &Pattern,
        scr_ty: &str,
        this: &Self,
    ) -> Vec<(String, String)> {
        match pat {
            Pattern::Wildcard(_) | Pattern::Literal(..) => vec![],
            Pattern::Ident { name, .. } => {
                vec![(name.clone(), scr_ty.to_string())]
            }
            Pattern::Binding { name, inner, .. } => {
                let mut out = vec![(name.clone(), scr_ty.to_string())];
                out.extend(Self::collect_pattern_inner_bindings(inner, scr_ty, this));
                out
            }
            Pattern::Or { alternatives, .. } => {
                if let Some(first) = alternatives.first() {
                    Self::collect_pattern_inner_bindings(first, scr_ty, this)
                } else { vec![] }
            }
            Pattern::Variant { path, kind, .. } => {
                let variant_name = path.last().cloned().unwrap_or_default();
                let patterns = match kind {
                    VariantPatternKind::Tuple { patterns, .. } => patterns.clone(),
                    VariantPatternKind::Unit => vec![],
                };
                if patterns.is_empty() { return vec![]; }
                // ---- Option ----
                if variant_name == "Some" && patterns.len() == 1 {
                    // scr_ty = "NovaOpt_<inner_id>" — recover real inner C-type
                    // via novaopt_value_types (handles pointer-sanitization).
                    if let Some(inner_id) = scr_ty.strip_prefix("NovaOpt_") {
                        let inner_c = this.novaopt_value_types.borrow()
                            .get(inner_id).cloned()
                            .unwrap_or_else(|| inner_id.to_string());
                        return Self::collect_pattern_inner_bindings(&patterns[0], &inner_c, this);
                    }
                    return vec![];
                }
                if variant_name == "None" { return vec![]; }
                // ---- Result (Ok/Err) ----
                // Mono'd Result type: Nova_Result____<Ok_c>__<Err_c>* or sanitized.
                // For bootstrap — parse the canonical form.
                if (variant_name == "Ok" || variant_name == "Err") && patterns.len() == 1 {
                    let bare = scr_ty.trim_end_matches('*').trim();
                    if let Some(suffix) = bare.strip_prefix("Nova_Result____") {
                        // suffix = "<Ok_c>__<Err_c>"; split на 2 части по "__".
                        // (Имена primitive C-типов не содержат "__".)
                        let parts: Vec<&str> = suffix.splitn(2, "__").collect();
                        if parts.len() == 2 {
                            let inner_c = if variant_name == "Ok" { parts[0] } else { parts[1] };
                            return Self::collect_pattern_inner_bindings(&patterns[0], inner_c, this);
                        }
                    }
                    return vec![];
                }
                // ---- User sum-types ----
                // Lookup sum_schemas: variant_field_types[(sum_name, variant_name)].
                let sum_name = scr_ty.trim_end_matches('*').trim()
                    .strip_prefix("Nova_").unwrap_or("").to_string();
                if let Some(schema) = this.sum_schemas.get(&sum_name) {
                    if let Some(variant_tys) = schema.get(&variant_name) {
                        let mut out = vec![];
                        for (i, p) in patterns.iter().enumerate() {
                            if let Some(field_c) = variant_tys.get(i) {
                                out.extend(Self::collect_pattern_inner_bindings(p, field_c, this));
                            }
                        }
                        return out;
                    }
                }
                vec![]
            }
            // For Tuple / Record / Array — would need tuple_element_types /
            // record_schemas lookup which is path-dependent. Bootstrap covers
            // 95% case (Option/Result/User sum-types via Variant above).
            // Extension point — add as needed when concrete bug surfaces.
            Pattern::Tuple(..) | Pattern::Record { .. } | Pattern::Array { .. } => vec![],
        }
    }

    fn emit_match_arm_body(&mut self, body: &MatchArmBody, result_tmp: &str) -> Result<(), String> {
        let result_ty = self.var_types.get(result_tmp).cloned().unwrap_or_default();
        match body {
            MatchArmBody::Expr(e) => {
                let val_ty = self.infer_expr_c_type(e);
                let v = self.emit_expr(e)?;
                let assignment = self.coerce_for_assignment(&v, &val_ty, &result_ty);
                self.line(&format!("{} = {};", result_tmp, assignment));
            }
            MatchArmBody::Block(b) => {
                // Plan 20 Ф.4/Ф.8: match-arm body — defer scope.
                // Trailing value присваивается ПОСЛЕ defer cleanup.
                let block_id = self.enter_defer_scope(b, false);
                for stmt in &b.stmts {
                    self.emit_stmt(stmt)?;
                }
                if let Some(trailing) = &b.trailing {
                    let val_ty = self.infer_expr_c_type(trailing);
                    let v = self.emit_expr(trailing)?;
                    let assignment = self.coerce_for_assignment(&v, &val_ty, &result_ty);
                    self.line(&format!("{} = {};", result_tmp, assignment));
                }
                self.leave_defer_scope(block_id);
            }
        }
        Ok(())
    }


    /// Emit `select { ... }` expression --- D94.
    fn emit_select(&mut self, arms: &[crate::ast::SelectArm]) -> Result<String, String> {
        use crate::ast::SelectOp;

        let n_ch: usize = arms.iter().filter(|a| !matches!(a.op, SelectOp::Default)).count();
        let has_default = arms.iter().any(|a| matches!(a.op, SelectOp::Default));

        // Plan 44.1 Ф.3-extended: per-call adaptive storage без cap'а.
        // Storage = SelectSlot[n_ch] + SelectWaiter[n_ch] на стеке через
        // compound literal (literal size known at codegen time, MSVC-
        // compatible). Размер stack frame пропорционален n_ch:
        // ~80n байт. На default minicoro 56 KB stack n_ch=700+ безопасно.
        // Реальные select'ы — 2-8 arms; даже n_ch=100 = 8 KB безопасно.
        let result_tmp = self.fresh_tmp_named("sel");
        self.line(&format!("nova_unit {};", result_tmp));
        self.var_types.insert(result_tmp.clone(), "nova_unit".to_string());

        // --- Single-arm fast path ---
        if n_ch == 1 && !has_default {
            let arm = arms.iter().find(|a| !matches!(a.op, SelectOp::Default)).unwrap();
            match &arm.op {
                SelectOp::Recv { binding, chan } => {
                    let ch = self.emit_expr(chan)?;
                    let opt_tmp = self.fresh_tmp_named("sel_opt");
                    self.line(&format!("NovaOpt_nova_int {} = nova_chan_reader_recv({});", opt_tmp, ch));
                    // Wildcard `_ = rx` fires on any recv result (Some or None/closed).
                    // Bound `Some(v) = rx` fires only on Some.
                    let cond = if binding.is_some() {
                        format!("{}.tag == NOVA_TAG_Option_Some", opt_tmp)
                    } else {
                        // _ = rx: fired whenever recv completed (value or closed)
                        "1".to_string()
                    };
                    self.line(&format!("if ({}) {{", cond));
                    self.indent += 1;
                    if let Some(b) = binding {
                        self.line(&format!("nova_int {} = {}.value;", b, opt_tmp));
                        self.var_types.insert(b.clone(), "nova_int".to_string());
                    }
                    if let Some(g) = &arm.guard {
                        let gv = self.emit_expr(g)?;
                        self.line(&format!("if ({}) {{", gv));
                        self.indent += 1;
                    }
                    let block_id = self.enter_defer_scope(&arm.body, false);
                    for stmt in &arm.body.stmts { self.emit_stmt(stmt)?; }
                    if let Some(tr) = &arm.body.trailing { let _ = self.emit_expr(tr)?; }
                    self.leave_defer_scope(block_id);
                    if arm.guard.is_some() {
                        self.indent -= 1;
                        self.line("}");
                    }
                    self.indent -= 1;
                    self.line("}");
                }
                SelectOp::Send { chan, value } => {
                    let ch = self.emit_expr(chan)?;
                    let v = self.emit_expr(value)?;
                    self.line(&format!("nova_chan_writer_send({}, {});", ch, v));
                    if let Some(g) = &arm.guard {
                        let gv = self.emit_expr(g)?;
                        self.line(&format!("if ({}) {{", gv));
                        self.indent += 1;
                    }
                    let block_id = self.enter_defer_scope(&arm.body, false);
                    for stmt in &arm.body.stmts { self.emit_stmt(stmt)?; }
                    if let Some(tr) = &arm.body.trailing { let _ = self.emit_expr(tr)?; }
                    self.leave_defer_scope(block_id);
                    if arm.guard.is_some() {
                        self.indent -= 1;
                        self.line("}");
                    }
                }
                SelectOp::Default => unreachable!(),
            }
            return Ok(result_tmp);
        }

        // --- Full SelectCtx path ---
        // Plan 44.1 Ф.3-extended: per-call adaptive storage.
        // Эмитим локальные массивы ровно n_ch размера (compound literal
        // на стеке, размер literal на codegen-time, MSVC-compatible).
        // nova_select_init принимает указатели на эти массивы.
        let ctx_tmp = self.fresh_tmp_named("sel_ctx");
        let arms_tmp = self.fresh_tmp_named("sel_arms");
        let waiters_tmp = self.fresh_tmp_named("sel_waiters");
        self.line(&format!("SelectSlot {}[{}];", arms_tmp, n_ch));
        self.line(&format!("SelectWaiter {}[{}];", waiters_tmp, n_ch));
        self.line(&format!(
            "SelectCtx {} = nova_select_init({}, {}, {});",
            ctx_tmp, n_ch, arms_tmp, waiters_tmp
        ));

        // Emit channel exprs upfront
        let mut ch_map: Vec<(usize, String)> = Vec::new();
        let mut sv_map: std::collections::HashMap<usize, String> = Default::default();
        for (i, arm) in arms.iter().enumerate() {
            match &arm.op {
                SelectOp::Recv { chan, .. } => {
                    let ch = self.emit_expr(chan)?;
                    ch_map.push((i, ch));
                }
                SelectOp::Send { chan, value } => {
                    let ch = self.emit_expr(chan)?;
                    let v = self.emit_expr(value)?;
                    ch_map.push((i, ch));
                    sv_map.insert(i, v);
                }
                SelectOp::Default => {}
            }
        }

        let mut ch_idx = 0usize;
        for (i, arm) in arms.iter().enumerate() {
            let guard_val = if let Some(g) = &arm.guard {
                self.emit_expr(g)?
            } else {
                "1".to_string()
            };
            match &arm.op {
                SelectOp::Recv { binding, .. } => {
                    let ch = &ch_map.iter().find(|(idx, _)| *idx == i).unwrap().1;
                    let wildcard = if binding.is_none() { 1 } else { 0 };
                    self.line(&format!(
                        "nova_select_set_recv(&{ctx}, {n}, {ch}, {guard}, {wildcard});",
                        ctx = ctx_tmp, n = ch_idx, ch = ch, guard = guard_val, wildcard = wildcard
                    ));
                    ch_idx += 1;
                }
                SelectOp::Send { .. } => {
                    let ch = &ch_map.iter().find(|(idx, _)| *idx == i).unwrap().1;
                    let val = sv_map.get(&i).cloned().unwrap_or_else(|| "0".to_string());
                    self.line(&format!(
                        "nova_select_set_send(&{ctx}, {n}, {ch}, {val}, {guard});",
                        ctx = ctx_tmp, n = ch_idx, ch = ch, guard = guard_val
                    ));
                    ch_idx += 1;
                }
                SelectOp::Default => {}
            }
        }

        let imm_tmp = self.fresh_tmp_named("sel_imm");
        self.line(&format!("int {} = nova_select_try_immediate(&{});", imm_tmp, ctx_tmp));
        if has_default {
            self.line(&format!("if (!{}) {{ {}.which = -2; }}", imm_tmp, ctx_tmp));
        } else {
            self.line(&format!("if (!{}) {{", imm_tmp));
            self.indent += 1;
            self.line(&format!("{}.scope = _nova_active_scope;", ctx_tmp));
            self.line(&format!("{}.slot = _nova_active_slot;", ctx_tmp));
            self.line(&format!("nova_select_park(&{});", ctx_tmp));
            self.indent -= 1;
            self.line("}");
        }

        let which = format!("{}.which", ctx_tmp);
        let mut ch_idx2 = 0usize;
        let mut first = true;
        for arm in arms.iter() {
            let cond = match &arm.op {
                SelectOp::Recv { .. } => { let c = format!("({} == {})", which, ch_idx2); ch_idx2 += 1; c }
                SelectOp::Send { .. } => { let c = format!("({} == {})", which, ch_idx2); ch_idx2 += 1; c }
                SelectOp::Default => format!("({} == -2)", which),
            };
            let kw = if first { "if" } else { "} else if" };
            first = false;
            self.line(&format!("{} ({}) {{", kw, cond));
            self.indent += 1;
            if let SelectOp::Recv { binding: Some(b), .. } = &arm.op {
                self.line(&format!("nova_int {} = {}.recv_val;", b, ctx_tmp));
                self.var_types.insert(b.clone(), "nova_int".to_string());
            }
            let block_id = self.enter_defer_scope(&arm.body, false);
            for stmt in &arm.body.stmts { self.emit_stmt(stmt)?; }
            if let Some(tr) = &arm.body.trailing { let _ = self.emit_expr(tr)?; }
            self.leave_defer_scope(block_id);
            self.indent -= 1;
        }
        if !first { self.line("}"); }

        Ok(result_tmp)
    }

    /// Produce a C expression that coerces `val` from `from_ty` to `to_ty` when types differ.
    fn coerce_for_assignment(&self, val: &str, from_ty: &str, to_ty: &str) -> String {
        if from_ty == to_ty || to_ty.is_empty() || from_ty.is_empty() {
            return val.to_string();
        }
        // nova_int → nova_str: unbox pointer stored as int
        if from_ty == "nova_int" && to_ty == "nova_str" {
            return format!("(*(nova_str*)(intptr_t)({}))", val);
        }
        // void* → nova_str: deref pointer
        if from_ty == "void*" && to_ty == "nova_str" {
            return format!("(*(nova_str*)({}))", val);
        }
        // nova_str → nova_int: box to pointer
        if from_ty == "nova_str" && to_ty == "nova_int" {
            return format!("(nova_int)(intptr_t)(&({}))", val);
        }
        val.to_string()
    }

    // ---- record literal ----

    /// Plan 52 Ф.10 production-fix: D55 map-coercion для `{field: v}`.
    /// Когда annotate_map_literals (types/mod.rs::MapLitAnnotator)
    /// устанавливает `RecordLit.inferred_map_v = Some(V)` — значит это
    /// map-coercion в позиции `HashMap[str, V]`. Эмитим mirror MapLit-
    /// десугаринга: `HashMap[str, V].with_capacity(n) + n × insert`.
    /// Не строим intermediate AST — прямой C-emit.
    fn emit_record_as_map(
        &mut self,
        fields: &[RecordLitField],
        v_ty: &TypeRef,
    ) -> Result<String, String> {
        let v_c = self.type_ref_to_c(v_ty)?;
        // Mangled HashMap[str, V] type name + static_with_capacity ctor.
        // Mirror как для MapLit с turbofish — codegen monomorph даёт
        // Nova_HashMap____nova_str__<V_c>.
        let v_mangled = v_c
            .replace('*', "")
            .replace(' ', "_");
        let map_ty = format!("Nova_HashMap____nova_str__{}", v_mangled);
        let with_cap_fn = format!("{}_static_with_capacity", map_ty);
        let insert_fn = format!("{}_method_insert", map_ty);

        let n = fields.len();
        let tmp = self.fresh_tmp();
        self.line(&format!(
            "{}* {} = {}((nova_int){}LL);",
            map_ty, tmp, with_cap_fn, n
        ));
        self.var_types.insert(tmp.clone(), format!("{}*", map_ty));

        for f in fields {
            if f.is_spread {
                return Err(
                    "spread `...` in map-coercion record-literal is not supported \
                     (Plan 52 Ф.3 spec; type-checker should have caught this)".into(),
                );
            }
            // Field-punning: { name } → значение это переменная `name` в scope.
            let value_expr_str = match &f.value {
                Some(v_expr) => self.emit_expr(v_expr)?,
                None => f.name.clone(),
            };
            // Ключ — str literal из имени поля.
            let key_str = format!(
                "(nova_str){{.ptr=\"{}\", .len={}}}",
                Self::escape_c_str(&f.name),
                f.name.len()
            );
            // `insert` возвращает Option[V] (старое значение) — discard.
            self.line(&format!(
                "(void){}({}, {}, {});",
                insert_fn, tmp, key_str, value_expr_str
            ));
        }
        Ok(tmp)
    }

    fn emit_record_lit(
        &mut self,
        type_name: Option<&[String]>,
        fields: &[RecordLitField],
    ) -> Result<String, String> {
        let tmp = self.fresh_tmp();

        // Find the first spread field to determine the struct type if type_name is absent
        let spread_src: Option<String> = if type_name.is_none() {
            let mut s = None;
            for f in fields {
                if f.is_spread {
                    if let Some(v) = &f.value {
                        s = Some(self.emit_expr(v)?);
                        break;
                    }
                }
            }
            s
        } else {
            None
        };

        if let Some(name) = type_name {
            let raw_name = name.join("_");
            // Resolve `Self` to current receiver type
            let struct_name = if raw_name == "Self" {
                self.current_receiver_type.clone().unwrap_or(raw_name)
            } else {
                raw_name
            };
            // If struct_name is a base generic type (e.g. "Box"), resolve to the concrete
            // monomorphized name (e.g. "Box____nova_int") using field value types.
            // This enables `Box { value: 42 }` to emit as Nova_Box____nova_int.
            let struct_name = if self.generic_types.contains(&struct_name) {
                if let Some(template) = self.generic_type_templates.get(&struct_name).cloned() {
                    use crate::ast::TypeDeclKind;
                    let mut type_args_c: Vec<String> = template.generics.iter()
                        .map(|_| "nova_int".to_string())
                        .collect();
                    if let TypeDeclKind::Record(field_decls) = &template.kind {
                        for (i, g) in template.generics.iter().enumerate() {
                            for f_decl in field_decls {
                                if let crate::ast::TypeRef::Named { path, generics: fgens, .. } = &f_decl.ty {
                                    if fgens.is_empty() && path.join("_") == g.name {
                                        if let Some(field) = fields.iter().find(|f| f.name == f_decl.name) {
                                            if let Some(v) = &field.value {
                                                let c_ty = self.infer_expr_c_type(v);
                                                if !c_ty.is_empty() && c_ty != "void*" {
                                                    type_args_c[i] = c_ty;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let mangled = Self::compute_generic_type_c_name(&struct_name, &type_args_c);
                    let concrete = mangled.strip_prefix("Nova_").unwrap_or(&mangled).to_string();
                    // Queue instance if not yet in worklist / emitted
                    if !self.emitted_generic_type_instances.contains(&mangled) {
                        let mut wl = self.generic_type_worklist.borrow_mut();
                        if !wl.iter().any(|(_, _, m)| m == &mangled) {
                            wl.push((struct_name.clone(), type_args_c, mangled));
                        }
                    }
                    // Drain so record_schemas is populated before we proceed
                    drop(template);
                    self.drain_generic_type_worklist()?;
                    concrete
                } else {
                    struct_name
                }
            } else {
                struct_name
            };
            // Check if this is a sum-type record variant (not a plain record)
            if let Some((sum_type_name, _)) = self.find_variant(&struct_name) {
                // Emit as sum-type record variant constructor: nova_make_T_Variant(field_vals...)
                // D109: In monomorphized context, sum_type_name may be the erased base type
                // (e.g. "Slot"). Compute concrete monomorphized name if generic params available.
                let (concrete_sum_c, ctor_prefix) = if self.generic_types.contains(&sum_type_name) {
                    if let Some(tmpl) = self.generic_type_templates.get(&sum_type_name).cloned() {
                        let type_args_c: Vec<String> = tmpl.generics.iter()
                            .filter_map(|g| self.current_type_subst.get(&g.name).cloned())
                            .collect();
                        if type_args_c.len() == tmpl.generics.len() {
                            let mangled = Self::compute_generic_type_c_name(&sum_type_name, &type_args_c);
                            // mangled = "Nova_Slot____nova_str__nova_int" (includes Nova_ prefix)
                            (mangled.clone(), mangled)
                        } else {
                            (format!("Nova_{}", sum_type_name), sum_type_name.clone())
                        }
                    } else {
                        (format!("Nova_{}", sum_type_name), sum_type_name.clone())
                    }
                } else {
                    (format!("Nova_{}", sum_type_name), sum_type_name.clone())
                };
                // Collect field values in schema order
                let order_key = format!("{}::{}", sum_type_name, struct_name);
                let field_order: Vec<String> = self.record_variant_field_order
                    .get(&order_key).cloned().unwrap_or_default();
                // Build arg list in field order from schema, or from provided fields
                let mut field_vals: Vec<(String, String)> = Vec::new();
                for f in fields {
                    if !f.is_spread {
                        let val = if let Some(v) = &f.value {
                            self.emit_expr(v)?
                        } else {
                            f.name.clone()
                        };
                        field_vals.push((f.name.clone(), val));
                    }
                }
                // Order args by schema field order
                let ordered_args: Vec<String> = if field_order.is_empty() {
                    field_vals.iter().map(|(_, v)| v.clone()).collect()
                } else {
                    field_order.iter().filter_map(|fname| {
                        field_vals.iter().find(|(n, _)| n == fname).map(|(_, v)| v.clone())
                    }).collect()
                };
                let call = format!("nova_make_{}_{}({})", ctor_prefix, struct_name, ordered_args.join(", "));
                self.line(&format!("{}* {} = {};", concrete_sum_c, tmp, call));
                self.var_types.insert(tmp.clone(), format!("{}*", concrete_sum_c));
            } else if !self.record_schemas.contains_key(&struct_name) {
                // Unknown struct (e.g. generic type not monomorphized) — emit null stub
                // Evaluate field expressions for side effects but discard
                for f in fields {
                    if let Some(v) = &f.value { let _ = self.emit_expr(v)?; }
                }
                self.line(&format!("void* {} = NULL; /* unknown type {} */", tmp, struct_name));
                self.var_types.insert(tmp.clone(), "void*".into());
            } else {
                // Plain record struct
                self.line(&format!("Nova_{}* {} = (Nova_{}*)nova_alloc(sizeof(Nova_{}));",
                    struct_name, tmp, struct_name, struct_name));
                for f in fields {
                    if f.is_spread {
                        if let Some(src_expr) = &f.value {
                            let src = self.emit_expr(src_expr)?;
                            self.line(&format!("*{} = *{};", tmp, src));
                        }
                    } else {
                        // Compute field_ty before emitting value so array literals get the hint.
                        let field_ty = self.record_schemas.get(&struct_name)
                            .and_then(|s| s.get(&f.name)).cloned().unwrap_or_default();
                        let val = if let Some(v) = &f.value {
                            self.emit_expr_with_target_type(v, &field_ty)?
                        } else {
                            f.name.clone() // field punning
                        };
                        // Check if the field is void* in schema (generic type erasure) — need to box the value
                        if field_ty == "void*" {
                            let val_ty = if let Some(v) = &f.value { self.infer_expr_c_type(v) } else { "nova_int".into() };
                            let boxed = self.box_value_as_void_ptr(&val, &val_ty);
                            let mfn = Self::mangle_field_name(&f.name);
                            self.line(&format!("{}->{} = {};", tmp, mfn, boxed));
                        } else {
                            let mfn = Self::mangle_field_name(&f.name);
                            self.line(&format!("{}->{} = {};", tmp, mfn, val));
                        }
                    }
                }
                self.var_types.insert(tmp.clone(), format!("Nova_{}*", struct_name));
            }
        } else if type_name.is_none() && spread_src.is_none()
            && self.expected_record_type.is_some()
        {
            // D55 inferred-type-context: anonymous record `{ a, b }` в позиции
            // с известным struct-target (e.g. `=> { end: ..., cur: ... }` в fn
            // с `-> RangeIter`). expected_record_type выставлен emit_fn_body /
            // emit_method_body перед emit_expr(body).
            let raw = self.expected_record_type.clone().unwrap();
            let struct_name = if raw == "Self" {
                self.current_receiver_type.clone().unwrap_or(raw)
            } else { raw };
            // Plan 48 Ф.3: drain pending generic type instances so record_schemas is populated.
            if !self.generic_type_worklist.borrow().is_empty() {
                self.drain_generic_type_worklist()?;
            }
            if !self.record_schemas.contains_key(&struct_name) {
                return Err(format!(
                    "anonymous record literal: expected struct '{}' not in record_schemas",
                    struct_name));
            }
            self.line(&format!("Nova_{0}* {1} = (Nova_{0}*)nova_alloc(sizeof(Nova_{0}));",
                struct_name, tmp));
            for f in fields {
                if f.is_spread { continue; }
                // Compute field_ty first so array literals get the correct element type hint.
                let field_ty = self.record_schemas.get(&struct_name)
                    .and_then(|s| s.get(&f.name)).cloned().unwrap_or_default();
                let val = if let Some(v) = &f.value {
                    self.emit_expr_with_target_type(v, &field_ty)?
                } else {
                    f.name.clone()
                };
                if field_ty == "void*" {
                    let val_ty = if let Some(v) = &f.value {
                        self.infer_expr_c_type(v)
                    } else { "nova_int".into() };
                    let boxed = self.box_value_as_void_ptr(&val, &val_ty);
                    let mfn = Self::mangle_field_name(&f.name);
                    self.line(&format!("{}->{} = {};", tmp, mfn, boxed));
                } else {
                    let mfn = Self::mangle_field_name(&f.name);
                    self.line(&format!("{}->{} = {};", tmp, mfn, val));
                }
            }
            self.var_types.insert(tmp.clone(), format!("Nova_{}*", struct_name));
        } else if let Some(src) = spread_src {
            // Anonymous record with spread: `{ ...p, y: 10.0 }`
            // Determine the struct type from var_types table.
            // src is the C expression for the spread source (e.g. "p").
            let struct_ty = self.var_types.get(&src).cloned()
                .unwrap_or_else(|| "void".into());
            // struct_ty is like "Nova_Point*" — strip the "*" to get struct name
            let struct_name = struct_ty.trim_end_matches('*').trim().to_string();
            if struct_name == "void" || struct_name.is_empty() {
                return Err(format!(
                    "cannot determine type for spread `{{...{}}}` — add explicit type annotation",
                    src
                ));
            }
            self.line(&format!("{struct_name}* {tmp} = ({struct_name}*)nova_alloc(sizeof({struct_name}));",
                struct_name = struct_name, tmp = tmp));
            self.line(&format!("*{tmp} = *{src};", tmp = tmp, src = src));
            for f in fields {
                if !f.is_spread {
                    let val = if let Some(v) = &f.value {
                        self.emit_expr(v)?
                    } else {
                        f.name.clone()
                    };
                    self.line(&format!("{tmp}->{name} = {val};",
                        tmp = tmp, name = f.name, val = val));
                }
            }
        } else {
            // Anonymous record without spread: infer struct type from function return context.
            // This handles `fn Foo[T].of(v T) -> Foo[T] => { field: v }` in monomorphized
            // context where current_fn_return_ty = "Nova_Foo____nova_int*".
            let inferred_struct_c_name = self.current_fn_return_ty.as_deref()
                .filter(|t| t.starts_with("Nova_") && t.ends_with('*'))
                .map(|t| t.trim_end_matches('*').trim().to_string());
            if let Some(struct_c_name) = inferred_struct_c_name {
                let struct_name = struct_c_name.strip_prefix("Nova_").unwrap_or(&struct_c_name).to_string();
                self.line(&format!("{cname}* {tmp} = ({cname}*)nova_alloc(sizeof({cname}));",
                    cname = struct_c_name, tmp = tmp));
                for f in fields {
                    if f.is_spread { continue; }
                    let val = if let Some(v) = &f.value {
                        self.emit_expr(v)?
                    } else {
                        f.name.clone()
                    };
                    let field_ty = self.record_schemas.get(&struct_name)
                        .and_then(|s| s.get(&f.name)).cloned().unwrap_or_default();
                    let mfn = Self::mangle_field_name(&f.name);
                    if field_ty == "void*" {
                        let val_ty = if let Some(v) = &f.value {
                            self.infer_expr_c_type(v)
                        } else { "nova_int".into() };
                        let boxed = self.box_value_as_void_ptr(&val, &val_ty);
                        self.line(&format!("{}->{} = {};", tmp, mfn, boxed));
                    } else {
                        self.line(&format!("{}->{} = {};", tmp, mfn, val));
                    }
                }
                self.var_types.insert(tmp.clone(), format!("{}*", struct_c_name));
            } else {
                return Err("anonymous record literal without spread not supported in codegen".into());
            }
        }
        Ok(tmp)
    }

    // ---- tuple destructure ----

    fn emit_tuple_destructure(&mut self, pats: &[Pattern], value: &Expr) -> Result<(), String> {
        // D91 (Plan 21): special-case for `let (tx, rx) = Channel.new(cap)`.
        // Channel.new returns Nova_ChannelPair {tx: Nova_ChanWriter*, rx: Nova_ChanReader*},
        // not a _NovaTuple2. Must be handled before the general case.
        let is_channel_new = match &value.kind {
            ExprKind::Call { func, .. } => {
                let f = func.unwrap_turbofish();
                match &f.kind {
                    ExprKind::Member { obj, name } => {
                        name == "new" && matches!(&obj.kind, ExprKind::Ident(n) if n == "Channel")
                    }
                    ExprKind::Path(parts) => {
                        parts.len() == 2 && parts[0] == "Channel" && parts[1] == "new"
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if is_channel_new && pats.len() == 2 {
            let tmp = self.fresh_tmp();
            let cap_expr = if let ExprKind::Call { args, .. } = &value.kind {
                if let Some(a) = args.first() { self.emit_expr(a.expr())? } else { "0".into() }
            } else { "0".into() };
            self.line(&format!("Nova_ChannelPair {} = nova_channel_new({});", tmp, cap_expr));
            let elem_types = ["Nova_ChanWriter*", "Nova_ChanReader*"];
            let fields     = ["tx", "rx"];
            for (i, pat) in pats.iter().enumerate() {
                match pat {
                    Pattern::Wildcard(_) => {}
                    Pattern::Ident { name, .. } => {
                        self.var_types.insert(name.clone(), elem_types[i].to_string());
                        self.line(&format!("{} {} = {}.{};", elem_types[i], name, tmp, fields[i]));
                    }
                    _ => return Err(format!(
                        "nested pattern in Channel.new destructure not supported: {:?}", pat
                    )),
                }
            }
            return Ok(());
        }

        match &value.kind {
            ExprKind::TupleLit(elems) => {
                // Direct pairing: let (a, b) = (x, y) — emit each binding separately
                for (pat, elem) in pats.iter().zip(elems.iter()) {
                    match pat {
                        Pattern::Wildcard(_) => {
                            // Side-effects: emit expr, discard
                            let v = self.emit_expr(elem)?;
                            self.line(&format!("(void)({});", v));
                        }
                        Pattern::Ident { name, .. } => {
                            let ty_c = self.infer_expr_c_type(elem);
                            let val = self.emit_expr(elem)?;
                            self.var_types.insert(name.clone(), ty_c.clone());
                            self.line(&format!("{} {} = {};", ty_c, name, val));
                        }
                        _ => {
                            return Err(format!(
                                "nested pattern in tuple destructure not yet supported: {:?}", pat
                            ));
                        }
                    }
                }
                Ok(())
            }
            _ => {
                // General case: emit RHS to a tmp struct, then bind each field
                // Infer element types from patterns via var_types (best-effort nova_int)
                let n = pats.len();
                let struct_name = format!("_NovaTuple{}", n);
                let tmp = self.fresh_tmp();
                // We don't know the element types without type inference, so use nova_int
                // as a best-effort (sufficient for numeric tuples).
                let elem_ty = "nova_int";
                // Emit the struct type definition
                let fields_decl: String = (0..n)
                    .map(|i| format!("{} f{};", elem_ty, i))
                    .collect::<Vec<_>>().join(" ");
                // Use pre-declared _NovaTupleN typedef (no struct re-declaration needed)
                let rhs = self.emit_expr(value)?;
                let _ = fields_decl;
                self.line(&format!("{} {} = {};", struct_name, tmp, rhs));
                for (i, pat) in pats.iter().enumerate() {
                    match pat {
                        Pattern::Wildcard(_) => {}
                        Pattern::Ident { name, .. } => {
                            self.var_types.insert(name.clone(), elem_ty.to_string());
                            self.line(&format!("{} {} = {}.f{};", elem_ty, name, tmp, i));
                        }
                        _ => {
                            return Err(format!(
                                "nested pattern in tuple destructure not yet supported: {:?}", pat
                            ));
                        }
                    }
                }
                Ok(())
            }
        }
    }

    // ---- array literal ----

    fn emit_array_lit(&mut self, elems: &[ArrayElem]) -> Result<String, String> {
        // Infer element type from first item (best-effort default: nova_int).
        let first_item_ty = elems.iter().find_map(|e| {
            if let ArrayElem::Item(expr) = e { Some(self.infer_expr_c_type(expr)) } else { None }
        }).unwrap_or_else(|| "nova_int".into());
        // Plan 55 Ф.1: closure-literal elements → void_p storage so they survive
        // as raw NovaClos_X* pointers (not int-cast'd) and can be dispatched
        // through NovaClosBase at call site.
        let first_is_closure = elems.iter().any(|e| {
            if let ArrayElem::Item(expr) = e {
                matches!(expr.kind, ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_))
            } else { false }
        });
        // Primitive value types use their own native array storage.
        // All other types (records, sum types, user types) use nova_int (pointer boxing).
        // When elements don't determine type (e.g. empty []), use hint from context.
        let elem_ty: &str = if first_is_closure || first_item_ty == "void*"
            || self.current_array_elem_hint.as_deref() == Some("void_p") {
            "void_p"
        } else { match first_item_ty.as_str() {
            "nova_str"  => "nova_str",
            "nova_bool" => "nova_bool",
            "nova_f64"  => "nova_f64",
            "nova_byte" => "nova_byte",
            _ => match self.current_array_elem_hint.as_deref().unwrap_or("nova_int") {
                "nova_str"  => "nova_str",
                "nova_bool" => "nova_bool",
                "nova_f64"  => "nova_f64",
                "nova_byte" => "nova_byte",
                "void_p"    => "void_p",
                _           => "nova_int",
            },
        }};
        let arr_ty = format!("NovaArray_{}", elem_ty);
        let tmp = self.fresh_tmp();
        // Count non-spread elements to set initial capacity
        let direct_count = elems.iter().filter(|e| matches!(e, ArrayElem::Item(_))).count();
        let init_cap = if direct_count > 0 { direct_count } else { 8 };
        self.line(&format!("{}* {} = nova_array_new_{}({});",
            arr_ty, tmp, elem_ty, init_cap));
        for elem in elems {
            match elem {
                ArrayElem::Item(expr) => {
                    let ety = self.infer_expr_c_type(expr);
                    let v = self.emit_expr(expr)?;
                    // Heap-alloc only when storing as nova_int pointer (boxed).
                    // When elem_ty is a primitive (str/bool/f64/byte), push directly.
                    let needs_heap_alloc = elem_ty == "nova_int"
                        && (ety.starts_with("_NovaTuple") || ety == "nova_str"
                            || ety.starts_with("NovaOpt_")) && !ety.ends_with('*');
                    if needs_heap_alloc {
                        let heap_tmp = self.fresh_tmp();
                        self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", ety, heap_tmp, ety, ety));
                        self.line(&format!("*{} = {};", heap_tmp, v));
                        self.line(&format!("nova_array_push_{}({}, ({})({})  );",
                            elem_ty, tmp, elem_ty, heap_tmp));
                    } else {
                        self.line(&format!("nova_array_push_{}({}, ({})({}));",
                            elem_ty, tmp, elem_ty, v));
                    }
                }
                ArrayElem::Spread(expr) => {
                    // Spread another array: for each element, push
                    let src = self.emit_expr(expr)?;
                    let i = self.fresh_tmp();
                    self.line(&format!("{{ nova_int {} = 0; while ({} < {}->len) {{ nova_array_push_{}({}, {}->data[{}]); {}++; }} }}",
                        i, i, src, elem_ty, tmp, src, i, i));
                }
            }
        }
        // Track the true element type if it's not nova_int (e.g. Nova_Box*)
        if first_item_ty != "nova_int" && first_item_ty != elem_ty {
            self.array_element_types.insert(tmp.clone(), first_item_ty);
        }
        Ok(tmp)
    }

    /// Plan 53: `let { tx, rx } = expr` / `let Pair { tx, rx } = expr` —
    /// record-destructuring в let-statement. Eval `value` ровно один раз
    /// в fresh tmp, потом `pattern_bind_typed` биндит поля (он уже умеет
    /// plain-record case через `record_schemas`).
    ///
    /// Refutable patterns (sum-variant в type_path / Variant / Literal /
    /// Or / Array) отсеиваются type-checker'ом — здесь ассамим plain
    /// record.
    fn emit_record_destructure(&mut self, decl: &LetDecl) -> Result<(), String> {
        // Plan 53 Ф.6.4: Channel.new больше не нуждается в special-case —
        // `Nova_ChannelPair` schema зарегистрирована в module-init
        // (см. emit_c.rs ~711, after Error). `is_value_type("Nova_ChannelPair")`
        // → true (accessor `.`), `infer_expr_c_type(Channel.new(_))` возвращает
        // `"Nova_ChannelPair"`. Общий путь ниже делегирует биндинг полей в
        // `pattern_bind_typed` — Plain-record case с tx: Nova_ChanWriter*,
        // rx: Nova_ChanReader*.
        //
        // Общий путь: eval RHS в tmp, делегируем биндинг полей в
        // pattern_bind_typed (plain-record-aware).
        let tmp = self.fresh_tmp();
        let ty_c = if let Some(ty) = &decl.ty {
            self.type_ref_to_c(ty)?
        } else {
            self.infer_expr_c_type(&decl.value)
        };
        // Plan 51 mirror: `let { ... } T = T { ... }` — typed-let
        // прокидывает expected_record_type для typeless литералов.
        let direct_typeless_record = matches!(
            &decl.value.kind,
            ExprKind::RecordLit { type_name: None, .. });
        let saved_expected = self.expected_record_type.clone();
        if decl.ty.is_some() && direct_typeless_record {
            self.expected_record_type = Self::struct_name_from_c_type(&ty_c);
        }
        let val = self.emit_expr_with_target_type(&decl.value, &ty_c)?;
        self.expected_record_type = saved_expected;
        // Объявляем tmp с типом — `pattern_bind_typed` смотрит var_types
        // чтобы понять, через `.` или `->` обращаться к полям.
        self.var_types.insert(tmp.clone(), ty_c.clone());
        self.line(&format!("{} {} = {};", ty_c, tmp, val));
        // Делегируем биндинг полей в общий helper match-arm'ов.
        self.pattern_bind_typed(&decl.pattern, &tmp)?;
        // Plan 53 Ф.6.3: propagate decl.mutable to all bound names from the
        // pattern. Без этого `let mut { x, y } = p; spawn { use(x); }` ловит
        // wrong capture-mode (by-value вместо by-ref) — `var_mutable` определяет
        // spawn-capture decision (emit_c.rs:2526 — by_value = is_scalar && !is_mut).
        let mut bound_names = Vec::new();
        Self::collect_pattern_bind_names(&decl.pattern, &mut bound_names);
        for name in bound_names {
            if decl.mutable {
                self.var_mutable.insert(name);
            } else {
                self.var_mutable.remove(&name);
            }
        }
        Ok(())
    }

    /// Plan 53 Ф.6.3: рекурсивно собирает все binding-имена из pattern.
    /// Используется для propagation `decl.mutable` в record-destructure
    /// (mirror того, что делает plain-let path: `var_mutable.insert/remove`).
    fn collect_pattern_bind_names(pat: &Pattern, out: &mut Vec<String>) {
        match pat {
            Pattern::Ident { name, .. } => out.push(name.clone()),
            Pattern::Wildcard(_) => {}
            Pattern::Tuple(pats, _) => {
                for p in pats {
                    Self::collect_pattern_bind_names(p, out);
                }
            }
            Pattern::Record { fields, .. } => {
                for f in fields {
                    match &f.pattern {
                        None => out.push(f.name.clone()), // shorthand `{ x }` → bind x
                        Some(sub) => Self::collect_pattern_bind_names(sub, out),
                    }
                }
            }
            Pattern::Binding { name, inner, .. } => {
                out.push(name.clone());
                Self::collect_pattern_bind_names(inner, out);
            }
            // Refutable patterns не должны доходить сюда (type-check ловит),
            // но on best-effort собираем bind-имена из вложенных полей.
            Pattern::Variant { kind, .. } => {
                use crate::ast::VariantPatternKind;
                if let VariantPatternKind::Tuple { patterns, .. } = kind {
                    for p in patterns {
                        Self::collect_pattern_bind_names(p, out);
                    }
                }
            }
            Pattern::Array { elems, .. } => {
                use crate::ast::ArrayPatternElem;
                for el in elems {
                    match el {
                        ArrayPatternElem::Item(p) => Self::collect_pattern_bind_names(p, out),
                        ArrayPatternElem::RestBind(name) => out.push(name.clone()),
                        ArrayPatternElem::Rest => {}
                    }
                }
            }
            Pattern::Literal(_, _) | Pattern::Or { .. } => {}
        }
    }

    // ---- pattern helpers ----

    fn pattern_binding(&mut self, pat: &Pattern) -> Result<String, String> {
        match pat {
            Pattern::Ident { name, .. } => Ok(name.clone()),
            Pattern::Wildcard(_) => Ok(self.fresh_tmp()),  // unique name to avoid redeclaration
            // Plan 06 Ф.2: tuple pattern `(a, b, ...)` — возвращаем fresh_tmp,
            // дальше caller (emit_for) делает destructure через
            // pattern_destructure_tuple_into_locals.
            Pattern::Tuple(_, _) => Ok(self.fresh_tmp()),
            _ => Err(format!("complex pattern in let binding not yet supported: {:?}", pat)),
        }
    }

    /// Plan 06 Ф.2: для tuple-pattern `(a, b)` в for-in эмитит локальные
    /// биндинги `T0 a = tmp.f0; T1 b = tmp.f1;`. Caller обеспечивает
    /// что `tmp` это `_NovaTuple{N}*` или `_NovaTuple{N}` value.
    fn pattern_destructure_tuple(
        &mut self,
        pat: &Pattern,
        scr_tmp: &str,
        scr_is_pointer: bool,
    ) -> Result<(), String> {
        if let Pattern::Tuple(parts, _) = pat {
            let accessor = if scr_is_pointer { "->" } else { "." };
            for (i, p) in parts.iter().enumerate() {
                let field = format!("{}{}f{}", scr_tmp, accessor, i);
                match p {
                    Pattern::Ident { name, .. } => {
                        // Default тип nova_int — bootstrap'ная convention
                        // (tuple-payload хранится как nova_int slots).
                        self.line(&format!("nova_int {} = {};", name, field));
                        self.var_types.insert(name.clone(), "nova_int".to_string());
                    }
                    Pattern::Wildcard(_) => {
                        // Skip — `_` биндинг не используется.
                        self.line(&format!("(void)({});", field));
                    }
                    other => {
                        return Err(format!(
                            "for-in tuple pattern: nested {:?} не поддерживается",
                            other));
                    }
                }
            }
            Ok(())
        } else {
            Err(format!("expected tuple pattern, got {:?}", pat))
        }
    }

    /// Returns a C boolean expression that is true when `scr` matches `pat`.
    fn pattern_cond(&mut self, pat: &Pattern, scr: &str) -> Result<String, String> {
        match pat {
            Pattern::Wildcard(_) => Ok("true".into()),
            Pattern::Ident { .. } => Ok("true".into()), // always matches, binds
            Pattern::Literal(lit, _) => {
                match lit {
                    Literal::Int(n) => Ok(format!("({} == {}LL)", scr, n)),
                    Literal::Char(cp) => Ok(format!("({} == {}LL)", scr, cp)),
                    Literal::Bool(b) => Ok(format!("({} == {})", scr, b)),
                    Literal::Str(s) => {
                        let escaped = Self::escape_c_str(s);
                        Ok(format!(
                            "({}.len == {} && memcmp({}.ptr, \"{}\", {}) == 0)",
                            scr, s.len(), scr, escaped, s.len()
                        ))
                    }
                    Literal::Float(f) => Ok(format!("({} == {})", scr, f)),
                    Literal::Unit => Ok("true".into()),
                }
            }
            Pattern::Variant { path, kind, .. } => {
                let variant_name = path.last().cloned().unwrap_or_default();
                // Determine the sum type name: explicit path or look up in schemas
                let scr_ty = self.var_types.get(scr).cloned().unwrap_or_default();
                let type_name = if path.len() > 1 {
                    path[..path.len()-1].join("_")
                } else {
                    // Look up which sum type has this variant
                    self.find_variant(&variant_name)
                        .map(|(t, _)| t)
                        .unwrap_or_else(|| {
                            // Fall back: infer from scrutinee's C type
                            scr_ty
                                .strip_prefix("Nova_")
                                .unwrap_or(&scr_ty)
                                .trim_end_matches('*')
                                .trim()
                                .to_string()
                        })
                };

                // NovaOpt_T is a value struct (not pointer), uses `.tag` and `.value`
                // But if scr itself is already a pointer-cast form (nested Option), use `->`
                let is_opt_ptr = scr.starts_with("((NovaOpt_");
                let is_opt = !is_opt_ptr && (type_name.starts_with("NovaOpt_") || scr_ty.starts_with("NovaOpt_"));
                let tag = if is_opt || is_opt_ptr {
                    format!("NOVA_TAG_Option_{}", variant_name)
                } else {
                    format!("NOVA_TAG_{}_{}", type_name, variant_name)
                };
                let accessor = if is_opt { "." } else { "->" };
                let base = format!("({}{acc}tag == {})", scr, tag, acc = accessor);
                match kind {
                    VariantPatternKind::Unit => Ok(base),
                    VariantPatternKind::Tuple { patterns, .. } => {
                        let mut cond = base;
                        // Plan 14 Ф.1: payload-types для recursive
                        // pattern_cond. Если scr_ty = NovaOpt_<X>, то
                        // scr.value: X. Регистрируем temp в var_types
                        // (по точной строке field expr'а) перед recursion'ом
                        // чтобы recursive pattern_cond смог увидеть тип.
                        let mut tmp_registrations: Vec<String> = Vec::new();
                        for (i, p) in patterns.iter().enumerate() {
                            let field = if is_opt {
                                let raw = format!("{}.value", scr);
                                let sub_is_opt_variant = matches!(p, Pattern::Variant { path, .. } if path.last().map_or(false, |n| n == "Some" || n == "None"));
                                if sub_is_opt_variant {
                                    let sub_t = scr_ty.strip_prefix("NovaOpt_");
                                    if let Some(sub_ty) = sub_t.filter(|t| t.starts_with("NovaOpt_")) {
                                        // Typed direct value — register для
                                        // recursive lookup и оставить как есть.
                                        self.var_types.insert(raw.clone(), sub_ty.to_string());
                                        tmp_registrations.push(raw.clone());
                                        raw
                                    } else {
                                        format!("((NovaOpt_nova_int*)({}))", raw)
                                    }
                                } else {
                                    raw
                                }
                            } else if is_opt_ptr {
                                let raw = format!("{}->value", scr);
                                let sub_is_opt_variant = matches!(p, Pattern::Variant { path, .. } if path.last().map_or(false, |n| n == "Some" || n == "None"));
                                if sub_is_opt_variant {
                                    format!("((NovaOpt_nova_int*)({}))", raw)
                                } else {
                                    raw
                                }
                            } else {
                                let raw = format!("{}->payload.{}._{}",
                                    scr, variant_name, i);
                                // If sub-pattern is an Option variant, the payload is a boxed pointer
                                let sub_is_opt_variant = matches!(p, Pattern::Variant { path, .. } if path.last().map_or(false, |n| n == "Some" || n == "None"));
                                if sub_is_opt_variant {
                                    format!("((NovaOpt_nova_int*)({}))", raw)
                                } else {
                                    raw
                                }
                            };
                            let sub = self.pattern_cond(p, &field)?;
                            if sub != "true" {
                                cond = format!("({} && {})", cond, sub);
                            }
                        }
                        // Plan 14 Ф.1: cleanup временных регистраций.
                        for k in &tmp_registrations {
                            self.var_types.remove(k);
                        }
                        Ok(cond)
                    }
                }
            }
            Pattern::Array { elems, .. } => {
                // Array pattern: [] → len == 0; [x, ..] → len >= 1; [x, y] → len == 2; etc.
                let n_items = elems.iter().filter(|e| matches!(e, ArrayPatternElem::Item(_))).count();
                let has_rest = elems.iter().any(|e| matches!(e, ArrayPatternElem::Rest | ArrayPatternElem::RestBind(_)));
                if has_rest {
                    Ok(format!("({}->len >= {})", scr, n_items))
                } else {
                    Ok(format!("({}->len == {})", scr, n_items))
                }
            }
            Pattern::Tuple(pats, _) => {
                let mut conds: Vec<String> = Vec::new();
                for (i, p) in pats.iter().enumerate() {
                    let field = format!("{}.f{}", scr, i);
                    let sub = self.pattern_cond(p, &field)?;
                    if sub != "true" {
                        conds.push(sub);
                    }
                }
                if conds.is_empty() {
                    Ok("true".into())
                } else {
                    Ok(format!("({})", conds.join(" && ")))
                }
            }
            Pattern::Record { type_path, fields, .. } => {
                let scr_ty = self.var_types.get(scr).cloned().unwrap_or_default();
                let type_name_from_path = type_path.as_ref().and_then(|p| p.last().cloned()).unwrap_or_default();
                // Determine if this is a plain record or a sum-type record variant
                let is_plain_record = self.record_schemas.contains_key(&type_name_from_path);
                let is_sum_variant = !is_plain_record && self.find_variant(&type_name_from_path).is_some();
                if is_plain_record {
                    // Plain record match: always succeeds; check literal fields
                    let mut conds = Vec::new();
                    for field in fields {
                        if let Some(Pattern::Literal(lit, _)) = &field.pattern {
                            let accessor = if Self::is_value_type(&scr_ty) { "." } else { "->" };
                            let mfn = Self::mangle_field_name(&field.name);
                            let field_access = format!("{}{}{}", scr, accessor, mfn);
                            let sub = self.pattern_cond(&Pattern::Literal(lit.clone(), Span::dummy()), &field_access)?;
                            if sub != "true" { conds.push(sub); }
                        }
                    }
                    if conds.is_empty() { Ok("true".into()) }
                    else { Ok(format!("({})", conds.join(" && "))) }
                } else if is_sum_variant {
                    // Sum-type record variant: check tag + literal fields
                    let (sum_type_name, _) = self.find_variant(&type_name_from_path).unwrap();
                    let variant_name = type_name_from_path.clone();
                    let mut conds = vec![format!("({}->tag == NOVA_TAG_{}_{})", scr, sum_type_name, variant_name)];
                    for field in fields {
                        if let Some(Pattern::Literal(lit, _)) = &field.pattern {
                            let mfn = Self::mangle_field_name(&field.name);
                            let field_access = format!("{}->payload.{}.{}", scr, variant_name, mfn);
                            let sub = self.pattern_cond(&Pattern::Literal(lit.clone(), Span::dummy()), &field_access)?;
                            if sub != "true" { conds.push(sub); }
                        }
                    }
                    Ok(format!("({})", conds.join(" && ")))
                } else {
                    Ok("true".into())
                }
            }
            Pattern::Or { alternatives, .. } => {
                // Pattern alternation: condition = disjunction всех вариантов.
                let mut conds = Vec::with_capacity(alternatives.len());
                for alt in alternatives {
                    conds.push(self.pattern_cond(alt, scr)?);
                }
                Ok(format!("({})", conds.join(" || ")))
            }
            _ => Ok("true".into()),
        }
    }

    /// Emit variable bindings for pattern (after condition is confirmed true).
    fn pattern_bind_typed(&mut self, pat: &Pattern, scr: &str) -> Result<(), String> {
        match pat {
            Pattern::Ident { name, .. } => {
                // Infer type from what scr is
                let ty = self.var_types.get(scr).cloned()
                    .unwrap_or_else(|| "nova_int".into());
                self.var_types.insert(name.clone(), ty.clone());
                self.line(&format!("{} {} = {};", ty, name, scr));
            }
            Pattern::Variant { path, kind, .. } => {
                let variant_name = path.last().cloned().unwrap_or_default();
                let scr_ty = self.var_types.get(scr).cloned().unwrap_or_default();
                // Detect if scr is already a pointer-cast form (e.g., "((NovaOpt_nova_int*)(outer.value))")
                let is_opt_ptr = scr.starts_with("((NovaOpt_");
                let is_opt = !is_opt_ptr && scr_ty.starts_with("NovaOpt_");
                match kind {
                    VariantPatternKind::Tuple { patterns, .. } => {
                        // Look up field types from sum schema
                        let type_name = if path.len() > 1 {
                            path[..path.len()-1].join("_")
                        } else if is_opt {
                            scr_ty.clone()
                        } else {
                            // Prefer mono schema derived from scrutinee's concrete type
                            // (e.g. Nova_LinkedList____nova_int* → LinkedList____nova_int)
                            // over find_variant which may return the erased base schema.
                            let scr_base = scr_ty
                                .trim_start_matches("Nova_")
                                .trim_end_matches('*')
                                .trim()
                                .to_string();
                            if !scr_base.is_empty() && self.sum_schemas.contains_key(&scr_base) {
                                scr_base
                            } else {
                                self.find_variant(&variant_name)
                                    .map(|(t, _)| t)
                                    .unwrap_or_default()
                            }
                        };

                        // Check if scrutinee has a boxed inner Option type
                        let scr_inner_ty = self.option_inner_types.get(scr).cloned();
                        // Plan 14 Ф.1: temp-registrations для recursive
                        // pattern_bind_typed (как в pattern_cond).
                        let mut tmp_registrations: Vec<String> = Vec::new();
                        for (i, p) in patterns.iter().enumerate() {
                            let sub_is_opt_variant = matches!(p, Pattern::Variant { path, .. } if path.last().map_or(false, |n| n == "Some" || n == "None"));
                            let (field, field_ty, is_boxed_inner) = if is_opt {
                                let raw = format!("{}.value", scr);
                                // Plan 14 Ф.1: Option-payload type — strip
                                // "NovaOpt_" из scr_ty чтобы получить
                                // реальный T (nova_bool, NovaOpt_nova_int,
                                // _NovaTuple2, Nova_Foo*, etc.).
                                // Plan 54 Ф.9: для pointer T (Nova_X*) sanitized
                                // id ≠ c_ty. recovery через novaopt_value_types
                                // если ранее зарегистрировано register_novaopt_decl.
                                let t_from_scr = scr_ty.strip_prefix("NovaOpt_")
                                    .map(|s| {
                                        self.novaopt_value_types.borrow()
                                            .get(s).cloned()
                                            .unwrap_or_else(|| s.to_string())
                                    });
                                if sub_is_opt_variant {
                                    // Inner — Option-typed (nested). Используем
                                    // sub_t если оно тоже NovaOpt_*; иначе legacy
                                    // pointer-cast форма.
                                    if let Some(sub_t) = t_from_scr.as_ref()
                                        .filter(|t| t.starts_with("NovaOpt_"))
                                    {
                                        // Direct value access — register
                                        // type для recursive pattern_bind_typed.
                                        self.var_types.insert(raw.clone(), sub_t.clone());
                                        tmp_registrations.push(raw.clone());
                                        (raw, sub_t.clone(), false)
                                    } else {
                                        (format!("((NovaOpt_nova_int*)({}))", raw),
                                         "NovaOpt_nova_int*".into(), true)
                                    }
                                } else if let Some(ref inner_ty) = scr_inner_ty {
                                    // Scrutinee has a boxed struct inner type; deref to get value
                                    let deref_ty = inner_ty.trim_end_matches('*').to_string();
                                    (format!("(*({})({}))", inner_ty, raw), deref_ty, false)
                                } else if let Some(t) = t_from_scr {
                                    // Plan 14 Ф.1: typed payload — bind с
                                    // реальным T вместо nova_int.
                                    (raw, t, false)
                                } else {
                                    (raw, "nova_int".into(), false)
                                }
                            } else if is_opt_ptr {
                                let raw = format!("{}->value", scr);
                                if sub_is_opt_variant {
                                    (format!("((NovaOpt_nova_int*)({}))", raw), "NovaOpt_nova_int*".into(), true)
                                } else {
                                    (raw, "nova_int".into(), false)
                                }
                            } else {
                                let field_types: Vec<String> = self.sum_schemas
                                    .get(&type_name)
                                    .and_then(|v| v.get(&variant_name))
                                    .cloned()
                                    .unwrap_or_default();
                                let ft = field_types.get(i)
                                    .cloned()
                                    .unwrap_or_else(|| "nova_int".into());
                                let raw = format!("{}->payload.{}._{}",
                                    scr, variant_name, i);
                                // If sub-pattern is an Option variant, payload is a boxed pointer
                                if sub_is_opt_variant {
                                    (format!("((NovaOpt_nova_int*)({}))", raw), "NovaOpt_nova_int*".into(), true)
                                } else {
                                    (raw, ft, false)
                                }
                            };
                            if let Pattern::Ident { name, .. } = p {
                                if is_boxed_inner {
                                    // field is a "NovaOpt_nova_int*" pointer; deref to get value
                                    self.var_types.insert(name.clone(), "NovaOpt_nova_int".into());
                                    self.line(&format!("NovaOpt_nova_int {} = *{};", name, field));
                                } else {
                                    self.var_types.insert(name.clone(), field_ty.clone());
                                    self.line(&format!("{} {} = {};", field_ty, name, field));
                                }
                            } else {
                                self.pattern_bind_typed(p, &field)?;
                            }
                        }
                        // Plan 14 Ф.1: cleanup временных регистраций
                        // (после recursive pattern_bind_typed).
                        for k in &tmp_registrations {
                            self.var_types.remove(k);
                        }
                    }
                    VariantPatternKind::Unit => {}
                }
            }
            Pattern::Array { elems, .. } => {
                // Bind array pattern elements: [x, ..] binds x = arr->data[0]
                let mut item_idx = 0usize;
                for elem in elems {
                    match elem {
                        ArrayPatternElem::Item(p) => {
                            let field = format!("{}->data[{}]", scr, item_idx);
                            if let Pattern::Ident { name, .. } = p {
                                self.var_types.insert(name.clone(), "nova_int".to_string());
                                self.line(&format!("nova_int {} = {};", name, field));
                            } else {
                                self.pattern_bind_typed(p, &field)?;
                            }
                            item_idx += 1;
                        }
                        ArrayPatternElem::RestBind(name) => {
                            // Bind the rest of the array from item_idx onwards as a new sub-array
                            let rest_tmp = self.fresh_tmp();
                            self.line(&format!(
                                "NovaArray_nova_int* {} = nova_array_new_nova_int({}->len - {});",
                                rest_tmp, scr, item_idx));
                            self.line(&format!(
                                "for (int64_t _ri = {}; _ri < {}->len; _ri++) {{ nova_array_push_nova_int({}, {}->data[_ri]); }}",
                                item_idx, scr, rest_tmp, scr));
                            self.var_types.insert(name.clone(), "NovaArray_nova_int*".to_string());
                            self.line(&format!("NovaArray_nova_int* {} = {};", name, rest_tmp));
                        }
                        ArrayPatternElem::Rest => {}
                    }
                }
            }
            Pattern::Tuple(pats, _) => {
                for (i, p) in pats.iter().enumerate() {
                    match p {
                        Pattern::Wildcard(_) | Pattern::Literal(..) => {}
                        Pattern::Ident { name, .. } => {
                            let field_ty = self.tuple_element_types.get(scr)
                                .and_then(|tys| tys.get(i))
                                .cloned()
                                .unwrap_or_else(|| "nova_int".into());
                            let field = format!("{}.f{}", scr, i);
                            self.var_types.insert(name.clone(), field_ty.clone());
                            self.line(&format!("{} {} = {};", field_ty, name, field));
                        }
                        Pattern::Tuple(..) => {
                            // The field may be stored as nova_int (pointer to _NovaTupleN).
                            // Look up the actual element type from tuple_element_types.
                            let elem_ty = self.tuple_element_types.get(scr)
                                .and_then(|tys| tys.get(i))
                                .cloned()
                                .unwrap_or_else(|| "nova_int".into());
                            let field_raw = format!("{}.f{}", scr, i);
                            if elem_ty.ends_with('*') && elem_ty.starts_with("_NovaTuple") {
                                // Cast nova_int back to pointer, bind through pointer
                                let base_ty = elem_ty.trim_end_matches('*');
                                let ptr_tmp = self.fresh_tmp();
                                self.line(&format!("{}* {} = ({}*)({});", base_ty, ptr_tmp, base_ty, field_raw));
                                // Build deref expression as struct value via temp
                                let val_tmp = self.fresh_tmp();
                                self.line(&format!("{} {} = *{};", base_ty, val_tmp, ptr_tmp));
                                // Register element types for val_tmp (we don't know them exactly, default nova_int)
                                self.var_types.insert(val_tmp.clone(), base_ty.to_string());
                                self.pattern_bind_typed(p, &val_tmp)?;
                            } else {
                                self.pattern_bind_typed(p, &field_raw)?;
                            }
                        }
                        _ => {
                            let field = format!("{}.f{}", scr, i);
                            self.pattern_bind_typed(p, &field)?;
                        }
                    }
                }
            }
            Pattern::Record { type_path, fields, .. } => {
                let scr_ty = self.var_types.get(scr).cloned().unwrap_or_default();
                // Plan 53: anonymous record pattern `{ x, y }` (type_path
                // is None) — выводим имя record-типа из scr_ty: e.g.
                // `Nova_Pair*` → `Pair`. Это нужно для let-destructuring,
                // где user обычно не пишет type-prefix.
                let type_name_from_path = type_path
                    .as_ref()
                    .and_then(|p| p.last().cloned())
                    .unwrap_or_else(|| {
                        scr_ty
                            .trim_end_matches('*')
                            .trim()
                            .strip_prefix("Nova_")
                            .unwrap_or("")
                            .to_string()
                    });
                let is_plain_record = self.record_schemas.contains_key(&type_name_from_path);
                let accessor = if Self::is_value_type(&scr_ty) { "." } else { "->" };
                if is_plain_record {
                    // Plain record: bind fields directly from scr->field or scr.field
                    let field_types = self.record_schemas.get(&type_name_from_path).cloned().unwrap_or_default();
                    for field in fields {
                        let ty = field_types.get(&field.name).cloned().unwrap_or_else(|| "nova_int".into());
                        let mfn = Self::mangle_field_name(&field.name);
                        let field_access = format!("{}{}{}", scr, accessor, mfn);
                        match &field.pattern {
                            None => {
                                self.var_types.insert(field.name.clone(), ty.clone());
                                self.line(&format!("{} {} = {};", ty, field.name, field_access));
                            }
                            Some(Pattern::Ident { name, .. }) => {
                                self.var_types.insert(name.clone(), ty.clone());
                                self.line(&format!("{} {} = {};", ty, name, field_access));
                            }
                            Some(Pattern::Wildcard(_)) | Some(Pattern::Literal(..)) => {}
                            Some(sub_pat) => {
                                // Plan 53: регистрируем тип field_access, чтобы
                                // рекурсивный pattern_bind_typed видел тип
                                // (нужно для nested record-destructure
                                // `{ inner: { x, y } }` — иначе inner идёт в
                                // sum-variant ветку из-за пустого scr_ty).
                                self.var_types.insert(field_access.clone(), ty.clone());
                                self.pattern_bind_typed(sub_pat, &field_access)?;
                            }
                        }
                    }
                } else {
                    // Sum-type record variant: bind fields from scr->payload.Variant.field
                    let variant_name = type_name_from_path.clone();
                    // D109: prefer concrete monomorphized type derived from scr_ty over
                    // the erased base type returned by find_variant (which picks the
                    // shortest name, e.g. "Slot" instead of "Slot____nova_str__nova_int").
                    let sum_type_name_from_scr = scr_ty
                        .strip_prefix("Nova_").unwrap_or(&scr_ty)
                        .trim_end_matches('*').trim().to_string();
                    let sum_type_name = if !sum_type_name_from_scr.is_empty()
                        && self.sum_schemas.contains_key(&sum_type_name_from_scr)
                    {
                        sum_type_name_from_scr
                    } else {
                        self.find_variant(&variant_name)
                            .map(|(t, _)| t)
                            .unwrap_or_else(|| {
                                scr_ty.strip_prefix("Nova_").unwrap_or(&scr_ty)
                                    .trim_end_matches('*').trim().to_string()
                            })
                    };
                    for field in fields {
                        let ty = self.get_record_variant_field_type(&sum_type_name, &variant_name, &field.name)
                            .unwrap_or_else(|| "nova_int".into());
                        let mfn = Self::mangle_field_name(&field.name);
                        let field_access = format!("{}->payload.{}.{}", scr, variant_name, mfn);
                        match &field.pattern {
                            None => {
                                self.var_types.insert(field.name.clone(), ty.clone());
                                self.line(&format!("{} {} = {};", ty, field.name, field_access));
                            }
                            Some(Pattern::Ident { name, .. }) => {
                                self.var_types.insert(name.clone(), ty.clone());
                                self.line(&format!("{} {} = {};", ty, name, field_access));
                            }
                            Some(Pattern::Wildcard(_)) | Some(Pattern::Literal(..)) => {}
                            Some(sub_pat) => {
                                self.pattern_bind_typed(sub_pat, &field_access)?;
                            }
                        }
                    }
                }
            }
            Pattern::Wildcard(_) | Pattern::Literal(..) => {}
            Pattern::Or { alternatives, .. } => {
                // Pattern alternation: bindings берём из первого варианта.
                // По pattern-matching semantike все альтернативы должны
                // вводить одинаковый набор bindings — bootstrap не проверяет,
                // используем первого как канонический.
                if let Some(first) = alternatives.first() {
                    self.pattern_bind_typed(first, scr)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ---- helpers ----

    /// Map (param_tys, ret_ty) to the NovaClos call macro name.
    fn clos_call_macro(param_tys: &[String], ret_ty: &str) -> Option<&'static str> {
        match (param_tys, ret_ty) {
            ([], r) if r == "nova_int"                                                => Some("NOVA_CLOS_CALL_vi"),
            ([p0], r) if p0 == "nova_int" && r == "nova_int"                         => Some("NOVA_CLOS_CALL_ii"),
            ([p0], r) if p0 == "nova_int" && r == "nova_bool"                        => Some("NOVA_CLOS_CALL_ib"),
            ([p0, p1], r) if p0 == "nova_int" && p1 == "nova_int" && r == "nova_int" => Some("NOVA_CLOS_CALL_iii"),
            ([p0, p1], r) if p0 == "void*"    && p1 == "nova_int" && r == "nova_int" => Some("NOVA_CLOS_CALL_vii"),
            _ => None,
        }
    }

    fn fresh_tmp(&mut self) -> String {
        let n = self.tmp_counter;
        self.tmp_counter += 1;
        format!("_nv_tmp_{}", n)
    }

    /// Clear the heap-promoted var_boxed registry at function exit.
    /// No #undef needed — var_boxed uses ExprKind::Ident rewriting, not macros.
    fn flush_boxed_vars(&mut self) {
        self.var_boxed.clear();
    }

    /// Сгенерировать temporary name с **семантической ролью** в имени:
    /// `_nv_<role>_<n>` (например `_nv_scr_0`, `_nv_match_3`,
    /// `_nv_matched_2`). Понятнее чем сырой `_nova_tmp42` при чтении
    /// сгенерированного C. Role должна быть коротким snake_case
    /// идентификатором.
    fn fresh_tmp_named(&mut self, role: &str) -> String {
        let n = self.tmp_counter;
        self.tmp_counter += 1;
        format!("_nv_{}_{}", role, n)
    }

    /// Box a value as void* for storage in a void* field (generic type erasure).
    /// Scalars: cast via intptr_t. Structs (nova_str): heap-allocate and return pointer.
    fn box_value_as_void_ptr(&mut self, val: &str, val_ty: &str) -> String {
        match val_ty {
            "nova_int" | "nova_bool" | "nova_f64" =>
                format!("(void*)(intptr_t)({})", val),
            "nova_str" => {
                let tmp = self.fresh_tmp();
                self.line(&format!("nova_str* {} = (nova_str*)nova_alloc(sizeof(nova_str));", tmp));
                self.line(&format!("*{} = {};", tmp, val));
                format!("(void*)({})", tmp)
            }
            _ if val_ty.ends_with('*') =>
                // Already a pointer — cast directly
                format!("(void*)({})", val),
            _ =>
                format!("(void*)(intptr_t)({})", val),
        }
    }

    /// Collect all identifier names referenced in an expression (for free-variable detection).
    fn collect_free_idents(expr: &Expr, out: &mut HashSet<String>) {
        match &expr.kind {
            ExprKind::Ident(n) => { out.insert(n.clone()); }
            ExprKind::Binary { left, right, .. } => { Self::collect_free_idents(left, out); Self::collect_free_idents(right, out); }
            ExprKind::Unary { operand, .. } => Self::collect_free_idents(operand, out),
            ExprKind::Call { func, args, .. } => {
                Self::collect_free_idents(func, out);
                for a in args { Self::collect_free_idents(a.expr(), out); }
            }
            ExprKind::Member { obj, .. } => Self::collect_free_idents(obj, out),
            ExprKind::Index { obj, index } => { Self::collect_free_idents(obj, out); Self::collect_free_idents(index, out); }
            ExprKind::Block(b) => {
                for s in &b.stmts { Self::collect_free_idents_stmt(s, out); }
                if let Some(t) = &b.trailing { Self::collect_free_idents(t, out); }
            }
            ExprKind::If { cond, then, else_, .. } => {
                Self::collect_free_idents(cond, out);
                Self::collect_free_idents_block(then, out);
                if let Some(e) = else_ {
                    match e {
                        ElseBranch::Block(b) => Self::collect_free_idents_block(b, out),
                        ElseBranch::If(ex) => Self::collect_free_idents(ex, out),
                    }
                }
            }
            ExprKind::Lambda { body, .. } => Self::collect_free_idents(body, out),
            ExprKind::TupleLit(elems) => { for e in elems { Self::collect_free_idents(e, out); } }
            ExprKind::Detach(b) => {
                for s in &b.stmts { Self::collect_free_idents_stmt(s, out); }
                if let Some(t) = &b.trailing { Self::collect_free_idents(t, out); }
            }
            ExprKind::Supervised { body, cancel } => {
                for s in &body.stmts { Self::collect_free_idents_stmt(s, out); }
                if let Some(t) = &body.trailing { Self::collect_free_idents(t, out); }
                // Plan 47: cancel-token expr — свободный идентификатор scope'а.
                if let Some(c) = cancel { Self::collect_free_idents(c, out); }
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => Self::collect_free_idents(chan, out),
                        SelectOp::Send { chan, value } => {
                            Self::collect_free_idents(chan, out);
                            Self::collect_free_idents(value, out);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard { Self::collect_free_idents(g, out); }
                    Self::collect_free_idents_block(&arm.body, out);
                }
            }
            _ => {}
        }
    }

    fn collect_free_idents_block(block: &Block, out: &mut HashSet<String>) {
        for s in &block.stmts { Self::collect_free_idents_stmt(s, out); }
        if let Some(t) = &block.trailing { Self::collect_free_idents(t, out); }
    }

    fn collect_free_idents_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
        match stmt {
            Stmt::Let(d) => { Self::collect_free_idents(&d.value, out); }
            Stmt::Assign { target, value, .. } => { Self::collect_free_idents(target, out); Self::collect_free_idents(value, out); }
            Stmt::Expr(e) => Self::collect_free_idents(e, out),
            Stmt::Return { value: Some(e), .. } => Self::collect_free_idents(e, out),
            _ => {}
        }
    }

    /// Emit a lambda expression. Returns the C expression (a function pointer or closure pointer).
    fn emit_lambda(
        &mut self,
        params: &[LambdaParam],
        body: &Expr,
        context_param_tys: Option<&[(String, String)]>, // (param_c_ty, ret_c_ty) from outer fn sig context
        return_type_ann: Option<&TypeRef>,
    ) -> Result<String, String> {
        let id = self.lambda_counter;
        self.lambda_counter += 1;

        // Determine param C types — use explicit types, or default to nova_int
        let param_c_tys: Vec<String> = params.iter().enumerate().map(|(i, p)| {
            if let Some(ty) = &p.ty {
                self.type_ref_to_c(ty).unwrap_or_else(|_| "nova_int".into())
            } else if let Some(ctx) = context_param_tys {
                ctx.get(i).map(|(ty, _)| ty.clone()).unwrap_or_else(|| "nova_int".into())
            } else {
                "nova_int".into()
            }
        }).collect();

        // Determine return type.
        // Приоритет: явная annotation `-> T` > context > inference из body > nova_int.
        //
        // Plan 20 follow-up (closure return type inference): closure'ы вроде
        // `|| { side_effect() }` без annotation должны inferить return type
        // из body. Без этого codegen эмитил `nova_int` по дефолту, и closure
        // c side-effect-only body (тип unit) ломал compilation:
        //     error: returning 'nova_unit' from a function with incompatible
        //     result type 'nova_int'
        // Это блокировало естественный паттерн callback-без-возврата для HOF
        // (map для side effects, defer { cleanup_callback() }, и т.д.).
        let ret_c_ty = if let Some(rt) = return_type_ann {
            self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into())
        } else if let Some(ctx) = context_param_tys {
            ctx.first().and_then(|(_, ret)| if !ret.is_empty() { Some(ret.clone()) } else { None })
                .unwrap_or_else(|| {
                    // Context был задан (например HOF), но return-type
                    // не указан — infer из body.
                    self.infer_lambda_return_type_with_params(body, params, &param_c_tys)
                })
        } else {
            // Ни annotation, ни context — infer из body.
            self.infer_lambda_return_type_with_params(body, params, &param_c_tys)
        };

        // Detect free variables: idents in body that are NOT lambda params
        let param_names: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
        let mut body_idents = HashSet::new();
        Self::collect_free_idents(body, &mut body_idents);
        // Free vars = body idents that exist in var_types and are not lambda params
        let free_vars: Vec<(String, String)> = body_idents.iter()
            .filter(|n| !param_names.contains(*n) && self.var_types.contains_key(*n))
            .filter(|n| {
                // Exclude global function names (they are registered too, but are not "captured")
                let ty = self.var_types.get(*n).map(|s| s.as_str()).unwrap_or("");
                !ty.starts_with("fn_ret_")
            })
            .map(|n| (n.clone(), self.var_types.get(n).cloned().unwrap_or_else(|| "nova_int".into())))
            .collect();

        // Determine the NovaClos_XX struct type name for this closure signature
        let clos_struct = Self::clos_struct_name(&param_c_tys, &ret_c_ty);
        let env_name = format!("nova_lambda_{}_env", id);
        let body_name = format!("nova_lambda_{}_body", id);

        // Plan 20 follow-up: mut-captures хранятся как pointer'ы в env,
        // чтобы writes из closure body обновляли original mut local в caller'е
        // (D32-spec mut-capture by-reference). Immutable captures — by value
        // (snapshot). Mutability detection: var_mutable set.
        let free_var_is_mut: Vec<bool> = free_vars.iter()
            .map(|(n, _)| self.var_mutable.contains(n))
            .collect();

        // Build env struct fields. Mut fields = pointer type.
        let env_fields: String = if free_vars.is_empty() {
            "int _dummy;".to_string() // avoid empty struct (UB in C)
        } else {
            free_vars.iter().zip(&free_var_is_mut)
                .map(|((n, ty), is_mut)| {
                    if *is_mut {
                        format!("{}* {};", ty, n)        // pointer for mut
                    } else {
                        format!("{} {};", ty, n)         // value for immutable
                    }
                })
                .collect::<Vec<_>>().join(" ")
        };

        // Body function signature: takes void* env + params
        let body_params_str = {
            let mut parts = vec!["void* _env_ptr".to_string()];
            for (p, ty) in params.iter().zip(&param_c_tys) {
                parts.push(format!("{} {}", ty, p.name));
            }
            parts.join(", ")
        };

        // Emit forward decl for body function
        let fwd = format!("static {} {}({});", ret_c_ty, body_name, body_params_str);
        self.lambda_forward_decls.push_str(&fwd);
        self.lambda_forward_decls.push('\n');

        // Save current var_types for params, emit body into lambda_impls
        let saved: Vec<(String, Option<String>)> = params.iter().zip(&param_c_tys)
            .map(|(p, ty)| (p.name.clone(), self.var_types.insert(p.name.clone(), ty.clone())))
            .collect();
        // Register function-typed lambda params in fn_param_sigs so f(x) calls work inside body
        let saved_fn_sigs: Vec<(String, Option<(Vec<String>, String)>)> = params.iter().filter_map(|p| {
            if let Some(TypeRef::Func { params: fp, return_type, .. }) = &p.ty {
                let ptys: Vec<String> = fp.iter().map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into())).collect();
                let rty = return_type.as_ref().map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into())).unwrap_or_else(|| "nova_unit".into());
                let prev = self.fn_param_sigs.insert(p.name.clone(), (ptys, rty));
                Some((p.name.clone(), prev))
            } else { None }
        }).collect();

        // Emit body into lambda_impls using a temp output buffer.
        // Save and clear var_boxed: the lambda body is a separate C function,
        // so caller-scope heap-promotion (#define tricks) must not bleed in.
        // Instead we register mut-captures as `_env->name` in var_boxed so
        // ExprKind::Ident resolves them to `(*_env->name)` without macros.
        let old_out = std::mem::take(&mut self.out);
        let old_indent = self.indent;
        let saved_var_boxed = std::mem::take(&mut self.var_boxed);
        self.indent = 0;

        // Env struct declaration
        self.out.push_str(&format!("typedef struct {{ {} }} {};\n", env_fields, env_name));
        // Body function implementation
        self.line(&format!("static {} {}({}) {{", ret_c_ty, body_name, body_params_str));
        self.indent = 1;
        // Unpack env. Mut-captures: register `name → _env->name` in var_boxed
        // so ExprKind::Ident emits `(*_env->name)` — pointer-safe, no #define.
        // Immutable captures: local copy (no aliasing needed).
        if !free_vars.is_empty() {
            self.line(&format!("{}* _env = ({}*)_env_ptr;", env_name, env_name));
            for ((name, ty), is_mut) in free_vars.iter().zip(&free_var_is_mut) {
                if *is_mut {
                    // Register in var_boxed as "_env->name" so Ident emits (*_env->name).
                    self.var_boxed.insert(name.clone(), format!("_env->{}", name));
                } else {
                    self.line(&format!("{} {} = _env->{};", ty, name, name));
                }
            }
        }
        let body_val = self.emit_expr(body)?;
        if ret_c_ty == "nova_unit" {
            self.line(&format!("{};", body_val));
            self.line("return NOVA_UNIT;");
        } else {
            self.line(&format!("return {};", body_val));
        }
        self.indent = 0;
        self.line("}");
        self.line("");
        let impl_str = std::mem::replace(&mut self.out, old_out);
        self.indent = old_indent;
        // Restore caller-scope var_boxed (lambda body used its own set of entries).
        self.var_boxed = saved_var_boxed;
        self.lambda_impls.push_str(&impl_str);

        // Restore params and fn_param_sigs
        for (name, prev) in saved {
            match prev {
                Some(old) => { self.var_types.insert(name, old); }
                None => { self.var_types.remove(&name); }
            }
        }
        for (name, prev) in saved_fn_sigs {
            match prev {
                Some(old) => { self.fn_param_sigs.insert(name, old); }
                None => { self.fn_param_sigs.remove(&name); }
            }
        }

        // At the call site: allocate env + NovaClos_XX struct.
        // Mut-captures are heap-promoted: the local is replaced by a heap box
        // so the closure can safely outlive the declaring scope (escape safety).
        // If a mut var is already boxed (from a prior closure in the same fn),
        // we reuse the existing box — all closures over the same var share state.
        let env_tmp = self.fresh_tmp();
        let clos_tmp = self.fresh_tmp();
        self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", env_name, env_tmp, env_name, env_name));
        for ((name, ty), is_mut) in free_vars.iter().zip(&free_var_is_mut) {
            if *is_mut {
                let box_var = if let Some(existing) = self.var_boxed.get(name) {
                    // Already heap-promoted by an earlier closure in this fn — reuse.
                    existing.clone()
                } else {
                    // First capture of this mut var: promote to heap box.
                    let bv = format!("_box_{}", name);
                    // Allocate box and copy current stack value into it.
                    // Use the plain name here (before var_boxed is set) so the
                    // emit_expr for `name` still resolves to the stack variable.
                    self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", ty, bv, ty, ty));
                    self.line(&format!("*{} = {};", bv, name));
                    // Register in var_boxed: from this point on, ExprKind::Ident
                    // for `name` emits `(*_box_name)` instead of bare `name`,
                    // keeping caller reads/writes in sync with the closure's env ptr.
                    self.var_boxed.insert(name.clone(), bv.clone());
                    bv
                };
                // Env stores the box pointer — safe even if closure escapes scope.
                self.line(&format!("{}->{} = {};", env_tmp, name, box_var));
            } else {
                // Immutable: snapshot value (no escape risk).
                self.line(&format!("{}->{} = {};", env_tmp, name, name));
            }
        }
        self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", clos_struct, clos_tmp, clos_struct, clos_struct));
        self.line(&format!("{}->{} = ({})({});", clos_tmp, "fn", Self::clos_fn_ty(&param_c_tys, &ret_c_ty), body_name));
        self.line(&format!("{}->{} = (void*)({});", clos_tmp, "env", env_tmp));

        Ok(format!("(void*)({})", clos_tmp))
    }

    fn clos_struct_name(param_tys: &[String], ret_ty: &str) -> &'static str {
        match (param_tys, ret_ty) {
            ([], r) if r == "nova_int"                                                => "NovaClos_vi",
            ([p0], r) if p0 == "nova_int" && r == "nova_int"                         => "NovaClos_ii",
            ([p0], r) if p0 == "nova_int" && r == "nova_bool"                        => "NovaClos_ib",
            ([p0, p1], r) if p0 == "nova_int" && p1 == "nova_int" && r == "nova_int" => "NovaClos_iii",
            ([p0, p1], r) if p0 == "void*"    && p1 == "nova_int" && r == "nova_int" => "NovaClos_vii",
            // Plan 11 Ф.4: для arbitrary signatures используем generic
            // NovaClosBase ({fn, env}) — size совпадает, fn-ptr cast'ается на
            // call-site по нужной сигнатуре.
            _ => "NovaClosBase",
        }
    }

    fn clos_fn_ty(param_tys: &[String], ret_ty: &str) -> &'static str {
        match (param_tys, ret_ty) {
            ([], r) if r == "nova_int"                                                => "nova_fn_vi",
            ([p0], r) if p0 == "nova_int" && r == "nova_int"                         => "nova_fn_ii",
            ([p0], r) if p0 == "nova_int" && r == "nova_bool"                        => "nova_fn_ib",
            ([p0, p1], r) if p0 == "nova_int" && p1 == "nova_int" && r == "nova_int" => "nova_fn_iii",
            ([p0, p1], r) if p0 == "void*"    && p1 == "nova_int" && r == "nova_int" => "nova_fn_vii",
            // Plan 11 Ф.4: void* — generic fn pointer (cast applied at call site).
            _ => "void*",
        }
    }

    fn line(&mut self, s: &str) {
        let indent = "    ".repeat(self.indent);
        let _ = writeln!(self.out, "{}{}", indent, s);
    }

    /// Plan 11 Ф.4: emit method value (bound or unbound).
    ///
    /// **Bound case** (`obj.@method`): obj is a value expression (variable,
    /// expression). Эмитим closure struct который захватывает `obj` как self
    /// и при вызове делает `Nova_T_method_<m>(self, args...)`.
    ///
    /// **Unbound case** (`Type.@method`): obj is a Type-name path. Эмитим
    /// closure (env пустой) который при вызове делает `Nova_T_method_<m>(arg0,
    /// arg1, ...)` где arg0 — receiver.
    ///
    /// Для overload'ов берём первый match (ambiguity resolution через Ф.5
    /// `as fn(...)` annotation). Single-overload — без проблем.
    fn emit_method_value(&mut self, obj: &Expr, method_name: &str) -> Result<String, String> {
        self.emit_method_value_typed(obj, method_name, None)
    }

    fn emit_method_value_typed(
        &mut self,
        obj: &Expr,
        method_name: &str,
        target_sig: Option<(Vec<String>, String)>,
    ) -> Result<String, String> {
        // Determine kind: bound (obj is value) vs unbound (obj is Type).
        // Type if obj is Path/Ident starting with uppercase (or primitive type).
        let (type_name, is_unbound) = match &obj.kind {
            ExprKind::Ident(n) => {
                let is_type = n.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
                    || matches!(n.as_str(),
                        "int" | "i8" | "i16" | "i32" | "i64"
                        | "u8" | "u16" | "u32" | "u64"
                        | "f32" | "f64" | "byte" | "bool" | "char" | "str");
                if is_type { (n.clone(), true) } else {
                    // Bound: derive type from var.
                    let obj_ty = self.var_types.get(n).cloned().unwrap_or_default();
                    let t = Self::nova_type_name_from_c(&obj_ty);
                    (t, false)
                }
            }
            ExprKind::Path(parts) if parts.len() == 1 => (parts[0].clone(), true),
            _ => {
                // Bound: infer from obj_ty.
                let obj_ty = self.infer_expr_c_type(obj);
                let t = Self::nova_type_name_from_c(&obj_ty);
                (t, false)
            }
        };
        // Lookup method in registry.
        let key = (type_name.clone(), method_name.to_string());
        let overloads = self.method_overloads.get(&key).cloned()
            .ok_or_else(|| format!("method value: no method `{}` on type `{}`",
                                   method_name, type_name))?;
        // Plan 11 Ф.5: если задан target_sig из `as fn(...)` annotation —
        // ищем overload с матч'ащимися param-типами. Иначе fallback —
        // первый overload (single-overload — typical case).
        let sig = if let Some((target_ptys, _)) = &target_sig {
            // Для bound: target_ptys = method params (без receiver'а).
            // Для unbound: target_ptys = receiver + method params.
            // method_overloads хранит param_c_types БЕЗ receiver'а; сравниваем
            // в зависимости от is_unbound.
            let recv_offset = if is_unbound { 1 } else { 0 };
            let method_target: Vec<String> = target_ptys.iter().skip(recv_offset).cloned().collect();
            overloads.iter()
                .find(|s| s.param_c_types == method_target)
                .cloned()
                .ok_or_else(|| format!("method value: no overload of `{}.{}` matches signature {:?}",
                                       type_name, method_name, method_target))?
        } else {
            overloads.first().cloned()
                .ok_or_else(|| format!("method value: empty overload list for `{}.{}`",
                                       type_name, method_name))?
        };
        let id = self.lambda_counter;
        self.lambda_counter += 1;
        let body_name = format!("nova_mv_{}_body", id);
        let env_name = format!("nova_mv_{}_env", id);

        // Determine receiver C-type. Для primitive — value; для record —
        // pointer Nova_<T>*.
        let recv_c_ty = match type_name.as_str() {
            "int" | "i64" => "nova_int".to_string(),
            "f64" => "nova_f64".to_string(),
            "f32" => "nova_f32".to_string(),
            "str" => "nova_str".to_string(),
            "char" => "nova_int".to_string(),
            "byte" => "nova_byte".to_string(),
            "bool" => "nova_bool".to_string(),
            _ => format!("Nova_{}*", type_name),
        };

        // Param C-types and ret type.
        let params = &sig.param_c_types;
        let ret_ty = &sig.return_c_type;
        let c_name = &sig.c_name;

        // Build closure struct/fn names. Reuse clos_struct_name selecting from
        // hardcoded list for known signatures.
        // For method-value the *closure* signature is:
        //   - bound: same params as method, same return.
        //   - unbound: receiver + same params, same return.
        let closure_param_tys: Vec<String> = if is_unbound {
            std::iter::once(recv_c_ty.clone()).chain(params.iter().cloned()).collect()
        } else {
            params.clone()
        };
        let clos_struct = Self::clos_struct_name(&closure_param_tys, ret_ty);
        let clos_fn_ty  = Self::clos_fn_ty(&closure_param_tys, ret_ty);

        // Generate body fn signature (C):
        //   static <ret> <body_name>(void* env, <closure_params>) { ... }
        let mut body_params = vec!["void* _env_ptr".to_string()];
        for (i, ty) in closure_param_tys.iter().enumerate() {
            body_params.push(format!("{} p{}", ty, i));
        }
        let body_sig = format!("static {} {}({})", ret_ty, body_name, body_params.join(", "));
        // Fwd decl.
        self.lambda_forward_decls.push_str(&format!("{};\n", body_sig));

        // Body emission into lambda_impls.
        let mut impl_buf = String::new();
        if is_unbound {
            // Env пустой — игнорим _env_ptr.
            impl_buf.push_str(&format!("typedef struct {{ int _dummy; }} {};\n", env_name));
            impl_buf.push_str(&format!("{} {{\n", body_sig));
            impl_buf.push_str("    (void)_env_ptr;\n");
            // Call: c_name(p0, p1, ...) — receiver is p0.
            let call_args: Vec<String> = (0..closure_param_tys.len())
                .map(|i| format!("p{}", i)).collect();
            if ret_ty == "nova_unit" {
                impl_buf.push_str(&format!("    {}({});\n", c_name, call_args.join(", ")));
                impl_buf.push_str("    return NOVA_UNIT;\n");
            } else {
                impl_buf.push_str(&format!("    return {}({});\n", c_name, call_args.join(", ")));
            }
            impl_buf.push_str("}\n\n");
        } else {
            // Bound — env содержит receiver.
            impl_buf.push_str(&format!("typedef struct {{ {} self; }} {};\n", recv_c_ty, env_name));
            impl_buf.push_str(&format!("{} {{\n", body_sig));
            impl_buf.push_str(&format!("    {}* _env = ({}*)_env_ptr;\n", env_name, env_name));
            // Call: c_name(_env->self, p0, p1, ...).
            let mut call_args = vec!["_env->self".to_string()];
            for i in 0..params.len() { call_args.push(format!("p{}", i)); }
            if ret_ty == "nova_unit" {
                impl_buf.push_str(&format!("    {}({});\n", c_name, call_args.join(", ")));
                impl_buf.push_str("    return NOVA_UNIT;\n");
            } else {
                impl_buf.push_str(&format!("    return {}({});\n", c_name, call_args.join(", ")));
            }
            impl_buf.push_str("}\n\n");
        }
        self.lambda_impls.push_str(&impl_buf);

        // At call site: allocate env + closure struct, return as void*.
        let env_tmp = self.fresh_tmp();
        let clos_tmp = self.fresh_tmp();
        if is_unbound {
            self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));",
                env_name, env_tmp, env_name, env_name));
            self.line(&format!("{}->_dummy = 0;", env_tmp));
        } else {
            // Emit obj expression to capture as self.
            let o = self.emit_expr(obj)?;
            self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));",
                env_name, env_tmp, env_name, env_name));
            self.line(&format!("{}->self = ({})({});", env_tmp, recv_c_ty, o));
        }
        self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));",
            clos_struct, clos_tmp, clos_struct, clos_struct));
        self.line(&format!("{}->fn = ({})({});", clos_tmp, clos_fn_ty, body_name));
        self.line(&format!("{}->env = (void*)({});", clos_tmp, env_tmp));
        Ok(format!("(void*)({})", clos_tmp))
    }

    /// Plan 14 Ф.3: emit free fn name as first-class value.
    ///
    /// `let f = inc` или `xs.map(inc)` — `inc` нужно превратить в
    /// closure-value `(void*)NovaClos_X*` чтобы callers (HOF, fn-typed
    /// params, fn_param_sigs-driven calls через NOVA_CLOS_CALL_*) работали.
    ///
    /// Generates:
    ///   1. **Thunk** (один раз на user fn, deduped через
    ///      `emitted_fn_thunks`): envless adapter
    ///      `static <ret> nova_fn_<name>_thunk(void* env, args...)
    ///      { (void)env; return nova_fn_<name>(args...); }`.
    ///      Игнорирует `env` (free fn без захвата).
    ///   2. **Closure-литерал** на use-site: alloc'ает `NovaClos_X*` с
    ///      `fn = &nova_fn_<name>_thunk, env = NULL`. Возвращается как
    ///      `(void*)clos_ptr`.
    ///
    /// Если sig user fn'а отсутствует в `user_fn_sigs` (не free fn —
    /// generic, метод, и т.д.) — возвращает `None`, caller должен
    /// fallback'ить на старое поведение (`nova_fn_<name>` raw pointer).
    fn emit_free_fn_value(&mut self, fn_name: &str) -> Option<String> {
        let (param_c_tys, ret_c_ty) = self.user_fn_sigs.get(fn_name).cloned()?;
        // Emit thunk one-time (deduped).
        if !self.emitted_fn_thunks.contains(fn_name) {
            let thunk_name = format!("nova_fn_{}_thunk", fn_name);
            // Build params: void* env, T0 p0, T1 p1, ...
            let mut params = vec!["void* _env".to_string()];
            for (i, ty) in param_c_tys.iter().enumerate() {
                params.push(format!("{} p{}", ty, i));
            }
            let params_str = params.join(", ");
            // Forward decl into lambda_forward_decls.
            self.lambda_forward_decls
                .push_str(&format!("static {} {}({});\n", ret_c_ty, thunk_name, params_str));
            // Body — call original nova_fn_<name>.
            let mut impl_buf = String::new();
            impl_buf.push_str(&format!("static {} {}({}) {{\n", ret_c_ty, thunk_name, params_str));
            impl_buf.push_str("    (void)_env;\n");
            let call_args: Vec<String> = (0..param_c_tys.len())
                .map(|i| format!("p{}", i))
                .collect();
            if ret_c_ty == "nova_unit" {
                impl_buf.push_str(&format!("    nova_fn_{}({});\n", fn_name, call_args.join(", ")));
                impl_buf.push_str("    return NOVA_UNIT;\n");
            } else {
                impl_buf.push_str(&format!("    return nova_fn_{}({});\n", fn_name, call_args.join(", ")));
            }
            impl_buf.push_str("}\n\n");
            self.lambda_impls.push_str(&impl_buf);
            self.emitted_fn_thunks.insert(fn_name.to_string());
        }
        // Emit closure-struct on use site.
        let clos_struct = Self::clos_struct_name(&param_c_tys, &ret_c_ty);
        let clos_fn_ty = Self::clos_fn_ty(&param_c_tys, &ret_c_ty);
        let clos_tmp = self.fresh_tmp();
        let thunk_name = format!("nova_fn_{}_thunk", fn_name);
        self.line(&format!(
            "{}* {} = ({}*)nova_alloc(sizeof({}));",
            clos_struct, clos_tmp, clos_struct, clos_struct
        ));
        self.line(&format!(
            "{}->fn = ({})({});",
            clos_tmp, clos_fn_ty, thunk_name
        ));
        self.line(&format!("{}->env = (void*)0;", clos_tmp));
        Some(format!("(void*)({})", clos_tmp))
    }


    /// Look up the C type of a record variant field.
    fn get_record_variant_field_type(&self, type_name: &str, variant_name: &str, field_name: &str) -> Option<String> {
        let key = format!("{}::{}::{}", type_name, variant_name, field_name);
        self.record_variant_field_types.get(&key).cloned()
    }

    /// Find which sum type a variant belongs to. Returns (type_name, field_types).
    fn find_variant(&self, variant_name: &str) -> Option<(String, Vec<String>)> {
        // Prefer canonical type names over C-mangled aliases (e.g. "Option" over "NovaOpt_nova_int")
        let mut result: Option<(String, Vec<String>)> = None;
        for (type_name, variants) in &self.sum_schemas {
            if let Some(fields) = variants.get(variant_name) {
                match result {
                    None => result = Some((type_name.clone(), fields.clone())),
                    Some((ref existing, _)) => {
                        // Prefer shorter, non-mangled type names
                        if type_name.len() < existing.len() || existing.starts_with("NovaOpt_") || existing.starts_with("Nova_Result") {
                            result = Some((type_name.clone(), fields.clone()));
                        }
                    }
                }
            }
        }
        result
    }

    fn infer_expr_c_type_str(&self, expr: &Expr) -> String {
        self.infer_expr_c_type(expr)
    }

    /// D74: Map a Nova f64/f32 method to a C math.h function.
    /// Все эти функции — стандартные libm; <math.h> подключён через
    /// nova_rt.h (косвенно, через alloc.h/effects.h fall-through).
    fn f64_method_to_c(method: &str) -> Option<&'static str> {
        match method {
            "sqrt"      => Some("sqrt"),
            "cbrt"      => Some("cbrt"),
            "abs"       => Some("fabs"),
            "ceil"      => Some("ceil"),
            "floor"     => Some("floor"),
            "round"     => Some("round"),
            "trunc"     => Some("trunc"),
            "sin"       => Some("sin"),
            "cos"       => Some("cos"),
            "tan"       => Some("tan"),
            "asin"      => Some("asin"),
            "acos"      => Some("acos"),
            "atan"      => Some("atan"),
            "atan2"     => Some("atan2"),
            "sinh"      => Some("sinh"),
            "cosh"      => Some("cosh"),
            "tanh"      => Some("tanh"),
            "exp"       => Some("exp"),
            "exp2"      => Some("exp2"),
            "ln"        => Some("log"),       // натуральный log
            "log2"      => Some("log2"),
            "log10"     => Some("log10"),
            "pow"       => Some("pow"),
            "hypot"     => Some("hypot"),
            "is_nan"    => Some("isnan"),
            "is_finite" => Some("isfinite"),
            "is_infinite" => Some("isinf"),
            _ => None,
        }
    }

    /// D109: встроенные методы примитивных типов (hash/eq/ord).
    /// Возвращает Fn(c_function_name) или BinOp(c_operator).
    fn prim_builtin_method(c_ty: &str, method: &str) -> Option<PrimBuiltin> {
        match (c_ty, method) {
            // hash — C-функция
            ("nova_int",  "hash") => Some(PrimBuiltin::Fn("nova_int_hash")),
            ("nova_bool", "hash") => Some(PrimBuiltin::Fn("nova_bool_hash")),
            ("nova_f64",  "hash") => Some(PrimBuiltin::Fn("nova_f64_hash")),
            // eq — inline оператор (все скаляры)
            ("nova_int" | "nova_bool" | "nova_f64", "eq") => Some(PrimBuiltin::BinOp("==")),
            // lt/le/gt/ge — только упорядоченные типы (не bool)
            ("nova_int" | "nova_f64", "lt") => Some(PrimBuiltin::BinOp("<")),
            ("nova_int" | "nova_f64", "le") => Some(PrimBuiltin::BinOp("<=")),
            ("nova_int" | "nova_f64", "gt") => Some(PrimBuiltin::BinOp(">")),
            ("nova_int" | "nova_f64", "ge") => Some(PrimBuiltin::BinOp(">=")),
            _ => None,
        }
    }

    /// D74: Map a Nova int method to a C function.
    /// Большинство int-операций — встроенные операторы; здесь только
    /// дополнительные (abs).
    fn int_method_to_c(method: &str) -> Option<&'static str> {
        match method {
            "abs"  => Some("llabs"),     // long long abs
            _ => None,
        }
    }

    /// Plan 48: вычислить return-type метода для монотипа.
    /// Если `obj_ty = "Nova_X____A__B*"`, извлекаем base = "X",
    /// type_args = ["A", "B"], находим метод в generic_type_methods["X"],
    /// применяем apply_type_subst_to_ref с подстановкой generics→type_args.
    fn infer_mono_method_ret(&self, obj_ty: &str, method: &str) -> Option<String> {
        // Формат: "Nova_X____A__B*" или без суффикса "*"
        let stripped = obj_ty.strip_prefix("Nova_")?.trim_end_matches('*');
        // Должно содержать "____" — разделитель base от type args
        let sep_pos = stripped.find("____")?;
        let base_name = &stripped[..sep_pos];
        let args_str = &stripped[sep_pos + 4..]; // после "____"
        // type args разделены "__" (двойное подчёркивание)
        let type_args: Vec<String> = args_str.split("__")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let template = self.generic_type_templates.get(base_name)?;
        let methods = self.generic_type_methods.get(base_name)?;
        let fd = methods.iter().find(|m| m.name == method)?;
        let ret_ref = fd.return_type.as_ref()?;
        // Строим подстановку: generic_param_name → concrete_c_type
        let subst: Vec<(String, Option<String>)> = template.generics.iter()
            .zip(type_args.iter())
            .map(|(g, c)| (g.name.clone(), Some(c.clone())))
            .collect();
        let ret_c = Self::apply_type_subst_to_ref(ret_ref, &subst)?;
        // Если return type совпадает с самим типом (Self), используем mono тип
        if ret_c == format!("Nova_{}*", base_name) {
            // Возвращается Self → нужен конкретный mono тип
            let mangled = Self::compute_generic_type_c_name(base_name, &type_args);
            return Some(format!("{}*", mangled));
        }
        Some(ret_c)
    }

    /// Plan 14 std-fix: возвращаемый C-тип для встроенных методов str.
    /// Используется в `infer_expr_c_type` для Call с `Member`-func, чтобы
    /// `s.starts_with(...)`/`s.contains(...)` (и т.д.) корректно
    /// инфер'ились как `nova_bool` для strict bool-check'а в `if`.
    fn str_method_ret_type(method: &str) -> Option<&'static str> {
        match method {
            "starts_with" | "ends_with" | "contains" | "eq"
            | "lt" | "le" | "gt" | "ge"
                => Some("nova_bool"),
            "to_upper" | "to_lower" | "trim" | "slice" | "concat"
                => Some("nova_str"),
            "char_len" | "byte_len" | "hash"
                => Some("nova_int"),
            "find" | "rfind"
                => Some("NovaOpt_nova_int"),
            // Iter[T] / NovaArray возврат — пока не критично для bool-check.
            _ => None,
        }
    }

    /// Map a Nova str method name to a nova_rt C function name.
    fn str_method_to_rt(method: &str) -> Option<&'static str> {
        match method {
            "starts_with" => Some("nova_str_starts_with"),
            "ends_with"   => Some("nova_str_ends_with"),
            "contains"    => Some("nova_str_contains"),
            "to_upper"    => Some("nova_str_to_upper"),
            "to_lower"    => Some("nova_str_to_lower"),
            "trim"        => Some("nova_str_trim"),
            "slice"       => Some("nova_str_slice"),
            "concat"      => Some("nova_str_concat"),
            "eq"          => Some("nova_str_eq"),
            "lt"          => Some("nova_str_lt"),
            "le"          => Some("nova_str_le"),
            "gt"          => Some("nova_str_gt"),
            "ge"          => Some("nova_str_ge"),
            "hash"        => Some("nova_str_hash"),
            "find"        => Some("nova_str_find"),
            "rfind"       => Some("nova_str_rfind"),
            "len"         => Some("nova_str_char_len"),   // Plan 55 Ф.6: s.len() == s.char_len() (D26).
            "char_len"    => Some("nova_str_char_len"),
            "byte_len"    => Some("nova_str_byte_len"),
            "bytes"       => Some("nova_str_bytes"),
            "chars"       => Some("nova_str_chars"),
            "char_at"     => Some("nova_str_char_at"),
            "split"       => Some("nova_str_split"),
            _ => None,
        }
    }

    /// Из C-типа `Nova_Foo*` (или `Nova_Foo`) извлечь struct name `Foo`.
    /// Для не-Nova_-типов возвращает None.
    fn struct_name_from_c_type(c_ty: &str) -> Option<String> {
        let trimmed = c_ty.trim_end_matches('*').trim();
        trimmed.strip_prefix("Nova_").map(|s| s.to_string())
    }

    /// Plan 14 Ф.6 (D69): возвращает `regular_arity` (число
    /// non-variadic параметров) если вызываемая fn variadic,
    /// иначе None. Поддерживает:
    ///   - top-level user fn (по `user_fn_variadic` / `user_fn_sigs`);
    ///   - method calls (по `method_overloads` → `MethodSig.variadic_last`).
    /// Если variadic_last == true, regular_arity = total - 1.
    /// Generic fn'ы / dynamically-resolved closure calls пропускаются.
    fn lookup_variadic_arity(&self, func: &Expr) -> Option<usize> {
        // 1. Top-level user fn: `name(...)` где name — известный variadic.
        if let ExprKind::Ident(name) = &func.kind {
            if self.suppress_variadic_routing { return None; }
            if self.user_fn_variadic.contains(name) {
                let total = self.user_fn_sigs.get(name).map(|(p, _)| p.len())?;
                return Some(total.saturating_sub(1));
            }
        }
        // 2. Method call: `obj.method(...)` или `Type.method(...)`.
        if self.suppress_variadic_routing { return None; }
        let recv_method: Option<(String, String)> = match &func.kind {
            ExprKind::Path(parts) if parts.len() == 2 => {
                Some((parts[0].clone(), parts[1].clone()))
            }
            ExprKind::Member { obj, name } => {
                match &obj.kind {
                    ExprKind::Ident(n) if self.method_overloads.keys().any(|(t, _)| t == n) => {
                        Some((n.clone(), name.clone()))
                    }
                    _ => {
                        let obj_ty = self.infer_expr_c_type(obj);
                        let trimmed = obj_ty.trim_start_matches("Nova_")
                            .trim_end_matches('*').trim().to_string();
                        if !trimmed.is_empty() && trimmed != "void" {
                            Some((trimmed, name.clone()))
                        } else {
                            None
                        }
                    }
                }
            }
            _ => None,
        };
        if let Some((rt, mn)) = recv_method {
            if let Some(sigs) = self.method_overloads.get(&(rt, mn)) {
                // Variadic — single-overload (multiple variadic overloads
                // ambiguous; в MVP не поддерживаем). Берём первый.
                if let Some(sig) = sigs.first() {
                    if sig.variadic_last {
                        // Receiver method: param_c_types НЕ включает self.
                        // Для instance method: regular_arity = param_c_types.len() - 1.
                        // Для static method: тоже param_c_types.len() - 1.
                        return Some(sig.param_c_types.len().saturating_sub(1));
                    }
                }
            }
        }
        None
    }

    /// Plan 14 Ф.1: sanitize C-type для использования в identifier
    /// `NovaOpt_<sanitized>` / `_NovaTuple<...>` / etc.
    ///
    /// `*` → `_p`, ` ` → `_`, `[` → `_arr_`, `]` → empty.
    fn sanitize_for_novaopt(c_ty: &str) -> String {
        c_ty.replace('*', "_p")
            .replace(' ', "_")
            .replace('[', "_arr_")
            .replace(']', "")
    }

    /// Plan 14 Ф.1: register typedef NovaOpt_<sanitized> { int tag; <c_ty> value; }
    ///
    /// Idempotent — каждый `sanitized` эмитится один раз. Pre-decl'нутые
    /// в runtime (`nova_int / nova_byte / nova_bool / nova_str / nova_f64`)
    /// помечены как seen в `new()` и пропускаются.
    ///
    /// Order — registration (insertion) order. recursive type_ref_to_c
    /// registers innermost types раньше outer'а, что даёт правильный
    /// topological order: `NovaOpt_X` стоит до `NovaOpt_NovaOpt_X` в
    /// файле (последний зависит от первого).
    fn register_novaopt_decl(&self, sanitized: &str, c_ty: &str) {
        let mut seen = self.novaopt_decls_seen.borrow_mut();
        if seen.contains(sanitized) { return; }
        seen.insert(sanitized.to_string());
        // Plan 54 Ф.9: запомнить реальный c_ty для recovery в
        // pattern_bind_typed (sanitized id ≠ c_ty для pointer types).
        self.novaopt_value_types.borrow_mut()
            .insert(sanitized.to_string(), c_ty.to_string());
        let line = format!(
            "typedef struct NovaOpt_{} {{ int tag; {} value; }} NovaOpt_{};\n",
            sanitized, c_ty, sanitized);
        self.novaopt_typedefs_buf.borrow_mut().push_str(&line);

        // Plan 39 Issue A: auto-generate `nova_opt_eq_<sanitized>` helper.
        // Без него `r == None` где `r: NovaOpt_<T>` падает с undefined symbol.
        // Сравнение: по tag. Если tag одинаковый — None: всегда equal; Some:
        // сравниваем value. Для pointer-types — pointer equality (как Rust для
        // `&T`). Для value-структур — memcmp (как для tuples). Для scalar —
        // плоское `==`.
        let is_pointer = c_ty.ends_with('*');
        let is_scalar = matches!(c_ty, "nova_int" | "nova_bool" | "nova_byte"
            | "nova_char" | "nova_i8" | "nova_i16" | "nova_i32" | "nova_i64"
            | "nova_u8" | "nova_u16" | "nova_u32" | "nova_u64"
            | "nova_f32" | "nova_f64");
        let cmp_body = if is_scalar || is_pointer {
            "a.value == b.value".to_string()
        } else {
            format!("memcmp(&a.value, &b.value, sizeof({})) == 0", c_ty)
        };
        let eq_fn = format!(
            "static inline nova_bool nova_opt_eq_{sani}(NovaOpt_{sani} a, NovaOpt_{sani} b) {{\n\
             \x20   if (a.tag != b.tag) return 0;\n\
             \x20   if (a.tag == NOVA_TAG_Option_None) return 1;\n\
             \x20   return {body};\n\
             }}\n",
            sani = sanitized, body = cmp_body);
        self.novaopt_typedefs_buf.borrow_mut().push_str(&eq_fn);

        // Plan 39 Issue A: auto-gen Option methods is_some / is_none /
        // unwrap_or — для скаляров эти helper'ы есть в array.h (nova_int,
        // nova_str). Для остальных генерируем здесь чтобы codegen мог
        // вызывать `.is_some()` / `.is_none()` / `.unwrap_or(default)`.
        if sanitized != "nova_int" && sanitized != "nova_str" {
            let methods = format!(
                "static inline nova_bool Nova_Option_method_is_some_{sani}(NovaOpt_{sani} o) {{ return o.tag == NOVA_TAG_Option_Some; }}\n\
                 static inline nova_bool Nova_Option_method_is_none_{sani}(NovaOpt_{sani} o) {{ return o.tag == NOVA_TAG_Option_None; }}\n\
                 static inline {cty} Nova_Option_method_unwrap_or_{sani}(NovaOpt_{sani} o, {cty} default_v) {{ return o.tag == NOVA_TAG_Option_Some ? o.value : default_v; }}\n",
                sani = sanitized, cty = c_ty);
            self.novaopt_typedefs_buf.borrow_mut().push_str(&methods);
        }
    }


    /// Plan 08 Ф.5: as-cast restrictions для char/byte/bool.
    /// По D54 запрещены конверсии где как-cast даёт неочевидную или
    /// небезопасную семантику. Для них — compile error с suggestion'ом
    /// использовать `try_from` или explicit comparison.
    ///
    /// Conservative: если src не определён (void*) или target не в
    /// special-cases — пропускаем (legacy backward-compat).
    fn check_as_cast_allowed(
        src_nova: &str,
        tgt_nova: &str,
        inner_kind: &ExprKind,
    ) -> Result<(), String> {
        // Спецслучай: CharLit. inner это литерал 'A'/'B'/etc — он уже
        // имеет nova_int представление, но семантически это char.
        // **Char-literals разрешены к as-cast в любой numeric** —
        // программист видит codepoint буквально, range-check не нужен.
        // `'A' as byte`, `'A' as int`, `'A' as u8` — все OK.
        if matches!(inner_kind, ExprKind::CharLit(_)) {
            return Ok(());
        }
        // Plan 14 Ф.7: IntLit → char для compile-time-known литералов.
        // По D54 `int as char` запрещён (suggested `char.try_from(n)?`),
        // но для **литерала** в валидном Unicode-диапазоне range-check
        // тривиален и checker может его выполнить статически:
        //   - n ∈ [0, 0x10FFFF]
        //   - n ∉ [0xD800, 0xDFFF] (surrogate range — invalid scalar)
        // Off-range литерал → compile error с точным сообщением (вместо
        // generic «use try_from»).
        if let ExprKind::IntLit(n) = *inner_kind {
            if tgt_nova == "char" {
                if n < 0 || n > 0x10FFFF {
                    return Err(format!(
                        "`as`-cast `{} as char` запрещён: codepoint 0x{:X} \
                        вне диапазона U+0..=U+10FFFF.",
                        n, n
                    ));
                }
                if (0xD800..=0xDFFF).contains(&n) {
                    return Err(format!(
                        "`as`-cast `{} as char` запрещён: codepoint U+{:04X} \
                        в surrogate range (U+D800..=U+DFFF) — не valid Unicode scalar.",
                        n, n
                    ));
                }
                return Ok(());
            }
        }
        let src = src_nova;

        // Запрещённые пары:
        let banned: &[(&str, &str, &str)] = &[
            // (src, tgt, suggestion)
            ("int",  "char", "use `char.try_from(n)?` (range-checked, returns Result[char, _])"),
            ("i32",  "char", "use `char.try_from(n)?`"),
            ("i64",  "char", "use `char.try_from(n)?`"),
            ("u32",  "char", "use `char.try_from(n)?`"),
            ("u64",  "char", "use `char.try_from(n)?`"),
            ("char", "byte", "use `byte.try_from(c)?` (fails if codepoint > 0xFF)"),
            ("int",  "bool", "use explicit comparison (`n != 0` for truthy-int)"),
            ("i8",   "bool", "use `n != 0`"),
            ("i16",  "bool", "use `n != 0`"),
            ("i32",  "bool", "use `n != 0`"),
            ("i64",  "bool", "use `n != 0`"),
            ("u8",   "bool", "use `n != 0`"),
            ("u16",  "bool", "use `n != 0`"),
            ("u32",  "bool", "use `n != 0`"),
            ("u64",  "bool", "use `n != 0`"),
            ("byte", "bool", "use `n != 0`"),
            ("f64",  "bool", "use `f != 0.0`"),
            ("f32",  "bool", "use `f != 0.0`"),
            ("str",  "int",  "use `int.try_from(s)?` (parses decimal)"),
            ("str",  "i32",  "use `i32.try_from(s)?`"),
            ("str",  "f64",  "use `f64.try_from(s)?`"),
            ("str",  "bool", "use `bool.try_from(s)?`"),
            ("int",  "str",  "use `str.from(n)`"),
            ("f64",  "str",  "use `str.from(f)`"),
            ("bool", "str",  "use `str.from(b)`"),
            ("char", "str",  "use `str.from(c)` (UTF-8 encode)"),
        ];
        for (s, t, hint) in banned {
            if &src == s && &tgt_nova == t {
                return Err(format!(
                    "`as`-cast `{} as {}` запрещён: {}.",
                    src, tgt_nova, hint
                ));
            }
        }
        Ok(())
    }

    /// Plan 08 Ф.4: strict bool-check для `if cond` / `while cond`.
    /// Возвращает Err если `cond_ty` ОЧЕВИДНО non-bool (numeric/string/...).
    /// Type-neutral (`void*`, unknown) — пропускаем (conservative).
    fn check_bool_condition(cond_ty: &str, ctx: &str) -> Result<(), String> {
        let definitely_non_bool = matches!(cond_ty,
            "nova_int" | "nova_f64" | "nova_f32" | "nova_str" | "nova_byte"
            | "int8_t" | "int16_t" | "int32_t" | "int64_t"
            | "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t");
        if definitely_non_bool {
            return Err(format!(
                "{} condition must be `bool`, got `{}`. \
                Hint: use explicit comparison (e.g. `n != 0` for truthy-int).",
                ctx, cond_ty));
        }
        Ok(())
    }

    /// Plan 14 std-fix: версия с указанием места (line:col) в условии.
    /// Использует `annotation_source` (set_source_for_annotations).
    fn check_bool_condition_at(&self, cond_ty: &str, ctx: &str, span: crate::diag::Span) -> Result<(), String> {
        if let Err(msg) = Self::check_bool_condition(cond_ty, ctx) {
            if let Some(src) = &self.annotation_source {
                let (line, col) = crate::diag::byte_to_line_col(src, span.start);
                return Err(format!("{}:{}: {}", line, col, msg));
            }
            return Err(msg);
        }
        Ok(())
    }

    /// Plan 08 Ф.3: convert C-type back to Nova-type name для lookup'ов в
    /// from_targets / try_from_targets (которые хранят Nova-имена).
    /// `nova_int` → `int`, `nova_str` → `str`, `Nova_Wrapper*` → `Wrapper`.
    /// Числовые primitive C-aliases (`int32_t` etc.) → соответствующее Nova-имя.
    fn nova_type_name_from_c(c_ty: &str) -> String {
        let trimmed = c_ty.trim_end_matches('*').trim();
        match trimmed {
            "nova_int"  => "int".into(),
            "nova_f64"  => "f64".into(),
            "nova_f32"  => "f32".into(),
            "nova_bool" => "bool".into(),
            "nova_str"  => "str".into(),
            "nova_byte" => "byte".into(),
            "int8_t"    => "i8".into(),
            "int16_t"   => "i16".into(),
            "int32_t"   => "i32".into(),
            "int64_t"   => "i64".into(),
            "uint8_t"   => "u8".into(),
            "uint16_t"  => "u16".into(),
            "uint32_t"  => "u32".into(),
            "uint64_t"  => "u64".into(),
            other => other.strip_prefix("Nova_").unwrap_or(other).to_string(),
        }
    }

    /// Returns true для C-целочисленных типов, явно несущих ширину/знак
    /// (uint8_t..uint64_t, int8_t..int32_t). `nova_int` (= int64_t) сюда
    /// **не** входит — это дефолтный тип IntLit'а, и его роль как раз
    /// в том, чтобы быть «promotable» к более типизированному операнду.
    fn is_typed_integer(ty: &str) -> bool {
        matches!(ty,
            "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t" |
            "int8_t" | "int16_t" | "int32_t"
        )
    }

    /// Plan 38: numeric type constants — `int.MAX` / `f64.NAN` / etc.
    /// Returns `Some((c_expression, c_type))` если path = primitive
    /// type constant, иначе `None`. C-expression готов к emit'у напрямую.
    ///
    /// Mapping table из spec D26 (08-runtime.md).
    fn numeric_type_constant_mapping(parts: &[String]) -> Option<(&'static str, &'static str)> {
        if parts.len() != 2 {
            return None;
        }
        let ty = parts[0].as_str();
        let name = parts[1].as_str();
        let mapping: &[(&str, &str, &str, &str)] = &[
            // (nova_type, const_name, c_expr, c_type)
            //
            // Signed integers
            ("int",  "MAX", "((nova_int)INT64_MAX)", "nova_int"),
            ("int",  "MIN", "((nova_int)INT64_MIN)", "nova_int"),
            ("i64",  "MAX", "((nova_int)INT64_MAX)", "nova_int"),
            ("i64",  "MIN", "((nova_int)INT64_MIN)", "nova_int"),
            ("i32",  "MAX", "INT32_MAX",             "int32_t"),
            ("i32",  "MIN", "INT32_MIN",             "int32_t"),
            ("i16",  "MAX", "INT16_MAX",             "int16_t"),
            ("i16",  "MIN", "INT16_MIN",             "int16_t"),
            ("i8",   "MAX", "INT8_MAX",              "int8_t"),
            ("i8",   "MIN", "INT8_MIN",              "int8_t"),
            // Unsigned integers
            ("u64",  "MAX", "UINT64_MAX",            "uint64_t"),
            ("u32",  "MAX", "UINT32_MAX",            "uint32_t"),
            ("u16",  "MAX", "UINT16_MAX",            "uint16_t"),
            ("u8",   "MAX", "UINT8_MAX",             "uint8_t"),
            ("byte", "MAX", "((nova_byte)UINT8_MAX)", "nova_byte"),
            // Char (codepoint)
            ("char", "MAX", "((nova_int)0x10FFFFLL)", "nova_int"),
            ("char", "MIN", "((nova_int)0LL)",        "nova_int"),
            // Float
            ("f64",  "MAX",          "DBL_MAX",                          "nova_f64"),
            ("f64",  "MIN_POSITIVE", "DBL_MIN",                          "nova_f64"),
            ("f64",  "EPSILON",      "DBL_EPSILON",                      "nova_f64"),
            ("f64",  "NAN",          "((double)NAN)",                    "nova_f64"),
            ("f64",  "INFINITY",     "((double)INFINITY)",               "nova_f64"),
            ("f64",  "NEG_INFINITY", "((double)(-INFINITY))",            "nova_f64"),
            ("f64",  "PI",           "3.14159265358979323846",           "nova_f64"),
            ("f64",  "E",            "2.71828182845904523536",           "nova_f64"),
            ("f32",  "MAX",          "FLT_MAX",                          "nova_f32"),
            ("f32",  "MIN_POSITIVE", "FLT_MIN",                          "nova_f32"),
            ("f32",  "EPSILON",      "FLT_EPSILON",                      "nova_f32"),
            ("f32",  "NAN",          "((float)NAN)",                     "nova_f32"),
            ("f32",  "INFINITY",     "((float)INFINITY)",                "nova_f32"),
            ("f32",  "NEG_INFINITY", "((float)(-INFINITY))",             "nova_f32"),
            ("f32",  "PI",           "3.14159265358979323846f",          "nova_f32"),
            ("f32",  "E",            "2.71828182845904523536f",          "nova_f32"),
        ];
        for (t, n, expr, c_ty) in mapping {
            if *t == ty && *n == name {
                return Some((expr, c_ty));
            }
        }
        None
    }

    /// Returns true for C types that are passed by value (use `.` accessor, not `->`).
    fn is_value_type(ty: &str) -> bool {
        if ty.starts_with("_NovaTuple") && !ty.ends_with('*') {
            return true;
        }
        matches!(ty,
            "nova_int" | "nova_f64" | "nova_f32" | "nova_bool" |
            "nova_str" | "nova_unit" | "nova_byte" |
            "int32_t" | "int16_t" | "int8_t" |
            "uint64_t" | "uint32_t" | "uint16_t" | "uint8_t" |
            "Nova_ChannelPair"
        )
    }

    /// Plan 20 follow-up: infer return type для lambda body, временно
    /// регистрируя params в `var_types` чтобы `infer_expr_c_type` мог найти
    /// их. Делает restore после.
    ///
    /// Используется когда у lambda нет явной annotation/context для return
    /// type — fallback на infer из тела. Поддерживает естественный паттерн
    /// `|| { side_effect() }` который должен вернуть `nova_unit`, а не
    /// hardcoded `nova_int` default.
    fn infer_lambda_return_type_with_params(
        &mut self,
        body: &Expr,
        params: &[LambdaParam],
        param_c_tys: &[String],
    ) -> String {
        let saved: Vec<(String, Option<String>)> = params.iter().zip(param_c_tys)
            .map(|(p, ty)| (p.name.clone(), self.var_types.insert(p.name.clone(), ty.clone())))
            .collect();
        let inferred = self.infer_expr_c_type(body);
        // Restore.
        for (name, prev) in saved {
            match prev {
                Some(old) => { self.var_types.insert(name, old); }
                None => { self.var_types.remove(&name); }
            }
        }
        if inferred.is_empty() {
            "nova_int".into()
        } else {
            inferred
        }
    }

    fn infer_expr_c_type(&self, expr: &Expr) -> String {
        // D38 turbofish: type_args не меняют c-тип; делегируем в base.
        if let ExprKind::TurboFish { base, .. } = &expr.kind {
            return self.infer_expr_c_type(base);
        }
        // Plan 38: numeric type constants — `int.MAX` etc.
        if let ExprKind::Path(parts) = &expr.kind {
            if let Some((_, c_ty)) = Self::numeric_type_constant_mapping(parts) {
                return c_ty.to_string();
            }
        }
        match &expr.kind {
            ExprKind::IntLit(_) => "nova_int".into(),
            ExprKind::CharLit(_) => "nova_int".into(),
            ExprKind::FloatLit(_) => "nova_f64".into(),
            ExprKind::BoolLit(_) => "nova_bool".into(),
            ExprKind::StrLit(_) => "nova_str".into(),
            ExprKind::InterpolatedStr { .. } => "nova_str".into(),
            ExprKind::UnitLit => "nova_unit".into(),
            ExprKind::TupleLit(elems) => format!("_NovaTuple{}", elems.len()),
            ExprKind::Binary { op, left, right } => match op {
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Le
                | BinOp::Gt | BinOp::Ge | BinOp::And | BinOp::Or
                | BinOp::Implies | BinOp::Iff => "nova_bool".into(),
                _ => {
                    let lt = self.infer_expr_c_type(left);
                    let rt = self.infer_expr_c_type(right);
                    // f64 побеждает: float-арифметика — результат f64.
                    if lt == "nova_f64" || rt == "nova_f64" {
                        return "nova_f64".into();
                    }
                    // Integer promotion: если один из операндов — typed
                    // integer (u8/u16/u32/u64/i8/i16/i32), а другой —
                    // `nova_int` (дефолт IntLit или int64-context), типизированный
                    // выигрывает. Это исправляет случаи вроде
                    //   let c u32 = ...; 0xEDB88320 ^ c >> 1
                    // где IntLit давал nova_int, и весь XOR терял u32.
                    // Симметрично для left/right. Если оба typed — берём lt
                    // (левый); правильный C-promotion разрешит cast'ами на
                    // emit_assign_typed.
                    let lt_is_typed_int = Self::is_typed_integer(&lt);
                    let rt_is_typed_int = Self::is_typed_integer(&rt);
                    if lt_is_typed_int && rt == "nova_int" {
                        return lt;
                    }
                    if rt_is_typed_int && lt == "nova_int" {
                        return rt;
                    }
                    lt
                }
            },
            // Plan 08 Ф.4 prerequisite: правильный infer для unary
            // (нужен strict bool-check). `!x` всегда даёт bool;
            // `-x` сохраняет тип operand'а.
            ExprKind::Unary { op, operand } => match op {
                UnOp::Not => "nova_bool".into(),
                UnOp::Neg => self.infer_expr_c_type(operand),
            },
            // Plan 08 Ф.4: Block — тип trailing expression.
            // Plan 52 Ф.16: enhancement — если trailing это Ident, и в
            // block.stmts есть `let <ident> [: T] = ...`, берём тип из
            // `ty`-аннотации (если есть) или из value-expression. Это
            // позволяет desugar'у map-литерала (без аннотации внешнего
            // let) дать корректный type-hint для outer-let через
            // typed-rebinding `let _mN_typed HashMap[K,V] = _mN; _mN_typed`.
            ExprKind::Block(b) => {
                if let Some(t) = &b.trailing {
                    // Если trailing — Ident, ищем последний let с тем же именем.
                    if let ExprKind::Ident(name) = &t.kind {
                        for s in b.stmts.iter().rev() {
                            if let crate::ast::Stmt::Let(d) = s {
                                if let crate::ast::Pattern::Ident { name: bn, .. } = &d.pattern {
                                    if bn == name {
                                        if let Some(ty) = &d.ty {
                                            // Используем ty-аннотацию (Plan 52 Ф.16).
                                            if let Ok(c) = self.type_ref_to_c(ty) {
                                                return c;
                                            }
                                        }
                                        // Fallback: infer из value-expr.
                                        return self.infer_expr_c_type(&d.value);
                                    }
                                }
                            }
                        }
                    }
                    self.infer_expr_c_type(t)
                } else {
                    "nova_unit".into()
                }
            }
            ExprKind::RecordLit { type_name: Some(name), fields, .. } => {
                let raw_name = name.join("_");
                let struct_name = if raw_name == "Self" {
                    self.current_receiver_type.clone().unwrap_or(raw_name)
                } else { raw_name };
                // Check if this is a sum-type record variant
                if let Some((sum_type_name, _)) = self.find_variant(&struct_name) {
                    format!("Nova_{}*", sum_type_name)
                } else if self.generic_types.contains(&struct_name) {
                    // Generic type: compute concrete mono name from field values.
                    // Check BEFORE record_schemas because record_schemas has the erased form
                    // (with void* fields) for generic types — we want the concrete mono form.
                    if let Some(template) = self.generic_type_templates.get(&struct_name) {
                        use crate::ast::TypeDeclKind;
                        let mut type_args_c: Vec<String> = template.generics.iter()
                            .map(|_| "nova_int".to_string())
                            .collect();
                        if let TypeDeclKind::Record(field_decls) = &template.kind {
                            for (i, g) in template.generics.iter().enumerate() {
                                for f_decl in field_decls {
                                    if let crate::ast::TypeRef::Named { path, generics: fgens, .. } = &f_decl.ty {
                                        if fgens.is_empty() && path.join("_") == g.name {
                                            if let Some(field) = fields.iter().find(|f| f.name == f_decl.name) {
                                                if let Some(v) = &field.value {
                                                    let c_ty = self.infer_expr_c_type(v);
                                                    if !c_ty.is_empty() && c_ty != "void*" {
                                                        type_args_c[i] = c_ty;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        let mangled = Self::compute_generic_type_c_name(&struct_name, &type_args_c);
                        format!("{}*", mangled)
                    } else {
                        "void*".into()
                    }
                } else if self.record_schemas.contains_key(&struct_name) {
                    format!("Nova_{}*", struct_name)
                } else {
                    "void*".into()
                }
            }
            ExprKind::RecordLit { type_name: None, fields, .. } => {
                // Anonymous record with spread — infer type from first spread source
                for f in fields {
                    if f.is_spread {
                        if let Some(v) = &f.value {
                            return self.infer_expr_c_type(v);
                        }
                    }
                }
                "nova_int".into()
            }
            ExprKind::Ident(name) => {
                if let Some(ty) = self.var_types.get(name) {
                    return ty.clone();
                }
                // Check if it's a unit variant (e.g. None, Err, Ok used as value)
                if let Some((type_name, fields)) = self.find_variant(name) {
                    if fields.is_empty() {
                        if type_name == "Option" || type_name == "NovaOpt_nova_int" {
                            // Plan 14 Ф.1: None infer'ится по контексту
                            // current_fn_return_ty (если NovaOpt_<X>),
                            // иначе legacy NovaOpt_nova_int.
                            if name == "None" {
                                if let Some(t) = self.current_fn_return_ty.as_ref() {
                                    if t.starts_with("NovaOpt_") {
                                        return t.clone();
                                    }
                                }
                            }
                            return "NovaOpt_nova_int".into();
                        }
                        return format!("Nova_{}*", type_name);
                    }
                }
                "nova_int".into()
            }
            ExprKind::Index { obj, .. } => {
                // arr[i] → element type of arr.
                // Check array_element_types first (pointer-stomped elements override).
                if let ExprKind::Ident(name) = &obj.kind {
                    if let Some(et) = self.array_element_types.get(name) {
                        return et.clone();
                    }
                }
                // @field access (nova_self->field) — check by synthesized C-expression key.
                if let ExprKind::Member { obj: inner, name: field } = &obj.kind {
                    if matches!(inner.kind, ExprKind::SelfAccess) {
                        let key = format!("(nova_self->{})", Self::mangle_field_name(field));
                        if let Some(et) = self.array_element_types.get(&key) {
                            return et.clone();
                        }
                    }
                }
                // Если obj — NovaArray_T*, элемент имеет тип T (из имени).
                let obj_ty = self.infer_expr_c_type(obj);
                if let Some(elem) = obj_ty.strip_prefix("NovaArray_") {
                    let elem = elem.trim_end_matches('*').trim();
                    return elem.to_string();
                }
                "nova_int".into()
            }
            ExprKind::SelfAccess => {
                self.var_types.get("nova_self").cloned().unwrap_or_else(|| "nova_int".into())
            }
            ExprKind::HandlerLit { effect_name, .. } => {
                // handler Switch { ... } has type NovaVtable_Switch*
                format!("NovaVtable_{}*", effect_name.join("_"))
            }
            ExprKind::Call { func, args, .. } => {
                // D38 turbofish прозрачен для inference — но extract type_args
                // ПЕРЕД unwrap чтобы Plan 54 Ф.4 return-type inference (для
                // generic-fn возвращающей []T) могла использовать turbofish
                // как Source 1.
                let turbofish_args: Vec<crate::ast::TypeRef> =
                    if let ExprKind::TurboFish { type_args, .. } = &func.kind {
                        type_args.clone()
                    } else { vec![] };
                let func = func.unwrap_turbofish();
                // D109: TurboFish member call on generic type, e.g. HashMap[str,int].new().
                // func = Member { obj: TurboFish(Ident("HashMap"), [str,int]), name: "new" }.
                // infer_expr_c_type(TurboFish→Ident("HashMap")) = "nova_int" (wrong).
                // Detect this BEFORE recv_and_method to return the concrete return type.
                if let ExprKind::Member { obj, name: method_name } = &func.kind {
                    if let ExprKind::TurboFish { base, type_args } = &obj.kind {
                        if let ExprKind::Ident(type_name) = &base.kind {
                            if self.generic_types.contains(type_name.as_str()) {
                                let type_args_c: Vec<String> = type_args.iter()
                                    .map(|tr| {
                                        // In a monomorphized context, type params (K, V, etc.)
                                        // are stored in current_type_subst as C-type strings.
                                        // simple_type_ref_to_c is static and can't see them.
                                        if let crate::ast::TypeRef::Named { path, generics, .. } = tr {
                                            if generics.is_empty() && path.len() == 1 {
                                                if let Some(c) = self.current_type_subst.get(&path[0]) {
                                                    return c.clone();
                                                }
                                            }
                                        }
                                        Self::simple_type_ref_to_c(tr)
                                    })
                                    .collect();
                                let mangled = Self::compute_generic_type_c_name(type_name, &type_args_c);
                                let concrete_type = format!("{}*", mangled);
                                if let Some(tmpl) = self.generic_type_templates.get(type_name) {
                                    let type_subst: Vec<(String, Option<String>)> = tmpl.generics.iter()
                                        .zip(type_args_c.iter())
                                        .map(|(g, c)| (g.name.clone(), Some(c.clone())))
                                        .collect();
                                    if let Some(method_ret) = self.generic_type_methods.get(type_name)
                                        .and_then(|ms| ms.iter().find(|m| m.name == *method_name))
                                        .and_then(|fd| fd.return_type.as_ref()
                                            .and_then(|rt| Self::apply_type_subst_to_ref(rt, &type_subst)))
                                    {
                                        if !method_ret.is_empty() && method_ret != "void*" {
                                            return if method_ret == format!("Nova_{}*", type_name) {
                                                concrete_type
                                            } else {
                                                method_ret
                                            };
                                        }
                                    }
                                    return concrete_type;
                                }
                                return concrete_type;
                            }
                        }
                    }
                }
                // Plan 11 Ф.1-Ф.3: multi-overload infer. Если func — Path/Member
                // call на known receiver-type, ищем в method_overloads. Это
                // решает single-key last-wins для одноимённых методов.
                {
                    let recv_and_method: Option<(String, String, bool)> = match &func.kind {
                        ExprKind::Path(parts) => {
                            // Self → current_receiver_type
                            let parts: Vec<String> = if !parts.is_empty() && parts[0] == "Self" {
                                if let Some(r) = &self.current_receiver_type {
                                    let mut p = parts.clone();
                                    p[0] = r.clone();
                                    p
                                } else {
                                    parts.clone()
                                }
                            } else {
                                parts.clone()
                            };
                            if parts.len() == 2 {
                                Some((parts[0].clone(), parts[1].clone(), false))
                            } else {
                                None
                            }
                        }
                        ExprKind::Member { obj, name } => {
                            // Static (obj=Ident("T")) or instance.
                            match &obj.kind {
                                ExprKind::Ident(n) if n == "Self" => {
                                    if let Some(r) = &self.current_receiver_type {
                                        Some((r.clone(), name.clone(), false))
                                    } else {
                                        None
                                    }
                                }
                                ExprKind::Ident(n) if self.method_overloads.keys().any(|(t, _)| t == n) => {
                                    Some((n.clone(), name.clone(), false))
                                }
                                _ => {
                                    let obj_ty = self.infer_expr_c_type(obj);
                                    let trimmed = obj_ty.trim_start_matches("Nova_")
                                        .trim_end_matches('*').trim().to_string();
                                    if !trimmed.is_empty() && trimmed != "void" {
                                        Some((trimmed, name.clone(), true))
                                    } else {
                                        None
                                    }
                                }
                            }
                        }
                        _ => None,
                    };
                    if let Some((rt, mn, want_inst)) = recv_and_method {
                        let key = (rt.clone(), mn.clone());
                        if let Some(overloads) = self.method_overloads.get(&key) {
                            let candidates: Vec<&MethodSig> = overloads.iter()
                                .filter(|s| s.is_instance == want_inst)
                                .collect();
                            if !candidates.is_empty() {
                                // Plan 48 Ф.7.1: sentinel — generic method с
                                // собственными type params. return_c_type у sentinel
                                // = "void*"; нужно резолвить через mono inference.
                                if candidates.iter().any(|c| c.c_name.starts_with("__mono_method__")) {
                                    let recv_key = (rt.clone(), mn.clone());
                                    if let Some(fn_decl) = self.mono_method_decls.get(&recv_key) {
                                        if let Ok(type_subst) = self.resolve_mono_type_args(fn_decl, &[], args) {
                                            let subst_opt: Vec<(String, Option<String>)> = type_subst.iter()
                                                .map(|(n, t)| (n.clone(), Some(t.clone()))).collect();
                                            if let Some(ret_ty) = &fn_decl.return_type {
                                                if let Some(c_ty) = Self::apply_type_subst_to_ref(ret_ty, &subst_opt) {
                                                    if !c_ty.is_empty() && c_ty != "void*" {
                                                        return c_ty;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    // Sentinel + не смогли резолвить → fallback на void*
                                    // (call site эмитит mono call, но тип не известен —
                                    // user должен дать explicit annotation).
                                    return "void*".into();
                                }
                                // Plan 11 Ф.9.3: override-precedence Own > Delegated.
                                // Single → return its return_c_type (no override
                                // conflict possible).
                                if candidates.len() == 1 {
                                    return candidates[0].return_c_type.clone();
                                }
                                // Multi → strict match по arg-types.
                                let arg_types: Vec<String> = args.iter()
                                    .map(|a| self.infer_expr_c_type(a.expr()))
                                    .collect();
                                let strict: Vec<&MethodSig> = candidates.iter().copied()
                                    .filter(|s| s.param_c_types.len() == arg_types.len())
                                    .filter(|s| s.param_c_types.iter().zip(arg_types.iter())
                                        .all(|(w, g)| w == g))
                                    .collect();
                                let pool: Vec<&MethodSig> = {
                                    let owns: Vec<&MethodSig> = strict.iter()
                                        .filter(|s| !s.is_delegated)
                                        .copied().collect();
                                    if !owns.is_empty() { owns } else { strict }
                                };
                                if let Some(sig) = pool.first() {
                                    return sig.return_c_type.clone();
                                }
                                // 0 matches — fallback на старую логику ниже.
                            }
                        }
                        // Ф.3: fallback for generic type instance methods.
                        // method_overloads is keyed by base type ("Stack"), but rt here is
                        // the concrete instance ("Stack____nova_str"). Look up return type
                        // from generic_type_methods + template substitution.
                        {
                            let info = self.generic_type_instance_info.borrow();
                            let instance_opt = info.get(&format!("Nova_{}", rt)).cloned();
                            drop(info);
                            if let Some((base_name, type_args_c)) = instance_opt {
                                if let Some(tmpl) = self.generic_type_templates.get(&base_name) {
                                    let subst: Vec<(String, Option<String>)> = tmpl.generics.iter()
                                        .zip(type_args_c.iter())
                                        .map(|(g, c)| (g.name.clone(), Some(c.clone())))
                                        .collect();
                                    if let Some(method_decl) = self.generic_type_methods.get(&base_name)
                                        .and_then(|ms| ms.iter().find(|m| m.name == mn))
                                    {
                                        if let Some(ret_ty) = &method_decl.return_type {
                                            if let Some(c_ty) = Self::apply_type_subst_to_ref(ret_ty, &subst) {
                                                if !c_ty.is_empty() && c_ty != "void*" {
                                                    return c_ty;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // Infer return type for call expressions
                if let ExprKind::Ident(name) = &func.kind {
                    if name == "println" || name == "print" || name == "assert" || name == "debug_assert" {
                        return "nova_unit".into();
                    }
                    // Variant constructor call: Some(x), None, etc. → return option/sum type
                    if let Some((type_name, _)) = self.find_variant(name) {
                        // Plan 14 Ф.1: Some(x) infer как NovaOpt_<T>, где T = тип аргумента.
                        // None infer'ится по контексту current_fn_return_ty (если NovaOpt_<X>).
                        // Иначе — legacy NovaOpt_nova_int.
                        if type_name == "Option" || type_name == "NovaOpt_nova_int" {
                            if name == "Some" && !args.is_empty() {
                                let arg_ty = self.infer_expr_c_type(args[0].expr());
                                if !arg_ty.is_empty() && arg_ty != "void*" {
                                    let sanitized = Self::sanitize_for_novaopt(&arg_ty);
                                    return format!("NovaOpt_{}", sanitized);
                                }
                            }
                            if name == "None" {
                                if let Some(t) = self.current_fn_return_ty.as_ref() {
                                    if t.starts_with("NovaOpt_") {
                                        return t.clone();
                                    }
                                }
                            }
                            return "NovaOpt_nova_int".into();
                        }
                        // Plan 48 Ф.7.4 (partial): user-defined generic sum
                        // constructor with args (`Ok2(42)` etc.) — infer mono
                        // instance from arg types so the let-binding gets the
                        // concrete `Nova_Result2____nova_int*` type, not the
                        // erased `Nova_Result2*`. This is what feeds the mono
                        // method-dispatch path on subsequent `.method()` calls.
                        if let Some((_, mangled, _)) =
                            self.try_infer_variant_mono_args(name, args)
                        {
                            return format!("{}*", mangled);
                        }
                        return format!("Nova_{}*", type_name);
                    }
                    let key = format!("fn_ret_{}", name);
                    if let Some(t) = self.var_types.get(&key).cloned() {
                        return t;
                    }
                    // Plan 48: infer concrete return type for monomorphized generic fn calls.
                    // When a generic fn's return type is a bare type param T, resolve T
                    // from the first matching argument type.
                    if self.generic_fns.contains(name.as_str()) {
                        if let Some(fn_decl) = self.mono_fn_decls.get(name).cloned() {
                            // Tuple-returning or unresolvable generics use erasure (void* return).
                            let is_tuple_return = matches!(fn_decl.return_type, Some(crate::ast::TypeRef::Tuple(..)));
                            if is_tuple_return {
                                return "void*".into();
                            }
                            if let Some(ref ret_ty_ref) = fn_decl.return_type {
                                // Collect subst from arg types
                                let mut subst: Vec<(String, Option<String>)> = fn_decl.generics.iter()
                                    .map(|g| (g.name.clone(), None))
                                    .collect();
                                // Plan 54 Ф.4 Source 1: turbofish args (наибольший
                                // приоритет). Без этого `collect_all[int]([...])` —
                                // T=int lost при inference → return-type void*.
                                for (i, tr) in turbofish_args.iter().enumerate() {
                                    if i < subst.len() {
                                        if let Ok(c_ty) = self.type_ref_to_c(tr) {
                                            if !c_ty.is_empty() && c_ty != "void*" {
                                                subst[i].1 = Some(c_ty);
                                            }
                                        }
                                    }
                                }
                                for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
                                    let arg_c = self.infer_expr_c_type(arg.expr());
                                    Self::infer_type_param_binding(&param.ty, &arg_c, &mut subst);
                                }
                                // Source 2b: for fn-typed params, infer T from closure body return type.
                                // `body fn() -> T` + closure `|| 42` → T = nova_int.
                                for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
                                    if let crate::ast::TypeRef::Func { return_type: Some(ret_ty_ref), .. } = &param.ty {
                                        let closure_ret_c = match &arg.expr().kind {
                                            ExprKind::ClosureLight { body, .. } => match body {
                                                crate::ast::ClosureBody::Expr(e) => {
                                                    let t = self.infer_expr_c_type(e);
                                                    if t.is_empty() || t == "void*" { String::new() } else { t }
                                                }
                                                crate::ast::ClosureBody::Block(b) => b.trailing.as_ref()
                                                    .map(|e| self.infer_expr_c_type(e))
                                                    .filter(|t| !t.is_empty() && t != "void*")
                                                    .unwrap_or_default(),
                                            },
                                            ExprKind::ClosureFull(sb) => sb.return_type.as_ref()
                                                .and_then(|rt| self.type_ref_to_c(rt).ok())
                                                .filter(|t| !t.is_empty() && t != "void*")
                                                .unwrap_or_default(),
                                            _ => String::new(),
                                        };
                                        if !closure_ret_c.is_empty() {
                                            Self::infer_type_param_binding(ret_ty_ref.as_ref(), &closure_ret_c, &mut subst);
                                        }
                                    }
                                }
                                // Plan 54 Ф.5 Source 2d: for variable references
                                // to fn-typed params — look up registered return
                                // type in fn_param_sigs (already substituted в
                                // outer mono context).
                                for (param, arg) in fn_decl.params.iter().zip(args.iter()) {
                                    if let crate::ast::TypeRef::Func { return_type: Some(ret_ty_ref), .. } = &param.ty {
                                        if let ExprKind::Ident(name) = &arg.expr().kind {
                                            if let Some((_, ret_c)) = self.fn_param_sigs.get(name) {
                                                if !ret_c.is_empty() && ret_c != "void*" && ret_c != "nova_unit" {
                                                    Self::infer_type_param_binding(ret_ty_ref.as_ref(), ret_c, &mut subst);
                                                }
                                            }
                                        }
                                    }
                                }
                                // Resolve return type by substituting bare type params
                                let resolved = Self::apply_type_subst_to_ref(ret_ty_ref, &subst);
                                if let Some(c_ty) = resolved {
                                    if !c_ty.is_empty() && c_ty != "void*" {
                                        return c_ty;
                                    }
                                }
                                // If return type resolution failed (e.g. generic record T),
                                // fall back to void* (erased return).
                                return "void*".into();
                            }
                        }
                    }
                    // Plan 08 Ф.4 prerequisite: closure-call (fn-параметр)
                    // имеет ret_ty в fn_param_sigs. Без этого `pred(x)` где
                    // `pred fn(int) -> bool` инфер'ится как nova_int.
                    if let Some((_, ret_ty)) = self.fn_param_sigs.get(name) {
                        return ret_ty.clone();
                    }
                    "nova_int".into()
                } else if let ExprKind::Member { obj, name: method } = &func.kind {
                    // D38 array-static-method: `[]T.new()` / `[]T.with_capacity(n)`
                    // → NovaArray_<T>*. obj — Path(["__array", "<T>"]).
                    if let ExprKind::Path(parts) = &obj.kind {
                        if parts.len() == 2 && parts[0] == "__array"
                            && (method == "new" || method == "with_capacity")
                        {
                            let arr_suffix = match parts[1].as_str() {
                                "str"            => "nova_str",
                                "byte" | "u8"    => "nova_byte",
                                "bool"           => "nova_bool",
                                "f64" | "f32"    => "nova_f64",
                                _                => "nova_int",
                            };
                            return format!("NovaArray_{}*", arr_suffix);
                        }
                    }
                    let obj_ty = self.infer_expr_c_type(obj);
                    // Plan 48: если obj_ty — монотип вида "Nova_X____A__B*",
                    // вычислить return-type метода через generic_type_methods["X"]
                    // с подстановкой type-аргументов. Это исправляет случаи вроде
                    // `let s = p.swap()` где p: Pair[int,str] — без этого
                    // fn_ret_swap = "void*" (erased) и поля s потом неверно typed.
                    if let Some(ret) = self.infer_mono_method_ret(&obj_ty, method) {
                        return ret;
                    }
                    // Plan 14 std-fix: built-in str методы (starts_with/ends_with/
                    // contains/eq/...) — return-type из hardcoded map'а. Без этого
                    // `s.starts_with(...)` инфер'ится как `nova_int` (default
                    // fallback), что ломает strict bool-check для `if`.
                    if obj_ty == "nova_str" {
                        if let Some(rt) = Self::str_method_ret_type(method) {
                            return rt.into();
                        }
                    }
                    // D109: prim_builtin_method — eq/lt/le/gt/ge на int/bool/f64
                    // возвращают nova_bool, не nova_int. Без этого `if x.lt(y)`
                    // в generic-теле с T=nova_int ломает strict bool-check.
                    if let Some(prim) = Self::prim_builtin_method(&obj_ty, method) {
                        return match prim {
                            PrimBuiltin::BinOp("==") | PrimBuiltin::BinOp("<")
                            | PrimBuiltin::BinOp("<=") | PrimBuiltin::BinOp(">")
                            | PrimBuiltin::BinOp(">=") => "nova_bool".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    // Plan 06 Ф.3: `coll.iter()` → registered IterT type.
                    if method == "iter" {
                        let coll_type = obj_ty.trim_start_matches("Nova_")
                            .trim_end_matches('*').trim().to_string();
                        if let Some(iter_t) = self.iter_returns.get(&coll_type) {
                            return format!("Nova_{}*", iter_t);
                        }
                    }
                    // D91 (Plan 21): Channel.new → Nova_ChannelPair.
                    if let ExprKind::Ident(n) = &obj.kind {
                        if n == "Channel" && method == "new" {
                            return "Nova_ChannelPair".into();
                        }
                        // D94 (Plan 31): Time.after(ms) → Nova_ChanReader*.
                        if n == "Time" && method == "after" {
                            return "Nova_ChanReader*".into();
                        }
                        // D75 (revised, Plan 47): CancelToken.new() — Member-form.
                        if n == "CancelToken" && method == "new" {
                            return "NovaCancelToken*".into();
                        }
                    }
                    // D75: instance methods on NovaCancelToken*.
                    // Plan 49 Ф.1 + Ф.6 P0 fix: reason() возвращает Option[T]
                    // где T определяется из cancel_token_t_map (если receiver —
                    // tracked Ident). Backward-compat: Option[str] default.
                    if self.infer_expr_c_type(obj) == "NovaCancelToken*" {
                        match method.as_str() {
                            "is_cancelled" => return "nova_bool".into(),
                            "cancel" | "cancelled_by" => return "nova_unit".into(),
                            "merge" => return "NovaCancelToken*".into(),
                            "reason" => {
                                if let ExprKind::Ident(name) = &obj.kind {
                                    if let Some(t_c) = self.cancel_token_t_map.get(name) {
                                        if t_c != "nova_str" {
                                            // Plan 54 Ф.9: использовать
                                            // sanitize_c_for_ident для NovaOpt
                                            // suffix чтобы соответствовать
                                            // register_novaopt_decl naming
                                            // (особенно для pointer types).
                                            let sanitized = Self::sanitize_c_for_ident(t_c);
                                            return format!("NovaOpt_{}", sanitized);
                                        }
                                    }
                                }
                                return "NovaOpt_nova_str".into();
                            }
                            _ => {}
                        }
                    }
                    // D91: Sender capability method return types.
                    if obj_ty == "Nova_ChanWriter*" {
                        return match method.as_str() {
                            "send"             => "nova_bool".into(),
                            "close"            => "nova_unit".into(),
                            "try_send"         => "nova_bool".into(),
                            "is_closed"        => "nova_bool".into(),
                            "len" | "capacity" => "nova_int".into(),
                            "clone"            => "Nova_ChanWriter*".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    // D91: Receiver capability method return types.
                    if obj_ty == "Nova_ChanReader*" {
                        return match method.as_str() {
                            "recv" | "try_recv" => "NovaOpt_nova_int".into(),
                            "is_closed"         => "nova_bool".into(),
                            "len" | "capacity"  => "nova_int".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    // Plan 04 Этап 6: Buffer removed. StringBuilder/WriteBuffer/
                    // ReadBuffer infer ниже.
                    // Plan 04: built-in StringBuilder/WriteBuffer/ReadBuffer
                    // static-method type inference.
                    if let ExprKind::Ident(n) = &obj.kind {
                        if n == "StringBuilder" {
                            return match method.as_str() {
                                "new" | "with_capacity" | "from" => "Nova_StringBuilder*".into(),
                                _ => "nova_int".into(),
                            };
                        }
                        if n == "WriteBuffer" {
                            return match method.as_str() {
                                "new" | "with_capacity" | "from" => "Nova_WriteBuffer*".into(),
                                _ => "nova_int".into(),
                            };
                        }
                        if n == "ReadBuffer" {
                            return match method.as_str() {
                                "from" => "Nova_ReadBuffer*".into(),
                                _ => "nova_int".into(),
                            };
                        }
                        // Plan 32: gc.* introspection — type inference.
                        if n == "gc" {
                            return match method.as_str() {
                                "heap_size" | "live_count" | "alloc_count" => "nova_int".into(),
                                "collect" | "reset_stats" => "nova_int".into(), // unit comma-expr
                                _ => "nova_int".into(),
                            };
                        }
                        // Plan 44.2 Этап 3: fibers.* introspection — type inference.
                        if n == "fibers" {
                            return match method.as_str() {
                                "virtual_reserved" | "slot_count" |
                                "slots_active" | "high_water" => "nova_int".into(),
                                "compact" => "nova_int".into(), // unit comma-expr
                                _ => "nova_int".into(),
                            };
                        }
                        // Plan 44 Этап 0: runtime.* — type inference.
                        if n == "runtime" {
                            return match method.as_str() {
                                "init" | "shutdown" => "nova_int".into(),
                                "worker_count" | "current_worker_id" => "nova_int".into(),
                                "is_initialized" => "nova_bool".into(),
                                _ => "nova_int".into(),
                            };
                        }
                        // Plan 04 follow-up: f64.from_bits(int) → nova_f64,
                        // int.to_bits(f64) → nova_int.
                        if n == "f64" && method == "from_bits" {
                            return "nova_f64".into();
                        }
                        if n == "int" && method == "to_bits" {
                            return "nova_int".into();
                        }
                        // Plan 12 + Plan 18: ExternalRegistry static-method return type.
                        // Handles AtomicInt.new(), Mutex.new(), WaitGroup.new(), etc.
                        if let Some(decls) = self.external_registry.lookup(n, method) {
                            if let Some(decl) = decls.iter().find(|d| !d.is_instance) {
                                return decl.return_c_type.clone();
                            }
                        }
                    }
                    // Plan 04 + Plan 13 Ф.9.1: instance-method type inference.
                    // Self-return для chaining (mut @append, all @write_*, @clone).
                    if obj_ty == "Nova_StringBuilder*" {
                        return match method.as_str() {
                            // byte_len — Plan 13 Ф.9 fix: codepoint vs byte split.
                            "len" | "byte_len" | "capacity" => "nova_int".into(),
                            "clone" => "Nova_StringBuilder*".into(),
                            "into" => "nova_str".into(),
                            // Self-return: @append(s|c), @plus(s|c) — Plan 13 Ф.9.2.
                            "append" | "plus" => "Nova_StringBuilder*".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    if obj_ty == "Nova_WriteBuffer*" {
                        return match method.as_str() {
                            "len" | "capacity" => "nova_int".into(),
                            "clone" => "Nova_WriteBuffer*".into(),
                            "into" => "NovaArray_nova_byte*".into(),
                            // Self-return для chaining (Ф.9.1).
                            m if m.starts_with("write_") => "Nova_WriteBuffer*".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    if obj_ty == "Nova_ReadBuffer*" {
                        return match method.as_str() {
                            "position" | "remaining" => "nova_int".into(),
                            "has_remaining" => "nova_bool".into(),
                            "remaining_bytes" => "NovaArray_nova_byte*".into(),
                            // Fail-form read_*: возвращает unboxed T (через
                            // Fail-throw на error). Тип зависит от method.
                            "read_byte" | "read_u8" => "nova_byte".into(),
                            "read_i8" => "nova_int".into(),
                            "read_bytes" => "NovaArray_nova_byte*".into(),
                            // Plan 13 Ф.9.4: codepoint-уровневые reads.
                            "read_char" => "nova_int".into(),  // char хранится как nova_int
                            "read_str"  => "nova_str".into(),
                            "read_u16_le" | "read_u16_be"
                            | "read_u32_le" | "read_u32_be"
                            | "read_u64_le" | "read_u64_be"
                            | "read_i16_le" | "read_i16_be"
                            | "read_i32_le" | "read_i32_be"
                            | "read_i64_le" | "read_i64_be" => "nova_int".into(),
                            "read_f32_le" | "read_f32_be"
                            | "read_f64_le" | "read_f64_be" => "nova_f64".into(),
                            // Try-form: Result[T, ReadBufferError].
                            m if m.starts_with("try_read_") => "Nova_Result*".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    // Plan 12 + Plan 18: ExternalRegistry instance-method return type.
                    // Handles AtomicInt.@load(), Mutex.@lock(), WaitGroup.@wait(), etc.
                    if obj_ty.starts_with("Nova_") && obj_ty.ends_with('*') {
                        let recv_ty = obj_ty.trim_start_matches("Nova_")
                            .trim_end_matches('*').trim();
                        if let Some(decls) = self.external_registry.lookup(recv_ty, method) {
                            if let Some(decl) = decls.iter().find(|d| d.is_instance) {
                                return decl.return_c_type.clone();
                            }
                        }
                    }
                    // D26 prelude: NovaOpt_T method type inference.
                    if obj_ty.starts_with("NovaOpt_") {
                        let elem_ty = obj_ty.strip_prefix("NovaOpt_")
                            .unwrap_or("nova_int")
                            .trim_end_matches('*')
                            .trim()
                            .to_string();
                        return match method.as_str() {
                            "is_some" | "is_none" => "nova_bool".into(),
                            "unwrap_or" | "unwrap" | "unwrap_or_else" => elem_ty,
                            "map" => format!("NovaOpt_{}", elem_ty),
                            "ok_or" => "Nova_Result*".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    // D26 prelude: Nova_Result* method type inference.
                    if obj_ty == "Nova_Result*" {
                        return match method.as_str() {
                            "is_ok" | "is_err" => "nova_bool".into(),
                            "unwrap" | "unwrap_or" | "unwrap_or_else" => "nova_int".into(),
                            "ok" | "err" => "NovaOpt_nova_int".into(),
                            "map" | "map_err" => "Nova_Result*".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    // Built-in primitive `str.from(x) -> str` (D35 + D73).
                    if let ExprKind::Ident(n) = &obj.kind {
                        if n == "str" && method == "from" { return "nova_str".into(); }
                        // User-defined `T.from(v)` returns Nova_T* (most cases).
                        if method == "from"
                            && (self.record_schemas.contains_key(n)
                                || self.sum_schemas.contains_key(n))
                        {
                            return format!("Nova_{}*", n);
                        }
                    }
                    // If object is an unknown generic stub (void*), method result is also void*
                    // Exception: self-referential call inside a sum-type method — look up
                    // return type from current receiver's method_overloads.
                    if obj_ty == "void*" {
                        if let Some(recv_ty) = &self.current_receiver_type {
                            if self.all_methods.contains(&(recv_ty.clone(), method.to_string())) {
                                let key = (recv_ty.clone(), method.to_string());
                                if let Some(overloads) = self.method_overloads.get(&key) {
                                    if let Some(sig) = overloads.iter().find(|s| s.is_instance) {
                                        return sig.return_c_type.clone();
                                    }
                                }
                            }
                        }
                        return "void*".into();
                    }
                    // Effect dispatch: TypeName.method() → look up in effect_schemas
                    let eff_name = match &obj.kind {
                        ExprKind::Ident(n) => Some(n.clone()),
                        ExprKind::Path(p) => Some(p.join("_")),
                        _ => None,
                    };
                    if let Some(ref eff) = eff_name {
                        if let Some(schema) = self.effect_schemas.get(eff.as_str()) {
                            if let Some((_, ret_ty)) = Self::schema_lookup(schema, method.as_str()) {
                                return ret_ty.clone();
                            }
                        }
                    }
                    // Plan 08 Ф.3: v.@try_into() через auto-derive →
                    // Result[T, E]. Distinct от @into (return T напрямую).
                    if method == "try_into" {
                        let recv_type = Self::nova_type_name_from_c(&obj_ty);
                        // Если есть `T.try_from(v V)` для V == recv_type, это
                        // synthesis target — возвращает Result.
                        let has_try_from_target = self.try_from_targets.iter()
                            .any(|(_, sources)| sources.iter().any(|s| s == &recv_type));
                        if has_try_from_target {
                            return "Nova_Result*".into();
                        }
                        // Иначе fallback на explicit @try_into.
                        if let Some(target) = self.try_into_targets.get(&recv_type) {
                            let _ = target;
                            return "Nova_Result*".into();
                        }
                    }
                    // Plan 08 Ф.3: v.@into() через auto-derive → T напрямую.
                    if method == "into" {
                        let recv_type = Self::nova_type_name_from_c(&obj_ty);
                        let target = self.from_targets.iter()
                            .find(|(_, sources)| sources.iter().any(|s| s == &recv_type))
                            .map(|(t, _)| t.clone());
                        if let Some(target_type) = target {
                            return format!("Nova_{}*", target_type);
                        }
                        if let Some(target_type) = self.into_targets.get(&recv_type) {
                            return format!("Nova_{}*", target_type);
                        }
                    }
                    // Array method calls
                    if obj_ty.starts_with("NovaArray_") {
                        let elem_ty = obj_ty.strip_prefix("NovaArray_").unwrap_or("nova_int")
                            .trim_end_matches('*').trim();
                        return match method.as_str() {
                            "get" | "pop" => format!("NovaOpt_{}", elem_ty),
                            "push" => "nova_unit".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    // D74 math methods on f64/f32 — return f64 (most) or bool (predicates).
                    if obj_ty == "nova_f64" || obj_ty == "nova_f32" {
                        if Self::f64_method_to_c(method).is_some() {
                            return match method.as_str() {
                                "is_nan" | "is_finite" | "is_infinite" => "nova_bool".into(),
                                _ => "nova_f64".into(),
                            };
                        }
                    }
                    // D109: built-in primitive methods return-type inference.
                    if let Some(builtin) = Self::prim_builtin_method(&obj_ty, method) {
                        return match builtin {
                            PrimBuiltin::Fn(_) => "nova_int".into(), // hash → u64 = nova_int
                            PrimBuiltin::BinOp(_) => "nova_bool".into(),
                        };
                    }
                    // D74 math methods on int.
                    if obj_ty == "nova_int" {
                        if Self::int_method_to_c(method).is_some() {
                            return "nova_int".into();
                        }
                    }
                    // String method calls return type inference
                    if obj_ty == "nova_str" {
                        return match method.as_str() {
                            "to_upper" | "to_lower" | "trim" | "slice" | "concat" => "nova_str".into(),
                            "starts_with" | "ends_with" | "contains" | "eq" => "nova_bool".into(),
                            "len" | "char_len" | "byte_len" => "nova_int".into(),
                            "find" | "rfind" | "char_at" => "NovaOpt_nova_int".into(),
                            // D26: s.bytes() → []byte (packed uint8_t[]).
                            "bytes" => "NovaArray_nova_byte*".into(),
                            // s.chars() → []char (bootstrap-eager codepoints как nova_int).
                            "chars" => "NovaArray_nova_int*".into(),
                            // s.split(sep) → []str (Iter[str] eager в bootstrap).
                            "split" => "NovaArray_nova_str*".into(),
                            _ => "nova_int".into(),
                        };
                    }
                    // Direct vtable call: switcher.flip() → look up method ret in effect schema
                    if obj_ty.starts_with("NovaVtable_") && obj_ty.ends_with('*') {
                        let eff = obj_ty
                            .strip_prefix("NovaVtable_").unwrap_or("")
                            .trim_end_matches('*').trim();
                        if let Some(schema) = self.effect_schemas.get(eff) {
                            if let Some((_, ret_ty)) = Self::schema_lookup(schema, method.as_str()) {
                                return ret_ty.clone();
                            }
                        }
                    }
                    // Plan 55 Ф.4: well-known protocol-method names — type-stable per protocol.
                    // Когда receiver — generic-bound (Hashable/Comparable/etc.), fallback
                    // на global fn_ret_<m> может выбрать stale int — нужны явные whitelist'ы.
                    match method.as_str() {
                        // Equality / ordering — Hashable / Comparable bounds.
                        "eq" | "ne" | "lt" | "le" | "gt" | "ge" => return "nova_bool".into(),
                        // Predicates на values.
                        "is_zero" | "is_positive" | "is_negative" | "is_nan"
                            | "is_finite" | "is_infinite" => return "nova_bool".into(),
                        // Hash → u64 (nova_int storage).
                        "hash" => return "nova_int".into(),
                        _ => {}
                    }
                    // User-defined method: look up return type registered during forward decl
                    let ret_key = format!("fn_ret_{}", method);
                    if let Some(ret_ty) = self.var_types.get(&ret_key) {
                        return ret_ty.clone();
                    }
                    "nova_int".into()
                } else if let ExprKind::Path(parts) = &func.kind {
                    // Plan 11 Ф.4.5: Self.method(...) → <current>.method(...).
                    let parts_resolved: Vec<String>;
                    let parts: &[String] = if !parts.is_empty() && parts[0] == "Self" {
                        if let Some(recv) = &self.current_receiver_type {
                            let mut p = parts.clone();
                            p[0] = recv.clone();
                            parts_resolved = p;
                            &parts_resolved
                        } else {
                            parts
                        }
                    } else {
                        parts
                    };
                    // Effect dispatch via path: `Echo.say()` → look up in effect_schemas
                    if parts.len() == 2 {
                        let eff = &parts[0];
                        let method_name = &parts[1];
                        // D91 (Plan 21): Channel.new(cap) — Path-form.
                        if eff == "Channel" && method_name == "new" {
                            return "Nova_ChannelPair".into();
                        }
                        // D75 (revised, Plan 47): CancelToken.new() — Path-form.
                        if eff == "CancelToken" && method_name == "new" {
                            return "NovaCancelToken*".into();
                        }
                        // Plan 04 Этап 6: Buffer removed. StringBuilder/
                        // WriteBuffer/ReadBuffer effect-schema ниже.
                        // Plan 04: built-in opaque static methods.
                        if eff == "StringBuilder" {
                            return match method_name.as_str() {
                                "new" | "with_capacity" | "from" => "Nova_StringBuilder*".into(),
                                _ => "nova_int".into(),
                            };
                        }
                        if eff == "WriteBuffer" {
                            return match method_name.as_str() {
                                "new" | "with_capacity" | "from" => "Nova_WriteBuffer*".into(),
                                _ => "nova_int".into(),
                            };
                        }
                        if eff == "ReadBuffer" {
                            return match method_name.as_str() {
                                "from" => "Nova_ReadBuffer*".into(),
                                _ => "nova_int".into(),
                            };
                        }
                        // Plan 04 follow-up: f64.from_bits / int.to_bits.
                        if eff == "f64" && method_name == "from_bits" {
                            return "nova_f64".into();
                        }
                        if eff == "int" && method_name == "to_bits" {
                            return "nova_int".into();
                        }
                        // D26 prelude: Error.new(msg) → Nova_Error*.
                        if eff == "Error" && method_name == "new" {
                            return "Nova_Error*".into();
                        }
                        // Plan 08 Ф.2: T.try_from(...) → Result[T, E] = Nova_Result*.
                        if method_name == "try_from" {
                            return "Nova_Result*".into();
                        }
                        // Plan 08 Ф.2: str.from(numeric/bool/char) → nova_str.
                        if eff == "str" && method_name == "from" {
                            return "nova_str".into();
                        }
                        // Built-in primitive `str.from(x) -> str` (D35 + D73).
                        if eff == "str" && method_name == "from" {
                            return "nova_str".into();
                        }
                        // User-defined `T.from(v)` returns Nova_T* (most cases).
                        // Match by receiver type explicitly — avoids `fn_ret_from`
                        // collision when multiple types have `from`.
                        if method_name == "from" {
                            // Heuristic: if T is a known record/sum, return Nova_T*.
                            if self.record_schemas.contains_key(eff)
                                || self.sum_schemas.contains_key(eff)
                            {
                                return format!("Nova_{}*", eff);
                            }
                        }
                        if let Some(schema) = self.effect_schemas.get(eff.as_str()) {
                            if let Some((_, ret_ty)) = Self::schema_lookup(schema, method_name.as_str()) {
                                return ret_ty.clone();
                            }
                        }
                        // Plan 12 + Plan 18: ExternalRegistry static-method lookup
                        // для Path-form вызовов (Once.new(), AtomicBool.new(), etc.).
                        // Path-form используется вместо Member-form для статических
                        // методов внешних типов — Member-branch выше не покрывает это.
                        if let Some(decls) = self.external_registry.lookup(eff, method_name) {
                            if let Some(decl) = decls.iter().find(|d| !d.is_instance) {
                                return decl.return_c_type.clone();
                            }
                        }
                        let key = format!("fn_ret_{}", method_name);
                        self.var_types.get(&key).cloned().unwrap_or_else(|| "nova_int".into())
                    } else {
                        "nova_int".into()
                    }
                } else {
                    "nova_int".into()
                }
            }
            ExprKind::ArrayLit(elems) => {
                // Infer element type from first element to handle []str literals
                for e in elems {
                    let inner = match e { ArrayElem::Item(x) | ArrayElem::Spread(x) => x };
                    let et = self.infer_expr_c_type(inner);
                    if et == "nova_str" {
                        return "NovaArray_nova_str*".into();
                    }
                    break;
                }
                "NovaArray_nova_int*".into()
            }
            ExprKind::If { else_, then, .. } => {
                // if without else is always nova_unit
                if else_.is_none() {
                    return "nova_unit".into();
                }
                // if/else: infer from then-block trailing
                then.trailing.as_ref()
                    .map(|e| self.infer_expr_c_type(e))
                    .unwrap_or_else(|| "nova_unit".into())
            }
            ExprKind::Match { arms, .. } => {
                // Infer result type from first non-unit arm
                for arm in arms {
                    let t = match &arm.body {
                        MatchArmBody::Expr(e) => self.infer_expr_c_type(e),
                        MatchArmBody::Block(b) => b.trailing.as_ref()
                            .map(|e| self.infer_expr_c_type(e))
                            .unwrap_or_else(|| "nova_unit".into()),
                    };
                    if t != "nova_unit" && t != "nova_int" {
                        return t;
                    }
                }
                "nova_int".into()
            }
            ExprKind::Member { obj, name } => {
                // Plan 11 Ф.4: method value `@`-prefix → closure (void*).
                if name.starts_with('@') {
                    return "void*".into();
                }
                let obj_ty = self.infer_expr_c_type(obj);
                if obj_ty == "nova_str" && name == "len" {
                    return "nova_int".into();
                }
                if obj_ty.starts_with("NovaArray_") && name == "len" {
                    return "nova_int".into();
                }
                // Plan 14 std-fix: D38 built-in `is_empty` для []T и str → bool.
                if (obj_ty.starts_with("NovaArray_") || obj_ty == "nova_str") && name == "is_empty" {
                    return "nova_bool".into();
                }
                // Tuple field access: check element type registry first (works for void* too)
                if name.chars().all(|c| c.is_ascii_digit()) {
                    if let ExprKind::Ident(var_name) = &obj.kind {
                        let idx: usize = name.parse().unwrap_or(0);
                        if let Some(elem_tys) = self.tuple_element_types.get(var_name.as_str()) {
                            if let Some(elem_ty) = elem_tys.get(idx) {
                                if !elem_ty.is_empty() {
                                    // Heap-wrapped value types are dereffed when emitted, return base type
                                    let base = elem_ty.trim_end_matches('*');
                                    let was_heap_wrapped = elem_ty.ends_with('*') && (
                                        base.starts_with("_NovaTuple") || base.starts_with("NovaOpt_") || base == "nova_str"
                                    );
                                    if was_heap_wrapped {
                                        return base.to_string();
                                    }
                                    return elem_ty.clone();
                                }
                            }
                        }
                    }
                }
                // Unknown generic stub (void*): field access returns void*
                if obj_ty == "void*" {
                    return "void*".into();
                }
                // D91 (Plan 21): Nova_ChannelPair field types.
                if obj_ty == "Nova_ChannelPair" {
                    return match name.as_str() {
                        "tx" => "Nova_ChanWriter*".into(),
                        "rx" => "Nova_ChanReader*".into(),
                        _ => "nova_int".into(),
                    };
                }
                // Field type lookup from record schema
                let struct_name = obj_ty
                    .strip_prefix("Nova_")
                    .unwrap_or("")
                    .trim_end_matches('*')
                    .trim()
                    .to_string();
                // For monomorphized names like "Queue____nova_int": look up concrete schema first.
                let base_name: &str = struct_name.split("____").next().unwrap_or(&struct_name);
                // 1. Concrete mono schema (registered by drain_generic_type_worklist).
                if let Some(schema) = self.record_schemas.get(&struct_name) {
                    if let Some(field_ty) = schema.get(name.as_str()) {
                        return field_ty.clone();
                    }
                }
                // 2. Plan 48: if mono schema not yet registered (test body runs before drain),
                //    compute field type directly from generic_type_templates + type arg substitution.
                if base_name.len() < struct_name.len() {
                    let args_str = &struct_name[base_name.len() + 4..]; // skip "____"
                    let type_args: Vec<String> = args_str.split("__")
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                    if let Some(template) = self.generic_type_templates.get(base_name).cloned() {
                        if let crate::ast::TypeDeclKind::Record(fields) = &template.kind {
                            if let Some(field_decl) = fields.iter().find(|f| f.name == *name) {
                                let subst: Vec<(String, Option<String>)> = template.generics.iter()
                                    .zip(type_args.iter())
                                    .map(|(g, c)| (g.name.clone(), Some(c.clone())))
                                    .collect();
                                if let Some(c_ty) = Self::apply_type_subst_to_ref(&field_decl.ty, &subst) {
                                    return c_ty;
                                }
                            }
                        }
                    }
                }
                // 3. Erased base schema fallback (void* for type-param fields).
                if let Some(schema) = self.record_schemas.get(base_name) {
                    if let Some(field_ty) = schema.get(name.as_str()) {
                        return field_ty.clone();
                    }
                }
                "nova_int".into()
            }
            ExprKind::Is(_, _) => "nova_bool".into(),
            ExprKind::As(_, ty) => {
                // D54: тип `expr as T` — это T, не type-of(expr).
                // Без этого `let b = a as byte` infer'ил бы тип b как nova_int
                // (тип a) вместо nova_byte. План 05.
                self.type_ref_to_c(ty).unwrap_or_else(|_| "nova_int".into())
            }
            // Plan 36 followup: `0..10` literal — Nova_Range* (если type
            // зарегистрирован). Без этого fallback nova_int ломал
            // `(0..N).step_by(K)` method-call inference: искал
            // method_overloads[("nova_int", "step_by")] = miss → nova_int
            // → for-in unsupported iterator.
            //
            // Если Range не зарегистрирован, остаётся nova_int — emit_for
            // Case 1 (primitive int loop) обрабатывает это через
            // `ExprKind::Range` pattern до infer call'а.
            ExprKind::Range { .. } => {
                if self.record_schemas.contains_key("Range") {
                    "Nova_Range*".into()
                } else {
                    "nova_int".into()
                }
            }
            ExprKind::For { .. } => "nova_unit".into(),
            ExprKind::ParallelFor { body, .. } => {
                // D71: array-mode when trailing exists, unit otherwise.
                match &body.trailing {
                    Some(t) => {
                        let et = self.infer_expr_c_type(t);
                        match et.as_str() {
                            "nova_int" | "nova_bool" | "nova_f64" | "nova_str" =>
                                format!("NovaArray_{}*", et),
                            _ => "nova_unit".into(),
                        }
                    }
                    None => "nova_unit".into(),
                }
            }
            ExprKind::While { .. } => "nova_unit".into(),
            ExprKind::WhileLet { .. } => "nova_unit".into(),
            ExprKind::Loop { .. } => "nova_unit".into(),
            ExprKind::Supervised { .. } => "nova_unit".into(),
            ExprKind::Detach(_) => "nova_unit".into(),
            ExprKind::TaggedTemplate { .. } => "nova_str".into(),
            // Plan 39 Issue A: With-блок тип = T_body. Если trailing == None
            // (body заканчивается throw/return/interrupt statement'ом), смотрим
            // на handler interrupt-VAL тип через bindings — это semantically
            // тип результата (W = type of every `interrupt v` ⊑ T_body).
            // Inline handler-лямбда (D31) — body содержит `interrupt VAL` —
            // ищем рекурсивно.
            ExprKind::With { bindings, body } => {
                if let Some(trailing) = &body.trailing {
                    return self.infer_expr_c_type(trailing);
                }
                // Trailing == None — body falls off (throw / return).
                // Probe handler-лямбды на interrupt VAL.
                for b in bindings {
                    if let Some(ty) = infer_handler_interrupt_ty(self, &b.handler) {
                        return ty;
                    }
                }
                "nova_unit".into()
            }
            _ => "nova_int".into(),
        }
    }

    /// Produce a short human-readable description of an expression for assert messages.
    fn expr_to_display(expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::IntLit(n) => n.to_string(),
            ExprKind::BoolLit(b) => b.to_string(),
            ExprKind::StrLit(s) => format!("\"{}\"", s),
            ExprKind::Ident(n) => n.clone(),
            ExprKind::Binary { op, left, right } => {
                let op_str = match op {
                    BinOp::Eq => "==", BinOp::Neq => "!=",
                    BinOp::Lt => "<",  BinOp::Le  => "<=",
                    BinOp::Gt => ">",  BinOp::Ge  => ">=",
                    BinOp::Add => "+", BinOp::Sub => "-",
                    BinOp::Mul => "*", BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::And => "&&", BinOp::Or => "||",
                    // Plan 33.1: ==> / <==> для display (диагностика).
                    BinOp::Implies => "==>", BinOp::Iff => "<==>",
                    BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<", BinOp::Shr => ">>",
                };
                format!("{} {} {}",
                    Self::expr_to_display(left), op_str, Self::expr_to_display(right))
            }
            ExprKind::Call { func, args, .. } => {
                let fn_name = Self::expr_to_display(func);
                let arg_strs: Vec<String> = args.iter().map(|a| {
                    let inner = Self::expr_to_display(a.expr());
                    if a.is_spread() { format!("...{}", inner) } else { inner }
                }).collect();
                format!("{}({})", fn_name, arg_strs.join(", "))
            }
            ExprKind::Unary { op, operand } => {
                let op_str = match op { UnOp::Neg => "-", UnOp::Not => "!" };
                format!("{}{}", op_str, Self::expr_to_display(operand))
            }
            _ => "assert".to_string(),
        }
    }

    fn zero_literal_for_type(ty: &str) -> &'static str {
        match ty {
            "nova_unit"  => "NOVA_UNIT",
            "nova_bool"  => "false",
            "nova_f64" | "nova_f32" => "0.0",
            _ => "((nova_int)0LL)",
        }
    }

    fn escape_c_str(s: &str) -> String {
        let mut out = String::new();
        for c in s.chars() {
            match c {
                '"'  => out.push_str("\\\""),
                '\\'  => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c    => out.push(c),
            }
        }
        out
    }
}
