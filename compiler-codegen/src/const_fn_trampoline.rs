//! Plan 114.4.4.3 V4.2: runtime trampoline generation для const fn first-class use.
//!
//! V3 baseline (Plan 114.4.4 Ф.2): const fn used as first-class value
//! (e.g., `ro f = double; apply(f, 5)` or `apply(double, 5)`) → friendly
//! error `E_CONST_FN_FIRST_CLASS` / `E_CONST_FN_FIRST_CLASS_RUNTIME_HOF`
//! с suggestion обернуть в lambda.
//!
//! V4.2 (this pass): автоматическая генерация runtime trampoline fn
//! `<name>__trampoline` для каждого fully-const fn, который используется
//! как first-class value. Trampoline сохраняет семантику оригинала,
//! трактуя `const` параметры как обычные runtime-параметры (всё body
//! работает за счёт того, что body fully-const fn состоит из выражений
//! над const-params, литералами и control-flow — то же самое работает
//! и в runtime). Транзитивные вызовы других const fn внутри body
//! перепиываются на `__trampoline`-имена; size_of[T]()/align_of[T]()
//! intrinsics инлайнятся литералами при генерации trampoline body.
//!
//! Pipeline placement: внутри `rewrite_const_fn_calls`, ПОСЛЕ основного
//! walker (который инлайнит const fn calls + intrinsics), ДО validation
//! и `retain` step (который дропает const fn декларации). Trampolines
//! добавляются к `module.items` под отдельным именем, поэтому переживают
//! retain (фильтрующий по оригинальным именам).
//!
//! Ограничения V1:
//! - Только fully-const fn (не mixed) eligible для trampolines.
//! - Body может содержать calls только к другим fully-const fn —
//!   они автоматически добавляются в trampoline-set транзитивно.
//! - Generic const fn unsupported (followup `[M-114.4.4-trampoline-generics]`).
//! - size_of[T]()/align_of[T]() — только primitive T (V4.0 limitation).
//! - Closure literals в body — обрабатываются Plan 114.4.4.4 (отдельно).

use std::collections::{HashMap, HashSet};

use crate::ast::{
    ArrayElem, Block, CallArg, ElseBranch, Expr, ExprKind, FnBody, FnDecl, Item, MapElem,
    MatchArmBody, Module, Stmt, Trailing, TypeRef,
};
use crate::const_fn_eval::{simple_type_name_str, type_size_or_align};
use crate::diag::Diagnostic;

const TRAMPOLINE_SUFFIX: &str = "__trampoline";

/// Plan 114.4.4 V4.5 Ф.3 + V4.6 M4: generic trampoline instantiation key.
/// Concrete types stored как Vec<TypeRef> (полные types — V4.6 supports
/// composite концретные types: Tuple, Array, FixedArray, Named-with-generics).
/// Hash/Eq использует mangled string representation (stable serialization).
#[derive(Clone, Debug)]
struct GenericInst {
    name: String,
    /// One entry per generic param из const fn's signature, в порядке
    /// declaration. V4.6: full TypeRef (V4.5 ограничивался simple Named).
    concrete: Vec<TypeRef>,
}

impl PartialEq for GenericInst {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.mangled_args() == other.mangled_args()
    }
}
impl Eq for GenericInst {}
impl std::hash::Hash for GenericInst {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.mangled_args().hash(state);
    }
}

impl GenericInst {
    fn mangled_args(&self) -> String {
        self.concrete.iter().map(mangle_type_ref).collect::<Vec<_>>().join("_")
    }
    fn mangled_name(&self) -> String {
        format!("{}{}_{}", self.name, TRAMPOLINE_SUFFIX, self.mangled_args())
    }
}

/// Plan 114.4.4 V4.6 M4: stable serialization для TypeRef как mangled C-identifier.
/// Supports composite types для generic concrete types:
/// - Named simple (`int`) → `int`
/// - Named с generics (`Option[int]`) → `Option_int`
/// - Tuple `(int, str)` → `tup2_int_str`
/// - Array `[]int` → `arr_int`
/// - FixedArray `[4]int` → `fixarr4_int`
/// - Unit `()` → `unit`
/// - Readonly `readonly T` → `ro_<mangle T>`
/// - Func type → `fn<n>_<param1>_..._ret_<ret>` (rarely used as concrete но supported).
pub fn mangle_type_ref(t: &TypeRef) -> String {
    use crate::ast::TypeRef as TR;
    match t {
        TR::Named { path, generics, .. } => {
            let base = path.join("_");
            if generics.is_empty() {
                base
            } else {
                let g_mangle: Vec<String> = generics.iter().map(mangle_type_ref).collect();
                format!("{}_{}", base, g_mangle.join("_"))
            }
        }
        TR::Tuple(elems, _) => {
            let parts: Vec<String> = elems.iter().map(mangle_type_ref).collect();
            format!("tup{}_{}", elems.len(), parts.join("_"))
        }
        TR::Array(inner, _) => format!("arr_{}", mangle_type_ref(inner)),
        TR::FixedArray(n, inner, _) => format!("fixarr{}_{}", n, mangle_type_ref(inner)),
        TR::Unit(_) => "unit".to_string(),
        TR::Readonly(inner, _) => format!("ro_{}", mangle_type_ref(inner)),
        TR::Func { params, return_type, .. } => {
            let p_mangle: Vec<String> = params.iter().map(mangle_type_ref).collect();
            let ret_mangle = return_type.as_ref()
                .map(|r| mangle_type_ref(r))
                .unwrap_or_else(|| "unit".to_string());
            format!("fn{}_{}_ret_{}", params.len(), p_mangle.join("_"), ret_mangle)
        }
        _ => "unknown".to_string(),
    }
}

/// Trampoline pass entry. Returns `(trampoline_set, generic_trampoline_set, errors)`.
///
/// - `trampoline_set` — имена non-generic fully-const fn (без суффикса)
///   для которых сгенерированы trampoline-декларации.
/// - `generic_trampoline_set` — имена generic fully-const fn для которых
///   сгенерированы concrete instantiations (V4.5 Ф.3).
///
/// Caller передаёт оба в `validate_const_fn_runtime_uses` чтобы подавить
/// first-class errors для этих имён.
pub fn generate_const_fn_trampolines(
    module: &mut Module,
    const_fn_decls: &HashMap<String, FnDecl>,
    aliases: &HashMap<String, String>,
) -> (HashSet<String>, HashSet<String>, Vec<Diagnostic>) {
    let mut errors: Vec<Diagnostic> = Vec::new();

    // Plan 114.4.4 V4.5 Ф.3: build fn signature registry для HOF type inference.
    // Map: fn name → params types. Only NON-const fns included (const fns
    // get dropped / inlined separately).
    let fn_signatures: HashMap<String, Vec<TypeRef>> = build_fn_signature_registry(module);

    // Split const_fn_decls into generic vs non-generic.
    let mut non_generic_const_fns: HashSet<String> = HashSet::new();
    let mut generic_const_fns: HashSet<String> = HashSet::new();
    for (name, fd) in const_fn_decls {
        if fd.generics.is_empty() {
            non_generic_const_fns.insert(name.clone());
        } else {
            generic_const_fns.insert(name.clone());
        }
    }
    let const_fn_names: HashSet<String> = const_fn_decls.keys().cloned().collect();

    // Step 1 — collect first-class use seeds.
    // Non-generic seeds: HashSet<String> (existing logic).
    let mut seeds: HashSet<String> = HashSet::new();
    {
        let mut ctx = CollectCtx {
            const_fns: &non_generic_const_fns,
            generic_const_fns: &generic_const_fns,
            const_fn_decls,
            fn_signatures: &fn_signatures,
            aliases,
            seeds: &mut seeds,
            generic_seeds: &mut HashSet::new(), // unused в this pass; mutate below
            errors: &mut errors,
        };
        for item in &module.items {
            ctx.visit_item(item);
        }
        for pf in &module.peer_files {
            for item in &pf.items_here {
                ctx.visit_item(item);
            }
        }
    }

    // Plan 114.4.4 V4.5 Ф.3: separately walk MUTABLY для generic seeds.
    // At each Call.args site с generic Ident, infer concrete types и
    // rewrite Ident в-place к mangled name (because multiple instantiations
    // нельзя disambiguate from name alone в later rewriter pass).
    let mut generic_seeds: HashSet<GenericInst> = HashSet::new();
    {
        let mut ctx = GenericMutateCtx {
            generic_const_fns: &generic_const_fns,
            const_fn_decls,
            fn_signatures: &fn_signatures,
            aliases,
            generic_seeds: &mut generic_seeds,
            errors: &mut errors,
        };
        for item in &mut module.items {
            ctx.visit_item_mut(item);
        }
        for pf in &mut module.peer_files {
            for item in &mut pf.items_here {
                ctx.visit_item_mut(item);
            }
        }
    }

    if seeds.is_empty() && generic_seeds.is_empty() {
        return (HashSet::new(), HashSet::new(), errors);
    }

    // Step 2 — transitive closure: nested Call targets inside trampoline
    // bodies должны тоже быть trampolined.
    let mut trampoline_set: HashSet<String> = seeds.clone();
    let mut worklist: Vec<String> = seeds.into_iter().collect();
    while let Some(name) = worklist.pop() {
        let Some(fd) = const_fn_decls.get(&name) else { continue };
        let mut nested: HashSet<String> = HashSet::new();
        match &fd.body {
            FnBody::Expr(e) => collect_call_targets_in_expr(e, &const_fn_names, aliases, &mut nested),
            FnBody::Block(b) => collect_call_targets_in_block(b, &const_fn_names, aliases, &mut nested),
            FnBody::External => {}
        }
        for n in nested {
            if !trampoline_set.contains(&n) {
                trampoline_set.insert(n.clone());
                worklist.push(n);
            }
        }
    }

    // Step 3 — generate trampoline FnDecls.
    let mut trampoline_items: Vec<Item> = Vec::new();
    let mut sorted_names: Vec<&String> = trampoline_set.iter().collect();
    sorted_names.sort(); // deterministic codegen order
    for name in sorted_names {
        let Some(orig) = const_fn_decls.get(name) else { continue };
        match generate_trampoline_decl(orig, &trampoline_set, &mut errors) {
            Some(td) => trampoline_items.push(Item::Fn(td)),
            None => { /* error pushed into `errors` */ }
        }
    }
    module.items.extend(trampoline_items);

    // Plan 114.4.4 V4.5 Ф.3: generate generic trampolines из generic_seeds.
    // Each (name, concrete_types) tuple → specialized monomorphic fn.
    let mut generic_trampoline_set: HashSet<String> = HashSet::new();
    let mut sorted_generic_seeds: Vec<&GenericInst> = generic_seeds.iter().collect();
    sorted_generic_seeds.sort_by(|a, b| {
        // Plan 114.4.4 V4.6 M4: concrete is Vec<TypeRef> (no Ord) — compare
        // via mangled string representation (stable + deterministic).
        a.name.cmp(&b.name).then(a.mangled_args().cmp(&b.mangled_args()))
    });
    let mut generic_items: Vec<Item> = Vec::new();
    for inst in &sorted_generic_seeds {
        let Some(orig) = const_fn_decls.get(&inst.name) else { continue };
        match generate_generic_trampoline_decl(orig, inst, &mut errors) {
            Some(td) => {
                generic_trampoline_set.insert(inst.name.clone());
                generic_items.push(Item::Fn(td));
            }
            None => { /* error pushed */ }
        }
    }
    module.items.extend(generic_items);

    // Step 4 — rewrite first-class Ident references к trampoline names
    // (both non-generic and generic).
    let generic_instance_map: HashMap<String, String> = generic_seeds.iter()
        .map(|inst| (inst.name.clone(), inst.mangled_name()))
        .collect();
    {
        let mut rctx = RewriteCtx {
            const_fns: &const_fn_names,
            aliases,
            trampoline_set: &trampoline_set,
            generic_instance_map: &generic_instance_map,
        };
        for item in &mut module.items {
            rctx.rewrite_item(item);
        }
        for pf in &mut module.peer_files {
            for item in &mut pf.items_here {
                rctx.rewrite_item(item);
            }
        }
    }

    (trampoline_set, generic_trampoline_set, errors)
}

