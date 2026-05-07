use crate::ast::*;
use crate::diag::Span;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;

pub struct CEmitter {
    out: String,
    /// File-scope handler impl function bodies (ctx structs + forward decls + bodies)
    deferred_impls: String,
    /// File-scope lambda forward declarations (static fn sig only). Flushed before fn definitions.
    lambda_forward_decls: String,
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
    /// D73 v2 auto-derive: target_type → list of source_types for which
    /// `target.from(src V)` is explicitly defined. Used to synthesize
    /// `v.into()` for V via target.from when no explicit `@into` exists.
    from_targets: HashMap<String, Vec<String>>,
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
    /// Maps local variable name → (param_c_types, return_c_type) for function-typed parameters.
    /// Used to emit proper function pointer calls for `body(args)` where body is a fn param.
    fn_param_sigs: HashMap<String, (Vec<String>, String)>,
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
    /// Optional Nova source text — when Some, codegen inserts each statement's
    /// originating Nova source as `/* SRC: ... */` comment in the generated C.
    /// Set via `--annotate-source` CLI flag. Off by default to keep .c diffs
    /// stable in CI.
    annotation_source: Option<String>,
}

impl CEmitter {
    pub fn new() -> Self {
        Self {
            out: String::new(),
            deferred_impls: String::new(),
            lambda_forward_decls: String::new(),
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
            from_targets: HashMap::new(),
            into_targets: HashMap::new(),
            tuple_element_types: HashMap::new(),
            record_variant_field_types: HashMap::new(),
            record_variant_field_order: HashMap::new(),
            current_fn_return_ty: None,
            array_element_types: HashMap::new(),
            option_inner_types: HashMap::new(),
            pending_option_inner_type: None,
            str_box_arrays: HashSet::new(),
            current_receiver_type: None,
            expected_record_type: None,
            fn_param_sigs: HashMap::new(),
            trailing_block_counter: 0,
            lambda_counter: 0,
            fn_returns_fn_sig: HashMap::new(),
            generic_fns: HashSet::new(),
            generic_types: HashSet::new(),
            generic_fn_tuple_arity: HashMap::new(),
            type_aliases: HashMap::new(),
            current_parfor_slot: None,
            annotation_source: None,
        }
    }

    /// Enable source-annotation mode: codegen will insert `/* SRC: ... */`
    /// comments before each statement showing the originating Nova source.
    /// Off by default — turn on for debugging the generated C.
    pub fn set_source_for_annotations(&mut self, src: String) {
        self.annotation_source = Some(src);
    }

    /// Get the Span of a statement (where in source it came from).
    fn stmt_span(stmt: &Stmt) -> Span {
        match stmt {
            Stmt::Let(d) => d.span,
            Stmt::Expr(e) => e.span,
            Stmt::Assign { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Throw { span, .. } => *span,
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

    pub fn emit_module(mut self, module: &Module) -> Result<String, String> {
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
            time_schema.insert("sleep".to_string(), (vec!["nova_int".into()], "nova_unit".into()));
            time_schema.insert("now".to_string(),   (vec![],                   "nova_int".into()));
            self.effect_schemas.insert("Time".to_string(), time_schema);
        }

        // Pre-register Mem as a built-in effect for runtime introspection.
        // Operations:
        //   - alloc_count() -> int : total nova_alloc calls since gc_init/reset
        //   - free_count()  -> int : total frees (plain malloc backend → 0)
        //   - live()        -> int : alloc_count - free_count
        //   - reset()       -> unit: zero stats counters (per-test isolation)
        // Used by leak/growth tests (see tests-nova/53_memory_growth.nv).
        {
            let mut mem_schema: HashMap<String, (Vec<String>, String)> = HashMap::new();
            mem_schema.insert("alloc_count".to_string(), (vec![], "nova_int".into()));
            mem_schema.insert("free_count".to_string(),  (vec![], "nova_int".into()));
            mem_schema.insert("live".to_string(),        (vec![], "nova_int".into()));
            mem_schema.insert("reset".to_string(),       (vec![], "nova_unit".into()));
            self.effect_schemas.insert("Mem".to_string(), mem_schema);
        }

        // Pre-register Buffer as a built-in record type with associated methods
        // (Q-buffer). Runtime impl in nova_rt/buffer.h. Methods registered in
        // method_receivers so emit_call routes Buffer.new() / buf.add_str() /
        // buf.into() / buf.try_into() etc. to the corresponding C functions.
        //
        // Note: `Buffer.from` is registered with overloads handled by the
        // dispatch logic in emit_call (different runtime fns for str/[]byte
        // arguments), since bootstrap's method_receivers is single-key —
        // see Q-overloading. We special-case the dispatch directly.
        {
            // record_schemas: Buffer has private fields (data/len/cap/consumed),
            // user code shouldn't touch them. We register with empty schema so
            // type lookup works but field access stays opaque.
            self.record_schemas.insert("Buffer".to_string(), HashMap::new());

            // Register methods. Static methods: new, with_capacity, from.
            // Instance methods: add_str/add_bytes/add_byte/add_char, len,
            // capacity, clone, into, try_into, into_str_unchecked.
            // method_receivers maps name -> (type, is_instance). Conflicts
            // with user-defined methods of the same name on other types are
            // possible: existing Nova_T_method_X dispatch wins for declared
            // user methods (registered later). For Buffer-only methods like
            // add_str, add_bytes, into_str_unchecked — no conflict.
            //
            // We do NOT register `new`, `with_capacity`, `from`, `len`,
            // `capacity`, `clone`, `into`, `try_into` here, because those
            // names are commonly shadowed by user types. Instead, dispatch
            // is special-cased in emit_call for receiver-typed Buffer*.
            // Only the unambiguous-name methods we register normally:
            self.method_receivers.insert("add_str".to_string(),
                ("Buffer".to_string(), true));
            self.method_receivers.insert("add_bytes".to_string(),
                ("Buffer".to_string(), true));
            self.method_receivers.insert("add_byte".to_string(),
                ("Buffer".to_string(), true));
            self.method_receivers.insert("add_char".to_string(),
                ("Buffer".to_string(), true));
            self.method_receivers.insert("into_str_unchecked".to_string(),
                ("Buffer".to_string(), true));
        }

        self.emit_preamble();

        // 1. Type declarations first (structs/unions needed by fn signatures)
        for item in &module.items {
            if let Item::Type(t) = item {
                self.emit_type_decl(t)?;
            }
        }

        // 1b. Const declarations (after types, before fn forward decls)
        for item in &module.items {
            if let Item::Const(c) = item {
                self.emit_const_decl(c)?;
            }
        }

        // 1c. Pre-populate method_receivers so emit_call can route obj.method() correctly
        for item in &module.items {
            if let Item::Fn(f) = item {
                if let Some(recv) = &f.receiver {
                    let is_instance = matches!(recv.kind, ReceiverKind::Instance);
                    self.method_receivers.insert(
                        f.name.clone(),
                        (recv.type_name.clone(), is_instance),
                    );
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
                }
            }
        }

        // 1d. Pre-populate generic_fns/generic_types sets for type-erased call site handling
        for item in &module.items {
            if let Item::Fn(f) = item {
                if !f.generics.is_empty() {
                    self.generic_fns.insert(f.name.clone());
                }
            }
            if let Item::Type(t) = item {
                if !t.generics.is_empty() {
                    self.generic_types.insert(t.name.clone());
                }
            }
        }

        // 2. Forward declarations for all functions (types are now known)
        for item in &module.items {
            if let Item::Fn(f) = item {
                self.emit_fn_forward_decl(f)?;
            }
        }
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

        // 6. Handler impl function bodies (ctx structs + bodies at file scope, after fn defs)
        if !self.deferred_impls.is_empty() {
            self.out.push_str(&self.deferred_impls.clone());
            self.out.push('\n');
        }

        self.emit_main_wrapper(module);
        Ok(self.out)
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
        self.line(&format!("static nova_unit nova_test_{}(void) {{", safe));
        self.indent = 1;
        self.emit_block_stmts(&t.body, "nova_unit")?;
        self.indent = 0;
        self.line("}");
        self.line("");
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
                return Ok(());
            }
        }
        // General case: emit as static const with initialiser expression
        // (covers int/bool/etc.)
        let val = self.emit_const_expr(&c.value)?;
        self.line(&format!("static const {} {} = {};", ty_c, c.name, val));
        Ok(())
    }