/// Plan 114.4.4 V4.5 Ф.3: generate generic trampoline FnDecl. Clone orig,
/// substitute generic params в param types + return type, rename с mangled
/// suffix, demote modifiers.
fn generate_generic_trampoline_decl(
    orig: &FnDecl,
    inst: &GenericInst,
    errors: &mut Vec<Diagnostic>,
) -> Option<FnDecl> {
    if orig.generics.len() != inst.concrete.len() {
        errors.push(Diagnostic::new(
            format!(
                "[E_CONST_FN_TRAMPOLINE_GENERIC_ARITY] generic arity mismatch для `{}`: \
                 {} declared vs {} inferred (V4.5 Ф.3).",
                orig.name, orig.generics.len(), inst.concrete.len()
            ),
            orig.span,
        ));
        return None;
    }
    // Build subst map: generic name → TypeRef.
    // Plan 114.4.4 V4.6 M4: concrete теперь Vec<TypeRef> вместо Vec<String>
    // — composite types passed directly.
    let mut subst: HashMap<String, TypeRef> = HashMap::new();
    for (g, ty) in orig.generics.iter().zip(inst.concrete.iter()) {
        subst.insert(g.name.clone(), ty.clone());
    }
    let mut td = orig.clone();
    td.name = inst.mangled_name();
    td.generics.clear(); // monomorph — no more generics.
    // Substitute в param types.
    for p in &mut td.params {
        p.ty = subst_type_ref(&p.ty, &subst);
        p.is_const = false;
    }
    // Substitute в return type.
    if let Some(ret) = &td.return_type {
        td.return_type = Some(subst_type_ref(ret, &subst));
    }
    td.return_is_const = false;
    // Body: walk and substitute size_of[T]/align_of[T] intrinsics с
    // substituted T → concrete. Reuses rewrite_trampoline_body после
    // body's TypeRef substitution.
    subst_type_refs_in_body(&mut td.body, &subst);
    let trampoline_set = HashSet::new(); // generic body — assume self-contained.
    let local_errors = rewrite_trampoline_body(&mut td.body, &trampoline_set);
    if !local_errors.is_empty() {
        errors.extend(local_errors);
        return None;
    }
    Some(td)
}

/// Walk body, replace any sizeof/align_of TurboFish type_args c substituted
/// TypeRef. Other body expressions only have TypeRef in `As`/`Is`/TurboFish
/// — substitute those too.
fn subst_type_refs_in_body(body: &mut FnBody, subst: &HashMap<String, TypeRef>) {
    match body {
        FnBody::Expr(e) => subst_type_refs_in_expr(e, subst),
        FnBody::Block(b) => subst_type_refs_in_block(b, subst),
        FnBody::External => {}
    }
}

fn subst_type_refs_in_block(b: &mut Block, subst: &HashMap<String, TypeRef>) {
    for s in &mut b.stmts {
        subst_type_refs_in_stmt(s, subst);
    }
    if let Some(t) = &mut b.trailing { subst_type_refs_in_expr(t, subst); }
}

fn subst_type_refs_in_stmt(s: &mut Stmt, subst: &HashMap<String, TypeRef>) {
    match s {
        Stmt::Let(d) => subst_type_refs_in_expr(&mut d.value, subst),
        Stmt::Const(d) => subst_type_refs_in_expr(&mut d.value, subst),
        Stmt::Expr(e) => subst_type_refs_in_expr(e, subst),
        Stmt::Assign { target, value, .. } => {
            subst_type_refs_in_expr(target, subst);
            subst_type_refs_in_expr(value, subst);
        }
        Stmt::Return { value: Some(v), .. } => subst_type_refs_in_expr(v, subst),
        Stmt::Throw { value, .. } => subst_type_refs_in_expr(value, subst),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            subst_type_refs_in_expr(body, subst);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            subst_type_refs_in_expr(init, subst);
            subst_type_refs_in_block(body, subst);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            subst_type_refs_in_expr(expr, subst);
        }
        _ => {}
    }
}

fn subst_type_refs_in_expr(e: &mut Expr, subst: &HashMap<String, TypeRef>) {
    match &mut e.kind {
        ExprKind::As(inner, t) => {
            subst_type_refs_in_expr(inner, subst);
            *t = subst_type_ref(t, subst);
        }
        ExprKind::Is(inner, t) => {
            subst_type_refs_in_expr(inner, subst);
            *t = subst_type_ref(t, subst);
        }
        ExprKind::TurboFish { base, type_args } => {
            subst_type_refs_in_expr(base, subst);
            for ta in type_args { *ta = subst_type_ref(ta, subst); }
        }
        ExprKind::Unary { operand, .. } => subst_type_refs_in_expr(operand, subst),
        ExprKind::Binary { left, right, .. } => {
            subst_type_refs_in_expr(left, subst);
            subst_type_refs_in_expr(right, subst);
        }
        ExprKind::Try(x) | ExprKind::Bang(x) => subst_type_refs_in_expr(x, subst),
        ExprKind::Coalesce(a, b) => { subst_type_refs_in_expr(a, subst); subst_type_refs_in_expr(b, subst); }
        ExprKind::Member { obj, .. } => subst_type_refs_in_expr(obj, subst),
        ExprKind::Index { obj, index } => {
            subst_type_refs_in_expr(obj, subst);
            subst_type_refs_in_expr(index, subst);
        }
        ExprKind::Call { func, args, trailing } => {
            subst_type_refs_in_expr(func, subst);
            for a in args {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => subst_type_refs_in_expr(x, subst),
                    CallArg::Named { value, .. } => subst_type_refs_in_expr(value, subst),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => subst_type_refs_in_block(b, subst),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => subst_type_refs_in_expr(e, subst),
                        FnBody::Block(b) => subst_type_refs_in_block(b, subst),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => subst_type_refs_in_block(&mut tb.body, subst),
                }
            }
        }
        ExprKind::If { cond, then, else_ } => {
            subst_type_refs_in_expr(cond, subst);
            subst_type_refs_in_block(then, subst);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => subst_type_refs_in_block(b, subst),
                    ElseBranch::If(ie) => subst_type_refs_in_expr(ie, subst),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            subst_type_refs_in_expr(scrutinee, subst);
            for arm in arms {
                if let Some(g) = &mut arm.guard { subst_type_refs_in_expr(g, subst); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => subst_type_refs_in_expr(e, subst),
                    MatchArmBody::Block(b) => subst_type_refs_in_block(b, subst),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            subst_type_refs_in_expr(iter, subst);
            subst_type_refs_in_block(body, subst);
        }
        ExprKind::While { cond, body, .. } => {
            subst_type_refs_in_expr(cond, subst);
            subst_type_refs_in_block(body, subst);
        }
        ExprKind::Loop { body, .. } => subst_type_refs_in_block(body, subst),
        ExprKind::Block(b) => subst_type_refs_in_block(b, subst),
        ExprKind::TupleLit(items) => for x in items { subst_type_refs_in_expr(x, subst); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => subst_type_refs_in_expr(x, subst), }
        },
        _ => {}
    }
}

/// Resolve alias chain: ALIAS → const_fn_name. Single-level (alias on alias
/// rejected by const_fn_eval).
fn resolve_alias<'a>(name: &'a str, aliases: &'a HashMap<String, String>) -> &'a str {
    aliases.get(name).map(|s| s.as_str()).unwrap_or(name)
}

// =========================================================================
// Step 1 — Collect first-class use seeds.
// =========================================================================

struct CollectCtx<'a> {
    const_fns: &'a HashSet<String>,
    /// Plan 114.4.4 V4.5 Ф.3: generic const fn names (separate set so
    /// they don't accidentally match non-generic seeding logic).
    generic_const_fns: &'a HashSet<String>,
    /// Plan 114.4.4 V4.5 Ф.3: full const fn decl lookup для signature.
    const_fn_decls: &'a HashMap<String, FnDecl>,
    /// Plan 114.4.4 V4.5 Ф.3: non-const fn signature registry — map
    /// fn name → param types. Used для HOF type inference: when generic
    /// const fn passed to apply(f fn(int) -> int, ...), look up apply,
    /// find param 0 type = Func{params: [int], ...}, infer T=int.
    fn_signatures: &'a HashMap<String, Vec<TypeRef>>,
    aliases: &'a HashMap<String, String>,
    seeds: &'a mut HashSet<String>,
    /// Plan 114.4.4 V4.5 Ф.3: generic instantiations.
    generic_seeds: &'a mut HashSet<GenericInst>,
    errors: &'a mut Vec<Diagnostic>,
}

impl<'a> CollectCtx<'a> {
    fn maybe_seed(&mut self, e: &Expr) {
        if let ExprKind::Ident(n) = &e.kind {
            let resolved = resolve_alias(n, self.aliases);
            if self.const_fns.contains(resolved) {
                self.seeds.insert(resolved.to_string());
            }
        }
    }
    fn visit_item(&mut self, item: &Item) {
        match item {
            Item::Fn(fd) => {
                // Plan 114.4.4 Ф.2: fully-const fn bodies dropped — but their
                // body still может содержать first-class refs к ДРУГИМ const
                // fn. Однако:
                // - Если этот fn сам fully-const и dropped, его body вообще
                //   не emitted — first-class refs внутри не имеют runtime
                //   effect. Skip.
                let all_const = !fd.params.is_empty() && fd.params.iter().all(|p| p.is_const);
                let is_fully = (all_const || fd.params.is_empty()) && fd.return_is_const;
                if is_fully {
                    return;
                }
                for p in &fd.params {
                    if let Some(def) = &p.default {
                        self.visit_expr(def);
                    }
                }
                match &fd.body {
                    FnBody::Expr(e) => self.visit_expr(e),
                    FnBody::Block(b) => self.visit_block(b),
                    FnBody::External => {}
                }
            }
            Item::Const(c) => {
                // `const ALIAS = const_fn` — это alias use, не runtime
                // first-class. Skip — handled через alias map.
                if let ExprKind::Ident(n) = &c.value.kind {
                    let r = resolve_alias(n, self.aliases);
                    if self.const_fns.contains(r) {
                        return;
                    }
                }
                self.visit_expr(&c.value);
            }
            Item::Let(l) => {
                self.maybe_seed(&l.value);
                self.visit_expr(&l.value);
            }
            Item::Test(t) => self.visit_block(&t.body),
            Item::Bench(b) => {
                for s in &b.setup { self.visit_stmt(s); }
                self.visit_block(&b.measure_body);
                for s in &b.teardown { self.visit_stmt(s); }
            }
            Item::Type(_) | Item::Lemma(_) => {}
        }
    }
    fn visit_block(&mut self, b: &Block) {
        for s in &b.stmts { self.visit_stmt(s); }
        if let Some(t) = &b.trailing { self.visit_expr(t); }
    }
    fn visit_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(d) => {
                self.maybe_seed(&d.value);
                self.visit_expr(&d.value);
            }
            Stmt::Const(_d) => {
                // local `const ALIAS = const_fn` aliasing — like top-level.
                // Conservative: visit only non-alias values.
            }
            Stmt::Expr(e) => self.visit_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.maybe_seed(value);
                self.visit_expr(target);
                self.visit_expr(value);
            }
            Stmt::Return { value: Some(v), .. } => {
                self.maybe_seed(v);
                self.visit_expr(v);
            }
            Stmt::Return { value: None, .. } => {}
            Stmt::Throw { value, .. } => self.visit_expr(value),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.visit_expr(body);
            }
            Stmt::ConsumeScope { init, body, .. } => {
                self.visit_expr(init);
                self.visit_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.visit_expr(expr);
            }
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
        }
    }
    fn visit_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Call { func, args, trailing } => {
                // `func` — это callee, НЕ first-class use (call site).
                // Однако `func` сам может быть Ident("apply") где args содержат
                // const fn — это HOF use.
                // Не делаем `maybe_seed(func)` — это callee.
                // Recurse into func только если это сложное выражение.
                if !matches!(&func.kind, ExprKind::Ident(_)) {
                    self.visit_expr(func);
                }
                // Plan 114.4.4 V4.5 Ф.3: HOF context type inference для
                // generic const fns в Call.args. Look up callee's fn signature,
                // get expected param type для position i, infer generic.
                let callee_name: Option<String> = match &func.kind {
                    ExprKind::Ident(n) => Some(n.clone()),
                    _ => None,
                };
                let expected_types: Option<&Vec<TypeRef>> = callee_name
                    .as_ref()
                    .and_then(|n| self.fn_signatures.get(n));
                for (idx, a) in args.iter().enumerate() {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => {
                            // Generic first-class detection: arg is Ident matching
                            // generic const fn name AND expected type is Func.
                            if let Some(et) = expected_types {
                                if let Some(expected) = et.get(idx) {
                                    self.try_seed_generic(x, expected);
                                }
                            }
                            self.maybe_seed(x);
                            self.visit_expr(x);
                        }
                        CallArg::Named { value, .. } => {
                            self.maybe_seed(value);
                            self.visit_expr(value);
                        }
                    }
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.visit_block(b),
                        Trailing::Fn(sb) => self.visit_fn_body(&sb.body),
                        Trailing::LegacyBlockWithParams(tb) => self.visit_block(&tb.body),
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.visit_expr(operand),
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr(left); self.visit_expr(right);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.visit_expr(x),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.visit_expr(x),
            ExprKind::Coalesce(a, b) => { self.visit_expr(a); self.visit_expr(b); }
            ExprKind::Member { obj, .. } => self.visit_expr(obj),
            ExprKind::Index { obj, index } => { self.visit_expr(obj); self.visit_expr(index); }
            ExprKind::TurboFish { base, .. } => self.visit_expr(base),
            ExprKind::TupleLit(items) => for x in items { self.visit_expr(x); },
            ExprKind::ArrayLit(elems) => for el in elems {
                match el {
                    ArrayElem::Item(x) | ArrayElem::Spread(x) => self.visit_expr(x),
                }
            },
            ExprKind::MapLit { elems, .. } => for el in elems {
                match el {
                    MapElem::Pair(k, v) => { self.visit_expr(k); self.visit_expr(v); }
                    MapElem::Spread(s) => self.visit_expr(s),
                }
            },
            ExprKind::RecordLit { fields, .. } => for f in fields {
                if let Some(v) = &f.value { self.visit_expr(v); }
            },
            ExprKind::InterpolatedStr { parts } => for p in parts {
                if let crate::ast::InterpStrPart::Expr(ex) = p { self.visit_expr(ex); }
            },
            ExprKind::If { cond, then, else_ } => {
                self.visit_expr(cond);
                self.visit_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.visit_block(b),
                        ElseBranch::If(ie) => self.visit_expr(ie),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.visit_expr(scrutinee);
                self.visit_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.visit_block(b),
                        ElseBranch::If(ie) => self.visit_expr(ie),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.visit_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.visit_expr(g); }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.visit_expr(e),
                        MatchArmBody::Block(b) => self.visit_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } => { self.visit_expr(iter); self.visit_block(body); }
            ExprKind::ParallelFor { iter, body, .. } => { self.visit_expr(iter); self.visit_block(body); }
            ExprKind::While { cond, body, .. } => { self.visit_expr(cond); self.visit_block(body); }
            ExprKind::WhileLet { scrutinee, body, .. } => { self.visit_expr(scrutinee); self.visit_block(body); }
            ExprKind::Loop { body, .. } => self.visit_block(body),
            ExprKind::Block(b) => self.visit_block(b),
            ExprKind::Lambda { body, .. } => self.visit_expr(body),
            ExprKind::ClosureLight { body, .. } => match body {
                crate::ast::ClosureBody::Expr(e) => self.visit_expr(e),
                crate::ast::ClosureBody::Block(b) => self.visit_block(b),
            },
            ExprKind::ClosureFull(sb) => self.visit_fn_body(&sb.body),
            ExprKind::Spawn(b) => self.visit_expr(b),
            ExprKind::Supervised { body, .. } => self.visit_block(body),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.visit_block(b),
            ExprKind::With { body, .. } => self.visit_block(body),
            ExprKind::Forbid { body, .. } => self.visit_block(body),
            ExprKind::Realtime { body, .. } => self.visit_block(body),
            ExprKind::Interrupt(opt) => { if let Some(x) = opt { self.visit_expr(x); } }
            _ => {}
        }
    }
    fn visit_fn_body(&mut self, body: &FnBody) {
        match body {
            FnBody::Expr(e) => self.visit_expr(e),
            FnBody::Block(b) => self.visit_block(b),
            FnBody::External => {}
        }
    }
    /// Plan 114.4.4 V4.5 Ф.3: try to seed a generic instantiation when
    /// `arg` is bare Ident matching a generic const fn name AND
    /// `expected_type` is a Func TypeRef. Inference matches const fn's
    /// signature к expected Func and derives concrete types для each
    /// generic param.
    fn try_seed_generic(&mut self, arg: &Expr, expected_type: &TypeRef) {
        let ExprKind::Ident(name) = &arg.kind else { return; };
        let resolved = resolve_alias(name, self.aliases);
        if !self.generic_const_fns.contains(resolved) { return; }
        let Some(fd) = self.const_fn_decls.get(resolved) else { return; };
        let expected = expected_type.strip_readonly();
        let TypeRef::Func { params: expected_params, return_type: expected_ret, .. } = expected
        else {
            // Not Func type — can't infer. Will be flagged via validate.
            return;
        };
        let span = arg.span;
        match infer_generic_subst(fd, expected_params, expected_ret.as_deref()) {
            Ok(subst) => {
                // Plan 114.4.4 V4.6 M4: concrete теперь Vec<TypeRef> — composite
                // types accepted (Tuple/Array/FixedArray/Named-with-generics/Unit/
                // Readonly). Validation via mangle_type_ref ниже.
                let mut concrete: Vec<TypeRef> = Vec::with_capacity(fd.generics.len());
                for g in &fd.generics {
                    let Some(ty) = subst.get(&g.name) else {
                        self.errors.push(Diagnostic::new(
                            format!(
                                "[E_CONST_FN_TRAMPOLINE_GENERIC_UNRESOLVED] generic param `{}` \
                                 of const fn `{}` could not be inferred from HOF context \
                                 (V4.5 Ф.3). Generic param должен встречаться в signature \
                                 positions which map к concrete expected type.",
                                g.name, fd.name
                            ),
                            span,
                        ));
                        return;
                    };
                    concrete.push(ty.clone());
                }
                self.generic_seeds.insert(GenericInst {
                    name: resolved.to_string(),
                    concrete,
                });
            }
            Err(reason) => {
                self.errors.push(Diagnostic::new(
                    format!(
                        "[E_CONST_FN_TRAMPOLINE_GENERIC_INFER] failed to infer generic \
                         types для const fn `{}` from HOF context (V4.5 Ф.3): {}.",
                        fd.name, reason
                    ),
                    span,
                ));
            }
        }
    }
}