    /// Emit a constant expression — like emit_expr but without side-effect statements.
    /// Used for file-scope const initialisers.
    fn emit_const_expr(&mut self, expr: &Expr) -> Result<String, String> {
        match &expr.kind {
            ExprKind::IntLit(n) => Ok(format!("((nova_int){}LL)", n)),
            ExprKind::CharLit(cp) => Ok(format!("((nova_int){}LL)", cp)),
            ExprKind::BoolLit(b) => Ok(if *b { "1".into() } else { "0".into() }),
            ExprKind::StrLit(s) => {
                let len = s.len();
                Ok(format!("{{.ptr=\"{}\", .len={}}}", Self::escape_c_str(s), len))
            }
            ExprKind::FloatLit(f) => Ok(f.to_string()),
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
                    "Option" => Ok("NovaOpt_nova_int".into()),
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
                    _ => {
                        // Check if it's a type alias — return the aliased type directly (no *)
                        if let Some(aliased_c) = self.type_aliases.get(&name).cloned() {
                            return Ok(aliased_c);
                        }
                        // User-defined type — pointer to struct
                        let base = if generics.is_empty() {
                            format!("Nova_{}", name)
                        } else {
                            // Monomorphization not implemented yet — emit raw name
                            format!("Nova_{}", name)
                        };
                        Ok(format!("{}*", base))
                    }
                }
            }
            TypeRef::Unit(_) => Ok("nova_unit".into()),
            TypeRef::Array(inner, _) => {
                // str arrays use NovaArray_nova_str*; all others use nova_int storage.
                if matches!(inner.as_ref(), TypeRef::Named { path, .. } if path.len() == 1 && path[0] == "str") {
                    Ok("NovaArray_nova_str*".into())
                } else {
                    Ok("NovaArray_nova_int*".into())
                }
            }
            TypeRef::Tuple(elems, _) => {
                let n = elems.len();
                Ok(format!("_NovaTuple{}", n))
            }
            TypeRef::Func { .. } => {
                // Function type — use a void pointer as opaque representation
                Ok("void*".into())
            }
            TypeRef::FixedArray(_, _, _) => Err("fixed arrays not yet supported in codegen".into()),
        }
    }

    fn return_type_c(&self, f: &FnDecl) -> Result<String, String> {
        match &f.return_type {
            None => Ok("nova_unit".into()),
            Some(ty) => self.type_ref_to_c(ty),
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
            let handler_val = self.emit_expr(&binding.handler)?;
            let prev_var = self.fresh_tmp();
            self.line(&format!(
                "NovaVtable_{eff}* {prev} = _nova_handler_{eff};",
                eff = effect_name, prev = prev_var
            ));
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

        // Emit interrupt frame so `interrupt v` can early-exit this with-block
        let iframe = self.fresh_tmp();
        let result_tmp = self.fresh_tmp();
        self.line(&format!("NovaInterruptFrame {};", iframe));
        self.line(&format!("nova_int {};", result_tmp));
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
        // Emit block statements; if there's a trailing expr use it as the int result
        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            let trail_ty = self.infer_expr_c_type(trailing);
            let tv = self.emit_expr(trailing)?;
            if trail_ty == "nova_int" || trail_ty == "nova_bool" {
                self.line(&format!("{} = (nova_int)({});", result_tmp, tv));
            } else {
                self.line(&format!("(void)({});", tv));
                self.line(&format!("{} = ((nova_int)0LL);", result_tmp));
            }
        } else {
            self.line(&format!("{} = ((nova_int)0LL);", result_tmp));
        }
        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        // Interrupt path: read interrupt value
        self.line(&format!("{} = {iframe}.value;", result_tmp, iframe = iframe));
        self.indent -= 1;
        self.line("}");

        // Close fail-frame outer if we opened it
        if fframe.is_some() {
            self.indent -= 1;
            self.line("} else {");
            self.indent += 1;
            // Fail path: handler already ran; result is unit (0).
            self.line(&format!("{} = ((nova_int)0LL);", result_tmp));
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
        // Patch vtable at runtime
        self.line(&format!("{vt}->ctx = {ctx};",
            vt = vtable_var, ctx = ctx_var));
        for m in methods {
            let fn_name = format!("{}_impl_{}_{}", handler_id, eff, m.name);
            self.line(&format!("{vt}->{method} = {fn};",
                vt = vtable_var, method = m.name, fn = fn_name));
        }

        // ---- Emit forward declarations into deferred_impls (file scope) ----
        for m in methods {
            let (param_types, ret_ty) = schema.get(&m.name)
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
                    for stmt in &b.stmts {
                        self.emit_stmt(stmt)?;
                    }
                    let last_is_return = b.stmts.last()
                        .map(|s| matches!(s, Stmt::Return { .. }))
                        .unwrap_or(false);
                    if let Some(trailing) = &b.trailing {
                        let v = self.emit_expr(trailing)?;
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
                        // Explicit return already emitted — no additional return needed.
                    } else if ret_ty == "nova_unit" {
                        self.line("return NOVA_UNIT;");
                    } else {
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
        self.line(&format!("nova_fiber_spawn_into(&{q}, {id}, {ctx});",
            q = queue, id = spawn_id, ctx = ctx_var));

        // Emit the ctx-struct typedef into lambda_forward_decls — flushed before the
        // current function in `out`, so the typedef is visible at the spawn-instance
        // declaration site (and also for the entry fn body which lives in deferred_impls).
        let _ = writeln!(self.lambda_forward_decls, "typedef struct {{");
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
        // We need at least one field for empty-capture spawns to satisfy MSVC.
        if captures.is_empty() && parfor_slot.is_none() {
            let _ = writeln!(self.lambda_forward_decls, "    char _nova_dummy;");
        }
        let _ = writeln!(self.lambda_forward_decls, "}} {};", ctx_ty);

        // Swap out to deferred_impls for body emission
        let saved_out    = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;

        self.line(&format!("static void {}(mco_coro* _co) {{", spawn_id));
        self.indent += 1;
        if !captures.is_empty() || parfor_slot.is_some() {
            self.line(&format!("{ctx}* _c = ({ctx}*)mco_get_user_data(_co);", ctx = ctx_ty));
        } else {
            // Even with no captures, mco_get_user_data is needed if we activate captures —
            // but we don't here, so just consume the parameter.
            self.line("(void)_co;");
        }
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
        // Report error to the scope. error_msg.ptr lives on heap or static — safe.
        self.line("nova_fiber_report_error(_ff.error_msg.ptr);");
        self.indent -= 1;
        self.line("}");
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
    fn emit_supervised(&mut self, body: &Block) -> Result<String, String> {
        let id = self.supervised_counter;
        self.supervised_counter += 1;
        let queue_var = format!("_nova_scope_q_{}", id);
        let prev_scope_var = format!("_nova_prev_scope_{}", id);

        // Wrap the scope in a C block so the queue is local.
        self.line("{");
        self.indent += 1;
        self.line(&format!("NovaFiberQueue {} = {{0}};", queue_var));
        self.line(&format!("nova_scope_init(&{});", queue_var));

        // Set _nova_active_scope to this queue so that on main-flow,
        // Time.sleep (default handler) finds the right scope to drive.
        // Saved/restored around the body.
        self.line(&format!("NovaFiberQueue* {} = _nova_active_scope;", prev_scope_var));
        self.line(&format!("_nova_active_scope = &{};", queue_var));

        // Activate scope: spawn inside body routes into queue.
        let prev = std::mem::replace(&mut self.current_scope_queue, Some(queue_var.clone()));

        // Emit body statements (trailing value is discarded — supervised yields unit).
        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            let v = self.emit_expr(trailing)?;
            self.line(&format!("(void)({});", v));
        }

        // Restore scope state.
        self.current_scope_queue = prev;

        // Run the scheduler: round-robin until all fibers in queue are dead.
        self.line(&format!("nova_supervised_run(&{});", queue_var));
        // Restore previous active scope (may be NULL or outer scope).
        self.line(&format!("_nova_active_scope = {};", prev_scope_var));

        self.indent -= 1;
        self.line("}");

        // supervised expression evaluates to unit.
        Ok("NOVA_UNIT".to_string())
    }

    /// Emit `cancel_scope { tok => body }` — D75 manual structured cancellation.
    /// Like `emit_supervised` but binds a `NovaCancelToken*` variable named
    /// `token_name` for the body to capture into spawns. External code that
    /// holds the token can call `nova_cancel_token_cancel()` to fail-fast.
    fn emit_cancel_scope(&mut self, token_name: &str, body: &Block) -> Result<String, String> {
        let id = self.supervised_counter;
        self.supervised_counter += 1;
        let queue_var = format!("_nova_scope_q_{}", id);
        let prev_scope_var = format!("_nova_prev_scope_{}", id);
        let tok_var = token_name.to_string();

        // The token must be heap-allocated: its address is captured by spawns
        // (by-pointer capture for non-scalars), but the queue lives on the
        // stack inside the supervised C-block. Spawns may run after we've
        // finished emitting body — they just resume in-place. The token
        // pointing into the same C-frame's stack is fine; we keep the
        // C-block open until nova_supervised_run returns.
        self.line("{");
        self.indent += 1;
        self.line(&format!("NovaFiberQueue {} = {{0}};", queue_var));
        self.line(&format!("nova_scope_init(&{});", queue_var));
        self.line(&format!(
            "NovaCancelToken* {} = (NovaCancelToken*)nova_alloc(sizeof(NovaCancelToken));",
            tok_var
        ));
        self.line(&format!("nova_cancel_token_init({}, &{});", tok_var, queue_var));

        self.line(&format!("NovaFiberQueue* {} = _nova_active_scope;", prev_scope_var));
        self.line(&format!("_nova_active_scope = &{};", queue_var));

        // Register the token in var_types so spawns capturing it work.
        // Type "NovaCancelToken*" is a pointer; spawn-capture treats it
        // by-pointer. We also mark non-mut so it's pass-by-pointer once
        // (token is immutable handle; the *scope it points to* is what mutates).
        self.var_types.insert(tok_var.clone(), "NovaCancelToken*".to_string());
        self.var_mutable.remove(&tok_var);

        let prev = std::mem::replace(&mut self.current_scope_queue, Some(queue_var.clone()));

        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            let v = self.emit_expr(trailing)?;
            self.line(&format!("(void)({});", v));
        }

        self.current_scope_queue = prev;
        self.line(&format!("nova_supervised_run(&{});", queue_var));
        self.line(&format!("_nova_active_scope = {};", prev_scope_var));

        // Drop the binding from var_types (scope-local).
        self.var_types.remove(&tok_var);

        self.indent -= 1;
        self.line("}");

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
                ExprKind::For { pattern: pattern.clone(), iter: Box::new(iter.clone()), body: for_body },
                span,
            );
            let supervised_block = Block { stmts: vec![Stmt::Expr(for_expr)], trailing: None, span };
            return self.emit_supervised(&supervised_block);
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
                    ExprKind::For { pattern: pattern.clone(), iter: Box::new(iter.clone()), body: for_body },
                    span,
                );
                let supervised_block = Block { stmts: vec![Stmt::Expr(for_expr)], trailing: None, span };
                return self.emit_supervised(&supervised_block);
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
                        ExprKind::For { pattern: pattern.clone(), iter: Box::new(iter.clone()), body: for_body },
                        span,
                    );
                    let supervised_block = Block { stmts: vec![Stmt::Expr(for_expr)], trailing: None, span };
                    return self.emit_supervised(&supervised_block);
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
                    ExprKind::For { pattern: pattern.clone(), iter: Box::new(iter.clone()), body: for_body },
                    span,
                );
                let supervised_block = Block { stmts: vec![Stmt::Expr(for_expr)], trailing: None, span };
                return self.emit_supervised(&supervised_block);
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
        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            let v = self.emit_expr(trailing)?;
            self.line(&format!("(void)({});", v));
        }
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
                    let (param_types, ret_ty) = schema.get(&m.name)
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
            ExprKind::Spawn(_body) => {
                let spawn_id = format!("_nova_spawn_{}", *s);
                *s += 1;
                self.line(&format!("static void {}(mco_coro* _co);", spawn_id));
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
                    self.scan_expr_fwd(&b.handler, h, s)?;
                }
                self.scan_block_fwd(body, h, s)?;
            }
            ExprKind::Call { func, args, .. } => {
                self.scan_expr_fwd(func, h, s)?;
                for a in args { self.scan_expr_fwd(a, h, s)?; }
            }
            ExprKind::Binary { left, right, .. } => {
                self.scan_expr_fwd(left, h, s)?;
                self.scan_expr_fwd(right, h, s)?;
            }
            ExprKind::While { cond, body } => {
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
            ExprKind::Loop { body } => {
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
            ExprKind::Supervised(b) => self.scan_block_fwd(b, h, s)?,
            ExprKind::Detach(b) => self.scan_block_fwd(b, h, s)?,
            ExprKind::CancelScope { body, .. } => self.scan_block_fwd(body, h, s)?,
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
            ExprKind::For { pattern, iter, body }
            | ExprKind::ParallelFor { pattern, iter, body } => {
                Self::collect_bound_names_expr(iter, out);
                Self::collect_bound_names_pattern(pattern, out);
                Self::collect_bound_names_block(body, out);
            }
            ExprKind::While { body, .. } => Self::collect_bound_names_block(body, out),
            ExprKind::WhileLet { pattern, body, .. } => {
                Self::collect_bound_names_pattern(pattern, out);
                Self::collect_bound_names_block(body, out);
            }
            ExprKind::Loop { body } => Self::collect_bound_names_block(body, out),
            ExprKind::With { body, .. } => Self::collect_bound_names_block(body, out),
            ExprKind::Supervised(body) => Self::collect_bound_names_block(body, out),
            ExprKind::Detach(body) => Self::collect_bound_names_block(body, out),
            ExprKind::CancelScope { token_name, body } => {
                out.insert(token_name.clone());
                Self::collect_bound_names_block(body, out);
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
                for a in args { Self::collect_idents_expr(a, out); }
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
            ExprKind::While { cond, body } => {
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
            ExprKind::Loop { body } => Self::collect_idents_block(body, out),
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
            ExprKind::Try(e) | ExprKind::As(e, _) | ExprKind::Is(e, _) => {
                Self::collect_idents_expr(e, out);
            }
            ExprKind::Interrupt(Some(v)) => Self::collect_idents_expr(v, out),
            ExprKind::Block(b) => Self::collect_idents_block(b, out),
            ExprKind::Supervised(b) => Self::collect_idents_block(b, out),
            ExprKind::Detach(b) => Self::collect_idents_block(b, out),
            ExprKind::CancelScope { body, .. } => Self::collect_idents_block(body, out),
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
        if f.name == "main" {
            return Ok(());
        }
        // Generic free functions: emit erased forward decl (void* params, void* return)
        if !f.generics.is_empty() && f.receiver.is_none() {
            let mangled = self.mangle_fn(f);
            let type_params: HashSet<String> = f.generics.iter().cloned().collect();
            let params_str = if f.params.is_empty() {
                "void".to_string()
            } else {
                f.params.iter().map(|p| {
                    match &p.ty {
                        TypeRef::Named { path, generics, .. } => {
                            let name = path.join("_");
                            if type_params.contains(&name) { "void*".into() }
                            else if !generics.is_empty() && self.record_schemas.contains_key(&name) {
                                format!("Nova_{}*", name)
                            } else { "void*".into() }
                        }
                        _ => "void*".into(),
                    }
                }).collect::<Vec<_>>().join(", ")
            };
            self.line(&format!("static void* {}({});", mangled, params_str));
            // Register erased return type as void* (call sites must cast)
            self.var_types.insert(format!("fn_ret_{}", f.name), "void*".into());
            // Track tuple return arity so call sites can populate tuple_element_types
            if let Some(TypeRef::Tuple(elems, _)) = &f.return_type {
                self.generic_fn_tuple_arity.insert(f.name.clone(), elems.len());
            }
            return Ok(());
        }
        // Skip generic methods on generic types — no monomorphization support
        if !f.generics.is_empty() {
            return Ok(());
        }
        if let Some(recv) = &f.receiver {
            if !recv.generics.is_empty() {
                // Generic method: emit erased forward decl
                let type_params: HashSet<String> = recv.generics.iter().filter_map(|tr| {
                    if let TypeRef::Named { path, .. } = tr { path.first().cloned() } else { None }
                }).collect();
                let mangled = self.mangle_fn(f);
                let ret_c = self.erased_type_ref_c(&f.return_type, &type_params);
                let mut parts = vec![format!("{} nova_self", self.receiver_c_type(&recv.type_name))];
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
        self.line(&format!("static {} {}({});", ret, mangled, params));
        Ok(())
    }

    // ---- type declarations ----

    fn emit_type_decl(&mut self, t: &TypeDecl) -> Result<(), String> {
        // Generic record types: emit with type-erased fields (nova_int for all type params)
        if !t.generics.is_empty() {
            if let TypeDeclKind::Record(fields) = &t.kind {
                // Collect type parameter names to identify erased fields
                let type_params: HashSet<String> = t.generics.iter().cloned().collect();
                // Emit erased struct: type-param fields become void*, others keep concrete type
                let mut field_c_pairs: Vec<(String, String)> = Vec::new();
                for f in fields {
                    let c_ty = match &f.ty {
                        TypeRef::Named { path, .. } if path.len() == 1 && type_params.contains(&path[0]) =>
                            "void*".to_string(),
                        TypeRef::Array(inner, _) if matches!(inner.as_ref(),
                            TypeRef::Named { path, .. } if path.len() == 1 && type_params.contains(&path[0])) =>
                            "NovaArray_nova_int*".to_string(),
                        _ => self.type_ref_to_c(&f.ty).unwrap_or_else(|_| "nova_int".into()),
                    };
                    field_c_pairs.push((c_ty, f.name.clone()));
                }
                // Emit the struct directly
                let mut schema = HashMap::new();
                self.line(&format!("typedef struct Nova_{0} Nova_{0};", t.name));
                self.line(&format!("struct Nova_{} {{", t.name));
                self.indent += 1;
                for (c_ty, fname) in &field_c_pairs {
                    self.line(&format!("{} {};", c_ty, fname));
                    schema.insert(fname.clone(), c_ty.clone());
                }
                self.indent -= 1;
                self.line("};");
                self.line("");
                self.record_schemas.insert(t.name.clone(), schema);
            }
            // Skip generic sum types and others — still no support
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
        }
        Ok(())
    }

    fn emit_effect_type(&mut self, name: &str, methods: &[EffectMethod]) -> Result<(), String> {
        // 1. Vtable struct: one fn ptr per method, plus void* ctx
        self.line(&format!("typedef struct {{"));
        self.indent += 1;
        self.line("void* ctx;");
        let mut schema: HashMap<String, (Vec<String>, String)> = HashMap::new();
        for m in methods {
            let ret = match &m.return_type {
                None => "nova_unit".to_string(),
                Some(t) => self.type_ref_to_c(t)?,
            };
            let mut param_types = vec!["void*".to_string()]; // ctx first
            for p in &m.params {
                param_types.push(self.type_ref_to_c(&p.ty)?);
            }
            let params_sig = param_types.join(", ");
            self.line(&format!("{} (*{})({}); ", ret, m.name, params_sig));
            schema.insert(m.name.clone(), (param_types[1..].to_vec(), ret));
        }
        self.indent -= 1;
        self.line(&format!("}} NovaVtable_{};", name));
        self.line("");

        // 2. Thread-local handler slot
        self.line(&format!(
            "__declspec(thread) static NovaVtable_{name}* _nova_handler_{name} = NULL;",
            name = name
        ));
        self.line("");

        // 3. Dispatch helpers: Nova_Counter_next() calls through vtable
        for m in methods {
            let (param_types, ret) = schema.get(&m.name).unwrap();
            let mut fn_params: Vec<String> = Vec::new();
            let mut call_args: Vec<String> = vec!["_nova_handler_{name}->ctx".to_string()];
            for (i, (p, ty)) in m.params.iter().zip(param_types.iter()).enumerate() {
                fn_params.push(format!("{} {}", ty, p.name));
                call_args.push(p.name.clone());
                let _ = i;
            }
            let fn_params_str = if fn_params.is_empty() {
                "void".to_string()
            } else {
                fn_params.join(", ")
            };
            let call_args_str = call_args.join(", ");
            // Replace {name} placeholder in call_args[0]
            let call_args_str = call_args_str.replace(
                "_nova_handler_{name}->ctx",
                &format!("_nova_handler_{name}->ctx", name = name)
            );
            self.line(&format!(
                "static inline {ret} Nova_{name}_{method}({params}) {{",
                ret = ret, name = name, method = m.name, params = fn_params_str
            ));
            self.indent += 1;
            self.line(&format!(
                "return _nova_handler_{name}->{method}({args});",
                name = name, method = m.name, args = call_args_str
            ));
            self.indent -= 1;
            self.line("}");
            self.line("");
        }

        self.effect_schemas.insert(name.to_string(), schema);
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
            self.line(&format!("{} {};", ty_c, f.name));
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
                        self.line(&format!("{} {};", tc, f.name));
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
                    // Named fields — assign by field name, not positional index
                    for (i, f) in fields.iter().enumerate() {
                        self.line(&format!("_r->payload.{var}.{fname} = _{i};",
                            var = v.name, fname = f.name, i = i));
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

    // ---- function emission ----

    fn mangle_fn(&self, f: &FnDecl) -> String {
        if let Some(recv) = &f.receiver {
            match recv.kind {
                ReceiverKind::Instance => format!("Nova_{}_method_{}", recv.type_name, f.name),
                ReceiverKind::Static   => format!("Nova_{}_static_{}", recv.type_name, f.name),
            }
        } else {
            format!("nova_fn_{}", f.name)
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
            other => format!("Nova_{}*", other),
        }
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

    /// Emit a type-erased version of a generic instance method.
    /// Type params in recv.generics map to nova_int.
    fn emit_generic_method_erased(&mut self, f: &FnDecl) -> Result<(), String> {
        let recv = f.receiver.as_ref().unwrap();
        let type_params: HashSet<String> = recv.generics.iter().filter_map(|tr| {
            if let TypeRef::Named { path, .. } = tr { path.first().cloned() } else { None }
        }).collect();
        let mangled = self.mangle_fn(f);
        let ret_c = self.erased_type_ref_c(&f.return_type, &type_params);
        // Build params: nova_self + erased params
        let recv_c = self.receiver_c_type(&recv.type_name);
        let mut parts = vec![format!("{} nova_self", recv_c)];
        for p in &f.params {
            let p_c = self.erased_type_ref_c(&Some(p.ty.clone()), &type_params);
            parts.push(format!("{} {}", p_c, p.name));
        }
        let params_s = if parts.is_empty() { "void".into() } else { parts.join(", ") };
        self.line(&format!("static {} {}({}) {{", ret_c, mangled, params_s));
        self.indent += 1;
        // Register nova_self and params
        self.var_types.insert("nova_self".into(), recv_c.clone());
        let saved: Vec<(String, Option<String>)> = f.params.iter().map(|p| {
            let p_c = self.erased_type_ref_c(&Some(p.ty.clone()), &type_params);
            (p.name.clone(), self.var_types.insert(p.name.clone(), p_c))
        }).collect();
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
        }
        self.expected_record_type = saved_expected;
        // Restore params
        for (name, prev) in saved {
            match prev {
                Some(old) => { self.var_types.insert(name, old); }
                None => { self.var_types.remove(&name); }
            }
        }
        self.var_types.remove("nova_self");
        self.current_receiver_type = None;
        self.indent -= 1;
        self.line("}");
        self.line("");
        Ok(())
    }

    /// All type parameters map to void*. The body is emitted with type params erased.
    fn emit_generic_fn_erased(&mut self, f: &FnDecl) -> Result<(), String> {
        let mangled = self.mangle_fn(f);
        let type_params: HashSet<String> = f.generics.iter().cloned().collect();
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
                for stmt in &block.stmts {
                    self.emit_stmt(stmt)?;
                }
                if let Some(trailing) = &block.trailing {
                    self.emit_source_annotation_for_expr(trailing);
                    let val_ty = self.infer_expr_c_type(trailing);
                    let val = self.emit_expr(trailing)?;
                    emit_erased_return(self, &val, &val_ty);
                } else {
                    self.line("return NULL;");
                }
            }
        }
        // Restore param types
        for (name, prev) in saved {
            match prev {
                Some(old) => { self.var_types.insert(name, old); }
                None => { self.var_types.remove(&name); }
            }
        }
        self.indent -= 1;
        self.line("}");
        self.line("");
        Ok(())
    }

    fn emit_fn(&mut self, f: &FnDecl) -> Result<(), String> {
        if f.name == "main" {
            return self.emit_nova_main(f);
        }
        // Generic free functions: emit void*-erased stub (type erasure)
        if !f.generics.is_empty() && f.receiver.is_none() {
            return self.emit_generic_fn_erased(f);
        }
        if let Some(recv) = &f.receiver {
            if !recv.generics.is_empty() {
                // Generic methods: emit with type-erased params (nova_int for type params)
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
        self.current_fn_return_ty = Some(ret.clone());
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
                    let param_c_tys: Vec<String> = fp.iter()
                        .map(|t| self.type_ref_to_c(t).unwrap_or_else(|_| "nova_int".into()))
                        .collect();
                    let ret_c = match return_type {
                        Some(rt) => self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into()),
                        None => "nova_unit".into(),
                    };
                    self.fn_param_sigs.insert(p.name.clone(), (param_c_tys, ret_c));
                }
                // Register element type for array params of non-primitive types
                if let TypeRef::Array(inner, _) = &p.ty {
                    if let Ok(elem_ty) = self.type_ref_to_c(inner) {
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
        let saved_expected = self.expected_record_type.clone();
        self.expected_record_type = Self::struct_name_from_c_type(&ret);
        match &f.body {
            FnBody::Expr(e) => {
                self.emit_source_annotation_for_expr(e);
                let val = self.emit_expr(e)?;
                if ret == "nova_unit" {
                    self.line(&format!("{};", val));
                    self.line("return NOVA_UNIT;");
                } else {
                    self.line(&format!("return {};", val));
                }
            }
            FnBody::Block(block) => {
                self.emit_block_stmts(block, &ret)?;
            }
        }
        self.expected_record_type = saved_expected;
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
        self.line("static nova_unit nova_fn_main_impl(void) {");
        self.indent += 1;
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
        }
        self.indent -= 1;
        self.line("}");
        self.line("");
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
        self.line("nova_fn_main_impl();");
        self.line("nova_gc_shutdown();");
        self.line("return 0;");
        self.indent -= 1;
        self.line("}");
    }

    // ---- block / statements ----

    fn emit_block_stmts(&mut self, block: &Block, ret_ty: &str) -> Result<(), String> {
        for stmt in &block.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &block.trailing {
            self.emit_source_annotation_for_expr(trailing);
            let val = self.emit_expr(trailing)?;
            if ret_ty == "nova_unit" {
                self.line(&format!("{};", val));
                self.line("return NOVA_UNIT;");
            } else {
                self.line(&format!("return {};", val));
            }
        } else if ret_ty == "nova_unit" {
            self.line("return NOVA_UNIT;");
        }
        Ok(())
    }

    fn emit_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        // Source annotation hook: if --annotate-source enabled, emit the
        // originating Nova source as a /* SRC: ... */ comment.
        self.emit_source_annotation_for_stmt(stmt);
        match stmt {
            Stmt::Let(decl) => {
                // Special case: tuple destructure  `let (a, b, c) = expr`
                if let Pattern::Tuple(pats, _) = &decl.pattern {
                    return self.emit_tuple_destructure(pats, &decl.value);
                }
                // Infer type BEFORE emitting so record literals get the right type
                let binding = self.pattern_binding(&decl.pattern)?;
                let ty_c = if let Some(ty) = &decl.ty {
                    self.type_ref_to_c(ty)?
                } else {
                    self.infer_expr_c_type(&decl.value)
                };
                let val = self.emit_expr(&decl.value)?;
                // For pointer types: the emitted tmp expression already carries the type.
                // Just declare the binding with the right type.
                self.var_types.insert(binding.clone(), ty_c.clone());
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
                // Special case: `let xs = s.bytes()` / `s.chars()` — set element type
                // explicitly, even though val is not a known variable.
                if let ExprKind::Call { func, .. } = &decl.value.kind {
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
                // If RHS is a lambda, register the binding in fn_param_sigs so inc(5) works
                if let ExprKind::Lambda { params, .. } = &decl.value.kind {
                    let param_c_tys: Vec<String> = params.iter().map(|p| {
                        if let Some(ty) = &p.ty { self.type_ref_to_c(ty).unwrap_or_else(|_| "nova_int".into()) }
                        else { "nova_int".into() }
                    }).collect();
                    // Infer return type from declared fn type or default to nova_int
                    let ret_c = if let Some(TypeRef::Func { return_type, .. }) = decl.ty.as_ref() {
                        return_type.as_ref().map(|rt| self.type_ref_to_c(rt).unwrap_or_else(|_| "nova_int".into()))
                            .unwrap_or_else(|| "nova_int".into())
                    } else { "nova_int".into() };
                    self.fn_param_sigs.insert(binding.clone(), (param_c_tys, ret_c));
                }
                // If RHS is a call to a function that returns fn(...), propagate closure sig to binding
                if let ExprKind::Call { func, args, .. } = &decl.value.kind {
                    if let ExprKind::Ident(fname) = &func.kind {
                        if let Some(sig) = self.fn_returns_fn_sig.get(fname).cloned() {
                            self.fn_param_sigs.insert(binding.clone(), sig);
                        }
                        // If RHS is a call to a generic fn returning a tuple, infer element types from args
                        if let Some(&arity) = self.generic_fn_tuple_arity.get(fname.as_str()) {
                            let elem_tys: Vec<String> = args.iter().take(arity)
                                .map(|a| self.infer_expr_c_type(a))
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
                self.line(&format!("{};", val));
            }
            Stmt::Assign { target, op, value, .. } => {
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
                if let Some(v) = value {
                    let val = self.emit_expr(v)?;
                    self.line(&format!("return {};", val));
                } else {
                    self.line("return NOVA_UNIT;");
                }
            }
            Stmt::Break(_) => self.line("break;"),
            Stmt::Continue(_) => self.line("continue;"),
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
        }
        Ok(())
    }

    // ---- expressions ----

    fn emit_expr(&mut self, expr: &Expr) -> Result<String, String> {
        match &expr.kind {
            ExprKind::IntLit(n)   => Ok(format!("((nova_int){}LL)", n)),
            ExprKind::CharLit(cp) => Ok(format!("((nova_int){}LL)", cp)),
            ExprKind::FloatLit(f) => Ok(format!("((nova_f64){})", f)),
            ExprKind::BoolLit(b)  => Ok(if *b { "true".into() } else { "false".into() }),
            ExprKind::UnitLit     => Ok("NOVA_UNIT".into()),
            ExprKind::StrLit(s)   => {
                let escaped = Self::escape_c_str(s);
                Ok(format!("(nova_str){{.ptr=\"{}\", .len={}}}", escaped, s.len()))
            }

            ExprKind::Ident(name) => {
                // Unit variants (e.g. `Red` from `type Color | Red | Green`) are
                // not function calls in Nova but need `nova_make_Color_Red()` in C.
                if let Some((type_name, fields)) = self.find_variant(name) {
                    if fields.is_empty() {
                        return Ok(format!("nova_make_{}_{}()", type_name, name));
                    }
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
                Ok(name.clone())
            }
            ExprKind::Path(parts) => Ok(parts.join("_")),

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
                            _ => Ok("(0)".into()),
                        };
                    } else {
                        // Both void* or unknown type — comparison is meaningless
                        return Ok("(0)".into());
                    }
                }
                // nova_str is a struct — can't use == directly
                if lty == "nova_str" || rty == "nova_str" {
                    return match op {
                        BinOp::Eq  => Ok(format!("(nova_str_eq({}, {}))", l, r)),
                        BinOp::Neq => Ok(format!("(!nova_str_eq({}, {}))", l, r)),
                        BinOp::Add => Ok(format!("(nova_str_concat({}, {}))", l, r)),
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
                    let elem_ty = lty.strip_prefix("NovaOpt_")
                        .or_else(|| rty.strip_prefix("NovaOpt_"))
                        .unwrap_or("nova_int");
                    return match op {
                        BinOp::Eq  => Ok(format!("(nova_opt_eq_{}({}, {}))", elem_ty, l, r)),
                        BinOp::Neq => Ok(format!("(!nova_opt_eq_{}({}, {}))", elem_ty, l, r)),
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
                    if matches!(op, BinOp::Eq | BinOp::Neq) {
                        let type_name = sty.strip_prefix("Nova_").unwrap_or("").trim_end_matches('*').to_string();
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

            ExprKind::Call { func, args, trailing_block } => {
                self.emit_call_with_trailing(func, args, trailing_block.as_ref())
            }

            ExprKind::Member { obj, name } => {
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
                let field_name = name.clone();
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

            ExprKind::For { pattern, iter, body } => {
                self.emit_for(pattern, iter, body)
            }

            ExprKind::While { cond, body } => {
                let cond_val = self.emit_expr(cond)?;
                let tmp = self.fresh_tmp_named("while");
                self.line(&format!("nova_unit {};", tmp));
                self.line(&format!("while ({}) {{", cond_val));
                self.indent += 1;
                for stmt in &body.stmts {
                    self.emit_stmt(stmt)?;
                }
                self.indent -= 1;
                self.line("}");
                self.line(&format!("{} = NOVA_UNIT;", tmp));
                Ok(tmp)
            }

            ExprKind::Loop { body } => {
                let tmp = self.fresh_tmp_named("loop");
                self.line(&format!("nova_unit {};", tmp));
                self.line("for (;;) {");
                self.indent += 1;
                for stmt in &body.stmts {
                    self.emit_stmt(stmt)?;
                }
                self.indent -= 1;
                self.line("}");
                self.line(&format!("{} = NOVA_UNIT;", tmp));
                Ok(tmp)
            }

            ExprKind::Match { scrutinee, arms } => {
                self.emit_match(scrutinee, arms)
            }

            ExprKind::Range { start, end, inclusive } => {
                // Range emitted as a struct literal — stdlib not linked in Phase 1
                // Just produce a placeholder that compiles
                let s = self.emit_expr(start)?;
                let e = self.emit_expr(end)?;
                let _ = inclusive;
                Ok(format!("/*range({}, {})*/NOVA_UNIT", s, e))
            }

            ExprKind::RecordLit { type_name, fields } => {
                self.emit_record_lit(type_name.as_deref(), fields)
            }

            ExprKind::ArrayLit(elems) => {
                self.emit_array_lit(elems)
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
                    // Option?: if None, return None; else extract value
                    self.line(&format!("{} {} = {};", inner_ty, try_tmp, val));
                    self.line(&format!("if ({}.tag == NOVA_TAG_Option_None) {{ return nova_make_Option_None(); }}", try_tmp));
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

            ExprKind::As(inner, _ty) => {
                // Type cast — emit without cast for now (types are compatible in C)
                self.emit_expr(inner)
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

            ExprKind::Lambda { params, body, .. } => {
                self.emit_lambda(params, body, None)
            }
            ExprKind::With { bindings, body } => {
                self.emit_with(bindings, body)
            }
            ExprKind::HandlerLit { effect_name, methods } => {
                self.emit_handler_lit(effect_name, methods)
            }
            ExprKind::Interrupt(val) => {
                // interrupt v — longjmp to the nearest with-block via nova_interrupt()
                // nova_interrupt принимает nova_int, поэтому unit-значение (() или
                // отсутствие val) кодируется как 0. Cast struct'а NOVA_UNIT
                // невалиден, поэтому detect'им UnitLit отдельно.
                let int_val = match val.as_deref().map(|e| &e.kind) {
                    None | Some(ExprKind::UnitLit) => "((nova_int)0LL)".to_string(),
                    Some(_) => {
                        let v = val.as_deref().unwrap();
                        let vstr = self.emit_expr(v)?;
                        // Если значение — nova_unit (например блок без trailing-value),
                        // cast'им через 0; иначе обычный numeric cast.
                        let v_ty = self.infer_expr_c_type(v);
                        if v_ty == "nova_unit" {
                            format!("((void)({}), (nova_int)0LL)", vstr)
                        } else {
                            format!("(nova_int)({})", vstr)
                        }
                    }
                };
                self.line(&format!("nova_interrupt({});", int_val));
                // After interrupt the code is unreachable, but emit a dummy value
                Ok("NOVA_UNIT".into())
            }
            ExprKind::Throw(value) => {
                // D25/D65: throw в expression-position. Тип Never — control
                // никогда не вернётся. Эмитируем effect-call Nova_Fail_fail
                // как statement, потом dummy zero-литерал нужного типа,
                // чтобы C-выражение было валидным. Тип-target не известен
                // здесь точно — берём nova_int (cast'ы в caller сделают
                // остальное).
                let v = self.emit_expr(value)?;
                self.line(&format!("Nova_Fail_fail({});", v));
                // Unreachable, но возвращаем синтаксически валидный dummy.
                Ok("((nova_int)0LL)".to_string())
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
            ExprKind::Supervised(body) => {
                self.emit_supervised(body)
            }
            ExprKind::Detach(body) => {
                self.emit_detach(body)
            }
            ExprKind::CancelScope { token_name, body } => {
                self.emit_cancel_scope(token_name, body)
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
                if obj_ty.starts_with("NovaArray_") {
                    let o = self.emit_expr(obj)?;
                    // Check if elements are pointer types stored as nova_int (e.g. inner arrays or records)
                    let arr_var_name = if let ExprKind::Ident(n) = &obj.kind { Some(n.as_str()) } else { None };
                    let inner_elem_ty = arr_var_name
                        .and_then(|n| self.array_element_types.get(n).cloned());
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
                for stmt in &then.stmts { self.emit_stmt(stmt)?; }
                if let Some(trailing) = &then.trailing {
                    let v = self.emit_expr(trailing)?;
                    self.line(&format!("{} = {};", result_tmp, v));
                }
                self.indent -= 1;
                match else_ {
                    Some(ElseBranch::Block(b)) => {
                        self.line("} else {");
                        self.indent += 1;
                        for stmt in &b.stmts { self.emit_stmt(stmt)?; }
                        if let Some(trailing) = &b.trailing {
                            let v = self.emit_expr(trailing)?;
                            self.line(&format!("{} = {};", result_tmp, v));
                        }
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
            ExprKind::WhileLet { pattern, scrutinee, body } => {
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
                for stmt in &body.stmts { self.emit_stmt(stmt)?; }
                if let Some(trailing) = &body.trailing {
                    let _ = self.emit_expr(trailing)?;
                }
                self.indent -= 1;
                self.line("}");
                self.line(&format!("{} = NOVA_UNIT;", loop_tmp));
                Ok(loop_tmp)
            }
        }
    }

    // ---- call emission ----

    /// Wrapper for emit_call that handles trailing blocks.
    /// A trailing block is emitted as a static C function and passed as an extra argument.
    fn emit_call_with_trailing(&mut self, func: &Expr, args: &[Expr], trailing: Option<&TrailingBlock>) -> Result<String, String> {
        if let Some(tb) = trailing {
            // Generate a unique name for the trailing block function
            let id = self.trailing_block_counter;
            self.trailing_block_counter += 1;
            let fn_name = format!("nova_trailing_block_{}", id);

            // Determine return type from current function's return type context
            // (trailing block inherits return type of its enclosing call's expected type).
            // For simplicity: look up fn-param signature for the function being called.
            let (param_c_tys, ret_c_ty) = self.infer_trailing_block_sig(func, tb);

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
            // Emit a forward declaration for the body function
            let fwd = format!("static {} {}({});", ret_c_ty, fn_name, body_param_list);
            self.line(&fwd);

            // Wrap in a NovaClos_XX struct so fn_param_sigs call mechanism works
            let clos_struct = Self::clos_struct_name(&param_c_tys, &ret_c_ty);
            let clos_fn_ty = Self::clos_fn_ty(&param_c_tys, &ret_c_ty);
            let clos_tmp = self.fresh_tmp();
            self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", clos_struct, clos_tmp, clos_struct, clos_struct));
            self.line(&format!("{}->{} = ({})({});", clos_tmp, "fn", clos_fn_ty, fn_name));
            self.line(&format!("{}->{} = (void*)0;", clos_tmp, "env"));

            // Emit the call with closure pointer as extra arg
            let func_c = self.infer_func_c_name(func);
            let mut arg_strs: Vec<String> = Vec::new();
            for a in args { arg_strs.push(self.emit_expr(a)?); }
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
        for stmt in &block.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &block.trailing {
            let v = self.emit_expr(trailing)?;
            if ret_ty == "nova_unit" {
                self.line(&format!("{};", v));
                self.line("return NOVA_UNIT;");
            } else {
                self.line(&format!("return {};", v));
            }
        } else {
            if ret_ty == "nova_unit" {
                self.line("return NOVA_UNIT;");
            }
        }
        Ok(())
    }

    fn emit_call(&mut self, func: &Expr, args: &[Expr]) -> Result<String, String> {
        // Special case: println / print builtins
        if let ExprKind::Ident(name) = &func.kind {
            if name == "println" || name == "print" {
                return self.emit_println(args, name == "println");
            }
            // D70 `to_str(x)` builtin removed (REPLACED → D73). String
            // conversion now via `str.from(x)` / `x.@into()` (with str-context).
            // assert(cond) → nova_assert(cond, "condition text")
            if name == "assert" {
                if let Some(cond_expr) = args.first() {
                    let cond_val = self.emit_expr(cond_expr)?;
                    let cond_text = Self::expr_to_display(cond_expr);
                    let escaped_text = Self::escape_c_str(&cond_text);
                    return Ok(format!("nova_assert({}, \"{}\")", cond_val, escaped_text));
                }
            }
        }

        // Mangle user-defined function calls: `foo(...)` → `nova_fn_foo(...)`
        // But variant constructors: `Circle(r)` → `nova_make_Shape_Circle(r)`
        // And effect operations: `Counter.next()` → `Nova_Counter_next()`
        // Check if func is a function-typed parameter (closure call via NovaClos_XX macro)
        if let ExprKind::Ident(name) = &func.kind {
            if let Some((param_tys, ret_ty)) = self.fn_param_sigs.get(name).cloned() {
                let mut arg_strs = Vec::new();
                for a in args { arg_strs.push(self.emit_expr(a)?); }
                // Determine which NovaClos macro to use based on (params, ret) types
                let macro_name = Self::clos_call_macro(&param_tys, &ret_ty);
                return match macro_name {
                    Some(m) => {
                        if arg_strs.is_empty() {
                            Ok(format!("{}({})", m, name))
                        } else {
                            Ok(format!("{}({}, {})", m, name, arg_strs.join(", ")))
                        }
                    }
                    None => {
                        // Fallback: cast to function pointer directly (for plain fn ptrs, no closure)
                        let cast_params = if param_tys.is_empty() { "void".to_string() } else { param_tys.join(", ") };
                        Ok(format!("(({ret}(*)({params}))({name}))({args})",
                            ret = ret_ty, params = cast_params, name = name, args = arg_strs.join(", ")))
                    }
                };
            }
        }

        let func_c = match &func.kind {
            ExprKind::Ident(name) => {
                if let Some((type_name, _)) = self.find_variant(name) {
                    format!("nova_make_{}_{}", type_name, name)
                } else {
                    format!("nova_fn_{}", name)
                }
            }
            ExprKind::Member { obj, name: method } => {
                // D75: built-in methods on NovaCancelToken*.
                {
                    let obj_ty = self.infer_expr_c_type(obj);
                    if obj_ty == "NovaCancelToken*" {
                        let obj_c = self.emit_expr(obj)?;
                        match method.as_str() {
                            "cancel" => {
                                return Ok(format!("nova_cancel_token_cancel({})", obj_c));
                            }
                            "is_cancelled" => {
                                return Ok(format!("nova_cancel_token_is_cancelled({})", obj_c));
                            }
                            "bind" => {
                                if let Some(parent_arg) = args.first() {
                                    let parent_c = self.emit_expr(parent_arg)?;
                                    return Ok(format!(
                                        "(nova_cancel_token_bind({}, {}), NOVA_UNIT)",
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
                                    let v = self.emit_expr(arg)?;
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
                            _ => {}
                        }
                    }
                    if obj_ty == "Nova_Result*" {
                        let obj_c = self.emit_expr(obj)?;
                        match method.as_str() {
                            "is_ok" => return Ok(format!("Nova_Result_method_is_ok({})", obj_c)),
                            "is_err" => return Ok(format!("Nova_Result_method_is_err({})", obj_c)),
                            "ok" => return Ok(format!("Nova_Result_method_ok({})", obj_c)),
                            "unwrap_or" => {
                                if let Some(arg) = args.first() {
                                    let v = self.emit_expr(arg)?;
                                    return Ok(format!(
                                        "Nova_Result_method_unwrap_or({}, {})", obj_c, v));
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
                    // Q-buffer: built-in methods on Nova_Buffer*.
                    if obj_ty == "Nova_Buffer*" {
                        let obj_c = self.emit_expr(obj)?;
                        match method.as_str() {
                            "len" => return Ok(format!("Nova_Buffer_method_len({})", obj_c)),
                            "capacity" => return Ok(format!("Nova_Buffer_method_capacity({})", obj_c)),
                            "clone" => return Ok(format!("Nova_Buffer_method_clone({})", obj_c)),
                            "into" => return Ok(format!("Nova_Buffer_method_into({})", obj_c)),
                            "try_into" => return Ok(format!("Nova_Buffer_method_try_into({})", obj_c)),
                            "into_str_unchecked" =>
                                return Ok(format!("Nova_Buffer_method_into_str_unchecked({})", obj_c)),
                            "add_str" | "add_bytes" | "add_byte" | "add_char" => {
                                if let Some(arg) = args.first() {
                                    let v = self.emit_expr(arg)?;
                                    return Ok(format!(
                                        "Nova_Buffer_method_{}({}, {})",
                                        method, obj_c, v));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                // 0a. Built-in Buffer static methods (Q-buffer).
                if let ExprKind::Ident(name) = &obj.kind {
                    if name == "Buffer" {
                        match method.as_str() {
                            "new" => return Ok("Nova_Buffer_static_new()".to_string()),
                            "with_capacity" => {
                                if let Some(arg) = args.first() {
                                    let v = self.emit_expr(arg)?;
                                    return Ok(format!("Nova_Buffer_static_with_capacity({})", v));
                                }
                            }
                            "from" => {
                                // Dispatch by argument type: str → from_str, []byte → from_bytes.
                                if let Some(arg) = args.first() {
                                    let arg_ty = self.infer_expr_c_type(arg);
                                    let v = self.emit_expr(arg)?;
                                    if arg_ty == "nova_str" {
                                        return Ok(format!("Nova_Buffer_static_from_str({})", v));
                                    } else if arg_ty.starts_with("NovaArray_") {
                                        return Ok(format!("Nova_Buffer_static_from_bytes({})", v));
                                    } else {
                                        return Err(format!(
                                            "Buffer.from(...) expects str or []byte, got {}", arg_ty));
                                    }
                                }
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
                            let arg_ty = self.infer_expr_c_type(arg);
                            let arg_type = arg_ty.trim_start_matches("Nova_").trim_end_matches('*').to_string();
                            // User-defined `fn str.from(...)` wins over builtin.
                            if let Some(("str", false)) = self.method_receivers.get("from").map(|(t, b)| (t.as_str(), *b)) {
                                let v = self.emit_expr(arg)?;
                                return Ok(format!("Nova_str_static_from({})", v));
                            }
                            if let Some(into_target) = self.into_targets.get(&arg_type) {
                                if into_target == "str" {
                                    let v = self.emit_expr(arg)?;
                                    return Ok(format!("Nova_{}_method_into({})", arg_type, v));
                                }
                            }
                            let v = self.emit_expr(arg)?;
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
                            arg_strs.push(self.emit_expr(a)?);
                        }
                        return Ok(format!("Nova_{}_{}({})", eff, method, arg_strs.join(", ")));
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
                            for a in args { arg_strs.push(self.emit_expr(a)?); }
                            return Ok(format!("nova_array_get_{}({})", elem_ty, arg_strs.join(", ")));
                        }
                        "push" => {
                            let obj_c = self.emit_expr(obj)?;
                            if elem_ty == "nova_int" && args.len() == 1 {
                                let arg_ty = self.infer_expr_c_type(&args[0]);
                                let arg_c = self.emit_expr(&args[0])?;
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
                                for a in args { arg_strs.push(self.emit_expr(a)?); }
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

                // 3. String methods: `s.starts_with(...)` → `nova_str_starts_with(s, ...)`
                if obj_ty == "nova_str" {
                    if let Some(rt_fn) = Self::str_method_to_rt(method) {
                        let obj_c = self.emit_expr(obj)?;
                        let mut arg_strs = vec![obj_c];
                        for a in args { arg_strs.push(self.emit_expr(a)?); }
                        return Ok(format!("{}({})", rt_fn, arg_strs.join(", ")));
                    }
                }

                // 4. Direct handler vtable call: `switcher.flip()` where switcher: NovaVtable_X*
                //    → `switcher->flip(switcher->ctx, args)`
                if obj_ty.starts_with("NovaVtable_") && obj_ty.ends_with('*') {
                    let obj_c = self.emit_expr(obj)?;
                    let mut arg_strs = vec![format!("{obj}->ctx", obj = obj_c)];
                    for a in args { arg_strs.push(self.emit_expr(a)?); }
                    return Ok(format!("{obj}->{method}({args})",
                        obj = obj_c, method = method, args = arg_strs.join(", ")));
                }

                // 4b. If object type is void* (unknown generic stub), the method call is
                //     undefined — emit NULL to prevent calls to undeclared functions.
                if obj_ty == "void*" {
                    for a in args { let _ = self.emit_expr(a)?; }
                    return Ok("NULL".into());
                }

                // 4c. D73 v2 auto-derive: `v.into()` for type V where V has no
                //     explicit `@into`, but some target T has `T.from(v V)`.
                //     Emit `Nova_T_static_from(v)` in that case.
                if method == "into" && args.is_empty() {
                    let recv_ty = self.infer_expr_c_type(obj);
                    let recv_type = recv_ty.trim_start_matches("Nova_").trim_end_matches('*').to_string();
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

                // 5. User-defined method call: `obj.method(args)` → `Nova_T_method_name(obj, args)`
                //    or static: `TypeName.method(args)` → `Nova_T_static_name(args)`
                //    Detect by checking method_receivers map populated at module-scan time.
                if let Some((type_name, is_instance)) = self.method_receivers.get(method).cloned() {
                    let is_generic_type = self.generic_types.contains(&type_name);
                    if is_instance {
                        let obj_c = self.emit_expr(obj)?;
                        let mut arg_strs = vec![obj_c];
                        for a in args {
                            if is_generic_type {
                                // Generic receiver: box args to void*
                                let arg_ty = self.infer_expr_c_type(a);
                                let v = self.emit_expr(a)?;
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
                                arg_strs.push(self.emit_expr(a)?);
                            }
                        }
                        return Ok(format!("Nova_{}_method_{}({})", type_name, method, arg_strs.join(", ")));
                    } else {
                        // Static method: obj is the type name (Ident), not a value
                        let mut arg_strs = Vec::new();
                        for a in args { arg_strs.push(self.emit_expr(a)?); }
                        return Ok(format!("Nova_{}_static_{}({})", type_name, method, arg_strs.join(", ")));
                    }
                }
                // Fallback: generic member call (field-function or unknown)
                let accessor = if Self::is_value_type(&obj_ty) { "." } else { "->" };
                let obj_c = self.emit_expr(obj)?;
                format!("{obj}{acc}{method}", obj = obj_c, acc = accessor, method = method)
            }
            ExprKind::Path(parts) => {
                // Q-buffer: built-in Buffer static methods (Path-form).
                if parts.len() == 2 && parts[0] == "Buffer" {
                    match parts[1].as_str() {
                        "new" => return Ok("Nova_Buffer_static_new()".to_string()),
                        "with_capacity" => {
                            if let Some(arg) = args.first() {
                                let v = self.emit_expr(arg)?;
                                return Ok(format!("Nova_Buffer_static_with_capacity({})", v));
                            }
                        }
                        "from" => {
                            if let Some(arg) = args.first() {
                                let arg_ty = self.infer_expr_c_type(arg);
                                let v = self.emit_expr(arg)?;
                                if arg_ty == "nova_str" {
                                    return Ok(format!("Nova_Buffer_static_from_str({})", v));
                                } else if arg_ty.starts_with("NovaArray_") {
                                    return Ok(format!("Nova_Buffer_static_from_bytes({})", v));
                                } else {
                                    return Err(format!(
                                        "Buffer.from(...) expects str or []byte, got {}", arg_ty));
                                }
                            }
                        }
                        _ => {}
                    }
                }
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
                            let arg_ty = self.infer_expr_c_type(arg);
                            let arg_type = arg_ty.trim_start_matches("Nova_").trim_end_matches('*').to_string();
                            // User-defined `fn str.from(...)` wins over builtin.
                            // method_receivers["from"] = ("str", false) means user defined a static
                            // `from` on str — emit the user impl. Note: doesn't disambiguate
                            // multiple from-on-str overloads (Q-overloading), but works for one.
                            if let Some(("str", false)) = self.method_receivers.get("from").map(|(t, b)| (t.as_str(), *b)) {
                                let v = self.emit_expr(arg)?;
                                return Ok(format!("Nova_str_static_from({})", v));
                            }
                            // Auto-derive: V has @into() -> str?
                            if let Some(into_target) = self.into_targets.get(&arg_type) {
                                if into_target == "str" {
                                    let v = self.emit_expr(arg)?;
                                    return Ok(format!("Nova_{}_method_into({})", arg_type, v));
                                }
                            }
                            let v = self.emit_expr(arg)?;
                            return Ok(if arg_ty == "nova_str" {
                                v
                            } else {
                                format!("nova_int_to_str((nova_int)({}))", v)
                            });
                        }
                    }
                    // Could be a static method call: `Type.method(args)`
                    // Check method_receivers for the method name
                    let method_name = &parts[1];
                    if let Some((type_name, false)) = self.method_receivers.get(method_name.as_str()).cloned() {
                        // Strict match: type_name must equal parts[0].
                        if type_name == parts[0] {
                            let mut arg_strs = Vec::new();
                            for a in args { arg_strs.push(self.emit_expr(a)?); }
                            return Ok(format!("Nova_{}_static_{}({})", type_name, method_name, arg_strs.join(", ")));
                        }
                    }
                    // D73 v2 auto-derive: `T.from(v)` when no explicit T.from
                    // exists, but `fn V @into() -> T` is defined where v: V.
                    if method_name == "from" && args.len() == 1 {
                        let target = parts[0].clone();
                        let arg_ty = self.infer_expr_c_type(&args[0]);
                        let arg_type = arg_ty.trim_start_matches("Nova_").trim_end_matches('*').to_string();
                        // Check that V has @into() -> T defined.
                        if let Some(into_target) = self.into_targets.get(&arg_type) {
                            if into_target == &target {
                                let v = self.emit_expr(&args[0])?;
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

        // Option/Result_Ok constructors use nova_int storage; nested struct args must be heap-boxed.
        // Result_Err takes nova_str directly. User-defined sum types have proper typed fields.
        let is_option_or_result_ok_ctor = func_c == "nova_make_Option_Some"
            || func_c == "nova_make_Result_Ok";
        // Generic erased functions take void* for all params; nova_str args must be boxed.
        let is_generic_call = if let ExprKind::Ident(name) = &func.kind {
            self.generic_fns.contains(name.as_str())
        } else { false };
        let mut arg_strs = Vec::new();
        for a in args {
            let arg_ty = if is_option_or_result_ok_ctor || is_generic_call {
                self.infer_expr_c_type(a)
            } else { String::new() };
            let v = self.emit_expr(a)?;
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

    fn emit_println(&mut self, args: &[Expr], newline: bool) -> Result<String, String> {
        // We emit a statement-expression block that prints each arg.
        // Since emit_expr returns a C expression, we use a GNU statement-expr ({ ... value })
        // or just emit statements and return NOVA_UNIT.
        // Strategy: emit print calls as statements, capture in tmp.
        let tmp = self.fresh_tmp_named("println");
        self.line(&format!("nova_unit {};", tmp));
        self.line("{");
        self.indent += 1;
        for arg in args {
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
            ExprKind::Binary { op, .. } => match op {
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Le
                | BinOp::Gt | BinOp::Ge | BinOp::And | BinOp::Or => "nova_print_bool",
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
                let v = self.emit_expr(e)?;
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
        for stmt in &block.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &block.trailing {
            let v = self.emit_expr(trailing)?;
            Self::emit_assign_typed(self, tmp, ty, &v);
        } else {
            Self::emit_zero_assign(self, tmp, ty);
        }
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
            for stmt in &body.stmts {
                self.emit_stmt(stmt)?;
            }
            if let Some(trailing) = &body.trailing {
                let v = self.emit_expr(trailing)?;
                self.line(&format!("(void)({});", v));
            }
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
            self.var_types.insert(binding.clone(), elem_ty);
            for stmt in &body.stmts {
                self.emit_stmt(stmt)?;
            }
            if let Some(trailing) = &body.trailing {
                let v = self.emit_expr(trailing)?;
                self.line(&format!("(void)({});", v));
            }
            self.indent -= 1;
            self.line("}");
            self.line(&format!("{} = NOVA_UNIT;", result_tmp));
            return Ok(result_tmp);
        }

        Err(format!("for-in: unsupported iterator type '{}' — only Range and Array are supported", arr_ty))
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
        // First pass: find a non-unit, non-nova_int type
        'outer: for arm in arms {
            let t = match &arm.body {
                MatchArmBody::Expr(e) => self.infer_expr_c_type(e),
                MatchArmBody::Block(b) => b.trailing.as_ref()
                    .map(|e| self.infer_expr_c_type(e))
                    .unwrap_or_else(|| "nova_unit".into()),
            };
            if t != "nova_unit" && t != "nova_int" {
                result_ty = t;
                break 'outer;
            }
        }
        // Second pass: settle for nova_int if no better type found
        if result_ty == "nova_unit" {
            for arm in arms {
                let t = match &arm.body {
                    MatchArmBody::Expr(e) => self.infer_expr_c_type(e),
                    MatchArmBody::Block(b) => b.trailing.as_ref()
                        .map(|e| self.infer_expr_c_type(e))
                        .unwrap_or_else(|| "nova_unit".into()),
                };
                if t != "nova_unit" { result_ty = t; break; }
            }
        }
        // Note: we intentionally don't inherit result_ty from current_fn_return_ty here,
        // because the match may be inside a for loop or other non-return context.

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
                for stmt in &b.stmts {
                    self.emit_stmt(stmt)?;
                }
                if let Some(trailing) = &b.trailing {
                    let val_ty = self.infer_expr_c_type(trailing);
                    let v = self.emit_expr(trailing)?;
                    let assignment = self.coerce_for_assignment(&v, &val_ty, &result_ty);
                    self.line(&format!("{} = {};", result_tmp, assignment));
                }
            }
        }
        Ok(())
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
            // Check if this is a sum-type record variant (not a plain record)
            if let Some((sum_type_name, _)) = self.find_variant(&struct_name) {
                // Emit as sum-type record variant constructor: nova_make_T_Variant(field_vals...)
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
                let call = format!("nova_make_{}_{}({})", sum_type_name, struct_name, ordered_args.join(", "));
                self.line(&format!("Nova_{}* {} = {};", sum_type_name, tmp, call));
                self.var_types.insert(tmp.clone(), format!("Nova_{}*", sum_type_name));
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
                        let val = if let Some(v) = &f.value {
                            self.emit_expr(v)?
                        } else {
                            f.name.clone() // field punning
                        };
                        // Check if the field is void* in schema (generic type erasure) — need to box the value
                        let field_ty = self.record_schemas.get(&struct_name)
                            .and_then(|s| s.get(&f.name)).cloned().unwrap_or_default();
                        if field_ty == "void*" {
                            let val_ty = if let Some(v) = &f.value { self.infer_expr_c_type(v) } else { "nova_int".into() };
                            let boxed = self.box_value_as_void_ptr(&val, &val_ty);
                            self.line(&format!("{}->{} = {};", tmp, f.name, boxed));
                        } else {
                            self.line(&format!("{}->{} = {};", tmp, f.name, val));
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
            if !self.record_schemas.contains_key(&struct_name) {
                return Err(format!(
                    "anonymous record literal: expected struct '{}' not in record_schemas",
                    struct_name));
            }
            self.line(&format!("Nova_{0}* {1} = (Nova_{0}*)nova_alloc(sizeof(Nova_{0}));",
                struct_name, tmp));
            for f in fields {
                if f.is_spread { continue; }
                let val = if let Some(v) = &f.value {
                    self.emit_expr(v)?
                } else {
                    f.name.clone()
                };
                let field_ty = self.record_schemas.get(&struct_name)
                    .and_then(|s| s.get(&f.name)).cloned().unwrap_or_default();
                if field_ty == "void*" {
                    let val_ty = if let Some(v) = &f.value {
                        self.infer_expr_c_type(v)
                    } else { "nova_int".into() };
                    let boxed = self.box_value_as_void_ptr(&val, &val_ty);
                    self.line(&format!("{}->{} = {};", tmp, f.name, boxed));
                } else {
                    self.line(&format!("{}->{} = {};", tmp, f.name, val));
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
            return Err("anonymous record literal without spread not supported in codegen".into());
        }
        Ok(tmp)
    }

    // ---- tuple destructure ----

    fn emit_tuple_destructure(&mut self, pats: &[Pattern], value: &Expr) -> Result<(), String> {
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
        // For pointer types (e.g. Nova_Box*), store as nova_int in the array
        // but remember the original type for field access.
        let elem_ty = "nova_int";
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
                    // Struct value types must be heap-allocated to cast to nova_int pointer
                    let needs_heap_alloc = (ety.starts_with("_NovaTuple") || ety == "nova_str"
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

    // ---- pattern helpers ----

    fn pattern_binding(&mut self, pat: &Pattern) -> Result<String, String> {
        match pat {
            Pattern::Ident { name, .. } => Ok(name.clone()),
            Pattern::Wildcard(_) => Ok(self.fresh_tmp()),  // unique name to avoid redeclaration
            _ => Err(format!("complex pattern in let binding not yet supported: {:?}", pat)),
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
                        for (i, p) in patterns.iter().enumerate() {
                            let field = if is_opt {
                                // Inner Option value is nova_int; if sub-pattern is also Option,
                                // wrap in pointer cast so recursive call uses -> accessor
                                let raw = format!("{}.value", scr);
                                let sub_is_opt_variant = matches!(p, Pattern::Variant { path, .. } if path.last().map_or(false, |n| n == "Some" || n == "None"));
                                if sub_is_opt_variant {
                                    format!("((NovaOpt_nova_int*)({}))", raw)
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
                            let field_access = format!("{}{}{}", scr, accessor, field.name);
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
                            let field_access = format!("{}->payload.{}.{}", scr, variant_name, field.name);
                            let sub = self.pattern_cond(&Pattern::Literal(lit.clone(), Span::dummy()), &field_access)?;
                            if sub != "true" { conds.push(sub); }
                        }
                    }
                    Ok(format!("({})", conds.join(" && ")))
                } else {
                    Ok("true".into())
                }
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
                            self.find_variant(&variant_name)
                                .map(|(t, _)| t)
                                .unwrap_or_default()
                        };

                        // Check if scrutinee has a boxed inner Option type
                        let scr_inner_ty = self.option_inner_types.get(scr).cloned();
                        for (i, p) in patterns.iter().enumerate() {
                            let sub_is_opt_variant = matches!(p, Pattern::Variant { path, .. } if path.last().map_or(false, |n| n == "Some" || n == "None"));
                            let (field, field_ty, is_boxed_inner) = if is_opt {
                                let raw = format!("{}.value", scr);
                                if sub_is_opt_variant {
                                    // Inner is a boxed Option pointer; use pointer-cast form for sub-pattern
                                    (format!("((NovaOpt_nova_int*)({}))", raw), "NovaOpt_nova_int*".into(), true)
                                } else if let Some(ref inner_ty) = scr_inner_ty {
                                    // Scrutinee has a boxed struct inner type; deref to get value
                                    let deref_ty = inner_ty.trim_end_matches('*').to_string();
                                    (format!("(*({})({}))", inner_ty, raw), deref_ty, false)
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
                let type_name_from_path = type_path.as_ref().and_then(|p| p.last().cloned()).unwrap_or_default();
                let is_plain_record = self.record_schemas.contains_key(&type_name_from_path);
                let accessor = if Self::is_value_type(&scr_ty) { "." } else { "->" };
                if is_plain_record {
                    // Plain record: bind fields directly from scr->field or scr.field
                    let field_types = self.record_schemas.get(&type_name_from_path).cloned().unwrap_or_default();
                    for field in fields {
                        let ty = field_types.get(&field.name).cloned().unwrap_or_else(|| "nova_int".into());
                        let field_access = format!("{}{}{}", scr, accessor, field.name);
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
                } else {
                    // Sum-type record variant: bind fields from scr->payload.Variant.field
                    let variant_name = type_name_from_path.clone();
                    let sum_type_name = self.find_variant(&variant_name)
                        .map(|(t, _)| t)
                        .unwrap_or_else(|| {
                            scr_ty.strip_prefix("Nova_").unwrap_or(&scr_ty)
                                .trim_end_matches('*').trim().to_string()
                        });
                    for field in fields {
                        let ty = self.get_record_variant_field_type(&sum_type_name, &variant_name, &field.name)
                            .unwrap_or_else(|| "nova_int".into());
                        let field_access = format!("{}->payload.{}.{}", scr, variant_name, field.name);
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
                for a in args { Self::collect_free_idents(a, out); }
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
            ExprKind::Supervised(b) | ExprKind::Detach(b) => {
                for s in &b.stmts { Self::collect_free_idents_stmt(s, out); }
                if let Some(t) = &b.trailing { Self::collect_free_idents(t, out); }
            }
            ExprKind::CancelScope { body, .. } => {
                for s in &body.stmts { Self::collect_free_idents_stmt(s, out); }
                if let Some(t) = &body.trailing { Self::collect_free_idents(t, out); }
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

        // Determine return type
        let ret_c_ty = if let Some(ctx) = context_param_tys {
            ctx.first().and_then(|(_, ret)| if !ret.is_empty() { Some(ret.clone()) } else { None })
                .unwrap_or_else(|| "nova_int".into())
        } else {
            "nova_int".into()
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

        // Build env struct fields
        let env_fields: String = if free_vars.is_empty() {
            "int _dummy;".to_string() // avoid empty struct (UB in C)
        } else {
            free_vars.iter().map(|(n, ty)| format!("{} {};", ty, n)).collect::<Vec<_>>().join(" ")
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

        // Emit body into lambda_impls using a temp output buffer
        let old_out = std::mem::take(&mut self.out);
        let old_indent = self.indent;
        self.indent = 0;

        // Env struct declaration
        self.out.push_str(&format!("typedef struct {{ {} }} {};\n", env_fields, env_name));
        // Body function implementation
        self.line(&format!("static {} {}({}) {{", ret_c_ty, body_name, body_params_str));
        self.indent = 1;
        // Unpack env
        if !free_vars.is_empty() {
            self.line(&format!("{}* _env = ({}*)_env_ptr;", env_name, env_name));
            for (name, ty) in &free_vars {
                self.line(&format!("{} {} = _env->{};", ty, name, name));
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

        // At the call site: allocate env + NovaClos_XX struct
        let env_tmp = self.fresh_tmp();
        let clos_tmp = self.fresh_tmp();
        self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", env_name, env_tmp, env_name, env_name));
        for (name, _ty) in &free_vars {
            self.line(&format!("{}->{} = {};", env_tmp, name, name));
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
            _ => "NovaClos_ii",
        }
    }

    fn clos_fn_ty(param_tys: &[String], ret_ty: &str) -> &'static str {
        match (param_tys, ret_ty) {
            ([], r) if r == "nova_int"                                                => "nova_fn_vi",
            ([p0], r) if p0 == "nova_int" && r == "nova_int"                         => "nova_fn_ii",
            ([p0], r) if p0 == "nova_int" && r == "nova_bool"                        => "nova_fn_ib",
            ([p0, p1], r) if p0 == "nova_int" && p1 == "nova_int" && r == "nova_int" => "nova_fn_iii",
            ([p0, p1], r) if p0 == "void*"    && p1 == "nova_int" && r == "nova_int" => "nova_fn_vii",
            _ => "nova_fn_ii",
        }
    }

    fn line(&mut self, s: &str) {
        let indent = "    ".repeat(self.indent);
        let _ = writeln!(self.out, "{}{}", indent, s);
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
            "find"        => Some("nova_str_find"),
            "rfind"       => Some("nova_str_rfind"),
            "char_len"    => Some("nova_str_char_len"),
            "byte_len"    => Some("nova_str_byte_len"),
            "bytes"       => Some("nova_str_bytes"),
            "chars"       => Some("nova_str_chars"),
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

    /// Returns true for C types that are passed by value (use `.` accessor, not `->`).
    fn is_value_type(ty: &str) -> bool {
        if ty.starts_with("_NovaTuple") && !ty.ends_with('*') {
            return true;
        }
        matches!(ty,
            "nova_int" | "nova_f64" | "nova_f32" | "nova_bool" |
            "nova_str" | "nova_unit" | "nova_byte" |
            "int32_t" | "int16_t" | "int8_t" |
            "uint64_t" | "uint32_t" | "uint16_t" | "uint8_t"
        )
    }

    fn infer_expr_c_type(&self, expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::IntLit(_) => "nova_int".into(),
            ExprKind::CharLit(_) => "nova_int".into(),
            ExprKind::FloatLit(_) => "nova_f64".into(),
            ExprKind::BoolLit(_) => "nova_bool".into(),
            ExprKind::StrLit(_) => "nova_str".into(),
            ExprKind::UnitLit => "nova_unit".into(),
            ExprKind::TupleLit(elems) => format!("_NovaTuple{}", elems.len()),
            ExprKind::Binary { op, left, right } => match op {
                BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Le
                | BinOp::Gt | BinOp::Ge | BinOp::And | BinOp::Or => "nova_bool".into(),
                _ => {
                    // If either operand is f64, result is f64
                    let lt = self.infer_expr_c_type(left);
                    let rt = self.infer_expr_c_type(right);
                    if lt == "nova_f64" || rt == "nova_f64" {
                        "nova_f64".into()
                    } else {
                        lt
                    }
                }
            },
            ExprKind::RecordLit { type_name: Some(name), .. } => {
                let raw_name = name.join("_");
                let struct_name = if raw_name == "Self" {
                    self.current_receiver_type.clone().unwrap_or(raw_name)
                } else { raw_name };
                // Check if this is a sum-type record variant
                if let Some((sum_type_name, _)) = self.find_variant(&struct_name) {
                    format!("Nova_{}*", sum_type_name)
                } else if !self.record_schemas.contains_key(&struct_name) {
                    // Unknown struct (generic or undeclared) — returns void* stub
                    "void*".into()
                } else {
                    format!("Nova_{}*", struct_name)
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
                            return "NovaOpt_nova_int".into();
                        }
                        return format!("Nova_{}*", type_name);
                    }
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
            ExprKind::Call { func, .. } => {
                // Infer return type for call expressions
                if let ExprKind::Ident(name) = &func.kind {
                    if name == "println" || name == "print" || name == "assert" {
                        return "nova_unit".into();
                    }
                    // Variant constructor call: Some(x), None, etc. → return option/sum type
                    if let Some((type_name, _)) = self.find_variant(name) {
                        // Built-in Option → NovaOpt_nova_int; user sum types → Nova_T*
                        if type_name == "Option" || type_name == "NovaOpt_nova_int" {
                            return "NovaOpt_nova_int".into();
                        }
                        return format!("Nova_{}*", type_name);
                    }
                    let key = format!("fn_ret_{}", name);
                    self.var_types.get(&key).cloned().unwrap_or_else(|| "nova_int".into())
                } else if let ExprKind::Member { obj, name: method } = &func.kind {
                    let obj_ty = self.infer_expr_c_type(obj);
                    // Q-buffer: built-in Buffer methods.
                    if let ExprKind::Ident(n) = &obj.kind {
                        if n == "Buffer" {
                            return match method.as_str() {
                                "new" | "with_capacity" | "from" => "Nova_Buffer*".into(),
                                _ => "nova_int".into(),
                            };
                        }
                    }
                    if obj_ty == "Nova_Buffer*" {
                        return match method.as_str() {
                            "len" | "capacity" => "nova_int".into(),
                            "clone" => "Nova_Buffer*".into(),
                            "into" => "NovaArray_nova_int*".into(),
                            "try_into" | "into_str_unchecked" => "nova_str".into(),
                            "add_str" | "add_bytes" | "add_byte" | "add_char" => "nova_unit".into(),
                            _ => "nova_int".into(),
                        };
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
                            "unwrap_or" | "unwrap" => elem_ty,
                            _ => "nova_int".into(),
                        };
                    }
                    // D26 prelude: Nova_Result* method type inference.
                    if obj_ty == "Nova_Result*" {
                        return match method.as_str() {
                            "is_ok" | "is_err" => "nova_bool".into(),
                            "unwrap" | "unwrap_or" => "nova_int".into(),
                            "ok" => "NovaOpt_nova_int".into(),
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
                    if obj_ty == "void*" {
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
                            if let Some((_, ret_ty)) = schema.get(method.as_str()) {
                                return ret_ty.clone();
                            }
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
                    // String method calls return type inference
                    if obj_ty == "nova_str" {
                        return match method.as_str() {
                            "to_upper" | "to_lower" | "trim" | "slice" | "concat" => "nova_str".into(),
                            "starts_with" | "ends_with" | "contains" | "eq" => "nova_bool".into(),
                            "len" | "char_len" | "byte_len" => "nova_int".into(),
                            "find" | "rfind" => "NovaOpt_nova_int".into(),
                            // D26: s.bytes() → []byte; s.chars() → []char (bootstrap-eager).
                            "bytes" | "chars" => "NovaArray_nova_int*".into(),
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
                            if let Some((_, ret_ty)) = schema.get(method.as_str()) {
                                return ret_ty.clone();
                            }
                        }
                    }
                    // User-defined method: look up return type registered during forward decl
                    let ret_key = format!("fn_ret_{}", method);
                    if let Some(ret_ty) = self.var_types.get(&ret_key) {
                        return ret_ty.clone();
                    }
                    "nova_int".into()
                } else if let ExprKind::Path(parts) = &func.kind {
                    // Effect dispatch via path: `Echo.say()` → look up in effect_schemas
                    if parts.len() == 2 {
                        let eff = &parts[0];
                        let method_name = &parts[1];
                        // Q-buffer: Buffer static methods.
                        if eff == "Buffer" {
                            return match method_name.as_str() {
                                "new" | "with_capacity" | "from" => "Nova_Buffer*".into(),
                                _ => "nova_int".into(),
                            };
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
                            if let Some((_, ret_ty)) = schema.get(method_name.as_str()) {
                                return ret_ty.clone();
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
                let obj_ty = self.infer_expr_c_type(obj);
                if obj_ty == "nova_str" && name == "len" {
                    return "nova_int".into();
                }
                if obj_ty.starts_with("NovaArray_") && name == "len" {
                    return "nova_int".into();
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
                // Field type lookup from record schema
                let struct_name = obj_ty
                    .strip_prefix("Nova_")
                    .unwrap_or("")
                    .trim_end_matches('*')
                    .trim()
                    .to_string();
                if let Some(schema) = self.record_schemas.get(&struct_name) {
                    if let Some(field_ty) = schema.get(name.as_str()) {
                        return field_ty.clone();
                    }
                }
                "nova_int".into()
            }
            ExprKind::Is(_, _) => "nova_bool".into(),
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
            ExprKind::Supervised(_) => "nova_unit".into(),
            ExprKind::Detach(_) => "nova_unit".into(),
            ExprKind::CancelScope { .. } => "nova_unit".into(),
            ExprKind::TaggedTemplate { .. } => "nova_str".into(),
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
                    BinOp::BitAnd => "&", BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::Shl => "<<", BinOp::Shr => ">>",
                };
                format!("{} {} {}",
                    Self::expr_to_display(left), op_str, Self::expr_to_display(right))
            }
            ExprKind::Call { func, args, .. } => {
                let fn_name = Self::expr_to_display(func);
                let arg_strs: Vec<String> = args.iter().map(Self::expr_to_display).collect();
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