// =========================================================================
// V4.5 Ф.3 — Mutating walker для generic in-place rewrite.
// =========================================================================

/// Plan 114.4.4 V4.5 Ф.3: mutable walker. At each Call.args site checks
/// if arg is bare Ident matching generic const fn, infers types from
/// callee fn signature, rewrites Ident в-place to mangled trampoline name,
/// and registers GenericInst.
struct GenericMutateCtx<'a> {
    generic_const_fns: &'a HashSet<String>,
    const_fn_decls: &'a HashMap<String, FnDecl>,
    fn_signatures: &'a HashMap<String, Vec<TypeRef>>,
    aliases: &'a HashMap<String, String>,
    generic_seeds: &'a mut HashSet<GenericInst>,
    errors: &'a mut Vec<Diagnostic>,
}

impl<'a> GenericMutateCtx<'a> {
    fn visit_item_mut(&mut self, item: &mut Item) {
        match item {
            Item::Fn(fd) => {
                let all_const = !fd.params.is_empty() && fd.params.iter().all(|p| p.is_const);
                let is_fully = (all_const || fd.params.is_empty()) && fd.return_is_const;
                if is_fully { return; }
                for p in &mut fd.params {
                    if let Some(def) = &mut p.default {
                        self.visit_expr_mut(def);
                    }
                }
                match &mut fd.body {
                    FnBody::Expr(e) => self.visit_expr_mut(e),
                    FnBody::Block(b) => self.visit_block_mut(b),
                    FnBody::External => {}
                }
            }
            Item::Const(c) => self.visit_expr_mut(&mut c.value),
            Item::Let(l) => self.visit_expr_mut(&mut l.value),
            Item::Test(t) => self.visit_block_mut(&mut t.body),
            Item::Bench(b) => {
                for s in &mut b.setup { self.visit_stmt_mut(s); }
                self.visit_block_mut(&mut b.measure_body);
                for s in &mut b.teardown { self.visit_stmt_mut(s); }
            }
            Item::Type(_) | Item::Lemma(_) => {}
        }
    }
    fn visit_block_mut(&mut self, b: &mut Block) {
        for s in &mut b.stmts { self.visit_stmt_mut(s); }
        if let Some(t) = &mut b.trailing { self.visit_expr_mut(t); }
    }
    fn visit_stmt_mut(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(d) => self.visit_expr_mut(&mut d.value),
            Stmt::Const(d) => self.visit_expr_mut(&mut d.value),
            Stmt::Expr(e) => self.visit_expr_mut(e),
            Stmt::Assign { target, value, .. } => {
                self.visit_expr_mut(target);
                self.visit_expr_mut(value);
            }
            Stmt::Return { value: Some(v), .. } => self.visit_expr_mut(v),
            Stmt::Throw { value, .. } => self.visit_expr_mut(value),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.visit_expr_mut(body);
            }
            Stmt::ConsumeScope { init, body, .. } => {
                self.visit_expr_mut(init);
                self.visit_block_mut(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.visit_expr_mut(expr);
            }
            _ => {}
        }
    }
    fn visit_expr_mut(&mut self, e: &mut Expr) {
        // Handle Call: at each arg position, check generic + rewrite.
        if let ExprKind::Call { func, args, trailing } = &mut e.kind {
            let callee_name: Option<String> = match &func.kind {
                ExprKind::Ident(n) => Some(n.clone()),
                _ => None,
            };
            let expected_types: Option<Vec<TypeRef>> = callee_name
                .as_ref()
                .and_then(|n| self.fn_signatures.get(n).cloned());
            // Recurse into func first (in case nested calls).
            if !matches!(&func.kind, ExprKind::Ident(_)) {
                self.visit_expr_mut(func);
            }
            for (idx, a) in args.iter_mut().enumerate() {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => {
                        if let Some(et) = &expected_types {
                            if let Some(expected) = et.get(idx) {
                                self.try_rewrite_generic(x, expected);
                            }
                        }
                        self.visit_expr_mut(x);
                    }
                    CallArg::Named { value, .. } => self.visit_expr_mut(value),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => self.visit_block_mut(b),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => self.visit_expr_mut(e),
                        FnBody::Block(b) => self.visit_block_mut(b),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => self.visit_block_mut(&mut tb.body),
                }
            }
            return;
        }
        // Recurse children.
        match &mut e.kind {
            ExprKind::Unary { operand, .. } => self.visit_expr_mut(operand),
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr_mut(left); self.visit_expr_mut(right);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.visit_expr_mut(x),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.visit_expr_mut(x),
            ExprKind::Coalesce(a, b) => { self.visit_expr_mut(a); self.visit_expr_mut(b); }
            ExprKind::Member { obj, .. } => self.visit_expr_mut(obj),
            ExprKind::Index { obj, index } => {
                self.visit_expr_mut(obj);
                self.visit_expr_mut(index);
            }
            ExprKind::TurboFish { base, .. } => self.visit_expr_mut(base),
            ExprKind::TupleLit(items) => for x in items { self.visit_expr_mut(x); },
            ExprKind::ArrayLit(elems) => for el in elems {
                match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => self.visit_expr_mut(x), }
            },
            ExprKind::If { cond, then, else_ } => {
                self.visit_expr_mut(cond);
                self.visit_block_mut(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.visit_block_mut(b),
                        ElseBranch::If(ie) => self.visit_expr_mut(ie),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.visit_expr_mut(scrutinee);
                for arm in arms {
                    if let Some(g) = &mut arm.guard { self.visit_expr_mut(g); }
                    match &mut arm.body {
                        MatchArmBody::Expr(e) => self.visit_expr_mut(e),
                        MatchArmBody::Block(b) => self.visit_block_mut(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } => {
                self.visit_expr_mut(iter);
                self.visit_block_mut(body);
            }
            ExprKind::While { cond, body, .. } => {
                self.visit_expr_mut(cond);
                self.visit_block_mut(body);
            }
            ExprKind::Loop { body, .. } => self.visit_block_mut(body),
            ExprKind::Block(b) => self.visit_block_mut(b),
            _ => {}
        }
    }
    fn try_rewrite_generic(&mut self, arg: &mut Expr, expected_type: &TypeRef) {
        let name = match &arg.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return,
        };
        let resolved = resolve_alias(&name, self.aliases).to_string();
        if !self.generic_const_fns.contains(&resolved) { return; }
        let Some(fd) = self.const_fn_decls.get(&resolved) else { return; };
        let expected = expected_type.strip_readonly();
        let TypeRef::Func { params: expected_params, return_type: expected_ret, .. } = expected
        else {
            return;
        };
        let span = arg.span;
        match infer_generic_subst(fd, expected_params, expected_ret.as_deref()) {
            Ok(subst) => {
                // Plan 114.4.4 V4.6 M4: composite TypeRef supported.
                let mut concrete: Vec<TypeRef> = Vec::with_capacity(fd.generics.len());
                for g in &fd.generics {
                    let Some(ty) = subst.get(&g.name) else { return; };
                    concrete.push(ty.clone());
                }
                let _ = span;
                let inst = GenericInst { name: resolved, concrete };
                let mangled = inst.mangled_name();
                self.generic_seeds.insert(inst);
                // Rewrite Ident in-place to mangled name.
                arg.kind = ExprKind::Ident(mangled);
            }
            Err(_reason) => {
                // Inference failed — leave Ident as-is; will be caught by
                // validate_const_fn_runtime_uses as first-class use error.
            }
        }
    }
}

// =========================================================================
// V4.5 Ф.3 — fn signature registry + generic inference.
// =========================================================================

/// Build map fn_name → params types для NON-const fns (regular runtime
/// fns). Used by HOF type inference. Excludes const fns (they're dropped
/// или specialized otherwise) и external fns (param types known but
/// not relevant to our use case).
fn build_fn_signature_registry(module: &Module) -> HashMap<String, Vec<TypeRef>> {
    let mut out: HashMap<String, Vec<TypeRef>> = HashMap::new();
    for item in &module.items {
        if let Item::Fn(fd) = item {
            let any_const = fd.return_is_const || fd.params.iter().any(|p| p.is_const);
            if any_const { continue; } // const fns skipped — handled separately.
            let params: Vec<TypeRef> = fd.params.iter().map(|p| p.ty.clone()).collect();
            out.insert(fd.name.clone(), params);
        }
    }
    for pf in &module.peer_files {
        for item in &pf.items_here {
            if let Item::Fn(fd) = item {
                let any_const = fd.return_is_const || fd.params.iter().any(|p| p.is_const);
                if any_const { continue; }
                let params: Vec<TypeRef> = fd.params.iter().map(|p| p.ty.clone()).collect();
                out.insert(fd.name.clone(), params);
            }
        }
    }
    out
}

/// Unify const fn's signature against expected Func{params, return}, derive
/// concrete types for generic params. Returns subst map (generic_name → concrete TypeRef)
/// or error reason на mismatch.
fn infer_generic_subst(
    const_fn: &FnDecl,
    expected_params: &[TypeRef],
    expected_ret: Option<&TypeRef>,
) -> Result<HashMap<String, TypeRef>, String> {
    let mut subst: HashMap<String, TypeRef> = HashMap::new();
    let generic_names: HashSet<String> = const_fn.generics.iter()
        .map(|g| g.name.clone())
        .collect();
    if generic_names.is_empty() {
        return Ok(subst);
    }
    // Match positional params.
    if const_fn.params.len() != expected_params.len() {
        return Err(format!(
            "param arity mismatch (const fn has {}, HOF expects {})",
            const_fn.params.len(), expected_params.len()
        ));
    }
    for (i, (p, expected)) in const_fn.params.iter().zip(expected_params.iter()).enumerate() {
        unify_type(&p.ty, expected, &generic_names, &mut subst)
            .map_err(|e| format!("param {} ({}): {}", i, p.name, e))?;
    }
    // Match return type.
    let const_ret_strip = const_fn.return_type.as_ref();
    match (const_ret_strip, expected_ret) {
        (Some(cr), Some(er)) => {
            unify_type(cr, er, &generic_names, &mut subst)
                .map_err(|e| format!("return type: {}", e))?;
        }
        (None, Some(er)) => {
            // const fn returns Unit implicitly если no return_type.
            if !matches!(er, TypeRef::Unit(_)) {
                return Err("const fn has no return type, HOF expects non-unit".to_string());
            }
        }
        (Some(_), None) => {
            return Err("const fn declares return type, HOF expects unit".to_string());
        }
        (None, None) => {}
    }
    Ok(subst)
}

/// Recursive unification. Если pattern (const_fn's TypeRef) — generic
/// param name (single Named, no generics, в generic_names set) — match с
/// concrete. Иначе recurse structurally. Mismatch — Err.
fn unify_type(
    pattern: &TypeRef,
    concrete: &TypeRef,
    generic_names: &HashSet<String>,
    subst: &mut HashMap<String, TypeRef>,
) -> Result<(), String> {
    if let TypeRef::Named { path, generics, .. } = pattern {
        if path.len() == 1 && generics.is_empty() && generic_names.contains(&path[0]) {
            let g_name = &path[0];
            if let Some(prev) = subst.get(g_name) {
                // Already bound — must match.
                if !type_refs_eq(prev, concrete) {
                    return Err(format!(
                        "generic `{}` bound to conflicting types",
                        g_name
                    ));
                }
            } else {
                subst.insert(g_name.clone(), concrete.clone());
            }
            return Ok(());
        }
    }
    // Structural recursion.
    use TypeRef as TR;
    match (pattern, concrete) {
        (TR::Named { path: p1, generics: g1, .. }, TR::Named { path: p2, generics: g2, .. }) => {
            if p1 != p2 { return Err(format!("Named path mismatch {:?} vs {:?}", p1, p2)); }
            if g1.len() != g2.len() { return Err("generics arity mismatch".to_string()); }
            for (x, y) in g1.iter().zip(g2.iter()) {
                unify_type(x, y, generic_names, subst)?;
            }
            Ok(())
        }
        (TR::Tuple(a, _), TR::Tuple(b, _)) => {
            if a.len() != b.len() { return Err("tuple arity mismatch".to_string()); }
            for (x, y) in a.iter().zip(b.iter()) {
                unify_type(x, y, generic_names, subst)?;
            }
            Ok(())
        }
        (TR::Array(a, _), TR::Array(b, _)) => unify_type(a, b, generic_names, subst),
        (TR::FixedArray(n1, e1, _), TR::FixedArray(n2, e2, _)) => {
            if n1 != n2 { return Err("array length mismatch".to_string()); }
            unify_type(e1, e2, generic_names, subst)
        }
        (TR::Func { params: p1, return_type: r1, .. }, TR::Func { params: p2, return_type: r2, .. }) => {
            if p1.len() != p2.len() { return Err("fn arity mismatch".to_string()); }
            for (x, y) in p1.iter().zip(p2.iter()) {
                unify_type(x, y, generic_names, subst)?;
            }
            match (r1, r2) {
                (Some(a), Some(b)) => unify_type(a, b, generic_names, subst),
                (None, None) => Ok(()),
                _ => Err("fn return type presence mismatch".to_string()),
            }
        }
        (TR::Unit(_), TR::Unit(_)) => Ok(()),
        (TR::Readonly(a, _), TR::Readonly(b, _)) => unify_type(a, b, generic_names, subst),
        (TR::Readonly(a, _), other) => unify_type(a, other, generic_names, subst),
        (pattern, TR::Readonly(b, _)) => unify_type(pattern, b, generic_names, subst),
        _ => Err(format!("type kind mismatch")),
    }
}

/// Structural equality на TypeRef (ignoring spans).
fn type_refs_eq(a: &TypeRef, b: &TypeRef) -> bool {
    use TypeRef as TR;
    match (a, b) {
        (TR::Named { path: p1, generics: g1, .. }, TR::Named { path: p2, generics: g2, .. }) => {
            p1 == p2 && g1.len() == g2.len() && g1.iter().zip(g2.iter()).all(|(x, y)| type_refs_eq(x, y))
        }
        (TR::Tuple(a, _), TR::Tuple(b, _)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| type_refs_eq(x, y))
        }
        (TR::Array(a, _), TR::Array(b, _)) => type_refs_eq(a, b),
        (TR::FixedArray(n1, e1, _), TR::FixedArray(n2, e2, _)) => n1 == n2 && type_refs_eq(e1, e2),
        (TR::Func { params: p1, return_type: r1, .. }, TR::Func { params: p2, return_type: r2, .. }) => {
            p1.len() == p2.len()
                && p1.iter().zip(p2.iter()).all(|(x, y)| type_refs_eq(x, y))
                && match (r1, r2) {
                    (Some(a), Some(b)) => type_refs_eq(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }
        (TR::Unit(_), TR::Unit(_)) => true,
        (TR::Readonly(a, _), TR::Readonly(b, _)) => type_refs_eq(a, b),
        _ => false,
    }
}

/// Public alias для cross-module use (const_fn_closure.rs V4.5 Ф.4).
pub fn subst_type_ref_pub(t: &TypeRef, subst: &HashMap<String, TypeRef>) -> TypeRef {
    subst_type_ref(t, subst)
}

/// Plan 114.4.4 V4.6 M3: pub re-export для const_fn_closure.rs HOF inference.
pub fn build_fn_signature_registry_pub(module: &Module) -> HashMap<String, Vec<TypeRef>> {
    build_fn_signature_registry(module)
}

/// Plan 114.4.4 V4.6 M3: pub re-export для const_fn_closure.rs.
pub fn infer_generic_subst_pub(
    const_fn: &FnDecl,
    expected_params: &[TypeRef],
    expected_ret: Option<&TypeRef>,
) -> Result<HashMap<String, TypeRef>, String> {
    infer_generic_subst(const_fn, expected_params, expected_ret)
}

/// Substitute generic param names с concrete types в TypeRef (returns new).
fn subst_type_ref(t: &TypeRef, subst: &HashMap<String, TypeRef>) -> TypeRef {
    use TypeRef as TR;
    match t {
        TR::Named { path, generics, span } => {
            if path.len() == 1 && generics.is_empty() {
                if let Some(replacement) = subst.get(&path[0]) {
                    return replacement.clone();
                }
            }
            TR::Named {
                path: path.clone(),
                generics: generics.iter().map(|g| subst_type_ref(g, subst)).collect(),
                span: *span,
            }
        }
        TR::Tuple(elems, span) => {
            TR::Tuple(elems.iter().map(|e| subst_type_ref(e, subst)).collect(), *span)
        }
        TR::Array(inner, span) => TR::Array(Box::new(subst_type_ref(inner, subst)), *span),
        TR::FixedArray(n, inner, span) => {
            TR::FixedArray(*n, Box::new(subst_type_ref(inner, subst)), *span)
        }
        TR::Func { params, effects, return_type, span } => {
            TR::Func {
                params: params.iter().map(|p| subst_type_ref(p, subst)).collect(),
                effects: effects.clone(),
                return_type: return_type.as_ref().map(|r| Box::new(subst_type_ref(r, subst))),
                span: *span,
            }
        }
        TR::Unit(span) => TR::Unit(*span),
        TR::Readonly(inner, span) => TR::Readonly(Box::new(subst_type_ref(inner, subst)), *span),
        TR::Protocol { methods, span } => TR::Protocol { methods: methods.clone(), span: *span },
        // Other TypeRef variants (e.g., Pointer от Plan 118) — clone as-is.
        // V4.5 Ф.3 substitution focuses on common generic positions.
        other => other.clone(),
    }
}

// =========================================================================
// Step 2 — Transitive Call-target collection (inside trampoline body).
// =========================================================================

fn collect_call_targets_in_expr(
    e: &Expr,
    const_fns: &HashSet<String>,
    aliases: &HashMap<String, String>,
    out: &mut HashSet<String>,
) {
    if let ExprKind::Call { func, args, trailing } = &e.kind {
        // Direct Call к const fn → транзитивный trampoline target.
        let callee = match &func.kind {
            ExprKind::Ident(n) => Some(n.as_str()),
            ExprKind::TurboFish { base, .. } => {
                if let ExprKind::Ident(n) = &base.kind { Some(n.as_str()) } else { None }
            }
            _ => None,
        };
        if let Some(n) = callee {
            let r = resolve_alias(n, aliases);
            if const_fns.contains(r) {
                out.insert(r.to_string());
            }
        }
        for a in args {
            match a {
                CallArg::Item(x) | CallArg::Spread(x) => collect_call_targets_in_expr(x, const_fns, aliases, out),
                CallArg::Named { value, .. } => collect_call_targets_in_expr(value, const_fns, aliases, out),
            }
        }
        if let Some(t) = trailing {
            match t {
                Trailing::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
                Trailing::Fn(sb) => match &sb.body {
                    FnBody::Expr(e) => collect_call_targets_in_expr(e, const_fns, aliases, out),
                    FnBody::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
                    FnBody::External => {}
                },
                Trailing::LegacyBlockWithParams(tb) => collect_call_targets_in_block(&tb.body, const_fns, aliases, out),
            }
        }
        return;
    }
    match &e.kind {
        ExprKind::Unary { operand, .. } => collect_call_targets_in_expr(operand, const_fns, aliases, out),
        ExprKind::Binary { left, right, .. } => {
            collect_call_targets_in_expr(left, const_fns, aliases, out);
            collect_call_targets_in_expr(right, const_fns, aliases, out);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => collect_call_targets_in_expr(x, const_fns, aliases, out),
        ExprKind::Try(x) | ExprKind::Bang(x) => collect_call_targets_in_expr(x, const_fns, aliases, out),
        ExprKind::Coalesce(a, b) => {
            collect_call_targets_in_expr(a, const_fns, aliases, out);
            collect_call_targets_in_expr(b, const_fns, aliases, out);
        }
        ExprKind::Member { obj, .. } => collect_call_targets_in_expr(obj, const_fns, aliases, out),
        ExprKind::Index { obj, index } => {
            collect_call_targets_in_expr(obj, const_fns, aliases, out);
            collect_call_targets_in_expr(index, const_fns, aliases, out);
        }
        ExprKind::TurboFish { base, .. } => collect_call_targets_in_expr(base, const_fns, aliases, out),
        ExprKind::TupleLit(items) => for x in items { collect_call_targets_in_expr(x, const_fns, aliases, out); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => collect_call_targets_in_expr(x, const_fns, aliases, out), }
        },
        ExprKind::RecordLit { fields, .. } => for f in fields {
            if let Some(v) = &f.value { collect_call_targets_in_expr(v, const_fns, aliases, out); }
        },
        ExprKind::If { cond, then, else_ } => {
            collect_call_targets_in_expr(cond, const_fns, aliases, out);
            collect_call_targets_in_block(then, const_fns, aliases, out);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
                    ElseBranch::If(ie) => collect_call_targets_in_expr(ie, const_fns, aliases, out),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_call_targets_in_expr(scrutinee, const_fns, aliases, out);
            for arm in arms {
                if let Some(g) = &arm.guard { collect_call_targets_in_expr(g, const_fns, aliases, out); }
                match &arm.body {
                    MatchArmBody::Expr(e) => collect_call_targets_in_expr(e, const_fns, aliases, out),
                    MatchArmBody::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            collect_call_targets_in_expr(iter, const_fns, aliases, out);
            collect_call_targets_in_block(body, const_fns, aliases, out);
        }
        ExprKind::While { cond, body, .. } => {
            collect_call_targets_in_expr(cond, const_fns, aliases, out);
            collect_call_targets_in_block(body, const_fns, aliases, out);
        }
        ExprKind::Loop { body, .. } => collect_call_targets_in_block(body, const_fns, aliases, out),
        ExprKind::Block(b) => collect_call_targets_in_block(b, const_fns, aliases, out),
        _ => {}
    }
}

fn collect_call_targets_in_block(
    b: &Block,
    const_fns: &HashSet<String>,
    aliases: &HashMap<String, String>,
    out: &mut HashSet<String>,
) {
    for s in &b.stmts {
        collect_call_targets_in_stmt(s, const_fns, aliases, out);
    }
    if let Some(t) = &b.trailing {
        collect_call_targets_in_expr(t, const_fns, aliases, out);
    }
}

fn collect_call_targets_in_stmt(
    s: &Stmt,
    const_fns: &HashSet<String>,
    aliases: &HashMap<String, String>,
    out: &mut HashSet<String>,
) {
    match s {
        Stmt::Let(d) => collect_call_targets_in_expr(&d.value, const_fns, aliases, out),
        Stmt::Const(d) => collect_call_targets_in_expr(&d.value, const_fns, aliases, out),
        Stmt::Expr(e) => collect_call_targets_in_expr(e, const_fns, aliases, out),
        Stmt::Assign { target, value, .. } => {
            collect_call_targets_in_expr(target, const_fns, aliases, out);
            collect_call_targets_in_expr(value, const_fns, aliases, out);
        }
        Stmt::Return { value: Some(v), .. } => collect_call_targets_in_expr(v, const_fns, aliases, out),
        Stmt::Throw { value, .. } => collect_call_targets_in_expr(value, const_fns, aliases, out),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            collect_call_targets_in_expr(body, const_fns, aliases, out);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            collect_call_targets_in_expr(init, const_fns, aliases, out);
            collect_call_targets_in_block(body, const_fns, aliases, out);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            collect_call_targets_in_expr(expr, const_fns, aliases, out);
        }
        _ => {}
    }
}

// =========================================================================
// Step 3 — Generate trampoline FnDecl.
// =========================================================================

fn trampoline_name(orig: &str) -> String {
    format!("{}{}", orig, TRAMPOLINE_SUFFIX)
}

fn generate_trampoline_decl(
    orig: &FnDecl,
    trampoline_set: &HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) -> Option<FnDecl> {
    // V4.5 Ф.3: generic const fns now handled by separate generic path
    // (generate_generic_trampoline_decl + HOF type inference). This path
    // skips them — they don't appear in non-generic trampoline_set.
    if !orig.generics.is_empty() {
        return None;
    }
    let _ = errors;
    let mut td = orig.clone();
    td.name = trampoline_name(&orig.name);
    // Demote: const params → runtime params; const return → runtime return.
    for p in &mut td.params {
        p.is_const = false;
    }
    td.return_is_const = false;
    // Walk body: rewrite Call к other const fn → trampoline name; substitute
    // size_of/align_of intrinsics с literal Int.
    let local_errors = rewrite_trampoline_body(&mut td.body, trampoline_set);
    if !local_errors.is_empty() {
        errors.extend(local_errors);
        return None;
    }
    Some(td)
}

fn rewrite_trampoline_body(body: &mut FnBody, trampoline_set: &HashSet<String>) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    match body {
        FnBody::Expr(e) => rewrite_body_expr(e, trampoline_set, &mut errors),
        FnBody::Block(b) => rewrite_body_block(b, trampoline_set, &mut errors),
        FnBody::External => {}
    }
    errors
}

fn rewrite_body_block(b: &mut Block, ts: &HashSet<String>, errs: &mut Vec<Diagnostic>) {
    for s in &mut b.stmts {
        rewrite_body_stmt(s, ts, errs);
    }
    if let Some(t) = &mut b.trailing {
        rewrite_body_expr(t, ts, errs);
    }
}

fn rewrite_body_stmt(s: &mut Stmt, ts: &HashSet<String>, errs: &mut Vec<Diagnostic>) {
    match s {
        Stmt::Let(d) => rewrite_body_expr(&mut d.value, ts, errs),
        Stmt::Const(d) => rewrite_body_expr(&mut d.value, ts, errs),
        Stmt::Expr(e) => rewrite_body_expr(e, ts, errs),
        Stmt::Assign { target, value, .. } => {
            rewrite_body_expr(target, ts, errs);
            rewrite_body_expr(value, ts, errs);
        }
        Stmt::Return { value: Some(v), .. } => rewrite_body_expr(v, ts, errs),
        Stmt::Throw { value, .. } => rewrite_body_expr(value, ts, errs),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            rewrite_body_expr(body, ts, errs);
        }
        Stmt::ConsumeScope { init, body, .. } => {
            rewrite_body_expr(init, ts, errs);
            rewrite_body_block(body, ts, errs);
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            rewrite_body_expr(expr, ts, errs);
        }
        _ => {}
    }
}

fn rewrite_body_expr(e: &mut Expr, ts: &HashSet<String>, errs: &mut Vec<Diagnostic>) {
    // First recurse into children.
    rewrite_body_children(e, ts, errs);
    // Handle size_of[T]() / align_of[T]() intrinsics.
    let span = e.span;
    if let ExprKind::Call { func, args, trailing: None } = &e.kind {
        if let ExprKind::TurboFish { base, type_args } = &func.kind {
            if let ExprKind::Ident(n) = &base.kind {
                if (n == "size_of" || n == "align_of") && args.is_empty() && type_args.len() == 1 {
                    match type_size_or_align(&type_args[0], n == "align_of") {
                        Some(v) => {
                            *e = Expr { kind: ExprKind::IntLit(v), span };
                            return;
                        }
                        None => {
                            errs.push(Diagnostic::new(
                                format!(
                                    "[E_CONST_FN_TRAMPOLINE_INTRINSIC] `{}[T]()` для type \
                                     {} не поддерживается в V4.2 trampoline body (only \
                                     primitives). Followup `[M-114.4.4-trampoline-record-reflection]`.",
                                    n,
                                    simple_type_name_str(&type_args[0])
                                        .unwrap_or_else(|| "<complex>".to_string())
                                ),
                                span,
                            ));
                            return;
                        }
                    }
                }
            }
        }
    }
    // Handle Call к other const fn — rewrite name к trampoline.
    if let ExprKind::Call { func, .. } = &mut e.kind {
        match &mut func.kind {
            ExprKind::Ident(n) => {
                if ts.contains(n.as_str()) {
                    *n = trampoline_name(n);
                }
            }
            ExprKind::TurboFish { base, .. } => {
                if let ExprKind::Ident(n) = &mut base.kind {
                    if ts.contains(n.as_str()) {
                        *n = trampoline_name(n);
                    }
                }
            }
            _ => {}
        }
    }
}

fn rewrite_body_children(e: &mut Expr, ts: &HashSet<String>, errs: &mut Vec<Diagnostic>) {
    match &mut e.kind {
        ExprKind::Unary { operand, .. } => rewrite_body_expr(operand, ts, errs),
        ExprKind::Binary { left, right, .. } => {
            rewrite_body_expr(left, ts, errs);
            rewrite_body_expr(right, ts, errs);
        }
        ExprKind::As(x, _) | ExprKind::Is(x, _) => rewrite_body_expr(x, ts, errs),
        ExprKind::Try(x) | ExprKind::Bang(x) => rewrite_body_expr(x, ts, errs),
        ExprKind::Coalesce(a, b) => { rewrite_body_expr(a, ts, errs); rewrite_body_expr(b, ts, errs); }
        ExprKind::Member { obj, .. } => rewrite_body_expr(obj, ts, errs),
        ExprKind::Index { obj, index } => {
            rewrite_body_expr(obj, ts, errs);
            rewrite_body_expr(index, ts, errs);
        }
        ExprKind::TurboFish { base, .. } => rewrite_body_expr(base, ts, errs),
        ExprKind::Call { func, args, trailing } => {
            rewrite_body_expr(func, ts, errs);
            for a in args {
                match a {
                    CallArg::Item(x) | CallArg::Spread(x) => rewrite_body_expr(x, ts, errs),
                    CallArg::Named { value, .. } => rewrite_body_expr(value, ts, errs),
                }
            }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => rewrite_body_block(b, ts, errs),
                    Trailing::Fn(sb) => match &mut sb.body {
                        FnBody::Expr(e) => rewrite_body_expr(e, ts, errs),
                        FnBody::Block(b) => rewrite_body_block(b, ts, errs),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => rewrite_body_block(&mut tb.body, ts, errs),
                }
            }
        }
        ExprKind::TupleLit(items) => for x in items { rewrite_body_expr(x, ts, errs); },
        ExprKind::ArrayLit(elems) => for el in elems {
            match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => rewrite_body_expr(x, ts, errs), }
        },
        ExprKind::RecordLit { fields, .. } => for f in fields {
            if let Some(v) = &mut f.value { rewrite_body_expr(v, ts, errs); }
        },
        ExprKind::If { cond, then, else_ } => {
            rewrite_body_expr(cond, ts, errs);
            rewrite_body_block(then, ts, errs);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => rewrite_body_block(b, ts, errs),
                    ElseBranch::If(ie) => rewrite_body_expr(ie, ts, errs),
                }
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            rewrite_body_expr(scrutinee, ts, errs);
            for arm in arms {
                if let Some(g) = &mut arm.guard { rewrite_body_expr(g, ts, errs); }
                match &mut arm.body {
                    MatchArmBody::Expr(e) => rewrite_body_expr(e, ts, errs),
                    MatchArmBody::Block(b) => rewrite_body_block(b, ts, errs),
                }
            }
        }
        ExprKind::For { iter, body, .. } => {
            rewrite_body_expr(iter, ts, errs);
            rewrite_body_block(body, ts, errs);
        }
        ExprKind::While { cond, body, .. } => {
            rewrite_body_expr(cond, ts, errs);
            rewrite_body_block(body, ts, errs);
        }
        ExprKind::Loop { body, .. } => rewrite_body_block(body, ts, errs),
        ExprKind::Block(b) => rewrite_body_block(b, ts, errs),
        _ => {}
    }
}

// =========================================================================
// Step 4 — Rewrite first-class Ident references к trampoline.
// =========================================================================

struct RewriteCtx<'a> {
    const_fns: &'a HashSet<String>,
    aliases: &'a HashMap<String, String>,
    trampoline_set: &'a HashSet<String>,
    /// Plan 114.4.4 V4.5 Ф.3: generic instance map — generic const fn name
    /// → mangled trampoline name (e.g. "id" → "id__trampoline_int").
    generic_instance_map: &'a HashMap<String, String>,
}

impl<'a> RewriteCtx<'a> {
    /// Replace `Ident(name)` in `e` if name (after alias resolution) is in
    /// trampoline_set OR generic_instance_map. Returns true if replaced.
    fn maybe_rewrite(&self, e: &mut Expr) -> bool {
        if let ExprKind::Ident(n) = &e.kind {
            let resolved = resolve_alias(n, self.aliases);
            if self.trampoline_set.contains(resolved) {
                let span = e.span;
                let new_name = trampoline_name(resolved);
                *e = Expr { kind: ExprKind::Ident(new_name), span };
                return true;
            }
            if let Some(mangled) = self.generic_instance_map.get(resolved) {
                let span = e.span;
                *e = Expr { kind: ExprKind::Ident(mangled.clone()), span };
                return true;
            }
        }
        false
    }
    fn rewrite_item(&mut self, item: &mut Item) {
        match item {
            Item::Fn(fd) => {
                // Skip fully-const fn bodies — they're dropped.
                let all_const = !fd.params.is_empty() && fd.params.iter().all(|p| p.is_const);
                let is_fully = (all_const || fd.params.is_empty()) && fd.return_is_const;
                if is_fully {
                    return;
                }
                // Trampoline fns themselves — also skip rewriting first-class
                // refs inside (they were already rewritten in step 3 specific
                // to call-target rewrite; let it be deterministic).
                if fd.name.ends_with(TRAMPOLINE_SUFFIX) {
                    return;
                }
                for p in &mut fd.params {
                    if let Some(def) = &mut p.default {
                        self.rewrite_expr(def);
                    }
                }
                match &mut fd.body {
                    FnBody::Expr(e) => self.rewrite_expr(e),
                    FnBody::Block(b) => self.rewrite_block(b),
                    FnBody::External => {}
                }
            }
            Item::Const(c) => {
                // Skip alias-to-const-fn const decls (will be dropped по retain).
                if let ExprKind::Ident(n) = &c.value.kind {
                    let r = resolve_alias(n, self.aliases);
                    if self.const_fns.contains(r) {
                        return;
                    }
                }
                self.rewrite_expr(&mut c.value);
            }
            Item::Let(l) => {
                if !self.maybe_rewrite(&mut l.value) {
                    self.rewrite_expr(&mut l.value);
                }
            }
            Item::Test(t) => self.rewrite_block(&mut t.body),
            Item::Bench(b) => {
                for s in &mut b.setup { self.rewrite_stmt(s); }
                self.rewrite_block(&mut b.measure_body);
                for s in &mut b.teardown { self.rewrite_stmt(s); }
            }
            Item::Type(_) | Item::Lemma(_) => {}
        }
    }
    fn rewrite_block(&mut self, b: &mut Block) {
        for s in &mut b.stmts { self.rewrite_stmt(s); }
        if let Some(t) = &mut b.trailing { self.rewrite_expr(t); }
    }
    fn rewrite_stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(d) => {
                if !self.maybe_rewrite(&mut d.value) {
                    self.rewrite_expr(&mut d.value);
                }
            }
            Stmt::Const(_d) => { /* alias decls — skipped */ }
            Stmt::Expr(e) => self.rewrite_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.rewrite_expr(target);
                if !self.maybe_rewrite(value) {
                    self.rewrite_expr(value);
                }
            }
            Stmt::Return { value: Some(v), .. } => {
                if !self.maybe_rewrite(v) {
                    self.rewrite_expr(v);
                }
            }
            Stmt::Return { value: None, .. } => {}
            Stmt::Throw { value, .. } => self.rewrite_expr(value),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.rewrite_expr(body);
            }
            Stmt::ConsumeScope { init, body, .. } => {
                self.rewrite_expr(init);
                self.rewrite_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.rewrite_expr(expr);
            }
            _ => {}
        }
    }
    fn rewrite_expr(&mut self, e: &mut Expr) {
        match &mut e.kind {
            ExprKind::Call { func, args, trailing } => {
                // func — callee, не first-class. Recurse без maybe_rewrite.
                if !matches!(&func.kind, ExprKind::Ident(_)) {
                    self.rewrite_expr(func);
                }
                for a in args {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => {
                            if !self.maybe_rewrite(x) {
                                self.rewrite_expr(x);
                            }
                        }
                        CallArg::Named { value, .. } => {
                            if !self.maybe_rewrite(value) {
                                self.rewrite_expr(value);
                            }
                        }
                    }
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.rewrite_block(b),
                        Trailing::Fn(sb) => match &mut sb.body {
                            FnBody::Expr(e) => self.rewrite_expr(e),
                            FnBody::Block(b) => self.rewrite_block(b),
                            FnBody::External => {}
                        },
                        Trailing::LegacyBlockWithParams(tb) => self.rewrite_block(&mut tb.body),
                    }
                }
            }
            ExprKind::Unary { operand, .. } => self.rewrite_expr(operand),
            ExprKind::Binary { left, right, .. } => {
                self.rewrite_expr(left); self.rewrite_expr(right);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.rewrite_expr(x),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.rewrite_expr(x),
            ExprKind::Coalesce(a, b) => { self.rewrite_expr(a); self.rewrite_expr(b); }
            ExprKind::Member { obj, .. } => self.rewrite_expr(obj),
            ExprKind::Index { obj, index } => { self.rewrite_expr(obj); self.rewrite_expr(index); }
            ExprKind::TurboFish { base, .. } => self.rewrite_expr(base),
            ExprKind::TupleLit(items) => for x in items { self.rewrite_expr(x); },
            ExprKind::ArrayLit(elems) => for el in elems {
                match el { ArrayElem::Item(x) | ArrayElem::Spread(x) => self.rewrite_expr(x), }
            },
            ExprKind::RecordLit { fields, .. } => for f in fields {
                if let Some(v) = &mut f.value { self.rewrite_expr(v); }
            },
            ExprKind::If { cond, then, else_ } => {
                self.rewrite_expr(cond);
                self.rewrite_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.rewrite_block(b),
                        ElseBranch::If(ie) => self.rewrite_expr(ie),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.rewrite_expr(scrutinee);
                for arm in arms {
                    if let Some(g) = &mut arm.guard { self.rewrite_expr(g); }
                    match &mut arm.body {
                        MatchArmBody::Expr(e) => self.rewrite_expr(e),
                        MatchArmBody::Block(b) => self.rewrite_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } => { self.rewrite_expr(iter); self.rewrite_block(body); }
            ExprKind::While { cond, body, .. } => { self.rewrite_expr(cond); self.rewrite_block(body); }
            ExprKind::Loop { body, .. } => self.rewrite_block(body),
            ExprKind::Block(b) => self.rewrite_block(b),
            ExprKind::Lambda { body, .. } => self.rewrite_expr(body),
            ExprKind::ClosureLight { body, .. } => match body {
                crate::ast::ClosureBody::Expr(e) => self.rewrite_expr(e),
                crate::ast::ClosureBody::Block(b) => self.rewrite_block(b),
            },
            ExprKind::ClosureFull(sb) => match &mut sb.body {
                FnBody::Expr(e) => self.rewrite_expr(e),
                FnBody::Block(b) => self.rewrite_block(b),
                FnBody::External => {}
            },
            ExprKind::InterpolatedStr { parts } => for p in parts {
                if let crate::ast::InterpStrPart::Expr(ex) = p { self.rewrite_expr(ex); }
            },
            ExprKind::Spawn(b) => self.rewrite_expr(b),
            ExprKind::Supervised { body, .. } => self.rewrite_block(body),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.rewrite_block(b),
            ExprKind::With { body, .. } => self.rewrite_block(body),
            ExprKind::Forbid { body, .. } => self.rewrite_block(body),
            ExprKind::Realtime { body, .. } => self.rewrite_block(body),
            ExprKind::Interrupt(opt) => { if let Some(x) = opt { self.rewrite_expr(x); } }
            _ => {}
        }
    }
}

// Re-export for tests/external use if needed.
#[allow(dead_code)]
pub(crate) fn trampoline_suffix() -> &'static str { TRAMPOLINE_SUFFIX }

#[allow(dead_code)]
fn _unused_typeref_marker(_t: &TypeRef) {}
